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
                        let vals: Vec<String> =
                            raw.iter().map(|&r| format!("{}", r as f64 / 65536.0)).collect();
                        format!("\"{}\":[{}]", json_escape(n), vals.join(","))
                    })
                    .collect();
                format!(
                    "{{\"id\":\"{}\",\"name\":\"{}\",\"sec\":{},\"controls\":{{{}}}}}",
                    it.pattern_id,
                    json_escape(&name),
                    sec,
                    controls.join(",")
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
    wake(); // apply edits if playing
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

/// Load item `i`: compile its stored pattern on the render task and apply its
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
    if crossfade > 0 {
        MSG_QUEUE.send(Msg::Crossfade(src, crossfade as u32)).await;
    } else {
        MSG_QUEUE.send(Msg::Code(src)).await;
    }
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
