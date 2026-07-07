//! `luxel serve` — a native mirror of the firmware's HTTP + WebSocket
//! server, backed by the same engine core, so the browser UI can be
//! developed and end-to-end tested without hardware. Keep routes, response
//! shapes, and the ws protocol in lockstep with firmware/src/server.rs.
//!
//! Hand-rolled HTTP over std TcpStream (rather than a server crate) so the
//! /ws upgrade keeps the raw socket and can set read timeouts — that's what
//! makes the single-threaded full-duplex ws loop (push + multiplexed API
//! calls) possible, mirroring the device exactly.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use luxel_core::diag::line_col;
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::jsonview::{self, json_escape};

const INDEX_HTML: &str = include_str!("../../../firmware/src/index.html");
const DEFAULT_PATTERN: &str = include_str!("../../../examples/rainbow.js");

enum Msg {
    Code(String),
    Control(String, Vec<Fx>),
    Var(String, Fx),
    Config(u32),
    /// Start the playlist at an index (loads that item + its params).
    PlaylistPlay(usize),
    /// Stop auto-advance; the current pattern keeps running.
    PlaylistStop,
    /// Manual advance by +1 / -1 (wraps).
    PlaylistStep(i32),
    /// The playlist definition changed while playing — re-enter the current
    /// item so edits (params/source) take effect.
    PlaylistReload,
}

/// One playlist entry: a stored pattern + a snapshot of its control values, so
/// the same pattern can appear multiple times with different params.
#[derive(Clone, Default)]
struct PlaylistItem {
    pattern_id: String,
    /// name → raw 16.16 control values (matches /api/control on the wire).
    controls: Vec<(String, Vec<i32>)>,
    /// Per-item duration override in seconds. `None` = inherit the playlist
    /// default; `Some(0)` = manual (wait for next); `Some(n)` = n seconds.
    override_sec: Option<i32>,
}

#[derive(Clone, Default)]
struct Playlist {
    /// Default seconds per item; 0 = manual (no auto-advance).
    default_sec: i32,
    /// Crossfade duration between items in ms; 0 = hard cut.
    crossfade_ms: i32,
    items: Vec<PlaylistItem>,
}

/// Blend two RGB pixels by `t` in 0..=65536 (0 = a, 65536 = b).
fn blend_px(a: [u8; 3], b: [u8; 3], t: i32) -> [u8; 3] {
    let mix = |x: u8, y: u8| (((x as i32) * (65536 - t) + (y as i32) * t) >> 16) as u8;
    [mix(a[0], b[0]), mix(a[1], b[1]), mix(a[2], b[2])]
}

impl Playlist {
    /// Effective auto-advance seconds for item `i` (0 = manual).
    fn item_sec(&self, i: usize) -> i32 {
        self.items
            .get(i)
            .and_then(|it| it.override_sec)
            .unwrap_or(self.default_sec)
    }
}

/// Protocol name for a stored code (mirrors leds::Protocol; the mirror drives
/// no real LEDs, so it just round-trips the setting for the UI/e2e).
fn protocol_name(code: u8) -> &'static str {
    match code {
        1 => "ws2812",
        _ => "sk9822",
    }
}

fn protocol_code(name: &str) -> Option<u8> {
    match name.trim().to_ascii_lowercase().as_str() {
        "sk9822" | "apa102" => Some(0),
        "ws2812" | "ws2811" | "ws2815" | "ws281x" => Some(1),
        _ => None,
    }
}

/// A stored pattern (the device pattern library; firmware keeps these in
/// flash, the mirror in memory). API contract — keep in lockstep with
/// firmware/src/server.rs:
///   GET    /api/patterns              {"patterns":[{"id","name"},…]}
///   GET    /api/patterns/<id>         {"id","name","source"}
///   POST   /api/patterns              body "name\nsource" → {"ok":true,"id"}
///                                     (same name = overwrite, id stable)
///   DELETE /api/patterns/<id>         {"ok":true}
///   POST   /api/patterns/<id>/activate  runs it → {"ok":true} | code-error shape
#[derive(Clone)]
struct StoredPattern {
    id: String,
    name: String,
    source: String,
}

const MAX_PIXELS: u32 = 2048;

struct State {
    pixel_count: AtomicU32,
    inbox: Mutex<Vec<Msg>>,
    pixels: Mutex<Vec<u8>>,
    fps: AtomicU32,
    vmerr: Mutex<Option<String>>,
    pattern_src: Mutex<String>,
    controls_json: Mutex<String>,
    vars_json: Mutex<String>,
    readouts_json: Mutex<String>,
    library: Mutex<Vec<StoredPattern>>,
    next_id: AtomicU32,
    brightness: AtomicU8,
    protocol: AtomicU8,
    playlist: Mutex<Playlist>,
    pl_playing: AtomicBool,
    pl_index: AtomicUsize,
    wifi_ssid: Mutex<Option<String>>,
    /// Installed pixel map (dims, per-pixel [x,y,z] in Fx) + a re-apply flag.
    device_map: Mutex<Option<(u8, Vec<[Fx; 3]>)>>,
    map_dirty: AtomicBool,
    /// Network input (DDP/E1.31): assembled RGB frame + when it last moved.
    /// While packets flow the render loop shows this instead of the engine;
    /// LIVE_TIMEOUT after the last packet, the running pattern resumes.
    live_pixels: Mutex<Vec<u8>>,
    live_mark: Mutex<Option<Instant>>,
    live_proto: AtomicU8, // 0 = none, 1 = ddp, 2 = e131
    /// Master power (the HA light switch; mirror drives no strip, so this is
    /// state-only) — see the MQTT bridge below.
    power: AtomicBool,
    /// Library id of the running pattern ("" = ad-hoc code push).
    current_pattern_id: Mutex<String>,
    /// MQTT broker config (None = disabled) + a generation counter the bridge
    /// watches to reconnect on change, and its connection status.
    mqtt_cfg: Mutex<Option<MqttCfg>>,
    mqtt_gen: AtomicU32,
    mqtt_connected: AtomicBool,
    /// Latest sensor frame (POST /api/sensors) + seq so the render loop
    /// applies each frame once (mirrors shared::SENSOR_FRAME).
    sensor_frame: Mutex<Option<luxel_core::engine::SensorFrame>>,
    sensor_seq: AtomicU32,
    /// Luxel-to-Luxel sync: role (0 off, 1 leader, 2 follower), this
    /// process's boot id, the engine clock (published by the render loop
    /// for the leader beacon), and the last beacon heard as a follower.
    sync_mode: AtomicU8,
    sync_boot_id: u32,
    engine_time_ms: std::sync::atomic::AtomicU64,
    sync_leader: Mutex<Option<(u32, u64, Instant)>>,
}

fn sync_mode_name(m: u8) -> &'static str {
    match m {
        1 => "leader",
        2 => "follower",
        _ => "off",
    }
}

