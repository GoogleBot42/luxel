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
//!   GET  /api/wifi      {"ssid":"…"|null,"source":"flash"|"builtin"|"none"}
//!   POST /api/wifi      body = "ssid\npassword" → stores creds in flash + reboots

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

fn status_json() -> String {
    let fps = FPS.load(Ordering::Relaxed);
    let slot = crate::ota::booted_slot();
    let version = env!("CARGO_PKG_VERSION");
    let heap = esp_alloc::HEAP.free();
    match get_vmerr() {
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
    }
}

async fn api_status() -> ApiResponse {
    json_response(status_json())
}

/// Handle one multiplexed request from the socket. Wire format keeps the
/// device side trivial: text frame `"<id> <call>\n<body>"`, reply
/// `{"id":<id>,"r":<the same JSON the HTTP route returns>}`. Multiplexing
/// API calls onto the push socket matters because the chip only serves two
/// connections — the browser needs just this one.
async fn handle_ws_call(frame: &str) -> String {
    let (header, body) = frame.split_once('\n').unwrap_or((frame, ""));
    let mut it = header.split_whitespace();
    let (id, call) = match (it.next().and_then(|v| v.parse::<u32>().ok()), it.next()) {
        (Some(id), Some(call)) => (id, call),
        _ => return String::from("{\"id\":0,\"r\":{\"ok\":false,\"error\":\"bad frame\"}}"),
    };
    let r = match call {
        "code" => api_code(String::from(body)).await.2,
        "control" => api_control(String::from(body)).await.2,
        "var" => api_var(String::from(body)).await.2,
        "pattern" => format!("{{\"pattern\":\"{}\"}}", json_escape(&get_pattern_src())),
        _ => String::from("{\"ok\":false,\"error\":\"unknown call\"}"),
    };
    format!("{{\"id\":{},\"r\":{}}}", id, r)
}

/// Full-duplex preview + API socket. Pushes binary pixel frames (~15 Hz)
/// and typed JSON text frames ({"type":"status"|"controls"|"vars"|
/// "readouts",…}); receives multiplexed API calls (see [handle_ws_call]).
/// Receiving uses next_message's signal parameter (a timer) — the
/// sanctioned way to interleave pushes with reads.
struct PreviewWs;

impl picoserve::response::ws::WebSocketCallback for PreviewWs {
    async fn run<R: picoserve::io::Read, W: picoserve::io::Write<Error = R::Error>>(
        self,
        mut rx: picoserve::response::ws::SocketRx<R>,
        mut tx: picoserve::response::ws::SocketTx<W>,
    ) -> Result<(), W::Error> {
        use picoserve::futures::Either;
        use picoserve::response::ws::Message;

        // pattern sources arrive here in device mode — size for the corpus
        let mut rxbuf = alloc::vec![0u8; 16 * 1024];
        let mut tick: u32 = 0;
        let mut next_push = embassy_time::Instant::now();
        loop {
            match rx
                .next_message(&mut rxbuf, embassy_time::Timer::at(next_push))
                .await?
            {
                Either::First(Ok(Message::Text(t))) => {
                    let reply = handle_ws_call(t).await;
                    tx.send_text(&reply).await?;
                }
                Either::First(Ok(Message::Ping(d))) => tx.send_pong(d).await?,
                Either::First(Ok(Message::Close(_))) => return tx.close(None).await,
                Either::First(Ok(_)) => {}
                Either::First(Err(_)) => return tx.close((1002u16, "protocol error")).await,
                Either::Second(()) => {
                    next_push = embassy_time::Instant::now()
                        + embassy_time::Duration::from_millis(66);
                    let px = get_pixels();
                    if !px.is_empty() {
                        tx.send_binary(&px).await?;
                    }
                    if tick % 4 == 0 {
                        let vars = snapshot(&VARS_JSON);
                        tx.send_text(&format!("{{\"type\":\"vars\",\"vars\":{}}}", vars))
                            .await?;
                        let ro = snapshot(&READOUTS_JSON);
                        tx.send_text(&format!("{{\"type\":\"readouts\",\"readouts\":{}}}", ro))
                            .await?;
                    }
                    if tick % 15 == 0 {
                        tx.send_text(&format!(
                            "{{\"type\":\"status\",\"status\":{}}}",
                            status_json()
                        ))
                        .await?;
                        let controls = {
                            let s = snapshot(&CONTROLS_JSON);
                            if s == "{}" {
                                String::from("[]")
                            } else {
                                s
                            }
                        };
                        tx.send_text(&format!(
                            "{{\"type\":\"controls\",\"controls\":{}}}",
                            controls
                        ))
                        .await?;
                    }
                    tick = tick.wrapping_add(1);
                }
            }
        }
    }
}

