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
    items: Vec<PlaylistItem>,
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
    format!(
        "{{\"fps\":{},\"pixels\":{},\"slot\":\"native\",\"version\":\"{}\",\"heap_free\":0,\"vmerr\":{}}}",
        fps,
        state.pixel_count.load(Ordering::Relaxed),
        env!("CARGO_PKG_VERSION"),
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

/// Load playlist item `i`: compile its stored pattern, apply its saved control
/// values, and publish the source/controls snapshots. Returns the engine +
/// source on success. Also records the active index.
fn enter_item(state: &State, i: usize) -> Option<(Engine, String)> {
    let item = state.playlist.lock().unwrap().items.get(i).cloned()?;
    // advance the active index even if the pattern is missing (deleted), so a
    // dangling entry just holds for its duration and the loop moves past it
    state.pl_index.store(i, Ordering::Relaxed);
    let sp = pattern_by_id(state, &item.pattern_id)?;
    let mut eng = Engine::new(&sp.source, state.pixel_count.load(Ordering::Relaxed), 1).ok()?;
    for (name, raw) in &item.controls {
        let vals: Vec<Fx> = raw.iter().map(|&r| Fx::from_raw(r)).collect();
        eng.set_control(name, &vals);
    }
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
    let mut frames: u32 = 0;
    let mut fps_mark = Instant::now();
    let mut vars_mark = Instant::now();

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
                    engine = Some(e);
                    current_src = src;
                    last = Instant::now();
                }
                pl_start = Instant::now();
            }
        }

        if vars_mark.elapsed() >= Duration::from_millis(250) {
            vars_mark = Instant::now();
            if let Some(eng) = engine.as_mut() {
                *state.vars_json.lock().unwrap() = jsonview::vars_json(eng);
                *state.readouts_json.lock().unwrap() = jsonview::readouts_json(eng);
            }
        }

        if let Some(eng) = engine.as_mut() {
            let now = Instant::now();
            let delta_us = now.duration_since(last).as_micros() as u64;
            last = now;
            let delta = Fx::from_raw(((delta_us << 16) / 1000) as i32);

            let px = eng.frame(delta);
            {
                let mut snap = state.pixels.lock().unwrap();
                snap.clear();
                for p in px {
                    snap.extend_from_slice(p);
                }
            }
            if let Some(e) = eng.take_error() {
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
                        raw.iter().map(|&r| format!("{}", r as f64 / 65536.0)).collect();
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
        "{{\"defaultSec\":{},\"playing\":{},\"index\":{},\"items\":[{}]}}",
        pl.default_sec,
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
                    Some(p) => api_code(&state, p.source),
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
    });

    {
        let state = state.clone();
        std::thread::spawn(move || render_loop(state));
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
