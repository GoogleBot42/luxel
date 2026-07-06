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
use picoserve::routing::RequestHandlerService as _;

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
    let slot = crate::ota::booted_slot();
    let version = env!("CARGO_PKG_VERSION");
    let heap = esp_alloc::HEAP.free();
    json_response(match get_vmerr() {
        Some(e) => format!(
            "{{\"fps\":{},\"pixels\":{},\"slot\":\"{}\",\"version\":\"{}\",\"heap_free\":{},\"vmerr\":\"{}\"}}",
            fps,
            PIXEL_COUNT,
            slot,
            version,
            heap,
            json_escape(&e)
        ),
        None => format!(
            "{{\"fps\":{},\"pixels\":{},\"slot\":\"{}\",\"version\":\"{}\",\"heap_free\":{},\"vmerr\":null}}",
            fps, PIXEL_COUNT, slot, version, heap
        ),
    })
}

/// Streams an app image into the inactive OTA slot. See src/ota.rs. Reboots
/// ~400 ms after the success response so the reply reaches the client.
struct OtaService;

impl<State, PathParameters> picoserve::routing::RequestHandlerService<State, PathParameters>
    for OtaService
{
    async fn call_request_handler_service<
        R: picoserve::io::Read,
        W: picoserve::response::ResponseWriter<Error = R::Error>,
    >(
        &self,
        _state: &State,
        _path_parameters: PathParameters,
        mut request: picoserve::request::Request<'_, R>,
        response_writer: W,
    ) -> Result<picoserve::ResponseSent, W::Error> {
        use picoserve::io::Read as _;

        let result: Result<u32, &'static str> = {
            let mut reader = request.body_connection.body().reader();
            match crate::ota::begin() {
                Err(e) => Err(e),
                Ok(mut writer) => {
                    // sector-sized chunks: FlashStorage handles erase, and
                    // 4 KiB keeps peak RAM small
                    let mut buf = alloc::vec![0u8; 4096];
                    let mut fill = 0usize;
                    let mut failed: Option<&'static str> = None;
                    loop {
                        match reader.read(&mut buf[fill..]).await {
                            Ok(0) => break,
                            Ok(n) => {
                                fill += n;
                                if fill == buf.len() {
                                    if let Err(e) = writer.write(&buf[..fill]) {
                                        failed = Some(e);
                                        break;
                                    }
                                    fill = 0;
                                }
                            }
                            Err(_) => {
                                failed = Some("body read failed");
                                break;
                            }
                        }
                    }
                    if failed.is_none() && fill > 0 {
                        if let Err(e) = writer.write(&buf[..fill]) {
                            failed = Some(e);
                        }
                    }
                    match failed {
                        None => writer.commit(),
                        Some(e) => Err(e),
                    }
                }
            }
        };

        let body = match &result {
            Ok(n) => {
                esp_println::println!("ota: {} bytes written, activating + rebooting", n);
                format!("{{\"ok\":true,\"bytes\":{}}}", n)
            }
            Err(e) => {
                esp_println::println!("ota failed: {}", e);
                format!("{{\"ok\":false,\"error\":\"{}\"}}", json_escape(e))
            }
        };
        use picoserve::response::IntoResponse as _;
        let connection = request.body_connection.finalize().await?;
        let sent = json_response(body)
            .write_to(connection, response_writer)
            .await?;
        if result.is_ok() {
            crate::REBOOT.signal(());
        }
        Ok(sent)
    }
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

/// Flat single-level dispatcher. Do NOT go back to chaining `.route()`
/// calls: picoserve nests one router type per route, and polling that
/// chain put a multi-KB stack frame on the executor per level — at 11
/// routes it overflowed the main task's stack straight into the WiFi
/// blob's .bss (g_phyFuns and pm state at 0x3ffdb1xx), which surfaced as
/// wild-pointer crashes deep inside radio code. A `match` keeps the poll
/// frame constant no matter how many routes we add.
struct Api;

impl<State, PathParameters> picoserve::routing::PathRouterService<State, PathParameters> for Api {
    async fn call_path_router_service<
        R: picoserve::io::Read,
        W: picoserve::response::ResponseWriter<Error = R::Error>,
    >(
        &self,
        state: &State,
        path_parameters: PathParameters,
        path: picoserve::request::Path<'_>,
        mut request: picoserve::request::Request<'_, R>,
        response_writer: W,
    ) -> Result<picoserve::ResponseSent, W::Error> {
        use picoserve::response::{IntoResponse as _, StatusCode};

        let method = request.parts.method();
        let route = path.encoded();

        if method.eq_ignore_ascii_case("POST") {
            match route {
                // streams its own body; delegates entirely
                "/api/ota" => {
                    return OtaService
                        .call_request_handler_service(
                            state,
                            path_parameters,
                            request,
                            response_writer,
                        )
                        .await;
                }
                "/api/code" | "/api/control" | "/api/var" => {
                    let body = match request.body_connection.body().read_all().await {
                        Ok(bytes) => String::from_utf8_lossy(bytes).into_owned(),
                        Err(_) => {
                            let conn = request.body_connection.finalize().await?;
                            return (StatusCode::BAD_REQUEST, "body read failed")
                                .write_to(conn, response_writer)
                                .await;
                        }
                    };
                    let response = match route {
                        "/api/code" => api_code(body).await,
                        "/api/control" => api_control(body).await,
                        _ => api_var(body).await,
                    };
                    let conn = request.body_connection.finalize().await?;
                    return response.write_to(conn, response_writer).await;
                }
                _ => {}
            }
        } else if method.eq_ignore_ascii_case("GET") {
            macro_rules! respond {
                ($resp:expr) => {{
                    let resp = $resp;
                    let conn = request.body_connection.finalize().await?;
                    return resp.write_to(conn, response_writer).await;
                }};
            }
            match route {
                "/" => respond!((("Content-Type", "text/html; charset=utf-8"), INDEX_HTML)),
                "/api/status" => respond!(api_status().await),
                "/api/pixels" => respond!(api_pixels().await),
                "/api/pattern" => respond!(api_pattern().await),
                "/api/controls" => respond!(api_controls().await),
                "/api/vars" => respond!(api_vars().await),
                "/api/readouts" => respond!(api_readouts().await),
                _ => {}
            }
        }

        let conn = request.body_connection.finalize().await?;
        (StatusCode::NOT_FOUND, "not found")
            .write_to(conn, response_writer)
            .await
    }
}

pub fn make_app() -> picoserve::Router<impl picoserve::routing::PathRouter> {
    picoserve::Router::new().nest_service("", Api)
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