/// A flash-resident asset (playground bundle) streamed in 2 KiB chunks —
/// whole files don't fit the heap.
struct FlashAsset(crate::assets::AssetEntry);

impl picoserve::response::Content for FlashAsset {
    fn content_type(&self) -> &'static str {
        // Content trait wants &'static; map the known types
        match self.0.ctype.as_str() {
            t if t.starts_with("text/html") => "text/html; charset=utf-8",
            t if t.starts_with("application/javascript") => {
                "application/javascript; charset=utf-8"
            }
            t if t.starts_with("text/css") => "text/css; charset=utf-8",
            t if t.starts_with("application/wasm") => "application/wasm",
            t if t.starts_with("image/svg") => "image/svg+xml",
            t if t.starts_with("image/png") => "image/png",
            _ => "application/octet-stream",
        }
    }

    fn content_length(&self) -> usize {
        self.0.len as usize
    }

    async fn write_content<W: picoserve::io::Write>(self, mut writer: W) -> Result<(), W::Error> {
        // Each esp-storage flash read briefly disables the cache and starves
        // WiFi/the executor. yield_now wasn't enough for multi-chunk files
        // (the second write_all hung): a real Timer::after cedes wall-clock
        // time so the WiFi task actually runs between flash reads. 4 KiB
        // chunks keep each cache-off window short.
        let mut buf = alloc::vec![0u8; 4096];
        let mut at = 0u32;
        while at < self.0.len {
            let n = (self.0.len - at).min(4096) as usize;
            if !crate::assets::read_chunk(self.0.offset + at, &mut buf[..n]) {
                break; // flash busy (OTA in flight) — truncated response
            }
            writer.write_all(&buf[..n]).await?;
            at += n as u32;
            embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
        }
        Ok(())
    }
}

/// Streams a LUXA archive into the assets flash region (see src/assets.rs).
/// Same streaming shape as OtaService; no reboot — the TOC hot-reloads.
struct AssetsService;

