//! The device Layout — `GET`/`POST /api/layout` (Gitea #465).
//!
//! One endpoint for the one geometry concept: what shape the installation is
//! (`strip` / `matrix` / `map`), how a matrix is tiled and chained, which
//! physical outputs carry which run of the pixel space, and the projection
//! defaults for patterns of another dimensionality. Everything the grammar,
//! the validation, the JSON and the flash record need lives in
//! [`luxel_core::layout`], shared with the `luxel serve` mirror; this module
//! is the device's half — the board's facts, the flash blob, and the wiring
//! into the state the firmware already keeps.
//!
//! **It does not duplicate state.** The pixel count stays in the nvs device
//! record and the pixel map stays in `devicemap`; a `POST` here applies its
//! edits through those same paths, which is exactly what keeps
//! `/api/config`, `/api/map`, `/api/datapin` and `/api/protocol` working as
//! aliases rather than as a second, drifting copy. The record under
//! [`patterns::LAYOUT_KEY`] is the POST wire itself (the playlist's idiom):
//! the kind, the matrix arrangement, the output table and the projection
//! triple, re-parsed at boot — one codec, no version to migrate.
//!
//! **Live vs reboot.** The pixel count, the grid, the map and the projection
//! defaults apply on the next frame, and so does most of an `out` line: the
//! run boundaries (`count`, `rev`) are re-read from the Layout every frame,
//! and output 0's protocol and colour order write through to the live strip
//! settings below. What a boot BUILDS is the chain wiring (`cols rows start
//! dir snake rot180 scan`, #475) and each output's driver INSTANCE (#474) —
//! whether it exists at all, its DATA pad, and the SPI clock a further
//! output's peripheral was configured for. A POST that changes one of those
//! answers `"reboot_required":true` (Gitea #550).

use alloc::string::String;
use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use esp_println::println;
use luxel_core::layout::{Layout, LayoutKind, Limits, Matrix, Output, Run, View};
use luxel_core::projection::Projection;

use crate::leds::Protocol;
use crate::patterns;

type Shared<T> = BlockingMutex<CriticalSectionRawMutex, RefCell<T>>;

static LAYOUT: Shared<Option<Layout>> = BlockingMutex::new(RefCell::new(None));
/// Set when the projection triple changed; the render task installs it on
/// the next frame (the map has its own flag in `devicemap`).
static PROJ_DIRTY: AtomicBool = AtomicBool::new(false);

/// The Layout a board with nothing stored comes up in: a HUB75 board IS its
/// panel, a strip board is its strip — and either becomes `map` the moment
/// someone installs one, so an existing device that has only ever used
/// `POST /api/map` reads correctly on first contact with this endpoint.
fn board_default() -> Layout {
    #[cfg(feature = "hub75")]
    let l = Layout::board_default(
        LayoutKind::Matrix,
        Matrix::single(crate::hub75::PANEL_COLS as u16, crate::hub75::PANEL_ROWS as u16),
    );
    #[cfg(not(feature = "hub75"))]
    let l = Layout::board_default(LayoutKind::Strip, Matrix::single(1, 1));
    l
}

/// A copy of the current Layout (the board default before `init`).
fn current() -> Layout {
    LAYOUT.lock(|c| match c.borrow().as_ref() {
        Some(l) => l.clone(),
        None => board_default(),
    })
}

/// The consecutive run of the ONE pixel space that output `n` drives
/// (Gitea #474, D11) — what the strip driver splits the frame by. `None` =
/// this output drives nothing. Read under the lock rather than through
/// `current()`: a clone would allocate, and this runs on every resize.
///
/// Before `init` (a `LUXEL_NO_OTA` build has no store to read) the answer is
/// the board default's: output 0 carries the whole frame.
pub fn run_of(n: u8, pixels: u32) -> Option<Run> {
    LAYOUT.lock(|c| match c.borrow().as_ref() {
        Some(l) => l.run_of(n, pixels),
        None => (n == 0).then_some(Run { start: 0, len: pixels, rev: false }),
    })
}

