//! Device scene records: the ordered layer stacks the compositor draws
//! (Gitea #478).
//!
//! Storage is ONE reserved-key blob ([`patterns::SCENES_KEY`]) holding every
//! scene block back to back, in exactly the wire format `POST /api/scenes`
//! accepts — `luxel_core::scene::parse_all` reads it and
//! `luxel_core::scene::serialize` writes it, so the flash bytes, the wire
//! bytes and the mirror's bytes are the same bytes. The blob must fit one
//! flash page ([`patterns::BLOB_MAX`] = 3840 B); a write that would exceed
//! it is REFUSED and nothing changes — unlike the playlist, which discards
//! `store_blob`'s verdict and silently loses an oversized definition at the
//! next reboot (Gitea #478, trap 10 of the Phase-B code map).
//!
//! The list algebra (ids, the blob, upsert/delete, the store-full message)
//! lives in [`crate::scenestore`], which is device-free and host-tested; this
//! module is the executor — flash, critical sections, the id counter and the
//! render task's resident [`Runtime`].

use alloc::string::String;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use esp_println::println;
use luxel_core::compose::{self, Compositor, SceneDriver, SceneHost, SpriteView};
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::jsonview::{self, push_piece, push_u32};
use luxel_core::outpipe::GridMap;
use luxel_core::projection::ProjectionMode;
use luxel_core::scene::{self, Scene, TextSource};
use luxel_core::text;

use crate::patterns;
use crate::scenestore;
use crate::shared::{Msg, MSG_QUEUE};

type Shared<T> = BlockingMutex<CriticalSectionRawMutex, RefCell<T>>;

static SCENES: Shared<Vec<Scene>> = BlockingMutex::new(RefCell::new(Vec::new()));
/// The scene the render task is showing (empty = a plain single pattern).
static ACTIVE: Shared<String> = BlockingMutex::new(RefCell::new(String::new()));
static NEXT_SEQ: AtomicU32 = AtomicU32::new(1);
/// Bytes the stored blob occupies, kept in step with flash by [`commit`].
///
/// It is what `GET /api/scenes` reports as `used`, and — the reason it
/// exists — the exact size the NEXT blob build reserves. Rebuilding the
/// blob just to measure it cost the write path a second 4 KiB-capable
/// `String`, which on the Seengreat panel at the two-pattern-layer cap
/// (`heap_largest` ~6.6 KB) was one of the infallible allocations that
/// rebooted the board instead of refusing the save (Gitea #724).
static BLOB_LEN: AtomicUsize = AtomicUsize::new(0);

/// Free internal heap, for the out-of-memory message.
fn free_heap() -> usize {
    esp_alloc::HEAP.free() as usize
}

/// Persist `blob` (the bytes `list` serializes to) and, only if the store
/// took it, adopt it. Every mutation funnels through here so a refused
/// write leaves RAM and flash agreeing.
///
/// The caller hands the blob in rather than letting this rebuild it: the
/// whole write path is now ONE fallible allocation of the blob plus the
/// store's own page buffer, which is what fits under a live scene.
fn commit(list: Vec<Scene>, blob: String) -> Result<(), String> {
    if blob.len() > patterns::BLOB_MAX {
        return Err(scenestore::too_big(blob.len(), patterns::BLOB_MAX));
    }
    if !patterns::store_blob(patterns::SCENES_KEY, blob.as_bytes()) {
        // `store_blob` answers false for "no flash driver", "over BLOB_MAX"
        // and — since #724 — "no heap for the store's 4 KiB page buffer".
        // The last is the one a user at the layer cap actually hits and the
        // only one they can act on, so say so when the heap is short.
        return Err(if free_heap() < 3 * patterns::BLOB_MAX {
            scenestore::no_memory(free_heap())
        } else {
            String::from("scenes: the store refused the record")
        });
    }
    BLOB_LEN.store(blob.len(), Ordering::Relaxed);
    SCENES.lock(|c| *c.borrow_mut() = list);
    Ok(())
}

// ---- reads ----


/// A copy of one scene, for the render task.
pub fn get(id: &str) -> Option<Scene> {
    SCENES.lock(|c| c.borrow().iter().find(|s| s.id == id).cloned())
}

pub fn name_of(id: &str) -> Option<String> {
    SCENES.lock(|c| c.borrow().iter().find(|s| s.id == id).map(|s| s.name.clone()))
}

