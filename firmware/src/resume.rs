//! Direct-play reboot persistence: the actively-running *saved* pattern
//! (set via `POST /api/patterns/<id>/activate`) + its explicitly-set control
//! values — or the stored *scene* shown via `POST /api/scenes/<id>/activate`
//! (Gitea #790) — survive a reboot, mirroring the playlist's flash
//! conventions.
//!
//! The record lives under [patterns::RESUME_KEY] in the same
//! sequential-storage partition as the playlist. Line-based wire format
//! (matching playlist.rs — no JSON parser needed):
//!   `P <patternId>`             the saved pattern to resume
//!   `S <sceneId>`               the stored scene to resume (instead of `P`)
//!   `C <name> <raw...>`         a control value (raw 16.16), one per line
//!
//! Rules:
//! - Only *library* patterns persist. An ad-hoc `POST /api/code` push has no
//!   id (persisting it is impossible — the source was never saved), so the
//!   record is left alone and a reboot resumes the last saved state.
//! - **A scene on screen outranks the pattern id.** `install_scene` stamps
//!   the base layer's pattern as the "current pattern" so `/api/pattern`
//!   shows something real, and a record written from that id would resume
//!   the base pattern ALONE (the #790 bug). The record is `S <sceneId>`
//!   with no `C` lines: a scene's layers carry their own control values.
//! - **Playlist precedence**: a resuming playlist always wins. The record is
//!   neither written while a playlist is playing nor applied at boot when the
//!   playlist's "was playing" flag resumes.
//! - **Flash-wear discipline**: writes are debounced — a change (activation
//!   or slider drag) arms a timer and the record is written once things have
//!   settled for [DEBOUNCE_SECS], not on every event. Identical records are
//!   not rewritten (re-activating the running pattern costs nothing).
//!
//! ## Passenger: the text slots (Gitea #745)
//!
//! `textslots::persist` rides this loop's debounce rather than carrying a
//! task of its own. It is the other small reserved-key record whose writes
//! arrive in bursts (a console text field, an HA automation), it wants the
//! same "settle, then write once, skip if unchanged" discipline, and a
//! second embassy task's storage is not free on a board with under 100
//! bytes of `.stack` margin. Its write does NOT observe the playlist
//! precedence above — a slot's text belongs to the device, not to whatever
//! is playing.

use alloc::string::String;
use alloc::vec::Vec;

use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use esp_println::println;
use luxel_core::fixed::Fx;
use luxel_core::jsonview::{push_i32, push_piece};

use crate::patterns;
use crate::shared::{Msg, MSG_QUEUE};

/// Quiet time after the last change before the record is written.
const DEBOUNCE_SECS: u64 = 3;

/// Armed by activation / control changes; the persist task debounces it.
static DIRTY: Signal<CriticalSectionRawMutex, ()> = Signal::new();

/// Note that persisted state changed — the single-pattern record (pattern
/// or controls) or a text slot (`textslots::mark_dirty`). Cheap;
/// call freely — the persist task coalesces bursts into one flash write.
pub fn mark_dirty() {
    DIRTY.signal(());
}

/// name → raw 16.16 control values (matches playlist.rs's Item::controls).
type Controls = Vec<(String, Vec<i32>)>;

/// What a stored record asks the boot to show.
enum Record {
    Pattern(String, Controls),
    Scene(String),
}

/// Parse a resume record. `None` if there's neither a pattern nor a scene
/// line. The writer never emits both; a record that somehow carries both
/// resumes the scene (its base pattern IS the current pattern, so `P` is
/// the lesser claim).
fn parse(body: &str) -> Option<Record> {
    let mut id: Option<String> = None;
    let mut scene: Option<String> = None;
    let mut controls: Controls = Vec::new();
    for line in body.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("P") => id = it.next().map(String::from),
            Some("S") => scene = it.next().map(String::from),
            Some("C") => {
                if let Some(name) = it.next() {
                    let raw: Vec<i32> = it.filter_map(|v| v.parse().ok()).collect();
                    controls.push((String::from(name), raw));
                }
            }
            _ => {}
        }
    }
    if let Some(scene) = scene {
        return Some(Record::Scene(scene));
    }
    id.map(|id| Record::Pattern(id, controls))
}

