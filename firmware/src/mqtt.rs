//! MQTT + Home Assistant integration. One embassy task owns a client
//! session against the configured broker (config::read_mqtt; none = idle
//! until /api/mqtt pokes). On connect it publishes HA MQTT-discovery
//! configs (retained) for a `light` (power + brightness) and a pattern
//! `select` (the device library by name), then serves commands and
//! publishes state. Topic names and payloads are shared with the native
//! mirror via luxel_core::hamqtt.

use alloc::string::String;
use core::sync::atomic::{AtomicBool, Ordering};

use embassy_futures::select::{select, Either};
use embassy_net::dns::DnsQueryType;
use embassy_net::tcp::TcpSocket;
use embassy_net::{IpAddress, Ipv4Address, Stack};
use embassy_time::{Duration, Timer};
use esp_println::println;
use luxel_core::hamqtt;
use luxel_core::jsonview::{push_hex, push_piece};
use rust_mqtt::client::client::MqttClient;
use rust_mqtt::client::client_config::{ClientConfig, MqttVersion};
use rust_mqtt::packet::v5::publish_packet::QualityOfService;
use rust_mqtt::utils::rng_generator::CountingRng;

use crate::config;
use crate::patterns;
use crate::shared::{self, Msg, BRIGHTNESS, MQTT_POKE, MSG_QUEUE, POWER};

/// Broker session currently up (for /api/mqtt's `connected`).
pub static CONNECTED: AtomicBool = AtomicBool::new(false);

/// embassy-net 0.9 implements embedded-io-async **0.7**; rust-mqtt is built
/// against **0.6**. Paper-thin adapter delegating to TcpSocket's inherent
/// async methods (which is all the 0.7 impls do too).
struct Io<'a>(TcpSocket<'a>);

impl embedded_io::ErrorType for Io<'_> {
    type Error = embedded_io::ErrorKind;
}
impl embedded_io_async::Read for Io<'_> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.0
            .read(buf)
            .await
            .map_err(|_| embedded_io::ErrorKind::ConnectionReset)
    }
}
impl embedded_io::ReadReady for Io<'_> {
    fn read_ready(&mut self) -> Result<bool, Self::Error> {
        // data buffered, or the peer closed (a read would return promptly)
        Ok(self.0.can_recv() || !self.0.may_recv())
    }
}
impl embedded_io_async::Write for Io<'_> {
    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.0
            .write(buf)
            .await
            .map_err(|_| embedded_io::ErrorKind::ConnectionReset)
    }
    async fn flush(&mut self) -> Result<(), Self::Error> {
        self.0
            .flush()
            .await
            .map_err(|_| embedded_io::ErrorKind::ConnectionReset)
    }
}

/// Device id (client id, topic segment, HA unique_id): "luxel-xxxxxx".
fn device_id() -> String {
    let mac = esp_hal::efuse::base_mac_address();
    let m = mac.as_bytes();
    let mut id = String::new();
    push_piece(&mut id, "luxel-");
    push_hex(&mut id, m[3] as u32, 2);
    push_hex(&mut id, m[4] as u32, 2);
    push_hex(&mut id, m[5] as u32, 2);
    id
}

#[embassy_executor::task]
pub async fn mqtt_task(stack: Stack<'static>) -> ! {
    loop {
        let Some(cfg) = config::read_mqtt() else {
            MQTT_POKE.wait().await; // disabled — sleep until configured
            continue;
        };
        match session(stack, &cfg).await {
            Ok(()) => {} // config changed — reconnect immediately
            Err(e) => {
                CONNECTED.store(false, Ordering::Relaxed);
                println!("mqtt: {} — retrying in 10s", e);
                // retry on a timer, or immediately on a config change
                let _ = select(Timer::after(Duration::from_secs(10)), MQTT_POKE.wait()).await;
            }
        }
        CONNECTED.store(false, Ordering::Relaxed);
    }
}