/// Layer count, for the playlist's scene items.
pub fn layer_count(id: &str) -> Option<usize> {
    SCENES.lock(|c| c.borrow().iter().find(|s| s.id == id).map(|s| s.layers.len()))
}

pub fn active_id() -> String {
    ACTIVE.lock(|c| c.borrow().clone())
}

/// The render task's report of what it is actually showing.
pub fn set_active(id: &str) {
    ACTIVE.lock(|c| {
        let mut a = c.borrow_mut();
        a.clear();
        a.push_str(id);
    });
}

/// What a scene READ answers with — the route turns each arm into a
/// response (`server::scenes_reply`).
pub enum Body {
    /// The JSON body, in segments (`jsonview::Chunks`) so it never needed a
    /// contiguous block. `Chunks::ok()` false means the builder ran out of
    /// heap partway and the route must answer 503, not ship a short body.
    Json(jsonview::Chunks),
    /// No scene by that id: 200 + `{"ok":false,"error":"no such scene"}`,
    /// the same shape `/api/patterns/<id>` uses.
    NoSuchScene,
}

/// `GET /api/scenes` — the body itself is [`scenestore::list_json`], which
/// is where the segmented build lives (Gitea #753); this supplies the list,
/// the lock and the device numbers.
pub fn to_json() -> Body {
    let layers_max = crate::server::scene_layer_cap();
    SCENES.lock(|c| {
        let list = c.borrow();
        let active = ACTIVE.lock(|a| a.borrow().clone());
        Body::Json(scenestore::list_json(
            &list,
            &active,
            layers_max as u32,
            BLOB_LEN.load(Ordering::Relaxed),
            patterns::BLOB_MAX,
        ))
    })
}

/// `GET /api/scenes/<id>` — one scene, segmented the same way [`to_json`]
/// is. `scene::json_bound` sizes the segment index (a handful of pointers),
/// not a body reservation: there is no longer a reservation to fail.
pub fn get_json(id: &str) -> Body {
    SCENES.lock(|c| match c.borrow().iter().find(|s| s.id == id) {
        None => Body::NoSuchScene,
        Some(s) => {
            let mut out = jsonview::Chunks::with_hint(scene::json_bound(s));
            scene::push_json(s, &mut out);
            Body::Json(out)
        }
    })
}

// ---- writes ----

/// `POST /api/scenes` (`id` None) or `POST /api/scenes/<id>` (`id` Some).
///
/// The body is one scene block. Its own `S` id is honoured when it names an
/// existing scene; `S -` (or an id nothing matches on a bare POST) assigns a
/// fresh one. Returns the id the record now carries.
pub fn set_from_wire(body: &str, id: Option<&str>) -> Result<String, String> {
    let list = SCENES.lock(|c| c.borrow().clone());
    // Reserve the id only once the record is known good, so a rejected POST
    // does not burn a number.
    let next = NEXT_SEQ.load(Ordering::Relaxed);
    let lim = scenestore::Limits {
        max: patterns::BLOB_MAX,
        layers: crate::server::scene_layer_cap() as usize,
        cur_len: BLOB_LEN.load(Ordering::Relaxed),
        free: free_heap(),
    };
    let (list, target, blob) = scenestore::upsert(list, body, id, next, &lim)?;
    if id.is_none() && target == scenestore::id_hex(next) {
        NEXT_SEQ.store(next.wrapping_add(1), Ordering::Relaxed);
    }
    commit(list, blob)?;
    Ok(target)
}

/// `DELETE /api/scenes/<id>` — also drops every playlist item that named it.
pub fn delete(id: &str) -> Result<(), String> {
    let list = scenestore::remove(SCENES.lock(|c| c.borrow().clone()), id)?;
    // A delete only ever shrinks the blob, so the stored length is the
    // reservation.
    let blob = scenestore::blob_try(&list, BLOB_LEN.load(Ordering::Relaxed), free_heap())?;
    commit(list, blob)?;
    // `active` names a STORED scene, so it clears with the record — the
    // pixels stay until something else is pushed, which is all a deleted
    // scene can honestly claim. Same as the mirror.
    if active_id() == id {
        set_active("");
    }
    crate::playlist::drop_scene(id);
    Ok(())
}

