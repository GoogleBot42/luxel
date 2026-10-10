//! Device pixel map: install a computed 2D/3D map (from the playground's map
//! program) or a procedural grid onto the device so its patterns render with
//! real geometry (render2D). The map is applied to the render engine after
//! every rebuild and persisted in flash (a compact binary blob) so it
//! survives reboots.
//!
//! Wire (POST /api/map):
//!   `<dims> <v0> <v1> …`  — dims (2|3) then dims raw-16.16 values per pixel
//!                            (mirrors the wasm lx_set_map).
//!   `grid <w> <h>`         — a procedural row-major grid: zero heap on the
//!                            device, no per-pixel body (Gitea #258). A 64x64
//!                            panel's map is 48 KB as coordinates — the whole
//!                            idle heap on the S3 panel board — and 5 bytes
//!                            here.
//!   empty/invalid          — clears the map. A panel board falls back to its
//!                            own geometry (`board_default`), a strip to 1D.
//!
//! Panel boards (`hub75`) install their grid at boot when nothing is stored,
//! so every pattern — not only the render2D-only ones the engine's default
//! grid covers — sees the panel as a matrix.

use alloc::vec::Vec;
use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use esp_println::println;
use luxel_core::caps::DeviceMap;
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::jsonview::{push_piece, push_u32};

use crate::patterns;

enum MapData {
    Coords { dims: u8, coords: Vec<[Fx; 3]> },
    Grid { w: u16, h: u16 },
}

impl MapData {
    fn dims(&self) -> u8 {
        match self {
            MapData::Coords { dims, .. } => *dims,
            MapData::Grid { .. } => 2,
        }
    }
    fn count(&self) -> usize {
        match self {
            MapData::Coords { coords, .. } => coords.len(),
            MapData::Grid { w, h } => *w as usize * *h as usize,
        }
    }
}

/// Blob tag for a persisted grid (a coordinate blob starts with dims 2|3).
const GRID_TAG: u8 = 0x80;

type Shared<T> = BlockingMutex<CriticalSectionRawMutex, RefCell<T>>;
static MAP: Shared<Option<MapData>> = BlockingMutex::new(RefCell::new(None));
/// Set when the map changed; the render task applies it on the next frame.
static DIRTY: AtomicBool = AtomicBool::new(false);
/// Where `MAP` came from, as `luxel_core::caps::DeviceMap` discriminants
/// (0 = None, 1 = Board, 2 = User) — `/api/status`'s `geom.source` has to
/// tell a real 64x64 panel from a map the user installed. Written only
/// alongside `MAP`.
static SOURCE: AtomicU8 = AtomicU8::new(0);

fn set_source(s: DeviceMap) {
    SOURCE.store(
        match s {
            DeviceMap::None => 0,
            DeviceMap::Board => 1,
            DeviceMap::User => 2,
        },
        Ordering::Relaxed,
    );
}

/// Where the installed device map came from (see `SOURCE`).
pub fn source() -> DeviceMap {
    match SOURCE.load(Ordering::Relaxed) {
        2 => DeviceMap::User,
        1 => DeviceMap::Board,
        _ => DeviceMap::None,
    }
}

/// The geometry a board knows it has, used when nothing is stored: a HUB75
/// panel board IS its panel's grid. Strips have none.
///
/// This is the board's DEFAULT panel — a configured one can be any shape
/// since #401, and [`refresh_board_grid`] widens this to it as soon as the
/// Layout is loaded (which is one call later in `main`; see its docs).
fn board_default() -> Option<MapData> {
    #[cfg(feature = "hub75")]
    {
        Some(MapData::Grid { w: crate::hub75::DEFAULT_PANEL_W, h: crate::hub75::DEFAULT_PANEL_H })
    }
    #[cfg(not(feature = "hub75"))]
    {
        None
    }
}

