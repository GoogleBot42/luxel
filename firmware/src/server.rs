//! HTTP server: serves the playground web app and the device API. Keep routes
//! and response shapes in lockstep with the native mirror
//! (crates/luxel-cli/src/serve.rs) — the playground's device mode talks to
//! both interchangeably.
//!
//! API (all responses carry Access-Control-Allow-Origin: * so the
//! playground dev server can target a device directly):
//!   GET  /              installed playground, else the minimal page (also /min)
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
//!   GET  /api/output    {"order","gamma","capMa","brightCurve","blur","glow",
//!                        "palette":[pos,r,g,b,…],"paletteAmount":0..100}
//!   POST /api/output    body = "<order> <gamma_tenths> <cap_ma> [<bright_curve_tenths> <blur_pct> <glow_pct>]"
//!   POST   /api/output/palette  body = "<amount_pct> <pos> <r> <g> <b> …" (0..=255 each)
//!   DELETE /api/output/palette  clears the device palette → {"ok":true}
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
//!   POST /api/events    body = "EV1\0" frame (netin::parse_events) → queues
//!                       [type,x,y,value] events for readEvent() patterns

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use core::sync::atomic::Ordering;

use embassy_net::Stack;
use luxel_core::fixed::Fx;
use luxel_core::jsonview::{json_escape, push_i32, push_i64, push_piece, push_u32, push_u64};
use picoserve::response::{Content, StatusCode};
use picoserve::routing::RequestHandlerService as _;

use crate::shared::{
    get_pixels, get_vmerr, snapshot, Msg, CONTROLS_JSON, FPS, MSG_QUEUE, READOUTS_JSON, VARS_JSON,
};
use crate::config::DeviceConfig;
use crate::leds::Protocol;
use crate::shared::{BRIGHTNESS, MAX_PIXELS, PIXEL_COUNT, PROTOCOL};

const INDEX_HTML: &str = include_str!(concat!(env!("OUT_DIR"), "/index.html"));

const TEXT_PLAIN: &str = "text/plain; charset=utf-8";
const TEXT_HTML: &str = "text/html; charset=utf-8";

/// The ONE header-value type in the image.
///
/// `picoserve::response::ForEachHeader::call<Value: Display>` is generic over
/// the value type, so every distinct `V` handed to a header instantiates a
/// fresh copy of picoserve's header-writing machinery — and every distinct
/// response *tuple shape* instantiated a whole `IntoResponse::write_to`
/// besides (22 of them, 20.5 KB, before Gitea #167). Collapsing all values to
/// this enum, and all responses to [Reply], leaves exactly one of each.
enum HVal {
    Static(&'static str),
    /// Only the asset ETag needs a runtime-built header value; a `hosted-ui`
    /// image serves no assets, so the variant (and `String`'s Display path)
    /// go with it.
    #[cfg(not(feature = "hosted-ui"))]
    Owned(String),
}

impl core::fmt::Display for HVal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // write_str ONLY — never `write!`/format args: an Arguments build here
        // would undo the fmt diet (Gitea #168) on the hottest path we have.
        f.write_str(match self {
            HVal::Static(s) => s,
            #[cfg(not(feature = "hosted-ui"))]
            HVal::Owned(s) => s.as_str(),
        })
    }
}

/// The ONE response-body type in the image: every route's content, behind one
/// `Content` impl, so picoserve's `Response`/`ContentBody`/`HeadersChain`
/// machinery is monomorphized exactly once. Variants that wrap a streaming
/// body delegate to it verbatim — the flash-readback discipline (chunk, pad,
/// yield) lives in those types, not here.
enum ApiBody {
    /// `application/json` — ~40 routes.
    Json(String),
    /// A `&'static str` body with an explicit content type (the embedded
    /// index page is `text/html`, the 404 / redirect notes are `text/plain`).
    Text {
        ct: &'static str,
        s: &'static str,
    },
    /// `application/octet-stream` — `/api/pixels`.
    Bytes(Vec<u8>),
    #[cfg(not(feature = "hosted-ui"))]
    Asset(FlashAsset),
    Source(CurrentSource),
    Envelope(CurrentEnvelope),
    /// A body-less response (204 preflight, 304 asset revalidation).
    /// `Response::new` always emits Content-Type + Content-Length and there is
    /// no way to suppress them, so both are carried explicitly: `len` is 0 for
    /// the 204 and the would-be-200 length for the 304 (RFC 9110 §15.4.5 — a
    /// 304 carries the metadata a 200 would have, and RFC 9112 §6.3 makes it
    /// body-less regardless of what Content-Length says).
    Empty {
        ct: &'static str,
        len: usize,
    },
}

impl Content for ApiBody {
    fn content_type(&self) -> &'static str {
        match self {
            ApiBody::Json(_) => "application/json",
            ApiBody::Text { ct, .. } => ct,
            ApiBody::Bytes(_) => "application/octet-stream",
            #[cfg(not(feature = "hosted-ui"))]
            ApiBody::Asset(a) => a.content_type(),
            ApiBody::Source(s) => s.content_type(),
            ApiBody::Envelope(e) => e.content_type(),
            ApiBody::Empty { ct, .. } => ct,
        }
    }

    fn content_length(&self) -> usize {
        match self {
            ApiBody::Json(s) => s.len(),
            ApiBody::Text { s, .. } => s.len(),
            ApiBody::Bytes(v) => v.len(),
            // exact-from-snapshot: these three compute their length from the
            // location snapshot the body will stream, so a swap landing
            // mid-response can never desync Content-Length from the wire
            #[cfg(not(feature = "hosted-ui"))]
            ApiBody::Asset(a) => a.content_length(),
            ApiBody::Source(s) => s.content_length(),
            ApiBody::Envelope(e) => e.content_length(),
            ApiBody::Empty { len, .. } => *len,
        }
    }

    async fn write_content<W: picoserve::io::Write>(self, writer: W) -> Result<(), W::Error> {
        match self {
            ApiBody::Json(s) => s.write_content(writer).await,
            ApiBody::Text { s, .. } => s.write_content(writer).await,
            ApiBody::Bytes(v) => v.write_content(writer).await,
            #[cfg(not(feature = "hosted-ui"))]
            ApiBody::Asset(a) => a.write_content(writer).await,
            ApiBody::Source(s) => s.write_content(writer).await,
            ApiBody::Envelope(e) => e.write_content(writer).await,
            ApiBody::Empty { .. } => Ok(()),
        }
    }
}

/// Widest arm: the CORS preflight's four headers. A silent push failure would
/// drop a CORS header and break cross-origin DELETE with no trace, so the
/// capacity is asserted here rather than discovered on a device.
const MAX_HEADERS: usize = 4;
const _: () = assert!(MAX_HEADERS >= 4, "the OPTIONS preflight arm needs 4 headers");