/// The record for the CURRENT state, or `None` when there's nothing to
/// persist (playlist owns playback, or the pattern is ad-hoc/unsaved).
fn snapshot_record() -> Option<String> {
    if crate::playlist::is_playing() {
        return None;
    }
    // A scene on screen wins over the (base-layer) pattern id — see the
    // module doc. `set_active("")` runs on every single-pattern install, so
    // a non-empty id here is exactly "a scene is what is showing".
    let scene = crate::scenes::active_id();
    if !scene.is_empty() {
        let mut out = String::new();
        push_piece(&mut out, "S ");
        push_piece(&mut out, &scene);
        push_piece(&mut out, "\n");
        return Some(out);
    }
    let id = crate::shared::get_current_pattern_id();
    if id.is_empty() {
        return None;
    }
    let mut out = String::new();
    push_piece(&mut out, "P ");
    push_piece(&mut out, &id);
    push_piece(&mut out, "\n");
    for (name, raw) in crate::shared::get_current_controls() {
        push_piece(&mut out, "C ");
        push_piece(&mut out, &name);
        for r in raw {
            push_piece(&mut out, " ");
            push_i32(&mut out, r);
        }
        push_piece(&mut out, "\n");
    }
    Some(out)
}

/// Write the current state to flash (skipping identical rewrites).
fn persist_now() {
    // The text slots ride this same debounce (Gitea #745) — a separate
    // embassy task for a ≤529 B record is not worth its storage on a board
    // with under 100 bytes of `.stack` margin, and they have exactly the
    // burst-of-writes problem this loop already exists to absorb. Their
    // write is independent of the playlist precedence below, so it runs
    // first.
    crate::textslots::persist();
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

/// Free heap a resume must see before it hands the render task a stack of
/// `layers` pattern engines, the largest of which stages `staging` bytes of
/// bytecode while it decodes: [`luxel_core::budget::install_need`], the
/// installer's own arithmetic (Gitea #869, #905). It used to be
/// `2 × Σ stored bytes + 24 KiB` — source, blob and envelope of every layer
/// resident at once — which nothing on the install path does, and which
/// asked 91,546 B on the Seengreat panel for a scene that installs from
/// 51,396 B at runtime.
///
/// The `black_box` is a TOOLCHAIN WORKAROUND, not a tuning knob. With the
/// constant term as a plain literal the Xtensa LLVM fork
/// (xtensa-rust-1.95.0.0) aborted instruction selection on `resume_task`'s
/// poll function: `rustc-LLVM ERROR: Cannot select: i32 = Constant<24576>`
/// — the number tracked the literal (23 * 1024 failed as
/// `Constant<23552>`), so it is the constant node itself the backend cannot
/// place, not a frame size. It only appears once the surrounding state
/// machine is complex enough: the same source built fine before the #330
/// store rewrite, and `#[inline(never)]` alone does not help (fat LTO folds
/// the body back in). So the floor — the constant term of `install_need` —
/// is swapped for an opaque copy of itself; one register move on a
/// once-per-boot path.
#[inline(never)]
fn resume_need(layers: usize, staging: usize) -> usize {
    use luxel_core::budget::{install_need, RUNTIME_FLOOR};
    let px = crate::shared::PIXEL_COUNT.load(core::sync::atomic::Ordering::Relaxed);
    install_need(px, layers, staging, luxel_core::arena::frames_external()) - RUNTIME_FLOOR
        + core::hint::black_box(RUNTIME_FLOOR)
}

/// The "heap too tight" line, with the terms of [resume_need] spelled out
/// so a serial log explains its number. Out of line so the formatting and
/// the per-term constants stay out of the task future's poll function.
#[inline(never)]
fn too_tight(what: &str, id: &str, free: usize, layers: usize, staging: usize, need: usize) {
    use luxel_core::budget::{compositor_scratch, layer_cost, RUNTIME_FLOOR};
    let px = crate::shared::PIXEL_COUNT.load(core::sync::atomic::Ordering::Relaxed);
    let ext = luxel_core::arena::frames_external();
    println!(
        "resume: heap too tight for {} {} ({} free, need {} = floor {} + {} layer(s) x {} + scratch {} + staging {}) — playing nothing",
        what,
        id,
        free,
        need,
        RUNTIME_FLOOR,
        layers,
        layer_cost(px, ext),
        compositor_scratch(px, ext),
        staging
    );
}

/// Load and apply the stored record at boot. The caller has already checked
/// playlist precedence.
///
/// Every bail-out below leaves the device playing NOTHING — a dark strip,
/// `/api/status` `engines: 0`, `src`/`bc` false (Gitea #744). It used to
/// leave the built-in rainbow rendering, which is why these paths only ever
/// logged to serial; now the absence of light IS the user-visible signal, so
/// each one says on the console why. Missing/deleted patterns and
/// stale-format bytecode (an OTA bumped the LXBC version) still skip the
/// resume gracefully rather than failing the boot.
async fn apply_stored() {
    let Some(bytes) = patterns::read_blob(patterns::RESUME_KEY) else {
        // genuine first boot (or a device whose record was cleared)
        println!("resume: nothing stored — playing nothing");
        return;
    };
    let Some(rec) = String::from_utf8(bytes).ok().as_deref().and_then(parse) else {
        println!("resume: stored record unreadable — playing nothing");
        return;
    };
    // One linear path for both kinds: every local below lives in this
    // task's FUTURE, which is a `.bss` static that comes straight out of
    // the main-task stack floor (tools/stack-check.sh), so a second arm's
    // worth of Strings and Vecs is not free.
    let (id, controls, is_scene) = match rec {
        Record::Pattern(id, controls) => (id, controls, false),
        Record::Scene(id) => (id, Vec::new(), true),
    };
    let what = if is_scene { "scene" } else { "pattern" };
    let Some((layers, staging)) = install_shape(&id, is_scene) else {
        println!("resume: stored {} {} is gone — playing nothing", what, id);
        return;
    };
    // Boot-time heap is at its trough while WiFi (whose mallocs don't
    // null-check) is still coming up. Loading straight away at a heavy
    // config OOM-panicked into the boot-loop guard once — three strikes
    // flipped the OTA slot back to the previous firmware — so the install
    // waits for what it will actually take (`resume_need`: the floor, each
    // layer's resident cost, the largest layer's staging copy). If that
    // never shows up, skip resume — the device then plays nothing (Gitea
    // #744), which is a dark strip and a serial line, not a crash.
    //
    // Poll every 2 s for up to 20 s, but give up as soon as two samples in
    // a row fail to rise: on the Seengreat panel free heap only FALLS after
    // WiFi-up, so a check that fails once fails all ten and the extra 18 s
    // were darkness for nothing (#869). Two more words in the task future.
    let need = resume_need(layers, staging);
    let mut free = esp_alloc::HEAP.free();
    let mut waited = 0u32;
    let mut flat = 0u32;
    while free < need {
        if waited >= 20 || flat >= 2 {
            too_tight(what, &id, free, layers, staging, need);
            return;
        }
        Timer::after(Duration::from_secs(2)).await;
        waited += 2;
        let now = esp_alloc::HEAP.free();
        flat = if now > free { 0 } else { flat + 1 };
        free = now;
    }
    if let Err(e) = validate(&id, is_scene) {
        println!("resume: {} — playing nothing", e);
        return;
    }
    if is_scene {
        // Same hand-off as `POST /api/scenes/<id>/activate` (scenes.rs):
        // the render task builds every layer engine from the stored scene;
        // controls and projection belong to the scene's own layers.
        MSG_QUEUE.send(Msg::Scene { id: id.clone(), ms: 0 }).await;
        crate::shared::set_current_controls(Vec::new());
    } else {
        MSG_QUEUE.send(Msg::Library { id: id.clone(), ms: 0 }).await;
        crate::shared::set_current_controls(controls.clone());
        for (name, raw) in controls {
            let vals: Vec<Fx> = raw.iter().map(|&r| Fx::from_raw(r)).collect();
            MSG_QUEUE.send(Msg::Control(name, vals)).await;
        }
    }
    println!("resume: {} {} restored", what, id);
}

/// (pattern layers, staging bytes) of what a resume will install: one
/// layer for a pattern, every pattern layer of a scene (each is a resident
/// engine). `staging` is the transient bytecode copy of the LARGEST layer
/// that cannot execute out of the flash mapping — the installer builds one
/// layer at a time and drops each copy before the next decode, so it is a
/// max, never a sum — and 0 when every layer is mapped or the copy lands in
/// the PSRAM arena (`patterns::payload_vec`, #905). `None` when the
/// record's target is gone from its store.
///
/// Pattern header fields only — no payload read, no copy. Synchronous and never
/// inlined: the scene copy and the walk stay on the stack for the call, not
/// in the task future.
#[inline(never)]
fn install_shape(id: &str, is_scene: bool) -> Option<(usize, usize)> {
    // a mapped layer executes in place (`code_of`); otherwise its copy is
    // `bc_len` bytes
    let staged = |pid: &str| -> usize {
        if patterns::code_of(pid).is_some() {
            0
        } else {
            // a gone layer pattern costs nothing here; `validate` names it
            patterns::bytecode_len_hint(pid).unwrap_or(0)
        }
    };
    let (layers, largest) = if is_scene {
        let sc = crate::scenes::get(id)?;
        let mut n = 0usize;
        let mut largest = 0usize;
        for l in &sc.layers {
            if let Some(pid) = l.pattern_id() {
                n += 1;
                largest = largest.max(staged(pid));
            }
        }
        (n, largest)
    } else {
        patterns::bytecode_len_hint(id)?;
        (1, staged(id))
    };
    let staging = if luxel_core::arena::installed() { 0 } else { largest };
    Some((layers, staging))
}

/// The single-pattern rule, applied to every pattern layer of a scene: a
/// pattern that is gone, or whose stored bytecode is unusable (an OTA
/// bumped the LXBC format), skips the resume with a sentence naming it —
/// the same graceful skip a lone pattern gets, never a failed boot.
#[inline(never)]
fn validate(id: &str, is_scene: bool) -> Result<(), String> {
    let check = |pid: &str| -> Result<(), String> {
        match patterns::validate_stored(pid) {
            None => Err(alloc::format!("stored pattern {} is gone", pid)),
            Some(Err(e)) => Err(alloc::format!("stored bytecode for {} unusable ({})", pid, e)),
            Some(Ok(())) => Ok(()),
        }
    };
    if !is_scene {
        return check(id);
    }
    let Some(sc) = crate::scenes::get(id) else {
        return Err(alloc::format!("stored scene {} is gone", id));
    };
    for l in &sc.layers {
        if let Some(pid) = l.pattern_id() {
            check(pid).map_err(|e| alloc::format!("scene {} layer: {}", id, e))?;
        }
    }
    Ok(())
}

/// Boot resume (when no playlist is resuming) + the debounced persist loop.
#[embassy_executor::task]
pub async fn resume_task() {
    // Precedence: an active playlist resume wins — playlist::init() ran
    // before any task spawned, so this check is race-free.
    // LUXEL_NO_RESUME=1 at build time: boot without restoring the stored
    // pattern or scene — a bench lever for a device that resets inside its
    // own resume and never reaches HTTP (2026-10-04, Seengreat boot loop).
    if option_env!("LUXEL_NO_RESUME").is_none() && !crate::playlist::is_playing() {
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