/// Re-derive the BOARD grid from the CONFIGURED panel (#401).
///
/// Panel geometry is a stored setting now, so the grid a panel board installs
/// when the user has installed no map of their own has to follow it —
/// otherwise a `matrix 128 64 …` Layout renders patterns into a 64x64 grid
/// while 8192 pixels exist.
///
/// Called from `layout::init()`, which runs immediately after [`init`] here.
/// The two cannot simply be reordered: this module's [`source`] is what tells
/// the Layout whether a user map has made its kind `map`. No-op unless the
/// installed map IS the board's own, and silent unless it actually changes —
/// so a default-shaped panel prints exactly the one map line it always did.
#[cfg(feature = "hub75")]
pub fn refresh_board_grid(w: u16, h: u16) {
    if !matches!(source(), DeviceMap::Board) || w == 0 || h == 0 {
        return;
    }
    let changed = MAP.lock(|c| {
        let mut m = c.borrow_mut();
        if let Some(MapData::Grid { w: gw, h: gh }) = m.as_ref() {
            if (*gw, *gh) == (w, h) {
                return false;
            }
        }
        *m = Some(MapData::Grid { w, h });
        true
    });
    if changed {
        println!("map: {}x{} grid (configured panel)", w, h);
        DIRTY.store(true, Ordering::Relaxed);
    }
}

pub fn has_map() -> bool {
    MAP.lock(|c| c.borrow().is_some())
}

/// The installed map's shape as `(dims, regular grid)` — what `/api/status`
/// reports while NO engine is resident (frozen for an OTA, or a boot decode
/// that failed). With an engine the effective geometry comes from the engine
/// instead, because only it knows about the fabricated square grid.
pub fn shape() -> (u8, Option<luxel_core::outpipe::GridMap>) {
    // the grid the engine would install: tiled when it covers the panel
    // chain (Gitea #948), so a compositor re-pointed at it walks the frame
    // in the same wire order its layers write
    let wire = crate::layout::wire_tiling();
    MAP.lock(|c| match c.borrow().as_ref() {
        Some(MapData::Grid { w, h }) => (
            2,
            Some(match wire {
                Some(t) if (t.width(), t.height()) == (*w, *h) => luxel_core::outpipe::GridMap::tiled(t),
                _ => luxel_core::outpipe::GridMap::new(*w, *h, false),
            }),
        ),
        Some(MapData::Coords { dims, coords }) => {
            (*dims, luxel_core::outpipe::detect_grid(*dims, coords))
        }
        None => (0, None),
    })
}

/// Consume the "map changed" flag (render task calls this each frame).
/// load+store, not `swap`: rv32imc (the C3) has no atomic RMW. The gap is
/// harmless — the only writer of `true` re-marks dirty, so a lost flag is
/// re-set; the render task is the sole consumer.
pub fn take_dirty() -> bool {
    let was = DIRTY.load(Ordering::Relaxed);
    if was {
        DIRTY.store(false, Ordering::Relaxed);
    }
    was
}

/// Re-apply the map on the next frame (e.g. after the engine was rebuilt).
pub fn mark_dirty() {
    DIRTY.store(true, Ordering::Relaxed);
}

/// Apply the installed map to a freshly-built engine (no-op if none).
///
/// On a panel board the engine is told the chain FIRST (Gitea #948): a grid
/// of the chain's extent then installs tiled and a coordinate map is
/// permuted into wire order, so the frame the engine writes is the order the
/// HUB75 driver clocks out — there is no remap after it. A live arrangement
/// change (`POST /api/layout`, #920) re-runs this through `mark_dirty`.
pub fn apply(engine: &mut Engine) {
    engine.set_wire_tiling(crate::layout::wire_tiling());
    MAP.lock(|c| match c.borrow().as_ref() {
        Some(MapData::Grid { w, h }) => engine.set_grid_map(*w, *h),
        Some(MapData::Coords { dims, coords }) => {
            if !engine.set_map(*dims, coords) {
                println!(
                    "map: not applied — out of memory for {} px (pattern runs 1D)",
                    coords.len()
                );
            }
        }
        None => {}
    });
}

/// Why a body did not become a map.
enum MapErr {
    /// Not a map at all — empty, or unparseable. The caller CLEARS the
    /// installed map (a panel board falls back to its own grid).
    Invalid,
    /// A well-formed coordinate map the heap cannot hold: `(pixels, bytes)`
    /// (Gitea #768). Distinct from [`MapErr::Invalid`] because clearing the
    /// map is the wrong answer here — the body was valid and the device is
    /// out of memory, which the client has to be told rather than shown as a
    /// silent revert to the board grid.
    NoHeap(usize, usize),
}

