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
//!
//! The `panel` line is three-quarters boot-built too — `planes` sizes the DMA
//! framebuffer, `clock_mhz` is the LCD_CAM's and `chip` is a register init on
//! the raw pins — but **`blank` applies live** (Gitea #778): it is control bits
//! in the framebuffer words, which the packer never writes, so this module
//! hands the value to the output task (`hub75::want_blank`) and the next frame
//! re-formats the buffer it composes into. That is also why the value is
//! validated HERE against the running row block: a blanking wide enough to
//! swallow the OE window is a legal line that would black the panel out, and
//! nothing reboots to catch it.

use alloc::string::String;
use core::cell::RefCell;
use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use esp_println::println;
use luxel_core::layout::{
    Heal, Layout, LayoutKind, Limits, Matrix, Output, PanelDriver, Reverted, Run, View,
};
use luxel_core::projection::Projection;

use crate::leds::Protocol;
use crate::patterns;

type Shared<T> = BlockingMutex<CriticalSectionRawMutex, RefCell<T>>;

static LAYOUT: Shared<Option<Layout>> = BlockingMutex::new(RefCell::new(None));
/// The projection the render task is to install on the next frame — the ONE
/// live-projection path on this device (Gitea #470/#598). `0..=6` is a
/// [`ProjectionMode`] OVERRIDE for the running pattern (a playlist item's
/// `P`, or a `proj` line on `POST /api/layout`); [`PROJ_DEFAULTS`] means the
/// Layout's own `proj1d/2d/3d`; `PROJ_NONE` means nothing is pending. The map
/// has its own flag in `devicemap`.
static PROJ_PENDING: AtomicU8 = AtomicU8::new(PROJ_DEFAULTS);

/// [`want_projection`]: install the Layout's own defaults (no override).
pub const PROJ_DEFAULTS: u8 = 0xFE;
/// Nothing pending. Not a [`ProjectionMode`] code (those are 0..=6), so
/// `ProjectionMode::from_u8` tells all three cases apart on its own.
const PROJ_NONE: u8 = 0xFF;

/// The boot self-heal's revert record, cached out of flash at [`init`] so a
/// status poll costs no fenced read (Gitea #822). `0` in [`REVERT_FROM`] = no
/// revert; that doubles as the "revert at most once per stored shape" marker,
/// because the pixel count IS what identifies the shape that was thrown away.
/// The largest free block a boot must still have once it is up, or the
/// stored layout is judged to have starved it (Gitea #822). The web server
/// needs a 4 KB connection buffer per slot and ~8 KB for a status body; a
/// healthy 64x64 Seengreat reads 35–48 KB here, the starved one a few KB.
const HEAL_LARGEST_FLOOR: usize = 12 * 1024;

static REVERT_FROM: AtomicU32 = AtomicU32::new(0);
/// Free heap at the end of the boot that reverted — see [`REVERT_FROM`].
static REVERT_HEAP: AtomicU32 = AtomicU32::new(0);

/// The revert this device is reporting on `GET /api/layout`, if any.
pub fn reverted() -> Option<Reverted> {
    match REVERT_FROM.load(Ordering::Relaxed) {
        0 => None,
        from_pixels => {
            Some(Reverted { from_pixels, heap_free: REVERT_HEAP.load(Ordering::Relaxed) })
        }
    }
}

/// `/api/status`'s `layout_reverted` — the one-bit form, so a client polling
/// status notices without fetching the Layout.
pub fn was_reverted() -> bool {
    REVERT_FROM.load(Ordering::Relaxed) != 0
}

/// Persist (or clear) the revert record and its cache. `None` writes eight
/// zero bytes rather than removing the key — a fixed-size record has no
/// "absent" state to get wrong, and the key area is swept wholesale by the
/// migrator either way.
fn set_reverted(r: Option<Reverted>) {
    let (from, heap) = r.map_or((0, 0), |r| (r.from_pixels, r.heap_free));
    REVERT_FROM.store(from, Ordering::Relaxed);
    REVERT_HEAP.store(heap, Ordering::Relaxed);
    let mut rec = [0u8; 8];
    rec[..4].copy_from_slice(&from.to_le_bytes());
    rec[4..].copy_from_slice(&heap.to_le_bytes());
    if !patterns::store_blob(patterns::LAYOUT_REVERT_KEY, &rec) {
        println!("layout: could not persist the revert record");
    }
}

