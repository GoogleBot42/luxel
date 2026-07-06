//! HTTP server: serves the live-code page and the device API. Keep routes
//! and response shapes in lockstep with the native mirror
//! (crates/luxel-cli/src/serve.rs) — the playground's device mode talks to
//! both interchangeably.
//!
//! API (all responses carry Access-Control-Allow-Origin: * so the
//! playground dev server can target a device directly):
//!   GET  /              live-code page
//!   GET  /api/status    {"fps":N,"pixels":N,"vmerr":"…"|null}
//!   GET  /api/pixels    raw RGB bytes, 3 per pixel
//!   GET  /api/pattern   source of the running pattern (text/plain)
//!   GET  /api/controls  [{"kind","label","name"},…]
//!   GET  /api/vars      {"name":raw|[raw,…],…}        (raw 16.16)
//!   GET  /api/readouts  {"showName":raw|null,…}       (showNumber/gauge)
//!   POST /api/code      body = source → {"ok":true} | {"ok":false,"line","col","error"}
//!   POST /api/control   body = "name raw0 [raw1 raw2]" → {"ok":true}
//!   POST /api/var       body = "name raw" → {"ok":true}

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use core::sync::atomic::Ordering;

use embassy_net::Stack;
use luxel_core::diag::line_col;
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::jsonview::json_escape;
use picoserve::routing::{get, get_service, post};

use crate::shared::{
    get_pattern_src, get_pixels, get_vmerr, snapshot, Msg, CONTROLS_JSON, FPS, MSG_QUEUE,
    READOUTS_JSON, VARS_JSON,
};
use crate::PIXEL_COUNT;

const INDEX_HTML: &str = include_str!("index.html");

const CORS: (&str, &str) = ("Access-Control-Allow-Origin", "*");
const JSON: (&str, &str) = ("Content-Type", "application/json");

type ApiResponse = ((&'static str, &'static str), (&'static str, &'static str), String);

fn json_response(body: String) -> ApiResponse {
    (CORS, JSON, body)
}

async fn api_status() -> ApiResponse {
    let fps = FPS.load(Ordering::Relaxed);
    json_response(match get_vmerr() {
        Some(e) => format!(
            "{{\"fps\":{},\"pixels\":{},\"vmerr\":\"{}\"}}",
            fps,
            PIXEL_COUNT,
            json_escape(&e)
        ),
        None => format!("{{\"fps\":{},\"pixels\":{},\"vmerr\":null}}", fps, PIXEL_COUNT),
    })
}

/// Last rendered frame as raw RGB bytes (3 per pixel) for the preview.
async fn api_pixels() -> impl picoserve::response::IntoResponse {
    (CORS, ("Content-Type", "application/octet-stream"), get_pixels())
}

async fn api_pattern() -> impl picoserve::response::IntoResponse {
    (CORS, ("Content-Type", "text/plain; charset=utf-8"), get_pattern_src())
}

async fn api_controls() -> ApiResponse {
    let s = snapshot(&CONTROLS_JSON);
    json_response(if s == "{}" { String::from("[]") } else { s })
}

async fn api_vars() -> ApiResponse {
    json_response(snapshot(&VARS_JSON))
}

async fn api_readouts() -> ApiResponse {
    json_response(snapshot(&READOUTS_JSON))
}

async fn api_code(src: String) -> ApiResponse {
    // Compile-check with the real pixel count so errors surface here with
    // source locations; the render task recompiles the accepted source.
    json_response(match Engine::new(&src, PIXEL_COUNT, 1) {
        Ok(_) => {
            MSG_QUEUE.send(Msg::Code(src)).await;
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
    })
}

/// Body: `name raw0 [raw1 raw2]` — whitespace-separated, values raw 16.16.
async fn api_control(body: String) -> ApiResponse {
    let mut it = body.split_whitespace();
    let Some(name) = it.next() else {
        return json_response(String::from("{\"ok\":false,\"error\":\"missing name\"}"));
    };
    let values: Vec<Fx> = it
        .filter_map(|v| v.parse::<i32>().ok())
        .map(Fx::from_raw)
        .collect();
    MSG_QUEUE.send(Msg::Control(String::from(name), values)).await;
    json_response(String::from("{\"ok\":true}"))
}

/// Body: `name raw` — raw 16.16.
async fn api_var(body: String) -> ApiResponse {
    let mut it = body.split_whitespace();
    let (Some(name), Some(raw)) = (it.next(), it.next().and_then(|v| v.parse::<i32>().ok()))
    else {
        return json_response(String::from("{\"ok\":false,\"error\":\"expected: name raw\"}"));
    };
    MSG_QUEUE.send(Msg::Var(String::from(name), Fx::from_raw(raw))).await;
    json_response(String::from("{\"ok\":true}"))
}

pub fn make_app() -> picoserve::Router<impl picoserve::routing::PathRouter> {
    picoserve::Router::new()
        .route("/", get_service(picoserve::response::File::html(INDEX_HTML)))
        .route("/api/status", get(api_status))
        .route("/api/pixels", get(api_pixels))
        .route("/api/pattern", get(api_pattern))
        .route("/api/controls", get(api_controls))
        .route("/api/vars", get(api_vars))
        .route("/api/readouts", get(api_readouts))
        .route("/api/code", post(api_code))
        .route("/api/control", post(api_control))
        .route("/api/var", post(api_var))
}

pub const WEB_TASK_POOL_SIZE: usize = 2;

// keep_connection_alive: without it every preview poll (15/s) pays a full
// TCP open/close on a chip with a 2-connection pool — the browser reuses
// one connection instead.
static CONFIG: picoserve::Config = picoserve::Config::const_default().keep_connection_alive();

#[embassy_executor::task(pool_size = WEB_TASK_POOL_SIZE)]
pub async fn web_task(task_id: usize, stack: Stack<'static>) -> ! {
    // Pattern uploads arrive in the http buffer; size it for real-world
    // .epe sources (the corpus tops out around 16 KB). Heap-allocated so
    // the task future (statically allocated per pool slot) stays small.
    let mut tcp_rx_buffer = alloc::vec![0u8; 4096];
    let mut tcp_tx_buffer = alloc::vec![0u8; 4096];
    let mut http_buffer = alloc::vec![0u8; 24 * 1024];

    let app = make_app();
    picoserve::Server::new(&app, &CONFIG, &mut http_buffer)
        .listen_and_serve(task_id, stack, 80, &mut tcp_rx_buffer, &mut tcp_tx_buffer)
        .await
        .into_never()
}