/// Bytes a coordinate map of `n` pixels holds — always three `Fx` per pixel
/// (a 2D map leaves z at zero), which is what `MapData::Coords` stores.
fn coords_bytes(n: usize) -> usize {
    n * core::mem::size_of::<[Fx; 3]>()
}

/// `Vec<[Fx; 3]>` of `n` pixels, or the byte count it wanted (Gitea #768).
/// `Vec::with_capacity` here was an infallible multi-KB allocation on the
/// one path that takes its length from a flash blob rather than a
/// length-capped HTTP body.
fn coords_vec(n: usize) -> Result<Vec<[Fx; 3]>, MapErr> {
    let mut v: Vec<[Fx; 3]> = Vec::new();
    match v.try_reserve_exact(n) {
        Ok(()) => Ok(v),
        Err(_) => Err(MapErr::NoHeap(n, coords_bytes(n))),
    }
}

fn parse(body: &str) -> Result<MapData, MapErr> {
    let mut it = body.split_whitespace();
    let first = it.next().ok_or(MapErr::Invalid)?;
    if first == "grid" {
        let w: u16 = it.next().ok_or(MapErr::Invalid)?.parse().map_err(|_| MapErr::Invalid)?;
        let h: u16 = it.next().ok_or(MapErr::Invalid)?.parse().map_err(|_| MapErr::Invalid)?;
        if w == 0 || h == 0 {
            return Err(MapErr::Invalid);
        }
        return Ok(MapData::Grid { w, h });
    }
    let dims: u8 = first.parse().map_err(|_| MapErr::Invalid)?;
    if dims < 2 || dims > 3 {
        return Err(MapErr::Invalid);
    }
    // Bounded by the HTTP body cap, not by the pixel count — the server
    // never hands this path a body larger than its POST limit.
    let vals: Vec<i32> = it.filter_map(|v| v.parse().ok()).collect();
    let n = vals.len() / dims as usize;
    if n == 0 {
        return Err(MapErr::Invalid);
    }
    let mut coords = coords_vec(n)?;
    for i in 0..n {
        let mut c = [Fx::ZERO; 3];
        for d in 0..dims as usize {
            c[d] = Fx::from_raw(vals[i * dims as usize + d]);
        }
        coords.push(c);
    }
    Ok(MapData::Coords { dims, coords })
}

fn serialize(m: &MapData) -> Vec<u8> {
    match m {
        MapData::Grid { w, h } => {
            let mut buf = Vec::with_capacity(5);
            buf.push(GRID_TAG);
            buf.extend_from_slice(&w.to_le_bytes());
            buf.extend_from_slice(&h.to_le_bytes());
            buf
        }
        MapData::Coords { dims, coords } => {
            let count = coords.len();
            let mut buf = Vec::with_capacity(3 + count * *dims as usize * 4);
            buf.push(*dims);
            buf.extend_from_slice(&(count as u16).to_le_bytes());
            for c in coords {
                for d in 0..*dims as usize {
                    buf.extend_from_slice(&c[d].raw().to_le_bytes());
                }
            }
            buf
        }
    }
}

fn deserialize(b: &[u8]) -> Option<MapData> {
    if b.len() >= 5 && b[0] == GRID_TAG {
        let w = u16::from_le_bytes([b[1], b[2]]);
        let h = u16::from_le_bytes([b[3], b[4]]);
        if w == 0 || h == 0 {
            return None;
        }
        return Some(MapData::Grid { w, h });
    }
    if b.len() < 3 {
        return None;
    }
    let dims = b[0];
    let count = u16::from_le_bytes([b[1], b[2]]) as usize;
    if !(2..=3).contains(&dims) || b.len() < 3 + count * dims as usize * 4 {
        return None;
    }
    // Fallible: `count` comes straight off a flash blob, so this is the one
    // path whose allocation is not bounded by an HTTP body (Gitea #768). A
    // refusal here logs and returns None, and `init` then installs the
    // board's own grid — a panel renders as a panel instead of aborting.
    let mut coords = match coords_vec(count) {
        Ok(v) => v,
        Err(MapErr::NoHeap(n, bytes)) => {
            println!("map: stored {} px map needs {} B — no heap, ignoring it", n, bytes);
            return None;
        }
        Err(MapErr::Invalid) => return None,
    };
    let mut o = 3;
    for _ in 0..count {
        let mut c = [Fx::ZERO; 3];
        for d in 0..dims as usize {
            c[d] = Fx::from_raw(i32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]));
            o += 4;
        }
        coords.push(c);
    }
    Some(MapData::Coords { dims, coords })
}