/// The stored `out <n> …` line, when the Layout configures one — the pin,
/// protocol and colour order the boot wiring needs to build that output's
/// driver instance. `None` for output 0 means "no table": the implicit
/// single output built from the live strip settings.
pub fn configured_output(n: u8) -> Option<Output> {
    LAYOUT.lock(|c| c.borrow().as_ref().and_then(|l| l.outputs.iter().find(|o| o.n == n).copied()))
}

/// The configured matrix arrangement — what the HUB75 driver builds its
/// panel→pixel remap from at boot (#475), and what the refresh estimate
/// describes. Copied out rather than cloning the whole Layout.
pub fn matrix() -> Matrix {
    LAYOUT.lock(|c| c.borrow().as_ref().map_or_else(|| board_default().matrix, |l| l.matrix))
}

/// The projection defaults to install on every engine (boot and rebuild).
pub fn projection() -> Projection {
    LAYOUT.lock(|c| c.borrow().as_ref().map_or(Projection::DEFAULT, |l| l.proj))
}

/// Consume the "projection changed" flag (render task calls this each frame).
/// load+store rather than `swap`: rv32imc has no atomic RMW, and the same
/// reasoning as `devicemap::take_dirty` applies.
pub fn take_proj_dirty() -> bool {
    let was = PROJ_DIRTY.load(Ordering::Relaxed);
    if was {
        PROJ_DIRTY.store(false, Ordering::Relaxed);
    }
    was
}

/// Note that the map changed OUTSIDE `/api/layout` — the `POST /api/map`
/// alias. A user map makes the Layout a `map`; clearing it goes back to the
/// board's own kind. Persisted so the kind survives a reboot.
pub fn note_map_changed(user_installed: bool) {
    let want = if user_installed { LayoutKind::Map } else { board_default().kind };
    note(Some(want));
}

/// Note that a strip setting changed through one of the ALIASES
/// (`/api/config`, `/api/protocol`, `/api/datapin`, `/api/output`). Output 0
/// IS the strip this board drives, so a stored table follows it — otherwise
/// `GET /api/layout` would report a pin or a protocol the device is not
/// using. A one-output table's count follows the pixel count too; with a
/// multi-output table the partition is the caller's to re-state through
/// `POST /api/layout` (documented in docs/api.md).
///
/// No-op — and no flash write — while nothing is stored, which is the
/// common case: the implicit single output is rendered from live state.
pub fn note_alias_change() {
    note(None);
}

/// The shared body of the two notices above: re-derive whatever the alias
/// just changed, and persist only if something actually moved. `kind` is
/// `Some` for a map notice; output 0 always follows the live strip settings.
fn note(kind: Option<LayoutKind>) {
    let cur = current();
    let mut next = cur.clone();
    if let Some(k) = kind {
        next.kind = k;
    }
    // the WANT_* values, not the applied ones: an alias POST is persisted
    // before the render task drains it, and this record must match what the
    // nvs device record just took (`shared::device_config_snapshot`).
    let proto = crate::shared::WANT_PROTOCOL.load(Ordering::Relaxed);
    let order = crate::shared::COLOR_ORDER.load(Ordering::Relaxed);
    let pixels = crate::shared::WANT_PIXEL_COUNT.load(Ordering::Relaxed);
    let single = next.outputs.len() == 1 && next.kind != LayoutKind::Matrix;
    if let Some(o) = next.outputs.iter_mut().find(|o| o.n == 0) {
        #[cfg(not(feature = "hub75"))]
        {
            o.pin = crate::shared::want_data_pin()
                .unwrap_or_else(|| crate::shared::DATA_PIN.load(Ordering::Relaxed));
        }
        o.proto = proto;
        o.order = order;
        if single {
            o.count = pixels;
        }
    }
    if next != cur {
        let _ = store(next, pixels);
    }
}

