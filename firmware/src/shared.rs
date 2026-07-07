//! State shared between the web server and the render task. The engine is
//! owned exclusively by the render task: writes flow in through a message
//! queue, reads come from JSON snapshots the render task publishes.

use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU8};

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
    /// New LED protocol — the render task reconfigures SPI + resizes the buffer
    /// live (0 = SK9822, 1 = WS2812; see leds::Protocol::from_u8).
    Protocol(u8),
    /// Like Code, but crossfade from the current pattern over `ms` (playlist
    /// transitions): the render task keeps the outgoing engine and blends.
    Crossfade(String, u32),
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

/// Active LED protocol as a code (0 = SK9822, 1 = WS2812; leds::Protocol
/// from_u8/as_u8). Runtime-configurable (`/api/protocol`): only the render task
/// writes it — on `Msg::Protocol` it reconfigures the SPI clock + resizes the
/// buffer, no reboot. Boot seeds it from flash (else the board default).
pub static PROTOCOL: AtomicU8 = AtomicU8::new(0);

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

/// Master power (Home Assistant's light switch): when false the render task
/// outputs black — the engine keeps ticking so ON resumes mid-animation.
/// Written by the MQTT task, read by the render task and state publishing.
pub static POWER: AtomicBool = AtomicBool::new(true);

/// Library id of the running pattern ("" = ad-hoc code push / built-in
/// default). Set on activate/playlist-enter, cleared on raw code push; the
/// MQTT pattern-select state reads it.
pub static CURRENT_PATTERN_ID: Shared<String> = BlockingMutex::new(RefCell::new(String::new()));

pub fn set_current_pattern_id(id: &str) {
    CURRENT_PATTERN_ID.lock(|c| {
        let mut s = c.borrow_mut();
        s.clear();
        s.push_str(id);
    });
}

pub fn get_current_pattern_id() -> String {
    share_get(&CURRENT_PATTERN_ID)
}

/// Poked when the MQTT broker config changes so the MQTT task reconnects
/// (or connects for the first time) without a reboot.
pub static MQTT_POKE: embassy_sync::signal::Signal<CriticalSectionRawMutex, ()> =
    embassy_sync::signal::Signal::new();

/// Latest sensor frame (PB sensor-board serial or POST /api/sensors) + a
/// sequence counter so the render task applies each frame exactly once.
pub static SENSOR_FRAME: Shared<Option<luxel_core::engine::SensorFrame>> =
    BlockingMutex::new(RefCell::new(None));
pub static SENSOR_SEQ: AtomicU32 = AtomicU32::new(0);

pub fn set_sensor_frame(s: luxel_core::engine::SensorFrame) {
    use core::sync::atomic::Ordering;
    // seq bump inside the critical section (rv32imc has no fetch_add, and
    // there are multiple writers: the UART task and HTTP handlers)
    SENSOR_FRAME.lock(|c| {
        *c.borrow_mut() = Some(s);
        SENSOR_SEQ.store(SENSOR_SEQ.load(Ordering::Relaxed).wrapping_add(1), Ordering::Relaxed);
    });
}

/// The newest unapplied sensor frame, if any (tracks per-caller via `seen`).
pub fn take_sensor_frame(seen: &mut u32) -> Option<luxel_core::engine::SensorFrame> {
    use core::sync::atomic::Ordering;
    let seq = SENSOR_SEQ.load(Ordering::Relaxed);
    if seq == *seen {
        return None;
    }
    *seen = seq;
    share_get(&SENSOR_FRAME)
}

/// Network input (DDP/E1.31): the assembled RGB frame. While packets flow
/// (see LIVE_MARK_MS) the render task outputs this instead of the engine.
pub static LIVE_PIXELS: Shared<Vec<u8>> = BlockingMutex::new(RefCell::new(Vec::new()));

/// embassy now() ms of the last network-input packet (0 = never), and which
/// protocol sent it (0 = none, 1 = DDP, 2 = E1.31). Written by the netin
/// task, read by the render task and /api/status.
pub static LIVE_MARK_MS: AtomicU32 = AtomicU32::new(0);
pub static LIVE_PROTO: AtomicU8 = AtomicU8::new(0);

/// How long after the last DDP/E1.31 packet the pattern takes back over.
pub const LIVE_TIMEOUT_MS: u32 = 2500;

/// The protocol currently overriding the engine, if any (shared by the
/// render task's frame gate and the status JSON).
pub fn live_proto(now_ms: u32) -> Option<&'static str> {
    use core::sync::atomic::Ordering;
    let mark = LIVE_MARK_MS.load(Ordering::Relaxed);
    if mark == 0 || now_ms.wrapping_sub(mark) >= LIVE_TIMEOUT_MS {
        return None;
    }
    match LIVE_PROTO.load(Ordering::Relaxed) {
        1 => Some("ddp"),
        2 => Some("e131"),
        _ => None,
    }
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
