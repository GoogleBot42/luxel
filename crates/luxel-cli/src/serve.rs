//! `luxel serve` — a native mirror of the firmware's HTTP live-code server.
//!
//! Serves the exact same page and API as `firmware/src/server.rs`, backed by
//! the same engine core, so the browser UI can be developed and end-to-end
//! tested without hardware. See the route table in firmware/src/server.rs —
//! keep the two in lockstep.

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

fn respond(
    req: tiny_http::Request,
    status: u32,
    content_type: &str,
    body: Vec<u8>,
) {
    let header =
        tiny_http::Header::from_bytes(&b"Content-Type"[..], content_type.as_bytes()).unwrap();
    let cors =
        tiny_http::Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap();
    let resp = tiny_http::Response::from_data(body)
        .with_status_code(status as u16)
        .with_header(header)
        .with_header(cors);
    let _ = req.respond(resp);
}

fn push(state: &State, msg: Msg) {
    state.inbox.lock().unwrap().push(msg);
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

    let server = match tiny_http::Server::http(("127.0.0.1", port)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot bind 127.0.0.1:{port}: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!("luxel serve: http://127.0.0.1:{port}/  ({pixels} px)");

    for mut req in server.incoming_requests() {
        let url = req.url().to_string();
        match (req.method().as_str(), url.as_str()) {
            ("GET", "/") => respond(
                req,
                200,
                "text/html; charset=utf-8",
                INDEX_HTML.as_bytes().to_vec(),
            ),
            ("GET", "/api/status") => {
                let fps = state.fps.load(Ordering::Relaxed);
                let body = match state.vmerr.lock().unwrap().as_deref() {
                    Some(e) => format!(
                        "{{\"fps\":{},\"pixels\":{},\"vmerr\":\"{}\"}}",
                        fps,
                        pixels,
                        json_escape(e)
                    ),
                    None => format!("{{\"fps\":{},\"pixels\":{},\"vmerr\":null}}", fps, pixels),
                };
                respond(req, 200, "application/json", body.into_bytes());
            }
            ("GET", "/api/pixels") => {
                let snap = state.pixels.lock().unwrap().clone();
                respond(req, 200, "application/octet-stream", snap);
            }
            ("GET", "/api/pattern") => {
                let src = state.pattern_src.lock().unwrap().clone();
                respond(req, 200, "text/plain; charset=utf-8", src.into_bytes());
            }
            ("GET", "/api/controls") => {
                let s = state.controls_json.lock().unwrap().clone();
                respond(req, 200, "application/json", s.into_bytes());
            }
            ("GET", "/api/vars") => {
                let s = state.vars_json.lock().unwrap().clone();
                respond(req, 200, "application/json", s.into_bytes());
            }
            ("GET", "/api/readouts") => {
                let s = state.readouts_json.lock().unwrap().clone();
                respond(req, 200, "application/json", s.into_bytes());
            }
            ("POST", "/api/code") => {
                let mut src = String::new();
                if req.as_reader().read_to_string(&mut src).is_err() {
                    respond(
                        req,
                        400,
                        "application/json",
                        b"{\"ok\":false,\"line\":0,\"col\":0,\"error\":\"bad body\"}".to_vec(),
                    );
                    continue;
                }
                let body = match Engine::new(&src, pixels, 1) {
                    Ok(_) => {
                        push(&state, Msg::Code(src));
                        String::from("{\"ok\":true}")
                    }
                    Err(d) => {
                        let (line, col) = line_col(&src, d.span.start);
                        format!(
                            "{{\"ok\":false,\"line\":{},\"col\":{},\"error\":\"{}\"}}",
                            line,
                            col,
                            json_escape(&d.message)
                        )
                    }
                };
                respond(req, 200, "application/json", body.into_bytes());
            }
            ("POST", "/api/control") | ("POST", "/api/var") => {
                let is_var = url == "/api/var";
                let mut body = String::new();
                let _ = req.as_reader().read_to_string(&mut body);
                let mut it = body.split_whitespace();
                let Some(name) = it.next() else {
                    respond(
                        req,
                        400,
                        "application/json",
                        b"{\"ok\":false,\"error\":\"missing name\"}".to_vec(),
                    );
                    continue;
                };
                let values: Vec<Fx> = it
                    .filter_map(|v| v.parse::<i32>().ok())
                    .map(Fx::from_raw)
                    .collect();
                if is_var {
                    push(
                        &state,
                        Msg::Var(name.to_string(), values.first().copied().unwrap_or(Fx::ZERO)),
                    );
                } else {
                    push(&state, Msg::Control(name.to_string(), values));
                }
                respond(req, 200, "application/json", b"{\"ok\":true}".to_vec());
            }
            _ => respond(req, 404, "text/plain", b"not found".to_vec()),
        }
    }
    ExitCode::SUCCESS
}
