//! State shared between the web server and the render task. The engine is
//! owned exclusively by the render task: writes flow in through a message
//! queue, reads come from JSON snapshots the render task publishes.

use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::sync::atomic::{AtomicU32, AtomicU8};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::channel::Channel;
use luxel_core::fixed::Fx;

/// Writes from HTTP handlers to the engine. Sources are compile-checked by
/// the upload handler before queueing (the render task recompiles — keeps
/// `Engine` off the channel, only `Send` data crosses).
pub enum Msg {
    Code(String),
    Control(String, Vec<Fx>),
    Var(String, Fx),
    /// New pixel count — the render task rebuilds the engine + SPI buffer live.
    Config(u32),
}

pub static MSG_QUEUE: Channel<CriticalSectionRawMutex, Msg, 8> = Channel::new();

/// Frames rendered in the last full second, updated by the render task.
pub static FPS: AtomicU32 = AtomicU32::new(0);

/// Global output brightness, 0–31. The render task reads it every frame and
/// feeds it to the encoder (SK9822's 5-bit current field; a software scale for
/// WS2812). HTTP `/api/brightness` writes it; boot seeds it from flash (else
/// the compile-time default). Cheap enough to apply per-frame with no cost.
pub static BRIGHTNESS: AtomicU8 = AtomicU8::new(4);

/// Active pixel count. Runtime-configurable (`/api/config`): only the render
/// task writes it — on `Msg::Config` it rebuilds the engine + SPI buffer to
/// match, no reboot. Everyone else (status, compile-validation) reads it. Boot
/// seeds it from flash (else the board default).
pub static PIXEL_COUNT: AtomicU32 = AtomicU32::new(300);

/// Hard cap on a runtime pixel count, to bound heap use (engine buffers + the
/// SPI encode buffer). 2048 leaves ample headroom on the ESP32.
pub const MAX_PIXELS: u32 = 2048;

type Shared<T> = BlockingMutex<CriticalSectionRawMutex, RefCell<T>>;

fn share_get<T: Clone>(cell: &Shared<T>) -> T {
    cell.lock(|c| c.borrow().clone())
}

/// Most recent runtime (vmerr) message with source location, cleared when a
/// new pattern is accepted.
pub static LAST_VMERR: Shared<Option<String>> = BlockingMutex::new(RefCell::new(None));

pub fn set_vmerr(msg: Option<String>) {
    LAST_VMERR.lock(|c| *c.borrow_mut() = msg);
}

pub fn get_vmerr() -> Option<String> {
    share_get(&LAST_VMERR)
}

/// Snapshot of the last rendered frame (RGB bytes, 3 per pixel) for the
/// browser preview (`GET /api/pixels`).
pub static PIXELS: Shared<Vec<u8>> = BlockingMutex::new(RefCell::new(Vec::new()));

pub fn set_pixels(rgb: &[[u8; 3]]) {
    PIXELS.lock(|c| {
        let mut v = c.borrow_mut();
        v.clear();
        for px in rgb {
            v.extend_from_slice(px);
        }
    });
}

pub fn get_pixels() -> Vec<u8> {
    share_get(&PIXELS)
}

/// Source of the running pattern (`GET /api/pattern`) — updated on swap.
pub static PATTERN_SRC: Shared<String> = BlockingMutex::new(RefCell::new(String::new()));

pub fn set_pattern_src(src: &str) {
    PATTERN_SRC.lock(|c| {
        let mut s = c.borrow_mut();
        s.clear();
        s.push_str(src);
    });
}

pub fn get_pattern_src() -> String {
    share_get(&PATTERN_SRC)
}

/// JSON snapshots published by the render task (see luxel_core::jsonview):
/// controls on pattern swap; vars/readouts every ~250 ms.
pub static CONTROLS_JSON: Shared<String> = BlockingMutex::new(RefCell::new(String::new()));
pub static VARS_JSON: Shared<String> = BlockingMutex::new(RefCell::new(String::new()));
pub static READOUTS_JSON: Shared<String> = BlockingMutex::new(RefCell::new(String::new()));

pub fn publish(cell: &Shared<String>, json: String) {
    cell.lock(|c| *c.borrow_mut() = json);
}

pub fn snapshot(cell: &Shared<String>) -> String {
    let s = share_get(cell);
    if s.is_empty() {
        String::from("{}")
    } else {
        s
    }
}