async fn resolve(stack: Stack<'static>, host: &str) -> Result<IpAddress, &'static str> {
    if let Ok(ip) = host.parse::<Ipv4Address>() {
        return Ok(IpAddress::Ipv4(ip));
    }
    let addrs = stack
        .dns_query(host, DnsQueryType::A)
        .await
        .map_err(|_| "dns lookup failed")?;
    addrs.first().copied().ok_or("host has no A record")
}

/// One broker session: connect, announce, serve until the network drops or
/// the config changes (Ok = config change, Err = reconnect after backoff).
async fn session(stack: Stack<'static>, cfg: &config::MqttConfig) -> Result<(), &'static str> {
    let addr = resolve(stack, &cfg.host).await?;

    // Heap, not locals: task futures are statically allocated, and statics
    // shrink the leftover DRAM that becomes the main task stack (the v0.1.4
    // lesson). ~11 KB of buffers must not live in this future.
    let mut rx = alloc::vec![0u8; 2048];
    let mut tx = alloc::vec![0u8; 2048];
    let mut sock = TcpSocket::new(stack, &mut rx, &mut tx);
    sock.set_timeout(Some(Duration::from_secs(90))); // dead-broker detection
    sock.connect((addr, cfg.port))
        .await
        .map_err(|_| "tcp connect failed")?;

    let id = device_id();
    let avail_topic = hamqtt::availability_topic(&id);
    let mut mc: ClientConfig<'_, 5, CountingRng> =
        ClientConfig::new(MqttVersion::MQTTv5, CountingRng(20000));
    mc.add_client_id(&id);
    mc.max_packet_size = 4096;
    mc.keep_alive = 60;
    if !cfg.user.is_empty() {
        mc.add_username(&cfg.user);
        mc.add_password(&cfg.pass);
    }
    mc.add_will(&avail_topic, hamqtt::OFFLINE.as_bytes(), true);
    mc.add_max_subscribe_qos(QualityOfService::QoS0);

    let mut wbuf = alloc::vec![0u8; 4096]; // fits the select discovery w/ a big library
    let mut rbuf = alloc::vec![0u8; 1024];
    let mut client = MqttClient::<_, 5, _>::new(Io(sock), &mut wbuf, 4096, &mut rbuf, 1024, mc);
    client
        .connect_to_broker()
        .await
        .map_err(|_| "broker refused connection")?;
    println!("mqtt: connected to {}:{}", cfg.host, cfg.port);
    CONNECTED.store(true, Ordering::Relaxed);

    macro_rules! publish {
        ($topic:expr, $payload:expr, $retain:expr) => {
            client
                .send_message($topic, $payload.as_bytes(), QualityOfService::QoS0, $retain)
                .await
                .map_err(|_| "publish failed")?
        };
    }

    // availability + discovery (retained: HA restarts pick the device up)
    publish!(&avail_topic, hamqtt::ONLINE, true);
    let version = env!("CARGO_PKG_VERSION");
    publish!(
        &hamqtt::light_config_topic(&id),
        hamqtt::light_discovery_json(&id, &id, version),
        true
    );
    let mut options: alloc::vec::Vec<String> =
        patterns::list().into_iter().map(|(_, name)| name).collect();
    publish!(
        &hamqtt::pattern_config_topic(&id),
        hamqtt::pattern_discovery_json(&id, &id, version, &options),
        true
    );
    // diagnostics + playlist entities
    for which in ["fps", "heap"] {
        publish!(
            &hamqtt::diag_config_topic(&id, which),
            hamqtt::diag_discovery_json(&id, &id, version, which),
            true
        );
    }
    publish!(
        &hamqtt::playlist_switch_config_topic(&id),
        hamqtt::playlist_switch_discovery_json(&id, &id, version),
        true
    );
    for which in ["next", "prev"] {
        publish!(
            &hamqtt::playlist_button_config_topic(&id, which),
            hamqtt::playlist_button_discovery_json(&id, &id, version, which),
            true
        );
    }
    let light_set = hamqtt::light_set_topic(&id);
    let pattern_set = hamqtt::pattern_set_topic(&id);
    let playlist_cmd = hamqtt::playlist_cmd_topic(&id);
    let event_cmd = hamqtt::event_topic(&id);
    for t in [&light_set, &pattern_set, &playlist_cmd, &event_cmd] {
        client
            .subscribe_to_topic(t)
            .await
            .map_err(|_| "subscribe failed")?;
    }

    // state publishing: on change (checked every tick) and at session start
    let mut last_light = String::new();
    let mut last_pattern: Option<String> = None;
    let mut last_playing: Option<bool> = None;
    let mut ticks: u32 = 0;
    loop {
        // ---- publish dirty state ----
        let light = hamqtt::light_state_json(
            POWER.load(Ordering::Relaxed),
            hamqtt::brightness_to_ha(BRIGHTNESS.load(Ordering::Relaxed)),
        );
        if light != last_light {
            publish!(&hamqtt::light_state_topic(&id), light, false);
            last_light = light;
        }
        let cur = shared::get_current_pattern_id();
        let name = patterns::name_of(&cur).unwrap_or_default();
        if last_pattern.as_deref() != Some(&name) {
            publish!(&hamqtt::pattern_state_topic(&id), name, false);
            last_pattern = Some(name);
        }
        let playing = crate::playlist::is_playing();
        if last_playing != Some(playing) {
            publish!(
                &hamqtt::playlist_state_topic(&id),
                if playing { "ON" } else { "OFF" },
                false
            );
            last_playing = Some(playing);
        }

        // ---- wait for a command, ~5s at a time ----
        match select(client.receive_message(), Timer::after(Duration::from_secs(5))).await {
            Either::First(Ok((topic, payload))) => {
                let payload = core::str::from_utf8(payload).unwrap_or("");
                if topic == light_set {
                    let cmd = hamqtt::parse_light_command(payload);
                    if let Some(ha) = cmd.brightness {
                        let dev = hamqtt::brightness_from_ha(ha);
                        BRIGHTNESS.store(dev, Ordering::Relaxed);
                        persist_brightness(dev);
                    }
                    if let Some(on) = cmd.on {
                        POWER.store(on, Ordering::Relaxed);
                    }
                } else if topic == pattern_set {
                    activate_by_name(payload).await;
                } else if topic == playlist_cmd {
                    match payload {
                        "play" => crate::playlist::play(0),
                        "stop" => crate::playlist::stop(),
                        "next" => crate::playlist::step(1),
                        "prev" => crate::playlist::step(-1),
                        other => println!("mqtt: unknown playlist cmd \"{}\"", other),
                    }
                } else if topic == event_cmd {
                    // pattern event injection: "type [x [y [value]]]" per
                    // line → the readEvent() queue, same as POST /api/events
                    let evs = hamqtt::parse_event_lines(payload);
                    if !evs.is_empty() {
                        shared::push_events(&evs);
                    }
                }
                // loop: publishes the resulting state before waiting again
            }
            Either::First(Err(_)) => return Err("connection lost"),
            Either::Second(()) => {
                ticks += 1;
                if ticks % 3 == 0 {
                    client.send_ping().await.map_err(|_| "ping failed")?;
                    // diagnostics every ~15s (HA graphs don't need more)
                    publish!(
                        &hamqtt::diag_state_topic(&id),
                        hamqtt::diag_state_json(
                            crate::shared::FPS.load(Ordering::Relaxed),
                            esp_alloc::HEAP.free() as u32
                        ),
                        false
                    );
                }
                // library changed? re-announce the select's options
                let now: alloc::vec::Vec<String> =
                    patterns::list().into_iter().map(|(_, name)| name).collect();
                if now != options {
                    options = now;
                    publish!(
                        &hamqtt::pattern_config_topic(&id),
                        hamqtt::pattern_discovery_json(&id, &id, version, &options),
                        true
                    );
                }
                if MQTT_POKE.signaled() {
                    MQTT_POKE.wait().await;
                    let _ = client.disconnect().await;
                    return Ok(()); // reconnect with the new config
                }
            }
        }
    }
}

/// Run a library pattern by its HA select option (exact name). Same path as
/// POST /api/patterns/<id>/activate, plus stopping the playlist — picking a
/// pattern from HA means "show this now", not "until the playlist advances".
async fn activate_by_name(name: &str) {
    let Some(id) = patterns::id_by_name(name) else {
        println!("mqtt: no pattern named \"{}\"", name);
        return;
    };
    let Some(source) = patterns::source_of(&id) else {
        return;
    };
    let Some(bc) = patterns::bytecode_of(&id) else {
        println!("mqtt: pattern \"{}\" has no stored bytecode", name);
        return;
    };
    if luxel_core::bytecode::validate(&bc).is_err() {
        println!("mqtt: stored bytecode for \"{}\" is stale (re-save from the app)", name);
        return;
    }
    crate::playlist::stop();
    let env = luxel_core::bytecode::encode_envelope("", &source, &bc);
    drop((source, bc));
    MSG_QUEUE.send(Msg::Code { env, id }).await;
    // same single-pattern resume bookkeeping as the HTTP activate
    crate::shared::set_current_controls(alloc::vec::Vec::new());
    crate::resume::mark_dirty();
}

/// Persist a brightness change (read-modify-write, like POST /api/brightness).
fn persist_brightness(dev: u8) {
    let mut c = config::read_device().unwrap_or(crate::shared::device_config_snapshot());
    c.brightness = dev;
    if let Err(e) = config::write_device(&c) {
        println!("mqtt: persist brightness: {}", e);
    }
}