/// `POST /api/scenes/<id>/activate` — parks the playlist and hands the scene
/// to the render task, exactly like a direct pattern play.
pub async fn activate(id: &str, ms: u32) -> Result<(), String> {
    if get(id).is_none() {
        return Err(String::from("no such scene"));
    }
    MSG_QUEUE
        .send(Msg::Scene {
            id: String::from(id),
            ms,
        })
        .await;
    Ok(())
}

// ---- boot ----

/// Load the scene blob from flash. Call after [`patterns::init`] — it shares
/// the storage partition. The blob is capped at one flash page, the same
/// size the playlist already reads here, so it does not need the post-WiFi
/// treatment `resume.rs` gets for its multi-KB pattern load.
pub fn init() {
    let Some(bytes) = patterns::read_blob(patterns::SCENES_KEY) else {
        return;
    };
    let Ok(text) = String::from_utf8(bytes) else {
        println!("scenes: stored blob is not utf-8 — ignored");
        return;
    };
    match scene::parse_all(&text) {
        Ok(list) => {
            NEXT_SEQ.store(scenestore::next_seq(&list), Ordering::Relaxed);
            BLOB_LEN.store(text.len(), Ordering::Relaxed);
            println!("scenes: {} loaded ({} B)", list.len(), text.len());
            SCENES.lock(|c| *c.borrow_mut() = list);
        }
        // A blob a NEWER firmware wrote can carry a layer type this one
        // cannot parse. Refusing the whole file would also refuse every
        // scene in it; logging and starting empty keeps the device serving,
        // and the next successful POST rewrites the blob.
        Err(e) => println!("scenes: stored blob rejected: {}", e),
    }
}

// ---- the render task's resident scene ----

/// What one scene layer costs the render task.
pub enum Slot {
    /// Text or colour: the compositor draws it natively, nothing resident.
    Native,
    /// The scene's FIRST pattern layer. It is rendered from the render
    /// task's PRIMARY engine, so `/api/controls`, `/api/vars`, the sensor
    /// and event inboxes, the projection override and the published geometry
    /// all keep pointing at a real engine exactly as they did before scenes
    /// existed — a single pattern is just a one-layer scene.
    Base,
    /// A further pattern layer's own engine.
    Pattern(Engine),
    /// A sprite layer: the compiled pattern (never stepped — [`sprite_view`]
    /// only reads its const arrays) plus the `// @sprite …` tag line, which
    /// lives in the SOURCE and so has to be carried beside the engine.
    Sprite(Engine, String),
}

/// A scene resident in the render task: the compositor plus one slot per
/// layer, bottom → top.
pub struct Runtime {
    pub id: String,
    pub comp: Compositor,
    pub slots: Vec<Slot>,
    /// Every pattern id a slot's engine executes from, for
    /// [`patterns::set_layer_pins`].
    pub pinned: Vec<String>,
    /// The SHARED full-frame driver (Gitea #732) — the same walk the wasm
    /// playground runs, remainder-carrying millisecond clock included.
    /// Four bytes; a resident scene's `.bss` footprint is measured
    /// (`tools/stack-check.sh`) and the classic-ESP32 boards have ~68 B of
    /// `.stack` over the floor, so nothing bigger belongs here — the
    /// resolved-text buffer is a per-frame [`SlotHost`] local for exactly
    /// that reason.
    driver: SceneDriver,
}

/// The device's side of [`SceneHost`]: the slot table, the render task's
/// primary engine (which draws the [`Slot::Base`] layer) and this frame's
/// civil time, read at most once and only if a clock layer asks.
struct SlotHost<'a> {
    slots: &'a mut [Slot],
    base: Option<&'a mut Engine>,
    /// `None` = not read yet; `Some(None)` = read, no SNTP sync.
    civil: Option<Option<(u16, u8, u8, u8, u8, u8)>>,
    /// Resolved clock / slot text, reused across the layers of ONE frame.
    /// Owned by the host, not by the [`Runtime`], so a resident scene
    /// carries no buffer between frames — see the note on `Runtime::driver`.
    /// (The pre-#732 walk allocated a fresh `String` per text layer per
    /// frame; this is at most one for the whole frame.)
    text: String,
}

/// Fill `buf` with `s` without ever panicking — a render-loop allocation
/// (Gitea #702). A refusal draws no text, which beats a reboot; the
/// capacity survives, so this allocates once per scene.
fn set_scratch(buf: &mut String, s: &str) {
    buf.clear();
    if s.len() > buf.capacity() && buf.try_reserve_exact(s.len()).is_err() {
        return;
    }
    buf.push_str(s);
}

