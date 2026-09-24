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
use core::sync::atomic::{AtomicU32, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use esp_println::println;
use luxel_core::compose::{self, Compositor};
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::jsonview::{push_piece, push_u32};
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

/// Persist `list` and, only if the store took it, adopt it. Every mutation
/// funnels through here so a refused write leaves RAM and flash agreeing.
fn commit(list: Vec<Scene>) -> Result<(), String> {
    let blob = scenestore::blob_of(&list);
    if blob.len() > patterns::BLOB_MAX {
        return Err(scenestore::too_big(blob.len(), patterns::BLOB_MAX));
    }
    if !patterns::store_blob(patterns::SCENES_KEY, blob.as_bytes()) {
        return Err(String::from("scenes: the store refused the record"));
    }
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

/// `GET /api/scenes`.
pub fn to_json() -> String {
    let layers_max = crate::server::scene_layer_cap();
    SCENES.lock(|c| {
        let list = c.borrow();
        let mut out = String::from("{\"active\":");
        let active = ACTIVE.lock(|a| a.borrow().clone());
        if active.is_empty() {
            push_piece(&mut out, "null");
        } else {
            push_piece(&mut out, "\"");
            push_piece(&mut out, &active);
            push_piece(&mut out, "\"");
        }
        push_piece(&mut out, ",\"layers_max\":");
        push_u32(&mut out, layers_max as u32);
        push_piece(&mut out, ",\"used\":");
        push_u32(&mut out, scenestore::blob_of(&list).len() as u32);
        push_piece(&mut out, ",\"max\":");
        push_u32(&mut out, patterns::BLOB_MAX as u32);
        push_piece(&mut out, ",\"scenes\":[");
        for (i, s) in list.iter().enumerate() {
            if i > 0 {
                push_piece(&mut out, ",");
            }
            scene::push_json(s, &mut out);
        }
        push_piece(&mut out, "]}");
        out
    })
}

/// `GET /api/scenes/<id>`.
pub fn get_json(id: &str) -> Option<String> {
    SCENES.lock(|c| {
        c.borrow().iter().find(|s| s.id == id).map(|s| {
            let mut out = String::new();
            scene::push_json(s, &mut out);
            out
        })
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
    let (list, target) = scenestore::upsert(
        &list,
        body,
        id,
        next,
        patterns::BLOB_MAX,
        crate::server::scene_layer_cap() as usize,
    )?;
    if id.is_none() && target == scenestore::id_hex(next) {
        NEXT_SEQ.store(next.wrapping_add(1), Ordering::Relaxed);
    }
    commit(list)?;
    Ok(target)
}

/// `DELETE /api/scenes/<id>` — also drops every playlist item that named it.
pub fn delete(id: &str) -> Result<(), String> {
    let list = SCENES.lock(|c| c.borrow().clone());
    commit(scenestore::remove(&list, id)?)?;
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
    pub fn render(
        &mut self,
        dst: &mut Vec<[u8; 3]>,
        mut base: Option<&mut Engine>,
        delta: Fx,
        dt_ms: u32,
        n: usize,
    ) {
        self.comp.advance(dt_ms);
        // Clock and slot sources are the HOST's to resolve; `lit` is seeded
        // by `Compositor::set_scene` and needs no call.
        let civil = civil_local();
        for i in 0..self.slots.len() {
            let resolved = match self.comp.text_source(i) {
                Some(TextSource::Clock(f)) => Some(match civil {
                    Some((y, mo, d, h, m, s)) => text::format_clock(*f, h, m, s, y, mo, d),
                    // no SNTP sync yet: say so rather than showing a
                    // plausible wrong time (contract §1)
                    None => String::from("--:--"),
                }),
                Some(TextSource::Slot(k)) => Some(text::with_slot(*k, |v: &str| String::from(v))),
                _ => None,
            };
            if let Some(s) = resolved {
                self.comp.set_text(i, &s);
            }
        }
        dst.clear();
        dst.resize(n, [0, 0, 0]);
        let Runtime { comp, slots, .. } = self;
        for (i, slot) in slots.iter_mut().enumerate() {
            match slot {
                Slot::Native => comp.native_layer(dst, i, None),
                Slot::Base => {
                    if let Some(e) = base.as_deref_mut() {
                        let f = e.frame(delta);
                        comp.pattern_layer(dst, i, f);
                    }
                }
                Slot::Pattern(e) => {
                    let f = e.frame(delta);
                    comp.pattern_layer(dst, i, f);
                }
                Slot::Sprite(e, src) => {
                    let view = compose::sprite_view(&*e, &*src);
                    comp.native_layer(dst, i, view.as_ref());
                }
            }
        }
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
    };
    rt.comp.set_scene(sc);
    let mut base: Option<Engine> = None;
    let mut err: Option<String> = None;
    let fail = |err: &mut Option<String>, n: usize, what: &str| {
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
        // Pre-flight the heap BEFORE decoding: the post-build floor check in
        // `try_budgeted_engine` is the real gate, but reaching it costs the
        // whole decode + build peak, and on a device already holding two
        // engines that peak is what panics rather than rejects (#479).
        if !luxel_core::budget::layer_fits(esp_alloc::HEAP.free() as usize, count) {
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
        let Ok(mut e) = crate::try_budgeted_engine(prog, count) else {
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
/// SNTP sync — a `clock` text layer then renders `--:--`.
pub fn civil_local() -> Option<(u16, u8, u8, u8, u8, u8)> {
    Some(scenestore::civil_from_unix(crate::shared::wall_now_local()?))
}