/// Apply a Layout live and persist it. False = applied but not stored (an OTA
/// holds the flash lease); callers that answer a request say so in the body.
fn store(l: Layout, pixels: u32) -> bool {
    let wire = l.to_wire(pixels, &proto_name);
    let ok = patterns::store_blob(patterns::LAYOUT_KEY, wire.as_bytes());
    if !ok {
        println!("layout: applied live, but the store refused to persist it");
    }
    LAYOUT.lock(|c| *c.borrow_mut() = Some(l));
    ok
}

/// Board facts the core parser validates a body against. `strict` is off
/// only when re-reading the persisted record (see `Limits::strict`).
fn limits_strict(strict: bool) -> Limits<'static> {
    Limits {
        max_pixels: crate::board::MAX_PIXELS,
        outputs: crate::board::OUTPUTS,
        panel: cfg!(feature = "hub75"),
        pin_ok: &crate::board::data_pin_ok,
        // what an empty `out` table means here: the one implicit output on
        // the pad the SPI driver actually bound at boot (Gitea #550)
        default_pin: crate::shared::DATA_PIN.load(Ordering::Relaxed),
        proto_code: &|s| Protocol::from_name(s).map(|p| p.as_u8()),
        strict,
    }
}

fn proto_name(c: u8) -> &'static str {
    Protocol::from_u8(c).name()
}

/// `GET /api/layout` (`pixels` = `None` reports the applied count; a POST
/// answer passes the requested one — see `layout::View::pixels`). `ok`
/// prefixes the `{"ok":true,"reboot_required":B,` a POST reply carries, so
/// the reply IS the GET body and a client never has to re-fetch.
fn json(pixels: Option<u32>, ok: Option<bool>) -> String {
    let map_json = crate::devicemap::to_json();
    let (map_dims, map_grid) = crate::devicemap::shape();
    #[cfg(not(feature = "hub75"))]
    let default_pin = crate::shared::DATA_PIN.load(Ordering::Relaxed);
    #[cfg(feature = "hub75")]
    let default_pin = 0u8;
    let v = View {
        pixels: pixels.unwrap_or_else(|| crate::shared::PIXEL_COUNT.load(Ordering::Relaxed)),
        max_pixels: crate::board::MAX_PIXELS,
        map_dims,
        map_grid,
        map_json: &map_json,
        proto_name: &proto_name,
        default_pin,
        default_proto: crate::shared::PROTOCOL.load(Ordering::Relaxed),
        default_order: crate::shared::COLOR_ORDER.load(Ordering::Relaxed),
        // `hub75` is what turns on luxel-core's `panel` feature, so a strip
        // board's `View` has no such field and pays nothing for it (#501).
        #[cfg(feature = "hub75")]
        panel: Some(crate::hub75::panel_view(&matrix())),
    };
    let mut out = String::new();
    if let Some(reboot) = ok {
        out.push_str("{\"ok\":true,\"reboot_required\":");
        out.push_str(if reboot { "true," } else { "false," });
    }
    let mut body = String::new();
    LAYOUT.lock(|c| match c.borrow().as_ref() {
        Some(l) => l.push_json(&mut body, &v),
        None => board_default().push_json(&mut body, &v),
    });
    // splice: a POST reply drops the GET body's leading `{`
    out.push_str(if ok.is_some() { &body[1..] } else { &body });
    out
}

/// `GET /api/layout`.
pub fn to_json() -> String {
    json(None, None)
}

/// What a `POST /api/layout` asks the caller to do beyond the Layout itself
/// — the two edits that belong to state this module does not own. The
/// server arm applies them (a pixel count needs the render task's message
/// queue, which is `async`).
pub struct Applied {
    pub pixels: Option<u32>,
    pub map: Option<String>,
    pub reboot_required: bool,
    /// The first output's data pin, when a `POST` moved it. Persisted here;
    /// it binds at boot like `/api/datapin`, so it is part of
    /// `reboot_required` rather than something this endpoint acts on.
    pub data_pin: Option<u8>,
    /// The first output’s protocol, when a `POST` changed it: the render
    /// task reconfigures the SPI on `Msg::Protocol`, so the server arm has
    /// to send it (this module is not `async`).
    pub proto: Option<u8>,
    /// False when the Layout applied live but the store refused it (an OTA
    /// holds the flash lease) — the reply says so.
    pub persisted: bool,
}

