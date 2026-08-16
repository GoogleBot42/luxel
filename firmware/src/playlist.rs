//! Device playlist: plays stored patterns in order with per-item parameters
//! and durations. The scheduler task loads each item's pattern + its saved
//! control values, pushes them to the render task, then waits the item's
//! effective duration (item override ?? playlist default; 0 = manual) before
//! advancing (looping at the end). The definition + "was playing" flag persist
//! in flash so a reboot resumes the playlist.
//!
//! Wire/flash format is line-based (no JSON parser needed), matching the
//! native mirror (crates/luxel-cli/src/serve.rs):
//!   `D <sec>`                    default seconds (0 = manual)
//!   `I <patternId> <sec|-1>`     item; -1 = inherit default
//!   `C <name> <raw...>`          a control for the last item (raw 16.16)

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Timer};
use esp_println::println;
use luxel_core::fixed::Fx;
use luxel_core::jsonview::json_escape;

use crate::patterns;
use crate::shared::{Msg, MSG_QUEUE};

#[derive(Clone, Default)]
struct Item {
    pattern_id: String,
    /// name → raw 16.16 control values.
    controls: Vec<(String, Vec<i32>)>,
    /// per-item override seconds; None = inherit the default.
    override_sec: Option<i32>,
}

#[derive(Clone)]
struct Playlist {
    default_sec: i32,
    /// Crossfade between items in ms; 0 = hard cut.
    crossfade_ms: i32,
    items: Vec<Item>,
}

impl Playlist {
    fn item_sec(&self, i: usize) -> i32 {
        self.items
            .get(i)
            .and_then(|it| it.override_sec)
            .unwrap_or(self.default_sec)
    }
}

type Shared<T> = BlockingMutex<CriticalSectionRawMutex, RefCell<T>>;

static PLAYLIST: Shared<Playlist> = BlockingMutex::new(RefCell::new(Playlist {
    default_sec: 0,
    crossfade_ms: 0,
    items: Vec::new(),
}));
static PLAYING: AtomicBool = AtomicBool::new(false);
static INDEX: AtomicUsize = AtomicUsize::new(0);

/// Wakes the scheduler to re-evaluate (play/stop/step/definition-changed).
enum Cmd {
    Wake,
}
static CMD: Channel<CriticalSectionRawMutex, Cmd, 4> = Channel::new();

fn wake() {
    let _ = CMD.try_send(Cmd::Wake);
}

// ---- pre-flight: validate items against the current config ----
//
// A stored pattern's `assert()` invariants can start failing when the
// device's pixel count changes — and a playlist happily schedules it,
// playing black for the whole slot. Pre-flight runs each (unique) item's
// init in a throwaway VM off the render task — one item between frames —
// and `GET /api/playlist` reports violations per item so the UI can badge
// them. Re-queued on: boot, playlist edit, pattern save, pixel-count
// change.

struct Preflight {
    /// Pattern ids awaiting a check.
    pending: Vec<String>,
    /// id → violation message (ok items are simply absent).
    violations: Vec<(String, String)>,
}

static PREFLIGHT: Shared<Preflight> = BlockingMutex::new(RefCell::new(Preflight {
    pending: Vec::new(),
    violations: Vec::new(),
}));

/// Queue every distinct playlist pattern for re-validation (config or
/// content changed). Previous results are kept until each item's fresh
/// verdict lands, so the UI never flickers to "all fine" mid-check.
pub fn preflight_mark_dirty() {
    let ids = PLAYLIST.lock(|c| {
        let pl = c.borrow();
        let mut ids: Vec<String> = Vec::new();
        for it in &pl.items {
            if !ids.contains(&it.pattern_id) {
                ids.push(it.pattern_id.clone());
            }
        }
        ids
    });
    PREFLIGHT.lock(|c| c.borrow_mut().pending = ids);
}

/// Next pattern id awaiting validation (the render task polls this).
pub fn preflight_next() -> Option<String> {
    PREFLIGHT.lock(|c| c.borrow_mut().pending.pop())
}

