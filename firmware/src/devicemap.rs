//! Device pixel map: install a computed 2D/3D map (from the playground's map
//! program) onto the device so its patterns render with real geometry
//! (render2D). The map is applied to the render engine after every rebuild and
//! persisted in flash (a compact binary blob) so it survives reboots.
//!
//! Wire (POST /api/map, raw 16.16, mirrors the wasm lx_set_map):
//!   `<dims> <v0> <v1> …`  — dims (2|3) then dims values per pixel.
//!   An empty/invalid body clears the map (patterns render 1D again).

use alloc::vec::Vec;
use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use esp_println::println;
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;

use crate::patterns;

struct MapData {
    dims: u8,
    coords: Vec<[Fx; 3]>,
}

type Shared<T> = BlockingMutex<CriticalSectionRawMutex, RefCell<T>>;
static MAP: Shared<Option<MapData>> = BlockingMutex::new(RefCell::new(None));
/// Set when the map changed; the render task applies it on the next frame.
static DIRTY: AtomicBool = AtomicBool::new(false);

pub fn has_map() -> bool {
    MAP.lock(|c| c.borrow().is_some())
}

/// Consume the "map changed" flag (render task calls this each frame).
pub fn take_dirty() -> bool {
    DIRTY.swap(false, Ordering::Relaxed)
}

/// Re-apply the map on the next frame (e.g. after the engine was rebuilt).
pub fn mark_dirty() {
    DIRTY.store(true, Ordering::Relaxed);
}

/// Apply the installed map to a freshly-built engine (no-op if none).
pub fn apply(engine: &mut Engine) {
    MAP.lock(|c| {
        if let Some(m) = c.borrow().as_ref() {
            engine.set_map(m.dims, &m.coords);
        }
    });
}

fn parse(body: &str) -> Option<MapData> {
    let mut it = body.split_whitespace();
    let dims: u8 = it.next()?.parse().ok()?;
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
    Some(MapData { dims, coords })
}

fn serialize(m: &MapData) -> Vec<u8> {
    let count = m.coords.len();
    let mut buf = Vec::with_capacity(3 + count * m.dims as usize * 4);
    buf.push(m.dims);
    buf.extend_from_slice(&(count as u16).to_le_bytes());
    for c in &m.coords {
        for d in 0..m.dims as usize {
            buf.extend_from_slice(&c[d].raw().to_le_bytes());
        }
    }
    buf
}

fn deserialize(b: &[u8]) -> Option<MapData> {
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
    Some(MapData { dims, coords })
}

/// `POST /api/map`. Returns (installed, count). Empty/invalid → clears.
pub fn set_from_wire(body: &str) -> (bool, usize) {
    match parse(body) {
        Some(m) => {
            let count = m.coords.len();
            let persisted = patterns::store_blob(patterns::MAP_KEY, &serialize(&m));
            if !persisted {
                println!("map: too large to persist ({} px) — applied live only", count);
            }
            MAP.lock(|c| *c.borrow_mut() = Some(m));
            DIRTY.store(true, Ordering::Relaxed);
            (true, count)
        }
        None => {
            MAP.lock(|c| *c.borrow_mut() = None);
            let _ = patterns::store_blob(patterns::MAP_KEY, &[0u8]); // invalid → treated as none
            DIRTY.store(true, Ordering::Relaxed);
            (false, 0)
        }
    }
}

/// `GET /api/map` → {"installed":bool,"dims":D,"count":N}.
pub fn to_json() -> alloc::string::String {
    MAP.lock(|c| match c.borrow().as_ref() {
        Some(m) => alloc::format!(
            "{{\"installed\":true,\"dims\":{},\"count\":{}}}",
            m.dims,
            m.coords.len()
        ),
        None => alloc::string::String::from("{\"installed\":false,\"dims\":0,\"count\":0}"),
    })
}

/// Load the persisted map. Call after patterns::init().
pub fn init() {
    if let Some(b) = patterns::read_blob(patterns::MAP_KEY) {
        if let Some(m) = deserialize(&b) {
            println!("map: {} px ({}D) from flash", m.coords.len(), m.dims);
            MAP.lock(|c| *c.borrow_mut() = Some(m));
            DIRTY.store(true, Ordering::Relaxed);
        }
    }
}
