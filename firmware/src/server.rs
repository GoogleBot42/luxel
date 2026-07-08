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
//!   GET  /api/brightness {"brightness":0..31,"max":31}
//!   POST /api/brightness body = "0".."31" → applied live + persisted {"ok":true,"brightness":N}
//!   GET  /api/config    {"pixels":N,"max":2048,"protocol":"sk9822"}
//!   POST /api/config    body = pixel count → live resize + persisted {"ok":true,"pixels":N}
//!   GET  /api/protocol  {"protocol":"sk9822","options":["sk9822","ws2812"]}
//!   POST /api/protocol  body = name → live SPI reconfig + persisted {"ok":true,"protocol":"…"}
//!   GET  /api/playlist  {"defaultSec":N,"playing":bool,"index":N,"items":[{"id","name","sec","controls"}]}
//!   POST /api/playlist  body = D/I/C lines → stores + applies live
//!   POST /api/playlist/{play,stop,next,prev}  play body = start index
//!   GET  /api/map       {"installed":bool,"dims":D,"count":N}
//!   POST /api/map       body = "<dims> <raw...>" → install (empty = clear); persisted
//!   GET    /api/patterns              {"patterns":[{"id","name"},…]}
//!   GET    /api/patterns/<id>         {"id","name","source"}
//!   POST   /api/patterns              body "name\nsource" → {"ok":true,"id"}
//!   DELETE /api/patterns/<id>         {"ok":true}
//!   POST   /api/patterns/<id>/activate  runs it → {"ok":true} | code-error shape

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use core::sync::atomic::Ordering;

use embassy_net::Stack;
use luxel_core::fixed::Fx;
use luxel_core::jsonview::json_escape;
use picoserve::routing::RequestHandlerService as _;

use crate::shared::{
    get_pattern_src, get_pixels, get_vmerr, snapshot, Msg, CONTROLS_JSON, FPS, MSG_QUEUE,
    READOUTS_JSON, VARS_JSON,
};
use crate::config::DeviceConfig;
use crate::leds::Protocol;
use crate::shared::{BRIGHTNESS, MAX_PIXELS, PIXEL_COUNT, PROTOCOL};

const INDEX_HTML: &str = include_str!("index.html");

const CORS: (&str, &str) = ("Access-Control-Allow-Origin", "*");
const JSON: (&str, &str) = ("Content-Type", "application/json");

type ApiResponse = ((&'static str, &'static str), (&'static str, &'static str), String);

fn json_response(body: String) -> ApiResponse {
    (CORS, JSON, body)
}

/// Cache policy for a flash asset. Content-hashed bundle files
/// (`/assets/index-<hash>.js`) can never change under their URL, so cache
/// them hard and skip revalidation entirely; everything else (index.html,
/// luxel.wasm, gallery.json) must revalidate — the ETag then yields 304s.
fn asset_cache_control(path: &str) -> (&'static str, &'static str) {
    if path.starts_with("/assets/") {
        ("Cache-Control", "public, max-age=31536000, immutable")
    } else {
        ("Cache-Control", "no-cache")
    }
}

/// True if one `If-None-Match` token matches our (quoted) ETag. Tolerates
/// surrounding whitespace, a `W/` weak-validator prefix, and `*`.
fn etag_matches(token: &[u8], etag: &[u8]) -> bool {
    let t = token.trim_ascii();
    if t.len() == 1 && t[0] == b'*' {
        return true;
    }
    let t = t.strip_prefix(b"W/").map(|w| w.trim_ascii()).unwrap_or(t);
    t == etag
}

fn sync_mode_name(m: u8) -> &'static str {
    match m {
        1 => "leader",
        2 => "follower",
        _ => "off",
    }
}

fn status_json() -> String {
    let fps = FPS.load(Ordering::Relaxed);
    let pixels = PIXEL_COUNT.load(Ordering::Relaxed);
    let slot = crate::ota::booted_slot();
    let version = env!("CARGO_PKG_VERSION");
    let heap = esp_alloc::HEAP.free();
    let live = match crate::shared::live_proto(embassy_time::Instant::now().as_millis() as u32) {
        Some(p) => format!("\"{p}\""),
        None => String::from("null"),
    };
    match get_vmerr() {
        Some(e) => format!(
            "{{\"fps\":{},\"pixels\":{},\"slot\":\"{}\",\"version\":\"{}\",\"heap_free\":{},\"live\":{},\"vmerr\":\"{}\"}}",
            fps,
            pixels,
            slot,
            version,
            heap,
            live,
            json_escape(&e)
        ),
        None => format!(
            "{{\"fps\":{},\"pixels\":{},\"slot\":\"{}\",\"version\":\"{}\",\"heap_free\":{},\"live\":{},\"vmerr\":null}}",
            fps, pixels, slot, version, heap, live
        ),
    }
}