/// The ONE response type in the image (Gitea #167). Status + a homogeneous
/// header slice + [ApiBody] — which makes exactly one
/// `Response<HeadersChain<ContentHeaders, &[(&str, HVal)]>, ContentBody<ApiBody>>`
/// and therefore exactly one `write_to`.
struct Reply {
    status: StatusCode,
    headers: heapless::Vec<(&'static str, HVal), MAX_HEADERS>,
    body: ApiBody,
}

impl Reply {
    fn new(status: StatusCode, body: ApiBody) -> Self {
        Reply {
            status,
            headers: heapless::Vec::new(),
            body,
        }
    }

    fn ok(body: ApiBody) -> Self {
        Reply::new(StatusCode::OK, body)
    }

    /// `200 application/json` + CORS — the shape ~40 routes return. The
    /// Content-Type comes from [ApiBody::Json] via picoserve's
    /// `ContentHeaders`; do NOT add one here or it goes out twice.
    fn json(body: String) -> Self {
        Reply::ok(ApiBody::Json(body)).cors()
    }

    fn hdr(mut self, name: &'static str, value: HVal) -> Self {
        if self.headers.push((name, value)).is_err() {
            debug_assert!(false, "Reply header overflow — raise MAX_HEADERS");
        }
        self
    }

    fn shdr(self, name: &'static str, value: &'static str) -> Self {
        self.hdr(name, HVal::Static(value))
    }

    fn cors(self) -> Self {
        self.shdr("Access-Control-Allow-Origin", "*")
    }
}

impl picoserve::response::IntoResponse for Reply {
    async fn write_to<
        R: picoserve::io::Read,
        W: picoserve::response::ResponseWriter<Error = R::Error>,
    >(
        self,
        connection: picoserve::response::Connection<'_, R>,
        response_writer: W,
    ) -> Result<picoserve::ResponseSent, W::Error> {
        // destructure so `body` moves into the response while `headers` stays
        // borrowable from this frame for the duration of the write
        let Reply {
            status,
            headers,
            body,
        } = self;
        response_writer
            .write_response(
                connection,
                picoserve::response::Response::new(status, body).with_headers(&headers[..]),
            )
            .await
    }
}

type ApiResponse = Reply;

fn json_response(body: String) -> ApiResponse {
    Reply::json(body)
}

/// `{"ok":false,"error":…}` for a static message. Static because every
/// caller's message is a literal — no escaping needed, and one shared
/// formatter instead of one per call site.
fn api_error(msg: &'static str) -> String {
    let mut out = String::from("{\"ok\":false,\"error\":\"");
    push_piece(&mut out, msg);
    push_piece(&mut out, "\"}");
    out
}

/// [api_error]'s twin for a runtime-built message (a driver's error text, a
/// compiler diagnostic) — escapes it into the same body shape.
fn api_error_esc(msg: &str) -> String {
    let mut out = String::from("{\"ok\":false,\"error\":\"");
    push_piece(&mut out, &json_escape(msg));
    push_piece(&mut out, "\"}");
    out
}

/// Closes an `{"ok":true,…` settings body whose flash write failed — the
/// value is applied live regardless, so only the note reports the failure.
fn push_not_persisted(out: &mut String, e: &str) {
    push_piece(out, ",\"note\":\"not persisted: ");
    push_piece(out, &json_escape(e));
    push_piece(out, "\"}");
}

/// The installed device output palette as the flat `[pos,r,g,b,…]` JSON
/// array the POST body and the `setOutputPalette` builtin both speak —
/// 0..=255 per component. `[]` = no device palette.
fn palette_json() -> String {
    let stops = crate::shared::post_palette_stops();
    let mut out = String::from("[");
    for (i, (pos, c)) in stops.iter().enumerate() {
        if i > 0 {
            push_piece(&mut out, ",");
        }
        push_u32(&mut out, *pos as u32);
        for v in c {
            push_piece(&mut out, ",");
            push_u32(&mut out, *v as u32);
        }
    }
    push_piece(&mut out, "]");
    out
}

/// Cache policy for a flash asset. Content-hashed bundle files
/// (`/assets/index-<hash>.js`) can never change under their URL, so cache
/// them hard and skip revalidation entirely; everything else (index.html,
/// luxel.wasm, gallery.json) must revalidate — the ETag then yields 304s.
#[cfg(not(feature = "hosted-ui"))]
fn asset_cache_control(path: &str) -> (&'static str, &'static str) {
    if path.starts_with("/assets/") {
        ("Cache-Control", "public, max-age=31536000, immutable")
    } else {
        ("Cache-Control", "no-cache")
    }
}

/// True if one `If-None-Match` token matches our (quoted) ETag. Tolerates
/// surrounding whitespace, a `W/` weak-validator prefix, and `*`.
#[cfg(not(feature = "hosted-ui"))]
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
    let live = crate::shared::live_proto(embassy_time::Instant::now().as_millis() as u32);
    // Read-back availability: the running pattern's source + blob now live in
    // flash (see patterns::store_current). True = serveable (rodata default or
    // a good flash write); false = a swap's flash write shed the copy — the
    // soak-log signal that shedding actually happened on this device.
    let src = crate::shared::current_src_available();
    let bc = crate::shared::current_bc_available();
    // Per-slot lifecycle stages (see SLOT_STAGE) — a slot parked at a
    // shutdown stage is wedged on a client that won't close its half.
    let web = {
        let mut s = String::new();
        for (i, st) in SLOT_STAGE.iter().take(WEB_TASK_POOL_SIZE).enumerate() {
            if i > 0 {
                push_piece(&mut s, ",");
            }
            push_u32(&mut s, st.load(Ordering::Relaxed) as u32);
        }
        s
    };
    // max_pixels: this board's cap (per-board since #74 — a HUB75 panel
    // board allows 4096). The playground polls status continuously, so
    // carrying the cap here keeps its pixel control clamped to the real
    // device even if the one-shot /api/config probe at connect failed.
    let mut out = String::from("{\"fps\":");
    push_u32(&mut out, fps);
    push_piece(&mut out, ",\"pixels\":");
    push_u32(&mut out, pixels);
    push_piece(&mut out, ",\"max_pixels\":");
    push_u32(&mut out, MAX_PIXELS);
    push_piece(&mut out, ",\"slot\":\"");
    push_piece(&mut out, slot);
    push_piece(&mut out, "\",\"version\":\"");
    push_piece(&mut out, version);
    push_piece(&mut out, "\",\"heap_free\":");
    push_u32(&mut out, heap as u32);
    push_piece(&mut out, ",\"live\":");
    match live {
        Some(p) => {
            push_piece(&mut out, "\"");
            push_piece(&mut out, p);
            push_piece(&mut out, "\"");
        }
        None => push_piece(&mut out, "null"),
    }
    push_piece(&mut out, ",\"src\":");
    push_piece(&mut out, if src { "true" } else { "false" });
    push_piece(&mut out, ",\"bc\":");
    push_piece(&mut out, if bc { "true" } else { "false" });
    push_piece(&mut out, ",\"web\":[");
    push_piece(&mut out, &web);
    push_piece(&mut out, "],\"vmerr\":");
    match get_vmerr() {
        Some(e) => {
            push_piece(&mut out, "\"");
            push_piece(&mut out, &json_escape(&e));
            push_piece(&mut out, "\"");
        }
        None => push_piece(&mut out, "null"),
    }
    push_piece(&mut out, "}");
    out
}