/// `POST /api/layout`. On success the Layout is persisted and the projection
/// applied; the returned [`Applied`] carries the edits the caller must make.
/// On failure nothing changed and the body is the `{"ok":false,…,"line":N}`
/// shape.
pub fn set_from_wire(body: &str) -> Result<Applied, String> {
    let cur = current();
    let pixels_now = crate::shared::PIXEL_COUNT.load(Ordering::Relaxed);
    let edit = match luxel_core::layout::parse(body, &cur, pixels_now, &limits_strict(true)) {
        Ok(e) => e,
        Err(e) => {
            let mut out = String::new();
            luxel_core::layout::push_error_json(&mut out, &e);
            return Err(out);
        }
    };
    let proj_changed = edit.layout.proj != cur.proj;
    // The first output IS the strip this board drives, so its protocol and
    // colour order write through to the live settings the aliases expose —
    // one source of truth, as the ticket requires. Its pin is persisted and
    // binds at boot (the SPI driver binds MOSI once), like `/api/datapin`.
    #[allow(unused_mut)] // a panel board refuses `out` lines, so nothing writes these
    let mut data_pin = None;
    #[allow(unused_mut)]
    let mut proto = None;
    #[cfg(not(feature = "hub75"))]
    if let Some(o) = edit.layout.outputs.iter().find(|o| o.n == 0) {
        crate::shared::COLOR_ORDER.store(o.order, Ordering::Relaxed);
        if crate::shared::WANT_PROTOCOL.load(Ordering::Relaxed) != o.proto {
            crate::shared::WANT_PROTOCOL.store(o.proto, Ordering::Relaxed);
            proto = Some(o.proto);
        }
        if crate::shared::DATA_PIN.load(Ordering::Relaxed) != o.pin {
            crate::shared::set_want_data_pin(Some(o.pin));
            data_pin = Some(o.pin);
        }
    }
    let persisted = store(edit.layout, edit.pixels.unwrap_or(pixels_now));
    if proj_changed {
        PROJ_DIRTY.store(true, Ordering::Relaxed);
    }
    Ok(Applied {
        pixels: edit.pixels,
        map: edit.map,
        reboot_required: edit.reboot_required,
        data_pin,
        proto,
        persisted,
    })
}

/// `{"ok":true,"reboot_required":B,…}` — the POST answer IS the GET body,
/// so a client never has to re-fetch.
pub fn ok_json(reboot_required: bool, pixels: Option<u32>) -> String {
    json(pixels, Some(reboot_required))
}

/// Load the persisted Layout, else the board's own. Call after
/// `patterns::init()` and `devicemap::init()` — a device that predates this
/// endpoint has no record, and its kind is then read off the map that IS
/// installed, so its first `GET /api/layout` is already right.
#[inline(never)]
pub fn init() {
    let mut l = board_default();
    if crate::devicemap::source() == luxel_core::caps::DeviceMap::User {
        l.kind = LayoutKind::Map;
    }
    // The stored form is the POST wire, re-parsed against the board default
    // (`patterns.rs`'s no-migration rule: a body this build cannot read just
    // leaves the default standing). `pixels` and `map` come back too and are
    // dropped — they are already loaded from their own records.
    if let Some(b) = patterns::read_blob(patterns::LAYOUT_KEY) {
        if let Ok(text) = core::str::from_utf8(&b) {
            let pixels = crate::shared::PIXEL_COUNT.load(Ordering::Relaxed);
            match luxel_core::layout::parse(text, &l, pixels, &limits_strict(false)) {
                Ok(edit) => l = edit.layout,
                Err(e) => println!("layout: stored record rejected ({}); board default", e.msg),
            }
        }
    }
    println!("layout: {}", l.kind.as_str());
    LAYOUT.lock(|c| *c.borrow_mut() = Some(l));
    PROJ_DIRTY.store(true, Ordering::Relaxed);
}