/// Load the cache from flash. Call from [`init`], after `patterns::init()`.
fn load_reverted() {
    let Some(b) = patterns::read_blob(patterns::LAYOUT_REVERT_KEY) else { return };
    if b.len() < 8 {
        return;
    }
    let n = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    REVERT_FROM.store(n(0), Ordering::Relaxed);
    REVERT_HEAP.store(n(4), Ordering::Relaxed);
}

/// Pixels the SHAPE of `l` describes — a matrix Layout's own area, else the
/// live pixel count, which is what a strip or a map Layout's extent is.
///
/// This is the number the self-heal compares, because it is the one that
/// scales every per-pixel allocation on the board: the framebuffers on a
/// panel, the frame and the engine's pixel state everywhere.
fn shape_pixels(l: &Layout, pixels_now: u32) -> u32 {
    match l.kind {
        LayoutKind::Matrix => l.matrix.width().saturating_mul(l.matrix.height()),
        _ => pixels_now,
    }
}

/// The self-heal's decision at these heap readings, with the shapes it was
/// made from — the side-effect-free half of [`heal_if_starved`].
fn heal_verdict(free: usize, largest: usize) -> (Heal, u32, u32, Layout, Layout) {
    let cur = current();
    let def = board_default();
    let pixels_now = crate::shared::PIXEL_COUNT.load(Ordering::Relaxed);
    let stored_px = shape_pixels(&cur, pixels_now);
    let default_px = shape_pixels(&def, pixels_now);
    let at_default = cur.kind == def.kind && cur.matrix == def.matrix;
    let decision = luxel_core::layout::heal_decision_fragmented(
        free,
        luxel_core::budget::RUNTIME_FLOOR,
        largest,
        HEAL_LARGEST_FLOOR,
        stored_px,
        default_px,
        at_default,
        REVERT_FROM.load(Ordering::Relaxed),
    );
    (decision, stored_px, default_px, def, cur)
}

/// Would [`heal_if_starved`] revert at these readings? No side effects — the
/// early check in `main.rs` asks this first so it can hand the boot's heal
/// reserve back before the revert has to persist anything.
pub fn heal_if_starved_early(free: usize, largest: usize) -> bool {
    matches!(heal_verdict(free, largest).0, Heal::Revert)
}