impl<State, PathParameters> picoserve::routing::RequestHandlerService<State, PathParameters>
    for AssetsService
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
            let mut body = request.body_connection.body();
            let expected = body.content_length() as u32;
            match crate::assets::begin(expected).await {
                Err(e) => Err(e),
                Ok(mut writer) => {
                    let mut reader = body.reader();
                    let mut buf = alloc::vec![0u8; 4096];
                    let mut fill = 0usize;
                    let mut failed: Option<&'static str> = None;
                    loop {
                        match reader.read(&mut buf[fill..]).await {
                            Ok(0) => break,
                            Ok(n) => {
                                fill += n;
                                if fill == buf.len() {
                                    if let Err(e) = writer.write(&buf[..fill]).await {
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
                        if let Err(e) = writer.write(&buf[..fill]).await {
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
                esp_println::println!("assets: {} bytes installed", n);
                format!("{{\"ok\":true,\"bytes\":{},\"files\":{}}}", n, crate::assets::count())
            }
            Err(e) => {
                esp_println::println!("assets install failed: {}", e);
                format!("{{\"ok\":false,\"error\":\"{}\"}}", json_escape(e))
            }
        };
        use picoserve::response::IntoResponse as _;
        let connection = request.body_connection.finalize().await?;
        json_response(body).write_to(connection, response_writer).await
    }
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
            // NOTE: picoserve's read_request timeout is one timer for the
            // WHOLE body (created when the reader is taken), not per read —
            // take the reader only after the erase phase so slow uploads get
            // the full budget.
            let mut body = request.body_connection.body();
            let expected = body.content_length() as u32;
            match crate::ota::begin() {
                Err(e) => Err(e),
                Ok(mut writer) => {
                    let mut reader = body.reader();
                    // 4 KiB (one sector) chunks: write() erases+writes each,
                    // interleaved with these network reads (see ota.rs)
                    let mut buf = alloc::vec![0u8; 4096];
                    let mut fill = 0usize;
                    let mut failed: Option<&'static str> = None;
                    loop {
                        match reader.read(&mut buf[fill..]).await {
                            Ok(0) => break,
                            Ok(n) => {
                                fill += n;
                                if fill == buf.len() {
                                    if let Err(e) = writer.write(&buf[..fill]).await {
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
                        if let Err(e) = writer.write(&buf[..fill]).await {
                            failed = Some(e);
                        }
                    }
                    match failed {
                        None => writer.commit(expected),
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
                // streams its own body; delegates entirely (boxed — large
                // sub-futures must stay off the dispatcher's poll stack, see
                // the /ws arm)
                "/api/ota" => {
                    return alloc::boxed::Box::pin(OtaService.call_request_handler_service(
                        state,
                        path_parameters,
                        request,
                        response_writer,
                    ))
                    .await;
                }
                "/api/assets" => {
                    return alloc::boxed::Box::pin(AssetsService.call_request_handler_service(
                        state,
                        path_parameters,
                        request,
                        response_writer,
                    ))
                    .await;
                }
                // body: "ssid\npassword" → stored in flash, applied by the
                // reboot this triggers. See config.rs.
                "/api/wifi" => {
                    let body = match request.body_connection.body().read_all().await {
                        Ok(bytes) => String::from_utf8_lossy(bytes).into_owned(),
                        Err(_) => String::new(),
                    };
                    let (ssid, pass) = body.split_once('\n').unwrap_or((body.as_str(), ""));
                    let (ssid, pass) = (ssid.trim_end_matches('\r'), pass.trim_end_matches(['\r', '\n']));
                    let result = crate::config::write_wifi(ssid, pass);
                    let response = json_response(match &result {
                        Ok(()) => {
                            esp_println::println!("wifi creds stored (ssid \"{}\"); rebooting", ssid);
                            format!("{{\"ok\":true,\"ssid\":\"{}\",\"note\":\"rebooting to apply\"}}", json_escape(ssid))
                        }
                        Err(e) => format!("{{\"ok\":false,\"error\":\"{}\"}}", json_escape(e)),
                    });
                    let conn = request.body_connection.finalize().await?;
                    let sent = response.write_to(conn, response_writer).await?;
                    if result.is_ok() {
                        crate::REBOOT.signal(());
                    }
                    return Ok(sent);
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
        } else if method.eq_ignore_ascii_case("OPTIONS") {
            // CORS preflight: a cross-origin DELETE (or any non-simple
            // request) sends OPTIONS first. Without these headers the
            // browser blocks the real call — the hosted playground talking
            // to a device by IP needs this.
            use picoserve::response::{IntoResponse as _, StatusCode};
            let conn = request.body_connection.finalize().await?;
            return (
                StatusCode::NO_CONTENT,
                [
                    ("Access-Control-Allow-Origin", "*"),
                    ("Access-Control-Allow-Methods", "GET, POST, DELETE, OPTIONS"),
                    ("Access-Control-Allow-Headers", "Content-Type"),
                    ("Access-Control-Max-Age", "86400"),
                ],
                "",
            )
                .write_to(conn, response_writer)
                .await;
        } else if method.eq_ignore_ascii_case("GET") {
            macro_rules! respond {
                ($resp:expr) => {{
                    let resp = $resp;
                    let conn = request.body_connection.finalize().await?;
                    return resp.write_to(conn, response_writer).await;
                }};
            }
            if route == "/ws" {
                use picoserve::extract::FromRequest as _;
                let parts = request.parts;
                let upgrade = picoserve::response::ws::WebSocketUpgrade::from_request(
                    state,
                    parts,
                    request.body_connection.body(),
                )
                .await;
                let conn = request.body_connection.finalize().await?;
                return match upgrade {
                    // Box::pin: the ws-session state machine is large, and
                    // inlining it bloated the dispatcher's poll frame enough
                    // that ANY request overflowed the task stack (second
                    // stack-guard incident). Heap-allocate it instead.
                    Ok(u) => {
                        alloc::boxed::Box::pin(
                            u.on_upgrade(PreviewWs).write_to(conn, response_writer),
                        )
                        .await
                    }
                    Err(_) => {
                        (StatusCode::BAD_REQUEST, "expected a websocket upgrade")
                            .write_to(conn, response_writer)
                            .await
                    }
                };
            }
            match route {
                // "/" serves the installed playground when present; the
                // embedded minimal page is the fallback (and stays reachable
                // at /min for bring-up debugging)
                "/" => {
                    if let Some(e) = crate::assets::lookup("/index.html") {
                        if e.gzip {
                            respond!((("Content-Encoding", "gzip"), FlashAsset(e)));
                        }
                        respond!(FlashAsset(e));
                    }
                    respond!((("Content-Type", "text/html; charset=utf-8"), INDEX_HTML));
                }
                "/min" => respond!((("Content-Type", "text/html; charset=utf-8"), INDEX_HTML)),
                "/api/status" => respond!(api_status().await),
                // which network the NEXT boot will join (never the password)
                "/api/wifi" => {
                    let body = match crate::config::read_wifi() {
                        Some((ssid, _)) => format!(
                            "{{\"ssid\":\"{}\",\"source\":\"flash\"}}",
                            json_escape(&ssid)
                        ),
                        None => match option_env!("LUXEL_SSID") {
                            Some(s) if !s.is_empty() => format!(
                                "{{\"ssid\":\"{}\",\"source\":\"builtin\"}}",
                                json_escape(s)
                            ),
                            _ => String::from("{\"ssid\":null,\"source\":\"none\"}"),
                        },
                    };
                    respond!(json_response(body));
                }
                "/api/pixels" => respond!(api_pixels().await),
                "/api/pattern" => respond!(api_pattern().await),
                "/api/controls" => respond!(api_controls().await),
                "/api/vars" => respond!(api_vars().await),
                "/api/readouts" => respond!(api_readouts().await),
                other => {
                    if let Some(e) = crate::assets::lookup(other) {
                        if e.gzip {
                            respond!((("Content-Encoding", "gzip"), FlashAsset(e)));
                        }
                        respond!(FlashAsset(e));
                    }
                }
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

// 3: one slot can be pinned by the preview websocket
// 3 connections: the bidirectional preview socket pins one, leaving two for
// page/asset loads + API — ends the ws-handshake-vs-poll race. (The earlier
// crash that forced this back to 2 was the dispatcher stack overflow, now
// fixed by boxing the ws/ota sub-futures; the pool size was never the cause.)
pub const WEB_TASK_POOL_SIZE: usize = 3;

// keep_connection_alive: without it every preview poll (15/s) pays a full
// TCP open/close on a chip with a 2-connection pool — the browser reuses
// one connection instead.
// keep-alive + a generous mid-request read timeout: OTA uploads over a busy
// WiFi link can see multi-second gaps (the default 3 s aborted real pushes).
static CONFIG: picoserve::Config = picoserve::Config {
    timeouts: picoserve::Timeouts {
        start_read_request: picoserve::time::Duration::from_secs(5),
        persistent_start_read_request: picoserve::time::Duration::from_secs(1),
        // one timer for an ENTIRE request body, not per-read — must cover a
        // full OTA upload on a slow link
        read_request: picoserve::time::Duration::from_secs(300),
        write: picoserve::time::Duration::from_secs(5),
    },
    connection: picoserve::KeepAlive::KeepAlive,
};

#[embassy_executor::task(pool_size = WEB_TASK_POOL_SIZE)]
pub async fn web_task(task_id: usize, stack: Stack<'static>) -> ! {
    // Pattern uploads arrive in the http buffer; size it for real-world
    // .epe sources (the corpus tops out around 16 KB). Heap-allocated so
    // the task future (statically allocated per pool slot) stays small.
    let mut tcp_rx_buffer = alloc::vec![0u8; 4096];
    let mut tcp_tx_buffer = alloc::vec![0u8; 4096];
    let mut http_buffer = alloc::vec![0u8; 16 * 1024];

    let app = make_app();
    picoserve::Server::new(&app, &CONFIG, &mut http_buffer)
        .listen_and_serve(task_id, stack, 80, &mut tcp_rx_buffer, &mut tcp_tx_buffer)
        .await
        .into_never()
}