impl SceneHost for SlotHost<'_> {
    fn pattern_frame(&mut self, layer: usize, delta: Fx) -> Option<&[[u8; 3]]> {
        // The base layer renders from the render task's primary engine,
        // which lives outside the slot table — read it first so the two
        // borrows never overlap.
        if matches!(self.slots.get(layer)?, Slot::Base) {
            return self.base.as_deref_mut().map(|e| e.frame(delta));
        }
        match self.slots.get_mut(layer)? {
            Slot::Pattern(e) => Some(e.frame(delta)),
            // A layer that failed to build is a `Slot::Native` no-op even
            // where the scene says `pat`: it draws nothing, the rest of the
            // stack still shows.
            _ => None,
        }
    }

    fn sprite(&mut self, layer: usize) -> Option<SpriteView<'_>> {
        match self.slots.get(layer)? {
            Slot::Sprite(e, src) => compose::sprite_view(e, src),
            _ => None,
        }
    }

    fn text(&mut self, _layer: usize, source: &TextSource) -> Option<&str> {
        match source {
            // Contract §1: with no SNTP sync yet, say so rather than
            // showing a plausible wrong time.
            TextSource::Clock(f) => {
                let civil = *self.civil.get_or_insert_with(civil_local);
                match civil {
                    Some((y, mo, d, h, m, s)) => {
                        set_scratch(&mut self.text, &text::format_clock(*f, h, m, s, y, mo, d))
                    }
                    // Before the first SNTP sync there is no honest time to
                    // show, so draw NOTHING. `--:--` was a time-shaped
                    // artifact that flashed on every boot for the seconds
                    // before `sntp_task` landed (Gitea #745, #729 item 35),
                    // and it is the wrong shape for a `date` layer anyway.
                    // An empty run composites as "the clock hasn't appeared
                    // yet" rather than as a glitch. `Some(&self.text)` is
                    // still the return: `None` means "already resolved by
                    // set_scene", which would draw the stored string.
                    None => set_scratch(&mut self.text, ""),
                }
            }
            TextSource::Slot(k) => text::with_slot(*k, |v: &str| set_scratch(&mut self.text, v)),
            // `lit` was resolved once by `Compositor::set_scene`.
            TextSource::Lit(_) => return None,
        }
        Some(&self.text)
    }
}

impl Runtime {
    /// Resident engines (the number `/api/status` reports as `engines`),
    /// EXCLUDING the base — the caller adds its own.
    pub fn engines(&self) -> u32 {
        self.slots
            .iter()
            .filter(|s| matches!(s, Slot::Pattern(_) | Slot::Sprite(..)))
            .count() as u32
    }

    /// Pattern layers this scene holds resident, the number the transition
    /// rule and `caps.layers` are about.
    pub fn pattern_layers(&self) -> usize {
        self.slots
            .iter()
            .filter(|s| matches!(s, Slot::Base | Slot::Pattern(_)))
            .count()
    }

    /// Re-point the compositor at the live grid (the layout changed, or the
    /// base engine fabricated a different one).
    pub fn set_grid(&mut self, grid: GridMap) {
        self.comp.set_grid(grid);
    }

    /// Hand every resident engine to `f` — the map install and projection
    /// override have to reach the whole stack, not just the base.
    pub fn for_each_engine(&mut self, mut f: impl FnMut(&mut Engine)) {
        for s in self.slots.iter_mut() {
            match s {
                Slot::Pattern(e) | Slot::Sprite(e, _) => f(e),
                _ => {}
            }
        }
    }