/// The boot self-heal (Gitea #822): a stored layout the heap cannot serve must
/// not be able to make the board unreachable for ever.
///
/// Called once, where the boot-loop guard decides this image is healthy
/// (`main.rs`, ~60 s in — WiFi up, web up, engine built, the playlist or the
/// resumed pattern loaded), with the free internal heap measured there.
/// Returns true when the caller must reboot.
///
/// **Why this and not a POST-time check alone.** `crate::hub75::boot_cost`
/// predicts the PANEL's share of internal RAM and `POST /api/layout` refuses a
/// body whose panel would not leave room — but what the engine, the
/// compositor, the JIT and the protocol encode buffers cost at a given pixel
/// count is not modelled anywhere, and deliberately is not guessed at. So the
/// authoritative guard is this one: it measures the heap a whole real boot
/// ended with, and if the stored shape is what starved it, that shape goes.
///
/// Two yardsticks, because total free heap alone missed the real case: on
/// 2026-09-27 the same 2x1 stored on the two-buffer build left the board
/// answering 503 on EVERY route — `POST /api/reboot` and `/api/ota`
/// included, and the boot-loop guard counted the boots as healthy — while
/// `HEAP.free()` sat above the 20 KB floor. The heap was fragmented past the
/// web server's 4 KB connection buffer. So the largest free block is checked
/// too, against [`HEAL_LARGEST_FLOOR`].
///
/// On 2026-09-26 a stored `matrix 64 64 2 1` (8192 px) on the Seengreat left
/// the board answering 503 or hanging on every route — `POST /api/layout`
/// could not complete a store, `POST /api/reboot` never landed, and an OTA of
/// a corrected image booted straight back into the same stored layout. Three
/// physical power cycles to trip the boot-loop guard were the only way out.
///
/// The three [`Heal`] refusals are each a boot loop avoided; see
/// [`luxel_core::layout::heal_decision`], which is where that logic is tested.
///
/// **What it can revert.** The check runs on every board, but the shape it can
/// fall back to is the board default's, and a strip board's default Layout
/// carries no pixel count of its own ([`shape_pixels`]) — so there the two
/// counts are equal and the answer is [`Heal::NoSmaller`] unless a `matrix`
/// line is what set the count. That is the honest answer, not a gap being
/// papered over: on a strip board the expensive stored number is the pixel
/// count in the nvs device record, which `board::MAX_PIXELS` and the protocol
/// encode buffer's own allocation already bound.
pub fn heal_if_starved(free: usize, largest: usize) -> bool {
    let (decision, stored_px, default_px, def, cur) = heal_verdict(free, largest);
    let why = match decision {
        Heal::Healthy => return false,
        Heal::AtDefault => "already the board default",
        Heal::NoSmaller => "the board default is no smaller",
        Heal::AlreadyReverted => "already reverted once from this shape",
        Heal::Revert => {
            println!(
                "layout: {} px left the heap at {} B free / {} B largest after boot (floors {} / \
                 {}) — reverting to the board default",
                stored_px,
                free,
                largest,
                luxel_core::budget::RUNTIME_FLOOR,
                HEAL_LARGEST_FLOOR
            );
            // The board default's SHAPE, everything else the user configured
            // kept: the `panel` driver line (planes/clock/chip/blank/lsb), the
            // outputs table and the projection defaults cost no memory that
            // scales with the pixel count, and throwing them away would make
            // the self-heal worse than the problem.
            let next = Layout {
                kind: def.kind,
                matrix: def.matrix,
                // the shape changed, so a per-panel chain for it cannot survive
                chain: alloc::vec::Vec::new(),
                outputs: cur.outputs.clone(),
                proj: cur.proj,
                driver: cur.driver,
            };
            // The layout store FIRST, and the marker only once it landed: on
            // the starved board this exists for, a store can fail, and a
            // marker without a stored default would read as "already
            // reverted" on the next boot and leave the board starved for
            // good. A failed store is logged and the next boot retries.
            if !store(next, default_px) {
                println!("layout: the revert could not persist the board default — next boot retries");
                return false;
            }
            // The stored Layout carries no pixel count — it lives in the nvs
            // device record, and that is what the next boot's engine is built
            // from (`main.rs`). Reverting the shape without it would give a
            // board-default panel still rendering 8192 px, which is half a
            // fix. The WANT_* atomic is what `device_config_snapshot` reads.
            crate::shared::WANT_PIXEL_COUNT.store(default_px, Ordering::Relaxed);
            if let Err(e) = crate::config::write_device(&crate::shared::device_config_snapshot()) {
                println!("layout: revert could not persist the pixel count ({e})");
            }
            // The `matrix` POST that stored this shape also installed its grid
            // as a USER map (`set_from_wire`), and that map outlives the
            // revert: the first healed boot on 2026-09-27 came up 64x64 with a
            // 128x64 user map still installed (`geom.source: user`, a
            // scrambled picture). An empty body is `POST /api/map`'s "clear":
            // a panel board falls back to its own grid.
            if crate::devicemap::source() == luxel_core::caps::DeviceMap::User {
                let _ = crate::devicemap::set_from_wire("");
            }
            // The marker last, so a stored default and a boot that is still
            // starved cannot revert twice from the same shape.
            set_reverted(Some(Reverted {
                from_pixels: stored_px,
                heap_free: free.min(u32::MAX as usize) as u32,
            }));
            return true;
        }
    };
    println!(
        "layout: {} B of heap left after boot (floor {}) — {}, staying at {} px",
        free,
        luxel_core::budget::RUNTIME_FLOOR,
        why,
        stored_px
    );
    false
}