async fn api_status() -> ApiResponse {
    json_response(status_json())
}

/// A flash-resident asset (playground bundle) streamed in 2 KiB chunks —
/// whole files don't fit the heap.
#[cfg(not(feature = "hosted-ui"))]
struct FlashAsset(crate::assets::AssetEntry);

/// The archive's stored content type, mapped to the `&'static str` the
/// `Content` trait wants. Free-standing so the 304 arm can report the same
/// type its 200 would have carried, without building a [FlashAsset].
#[cfg(not(feature = "hosted-ui"))]
fn asset_content_type(ctype: &str) -> &'static str {
    match ctype {
        t if t.starts_with("text/html") => TEXT_HTML,
        t if t.starts_with("application/javascript") => "application/javascript; charset=utf-8",
        t if t.starts_with("text/css") => "text/css; charset=utf-8",
        t if t.starts_with("application/wasm") => "application/wasm",
        t if t.starts_with("image/svg") => "image/svg+xml",
        t if t.starts_with("image/png") => "image/png",
        _ => "application/octet-stream",
    }
}

#[cfg(not(feature = "hosted-ui"))]
impl picoserve::response::Content for FlashAsset {
    fn content_type(&self) -> &'static str {
        asset_content_type(&self.0.ctype)
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

/// Stream a flash-resident read-back blob (the running pattern's source or
/// bytecode) chunk by chunk, yielding a real Timer between reads so the WiFi
/// task runs during each esp-storage cache-off window (same discipline as
/// [FlashAsset]). Bytes written are capped at `len` (the snapshot the
/// Content-Length was computed from) so a swap landing mid-stream can only
/// truncate the body, never overrun it; a flash-busy read truncates too.
async fn stream_flash_readback<W: picoserve::io::Write>(
    writer: &mut W,
    abs: Option<u32>,
    len: usize,
    label: &str,
) -> Result<(), W::Error> {
    let mut written = 0usize;
    if let Some(abs) = abs {
        let mut buf = alloc::vec![0u8; 4096];
        while written < len {
            let n = (len - written).min(4096);
            if !crate::assets::read_chunk(abs + written as u32, &mut buf[..n]) {
                // Flash busy (OTA / library save holds the lease). The
                // Content-Length header is already on the wire, so a SHORT
                // body would desync the connection and wedge this pool slot
                // until the write timeout (observed on-device as cascading
                // dead requests). Pad to the promised length below instead:
                // framing stays valid, the client sees obviously-wrong
                // bytes, and the log names the failure.
                esp_println::println!(
                    "readback: {} read failed at {}/{} B — padding",
                    label,
                    written,
                    len
                );
                break;
            }
            writer.write_all(&buf[..n]).await?;
            written += n;
            embassy_time::Timer::after(embassy_time::Duration::from_millis(1)).await;
        }
    } else {
        esp_println::println!("readback: {} — no storage region, padding {} B", label, len);
    }
    while written < len {
        let pad = [b'\n'; 64];
        let take = pad.len().min(len - written);
        writer.write_all(&pad[..take]).await?;
        written += take;
    }
    Ok(())
}

/// Serve a LIBRARY pattern's read-back: `bytes` freshly fetched from the
/// pattern store (a transient heap copy, dropped when the response ends —
/// the standing-RAM-copy cost this design replaced stays gone). Exactly
/// `len` bytes go out (the Content-Length snapshot from the swap):
/// truncated / newline-padded on mismatch (a re-save changed the stored
/// content mid-session) and fully padded when `bytes` is None (pattern
/// deleted, or the store's flash lease is busy) — framing stays valid,
/// same discipline as [stream_flash_readback].
async fn stream_store_readback<W: picoserve::io::Write>(
    writer: &mut W,
    bytes: Option<alloc::vec::Vec<u8>>,
    len: usize,
    label: &str,
) -> Result<(), W::Error> {
    let mut written = 0usize;
    match bytes {
        Some(b) => {
            let n = b.len().min(len);
            writer.write_all(&b[..n]).await?;
            written = n;
            if b.len() != len {
                esp_println::println!(
                    "readback: {} store copy is {} B, snapshot promised {} B — truncating/padding",
                    label,
                    b.len(),
                    len
                );
            }
        }
        None => {
            esp_println::println!(
                "readback: {} — library pattern unavailable (deleted / store busy), padding {} B",
                label,
                len
            );
        }
    }
    while written < len {
        let pad = [b'\n'; 64];
        let take = pad.len().min(len - written);
        writer.write_all(&pad[..take]).await?;
        written += take;
    }
    Ok(())
}

/// `GET /api/pattern`: the running pattern's SOURCE, streamed from flash (or
/// rodata for the default) — the bytes no longer sit in a standing RAM copy.
/// The location is snapshotted at construction so Content-Length matches the
/// bytes written even if a swap lands mid-response.
struct CurrentSource(crate::shared::SrcLoc);

impl picoserve::response::Content for CurrentSource {
    fn content_type(&self) -> &'static str {
        "text/plain; charset=utf-8"
    }
    fn content_length(&self) -> usize {
        match self.0 {
            crate::shared::SrcLoc::Default(s) => s.len(),
            crate::shared::SrcLoc::Flash(len) => len,
            crate::shared::SrcLoc::Library(len) => len,
            crate::shared::SrcLoc::Gone => 0,
        }
    }
    async fn write_content<W: picoserve::io::Write>(self, mut writer: W) -> Result<(), W::Error> {
        match self.0 {
            // rodata: a plain RAM slice, no flash cache-off — write it straight
            crate::shared::SrcLoc::Default(s) => writer.write_all(s.as_bytes()).await,
            crate::shared::SrcLoc::Flash(len) => {
                let abs = crate::patterns::current_slot_abs().map(|(s, _)| s);
                stream_flash_readback(&mut writer, abs, len, "src").await
            }
            crate::shared::SrcLoc::Library(len) => {
                let src = crate::patterns::source_of(&crate::shared::get_current_pattern_id());
                stream_store_readback(&mut writer, src.map(String::into_bytes), len, "src").await
            }
            crate::shared::SrcLoc::Gone => Ok(()),
        }
    }
}

/// `GET /api/pattern.lxp`: the running pattern as an LXP1 envelope (empty
/// name + source + bytecode) — what a sync follower adopts. Assembled on the
/// wire without ever materialising the whole envelope in RAM: the fixed
/// header + length prefixes are tiny and the source/blob bytes stream
/// straight from flash (or rodata). Byte-identical to
/// bytecode::encode_envelope("", src, bc), so the follower's decoder is
/// unchanged. Locations snapshotted at construction for a stable
/// Content-Length.
struct CurrentEnvelope {
    src: crate::shared::SrcLoc,
    bc: crate::shared::BcLoc,
}

