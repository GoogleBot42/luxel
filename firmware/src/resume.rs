//! Single-pattern reboot persistence: the actively-running *saved* pattern
//! (set via `POST /api/patterns/<id>/activate`) + its explicitly-set control
//! values survive a reboot, mirroring the playlist's flash conventions.
//!
//! The record lives under [patterns::RESUME_KEY] in the same
//! sequential-storage partition as the playlist. Line-based wire format
//! (matching playlist.rs — no JSON parser needed):
//!   `P <patternId>`             the saved pattern to resume
//!   `C <name> <raw...>`         a control value (raw 16.16), one per line
//!
//! Rules:
//! - Only *library* patterns persist. An ad-hoc `POST /api/code` push has no
//!   id (persisting it is impossible — the source was never saved), so the
//!   record is left alone and a reboot resumes the last saved state.
//! - **Playlist precedence**: a resuming playlist always wins. The record is
//!   neither written while a playlist is playing nor applied at boot when the
//!   playlist's "was playing" flag resumes.
//! - **Flash-wear discipline**: writes are debounced — a change (activation
//!   or slider drag) arms a timer and the record is written once things have
//!   settled for [DEBOUNCE_SECS], not on every event. Identical records are
//!   not rewritten (re-activating the running pattern costs nothing).

use alloc::string::String;
use alloc::vec::Vec;

use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use esp_println::println;
use luxel_core::fixed::Fx;

use crate::patterns;
use crate::shared::{Msg, MSG_QUEUE};

/// Quiet time after the last change before the record is written.
const DEBOUNCE_SECS: u64 = 3;

/// Armed by activation / control changes; the persist task debounces it.
static DIRTY: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Note that the single-pattern state (pattern or controls) changed. Cheap;
/// call freely — the persist task coalesces bursts into one flash write.
pub fn mark_dirty() {
    DIRTY.signal(());
}

/// name → raw 16.16 control values (matches playlist.rs's Item::controls).
type Controls = Vec<(String, Vec<i32>)>;

/// Parse a resume record. `None` if there's no pattern line.
fn parse(body: &str) -> Option<(String, Controls)> {
    let mut id: Option<String> = None;
    let mut controls: Controls = Vec::new();
    for line in body.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("P") => id = it.next().map(String::from),
            Some("C") => {
                if let Some(name) = it.next() {
                    let raw: Vec<i32> = it.filter_map(|v| v.parse().ok()).collect();
                    controls.push((String::from(name), raw));
                }
            }
            _ => {}
        }
    }
    id.map(|id| (id, controls))
}

/// The record for the CURRENT state, or `None` when there's nothing to
/// persist (playlist owns playback, or the pattern is ad-hoc/unsaved).
fn snapshot_record() -> Option<String> {
    if crate::playlist::is_playing() {
        return None;
    }
    let id = crate::shared::get_current_pattern_id();
    if id.is_empty() {
        return None;
    }
    let mut out = String::new();
    out.push_str("P ");
    out.push_str(&id);
    out.push('\n');
    for (name, raw) in crate::shared::get_current_controls() {
        out.push_str("C ");
        out.push_str(&name);
        for r in raw {
            out.push_str(&alloc::format!(" {}", r));
        }
        out.push('\n');
    }
    Some(out)
}

/// Write the current state to flash (skipping identical rewrites).
fn persist_now() {
    let Some(rec) = snapshot_record() else {
        return;
    };
    // skip the write if the stored record already matches (wear discipline)
    if patterns::read_blob(patterns::RESUME_KEY).as_deref() == Some(rec.as_bytes()) {
        return;
    }
    if !patterns::store_blob(patterns::RESUME_KEY, rec.as_bytes()) {
        println!("resume: flash write failed (update in progress?)");
    }
}

/// Load and apply the stored record at boot. The caller has already checked
/// playlist precedence. Missing/deleted patterns and stale-format bytecode
/// (an OTA bumped the LXBC version) skip the resume gracefully — the
/// built-in default keeps rendering.
async fn apply_stored() {
    let Some(bytes) = patterns::read_blob(patterns::RESUME_KEY) else {
        return;
    };
    let Some((id, controls)) = String::from_utf8(bytes).ok().as_deref().and_then(parse) else {
        return;
    };
    let Some(stored) = patterns::stored_size_hint(&id) else {
        println!("resume: stored pattern {} is gone — skipping", id);
        return;
    };
    // Boot-time heap is at its trough while WiFi (whose mallocs don't
    // null-check) is still coming up, and everything below allocates
    // infallibly: src + bc + the envelope ≈ 2× the stored bytes. Loading
    // straight away at a heavy config (big LED buffer, large pattern)
    // OOM-panicked into the boot-loop guard — three strikes flipped the OTA
    // slot back to the previous firmware. Wait for comfortable headroom;
    // if it never shows up, skip resume and leave the default rendering.
    let need = stored * 2 + 24 * 1024;
    let mut waited = 0u32;
    while esp_alloc::HEAP.free() < need {
        if waited >= 20 {
            println!(
                "resume: heap too tight for {} ({} free, need {}) — skipping",
                id,
                esp_alloc::HEAP.free(),
                need
            );
            return;
        }
        Timer::after(Duration::from_secs(2)).await;
        waited += 2;
    }
    let (Some(src), Some(bc)) = (patterns::source_of(&id), patterns::bytecode_of(&id)) else {
        println!("resume: stored pattern {} is gone — skipping", id);
        return;
    };
    if let Err(e) = luxel_core::bytecode::validate(&bc) {
        println!("resume: stored bytecode for {} unusable ({}) — skipping", id, e);
        return;
    }
    let env = luxel_core::bytecode::encode_envelope("", &src, &bc);
    drop((src, bc));
    MSG_QUEUE.send(Msg::Code { env, id: id.clone() }).await;
    crate::shared::set_current_controls(controls.clone());
    for (name, raw) in controls {
        let vals: Vec<Fx> = raw.iter().map(|&r| Fx::from_raw(r)).collect();
        MSG_QUEUE.send(Msg::Control(name, vals)).await;
    }
    println!("resume: pattern {} restored", id);
}

/// Boot resume (when no playlist is resuming) + the debounced persist loop.
#[embassy_executor::task]
pub async fn resume_task() {
    // Precedence: an active playlist resume wins — playlist::init() ran
    // before any task spawned, so this check is race-free.
    if !crate::playlist::is_playing() {
        apply_stored().await;
    }
    loop {
        DIRTY.wait().await;
        // debounce: keep waiting while changes are still arriving
        loop {
            match select(Timer::after(Duration::from_secs(DEBOUNCE_SECS)), DIRTY.wait()).await {
                Either::First(_) => break,
                Either::Second(_) => {}
            }
        }
        persist_now();
    }
}