/// Leader/follower beacon loop (both roles share the thread; the mode
/// atomic steers it live). `target`/`port` come from --sync-target/-port
/// so e2e can run two mirrors over loopback.
fn sync_thread(state: Arc<State>, target: String, port: u16) {
    use luxel_core::netin::{build_sync, parse_sync};
    let send_sock = std::net::UdpSocket::bind(("0.0.0.0", 0)).ok();
    if let Some(s) = &send_sock {
        let _ = s.set_broadcast(true);
    }
    let mut recv_sock: Option<std::net::UdpSocket> = None;
    let mut sensor_sent: u32 = 0;
    let mut buf = [0u8; 128];
    loop {
        match state.sync_mode.load(Ordering::Relaxed) {
            1 => {
                recv_sock = None;
                // piggyback the sensor frame only when it moved since last
                let seq = state.sensor_seq.load(Ordering::Relaxed);
                let sb = if seq != sensor_sent {
                    sensor_sent = seq;
                    state
                        .sensor_frame
                        .lock()
                        .unwrap()
                        .as_ref()
                        .map(sensor_frame_to_sb)
                } else {
                    None
                };
                let pkt = build_sync(
                    state.sync_boot_id,
                    state.engine_time_ms.load(Ordering::Relaxed),
                    sb.as_deref(),
                );
                if let Some(s) = &send_sock {
                    let _ = s.send_to(&pkt, (target.as_str(), port));
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            2 => {
                if recv_sock.is_none() {
                    recv_sock = std::net::UdpSocket::bind(("0.0.0.0", port))
                        .inspect_err(|e| eprintln!("sync: bind :{port} failed ({e})"))
                        .ok();
                    if let Some(s) = &recv_sock {
                        let _ = s.set_read_timeout(Some(Duration::from_millis(400)));
                    }
                    if recv_sock.is_none() {
                        std::thread::sleep(Duration::from_secs(2));
                        continue;
                    }
                }
                if let Some(s) = &recv_sock {
                    if let Ok((n, _)) = s.recv_from(&mut buf) {
                        if let Some(b) = parse_sync(&buf[..n]) {
                            *state.sync_leader.lock().unwrap() =
                                Some((b.boot_id, b.time_ms, Instant::now()));
                            if let Some(sf) = b.sensor {
                                *state.sensor_frame.lock().unwrap() = Some(sf);
                                state.sensor_seq.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                }
            }
            _ => {
                recv_sock = None;
                std::thread::sleep(Duration::from_millis(300));
            }
        }
    }
}

/// Re-encode a SensorFrame as the 98-byte SB wire format (for the beacon).
fn sensor_frame_to_sb(s: &luxel_core::engine::SensorFrame) -> Vec<u8> {
    let mut p = Vec::with_capacity(luxel_core::netin::SB_FRAME_LEN);
    p.extend_from_slice(luxel_core::netin::SB_MAGIC);
    let u16le = |v: Fx| (v.raw().clamp(0, 0xFFFF) as u16).to_le_bytes();
    for v in &s.frequency_data {
        p.extend_from_slice(&u16le(*v));
    }
    p.extend_from_slice(&u16le(s.energy_average));
    p.extend_from_slice(&u16le(s.max_frequency_magnitude));
    p.extend_from_slice(&(s.max_frequency.to_int_trunc().clamp(0, 65535) as u16).to_le_bytes());
    for v in &s.accelerometer {
        p.extend_from_slice(&((v.raw().clamp(-32768, 32767) as i16).to_le_bytes()));
    }
    p.extend_from_slice(&u16le(s.light));
    for v in &s.analog_inputs {
        p.extend_from_slice(&u16le(*v));
    }
    p.extend_from_slice(b"END\0");
    p
}

/// MQTT broker settings (mirrors the firmware's config::MqttConfig).
#[derive(Clone)]
struct MqttCfg {
    host: String,
    port: u16,
    user: String,
    pass: String,
}

/// How long after the last DDP/E1.31 packet the pattern takes back over.
const LIVE_TIMEOUT: Duration = Duration::from_millis(2500);

/// The protocol currently overriding the engine, if any.
fn live_proto(state: &State) -> Option<&'static str> {
    let fresh = state
        .live_mark
        .lock()
        .unwrap()
        .is_some_and(|m| m.elapsed() < LIVE_TIMEOUT);
    match state.live_proto.load(Ordering::Relaxed) {
        1 if fresh => Some("ddp"),
        2 if fresh => Some("e131"),
        _ => None,
    }
}

/// Write `data` at byte `offset` of the live frame (grows as needed, bounded
/// by the strip) and stamp it fresh.
fn live_write(state: &State, offset: usize, data: &[u8], proto: u8) {
    let max = state.pixel_count.load(Ordering::Relaxed) as usize * 3;
    if offset >= max {
        return;
    }
    let n = data.len().min(max - offset);
    {
        let mut buf = state.live_pixels.lock().unwrap();
        if buf.len() < offset + n {
            buf.resize(offset + n, 0);
        }
        buf[offset..offset + n].copy_from_slice(&data[..n]);
    }
    *state.live_mark.lock().unwrap() = Some(Instant::now());
    state.live_proto.store(proto, Ordering::Relaxed);
}

// ---- MQTT / Home Assistant bridge (mirrors firmware/src/mqtt.rs) ----
// Topics and payloads come from luxel_core::hamqtt, so the wire contract is
// byte-identical to the device's. rumqttc's sync client runs in one thread.

/// The mirror's device id (client id, topic segment, HA unique_id).
const MQTT_ID: &str = "luxel-native";

fn mqtt_thread(state: Arc<State>) {
    loop {
        let gen = state.mqtt_gen.load(Ordering::Relaxed);
        let cfg = state.mqtt_cfg.lock().unwrap().clone();
        let Some(cfg) = cfg else {
            state.mqtt_connected.store(false, Ordering::Relaxed);
            // disabled — poll for a config (the mirror has no signal primitive)
            std::thread::sleep(Duration::from_millis(300));
            continue;
        };
        if let Err(e) = mqtt_session(&state, &cfg, gen) {
            eprintln!("mqtt: {e}");
        }
        state.mqtt_connected.store(false, Ordering::Relaxed);
        // back off before reconnecting, but react to config changes quickly
        for _ in 0..30 {
            if state.mqtt_gen.load(Ordering::Relaxed) != gen {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

/// One broker session: announce (availability + HA discovery + state), then
/// serve commands until the connection drops or the config generation moves.
fn mqtt_session(state: &Arc<State>, cfg: &MqttCfg, gen: u32) -> Result<(), String> {
    use luxel_core::hamqtt;
    use rumqttc::{Client, Event, Incoming, LastWill, MqttOptions, QoS};

    let avail = hamqtt::availability_topic(MQTT_ID);
    let mut opts = MqttOptions::new(MQTT_ID, &cfg.host, cfg.port);
    opts.set_keep_alive(Duration::from_secs(30));
    opts.set_last_will(LastWill::new(&avail, hamqtt::OFFLINE, QoS::AtMostOnce, true));
    if !cfg.user.is_empty() {
        opts.set_credentials(&cfg.user, &cfg.pass);
    }
    let (client, mut conn) = Client::new(opts, 16);
    let err = |e: rumqttc::ClientError| e.to_string();

    let light_set = hamqtt::light_set_topic(MQTT_ID);
    let pattern_set = hamqtt::pattern_set_topic(MQTT_ID);
    client.subscribe(&light_set, QoS::AtMostOnce).map_err(err)?;
    client.subscribe(&pattern_set, QoS::AtMostOnce).map_err(err)?;
    client
        .publish(&avail, QoS::AtMostOnce, true, hamqtt::ONLINE)
        .map_err(err)?;
    let version = env!("CARGO_PKG_VERSION");
    client
        .publish(
            hamqtt::light_config_topic(MQTT_ID),
            QoS::AtMostOnce,
            true,
            hamqtt::light_discovery_json(MQTT_ID, MQTT_ID, version),
        )
        .map_err(err)?;

    let options = |state: &State| -> Vec<String> {
        state.library.lock().unwrap().iter().map(|p| p.name.clone()).collect()
    };
    let mut last_options = options(state);
    client
        .publish(
            hamqtt::pattern_config_topic(MQTT_ID),
            QoS::AtMostOnce,
            true,
            hamqtt::pattern_discovery_json(MQTT_ID, MQTT_ID, version, &last_options),
        )
        .map_err(err)?;

    let mut last_light = String::new();
    let mut last_pattern: Option<String> = None;
    loop {
        // publish dirty state (initial pass runs before the first recv)
        let light = hamqtt::light_state_json(
            state.power.load(Ordering::Relaxed),
            hamqtt::brightness_to_ha(state.brightness.load(Ordering::Relaxed)),
        );
        if light != last_light {
            client
                .publish(hamqtt::light_state_topic(MQTT_ID), QoS::AtMostOnce, false, light.clone())
                .map_err(err)?;
            last_light = light;
        }
        let cur = state.current_pattern_id.lock().unwrap().clone();
        let name = pattern_by_id(state, &cur).map(|p| p.name).unwrap_or_default();
        if last_pattern.as_deref() != Some(&name) {
            client
                .publish(hamqtt::pattern_state_topic(MQTT_ID), QoS::AtMostOnce, false, name.clone())
                .map_err(err)?;
            last_pattern = Some(name);
        }
        let now = options(state);
        if now != last_options {
            last_options = now;
            client
                .publish(
                    hamqtt::pattern_config_topic(MQTT_ID),
                    QoS::AtMostOnce,
                    true,
                    hamqtt::pattern_discovery_json(MQTT_ID, MQTT_ID, version, &last_options),
                )
                .map_err(err)?;
        }

        match conn.recv_timeout(Duration::from_millis(500)) {
            Ok(Ok(Event::Incoming(Incoming::ConnAck(_)))) => {
                state.mqtt_connected.store(true, Ordering::Relaxed);
            }
            Ok(Ok(Event::Incoming(Incoming::Publish(p)))) => {
                let payload = String::from_utf8_lossy(&p.payload).into_owned();
                if p.topic == light_set {
                    let cmd = hamqtt::parse_light_command(&payload);
                    if let Some(ha) = cmd.brightness {
                        state
                            .brightness
                            .store(hamqtt::brightness_from_ha(ha), Ordering::Relaxed);
                    }
                    if let Some(on) = cmd.on {
                        state.power.store(on, Ordering::Relaxed);
                    }
                } else if p.topic == pattern_set {
                    // run a library pattern by its HA option (exact name)
                    let found = state
                        .library
                        .lock()
                        .unwrap()
                        .iter()
                        .find(|sp| sp.name == payload)
                        .map(|sp| (sp.id.clone(), sp.source.clone()));
                    match found {
                        Some((id, source)) => {
                            if api_code(state, source).contains("\"ok\":true") {
                                *state.current_pattern_id.lock().unwrap() = id;
                            }
                        }
                        None => eprintln!("mqtt: no pattern named \"{payload}\""),
                    }
                }
            }
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return Err(e.to_string()),
            Err(rumqttc::RecvTimeoutError::Timeout) => {}
            Err(rumqttc::RecvTimeoutError::Disconnected) => return Err("disconnected".into()),
        }
        if state.mqtt_gen.load(Ordering::Relaxed) != gen {
            let _ = client.publish(&avail, QoS::AtMostOnce, true, hamqtt::OFFLINE);
            let _ = client.disconnect();
            return Ok(()); // config changed — reconnect with the new one
        }
    }
}

/// UDP listener for one network-input protocol; parse is shared with the
/// firmware via luxel_core::netin.
fn netin_listener(state: Arc<State>, port: u16) {
    let sock = match std::net::UdpSocket::bind(("0.0.0.0", port)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("netin: bind :{port} failed ({e}); network input off");
            return;
        }
    };
    if port == luxel_core::netin::E131_PORT {
        // sACN defaults to multicast 239.255.<universe-hi>.<universe-lo>;
        // join enough universes for the largest strip. Unicast also works.
        let n = (MAX_PIXELS as usize * 3).div_ceil(luxel_core::netin::E131_CHANNELS);
        for u in 1..=n as u16 {
            let [hi, lo] = u.to_be_bytes();
            let _ = sock.join_multicast_v4(
                &std::net::Ipv4Addr::new(239, 255, hi, lo),
                &std::net::Ipv4Addr::UNSPECIFIED,
            );
        }
    }
    let mut buf = [0u8; 2048];
    loop {
        let Ok((len, _)) = sock.recv_from(&mut buf) else {
            continue;
        };
        let pkt = &buf[..len];
        if port == luxel_core::netin::DDP_PORT {
            if let Some(d) = luxel_core::netin::parse_ddp(pkt) {
                live_write(&state, d.offset, d.data, 1);
            }
        } else if let Some(d) = luxel_core::netin::parse_e131(pkt) {
            let off = (d.universe.max(1) as usize - 1) * luxel_core::netin::E131_CHANNELS;
            live_write(&state, off, d.data, 2);
        }
    }
}

fn push(state: &State, msg: Msg) {
    state.inbox.lock().unwrap().push(msg);
}

fn status_json(state: &State) -> String {
    let fps = state.fps.load(Ordering::Relaxed);
    let vmerr = match state.vmerr.lock().unwrap().as_deref() {
        Some(e) => format!("\"{}\"", json_escape(e)),
        None => String::from("null"),
    };
    let live = match live_proto(state) {
        Some(p) => format!("\"{p}\""),
        None => String::from("null"),
    };
    format!(
        "{{\"fps\":{},\"pixels\":{},\"slot\":\"native\",\"version\":\"{}\",\"heap_free\":0,\"live\":{},\"vmerr\":{}}}",
        fps,
        state.pixel_count.load(Ordering::Relaxed),
        env!("CARGO_PKG_VERSION"),
        live,
        vmerr
    )
}

fn controls_json(state: &State) -> String {
    let s = state.controls_json.lock().unwrap().clone();
    if s.is_empty() {
        String::from("[]")
    } else {
        s
    }
}

/// Parse a `POST /api/map` body: `<dims> <raw...>` (raw 16.16, dims per pixel).
/// None = clear.
fn parse_map(body: &str) -> Option<(u8, Vec<[Fx; 3]>)> {
    let mut it = body.split_whitespace();
    let dims: u8 = it.next()?.parse().ok()?;
    if !(2..=3).contains(&dims) {
        return None;
    }
    let vals: Vec<i32> = it.filter_map(|v| v.parse().ok()).collect();
    let n = vals.len() / dims as usize;
    if n == 0 {
        return None;
    }
    let coords: Vec<[Fx; 3]> = (0..n)
        .map(|i| {
            let mut c = [Fx::ZERO; 3];
            for d in 0..dims as usize {
                c[d] = Fx::from_raw(vals[i * dims as usize + d]);
            }
            c
        })
        .collect();
    Some((dims, coords))
}

/// Apply the installed map to an engine (no-op if none).
fn apply_map(state: &State, engine: &mut Engine) {
    if let Some((dims, coords)) = state.device_map.lock().unwrap().as_ref() {
        engine.set_map(*dims, coords);
    }
}

/// Load playlist item `i`: compile its stored pattern, apply its saved control
/// values, and publish the source/controls snapshots. Returns the engine +
/// source on success. Also records the active index.
fn enter_item(state: &State, i: usize) -> Option<(Engine, String)> {
    let item = state.playlist.lock().unwrap().items.get(i).cloned()?;
    // advance the active index even if the pattern is missing (deleted), so a
    // dangling entry just holds for its duration and the loop moves past it
    state.pl_index.store(i, Ordering::Relaxed);
    let sp = pattern_by_id(state, &item.pattern_id)?;
    *state.current_pattern_id.lock().unwrap() = item.pattern_id.clone();
    let mut eng = Engine::new(&sp.source, state.pixel_count.load(Ordering::Relaxed), 1).ok()?;
    for (name, raw) in &item.controls {
        let vals: Vec<Fx> = raw.iter().map(|&r| Fx::from_raw(r)).collect();
        eng.set_control(name, &vals);
    }
    apply_map(state, &mut eng);
    *state.pattern_src.lock().unwrap() = sp.source.clone();
    *state.controls_json.lock().unwrap() = jsonview::controls_json(&eng);
    *state.vmerr.lock().unwrap() = None;
    Some((eng, sp.source))
}

fn render_loop(state: Arc<State>) {
    let count = || state.pixel_count.load(Ordering::Relaxed);
    let mut current_src = DEFAULT_PATTERN.to_string();
    let mut engine = Engine::new(DEFAULT_PATTERN, count(), 1).ok();
    *state.pattern_src.lock().unwrap() = DEFAULT_PATTERN.to_string();
    if let Some(eng) = engine.as_ref() {
        *state.controls_json.lock().unwrap() = jsonview::controls_json(eng);
    }
    let mut last = Instant::now();
    let mut pl_start = Instant::now(); // when the current playlist item started
    // crossfade: the outgoing engine + when/how long to blend
    let mut prev: Option<Engine> = None;
    let mut blend_start = Instant::now();
    let mut blend_ms: i32 = 0;
    // start a crossfade from the current engine (call before swapping engine in)
    macro_rules! begin_crossfade {
        () => {{
            let cf = state.playlist.lock().unwrap().crossfade_ms;
            if cf > 0 && engine.is_some() {
                prev = engine.take();
                blend_start = Instant::now();
                blend_ms = cf;
            }
        }};
    }
    let mut frames: u32 = 0;
    let mut fps_mark = Instant::now();
    let mut vars_mark = Instant::now();
    let mut sensor_seen: u32 = 0;

    loop {
        for msg in state.inbox.lock().unwrap().drain(..) {
            match msg {
                Msg::Code(src) => {
                    // a manual code push takes over from the playlist
                    state.pl_playing.store(false, Ordering::Relaxed);
                    if let Ok(e) = Engine::new(&src, count(), 1) {
                        *state.controls_json.lock().unwrap() = jsonview::controls_json(&e);
                        engine = Some(e);
                        current_src = src.clone();
                        *state.pattern_src.lock().unwrap() = src;
                        *state.vmerr.lock().unwrap() = None;
                        last = Instant::now();
                        state.map_dirty.store(true, Ordering::Relaxed);
                    }
                }
                Msg::Control(name, values) => {
                    if let Some(eng) = engine.as_mut() {
                        eng.set_control(&name, &values);
                    }
                }
                Msg::Var(name, value) => {
                    if let Some(eng) = engine.as_mut() {
                        eng.set_var(&name, value);
                    }
                }
                // live pixel-count change: rebuild the engine at the new count
                Msg::Config(n) => {
                    let n = n.clamp(1, MAX_PIXELS);
                    state.pixel_count.store(n, Ordering::Relaxed);
                    if let Ok(e) = Engine::new(&current_src, n, 1) {
                        *state.controls_json.lock().unwrap() = jsonview::controls_json(&e);
                        engine = Some(e);
                        *state.vmerr.lock().unwrap() = None;
                        last = Instant::now();
                        state.map_dirty.store(true, Ordering::Relaxed);
                    }
                }
                Msg::PlaylistPlay(i) => {
                    state.pl_playing.store(true, Ordering::Relaxed);
                    if let Some((e, src)) = enter_item(&state, i) {
                        engine = Some(e);
                        current_src = src;
                        last = Instant::now();
                        pl_start = Instant::now();
                    }
                }
                Msg::PlaylistStop => state.pl_playing.store(false, Ordering::Relaxed),
                Msg::PlaylistStep(d) => {
                    let len = state.playlist.lock().unwrap().items.len();
                    if state.pl_playing.load(Ordering::Relaxed) && len > 0 {
                        let cur = state.pl_index.load(Ordering::Relaxed) as i64;
                        let ni = (cur + d as i64).rem_euclid(len as i64) as usize;
                        if let Some((e, src)) = enter_item(&state, ni) {
                            begin_crossfade!();
                            engine = Some(e);
                            current_src = src;
                            last = Instant::now();
                            pl_start = Instant::now();
                        }
                    }
                }
                Msg::PlaylistReload => {
                    if state.pl_playing.load(Ordering::Relaxed) {
                        let i = state.pl_index.load(Ordering::Relaxed);
                        if let Some((e, src)) = enter_item(&state, i) {
                            engine = Some(e);
                            current_src = src;
                            last = Instant::now();
                            pl_start = Instant::now();
                        }
                    }
                }
            }
        }

        // playlist auto-advance: effective seconds = item override ?? default;
        // 0 means manual (wait for a next/prev).
        if state.pl_playing.load(Ordering::Relaxed) {
            let (len, sec) = {
                let pl = state.playlist.lock().unwrap();
                (pl.items.len(), pl.item_sec(state.pl_index.load(Ordering::Relaxed)))
            };
            if len == 0 {
                state.pl_playing.store(false, Ordering::Relaxed);
            } else if sec > 0 && pl_start.elapsed() >= Duration::from_secs(sec as u64) {
                let ni = (state.pl_index.load(Ordering::Relaxed) + 1) % len;
                if let Some((e, src)) = enter_item(&state, ni) {
                    begin_crossfade!();
                    engine = Some(e);
                    current_src = src;
                    last = Instant::now();
                }
                pl_start = Instant::now();
            }
        }

        // apply (or clear) the installed pixel map when it changed
        if state.map_dirty.swap(false, Ordering::Relaxed) {
            if state.device_map.lock().unwrap().is_some() {
                if let Some(eng) = engine.as_mut() {
                    apply_map(&state, eng);
                }
            } else if let Ok(e) = Engine::new(&current_src, count(), 1) {
                engine = Some(e); // cleared → rebuild without a map
            }
        }

        if vars_mark.elapsed() >= Duration::from_millis(250) {
            vars_mark = Instant::now();
            if let Some(eng) = engine.as_mut() {
                *state.vars_json.lock().unwrap() = jsonview::vars_json(eng);
                *state.readouts_json.lock().unwrap() = jsonview::readouts_json(eng);
            }
        }

        // sensor data (POST /api/sensors) lands between frames
        let seq = state.sensor_seq.load(Ordering::Relaxed);
        if seq != sensor_seen {
            sensor_seen = seq;
            if let (Some(sf), Some(eng)) =
                (state.sensor_frame.lock().unwrap().clone(), engine.as_mut())
            {
                eng.set_sensors(&sf);
            }
        }

        // network input (DDP/E1.31) overrides the engine while packets flow
        if live_proto(&state).is_some() {
            let live = state.live_pixels.lock().unwrap().clone();
            let mut snap = state.pixels.lock().unwrap();
            snap.clear();
            snap.extend_from_slice(&live);
            snap.resize(count() as usize * 3, 0);
            last = Instant::now(); // keep the pattern clock fresh for resume
        } else if engine.is_some() {
            let now = Instant::now();
            let delta_us = now.duration_since(last).as_micros() as u64;
            last = now;
            let mut delta = Fx::from_raw(((delta_us << 16) / 1000) as i32);

            // follower: converge on the leader clock — big offsets jump,
            // small ones slew by stretching this frame's delta ≤ ±25%
            if state.sync_mode.load(Ordering::Relaxed) == 2 {
                if let Some((_, lt, at)) = *state.sync_leader.lock().unwrap() {
                    let eng = engine.as_mut().unwrap();
                    let target = lt + at.elapsed().as_millis() as u64;
                    let err = target as i64 - eng.time_ms() as i64;
                    if err.unsigned_abs() > 1000 {
                        eng.set_time_ms(target);
                    } else {
                        let cap = (delta.raw() as i64 / 4).max(1);
                        let adj = (err << 16).clamp(-cap, cap); // err ms → raw 16.16
                        delta = Fx::from_raw((delta.raw() as i64 + adj).clamp(0, i32::MAX as i64) as i32);
                    }
                }
            }

            // crossfade progress (0..=65536); 65536 = done
            let t = if blend_ms > 0 {
                (blend_start.elapsed().as_millis() as i64 * 65536 / blend_ms as i64).min(65536) as i32
            } else {
                65536
            };
            let px_new: Vec<[u8; 3]> = engine.as_mut().unwrap().frame(delta).to_vec();
            let out: Vec<[u8; 3]> = match prev.as_mut() {
                Some(p) if t < 65536 => {
                    let px_old = p.frame(delta);
                    px_new
                        .iter()
                        .zip(px_old.iter())
                        .map(|(n, o)| blend_px(*o, *n, t))
                        .collect()
                }
                _ => px_new,
            };
            if t >= 65536 {
                prev = None; // fade finished
            }
            state
                .engine_time_ms
                .store(engine.as_ref().unwrap().time_ms(), Ordering::Relaxed);
            {
                let mut snap = state.pixels.lock().unwrap();
                snap.clear();
                for p in &out {
                    snap.extend_from_slice(p);
                }
            }
            if let Some(e) = engine.as_mut().unwrap().take_error() {
                *state.vmerr.lock().unwrap() =
                    Some(format!("line {}:{}: {}", e.line, e.col, e.message));
            }
        }

        frames += 1;
        if fps_mark.elapsed() >= Duration::from_secs(1) {
            state.fps.store(frames, Ordering::Relaxed);
            frames = 0;
            fps_mark = Instant::now();
        }

        // pace roughly like a strip-bound device rather than spinning a core
        std::thread::sleep(Duration::from_millis(8));
    }
}

// ---- shared request handlers (same JSON as the firmware routes) ----

fn api_code(state: &State, body: String) -> String {
    match Engine::new(&body, state.pixel_count.load(Ordering::Relaxed), 1) {
        Ok(_) => {
            push(state, Msg::Code(body));
            state.current_pattern_id.lock().unwrap().clear(); // ad-hoc code
            String::from("{\"ok\":true}")
        }
        Err(d) => {
            let (line, col) = line_col(&body, d.span.start);
            format!(
                "{{\"ok\":false,\"line\":{},\"col\":{},\"error\":\"{}\"}}",
                line,
                col,
                json_escape(&d.message)
            )
        }
    }
}

fn api_control_or_var(state: &State, body: &str, is_var: bool) -> String {
    let mut it = body.split_whitespace();
    let Some(name) = it.next() else {
        return String::from("{\"ok\":false,\"error\":\"missing name\"}");
    };
    let values: Vec<Fx> = it
        .filter_map(|v| v.parse::<i32>().ok())
        .map(Fx::from_raw)
        .collect();
    if is_var {
        push(
            state,
            Msg::Var(name.to_string(), values.first().copied().unwrap_or(Fx::ZERO)),
        );
    } else {
        push(state, Msg::Control(name.to_string(), values));
    }
    String::from("{\"ok\":true}")
}

// ---- pattern library (see the StoredPattern contract above) ----

fn patterns_list_json(state: &State) -> String {
    let lib = state.library.lock().unwrap();
    let items: Vec<String> = lib
        .iter()
        .map(|p| format!("{{\"id\":\"{}\",\"name\":\"{}\"}}", p.id, json_escape(&p.name)))
        .collect();
    format!("{{\"patterns\":[{}]}}", items.join(","))
}

fn patterns_save(state: &State, body: &str) -> String {
    let (name, source) = match body.split_once('\n') {
        Some((n, s)) if !n.trim().is_empty() && !s.is_empty() => (n.trim().to_string(), s),
        _ => return String::from("{\"ok\":false,\"error\":\"expected: name\\nsource\"}"),
    };
    // compile-check before storing — the library never holds broken source
    if let Err(d) = Engine::new(source, state.pixel_count.load(Ordering::Relaxed), 1) {
        let (line, col) = line_col(source, d.span.start);
        return format!(
            "{{\"ok\":false,\"line\":{},\"col\":{},\"error\":\"{}\"}}",
            line,
            col,
            json_escape(&d.message)
        );
    }
    let mut lib = state.library.lock().unwrap();
    if let Some(p) = lib.iter_mut().find(|p| p.name == name) {
        p.source = source.to_string();
        return format!("{{\"ok\":true,\"id\":\"{}\"}}", p.id);
    }
    let id = format!("{:08x}", state.next_id.fetch_add(1, Ordering::Relaxed) ^ 0x5eed_1e55);
    lib.push(StoredPattern {
        id: id.clone(),
        name,
        source: source.to_string(),
    });
    format!("{{\"ok\":true,\"id\":\"{}\"}}", id)
}

fn pattern_by_id(state: &State, id: &str) -> Option<StoredPattern> {
    state.library.lock().unwrap().iter().find(|p| p.id == id).cloned()
}

fn patterns_delete(state: &State, id: &str) -> String {
    let mut lib = state.library.lock().unwrap();
    let before = lib.len();
    lib.retain(|p| p.id != id);
    if lib.len() < before {
        String::from("{\"ok\":true}")
    } else {
        String::from("{\"ok\":false,\"error\":\"no such pattern\"}")
    }
}

// ---- playlist (see firmware/src/server.rs for the same contract) ----

fn playlist_json(state: &State) -> String {
    let pl = state.playlist.lock().unwrap();
    let lib = state.library.lock().unwrap();
    let items: Vec<String> = pl
        .items
        .iter()
        .map(|it| {
            let name = lib
                .iter()
                .find(|p| p.id == it.pattern_id)
                .map(|p| p.name.clone())
                .unwrap_or_default();
            let sec = it.override_sec.map(|s| s.to_string()).unwrap_or_else(|| "null".into());
            let controls: Vec<String> = it
                .controls
                .iter()
                .map(|(n, raw)| {
                    let vals: Vec<String> =
                        raw.iter().map(|&r| format!("{}", Fx::from_raw(r))).collect();
                    format!("\"{}\":[{}]", json_escape(n), vals.join(","))
                })
                .collect();
            format!(
                "{{\"id\":\"{}\",\"name\":\"{}\",\"sec\":{},\"controls\":{{{}}}}}",
                it.pattern_id,
                json_escape(&name),
                sec,
                controls.join(",")
            )
        })
        .collect();
    format!(
        "{{\"defaultSec\":{},\"crossfadeMs\":{},\"playing\":{},\"index\":{},\"items\":[{}]}}",
        pl.default_sec,
        pl.crossfade_ms,
        state.pl_playing.load(Ordering::Relaxed),
        state.pl_index.load(Ordering::Relaxed),
        items.join(",")
    )
}

/// Parse the line-based playlist body (no JSON parser needed, mirrors the
/// firmware). Lines: `D <sec>` default; `I <patternId> <sec|-1>` item
/// (-1 = inherit default); `C <name> <raw...>` a control for the last item.
fn parse_playlist(body: &str) -> Playlist {
    let mut pl = Playlist::default();
    for line in body.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("D") => pl.default_sec = it.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            Some("X") => pl.crossfade_ms = it.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            Some("I") => {
                let id = it.next().unwrap_or("").to_string();
                let sec = it.next().and_then(|v| v.parse::<i32>().ok());
                let override_sec = match sec {
                    Some(n) if n < 0 => None,
                    other => other,
                };
                pl.items.push(PlaylistItem {
                    pattern_id: id,
                    controls: Vec::new(),
                    override_sec,
                });
            }
            Some("C") => {
                if let (Some(item), Some(name)) = (pl.items.last_mut(), it.next()) {
                    let raw: Vec<i32> = it.filter_map(|v| v.parse().ok()).collect();
                    item.controls.push((name.to_string(), raw));
                }
            }
            _ => {}
        }
    }
    pl
}

/// One multiplexed ws request: `"<id> <call>\n<body>"` →
/// `{"id":<id>,"r":<json>}`. Mirrors firmware handle_ws_call.
fn handle_ws_call(state: &State, frame: &str) -> String {
    let (header, body) = frame.split_once('\n').unwrap_or((frame, ""));
    let mut it = header.split_whitespace();
    let (id, call) = match (it.next().and_then(|v| v.parse::<u32>().ok()), it.next()) {
        (Some(id), Some(call)) => (id, call),
        _ => return String::from("{\"id\":0,\"r\":{\"ok\":false,\"error\":\"bad frame\"}}"),
    };
    let r = match call {
        "code" => api_code(state, String::from(body)),
        "control" => api_control_or_var(state, body, false),
        "var" => api_control_or_var(state, body, true),
        "pattern" => format!(
            "{{\"pattern\":\"{}\"}}",
            json_escape(&state.pattern_src.lock().unwrap())
        ),
        _ => String::from("{\"ok\":false,\"error\":\"unknown call\"}"),
    };
    format!("{{\"id\":{},\"r\":{}}}", id, r)
}

// ---- minimal HTTP plumbing ----

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

fn header<'a>(req: &'a Request, name: &str) -> Option<&'a str> {
    req.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

fn read_request(reader: &mut BufReader<TcpStream>) -> Option<Request> {
    let mut line = String::new();
    if reader.read_line(&mut line).ok()? == 0 {
        return None;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let mut headers = Vec::new();
    loop {
        let mut h = String::new();
        reader.read_line(&mut h).ok()?;
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let len: usize = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    let mut body = vec![0u8; len];
    if len > 0 {
        reader.read_exact(&mut body).ok()?;
    }
    Some(Request {
        method,
        path,
        headers,
        body,
    })
}

fn respond(stream: &mut TcpStream, status: u16, content_type: &str, body: &[u8]) {
    let _ = write!(
        stream,
        "HTTP/1.1 {} X\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        status,
        content_type,
        body.len()
    );
    let _ = stream.write_all(body);
}

/// CORS preflight reply: a cross-origin DELETE (and any non-simple method)
/// sends an OPTIONS first; without these headers the browser blocks the
/// real request. GET/POST with a text body are "simple" and skip this.
fn respond_preflight(stream: &mut TcpStream) {
    let _ = write!(
        stream,
        "HTTP/1.1 204 X\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, DELETE, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nAccess-Control-Max-Age: 86400\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
}

/// Full-duplex ws loop, single thread: a short read timeout doubles as the
/// push tick (mirrors the device's next_message-with-signal structure).
fn ws_session(stream: TcpStream, key: &str, state: Arc<State>) {
    let accept = tungstenite::handshake::derive_accept_key(key.as_bytes());
    let mut stream = stream;
    let _ = write!(
        stream,
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
        accept
    );
    stream
        .set_read_timeout(Some(Duration::from_millis(66)))
        .ok();
    let mut ws = tungstenite::WebSocket::from_raw_socket(
        stream,
        tungstenite::protocol::Role::Server,
        None,
    );
    let mut tick: u32 = 0;
    loop {
        // read with the 66 ms timeout; WouldBlock/TimedOut = push tick
        match ws.read() {
            Ok(tungstenite::Message::Text(t)) => {
                let reply = handle_ws_call(&state, &t);
                if ws.send(tungstenite::Message::Text(reply)).is_err() {
                    return;
                }
                continue;
            }
            Ok(tungstenite::Message::Close(_)) | Err(tungstenite::Error::ConnectionClosed) => {
                return;
            }
            Ok(_) => continue,
            Err(tungstenite::Error::Io(e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(_) => return,
        }

        let px = state.pixels.lock().unwrap().clone();
        if !px.is_empty() && ws.send(tungstenite::Message::Binary(px)).is_err() {
            return;
        }
        if tick % 4 == 0 {
            let vars = state.vars_json.lock().unwrap().clone();
            let ro = state.readouts_json.lock().unwrap().clone();
            if ws
                .send(tungstenite::Message::Text(format!(
                    "{{\"type\":\"vars\",\"vars\":{}}}",
                    vars
                )))
                .is_err()
                || ws
                    .send(tungstenite::Message::Text(format!(
                        "{{\"type\":\"readouts\",\"readouts\":{}}}",
                        ro
                    )))
                    .is_err()
            {
                return;
            }
        }
        if tick % 15 == 0 {
            if ws
                .send(tungstenite::Message::Text(format!(
                    "{{\"type\":\"status\",\"status\":{}}}",
                    status_json(&state)
                )))
                .is_err()
                || ws
                    .send(tungstenite::Message::Text(format!(
                        "{{\"type\":\"controls\",\"controls\":{}}}",
                        controls_json(&state)
                    )))
                    .is_err()
            {
                return;
            }
        }
        tick = tick.wrapping_add(1);
    }
}

fn handle_connection(stream: TcpStream, state: Arc<State>) {
    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });
    let Some(req) = read_request(&mut reader) else {
        return;
    };
    let mut stream = stream;

    match (req.method.as_str(), req.path.as_str()) {
        ("OPTIONS", _) => respond_preflight(&mut stream),
        ("GET", "/ws") => {
            if let Some(key) = header(&req, "Sec-WebSocket-Key") {
                let key = key.to_string();
                ws_session(stream, &key, state);
            } else {
                respond(&mut stream, 400, "text/plain", b"missing Sec-WebSocket-Key");
            }
        }
        ("GET", "/") => respond(
            &mut stream,
            200,
            "text/html; charset=utf-8",
            INDEX_HTML.as_bytes(),
        ),
        ("GET", "/api/status") => {
            respond(&mut stream, 200, "application/json", status_json(&state).as_bytes())
        }
        ("GET", "/api/pixels") => {
            let snap = state.pixels.lock().unwrap().clone();
            respond(&mut stream, 200, "application/octet-stream", &snap);
        }
        ("GET", "/api/pattern") => {
            let src = state.pattern_src.lock().unwrap().clone();
            respond(&mut stream, 200, "text/plain; charset=utf-8", src.as_bytes());
        }
        ("GET", "/api/controls") => {
            respond(&mut stream, 200, "application/json", controls_json(&state).as_bytes())
        }
        ("GET", "/api/vars") => {
            let s = state.vars_json.lock().unwrap().clone();
            let s = if s.is_empty() { String::from("{}") } else { s };
            respond(&mut stream, 200, "application/json", s.as_bytes());
        }
        ("GET", "/api/readouts") => {
            let s = state.readouts_json.lock().unwrap().clone();
            let s = if s.is_empty() { String::from("{}") } else { s };
            respond(&mut stream, 200, "application/json", s.as_bytes());
        }
        ("GET", "/api/brightness") => {
            let b = state.brightness.load(Ordering::Relaxed);
            let body = format!("{{\"brightness\":{},\"max\":31}}", b);
            respond(&mut stream, 200, "application/json", body.as_bytes());
        }
        ("POST", "/api/brightness") => {
            let body = String::from_utf8_lossy(&req.body);
            let r = match body.trim().parse::<u8>() {
                Ok(b) if b <= 31 => {
                    state.brightness.store(b, Ordering::Relaxed);
                    format!("{{\"ok\":true,\"brightness\":{}}}", b)
                }
                _ => String::from("{\"ok\":false,\"error\":\"brightness must be 0..=31\"}"),
            };
            respond(&mut stream, 200, "application/json", r.as_bytes());
        }
        ("GET", "/api/config") => {
            let body = format!(
                "{{\"pixels\":{},\"max\":{},\"protocol\":\"{}\"}}",
                state.pixel_count.load(Ordering::Relaxed),
                MAX_PIXELS,
                protocol_name(state.protocol.load(Ordering::Relaxed))
            );
            respond(&mut stream, 200, "application/json", body.as_bytes());
        }
        ("POST", "/api/config") => {
            let body = String::from_utf8_lossy(&req.body);
            let r = match body.trim().parse::<u32>() {
                Ok(n) if n >= 1 && n <= MAX_PIXELS => {
                    push(&state, Msg::Config(n));
                    format!("{{\"ok\":true,\"pixels\":{}}}", n)
                }
                _ => format!("{{\"ok\":false,\"error\":\"pixels must be 1..={}\"}}", MAX_PIXELS),
            };
            respond(&mut stream, 200, "application/json", r.as_bytes());
        }
        ("GET", "/api/protocol") => {
            let body = format!(
                "{{\"protocol\":\"{}\",\"options\":[\"sk9822\",\"ws2812\"]}}",
                protocol_name(state.protocol.load(Ordering::Relaxed))
            );
            respond(&mut stream, 200, "application/json", body.as_bytes());
        }
        ("POST", "/api/protocol") => {
            let body = String::from_utf8_lossy(&req.body);
            let r = match protocol_code(&body) {
                Some(code) => {
                    // the mirror drives no real LEDs — just round-trip the setting
                    state.protocol.store(code, Ordering::Relaxed);
                    format!("{{\"ok\":true,\"protocol\":\"{}\"}}", protocol_name(code))
                }
                None => String::from("{\"ok\":false,\"error\":\"protocol must be sk9822 or ws2812\"}"),
            };
            respond(&mut stream, 200, "application/json", r.as_bytes());
        }
        ("GET", "/api/sync") => {
            let mode = sync_mode_name(state.sync_mode.load(Ordering::Relaxed));
            let time_ms = state.engine_time_ms.load(Ordering::Relaxed);
            let leader = match *state.sync_leader.lock().unwrap() {
                Some((boot, lt, at)) => {
                    let age = at.elapsed().as_millis() as u64;
                    let offset = (lt + age) as i64 - time_ms as i64;
                    format!(
                        "{{\"bootId\":{},\"ageMs\":{},\"offsetMs\":{}}}",
                        boot, age, offset
                    )
                }
                None => String::from("null"),
            };
            let body = format!(
                "{{\"mode\":\"{}\",\"timeMs\":{},\"leader\":{}}}",
                mode, time_ms, leader
            );
            respond(&mut stream, 200, "application/json", body.as_bytes());
        }
        ("POST", "/api/sync") => {
            // body: "off" | "leader" | "follower"
            let body = String::from_utf8_lossy(&req.body);
            let r = match body.trim() {
                "off" => Some(0u8),
                "leader" => Some(1),
                "follower" => Some(2),
                _ => None,
            };
            let resp = match r {
                Some(m) => {
                    state.sync_mode.store(m, Ordering::Relaxed);
                    if m != 2 {
                        *state.sync_leader.lock().unwrap() = None;
                    }
                    format!("{{\"ok\":true,\"mode\":\"{}\"}}", sync_mode_name(m))
                }
                None => String::from(
                    "{\"ok\":false,\"error\":\"mode must be off, leader, or follower\"}",
                ),
            };
            respond(&mut stream, 200, "application/json", resp.as_bytes());
        }
        ("POST", "/api/sensors") => {
            // binary body: one raw sensor-board frame ("SB1.0\0"…"END\0") —
            // same parser the firmware runs on its UART stream
            let r = match luxel_core::netin::parse_sensor_board(&req.body) {
                Some(s) => {
                    *state.sensor_frame.lock().unwrap() = Some(s);
                    state.sensor_seq.fetch_add(1, Ordering::Relaxed);
                    String::from("{\"ok\":true}")
                }
                None => String::from("{\"ok\":false,\"error\":\"not a sensor-board frame\"}"),
            };
            respond(&mut stream, 200, "application/json", r.as_bytes());
        }
        ("GET", "/api/mqtt") => {
            // broker settings (never the password) + connection state
            let body = match &*state.mqtt_cfg.lock().unwrap() {
                Some(c) => format!(
                    "{{\"enabled\":true,\"host\":\"{}\",\"port\":{},\"user\":\"{}\",\"hasPass\":{},\"connected\":{}}}",
                    json_escape(&c.host),
                    c.port,
                    json_escape(&c.user),
                    !c.pass.is_empty(),
                    state.mqtt_connected.load(Ordering::Relaxed)
                ),
                None => String::from(
                    "{\"enabled\":false,\"host\":\"\",\"port\":1883,\"user\":\"\",\"hasPass\":false,\"connected\":false}",
                ),
            };
            respond(&mut stream, 200, "application/json", body.as_bytes());
        }
        ("POST", "/api/mqtt") => {
            // body: "host\nport\nuser\npass"; empty host disables MQTT
            let body = String::from_utf8_lossy(&req.body).into_owned();
            let mut lines = body.lines();
            let host = lines.next().unwrap_or("").trim().to_string();
            let port = lines.next().unwrap_or("").trim().parse::<u16>().unwrap_or(1883);
            let user = lines.next().unwrap_or("").trim().to_string();
            let pass = lines.next().unwrap_or("").trim().to_string();
            let enabled = !host.is_empty();
            *state.mqtt_cfg.lock().unwrap() = enabled.then(|| MqttCfg {
                host,
                port: if port == 0 { 1883 } else { port },
                user,
                pass,
            });
            state.mqtt_gen.fetch_add(1, Ordering::Relaxed);
            let r = format!("{{\"ok\":true,\"enabled\":{}}}", enabled);
            respond(&mut stream, 200, "application/json", r.as_bytes());
        }
        ("GET", "/api/wifi") => {
            let body = match &*state.wifi_ssid.lock().unwrap() {
                Some(ssid) => format!("{{\"ssid\":\"{}\",\"source\":\"flash\"}}", json_escape(ssid)),
                None => String::from("{\"ssid\":null,\"source\":\"none\"}"),
            };
            respond(&mut stream, 200, "application/json", body.as_bytes());
        }
        ("POST", "/api/wifi") => {
            // body = "ssid\npassword"; the mirror just stores the ssid (no reboot)
            let body = String::from_utf8_lossy(&req.body);
            let ssid = body.split('\n').next().unwrap_or("").trim().to_string();
            let r = if ssid.is_empty() {
                String::from("{\"ok\":false,\"error\":\"ssid must be 1..=32 bytes\"}")
            } else {
                *state.wifi_ssid.lock().unwrap() = Some(ssid.clone());
                format!("{{\"ok\":true,\"ssid\":\"{}\",\"note\":\"rebooting to apply\"}}", json_escape(&ssid))
            };
            respond(&mut stream, 200, "application/json", r.as_bytes());
        }
        ("GET", "/api/map") => {
            let body = match &*state.device_map.lock().unwrap() {
                Some((dims, coords)) => format!(
                    "{{\"installed\":true,\"dims\":{},\"count\":{}}}",
                    dims,
                    coords.len()
                ),
                None => String::from("{\"installed\":false,\"dims\":0,\"count\":0}"),
            };
            respond(&mut stream, 200, "application/json", body.as_bytes());
        }
        ("POST", "/api/map") => {
            let body = String::from_utf8_lossy(&req.body);
            let (installed, count) = match parse_map(&body) {
                Some((dims, coords)) => {
                    let n = coords.len();
                    *state.device_map.lock().unwrap() = Some((dims, coords));
                    (true, n)
                }
                None => {
                    *state.device_map.lock().unwrap() = None;
                    (false, 0)
                }
            };
            state.map_dirty.store(true, Ordering::Relaxed);
            let r = format!("{{\"ok\":true,\"installed\":{},\"count\":{}}}", installed, count);
            respond(&mut stream, 200, "application/json", r.as_bytes());
        }
        ("GET", "/api/playlist") => {
            respond(&mut stream, 200, "application/json", playlist_json(&state).as_bytes());
        }
        ("POST", "/api/playlist") => {
            let body = String::from_utf8_lossy(&req.body);
            *state.playlist.lock().unwrap() = parse_playlist(&body);
            push(&state, Msg::PlaylistReload); // apply edits if already playing
            respond(&mut stream, 200, "application/json", b"{\"ok\":true}");
        }
        ("POST", "/api/playlist/play") => {
            let body = String::from_utf8_lossy(&req.body);
            let i = body.trim().parse::<usize>().unwrap_or(0);
            push(&state, Msg::PlaylistPlay(i));
            respond(&mut stream, 200, "application/json", b"{\"ok\":true}");
        }
        ("POST", "/api/playlist/stop") => {
            push(&state, Msg::PlaylistStop);
            respond(&mut stream, 200, "application/json", b"{\"ok\":true}");
        }
        ("POST", "/api/playlist/next") => {
            push(&state, Msg::PlaylistStep(1));
            respond(&mut stream, 200, "application/json", b"{\"ok\":true}");
        }
        ("POST", "/api/playlist/prev") => {
            push(&state, Msg::PlaylistStep(-1));
            respond(&mut stream, 200, "application/json", b"{\"ok\":true}");
        }
        ("POST", "/api/code") => {
            let body = String::from_utf8_lossy(&req.body).into_owned();
            let r = api_code(&state, body);
            respond(&mut stream, 200, "application/json", r.as_bytes());
        }
        ("POST", "/api/control") => {
            let body = String::from_utf8_lossy(&req.body);
            let r = api_control_or_var(&state, &body, false);
            respond(&mut stream, 200, "application/json", r.as_bytes());
        }
        ("POST", "/api/var") => {
            let body = String::from_utf8_lossy(&req.body);
            let r = api_control_or_var(&state, &body, true);
            respond(&mut stream, 200, "application/json", r.as_bytes());
        }
        ("GET", "/api/patterns") => {
            respond(&mut stream, 200, "application/json", patterns_list_json(&state).as_bytes())
        }
        ("POST", "/api/patterns") => {
            let body = String::from_utf8_lossy(&req.body).into_owned();
            let r = patterns_save(&state, &body);
            respond(&mut stream, 200, "application/json", r.as_bytes());
        }
        (m, p) if p.starts_with("/api/patterns/") => {
            let rest = &p["/api/patterns/".len()..];
            let (id, action) = match rest.split_once('/') {
                Some((id, act)) => (id, Some(act)),
                None => (rest, None),
            };
            let r = match (m, action) {
                ("GET", None) => match pattern_by_id(&state, id) {
                    Some(p) => format!(
                        "{{\"id\":\"{}\",\"name\":\"{}\",\"source\":\"{}\"}}",
                        p.id,
                        json_escape(&p.name),
                        json_escape(&p.source)
                    ),
                    None => String::from("{\"ok\":false,\"error\":\"no such pattern\"}"),
                },
                ("DELETE", None) => patterns_delete(&state, id),
                ("POST", Some("activate")) => match pattern_by_id(&state, id) {
                    Some(p) => {
                        let r = api_code(&state, p.source);
                        if r.contains("\"ok\":true") {
                            *state.current_pattern_id.lock().unwrap() = p.id;
                        }
                        r
                    }
                    None => String::from("{\"ok\":false,\"error\":\"no such pattern\"}"),
                },
                _ => String::from("{\"ok\":false,\"error\":\"bad patterns route\"}"),
            };
            respond(&mut stream, 200, "application/json", r.as_bytes());
        }
        _ => respond(&mut stream, 404, "text/plain", b"not found"),
    }
}

pub fn serve_cmd(rest: &[String]) -> ExitCode {
    let mut pixels: u32 = 300;
    let mut port: u16 = 8720;
    // Luxel-to-Luxel sync transport (overridable so e2e can run two
    // mirrors over loopback; the firmware broadcasts on the LAN)
    let mut sync_target = String::from("255.255.255.255");
    let mut sync_port: u16 = luxel_core::netin::SYNC_PORT;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        match (flag.as_str(), it.next()) {
            ("--pixels", Some(v)) => match v.parse() {
                Ok(n) => pixels = n,
                Err(_) => return super::usage(),
            },
            ("--port", Some(v)) => match v.parse() {
                Ok(n) => port = n,
                Err(_) => return super::usage(),
            },
            ("--sync-target", Some(v)) => sync_target = v.clone(),
            ("--sync-port", Some(v)) => match v.parse() {
                Ok(n) => sync_port = n,
                Err(_) => return super::usage(),
            },
            _ => return super::usage(),
        }
    }

    let state = Arc::new(State {
        pixel_count: AtomicU32::new(pixels),
        inbox: Mutex::new(Vec::new()),
        pixels: Mutex::new(Vec::new()),
        fps: AtomicU32::new(0),
        vmerr: Mutex::new(None),
        pattern_src: Mutex::new(String::new()),
        controls_json: Mutex::new(String::from("[]")),
        vars_json: Mutex::new(String::from("{}")),
        readouts_json: Mutex::new(String::from("{}")),
        library: Mutex::new(Vec::new()),
        next_id: AtomicU32::new(0x1a5e_0001),
        brightness: AtomicU8::new(4), // matches the firmware's default
        protocol: AtomicU8::new(0), // sk9822
        playlist: Mutex::new(Playlist::default()),
        pl_playing: AtomicBool::new(false),
        pl_index: AtomicUsize::new(0),
        wifi_ssid: Mutex::new(None),
        device_map: Mutex::new(None),
        map_dirty: AtomicBool::new(false),
        live_pixels: Mutex::new(Vec::new()),
        live_mark: Mutex::new(None),
        live_proto: AtomicU8::new(0),
        power: AtomicBool::new(true),
        current_pattern_id: Mutex::new(String::new()),
        mqtt_cfg: Mutex::new(None),
        mqtt_gen: AtomicU32::new(0),
        mqtt_connected: AtomicBool::new(false),
        sensor_frame: Mutex::new(None),
        sensor_seq: AtomicU32::new(0),
        sync_mode: AtomicU8::new(0),
        sync_boot_id: std::process::id() ^ 0x5a5a_5a5a,
        engine_time_ms: std::sync::atomic::AtomicU64::new(0),
        sync_leader: Mutex::new(None),
    });

    {
        let state = state.clone();
        std::thread::spawn(move || render_loop(state));
    }
    for port in [luxel_core::netin::DDP_PORT, luxel_core::netin::E131_PORT] {
        let state = state.clone();
        std::thread::spawn(move || netin_listener(state, port));
    }
    {
        let state = state.clone();
        std::thread::spawn(move || mqtt_thread(state));
    }
    {
        let state = state.clone();
        std::thread::spawn(move || sync_thread(state, sync_target, sync_port));
    }

    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: cannot bind 127.0.0.1:{port}: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!("luxel serve: http://127.0.0.1:{port}/  ({pixels} px)");

    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let state = state.clone();
        std::thread::spawn(move || handle_connection(stream, state));
    }
    ExitCode::SUCCESS
}