impl CurrentEnvelope {
    fn src_len(&self) -> usize {
        match self.src {
            crate::shared::SrcLoc::Default(s) => s.len(),
            crate::shared::SrcLoc::Flash(len) => len,
            crate::shared::SrcLoc::Library(len) => len,
            crate::shared::SrcLoc::Gone => 0,
        }
    }
    fn bc_len(&self) -> usize {
        match self.bc {
            crate::shared::BcLoc::Default(b) => b.len(),
            crate::shared::BcLoc::Flash(len) => len,
            crate::shared::BcLoc::Library(len) => len,
            crate::shared::BcLoc::Gone => 0,
        }
    }
}

impl picoserve::response::Content for CurrentEnvelope {
    fn content_type(&self) -> &'static str {
        "application/octet-stream"
    }
    fn content_length(&self) -> usize {
        // LXP1(4) | name_len(1) | name(0) | src_len(4) | src | bc_len(4) | bc
        4 + 1 + 4 + self.src_len() + 4 + self.bc_len()
    }
    async fn write_content<W: picoserve::io::Write>(self, mut writer: W) -> Result<(), W::Error> {
        let src_len = self.src_len();
        let bc_len = self.bc_len();
        writer.write_all(&luxel_core::bytecode::ENVELOPE_MAGIC).await?;
        writer.write_all(&[0u8]).await?; // empty name (len 0)
        writer.write_all(&(src_len as u32).to_le_bytes()).await?;
        match self.src {
            crate::shared::SrcLoc::Default(s) => writer.write_all(s.as_bytes()).await?,
            crate::shared::SrcLoc::Flash(len) => {
                let abs = crate::patterns::current_slot_abs().map(|(s, _)| s);
                stream_flash_readback(&mut writer, abs, len, "src").await?
            }
            crate::shared::SrcLoc::Library(len) => {
                let src = crate::patterns::source_of(&crate::shared::get_current_pattern_id());
                stream_store_readback(&mut writer, src.map(String::into_bytes), len, "src").await?
            }
            crate::shared::SrcLoc::Gone => {}
        }
        writer.write_all(&(bc_len as u32).to_le_bytes()).await?;
        match self.bc {
            crate::shared::BcLoc::Default(b) => writer.write_all(b).await?,
            crate::shared::BcLoc::Flash(len) => {
                let abs = crate::patterns::current_slot_abs().map(|(_, b)| b);
                stream_flash_readback(&mut writer, abs, len, "bc").await?
            }
            crate::shared::BcLoc::Library(len) => {
                let bc = crate::patterns::bytecode_of(&crate::shared::get_current_pattern_id());
                stream_store_readback(&mut writer, bc, len, "bc").await?
            }
            crate::shared::BcLoc::Gone => {}
        }
        Ok(())
    }
}

/// Streams a LUXA archive into the assets flash region (see src/assets.rs).
/// Same streaming shape as OtaService; no reboot — the TOC hot-reloads.
#[cfg(not(feature = "hosted-ui"))]
struct AssetsService;