/// `POST /api/map`. `Ok((installed, count))`; empty/invalid → clears (a panel
/// board goes back to its own grid, which still counts as installed).
///
/// `Err(bytes)` = a valid coordinate map the heap could not hold, with the
/// byte count it wanted (Gitea #768). Nothing changed: the installed map is
/// left alone rather than reverted to the board grid, because the body was
/// not the problem.
pub fn set_from_wire(body: &str) -> Result<(bool, usize), usize> {
    match parse(body) {
        Err(MapErr::NoHeap(n, bytes)) => {
            println!("map: {} px map needs {} B — no heap, refused", n, bytes);
            Err(bytes)
        }
        Ok(m) => {
            let count = m.count();
            let persisted = patterns::store_blob(patterns::MAP_KEY, &serialize(&m));
            if !persisted {
                println!("map: too large to persist ({} px) — applied live only", count);
            }
            MAP.lock(|c| *c.borrow_mut() = Some(m));
            set_source(DeviceMap::User);
            DIRTY.store(true, Ordering::Relaxed);
            Ok((true, count))
        }
        Err(MapErr::Invalid) => {
            let fallback = board_default();
            let out = fallback.as_ref().map_or((false, 0), |m| (true, m.count()));
            set_source(if fallback.is_some() { DeviceMap::Board } else { DeviceMap::None });
            MAP.lock(|c| *c.borrow_mut() = fallback);
            let _ = patterns::store_blob(patterns::MAP_KEY, &[0u8]); // invalid → treated as none
            DIRTY.store(true, Ordering::Relaxed);
            Ok(out)
        }
    }
}

/// `GET /api/map` → {"installed":bool,"dims":D,"count":N,"kind":"grid"|"coords"[,"w":W,"h":H]}.
pub fn to_json() -> alloc::string::String {
    MAP.lock(|c| match c.borrow().as_ref() {
        Some(m) => {
            let mut out = alloc::string::String::new();
            push_piece(&mut out, "{\"installed\":true,\"dims\":");
            push_u32(&mut out, m.dims() as u32);
            push_piece(&mut out, ",\"count\":");
            push_u32(&mut out, m.count() as u32);
            match m {
                MapData::Grid { w, h } => {
                    push_piece(&mut out, ",\"kind\":\"grid\",\"w\":");
                    push_u32(&mut out, *w as u32);
                    push_piece(&mut out, ",\"h\":");
                    push_u32(&mut out, *h as u32);
                }
                MapData::Coords { .. } => push_piece(&mut out, ",\"kind\":\"coords\""),
            }
            push_piece(&mut out, "}");
            out
        }
        None => alloc::string::String::from("{\"installed\":false,\"dims\":0,\"count\":0}"),
    })
}

/// Load the persisted map, else the board's own geometry. Call after
/// patterns::init().
pub fn init() {
    if let Some(b) = patterns::read_blob(patterns::MAP_KEY) {
        if let Some(m) = deserialize(&b) {
            match &m {
                MapData::Grid { w, h } => println!("map: {}x{} grid from flash", w, h),
                MapData::Coords { dims, coords } => {
                    println!("map: {} px ({}D) from flash", coords.len(), dims)
                }
            }
            MAP.lock(|c| *c.borrow_mut() = Some(m));
            set_source(DeviceMap::User);
            DIRTY.store(true, Ordering::Relaxed);
            return;
        }
    }
    if let Some(m) = board_default() {
        if let MapData::Grid { w, h } = &m {
            println!("map: {}x{} grid (board default)", w, h);
        }
        MAP.lock(|c| *c.borrow_mut() = Some(m));
        set_source(DeviceMap::Board);
        DIRTY.store(true, Ordering::Relaxed);
    }
}
