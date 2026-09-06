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
use core::sync::atomic::{AtomicBool, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use esp_println::println;
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

/// The geometry a board knows it has, used when nothing is stored: a HUB75
/// panel board IS a `PANEL_COLS`×`PANEL_ROWS` grid. Strips have none.
fn board_default() -> Option<MapData> {
    #[cfg(feature = "hub75")]
    {
        Some(MapData::Grid {
            w: crate::hub75::PANEL_COLS as u16,
            h: crate::hub75::PANEL_ROWS as u16,
        })
    }
    #[cfg(not(feature = "hub75"))]
    {
        None
    }
}

pub fn has_map() -> bool {
    MAP.lock(|c| c.borrow().is_some())
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
pub fn apply(engine: &mut Engine) {
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

fn parse(body: &str) -> Option<MapData> {
    let mut it = body.split_whitespace();
    let first = it.next()?;
    if first == "grid" {
        let w: u16 = it.next()?.parse().ok()?;
        let h: u16 = it.next()?.parse().ok()?;
        if w == 0 || h == 0 {
            return None;
        }
        return Some(MapData::Grid { w, h });
    }
    let dims: u8 = first.parse().ok()?;
    if dims < 2 || dims > 3 {
        return None;
    }
    let vals: Vec<i32> = it.filter_map(|v| v.parse().ok()).collect();
    let n = vals.len() / dims as usize;
    if n == 0 {
        return None;
    }
    let mut coords = Vec::with_capacity(n);
    for i in 0..n {
        let mut c = [Fx::ZERO; 3];
        for d in 0..dims as usize {
            c[d] = Fx::from_raw(vals[i * dims as usize + d]);
        }
        coords.push(c);
    }
    Some(MapData::Coords { dims, coords })
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
    let mut coords = Vec::with_capacity(count);
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

/// `POST /api/map`. Returns (installed, count). Empty/invalid → clears
/// (a panel board goes back to its own grid, which still counts as
/// installed).
pub fn set_from_wire(body: &str) -> (bool, usize) {
    match parse(body) {
        Some(m) => {
            let count = m.count();
            let persisted = patterns::store_blob(patterns::MAP_KEY, &serialize(&m));
            if !persisted {
                println!("map: too large to persist ({} px) — applied live only", count);
            }
            MAP.lock(|c| *c.borrow_mut() = Some(m));
            DIRTY.store(true, Ordering::Relaxed);
            (true, count)
        }
        None => {
            let fallback = board_default();
            let out = fallback.as_ref().map_or((false, 0), |m| (true, m.count()));
            MAP.lock(|c| *c.borrow_mut() = fallback);
            let _ = patterns::store_blob(patterns::MAP_KEY, &[0u8]); // invalid → treated as none
            DIRTY.store(true, Ordering::Relaxed);
            out
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
            DIRTY.store(true, Ordering::Relaxed);
            return;
        }
    }
    if let Some(m) = board_default() {
        if let MapData::Grid { w, h } = &m {
            println!("map: {}x{} grid (board default)", w, h);
        }
        MAP.lock(|c| *c.borrow_mut() = Some(m));
        DIRTY.store(true, Ordering::Relaxed);
    }
}