/// The Layout a board with nothing stored comes up in: a HUB75 board IS its
/// panel, a strip board is its strip — and either becomes `map` the moment
/// someone installs one, so an existing device that has only ever used
/// `POST /api/map` reads correctly on first contact with this endpoint.
fn board_default() -> Layout {
    #[cfg(feature = "hub75")]
    let l = Layout::board_default(LayoutKind::Matrix, crate::hub75::board_default_matrix());
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

/// The chain as the remap walks it (Gitea #920): the explicit `chain` line's
/// tiles when one is stored, else the rule's. Allocates the list; called at
/// boot and on an arrangement change, never per frame.
#[cfg(feature = "hub75")]
pub fn tiles() -> alloc::vec::Vec<luxel_core::layout::Tile> {
    LAYOUT.lock(|c| c.borrow().as_ref().map_or_else(|| board_default().tiles(), |l| l.tiles()))
}

/// `POST /api/layout/card` (Gitea #920): the test card to draw instead of
/// the pattern. `Err` is the `{"ok":false,…}` body.
#[cfg(feature = "hub75")]
pub fn set_card(body: &str) -> Result<String, String> {
    let t = body.trim();
    // `{"mode":"panels"}` or the bare word
    let word = match t.find("\"mode\"") {
        Some(i) => {
            let rest = &t[i + 6..];
            rest.find('"').and_then(|q| {
                let v = &rest[q + 1..];
                v.find('"').map(|e| &v[..e])
            })
        }
        None => Some(t),
    };
    match word.and_then(luxel_core::layout::Card::from_str) {
        Some(c) => {
            crate::hub75::set_card(c);
            Ok(alloc::format!("{{\"ok\":true,\"card\":\"{}\"}}", c.as_str()))
        }
        None => Err(String::from("{\"ok\":false,\"error\":\"mode must be off, panels or cells\"}")),
    }
}

/// The configured panel driver — the `panel` wire line (#525): bit depth,
/// pixel clock, driver chip and latch blanking. What the HUB75 boot builds
/// its framebuffer and its control template from, and what
/// `GET /api/layout`'s `driver` block reports as CONFIGURED against the
/// `live` values the running DMA actually has. Meaningless on a strip board,
/// which never asks.
pub fn driver() -> PanelDriver {
    LAYOUT.lock(|c| c.borrow().as_ref().map_or_else(|| board_default().driver, |l| l.driver))
}

/// The projection defaults to install on every engine (boot and rebuild).
pub fn projection() -> Projection {
    LAYOUT.lock(|c| c.borrow().as_ref().map_or(Projection::DEFAULT, |l| l.proj))
}

/// Ask the render task to install a projection on the next frame: a
/// [`ProjectionMode`] code for an override, or [`PROJ_DEFAULTS`] for the
/// Layout's own triple.
pub fn want_projection(code: u8) {
    PROJ_PENDING.store(code, Ordering::Relaxed);
}

/// Consume that request (the render task calls this each frame). load+store
/// rather than `swap`: rv32imc has no atomic RMW, and the same reasoning as
/// `devicemap::take_dirty` applies.
pub fn take_projection() -> Option<u8> {
    let want = PROJ_PENDING.load(Ordering::Relaxed);
    if want == PROJ_NONE {
        return None;
    }
    PROJ_PENDING.store(PROJ_NONE, Ordering::Relaxed);
    Some(want)
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
        reverted: reverted(),
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
    // Latch blanking applies LIVE on a panel board (#778), so it is judged
    // against the RUNNING row block here rather than trusted until a boot
    // re-checks it: a blanking that swallows the whole OE window is a legal
    // `panel` line that would simply black the panel out, and the boot-time
    // `template_lights` check cannot save a device that never reboots.
    #[cfg(feature = "hub75")]
    if edit.driver_set {
        if let Some((latch, cols)) = crate::hub75::blank_would_darken(edit.layout.driver.blank) {
            // the same `{"ok":false,"error":…,"line":N}` shape the core writes,
            // with numbers in the message the core's `&'static str` cannot hold
            return Err(alloc::format!(
                "{{\"ok\":false,\"error\":\"panel: blank {} + {} latch clocks leave no lit clock \
                 in a {}-word row block\",\"line\":{}}}",
                edit.layout.driver.blank,
                latch,
                cols,
                wire_line_no(body, "panel"),
            ));
        }
        crate::hub75::want_blank(edit.layout.driver.blank);
    }
    // Would the panel this body asks for leave a boot any heap? (Gitea #822.)
    // The measurement is this boot's own — free internal heap at the top of
    // `try_boot`, before a single panel byte was allocated — so the answer is
    // about THIS board, not a table. Only when the panel's inputs actually
    // moved: re-posting an unchanged arrangement must not be refused by a
    // floor the running configuration already sits under.
    #[cfg(feature = "hub75")]
    if edit.layout.matrix != cur.matrix || edit.driver_set {
        let before = crate::hub75::heap_before_panel();
        let floor = crate::hub75::boot_heap_floor();
        if let Some(cost) = crate::hub75::boot_cost(&edit.layout.matrix, &edit.layout.driver) {
            let left = before.saturating_sub(cost);
            // `before == 0` = the panel never booted (a `LUXEL_NO_OTA` build,
            // or a POST that somehow beat the wiring): nothing measured,
            // nothing predicted, nothing refused.
            if before > 0 && left < floor {
                return Err(alloc::format!(
                    "{{\"ok\":false,\"error\":\"this panel would leave {} B of heap at boot \
                     (floor {} B) — it cannot be driven on this board\",\"line\":{}}}",
                    left,
                    floor,
                    wire_line_no(
                        body,
                        if edit.layout.matrix != cur.matrix { "matrix" } else { "panel" }
                    ),
                ));
            }
        }
    }
    // A `proj` line is the RUNNING pattern's override and outranks the
    // defaults in the same body; without one, a changed default is itself
    // what the next frame installs (#598).
    let proj_now = edit.proj_now;
    let defaults_changed = edit.layout.proj != cur.proj;
    // The arrangement is a table the output task can swap between frames
    // (Gitea #920): when only WHICH panel sits where / how it is turned
    // moved — not the framebuffer's shape — ask for the swap. That is the
    // same test the core used to answer `reboot_required: false`.
    #[cfg(feature = "hub75")]
    let swap_remap = edit.layout.kind == luxel_core::layout::LayoutKind::Matrix
        && cur.arrangement_differs(&edit.layout)
        && !cur.fb_geometry_changed(&edit.layout);
    let persisted = store(edit.layout, edit.pixels.unwrap_or(pixels_now));
    #[cfg(feature = "hub75")]
    if swap_remap {
        crate::hub75::want_remap();
    }
    // A successful edit is the user having seen (or at least overwritten) the
    // self-heal's verdict — the record has done its job (Gitea #822). Only
    // written when there is one, so the ordinary POST costs no flash.
    if was_reverted() {
        set_reverted(None);
    }
    match proj_now {
        Some(o) => want_projection(o.map_or(PROJ_DEFAULTS, |m| m.as_u8())),
        None if defaults_changed => want_projection(PROJ_DEFAULTS),
        None => {}
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

/// The 1-based line `verb` is on, numbered exactly as
/// `luxel_core::layout::parse` numbers its own errors (raw lines, blanks
/// included). 1 when there is none — the refusals above only run on a body
/// that carried the line they name.
#[cfg(feature = "hub75")]
fn wire_line_no(body: &str, verb: &str) -> u32 {
    body.lines()
        .position(|l| l.trim().split_whitespace().next() == Some(verb))
        .map_or(1, |i| i as u32 + 1)
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
    load_reverted();
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
    // A panel board's BOARD device map is its panel's grid, and the panel is
    // a stored setting now (#401) — so the grid `devicemap::init()` installed
    // from the board DEFAULT has to follow the configured arrangement. Only
    // acts when no user map is installed, and only when it differs.
    #[cfg(feature = "hub75")]
    if l.kind == LayoutKind::Matrix {
        crate::devicemap::refresh_board_grid(l.matrix.width() as u16, l.matrix.height() as u16);
    }
    LAYOUT.lock(|c| *c.borrow_mut() = Some(l));
    want_projection(PROJ_DEFAULTS);
}