    /// Composite the whole stack into `dst` (`n` pixels). `base` is the
    /// render task's primary engine, which draws the [`Slot::Base`] layer.
    ///
    /// The walk itself is `luxel_core::compose::SceneDriver` — the SAME
    /// code the wasm playground runs (Gitea #732). This device used to
    /// carry its own copy of it, and the two had drifted: the playground
    /// accumulated the sub-millisecond remainder of each frame delta and
    /// this side truncated it away every frame, so a caption crawled
    /// slower here than in the preview that was supposed to be showing it.
    /// The driver owns that accumulator now, so `dt_ms` is no longer read —
    /// whole milliseconds come from `delta`, remainder carried.
    pub fn render(
        &mut self,
        dst: &mut Vec<[u8; 3]>,
        base: Option<&mut Engine>,
        delta: Fx,
        _dt_ms: u32,
        n: usize,
    ) {
        let Runtime { comp, slots, driver, .. } = self;
        let mut host = SlotHost {
            slots: slots.as_mut_slice(),
            base,
            civil: None,
            text: String::new(),
        };
        // `false` = the staging buffer could not be sized this frame. The
        // host releases it while a plain pattern runs (Gitea #704), so a
        // scene's first frame after an activation grows it by 3 B/px INSIDE
        // the render loop — the exact shape that panicked the Seengreat
        // panel in #702. A frame this board cannot afford is a frame not
        // drawn, not a reboot.
        driver.frame(comp, dst, n, delta, &mut host);
    }
}

/// Decode a stored pattern into a `Program`, borrowing the mapped extent
/// where there is one (no blob Vec) — the `Msg::Library` discipline.
///
/// The caller must have pinned `id` already and must keep it pinned for as
/// long as the engine lives: the `Program` executes from those bytes in
/// place and a save on the other core compacts the arena without asking
/// (Gitea #260).
fn decode(id: &str) -> Option<luxel_core::vm::Program> {
    match patterns::code_of(id) {
        Some(code) => luxel_core::bytecode::deserialize_lean_static(code).ok(),
        None => {
            let bc = patterns::bytecode_of(id)?;
            luxel_core::bytecode::deserialize_lean(&bc).ok()
        }
    }
}

/// The `// @sprite …` tag line of a stored pattern, read out of the mapped
/// source extent. Only the first line is kept — that is all `sprite_view`
/// reads, and a whole source would be tens of KB of heap per sprite layer.
fn tag_line(id: &str) -> String {
    let Some(src) = patterns::source_slice(id) else {
        return String::new();
    };
    let end = src.iter().position(|&b| b == b'\n').unwrap_or(src.len());
    core::str::from_utf8(&src[..end.min(128)])
        .map(String::from)
        .unwrap_or_default()
}