#[cfg(not(feature = "hosted-ui"))]
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
                let mut out = String::from("{\"ok\":true,\"bytes\":");
                push_u32(&mut out, *n);
                push_piece(&mut out, ",\"files\":");
                push_u32(&mut out, crate::assets::count() as u32);
                push_piece(&mut out, "}");
                out
            }
            Err(e) => {
                esp_println::println!("assets install failed: {}", e);
                api_error_esc(e)
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
                let mut out =
                    String::from("{\"ok\":false,\"error\":\"pattern upload too large (");
                push_u32(&mut out, (expected / 1024) as u32);
                push_piece(&mut out, " KB; this device accepts up to ");
                push_u32(&mut out, (MAX_UPLOAD / 1024) as u32);
                push_piece(&mut out, " KB)\"}");
                break 'resp out;
            }
            let mut env: Vec<u8> = Vec::new();
            if env.try_reserve_exact(expected).is_err() {
                // the running pattern owns the heap — freeze it (frees its
                // program + arrays; the strip holds its last frame) and
                // retry once the render task has drained the message
                MSG_QUEUE.send(Msg::Freeze).await;
                embassy_time::Timer::after(embassy_time::Duration::from_millis(60)).await;
                if env.try_reserve_exact(expected).is_err() {
                    let mut out = String::from(
                        "{\"ok\":false,\"error\":\"not enough free memory on the device for this ",
                    );
                    push_u32(&mut out, (expected / 1024) as u32);
                    push_piece(&mut out, " KB upload (about ");
                    push_u32(&mut out, (esp_alloc::HEAP.free() as usize / 1024) as u32);
                    push_piece(&mut out, " KB free) — it is too large to run here\"}");
                    break 'resp out;
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
                let mut out = String::from("{\"ok\":true,\"bytes\":");
                push_u32(&mut out, *n);
                push_piece(&mut out, "}");
                out
            }
            Err(e) => {
                esp_println::println!("ota failed: {}", e);
                api_error_esc(e)
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
async fn api_pixels() -> ApiResponse {
    Reply::ok(ApiBody::Bytes(get_pixels())).cors()
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
    let env = decode_envelope(raw).map_err(|e| api_error_esc(&e.to_string()))?;
    // validate() checks everything deserialize() would but allocates nothing —
    // the full Program is only ever materialized once, by the render task
    // (building it here too OOM'd the heap under soak-speed pattern churn).
    match validate(env.bytecode) {
        Ok(_) => Ok(env),
        Err(e @ BcError::Version { .. }) => {
            let mut out = String::from("{\"ok\":false,\"code\":\"bc-version\",\"error\":\"");
            push_piece(&mut out, &json_escape(&e.to_string()));
            push_piece(&mut out, "\"}");
            Err(out)
        }
        Err(e) => Err(api_error_esc(&e.to_string())),
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
            // empty id = ad-hoc; the render task stamps identity at the swap
            MSG_QUEUE.send(Msg::Code { env: raw, id: String::new() }).await;
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
            MSG_QUEUE.send(Msg::Code { env, id: String::from(id) }).await;
            // controls reset to the pattern's defaults on activation
            crate::shared::set_current_controls(Vec::new());
            crate::resume::mark_dirty(); // debounced single-pattern persist
            String::from("{\"ok\":true}")
        }
        Err(e @ BcError::Version { .. }) => {
            let mut out = String::from("{\"ok\":false,\"code\":\"bc-version\",\"error\":\"");
            push_piece(&mut out, &json_escape(&e.to_string()));
            push_piece(&mut out, "\"}");
            out
        }
        Err(e) => api_error_esc(&e.to_string()),
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
    // remember explicit tweaks for single-pattern reboot resume — only when
    // the running pattern is a saved one and no playlist owns the params
    if !crate::playlist::is_playing() && !crate::shared::get_current_pattern_id().is_empty() {
        crate::shared::record_control(name, &values);
        crate::resume::mark_dirty(); // debounced — a slider drag is a burst
    }
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
///
/// It is also why this stays a `PathRouterService` rather than picoserve's
/// `MethodRouter`: the HEAD arm of `MethodRouter` wraps the response writer in
/// a private `IgnoreBody<W>`, i.e. a SECOND `W` type — which would duplicate
/// every GET instantiation in the image, undoing the response collapse this
/// module is built around (Gitea #167).
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
        use picoserve::response::IntoResponse as _;

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
                #[cfg(not(feature = "hosted-ui"))]
                "/api/assets" => {
                    return alloc::boxed::Box::pin(AssetsService.call_request_handler_service(
                        state,
                        path_parameters,
                        request,
                        response_writer,
                    ))
                    .await;
                }
                // A hosted-ui image has no on-device web app by construction.
                // Say so instead of 404ing: tools/deploy.sh --assets-only and
                // the release instructions both aim here.
                #[cfg(feature = "hosted-ui")]
                "/api/assets" => {
                    let conn = request.body_connection.finalize().await?;
                    return json_response(api_error_esc(
                        "hosted-ui build: this image has no on-device web app",
                    ))
                    .write_to(conn, response_writer)
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
                            let mut out = String::from("{\"ok\":true,\"ssid\":\"");
                            push_piece(&mut out, &json_escape(ssid));
                            push_piece(&mut out, "\",\"note\":\"rebooting to apply\"}");
                            out
                        }
                        Err(e) => api_error_esc(e),
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
                            let mut out = String::from("{\"ok\":true,\"tzMinutes\":");
                            push_i32(&mut out, tz as i32);
                            push_piece(&mut out, "}");
                            out
                        }
                        _ => String::from(
                            "{\"ok\":false,\"error\":\"tz must be minutes in -840..=840\"}",
                        ),
                    }))
                }
                // body: "<order> <gamma_tenths> <cap_ma> [<bright_curve_tenths>
                // <blur_pct> <glow_pct>]" (e.g. "grb 22 1500 22 20 40") → the
                // output pipeline, applied live + persisted. The last three are
                // optional so pre-post-process clients keep working: absent =
                // keep the stored value. Present-but-out-of-range fails the
                // whole request, like a bad gamma.
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
                    let opt = |tok: Option<&str>, cur: u8, max: u8| match tok {
                        None => Some(cur),
                        Some(v) => v.parse::<u8>().ok().filter(|x| *x <= max),
                    };
                    let curve = opt(
                        it.next(),
                        crate::shared::BRIGHT_CURVE.load(Ordering::Relaxed),
                        50,
                    );
                    let blur =
                        opt(it.next(), crate::shared::POST_BLUR.load(Ordering::Relaxed), 100);
                    let glow =
                        opt(it.next(), crate::shared::POST_GLOW.load(Ordering::Relaxed), 100);
                    Some(json_response(match (order, gamma, cap, curve, blur, glow) {
                        (Some(o), Some(g), Some(c), Some(bc), Some(bl), Some(gl)) => {
                            crate::shared::COLOR_ORDER.store(o.0, Ordering::Relaxed);
                            crate::shared::GAMMA_TENTHS.store(g, Ordering::Relaxed);
                            crate::shared::CAP_MA.store(c as u32, Ordering::Relaxed);
                            crate::shared::BRIGHT_CURVE.store(bc, Ordering::Relaxed);
                            crate::shared::POST_BLUR.store(bl, Ordering::Relaxed);
                            crate::shared::POST_GLOW.store(gl, Ordering::Relaxed);
                            let _ = crate::config::write_device(
                                &crate::shared::device_config_snapshot(),
                            );
                            let mut out = String::from("{\"ok\":true,\"order\":\"");
                            push_piece(&mut out, o.name());
                            push_piece(&mut out, "\",\"gamma\":");
                            push_u32(&mut out, g as u32);
                            push_piece(&mut out, ",\"capMa\":");
                            push_u32(&mut out, c as u32);
                            push_piece(&mut out, ",\"brightCurve\":");
                            push_u32(&mut out, bc as u32);
                            push_piece(&mut out, ",\"blur\":");
                            push_u32(&mut out, bl as u32);
                            push_piece(&mut out, ",\"glow\":");
                            push_u32(&mut out, gl as u32);
                            push_piece(&mut out, "}");
                            out
                        }
                        _ => String::from(
                            "{\"ok\":false,\"error\":\"expected: <rgb|rbg|grb|gbr|brg|bgr> <gamma_tenths 0-50> <cap_ma 0-20000> [<bright_curve_tenths 0-50> <blur_pct 0-100> <glow_pct 0-100>]\"}",
                        ),
                    }))
                }
                // body: "<amount_pct> <pos> <r> <g> <b> …" (0..=255 each) →
                // the device output palette, applied live + persisted in its
                // own flash record. Composes with a pattern's own
                // setOutputPalette rather than replacing it (Gitea #139).
                "/api/output/palette" => {
                    // clients re-read GET /api/output for the echo, so the
                    // success body stays minimal (image bytes are scarce)
                    Some(json_response(
                        match luxel_core::outpipe::parse_palette_stops(&text(&raw)) {
                            Ok((amount, stops)) => {
                                if crate::outpal::store(stops, amount) {
                                    String::from("{\"ok\":true}")
                                } else {
                                    api_error("applied live, but the store refused to persist it")
                                }
                            }
                            Err(e) => api_error(e),
                        },
                    ))
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
                            let mut out = String::from("{\"ok\":true,\"mode\":\"");
                            push_piece(&mut out, sync_mode_name(m));
                            push_piece(&mut out, "\"}");
                            out
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
                // binary body: one "EV1\0" event frame (netin::parse_events)
                // — external event injection for readEvent()-driven patterns
                // (keyboards, MQTT/HA bridges, preview clicks).
                "/api/events" => {
                    Some(json_response(match luxel_core::netin::parse_events(&raw) {
                        Some(evs) => {
                            crate::shared::push_events(&evs);
                            String::from("{\"ok\":true}")
                        }
                        None => String::from("{\"ok\":false,\"error\":\"not an event frame\"}"),
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
                            let mut out = String::from("{\"ok\":true,\"enabled\":");
                            push_piece(&mut out, if host.is_empty() { "false" } else { "true" });
                            push_piece(&mut out, "}");
                            out
                        }
                        Err(e) => api_error_esc(e),
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
                            let mut out = String::from("{\"ok\":true,\"brightness\":");
                            push_u32(&mut out, b as u32);
                            match crate::config::write_device(&cfg) {
                                Ok(()) => push_piece(&mut out, "}"),
                                // applied live even if the flash write failed
                                Err(e) => push_not_persisted(&mut out, e),
                            }
                            out
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
                            // it flips the atomic + rebuilds when it drains this.
                            // WANT_PIXEL_COUNT is the requested value — what
                            // persistence reads (the applied atomic lags).
                            crate::shared::WANT_PIXEL_COUNT.store(n, Ordering::Relaxed);
                            MSG_QUEUE.send(Msg::Config(n)).await;
                            let cfg = DeviceConfig { pixel_count: n, ..crate::shared::device_config_snapshot() };
                            let mut out = String::from("{\"ok\":true,\"pixels\":");
                            push_u32(&mut out, n);
                            match crate::config::write_device(&cfg) {
                                Ok(()) => push_piece(&mut out, "}"),
                                Err(e) => push_not_persisted(&mut out, e),
                            }
                            out
                        }
                        _ => {
                            let mut out =
                                String::from("{\"ok\":false,\"error\":\"pixels must be 1..=");
                            push_u32(&mut out, MAX_PIXELS);
                            push_piece(&mut out, "\"}");
                            out
                        }
                    }))
                }
                // POST /api/protocol — body is a protocol name (sk9822/ws2812
                // + aliases). Reconfigures SPI + resizes the buffer live, and
                // persists. No reboot.
                "/api/protocol" => {
                    Some(json_response(match Protocol::from_name(text(&raw).trim()) {
                        Some(p) => {
                            // WANT_PROTOCOL: same requested-vs-applied split
                            // as /api/config above
                            crate::shared::WANT_PROTOCOL.store(p.as_u8(), Ordering::Relaxed);
                            MSG_QUEUE.send(Msg::Protocol(p.as_u8())).await;
                            let cfg = DeviceConfig { protocol: p.as_u8(), ..crate::shared::device_config_snapshot() };
                            let mut out = String::from("{\"ok\":true,\"protocol\":\"");
                            push_piece(&mut out, p.name());
                            push_piece(&mut out, "\"");
                            match crate::config::write_device(&cfg) {
                                Ok(()) => push_piece(&mut out, "}"),
                                Err(e) => push_not_persisted(&mut out, e),
                            }
                            out
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
                    let mut out = String::from("{\"ok\":true,\"installed\":");
                    push_piece(&mut out, if installed { "true" } else { "false" });
                    push_piece(&mut out, ",\"count\":");
                    push_u32(&mut out, count as u32);
                    push_piece(&mut out, "}");
                    Some(json_response(out))
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
            // clear the device output palette (record erased, stage off)
            if route == "/api/output/palette" {
                let body = if crate::outpal::clear() {
                    String::from("{\"ok\":true}")
                } else {
                    api_error("cleared live, but the store refused to persist it")
                };
                let response = json_response(body);
                let conn = request.body_connection.finalize().await?;
                return response.write_to(conn, response_writer).await;
            }
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
            // All four must survive (MAX_HEADERS is sized for exactly this
            // arm) — a dropped one breaks cross-origin DELETE silently.
            let conn = request.body_connection.finalize().await?;
            return Reply::new(
                StatusCode::NO_CONTENT,
                ApiBody::Empty {
                    ct: TEXT_PLAIN,
                    len: 0,
                },
            )
            .cors()
            .shdr("Access-Control-Allow-Methods", "GET, POST, DELETE, OPTIONS")
            .shdr("Access-Control-Allow-Headers", "Content-Type")
            .shdr("Access-Control-Max-Age", "86400")
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
            #[cfg(not(feature = "hosted-ui"))]
            macro_rules! serve_asset {
                ($e:expr) => {{
                    let mut e = $e;
                    let cc = asset_cache_control(&e.path);
                    let matched = !e.etag.is_empty()
                        && request
                            .parts
                            .headers()
                            .get("If-None-Match")
                            .is_some_and(|inm| {
                                inm.split(b',').any(|t| etag_matches(t.as_raw(), e.etag.as_bytes()))
                            });
                    // taken (not cloned) — nothing downstream reads it, and
                    // an empty etag means a legacy "LUXA" archive with no hash
                    let etag = core::mem::take(&mut e.etag);
                    if matched {
                        // 304: same ETag/Cache-Control the 200 would carry,
                        // and the would-be-200 length (see ApiBody::Empty)
                        respond!(Reply::new(
                            StatusCode::NOT_MODIFIED,
                            ApiBody::Empty {
                                ct: asset_content_type(&e.ctype),
                                len: e.len as usize,
                            },
                        )
                        .hdr("ETag", HVal::Owned(etag))
                        .shdr(cc.0, cc.1));
                    }
                    let gzip = e.gzip;
                    let mut reply = Reply::ok(ApiBody::Asset(FlashAsset(e))).shdr(cc.0, cc.1);
                    if !etag.is_empty() {
                        reply = reply.hdr("ETag", HVal::Owned(etag));
                    }
                    if gzip {
                        reply = reply.shdr("Content-Encoding", "gzip");
                    }
                    respond!(reply);
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
                    #[cfg(not(feature = "hosted-ui"))]
                    if let Some(e) = crate::assets::lookup("/index.html") {
                        serve_asset!(e);
                    }
                    respond!(Reply::ok(ApiBody::Text {
                        ct: TEXT_HTML,
                        s: INDEX_HTML
                    }));
                }
                "/min" => respond!(Reply::ok(ApiBody::Text {
                    ct: TEXT_HTML,
                    s: INDEX_HTML
                })),
                "/api/status" => Some(api_status().await),
                // which network the NEXT boot will join (never the password)
                "/api/wifi" => {
                    let ssid_body = |ssid: &str, source: &str| {
                        let mut out = String::from("{\"ssid\":\"");
                        push_piece(&mut out, &json_escape(ssid));
                        push_piece(&mut out, "\",\"source\":\"");
                        push_piece(&mut out, source);
                        push_piece(&mut out, "\"}");
                        out
                    };
                    let body = match crate::config::read_wifi() {
                        Some((ssid, _)) => ssid_body(&ssid, "flash"),
                        None => match option_env!("LUXEL_SSID") {
                            Some(s) if !s.is_empty() => ssid_body(s, "builtin"),
                            _ => String::from("{\"ssid\":null,\"source\":\"none\"}"),
                        },
                    };
                    Some(json_response(body))
                }
                "/api/brightness" => {
                    let mut out = String::from("{\"brightness\":");
                    push_u32(&mut out, BRIGHTNESS.load(Ordering::Relaxed) as u32);
                    push_piece(&mut out, ",\"max\":31}");
                    Some(json_response(out))
                }
                // whether we're currently the provisioning AP
                "/api/apmode" => {
                    let mut out = String::from("{\"ap\":");
                    push_piece(&mut out, if crate::provision::AP_MODE.load(Ordering::Relaxed) {
                        "true"
                    } else {
                        "false"
                    });
                    push_piece(&mut out, "}");
                    Some(json_response(out))
                }
                // output pipeline settings (palette included: one fetch
                // backs the whole Output card)
                "/api/output" => {
                    let mut out = String::from("{\"order\":\"");
                    push_piece(&mut out, 
                        luxel_core::outpipe::ColorOrder(
                            crate::shared::COLOR_ORDER.load(Ordering::Relaxed),
                        )
                        .name(),
                    );
                    push_piece(&mut out, "\",\"gamma\":");
                    push_u32(&mut out, crate::shared::GAMMA_TENTHS.load(Ordering::Relaxed) as u32);
                    push_piece(&mut out, ",\"capMa\":");
                    push_u32(&mut out, crate::shared::CAP_MA.load(Ordering::Relaxed));
                    push_piece(&mut out, ",\"brightCurve\":");
                    push_u32(&mut out, crate::shared::BRIGHT_CURVE.load(Ordering::Relaxed) as u32);
                    push_piece(&mut out, ",\"blur\":");
                    push_u32(&mut out, crate::shared::POST_BLUR.load(Ordering::Relaxed) as u32);
                    push_piece(&mut out, ",\"glow\":");
                    push_u32(&mut out, crate::shared::POST_GLOW.load(Ordering::Relaxed) as u32);
                    push_piece(&mut out, ",\"palette\":");
                    push_piece(&mut out, &palette_json());
                    push_piece(&mut out, ",\"paletteAmount\":");
                    push_u32(
                        &mut out,
                        crate::shared::POST_PALETTE_AMOUNT.load(Ordering::Relaxed) as u32,
                    );
                    push_piece(&mut out, "}");
                    Some(json_response(out))
                }
                // wall clock: NTP-synced local time + tz (clock builtins)
                "/api/clock" => {
                    let local = crate::shared::wall_now_local();
                    let mut out = String::from("{\"synced\":");
                    push_piece(&mut out, if local.is_some() { "true" } else { "false" });
                    push_piece(&mut out, ",\"local\":");
                    push_i64(&mut out, local.unwrap_or(0));
                    push_piece(&mut out, ",\"tzMinutes\":");
                    push_i32(&mut out, crate::shared::TZ_MINUTES.load(Ordering::Relaxed));
                    push_piece(&mut out, "}");
                    Some(json_response(out))
                }
                // sync role + engine clock + last leader beacon heard
                "/api/sync" => {
                    let mode = sync_mode_name(crate::shared::SYNC_MODE.load(Ordering::Relaxed));
                    let time_ms = crate::shared::engine_time_ms();
                    let mut out = String::from("{\"mode\":\"");
                    push_piece(&mut out, mode);
                    push_piece(&mut out, "\",\"timeMs\":");
                    push_u64(&mut out, time_ms);
                    push_piece(&mut out, ",\"leader\":");
                    match crate::shared::sync_leader() {
                        Some((boot, lt, at)) => {
                            let age = at.elapsed().as_millis();
                            let offset = (lt + age) as i64 - time_ms as i64;
                            push_piece(&mut out, "{\"bootId\":");
                            push_u32(&mut out, boot);
                            push_piece(&mut out, ",\"ageMs\":");
                            push_u64(&mut out, age);
                            push_piece(&mut out, ",\"offsetMs\":");
                            push_i64(&mut out, offset);
                            push_piece(&mut out, "}");
                        }
                        None => push_piece(&mut out, "null"),
                    }
                    push_piece(&mut out, "}");
                    Some(json_response(out))
                }
                // broker settings (never the password) + connection state
                "/api/mqtt" => {
                    let body = match crate::config::read_mqtt() {
                        Some(c) => {
                            let mut out = String::from("{\"enabled\":true,\"host\":\"");
                            push_piece(&mut out, &json_escape(&c.host));
                            push_piece(&mut out, "\",\"port\":");
                            push_u32(&mut out, c.port as u32);
                            push_piece(&mut out, ",\"user\":\"");
                            push_piece(&mut out, &json_escape(&c.user));
                            push_piece(&mut out, "\",\"hasPass\":");
                            push_piece(&mut out, if c.pass.is_empty() { "false" } else { "true" });
                            push_piece(&mut out, ",\"connected\":");
                            push_piece(&mut out, if crate::mqtt::CONNECTED.load(Ordering::Relaxed) {
                                "true"
                            } else {
                                "false"
                            });
                            push_piece(&mut out, "}");
                            out
                        }
                        None => String::from(
                            "{\"enabled\":false,\"host\":\"\",\"port\":1883,\"user\":\"\",\"hasPass\":false,\"connected\":false}",
                        ),
                    };
                    Some(json_response(body))
                }
                "/api/config" => {
                    let mut out = String::from("{\"pixels\":");
                    push_u32(&mut out, PIXEL_COUNT.load(Ordering::Relaxed));
                    push_piece(&mut out, ",\"max\":");
                    push_u32(&mut out, MAX_PIXELS);
                    push_piece(&mut out, ",\"protocol\":\"");
                    push_piece(&mut out, Protocol::from_u8(PROTOCOL.load(Ordering::Relaxed)).name());
                    push_piece(&mut out, "\"}");
                    Some(json_response(out))
                }
                "/api/playlist" => Some(json_response(crate::playlist::to_json())),
                "/api/map" => Some(json_response(crate::devicemap::to_json())),
                "/api/protocol" => {
                    let mut out = String::from("{\"protocol\":\"");
                    push_piece(&mut out, Protocol::from_u8(PROTOCOL.load(Ordering::Relaxed)).name());
                    push_piece(&mut out, "\",\"options\":[\"sk9822\",\"ws2812\"]}");
                    Some(json_response(out))
                }
                "/api/pixels" => respond!(api_pixels().await),
                // running pattern source, streamed from flash (no RAM copy)
                "/api/pattern" => respond!(Reply::ok(ApiBody::Source(CurrentSource(
                    crate::shared::current_src()
                )))
                .cors()),
                // running pattern as an LXP1 envelope (source + bytecode) —
                // what a sync follower adopts (it has no compiler); streamed
                // from flash so a big pattern needs no ~40 KB RAM residency
                "/api/pattern.lxp" => respond!(Reply::ok(ApiBody::Envelope(CurrentEnvelope {
                    src: crate::shared::current_src(),
                    bc: crate::shared::current_bc(),
                }))
                .cors()),
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
                    #[cfg(not(feature = "hosted-ui"))]
                    if let Some(e) = crate::assets::lookup(other) {
                        serve_asset!(e);
                    }
                    #[cfg(feature = "hosted-ui")]
                    let _ = other;
                    // captive-portal detection: as a provisioning AP, any
                    // unknown URL (a phone's connectivity probe) redirects
                    // to the portal, which pops the sign-in sheet
                    if crate::provision::AP_MODE.load(Ordering::Relaxed) {
                        let conn = request.body_connection.finalize().await?;
                        return Reply::new(
                            StatusCode::TEMPORARY_REDIRECT,
                            ApiBody::Text {
                                ct: TEXT_PLAIN,
                                s: "redirecting to setup",
                            },
                        )
                        .shdr("Location", "http://192.168.4.1/")
                        .shdr("Cache-Control", "no-store")
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
        Reply::new(
            StatusCode::NOT_FOUND,
            ApiBody::Text {
                ct: TEXT_PLAIN,
                s: "not found",
            },
        )
        .write_to(conn, response_writer)
        .await
    }
}

pub fn make_app() -> picoserve::Router<impl picoserve::routing::PathRouter> {
    picoserve::Router::new().nest_service("", Api)
}

// 2. History: the original 3rd slot fed the preview websocket and cost
// 32 KB of static connection buffers, so the size diet cut it. Buffers
// have since moved to per-task 4+4 KB heap vecs — but each slot still
// embeds picoserve's whole response-path future as ~8.6 KB of STATIC
// task arena (the arena that ate v0.1.33's stack margin) plus ~8 KB of
// heap. The pool briefly went back to 3 (v0.1.31) because a cold page
// load fires css/js/wasm/gallery in parallel and Chromium against the
// Athom reproducibly dropped luxel.wasm at 2 (ERR_CONNECTION_REFUSED
// mid-load, 2026-07-26) — but that was the wrong layer paying: the web
// app now routes EVERY fetch it fires (assets included, not just API
// calls) through a global 2-in-flight gate with backoff-retry on
// refused connections (web fetchgate.ts). That absorbs everything the
// PAGE does — but not what the BROWSER does before the page exists:
// Chromium opens ~2 sockets at cold navigation (speculative preconnect
// plus the nav itself; serial-correlated on the Athom 2026-08-15, and
// --disable-features=NetworkPrediction does not stop it). At 2 slots
// the preconnects win and the navigation SYN is refused — "site can't
// be reached" on first visit, no client code can compensate. So:
// browser-facing devices keep 3 slots; the `small-chip` profile takes
// 2 slots (saving ~8.6 KB static task arena + ~8 KB heap) and accepts
// an occasionally-refused first navigation (reload works).
pub const WEB_TASK_POOL_SIZE: usize = if cfg!(feature = "small-chip") { 2 } else { 3 };

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
        // pins one of only TWO server sockets until this expires — at the
        // original 300s a couple of WiFi hiccups cascaded into minutes-long
        // outages (the hw-bench soak lost ~60 requests to exactly this).
        read_request: picoserve::time::Duration::from_secs(45),
        write: picoserve::time::Duration::from_secs(5),
    },
    connection: picoserve::KeepAlive::KeepAlive,
};

// Per-slot lifecycle stage, exposed as "web" in /api/status. Diagnostic for
// the slot-wedge class: a slot that sits at a shutdown stage (3-5) is stuck
// tearing down a connection the client won't close.
//   0 accepting · 1 serving · 2 shutdown entered · 3 FIN sent (close)
//   4 discard done · 5 flush done · 9 abort path
pub static SLOT_STAGE: [core::sync::atomic::AtomicU8; 4] = [
    core::sync::atomic::AtomicU8::new(0),
    core::sync::atomic::AtomicU8::new(0),
    core::sync::atomic::AtomicU8::new(0),
    core::sync::atomic::AtomicU8::new(0),
];
fn slot_stage(id: usize, stage: u8) {
    SLOT_STAGE[id & 3].store(stage, Ordering::Relaxed);
}

// TcpSocket wrapper that bounds the graceful-shutdown FIN-wait at 2 s.
// picoserve's embassy shutdown reuses `timeouts.read_request` (our 45 s,
// sized for OTA bodies) as "wait for the CLIENT's FIN after ours" — and
// browsers' socket pools routinely sit on connections after our FIN, so
// an idle keep-alive/preconnect socket pinned a pool slot (measured on the
// Athom with 2 idle raw connections: refusals until the client closed). At
// 2 slots that starves every cold page load. The shutdown stages are
// reimplemented inline (close → bounded discard → bounded flush) with
// embassy_time::with_timeout directly and stage markers, so a wedge shows
// up in /api/status as the exact stuck stage. A client that hasn't FIN'd
// 2 s after us gets the connection dropped (embassy-net aborts on drop);
// the response was already flushed.
struct QuickCloseSocket<'s> {
    sock: embassy_net::tcp::TcpSocket<'s>,
    task_id: usize,
}

const SHUTDOWN_GRACE: embassy_time::Duration = embassy_time::Duration::from_secs(2);

impl<'s> picoserve::io::Socket<picoserve::EmbassyRuntime> for QuickCloseSocket<'s> {
    type Error = embassy_net::tcp::Error;
    type ReadHalf<'a>
        = embassy_net::tcp::TcpReader<'a>
    where
        's: 'a;
    type WriteHalf<'a>
        = embassy_net::tcp::TcpWriter<'a>
    where
        's: 'a;

    fn split(&mut self) -> (Self::ReadHalf<'_>, Self::WriteHalf<'_>) {
        self.sock.split()
    }

    async fn abort<T: picoserve::Timer<picoserve::EmbassyRuntime>>(
        mut self,
        _timeouts: &picoserve::Timeouts,
        _timer: &mut T,
    ) -> Result<(), picoserve::Error<Self::Error>> {
        slot_stage(self.task_id, 9);
        embassy_net::tcp::TcpSocket::abort(&mut self.sock);
        let _ = embassy_time::with_timeout(SHUTDOWN_GRACE, self.sock.flush()).await;
        Ok(())
    }

    async fn shutdown<T: picoserve::Timer<picoserve::EmbassyRuntime>>(
        mut self,
        _timeouts: &picoserve::Timeouts,
        _timer: &mut T,
    ) -> Result<(), picoserve::Error<Self::Error>> {
        slot_stage(self.task_id, 2);
        let t0 = embassy_time::Instant::now();
        self.sock.close();
        slot_stage(self.task_id, 3);
        {
            let (mut rx, _tx) = self.sock.split();
            let mut buf = [0u8; 128];
            let r = embassy_time::with_timeout(SHUTDOWN_GRACE, async {
                use embedded_io_async::Read;
                while matches!(rx.read(&mut buf).await, Ok(n) if n > 0) {}
            })
            .await;
            if r.is_err() {
                esp_println::println!("http[{}]: shutdown discard grace expired", self.task_id);
            }
        }
        slot_stage(self.task_id, 4);
        if embassy_time::with_timeout(SHUTDOWN_GRACE, self.sock.flush())
            .await
            .is_err()
        {
            esp_println::println!("http[{}]: shutdown flush grace expired", self.task_id);
        }
        esp_println::println!(
            "http[{}]: closed ({} ms)",
            self.task_id,
            t0.elapsed().as_millis()
        );
        // Leave the socket in a terminal state before drop: if the peer
        // never FIN'd, the graceful close above leaves a live TCP state
        // (FIN-WAIT), and the slot's rx/tx buffers are re-lent to a fresh
        // socket the moment this returns. An explicit abort (RST, flushed)
        // ends the connection for certain.
        embassy_net::tcp::TcpSocket::abort(&mut self.sock);
        let _ = embassy_time::with_timeout(SHUTDOWN_GRACE, self.sock.flush()).await;
        slot_stage(self.task_id, 5);
        Ok(())
    }
}

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

        slot_stage(task_id, 1);
        let served = picoserve::Server::new(&app, &CONFIG, &mut http_buffer)
            .serve(QuickCloseSocket {
                sock: socket,
                task_id,
            })
            .await;
        if let Err(e) = served {
            esp_println::println!("http[{}]: serve error: {:?}", task_id, e);
        }
        slot_stage(task_id, 0);
        // http_buffer freed here
    }
}
