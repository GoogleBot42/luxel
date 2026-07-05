//! HTTP server: serves the live-code page and the pattern-upload API.
//!
//! v1 protocol (plain HTTP, JSON responses):
//!   GET  /            → live-code page
//!   GET  /api/status  → {"fps":N,"pixels":N,"vmerr":"..."|null}
//!   POST /api/code    → body = pattern source; compile-checked here, then
//!                       handed to the render task. {"ok":true} or
//!                       {"ok":false,"line":N,"col":N,"error":"..."}

use alloc::format;
use alloc::string::String;

use core::sync::atomic::Ordering;

use embassy_net::Stack;
use luxel_core::diag::line_col;
use luxel_core::engine::Engine;
use picoserve::routing::{get, get_service, post};

use crate::shared::{get_pixels, get_vmerr, CODE_QUEUE, FPS};
use crate::PIXEL_COUNT;

const INDEX_HTML: &str = include_str!("index.html");

/// Escape a string for embedding in a JSON literal.
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Last rendered frame as raw RGB bytes (3 per pixel) for the preview.
async fn api_pixels() -> impl picoserve::response::IntoResponse {
    (
        ("Content-Type", "application/octet-stream"),
        get_pixels(),
    )
}

async fn api_status() -> String {
    let fps = FPS.load(Ordering::Relaxed);
    match get_vmerr() {
        Some(e) => format!(
            "{{\"fps\":{},\"pixels\":{},\"vmerr\":\"{}\"}}",
            fps,
            PIXEL_COUNT,
            json_escape(&e)
        ),
        None => format!("{{\"fps\":{},\"pixels\":{},\"vmerr\":null}}", fps, PIXEL_COUNT),
    }
}

async fn api_code(src: String) -> String {
    // Compile-check with the real pixel count so errors surface here with
    // source locations; the render task recompiles the accepted source.
    match Engine::new(&src, PIXEL_COUNT, 1) {
        Ok(_) => {
            CODE_QUEUE.send(src).await;
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
    }
}

pub fn make_app() -> picoserve::Router<impl picoserve::routing::PathRouter> {
    picoserve::Router::new()
        .route(
            "/",
            get_service(picoserve::response::File::html(INDEX_HTML)),
        )
        .route("/api/status", get(api_status))
        .route("/api/pixels", get(api_pixels))
        .route("/api/code", post(api_code))
}

pub const WEB_TASK_POOL_SIZE: usize = 2;

static CONFIG: picoserve::Config = picoserve::Config::const_default();

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