async fn api_status() -> ApiResponse {
    json_response(status_json())
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
            let body = request.body_connection.body();
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

/// Streams a pattern upload (an LXP1 envelope: name + source + LXBC) into
/// an exact-size heap Vec, then validates and dispatches it — `/api/code`
/// runs it, `/api/patterns` stores it. Streaming keeps the per-connection
/// HTTP buffer small (4 KB — big uploads used to dictate 24 KB for every
/// connection) and removes the upload-size cap: the only limit is what
/// actually fits free heap, reserved fallibly. If the running pattern owns
/// too much heap for the buffer, the engine is frozen (its heap freed) and
/// the reservation retried — the upload that follows revives rendering
/// anyway.
struct PatternService {
    save: bool,
}

impl<State, PathParameters> picoserve::routing::RequestHandlerService<State, PathParameters>
    for PatternService
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

        const MAX_UPLOAD: usize = 80 * 1024;

        let body = 'resp: {
            let body = request.body_connection.body();
            let expected = body.content_length();
            if expected == 0 {
                break 'resp String::from(
                    "{\"ok\":false,\"error\":\"empty upload (the request needs a Content-Length body)\"}",
                );
            }
            if expected > MAX_UPLOAD {
                break 'resp format!(
                    "{{\"ok\":false,\"error\":\"pattern upload too large ({} KB; this device accepts up to {} KB)\"}}",
                    expected / 1024,
                    MAX_UPLOAD / 1024
                );
            }
            let mut env: Vec<u8> = Vec::new();
            if env.try_reserve_exact(expected).is_err() {
                // the running pattern owns the heap — freeze it (frees its
                // program + arrays; the strip holds its last frame) and
                // retry once the render task has drained the message
                MSG_QUEUE.send(Msg::Freeze).await;
                embassy_time::Timer::after(embassy_time::Duration::from_millis(60)).await;
                if env.try_reserve_exact(expected).is_err() {
                    break 'resp format!(
                        "{{\"ok\":false,\"error\":\"not enough free memory on the device for this {} KB upload (about {} KB free) — it is too large to run here\"}}",
                        expected / 1024,
                        esp_alloc::HEAP.free() as usize / 1024
                    );
                }
            }
            env.resize(expected, 0);
            let mut reader = body.reader();
            let mut fill = 0usize;
            while fill < expected {
                match reader.read(&mut env[fill..]).await {
                    Ok(0) => break,
                    Ok(n) => fill += n,
                    Err(_) => break,
                }
            }
            if fill != expected {
                break 'resp String::from("{\"ok\":false,\"error\":\"upload truncated\"}");
            }
            if self.save {
                api_patterns_save(&env)
            } else {
                api_code(env).await
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

        // free the engine's heap for the flash phase — a reboot follows a
        // successful OTA anyway, and the strip just holds its last frame
        MSG_QUEUE.send(Msg::Freeze).await;

        let result: Result<u32, &'static str> = {
            // NOTE: picoserve's read_request timeout is one timer for the
            // WHOLE body (created when the reader is taken), not per read —
            // take the reader only after the erase phase so slow uploads get
            // the full budget.
            let body = request.body_connection.body();
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

async fn api_pattern() -> ApiResponse {
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

/// Decode an LXP1 envelope and validate its bytecode blob. Compilation
/// happens in the browser/CLI — the device only checks that the blob
/// decodes for this firmware's format version. A version mismatch gets
/// `"code":"bc-version"` so clients know to recompile from source.
fn decode_upload(raw: &[u8]) -> Result<luxel_core::bytecode::Envelope<'_>, String> {
    use luxel_core::bytecode::{decode_envelope, validate, BcError};
    let env = decode_envelope(raw)
        .map_err(|e| format!("{{\"ok\":false,\"error\":\"{}\"}}", json_escape(&e.to_string())))?;
    // validate() checks everything deserialize() would but allocates nothing —
    // the full Program is only ever materialized once, by the render task
    // (building it here too OOM'd the heap under soak-speed pattern churn).
    match validate(env.bytecode) {
        Ok(_) => Ok(env),
        Err(e @ BcError::Version { .. }) => Err(format!(
            "{{\"ok\":false,\"code\":\"bc-version\",\"error\":\"{}\"}}",
            json_escape(&e.to_string())
        )),
        Err(e) => Err(format!(
            "{{\"ok\":false,\"error\":\"{}\"}}",
            json_escape(&e.to_string())
        )),
    }
}

/// POST /api/code — LXP1 envelope (empty name): run this pattern now.
/// Takes the body by VALUE and forwards it unparsed: this handler must not
/// allocate source/blob copies — while a heavy pattern owns the heap those
/// copies OOM'd (soak v5). The render task frees the old engine first,
/// then parses.
async fn api_code(raw: Vec<u8>) -> String {
    match decode_upload(&raw) {
        Ok(_) => {
            crate::playlist::stop(); // a manual push takes over from the playlist
            MSG_QUEUE.send(Msg::Code { env: raw }).await;
            crate::shared::set_current_pattern_id(""); // ad-hoc code, no library id
            String::from("{\"ok\":true}")
        }
        Err(e) => e,
    }
}

/// POST /api/patterns — LXP1 envelope (name + source + bytecode) → persist
/// via the pattern library. The blob is decode-validated so the store never
/// holds bytecode this firmware can't run. Mirrors serve.rs `patterns_save`.
fn api_patterns_save(raw: &[u8]) -> String {
    match decode_upload(raw) {
        Ok(env) if env.name.is_empty() => {
            String::from("{\"ok\":false,\"error\":\"pattern name required\"}")
        }
        Ok(env) => {
            let r = crate::patterns::save(env.name, env.source, env.bytecode);
            // content changed — re-validate any playlist entries using it
            crate::playlist::preflight_mark_dirty();
            r
        }
        Err(e) => e,
    }
}

/// POST /api/patterns/<id>/activate — load the stored bytecode and run it
/// (same swap path as /api/code). A stored blob that no longer decodes
/// (format bump via OTA) reports `bc-version` so the client re-saves.
async fn api_patterns_activate(id: &str) -> String {
    use luxel_core::bytecode::{validate, BcError};
    let Some(bc) = crate::patterns::bytecode_of(id) else {
        return String::from("{\"ok\":false,\"error\":\"no such pattern\"}");
    };
    let Some(source) = crate::patterns::source_of(id) else {
        return String::from("{\"ok\":false,\"error\":\"no such pattern\"}");
    };
    match validate(&bc) {
        Ok(_) => {
            let env = luxel_core::bytecode::encode_envelope("", &source, &bc);
            drop((source, bc));
            MSG_QUEUE.send(Msg::Code { env }).await;
            crate::shared::set_current_pattern_id(id);
            String::from("{\"ok\":true}")
        }
        Err(e @ BcError::Version { .. }) => format!(
            "{{\"ok\":false,\"code\":\"bc-version\",\"error\":\"{}\"}}",
            json_escape(&e.to_string())
        ),
        Err(e) => format!(
            "{{\"ok\":false,\"error\":\"{}\"}}",
            json_escape(&e.to_string())
        ),
    }
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
                // the old /ws incident: stack-guard overflow)
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
                // pattern uploads (LXP1 envelopes) stream into an exact-size
                // heap Vec — no resident big HTTP buffer, no upload cap
                // beyond what actually fits free heap
                "/api/code" => {
                    return alloc::boxed::Box::pin(
                        PatternService { save: false }.call_request_handler_service(
                            state,
                            path_parameters,
                            request,
                            response_writer,
                        ),
                    )
                    .await;
                }
                "/api/patterns" => {
                    return alloc::boxed::Box::pin(
                        PatternService { save: true }.call_request_handler_service(
                            state,
                            path_parameters,
                            request,
                            response_writer,
                        ),
                    )
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
                // any body → set the one-shot force-AP flag and reboot into
                // the provisioning access point ("luxel-xxxx" @ 192.168.4.1)
                "/api/apmode" => {
                    crate::ota::set_force_ap();
                    let response = json_response(String::from(
                        "{\"ok\":true,\"note\":\"rebooting into the setup AP (one boot only)\"}",
                    ));
                    let conn = request.body_connection.finalize().await?;
                    let sent = response.write_to(conn, response_writer).await?;
                    crate::REBOOT.signal(());
                    return Ok(sent);
                }
                _ => {}
            }
            // Every other POST reads its body, builds ONE json ApiResponse,
            // and leaves through a single write at the bottom. Inlining a
            // finalize+write_to future per route made this poll frame the
            // largest symbol in the image (see docs/size-report.md).
            let raw: alloc::vec::Vec<u8> = match request.body_connection.body().read_all().await {
                // fallible copy: on a starved heap a failed POST body beats
                // an OOM panic — the route then rejects it cleanly
                Ok(b) => {
                    let mut v = alloc::vec::Vec::new();
                    if v.try_reserve_exact(b.len()).is_ok() {
                        v.extend_from_slice(b);
                    }
                    v
                }
                Err(_) => alloc::vec::Vec::new(),
            };
            let text = |r: &[u8]| String::from_utf8_lossy(r).into_owned();
            let api: Option<ApiResponse> = match route {
                // body: tz offset from UTC in minutes (e.g. "-360") →
                // applied live + persisted (clock builtins shift with it)
                "/api/clock" => {
                    Some(json_response(match text(&raw).trim().parse::<i16>() {
                        Ok(tz) if (-14 * 60..=14 * 60).contains(&(tz as i32)) => {
                            crate::shared::TZ_MINUTES.store(tz as i32, Ordering::Relaxed);
                            let cfg = DeviceConfig { tz_minutes: tz, ..crate::shared::device_config_snapshot() };
                            let _ = crate::config::write_device(&cfg);
                            format!("{{\"ok\":true,\"tzMinutes\":{}}}", tz)
                        }
                        _ => String::from(
                            "{\"ok\":false,\"error\":\"tz must be minutes in -840..=840\"}",
                        ),
                    }))
                }
                // body: "<order> <gamma_tenths> <cap_ma>" (e.g. "grb 22 1500")
                // → the output pipeline, applied live + persisted
                "/api/output" => {
                    let body = text(&raw);
                    let mut it = body.split_whitespace();
                    let order = it
                        .next()
                        .and_then(luxel_core::outpipe::ColorOrder::from_name);
                    let gamma: Option<u8> =
                        it.next().and_then(|v| v.parse().ok()).filter(|g| *g <= 50);
                    let cap: Option<u16> =
                        it.next().and_then(|v| v.parse().ok()).filter(|c| *c <= 20_000);
                    Some(json_response(match (order, gamma, cap) {
                        (Some(o), Some(g), Some(c)) => {
                            crate::shared::COLOR_ORDER.store(o.0, Ordering::Relaxed);
                            crate::shared::GAMMA_TENTHS.store(g, Ordering::Relaxed);
                            crate::shared::CAP_MA.store(c as u32, Ordering::Relaxed);
                            let _ = crate::config::write_device(
                                &crate::shared::device_config_snapshot(),
                            );
                            format!(
                                "{{\"ok\":true,\"order\":\"{}\",\"gamma\":{},\"capMa\":{}}}",
                                o.name(),
                                g,
                                c
                            )
                        }
                        _ => String::from(
                            "{\"ok\":false,\"error\":\"expected: <rgb|rbg|grb|gbr|brg|bgr> <gamma_tenths 0-50> <cap_ma 0-20000>\"}",
                        ),
                    }))
                }
                // body: "off" | "leader" | "follower" → applied live +
                // persisted (Luxel-to-Luxel sync role)
                "/api/sync" => {
                    let body = text(&raw);
                    let mode = match body.trim() {
                        "off" => Some(0u8),
                        "leader" => Some(1),
                        "follower" => Some(2),
                        _ => None,
                    };
                    Some(json_response(match mode {
                        Some(m) => {
                            crate::shared::SYNC_MODE.store(m, Ordering::Relaxed);
                            if m != 2 {
                                crate::shared::clear_sync_leader();
                            }
                            let cfg = DeviceConfig { sync_mode: m, ..crate::shared::device_config_snapshot() };
                            let _ = crate::config::write_device(&cfg);
                            format!("{{\"ok\":true,\"mode\":\"{}\"}}", sync_mode_name(m))
                        }
                        None => String::from(
                            "{\"ok\":false,\"error\":\"mode must be off, leader, or follower\"}",
                        ),
                    }))
                }
                // binary body: one raw sensor-board frame ("SB1.0\0"…"END\0")
                // — network sensor injection, byte-identical to the serial
                // board's stream (luxel_core::netin::parse_sensor_board).
                "/api/sensors" => {
                    Some(json_response(match luxel_core::netin::parse_sensor_board(&raw) {
                        Some(s) => {
                            crate::shared::set_sensor_frame(s);
                            String::from("{\"ok\":true}")
                        }
                        None => {
                            String::from("{\"ok\":false,\"error\":\"not a sensor-board frame\"}")
                        }
                    }))
                }
                // body: "host\nport\nuser\npass" → stored in flash; the MQTT
                // task reconnects live (no reboot). Empty host disables MQTT.
                "/api/mqtt" => {
                    let body = text(&raw);
                    let mut lines = body.lines();
                    let host = lines.next().unwrap_or("").trim();
                    let port = lines.next().unwrap_or("").trim().parse::<u16>().unwrap_or(1883);
                    let user = lines.next().unwrap_or("").trim();
                    let pass = lines.next().unwrap_or("").trim();
                    let result = if host.is_empty() {
                        crate::config::write_mqtt(None)
                    } else {
                        crate::config::write_mqtt(Some(&crate::config::MqttConfig {
                            host: String::from(host),
                            port: if port == 0 { 1883 } else { port },
                            user: String::from(user),
                            pass: String::from(pass),
                        }))
                    };
                    Some(json_response(match &result {
                        Ok(()) => {
                            crate::shared::MQTT_POKE.signal(());
                            format!(
                                "{{\"ok\":true,\"enabled\":{}}}",
                                if host.is_empty() { "false" } else { "true" }
                            )
                        }
                        Err(e) => format!("{{\"ok\":false,\"error\":\"{}\"}}", json_escape(e)),
                    }))
                }
                // POST /api/brightness — body is a number 0..=31. Applied live
                // (the render task reads BRIGHTNESS every frame) and persisted
                // to flash so it survives reboot. No reboot needed.
                "/api/brightness" => {
                    Some(json_response(match text(&raw).trim().parse::<u8>() {
                        Ok(b) if b <= 31 => {
                            BRIGHTNESS.store(b, Ordering::Relaxed);
                            // read-modify-write so we don't clobber the others
                            let cfg = DeviceConfig { brightness: b, ..crate::shared::device_config_snapshot() };
                            match crate::config::write_device(&cfg) {
                                Ok(()) => format!("{{\"ok\":true,\"brightness\":{}}}", b),
                                // applied live even if the flash write failed
                                Err(e) => format!(
                                    "{{\"ok\":true,\"brightness\":{},\"note\":\"not persisted: {}\"}}",
                                    b,
                                    json_escape(e)
                                ),
                            }
                        }
                        _ => String::from("{\"ok\":false,\"error\":\"brightness must be 0..=31\"}"),
                    }))
                }
                // POST /api/config — body is a pixel count 1..=MAX_PIXELS.
                // Applied live (render task rebuilds the engine + SPI buffer)
                // and persisted. No reboot.
                "/api/config" => {
                    Some(json_response(match text(&raw).trim().parse::<u32>() {
                        Ok(n) if n >= 1 && n <= MAX_PIXELS => {
                            // the render task is the sole writer of PIXEL_COUNT;
                            // it flips the atomic + rebuilds when it drains this
                            MSG_QUEUE.send(Msg::Config(n)).await;
                            let cfg = DeviceConfig { pixel_count: n, ..crate::shared::device_config_snapshot() };
                            match crate::config::write_device(&cfg) {
                                Ok(()) => format!("{{\"ok\":true,\"pixels\":{}}}", n),
                                Err(e) => format!(
                                    "{{\"ok\":true,\"pixels\":{},\"note\":\"not persisted: {}\"}}",
                                    n,
                                    json_escape(e)
                                ),
                            }
                        }
                        _ => format!(
                            "{{\"ok\":false,\"error\":\"pixels must be 1..={}\"}}",
                            MAX_PIXELS
                        ),
                    }))
                }
                // POST /api/protocol — body is a protocol name (sk9822/ws2812
                // + aliases). Reconfigures SPI + resizes the buffer live, and
                // persists. No reboot.
                "/api/protocol" => {
                    Some(json_response(match Protocol::from_name(text(&raw).trim()) {
                        Some(p) => {
                            MSG_QUEUE.send(Msg::Protocol(p.as_u8())).await;
                            let cfg = DeviceConfig { protocol: p.as_u8(), ..crate::shared::device_config_snapshot() };
                            match crate::config::write_device(&cfg) {
                                Ok(()) => format!("{{\"ok\":true,\"protocol\":\"{}\"}}", p.name()),
                                Err(e) => format!(
                                    "{{\"ok\":true,\"protocol\":\"{}\",\"note\":\"not persisted: {}\"}}",
                                    p.name(),
                                    json_escape(e)
                                ),
                            }
                        }
                        None => {
                            String::from("{\"ok\":false,\"error\":\"protocol must be sk9822 or ws2812\"}")
                        }
                    }))
                }
                // POST /api/map — install a computed 2D/3D map (raw 16.16).
                // Empty/invalid body clears it. Applied live + persisted.
                "/api/map" => {
                    let (installed, count) = crate::devicemap::set_from_wire(&text(&raw));
                    Some(json_response(format!(
                        "{{\"ok\":true,\"installed\":{},\"count\":{}}}",
                        installed, count
                    )))
                }
                // POST /api/playlist — line-format definition (D/I/C lines).
                // Persisted to flash; applied live if already playing.
                "/api/playlist" => {
                    crate::playlist::set_from_wire(&text(&raw));
                    Some(json_response(String::from("{\"ok\":true}")))
                }
                "/api/playlist/play"
                | "/api/playlist/stop"
                | "/api/playlist/next"
                | "/api/playlist/prev" => {
                    match route {
                        "/api/playlist/play" => {
                            crate::playlist::play(text(&raw).trim().parse().unwrap_or(0))
                        }
                        "/api/playlist/stop" => crate::playlist::stop(),
                        "/api/playlist/next" => crate::playlist::step(1),
                        _ => crate::playlist::step(-1),
                    }
                    Some(json_response(String::from("{\"ok\":true}")))
                }
                // (/api/code and /api/patterns stream their own bodies via
                // PatternService — dispatched before the body is read)
                "/api/control" => Some(api_control(text(&raw)).await),
                "/api/var" => Some(api_var(text(&raw)).await),
                // POST /api/patterns/<id>/activate — run a stored pattern
                r if r.starts_with("/api/patterns/") => {
                    Some(match r["/api/patterns/".len()..].strip_suffix("/activate") {
                        Some(id) => json_response(api_patterns_activate(id).await),
                        None => json_response(String::from(
                            "{\"ok\":false,\"error\":\"bad patterns route\"}",
                        )),
                    })
                }
                _ => None,
            };
            if let Some(response) = api {
                let conn = request.body_connection.finalize().await?;
                return response.write_to(conn, response_writer).await;
            }
        } else if method.eq_ignore_ascii_case("DELETE") {
            if let Some(id) = route.strip_prefix("/api/patterns/") {
                crate::playlist::preflight_mark_dirty();
                let response = json_response(crate::patterns::delete(id));
                let conn = request.body_connection.finalize().await?;
                return response.write_to(conn, response_writer).await;
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
            // Serve a flash asset with a strong ETag + Cache-Control, and a
            // 304 shortcut when the client's If-None-Match still matches — so
            // an unchanged asset is never re-downloaded.
            macro_rules! serve_asset {
                ($e:expr) => {{
                    let e = $e;
                    let cc = asset_cache_control(&e.path);
                    let matched = !e.etag.is_empty()
                        && request
                            .parts
                            .headers()
                            .get("If-None-Match")
                            .is_some_and(|inm| {
                                inm.split(b',').any(|t| etag_matches(t.as_raw(), e.etag.as_bytes()))
                            });
                    if matched {
                        respond!((
                            StatusCode::NOT_MODIFIED,
                            ("ETag", e.etag),
                            cc,
                            picoserve::response::NoContent,
                        ));
                    }
                    if e.etag.is_empty() {
                        // legacy "LUXA" archive without hashes — still cacheable
                        if e.gzip {
                            respond!((("Content-Encoding", "gzip"), cc, FlashAsset(e)));
                        }
                        respond!((cc, FlashAsset(e)));
                    }
                    let etag = ("ETag", e.etag.clone());
                    if e.gzip {
                        respond!((("Content-Encoding", "gzip"), etag, cc, FlashAsset(e)));
                    }
                    respond!((etag, cc, FlashAsset(e)));
                }};
            }
            // JSON arms collect into ONE ApiResponse and exit through a
            // single write below; assets / binary / redirects keep their own
            // early returns (respond!). Same shape as the POST section — it
            // keeps a write_to future per route out of this poll frame.
            let api: Option<ApiResponse> = match route {
                // "/" serves the installed playground when present; the
                // embedded minimal page is the fallback (and stays reachable
                // at /min for bring-up debugging)
                "/" => {
                    if let Some(e) = crate::assets::lookup("/index.html") {
                        serve_asset!(e);
                    }
                    respond!((("Content-Type", "text/html; charset=utf-8"), INDEX_HTML));
                }
                "/min" => respond!((("Content-Type", "text/html; charset=utf-8"), INDEX_HTML)),
                "/api/status" => Some(api_status().await),
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
                    Some(json_response(body))
                }
                "/api/brightness" => Some(json_response(format!(
                    "{{\"brightness\":{},\"max\":31}}",
                    BRIGHTNESS.load(Ordering::Relaxed)
                ))),
                // whether we're currently the provisioning AP
                "/api/apmode" => Some(json_response(format!(
                    "{{\"ap\":{}}}",
                    crate::provision::AP_MODE.load(Ordering::Relaxed)
                ))),
                // output pipeline settings
                "/api/output" => Some(json_response(format!(
                    "{{\"order\":\"{}\",\"gamma\":{},\"capMa\":{}}}",
                    luxel_core::outpipe::ColorOrder(
                        crate::shared::COLOR_ORDER.load(Ordering::Relaxed)
                    )
                    .name(),
                    crate::shared::GAMMA_TENTHS.load(Ordering::Relaxed),
                    crate::shared::CAP_MA.load(Ordering::Relaxed)
                ))),
                // wall clock: NTP-synced local time + tz (clock builtins)
                "/api/clock" => {
                    let local = crate::shared::wall_now_local();
                    Some(json_response(format!(
                        "{{\"synced\":{},\"local\":{},\"tzMinutes\":{}}}",
                        local.is_some(),
                        local.unwrap_or(0),
                        crate::shared::TZ_MINUTES.load(Ordering::Relaxed)
                    )))
                }
                // sync role + engine clock + last leader beacon heard
                "/api/sync" => {
                    let mode = sync_mode_name(crate::shared::SYNC_MODE.load(Ordering::Relaxed));
                    let time_ms = crate::shared::engine_time_ms();
                    let leader = match crate::shared::sync_leader() {
                        Some((boot, lt, at)) => {
                            let age = at.elapsed().as_millis();
                            let offset = (lt + age) as i64 - time_ms as i64;
                            format!(
                                "{{\"bootId\":{},\"ageMs\":{},\"offsetMs\":{}}}",
                                boot, age, offset
                            )
                        }
                        None => String::from("null"),
                    };
                    Some(json_response(format!(
                        "{{\"mode\":\"{}\",\"timeMs\":{},\"leader\":{}}}",
                        mode, time_ms, leader
                    )))
                }
                // broker settings (never the password) + connection state
                "/api/mqtt" => {
                    let body = match crate::config::read_mqtt() {
                        Some(c) => format!(
                            "{{\"enabled\":true,\"host\":\"{}\",\"port\":{},\"user\":\"{}\",\"hasPass\":{},\"connected\":{}}}",
                            json_escape(&c.host),
                            c.port,
                            json_escape(&c.user),
                            !c.pass.is_empty(),
                            crate::mqtt::CONNECTED.load(Ordering::Relaxed)
                        ),
                        None => String::from(
                            "{\"enabled\":false,\"host\":\"\",\"port\":1883,\"user\":\"\",\"hasPass\":false,\"connected\":false}",
                        ),
                    };
                    Some(json_response(body))
                }
                "/api/config" => Some(json_response(format!(
                    "{{\"pixels\":{},\"max\":{},\"protocol\":\"{}\"}}",
                    PIXEL_COUNT.load(Ordering::Relaxed),
                    MAX_PIXELS,
                    Protocol::from_u8(PROTOCOL.load(Ordering::Relaxed)).name()
                ))),
                "/api/playlist" => Some(json_response(crate::playlist::to_json())),
                "/api/map" => Some(json_response(crate::devicemap::to_json())),
                "/api/protocol" => Some(json_response(format!(
                    "{{\"protocol\":\"{}\",\"options\":[\"sk9822\",\"ws2812\"]}}",
                    Protocol::from_u8(PROTOCOL.load(Ordering::Relaxed)).name()
                ))),
                "/api/pixels" => respond!(api_pixels().await),
                "/api/pattern" => Some(api_pattern().await),
                // running pattern as an LXP1 envelope (source + bytecode) —
                // what a sync follower adopts (it has no compiler)
                "/api/pattern.lxp" => respond!((
                    CORS,
                    ("Content-Type", "application/octet-stream"),
                    luxel_core::bytecode::encode_envelope(
                        "",
                        &get_pattern_src(),
                        &crate::shared::get_pattern_bc(),
                    ),
                )),
                "/api/controls" => Some(api_controls().await),
                "/api/vars" => Some(api_vars().await),
                "/api/readouts" => Some(api_readouts().await),
                "/api/patterns" => Some(json_response(crate::patterns::list_json())),
                // GET /api/patterns/<id> → {"id","name","source"}; missing id
                // returns 200 + {"ok":false,…} to match the mirror (serve.rs).
                r if r.starts_with("/api/patterns/") => {
                    let j = crate::patterns::get_json(&r["/api/patterns/".len()..])
                        .unwrap_or_else(|| String::from("{\"ok\":false,\"error\":\"no such pattern\"}"));
                    Some(json_response(j))
                }
                other => {
                    if let Some(e) = crate::assets::lookup(other) {
                        serve_asset!(e);
                    }
                    // captive-portal detection: as a provisioning AP, any
                    // unknown URL (a phone's connectivity probe) redirects
                    // to the portal, which pops the sign-in sheet
                    if crate::provision::AP_MODE.load(Ordering::Relaxed) {
                        let conn = request.body_connection.finalize().await?;
                        return (
                            StatusCode::TEMPORARY_REDIRECT,
                            [
                                ("Location", "http://192.168.4.1/"),
                                ("Cache-Control", "no-store"),
                            ],
                            "redirecting to setup",
                        )
                            .write_to(conn, response_writer)
                            .await;
                    }
                    None
                }
            };
            if let Some(response) = api {
                let conn = request.body_connection.finalize().await?;
                return response.write_to(conn, response_writer).await;
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

// 2: the third slot existed for the bidirectional preview WEBSOCKET, which
// the size diet removed (the preview polls /api/pixels over keep-alive
// now). Each slot costs 32 KB of heap in connection buffers — on a device
// where patterns compete for ~50 KB, the idle third slot was a quarter of
// the pattern budget.
pub const WEB_TASK_POOL_SIZE: usize = 2;

// keep_connection_alive: without it every preview poll (15/s) pays a full
// TCP open/close on a chip with a 2-connection pool — the browser reuses
// one connection instead.
// keep-alive + a generous mid-request read timeout: OTA uploads over a busy
// WiFi link can see multi-second gaps (the default 3 s aborted real pushes).
static CONFIG: picoserve::Config = picoserve::Config {
    timeouts: picoserve::Timeouts {
        start_read_request: picoserve::time::Duration::from_secs(5),
        persistent_start_read_request: picoserve::time::Duration::from_secs(1),
        // One timer for an ENTIRE request body, not per-read — must cover a
        // full OTA upload on a slow link (~20s at 55 KB/s + erase pauses).
        // But NOT much more: an abandoned upload (client timed out mid-body)
        // pins one of only THREE server sockets until this expires — at the
        // original 300s a couple of WiFi hiccups cascaded into minutes-long
        // outages (the hw-bench soak lost ~60 requests to exactly this).
        read_request: picoserve::time::Duration::from_secs(45),
        write: picoserve::time::Duration::from_secs(5),
    },
    connection: picoserve::KeepAlive::KeepAlive,
};

#[embassy_executor::task(pool_size = WEB_TASK_POOL_SIZE)]
pub async fn web_task(task_id: usize, stack: Stack<'static>) -> ! {
    // Only the TCP rx/tx buffers persist (they must exist to accept). The
    // HTTP buffer is allocated per CONNECTION and freed at close, and it's
    // small: request lines + headers + text bodies. Everything big streams
    // past it — OTA images, asset archives, and pattern uploads all read
    // their bodies in chunks (PatternService/OtaService/AssetsService), so
    // no connection ever needs a body-sized buffer. If even 4 KB can't be
    // allocated, the connection is turned away with a 503 instead of an
    // alloc panic.
    let mut tcp_rx_buffer = alloc::vec![0u8; 4096];
    let mut tcp_tx_buffer = alloc::vec![0u8; 4096];

    let app = make_app();
    loop {
        let mut socket =
            embassy_net::tcp::TcpSocket::new(stack, &mut tcp_rx_buffer, &mut tcp_tx_buffer);
        if socket.accept(80).await.is_err() {
            continue;
        }
        // same knobs picoserve's own accept loop sets
        socket.set_keep_alive(Some(embassy_time::Duration::from_secs(30)));
        socket.set_timeout(Some(embassy_time::Duration::from_secs(45)));

        let mut http_buffer: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
        if http_buffer.try_reserve_exact(4 * 1024).is_err() {
            let mut msg: &[u8] =
                b"HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
            while !msg.is_empty() {
                match socket.write(msg).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => msg = &msg[n..],
                }
            }
            let _ = socket.flush().await;
            socket.close();
            esp_println::println!("http[{}]: 503 — no heap for a connection buffer", task_id);
            continue;
        }
        http_buffer.resize(4 * 1024, 0);

        let _ = picoserve::Server::new(&app, &CONFIG, &mut http_buffer)
            .serve(socket)
            .await;
        // http_buffer freed here
    }
}