/// Build the resident runtime for `sc`.
///
/// Returns the runtime, the engine the [`Slot::Base`] layer renders from
/// (the caller installs it as the render task's primary engine) and the
/// first per-layer failure, if any — an over-budget or missing layer becomes
/// a [`Slot::Native`] no-op so the rest of the scene still shows.
///
/// `pixels` is the device pixel count; a SPRITE layer's engine is built at
/// ONE pixel, because its `renderFrame` is never called and its frame buffer
/// would otherwise cost 3 B/px for nothing (12 KB each on the panel).
///
/// `keep` is the OUTGOING scene's pinned ids: the arena pin set is
/// republished after every engine, so a save on the other core can never
/// compact a layer that is already built out from under the ones that
/// follow, and the outgoing stack keeps its pins through the whole build.
pub fn build_runtime(
    sc: &Scene,
    pixels: u32,
    grid: Option<GridMap>,
    keep: &[String],
) -> (Runtime, Option<Engine>, Option<String>) {
    let mut rt = Runtime {
        id: sc.id.clone(),
        comp: Compositor::new(grid.unwrap_or(GridMap {
            w: 0,
            h: 0,
            serpentine: false,
        })),
        slots: Vec::new(),
        pinned: Vec::new(),
        driver: SceneDriver::new(),
    };
    rt.comp.set_scene(sc);
    // A scene replaces the whole resident stack, so the per-slot JIT table
    // starts empty and is filled one layer at a time below (Gitea #718).
    // Until a layer compiles, `/api/status` reports `jit.state:"none"` —
    // which is the truth while the stack is being built.
    #[cfg(feature = "jit")]
    {
        crate::jit::reset_stack();
        crate::jit::commit_stack(sc.layers.len());
    }
    let mut base: Option<Engine> = None;
    let mut err: Option<String> = None;
    // The compositor allocates ONE grid-sized scratch, inside the render
    // loop and with an infallible `Vec::resize`, the first time a text layer
    // draws or a layer ramp is applied. It has to come out of the budget
    // HERE, before any engine is built, or the allocation panics the render
    // task instead of being refused — which is exactly what took the
    // Seengreat panel down on 2026-09-24 (`memory allocation of 2688 bytes
    // failed`, one frame after the base engine and its JIT compile had both
    // been accepted). See `budget::compositor_scratch`.
    let scratch = if sc.layers.iter().any(|l| {
        l.kind() == luxel_core::scene::LayerKind::Text
            || matches!(&l.body, luxel_core::scene::LayerBody::Pattern(p) if p.ramp.is_some())
    }) {
        luxel_core::budget::compositor_scratch(pixels)
    } else {
        0
    };
    let fail = |err: &mut Option<String>, n: usize, what: &str| {
        // Every failure arm below becomes a `Slot::Native` no-op, so the
        // layer holds no engine and must not be reported as one — even
        // where `try_budgeted_layer` already recorded a refusal into it
        // (Gitea #718). The choke point, so a new arm cannot forget.
        #[cfg(feature = "jit")]
        crate::jit::clear_slot(n);
        if err.is_none() {
            let mut m = String::from("scene: layer ");
            push_u32(&mut m, n as u32 + 1);
            push_piece(&mut m, " ");
            push_piece(&mut m, what);
            *err = Some(m);
        }
    };
    for (i, layer) in sc.layers.iter().enumerate() {
        let Some(pid) = layer.pattern_id() else {
            rt.slots.push(Slot::Native);
            continue;
        };
        let sprite = layer.kind() == luxel_core::scene::LayerKind::Sprite;
        let count = if sprite { 1 } else { pixels };
        // Name the slot this layer's compile belongs to BEFORE anything is
        // built (Gitea #718). `i` is the scene layer index — the identity —
        // and the first non-sprite layer to build is the one that becomes
        // `Slot::Base`, so it is the engine the scalar `jit` block
        // describes. A sprite layer is armed too: its engine is really
        // built and really compiled (never stepped — docs/spec/scenes.md
        // §4), and hiding that compile is the bug this ticket is about.
        #[cfg(feature = "jit")]
        crate::jit::arm(i, base.is_none() && !sprite, sprite);
        // Pre-flight the heap BEFORE decoding: the post-build floor check in
        // `try_budgeted_layer` is the real gate, but reaching it costs the
        // whole decode + build peak, and on a device already holding two
        // engines that peak is what panics rather than rejects (#479).
        if !luxel_core::budget::layer_fits_with(
            esp_alloc::HEAP.free() as usize,
            count,
            scratch,
            luxel_core::arena::frames_external(),
        ) {
            fail(&mut err, i, "does not fit");
            rt.slots.push(Slot::Native);
            continue;
        }
        patterns::pin_code(pid);
        let prog = decode(pid);
        patterns::unpin_code();
        let Some(prog) = prog else {
            fail(&mut err, i, "has no such pattern");
            rt.slots.push(Slot::Native);
            continue;
        };
        let Ok(mut e) = crate::try_budgeted_layer(prog, count) else {
            fail(&mut err, i, "does not fit");
            rt.slots.push(Slot::Native);
            continue;
        };
        rt.pinned.push(String::from(pid));
        {
            let mut all: Vec<String> = keep.to_vec();
            all.extend(rt.pinned.iter().cloned());
            patterns::set_layer_pins(&all);
        }
        if sprite {
            rt.slots.push(Slot::Sprite(e, tag_line(pid)));
            continue;
        }
        // per-layer control and projection overrides (the playlist's `C`/`P`
        // grammar, applied to this layer's own engine)
        if let luxel_core::scene::LayerBody::Pattern(p) = &layer.body {
            for (name, raw) in &p.controls {
                let vals: Vec<Fx> = raw.iter().map(|&r| Fx::from_raw(r)).collect();
                e.set_control(name, &vals);
            }
            if let Some(mode) = p.proj.and_then(ProjectionMode::from_u8) {
                let mut pr = e.projection();
                pr.set(e.preferred_dims(), mode);
                e.set_projection(pr);
            }
        }
        if base.is_none() {
            base = Some(e);
            rt.slots.push(Slot::Base);
        } else {
            rt.slots.push(Slot::Pattern(e));
        }
    }
    (rt, base, err)
}

// ---- clock text sources ----

/// Local wall clock as `(y, mo, d, h, m, s)`, or `None` before the first
/// SNTP sync — a `clock` text layer then draws nothing (Gitea #745).
pub fn civil_local() -> Option<(u16, u8, u8, u8, u8, u8)> {
    Some(scenestore::civil_from_unix(crate::shared::wall_now_local()?))
}