/// Record a verdict from the render task.
pub fn preflight_record(id: &str, violation: Option<String>) {
    PREFLIGHT.lock(|c| {
        let mut p = c.borrow_mut();
        p.violations.retain(|(i, _)| i != id);
        if let Some(msg) = violation {
            p.violations.push((String::from(id), msg));
        }
    });
}

fn preflight_violation(id: &str) -> Option<String> {
    PREFLIGHT.lock(|c| {
        c.borrow()
            .violations
            .iter()
            .find(|(i, _)| i == id)
            .map(|(_, m)| m.clone())
    })
}

// ---- parsing / serialization ----

fn parse(body: &str) -> Playlist {
    let mut pl = Playlist {
        default_sec: 0,
        crossfade_ms: 0,
        items: Vec::new(),
    };
    for line in body.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("D") => pl.default_sec = it.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            Some("X") => pl.crossfade_ms = it.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            Some("I") => {
                let id = it.next().unwrap_or("").into();
                let sec = it.next().and_then(|v| v.parse::<i32>().ok());
                let override_sec = match sec {
                    Some(n) if n < 0 => None,
                    other => other,
                };
                pl.items.push(Item {
                    pattern_id: id,
                    controls: Vec::new(),
                    override_sec,
                });
            }
            Some("C") => {
                if let (Some(item), Some(name)) = (pl.items.last_mut(), it.next()) {
                    let raw: Vec<i32> = it.filter_map(|v| v.parse().ok()).collect();
                    item.controls.push((name.into(), raw));
                }
            }
            _ => {}
        }
    }
    pl
}

/// `GET /api/playlist` body — names resolved from the pattern library.
pub fn to_json() -> String {
    PLAYLIST.lock(|c| {
        let pl = c.borrow();
        let items: Vec<String> = pl
            .items
            .iter()
            .map(|it| {
                let name = patterns::name_of(&it.pattern_id).unwrap_or_default();
                let sec = it
                    .override_sec
                    .map(|s| format!("{}", s))
                    .unwrap_or_else(|| String::from("null"));
                let controls: Vec<String> = it
                    .controls
                    .iter()
                    .map(|(n, raw)| {
                        // Fx's Display, not f64's: `r as f64 / 65536.0` was the
                        // last user of core's ~8 KB flt2dec printing machinery
                        let vals: Vec<String> =
                            raw.iter().map(|&r| format!("{}", Fx::from_raw(r))).collect();
                        format!("\"{}\":[{}]", json_escape(n), vals.join(","))
                    })
                    .collect();
                // pre-flight verdict: the item's assert() invariants vs the
                // CURRENT config (absent = fine / still checking)
                let invalid = match preflight_violation(&it.pattern_id) {
                    Some(m) => format!(",\"invalid\":\"{}\"", json_escape(&m)),
                    None => String::new(),
                };
                format!(
                    "{{\"id\":\"{}\",\"name\":\"{}\",\"sec\":{},\"controls\":{{{}}}{}}}",
                    it.pattern_id,
                    json_escape(&name),
                    sec,
                    controls.join(","),
                    invalid
                )
            })
            .collect();
        format!(
            "{{\"defaultSec\":{},\"crossfadeMs\":{},\"playing\":{},\"index\":{},\"items\":[{}]}}",
            pl.default_sec,
            pl.crossfade_ms,
            PLAYING.load(Ordering::Relaxed),
            INDEX.load(Ordering::Relaxed),
            items.join(",")
        )
    })
}

// ---- public commands (from the HTTP handlers) ----

pub fn set_from_wire(body: &str) {
    let pl = parse(body);
    PLAYLIST.lock(|c| *c.borrow_mut() = pl);
    patterns::store_blob(patterns::PLAYLIST_KEY, body.as_bytes());
    preflight_mark_dirty();
    wake(); // apply edits if playing
}

/// Whether the playlist is auto-advancing (the HA switch state).
pub fn is_playing() -> bool {
    PLAYING.load(Ordering::Relaxed)
}

pub fn play(i: usize) {
    PLAYING.store(true, Ordering::Relaxed);
    INDEX.store(i, Ordering::Relaxed);
    persist_state();
    wake();
}

pub fn stop() {
    PLAYING.store(false, Ordering::Relaxed);
    persist_state();
    // playback now rests on the last-entered item — persist it as the
    // single-pattern resume state (debounced; see resume.rs)
    crate::resume::mark_dirty();
    wake();
}

