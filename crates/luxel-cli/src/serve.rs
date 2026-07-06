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
use std::sync::atomic::{AtomicU32, Ordering};
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
}

struct State {
    pixel_count: u32,
    inbox: Mutex<Vec<Msg>>,
    pixels: Mutex<Vec<u8>>,
    fps: AtomicU32,
    vmerr: Mutex<Option<String>>,
    pattern_src: Mutex<String>,
    controls_json: Mutex<String>,
    vars_json: Mutex<String>,
    readouts_json: Mutex<String>,
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
        state.pixel_count,
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

fn render_loop(state: Arc<State>) {
    let mut engine = Engine::new(DEFAULT_PATTERN, state.pixel_count, 1).ok();
    *state.pattern_src.lock().unwrap() = DEFAULT_PATTERN.to_string();
    if let Some(eng) = engine.as_ref() {
        *state.controls_json.lock().unwrap() = jsonview::controls_json(eng);
    }
    let mut last = Instant::now();
    let mut frames: u32 = 0;
    let mut fps_mark = Instant::now();
    let mut vars_mark = Instant::now();

    loop {
        for msg in state.inbox.lock().unwrap().drain(..) {
            match msg {
                Msg::Code(src) => {
                    if let Ok(e) = Engine::new(&src, state.pixel_count, 1) {
                        *state.controls_json.lock().unwrap() = jsonview::controls_json(&e);
                        engine = Some(e);
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
    match Engine::new(&body, state.pixel_count, 1) {
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
        pixel_count: pixels,
        inbox: Mutex::new(Vec::new()),
        pixels: Mutex::new(Vec::new()),
        fps: AtomicU32::new(0),
        vmerr: Mutex::new(None),
        pattern_src: Mutex::new(String::new()),
        controls_json: Mutex::new(String::from("[]")),
        vars_json: Mutex::new(String::from("{}")),
        readouts_json: Mutex::new(String::from("{}")),
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