pub fn step(d: i32) {
    let len = PLAYLIST.lock(|c| c.borrow().items.len());
    if PLAYING.load(Ordering::Relaxed) && len > 0 {
        let cur = INDEX.load(Ordering::Relaxed) as i64;
        let ni = (cur + d as i64).rem_euclid(len as i64) as usize;
        INDEX.store(ni, Ordering::Relaxed);
    }
    wake();
}

fn persist_state() {
    let byte = [u8::from(PLAYING.load(Ordering::Relaxed))];
    patterns::store_blob(patterns::PLAYSTATE_KEY, &byte);
}

/// Load definition + "was playing" from flash. Call after patterns::init().
pub fn init() {
    if let Some(bytes) = patterns::read_blob(patterns::PLAYLIST_KEY) {
        if let Ok(s) = String::from_utf8(bytes) {
            let pl = parse(&s);
            PLAYLIST.lock(|c| *c.borrow_mut() = pl);
            preflight_mark_dirty(); // validate against the booted config
        }
    }
    if patterns::read_blob(patterns::PLAYSTATE_KEY)
        .and_then(|b| b.first().copied())
        == Some(1)
    {
        PLAYING.store(true, Ordering::Relaxed);
        INDEX.store(0, Ordering::Relaxed);
        println!("playlist: resuming (was playing at reboot)");
    }
}

// ---- scheduler ----

/// Load item `i`: hand its stored bytecode to the render task and apply its
/// saved control values.
async fn enter_item(i: usize) {
    let (item, crossfade) = PLAYLIST.lock(|c| {
        let pl = c.borrow();
        (pl.items.get(i).cloned(), pl.crossfade_ms)
    });
    let Some(item) = item else {
        return;
    };
    let Some(src) = patterns::source_of(&item.pattern_id) else {
        println!("playlist: item {} missing pattern {}", i, item.pattern_id);
        return;
    };
    let Some(bc) = patterns::bytecode_of(&item.pattern_id) else {
        println!("playlist: item {} pattern {} has no bytecode", i, item.pattern_id);
        return;
    };
    let env = luxel_core::bytecode::encode_envelope("", &src, &bc);
    drop((src, bc));
    // the library id rides in the message: the render task stamps identity
    // + library read-back at the swap (and writes NOTHING to flash — the
    // wear fix), so no post-send set_current_pattern_id race here
    if crossfade > 0 {
        MSG_QUEUE
            .send(Msg::Crossfade {
                env,
                ms: crossfade as u32,
                id: item.pattern_id.clone(),
            })
            .await;
    } else {
        MSG_QUEUE.send(Msg::Code { env, id: item.pattern_id.clone() }).await;
    }
    // seed the resume-controls set with the item's saved values, so stopping
    // the playlist persists exactly what's showing (resume.rs)
    crate::shared::set_current_controls(item.controls.clone());
    for (name, raw) in item.controls {
        let vals: Vec<Fx> = raw.iter().map(|&r| Fx::from_raw(r)).collect();
        MSG_QUEUE.send(Msg::Control(name, vals)).await;
    }
}

#[embassy_executor::task]
pub async fn playlist_task() {
    loop {
        if !PLAYING.load(Ordering::Relaxed) {
            let _ = CMD.receive().await; // idle until a command
            continue;
        }
        let (len, i) = (
            PLAYLIST.lock(|c| c.borrow().items.len()),
            INDEX.load(Ordering::Relaxed),
        );
        if len == 0 {
            PLAYING.store(false, Ordering::Relaxed);
            continue;
        }
        enter_item(i).await;
        let sec = PLAYLIST.lock(|c| c.borrow().item_sec(i));
        if sec > 0 {
            match select(Timer::after(Duration::from_secs(sec as u64)), CMD.receive()).await {
                // timer elapsed → advance (wraps)
                Either::First(_) => INDEX.store((i + 1) % len, Ordering::Relaxed),
                // a command changed play/index/definition → re-evaluate
                Either::Second(_) => {}
            }
        } else {
            let _ = CMD.receive().await; // manual: hold until next/prev/stop
        }
    }
}
