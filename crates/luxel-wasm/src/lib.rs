//! wasm32 bindings for the Luxel engine — a small hand-rolled C ABI so the
//! web IDE needs no wasm-bindgen glue and the interface stays transparent.
//!
//! Conventions (wasm is single-threaded; the globals are safe by
//! construction and guarded by a Mutex anyway):
//! - Strings cross the boundary as (ptr, len) pairs in linear memory; the
//!   caller allocates input buffers via `lx_alloc` and frees via `lx_dealloc`.
//! - Calls that produce a string (errors, JSON) leave it in a response
//!   buffer read via `lx_response_ptr()` / `lx_response_len()` — valid until
//!   the next call.
//! - Engine handles are indices; freed slots are reused.
//! - Numbers use raw 16.16 fixed-point i32 wherever the pattern domain is
//!   involved (`raw = value * 65536`), so the JS side is explicit about
//!   quantization.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::Mutex;

use luxel_core::diag::line_col;
use luxel_core::engine::{ControlKind, Engine};
use luxel_core::outpipe;
use luxel_core::fixed::Fx;
use luxel_core::vm::{StepKind, Value};

/// Counting allocator — the same instrument `crates/luxel-cli/tests/heapstat.rs`
/// uses to model the device's pattern lifecycle, moved into the wasm build so
/// the browser can run that model against the pattern in the editor
/// (Gitea #15: "we run the exact same pixel VM engine in WASM … while we are
/// executing the script, we see if we go over that threshold").
///
/// wasm32 is a 32-bit target like the ESP32, so pointer-bearing structures
/// measure the same width here as they do on-device — this model is strictly
/// closer to hardware than the 64-bit host test is. Cost is two relaxed
/// atomics per allocation on a single-threaded target; measured at noise
/// level against frame rendering.
struct Counting;

/// The array ELEMENT ledger every engine built from here on enforces.
///
/// PB's 10,236-unit count is a memory budget in disguise, and it is the right
/// default for a host with no device behind it. A board with a dedicated
/// external array arena (Gitea #253) has a real BYTE budget and raises the
/// element ledger out of the way — so a preview that keeps the PB number
/// renders BLACK for an `array(pixelCount)` pattern the device runs happily
/// (`library/fairies.js` on the 64x64 panel: 15,104 elements). A console
/// calls `lx_set_array_elements` with `lx_array_elements_for`'s answer for the
/// device it is bound to; the playground leaves it at the PB default.
static ARRAY_ELEMENTS: AtomicUsize = AtomicUsize::new(luxel_core::vm::DEFAULT_ARRAY_BUDGET);

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let live = LIVE.fetch_add(l.size(), Ordering::Relaxed) + l.size();
        PEAK.fetch_max(live, Ordering::Relaxed);
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        System.dealloc(p, l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        let live = LIVE.fetch_add(new, Ordering::Relaxed) + new;
        PEAK.fetch_max(live, Ordering::Relaxed);
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        System.realloc(p, l, new)
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

static ENGINES: Mutex<Vec<Option<EngineSlot>>> = Mutex::new(Vec::new());
// Wall clock handed to engines at CREATION so top-level init sees
// time-of-day (Gitea #104); i64::MIN = never set. `lx_set_wall_clock`
// still updates a live engine after creation.
static DEFAULT_WALL_CLOCK: AtomicI64 = AtomicI64::new(i64::MIN);

fn default_wall_clock() -> Option<i64> {
    match DEFAULT_WALL_CLOCK.load(Ordering::Relaxed) {
        i64::MIN => None,
        v => Some(v),
    }
}
static RESPONSE: Mutex<String> = Mutex::new(String::new());

struct EngineSlot {
    engine: Engine,
    src: String,
    pixels: Vec<u8>, // flattened RGB copy handed to JS
    map_buf: Vec<i32>, // flattened raw-16.16 [x y z] map coords handed to JS
    bc: Vec<u8>,       // LXBC blob, filled by lx_bytecode
    /// The DEVICE output chain (Gitea #466) — the same
    /// `luxel_core::outpipe::DeviceChain` the firmware runs, so a console
    /// preview can show what the wire will carry instead of the raw engine
    /// frame. Configured by `lx_outpipe_set`, run by `lx_outpipe`; holds no
    /// memory until a stage is switched on.
    chain: outpipe::DeviceChain,
    chain_settings: outpipe::ChainSettings,
    chain_stops: Vec<(u8, [u8; 3])>,
    /// The device's global level AFTER its brightness curve — what the power
    /// cap models (`outpipe::curve_brightness`, applied in `lx_outpipe_set`).
    chain_brightness5: u8,
    chain_model: outpipe::PowerModel,
    outpipe_px: Vec<u8>, // flattened post-chain RGB handed to JS
}

fn set_response(s: String) {
    *RESPONSE.lock().unwrap() = s;
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// # Safety
/// `ptr`/`len` must describe a valid UTF-8 buffer previously written by the
/// caller into linear memory.
unsafe fn str_arg<'a>(ptr: *const u8, len: usize) -> &'a str {
    std::str::from_utf8(std::slice::from_raw_parts(ptr, len)).unwrap_or("")
}

#[no_mangle]
pub extern "C" fn lx_alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len.max(1));
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// # Safety
/// `ptr` must come from `lx_alloc(len)` and not have been freed.
#[no_mangle]
pub unsafe extern "C" fn lx_dealloc(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

#[no_mangle]
pub extern "C" fn lx_response_ptr() -> *const u8 {
    RESPONSE.lock().unwrap().as_ptr()
}

#[no_mangle]
pub extern "C" fn lx_response_len() -> usize {
    RESPONSE.lock().unwrap().len()
}

/// Compile + init a pattern. Returns a handle ≥ 0, or -1 with a JSON
/// diagnostic `{line, col, message}` in the response buffer.
///
/// # Safety
/// `src_ptr`/`src_len` per `str_arg`.
#[no_mangle]
pub unsafe extern "C" fn lx_new(
    src_ptr: *const u8,
    src_len: usize,
    pixel_count: u32,
    seed: u32,
) -> i32 {
    let src = str_arg(src_ptr, src_len);
    // `Engine::new_at` with the host's element ledger spliced in: the browser
    // has no byte limit worth enforcing, but the ELEMENT count is the device's
    // and a preview that ignores it renders black (see `ARRAY_ELEMENTS`).
    let built = luxel_core::compile::compile(src).map(|prog| {
        Engine::from_program_budgeted_at_ext(
            prog,
            pixel_count,
            seed as u64,
            usize::MAX,
            ARRAY_ELEMENTS.load(Ordering::Relaxed),
            default_wall_clock(),
        )
    });
    match built {
        Ok(engine) => {
            let slot = EngineSlot {
                engine,
                src: src.to_string(),
                pixels: vec![0; pixel_count as usize * 3],
                map_buf: Vec::new(),
                bc: Vec::new(),
                chain: outpipe::DeviceChain::new(),
                chain_settings: outpipe::ChainSettings::default(),
                chain_stops: Vec::new(),
                chain_brightness5: 31,
                chain_model: outpipe::PowerModel::Strip,
                outpipe_px: Vec::new(),
            };
            let mut engines = ENGINES.lock().unwrap();
            let h = engines.iter().position(|e| e.is_none());
            match h {
                Some(i) => {
                    engines[i] = Some(slot);
                    i as i32
                }
                None => {
                    engines.push(Some(slot));
                    (engines.len() - 1) as i32
                }
            }
        }
        Err(d) => {
            let (line, col) = line_col(src, d.span.start);
            set_response(format!(
                "{{\"line\":{line},\"col\":{col},\"start\":{},\"end\":{},\"message\":\"{}\"}}",
                d.span.start,
                d.span.end,
                json_escape(&d.message)
            ));
            -1
        }
    }
}

/// Serialize this engine's compiled program to LXBC bytecode (what devices
/// execute — see docs/spec/bytecode.md). Returns the blob length (read it
/// via `lx_bytecode_ptr`, valid until the engine is freed or this is called
/// again) or -1 with an error message in the response buffer.
#[no_mangle]
pub extern "C" fn lx_bytecode(h: i32) -> i32 {
    with_engine(h, |s| match luxel_core::bytecode::serialize(s.engine.program()) {
        Ok(blob) => {
            let len = blob.len() as i32;
            s.bc = blob;
            len
        }
        Err(e) => {
            set_response(format!("{{\"message\":\"{}\"}}", json_escape(&e.to_string())));
            -1
        }
    })
    .unwrap_or(-1)
}

#[no_mangle]
pub extern "C" fn lx_bytecode_ptr(h: i32) -> *const u8 {
    with_engine(h, |s| s.bc.as_ptr()).unwrap_or(std::ptr::null())
}

/// The page-sized staging buffer `firmware/src/patterns.rs::write_raw`
/// allocates for every flash write burst. A live push runs it inside
/// `persist_current_pattern`, while the upload envelope and the freshly
/// decoded program are both still resident — so it belongs to the modelled
/// peak. A stored-pattern activation writes nothing at all (the wear rule).
const FLASH_STAGING: usize = 4096;

/// A real `n`-byte heap allocation the optimiser cannot elide — the
/// counting allocator only sees bytes that are genuinely requested.
#[inline(never)]
fn alloc_bytes(n: usize) -> Vec<u8> {
    let mut v = Vec::new();
    v.resize(n, 0u8);
    std::hint::black_box(v)
}

/// Model what an LXBC blob would cost the connected device's heap, by
/// replaying the firmware's own pattern-load sequence under the counting
/// allocator (Gitea #15, recalibrated for Gitea #287).
///
/// Two lifecycles are replayed, because the firmware has two and they no
/// longer cost the same thing (Gitea #276/#300/#330):
///
/// * **live** — `POST /api/code` → `Msg::Code` (`firmware/src/main.rs`). The
///   whole LXP envelope (name + SOURCE + bytecode) stays resident while
///   `deserialize_lean` COPIES the code and constant pool into an owned
///   `Words::Owned`, and `persist_current_pattern` runs its 4 KiB flash
///   write-staging buffer inside that same window. Then the envelope and the
///   staging go, and only then is the engine built.
/// * **stored** — a pattern in the device's library, activated by id
///   (`Msg::Library`). Nothing travels but the id: the program is decoded
///   with `deserialize_lean_static` straight off the flash mapping, so its
///   code and constant pool are BORROWED and cost no heap at all. No
///   envelope, no blob Vec, no write staging.
///
/// The editor pushes the LIVE path on every recompile, so that is the
/// verdict the UI shows; the stored numbers ride along so it can say when
/// saving to the device library would fit something a live push won't.
///
/// For each lifecycle the model reports two numbers, because the device
/// applies two different tests:
///
/// * `resident` — what the pattern still occupies once the load settles.
///   `try_budgeted_engine`'s floor check measures free heap right after the
///   engine builds, with the envelope already dropped, so *this* is the
///   number that decides acceptance. (Measured after three frames: the array
///   arena settles in the first few.)
/// * `peak` — the transient high-water of the load window. It never reaches
///   the floor check, but the decode's fallible allocations have to fit in
///   free heap, so it is compared against the whole load base.
///
/// `envelope_len` is the byte length of the LXP1 envelope that will actually
/// be uploaded (`web/src/lib/device.ts` `lxpEnvelope`). Pass 0 and only the
/// bytecode is assumed resident.
///
/// `heap_free` is the device's `/api/status` `heap_free` and `engine_heap`
/// Raise (or restore) the array ELEMENT ledger every engine built from here
/// on enforces — `0` means the PB-compat default. See `ARRAY_ELEMENTS`.
///
/// Engines already built keep the ledger they were built with, so a host that
/// changes this has to recompile to see the difference.
#[no_mangle]
pub extern "C" fn lx_set_array_elements(n: u32) {
    let n = if n == 0 {
        luxel_core::vm::DEFAULT_ARRAY_BUDGET
    } else {
        n as usize
    };
    ARRAY_ELEMENTS.store(n, Ordering::Relaxed);
}

/// The element ledger a device reporting these three `/api/status` figures
/// enforces — `luxel_core::budget`'s own arithmetic, so a console never
/// restates it in TypeScript and can never drift from the firmware
/// (`firmware/src/main.rs`'s `element_budget`). `arena_free` 0 = a board
/// without an external arena = the PB-compat count.
#[no_mangle]
pub extern "C" fn lx_array_elements_for(heap_free: u32, engine_heap: u32, arena_free: u32) -> u32 {
    let base = luxel_core::budget::load_base(heap_free as usize, engine_heap as usize);
    model_budget(base, arena_free as usize).1 as u32
}

/// The LXBC format version this bundle's compiler EMITS. The console
/// compares it with `/api/status`'s `bc_format` (what the device READS): a
/// device that reads a newer format cannot run anything saved from here, and
/// one that reads an older format cannot run what is already in its store.
/// Gitea #643 — the Athom went dark across exactly that skew.
#[no_mangle]
pub extern "C" fn lx_bc_format() -> u32 {
    luxel_core::bytecode::FORMAT_VERSION as u32
}

/// The array-arena BYTE budget and ELEMENT ledger a load starting from
/// `free` DRAM gets, on a board with `arena_free` bytes of external arena
/// (0 = none). The twin of `firmware/src/main.rs`'s `array_budget_now` +
/// `element_budget`, which is why both halves live in one place here too.
fn model_budget(free: usize, arena_free: usize) -> (usize, usize) {
    use luxel_core::budget;
    if arena_free == 0 {
        return (budget::array_budget(free), luxel_core::vm::DEFAULT_ARRAY_BUDGET);
    }
    let bytes = budget::external_array_budget(free, arena_free);
    (bytes, budget::external_element_budget(bytes))
}

/// its `engine_heap` — what the CURRENTLY loaded pattern's engine costs, 0
/// on firmware that doesn't report it. The firmware drops the outgoing
/// engine before decoding the incoming one, so the load starts from
/// `budget::load_base(heap_free, engine_heap)`; charging the incoming
/// pattern for the outgoing one is what made the old prediction cry wolf
/// (Gitea #287).
///
/// Leaves JSON in the response buffer and returns 0:
/// `{"resident":B,"peak":B,"budget":B,"storedResident":B,"storedPeak":B,`
/// `"storedBudget":B,"base":B,"headroom":B,"floor":B,"fit":"fits|tight|over",`
/// `"storedFit":…,"vmerr":string|null,"storedVmerr":string|null}`
/// Returns -1 (with `{"message":…}`) if the blob will not even decode —
/// which is itself a device-relevant answer.
///
/// `vmerr` reports ONLY the array-budget refusals
/// (`luxel_core::vm::is_array_budget_error`) — the device's byte budget AND
/// the PB-compat element ledger. Ordinary runtime errors are the same on
/// every host, so the local preview already shows them and repeating them
/// here would blame the device for a pattern that is simply broken.
///
/// The element ledger used to be filtered out on that same reasoning, and it
/// was wrong: the model runs at the DEVICE's pixel count, the preview runs at
/// the editor's layout, and `array(pixelCount)` costs what the rig says.
/// Three such channels fit a 300 px strip and blow the ledger on a 4096 px
/// panel, so the editor answered "fits" for a pattern that loads black
/// (Gitea #420).
///
/// # Safety
/// `blob_ptr`/`blob_len` must describe a valid LXBC buffer in linear memory.
#[no_mangle]
pub unsafe extern "C" fn lx_device_model(
    blob_ptr: *const u8,
    blob_len: usize,
    envelope_len: usize,
    pixel_count: u32,
    heap_free: u32,
    engine_heap: u32,
    arena_free: u32,
) -> i32 {
    use luxel_core::budget;

    let blob = std::slice::from_raw_parts(blob_ptr, blob_len);
    let base_free = budget::load_base(heap_free as usize, engine_heap as usize);

    // --- the live push (`Msg::Code`) -------------------------------------
    // Take the baseline BEFORE anything for this pattern is allocated, then
    // arm the peak tracker. Single-threaded wasm and a synchronous call, so
    // nothing else can allocate inside the window. (`blob` itself was
    // allocated by the caller before this point, so it sits in the baseline
    // and is not double-counted against the envelope stand-in below.)
    let base = LIVE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);

    let (live_resident, live_peak, live_budget, live_vmerr) = {
        // Stand-in for the uploaded envelope the device is still holding.
        let env = alloc_bytes(envelope_len.max(blob_len));
        let after_env = LIVE.load(Ordering::Relaxed);
        let prog = match luxel_core::bytecode::deserialize_lean(blob) {
            Ok(p) => p,
            Err(e) => {
                set_response(format!("{{\"message\":\"{}\"}}", json_escape(&e.to_string())));
                return -1;
            }
        };
        // The program is resident from here on; `persist_current_pattern`
        // runs its page-sized flash staging buffer while the envelope is
        // still alive (`patterns::write_raw`), which is the true high-water.
        let prog_bytes = LIVE.load(Ordering::Relaxed) - after_env;
        let staging = alloc_bytes(FLASH_STAGING);
        drop(staging);
        drop(env);
        // The device reads `HEAP.free()` inside `budgeted_engine`, i.e. with
        // the program decoded and the envelope gone.
        let (arena, elems) =
            model_budget(base_free.saturating_sub(prog_bytes), arena_free as usize);
        let mut eng = Engine::from_program_budgeted_at_ext(
            prog,
            pixel_count,
            1,
            arena,
            elems,
            default_wall_clock(),
        );
        // Take the INIT error before rendering: top-level `array(...)` calls
        // are where the arena runs out, and the frames that follow would
        // overwrite that with the downstream "not an array" confusion.
        let init_err = eng.take_error();
        for _ in 0..3 {
            let _ = eng.frame(Fx::from_f64(16.7));
        }
        let resident = LIVE.load(Ordering::Relaxed) - base;
        let peak = PEAK.load(Ordering::Relaxed) - base;
        let vmerr = init_err
            .or_else(|| eng.take_error())
            .map(|e| e.message)
            // vm.rs's array-budget messages; see the doc comment above.
            .filter(|m| luxel_core::vm::is_array_budget_error(m));
        (resident, peak, arena, vmerr)
    };

    // --- the stored pattern (`Msg::Library`) -----------------------------
    // The blob lives in the flash mapping, so its bytes are not heap at all
    // and `deserialize_lean_static` borrows the code and constant pool. A
    // 4-aligned copy stands in for the mapping; it is allocated OUTSIDE the
    // measured window and freed after, exactly as `heapstat.rs` does it.
    let mut mapping: Vec<u32> = vec![0u32; blob_len.div_ceil(4)];
    let flash: &'static [u8] = {
        let p = mapping.as_mut_ptr() as *mut u8;
        std::ptr::copy_nonoverlapping(blob.as_ptr(), p, blob_len);
        std::slice::from_raw_parts(p as *const u8, blob_len)
    };
    let base = LIVE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    let (stored_resident, stored_peak, stored_budget, stored_vmerr) = {
        // Cannot fail: the live path already decoded this blob.
        let prog = match luxel_core::bytecode::deserialize_lean_static(flash) {
            Ok(p) => p,
            Err(e) => {
                set_response(format!("{{\"message\":\"{}\"}}", json_escape(&e.to_string())));
                return -1;
            }
        };
        let prog_bytes = LIVE.load(Ordering::Relaxed) - base;
        let (arena, elems) =
            model_budget(base_free.saturating_sub(prog_bytes), arena_free as usize);
        let mut eng = Engine::from_program_budgeted_at_ext(
            prog,
            pixel_count,
            1,
            arena,
            elems,
            default_wall_clock(),
        );
        let init_err = eng.take_error();
        for _ in 0..3 {
            let _ = eng.frame(Fx::from_f64(16.7));
        }
        let resident = LIVE.load(Ordering::Relaxed) - base;
        let peak = PEAK.load(Ordering::Relaxed) - base;
        let vmerr = init_err
            .or_else(|| eng.take_error())
            .map(|e| e.message)
            .filter(|m| luxel_core::vm::is_array_budget_error(m));
        (resident, peak, arena, vmerr)
    };
    drop(mapping);

    let quoted = |m: &Option<String>| match m {
        Some(m) => format!("\"{}\"", json_escape(m)),
        None => String::from("null"),
    };
    let verdict = |resident: usize, peak: usize, vmerr: &Option<String>| {
        // The array arena running out is its own rejection, upstream of the
        // floor check — the engine never finishes building.
        if vmerr.is_some() {
            budget::Fit::Over
        } else {
            budget::fit(resident, peak, base_free)
        }
    };
    set_response(format!(
        "{{\"resident\":{},\"peak\":{},\"budget\":{},\
          \"storedResident\":{},\"storedPeak\":{},\"storedBudget\":{},\
          \"base\":{},\"headroom\":{},\"floor\":{},\
          \"fit\":\"{}\",\"storedFit\":\"{}\",\"vmerr\":{},\"storedVmerr\":{}}}",
        live_resident,
        live_peak,
        live_budget,
        stored_resident,
        stored_peak,
        stored_budget,
        base_free,
        budget::load_headroom(base_free),
        budget::RUNTIME_FLOOR,
        verdict(live_resident, live_peak, &live_vmerr).as_str(),
        verdict(stored_resident, stored_peak, &stored_vmerr).as_str(),
        quoted(&live_vmerr),
        quoted(&stored_vmerr),
    ));
    0
}
#[no_mangle]
pub extern "C" fn lx_free(h: i32) {
    if h < 0 {
        return;
    }
    if let Some(slot) = ENGINES.lock().unwrap().get_mut(h as usize) {
        *slot = None;
    }
}

fn with_engine<R>(h: i32, f: impl FnOnce(&mut EngineSlot) -> R) -> Option<R> {
    ENGINES.lock().unwrap().get_mut(h as usize)?.as_mut().map(f)
}

/// Render one frame (delta in raw 16.16 ms) and return a pointer to the
/// RGB byte buffer (pixelCount·3 bytes). Copy it out before the next call.
#[no_mangle]
pub extern "C" fn lx_frame(h: i32, delta_raw: i32) -> *const u8 {
    with_engine(h, |s| {
        let frame = s.engine.frame(Fx::from_raw(delta_raw));
        s.pixels.clear();
        for px in frame {
            s.pixels.extend_from_slice(px);
        }
        s.pixels.as_ptr()
    })
    .unwrap_or(std::ptr::null())
}

/// 1 if a runtime error was recorded (message + location in the response
/// buffer as JSON `{message, line, col}`), else 0. Clears the error.
#[no_mangle]
pub extern "C" fn lx_take_error(h: i32) -> i32 {
    with_engine(h, |s| match s.engine.take_error() {
        Some(e) => {
            set_response(format!(
                "{{\"message\":\"{}\",\"fn\":{},\"pc\":{},\"line\":{},\"col\":{}}}",
                json_escape(&e.message),
                e.fn_idx,
                e.pc,
                e.line,
                e.col
            ));
            let _ = &s.src;
            1
        }
        None => 0,
    })
    .unwrap_or(0)
}

/// Controls of the active pattern as JSON
/// `[{"kind":"slider","label":"Speed","name":"sliderSpeed"}, …]`.
#[no_mangle]
pub extern "C" fn lx_controls(h: i32) -> i32 {
    with_engine(h, |s| {
        let mut out = String::from("[");
        for (i, c) in s.engine.controls().iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let kind = match c.kind {
                ControlKind::Slider => "slider",
                ControlKind::HsvPicker => "hsvPicker",
                ControlKind::RgbPicker => "rgbPicker",
                ControlKind::Toggle => "toggle",
                ControlKind::Trigger => "trigger",
                ControlKind::InputNumber => "inputNumber",
                ControlKind::ShowNumber => "showNumber",
                ControlKind::Gauge => "gauge",
            };
            out.push_str(&format!(
                "{{\"kind\":\"{kind}\",\"label\":\"{}\",\"name\":\"{}\"}}",
                json_escape(&c.label),
                json_escape(&c.name)
            ));
        }
        out.push(']');
        set_response(out);
        1
    })
    .unwrap_or(0)
}

/// Invoke a control with up to 3 raw values. Returns the control's return
/// value (showNumber/gauge) as raw 16.16, or i32::MIN if absent.
///
/// # Safety
/// `name_ptr`/`name_len` per `str_arg`.
#[no_mangle]
pub unsafe extern "C" fn lx_set_control(
    h: i32,
    name_ptr: *const u8,
    name_len: usize,
    v0: i32,
    v1: i32,
    v2: i32,
    argc: u32,
) -> i32 {
    let name = str_arg(name_ptr, name_len);
    with_engine(h, |s| {
        let vals = [Fx::from_raw(v0), Fx::from_raw(v1), Fx::from_raw(v2)];
        match s.engine.set_control(name, &vals[..argc.min(3) as usize]) {
            Some(v) => v.raw(),
            None => i32::MIN,
        }
    })
    .unwrap_or(i32::MIN)
}

/// Exported vars as JSON `{name: raw | [raw, …]}`.
#[no_mangle]
pub extern "C" fn lx_vars(h: i32) -> i32 {
    with_engine(h, |s| {
        let names: Vec<String> = s.engine.exported_vars().map(String::from).collect();
        let mut out = String::from("{");
        for (i, name) in names.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!("\"{}\":", json_escape(name)));
            match s.engine.var(name) {
                Some(Value::Num(v)) => out.push_str(&v.raw().to_string()),
                Some(Value::Arr(_)) => {
                    let vals: Vec<String> = s
                        .engine
                        .var_array(name)
                        .into_iter()
                        .flat_map(|a| a.iter())
                        .map(|v| v.num().raw().to_string())
                        .collect();
                    out.push_str(&format!("[{}]", vals.join(",")));
                }
                _ => out.push_str("null"),
            }
        }
        out.push('}');
        set_response(out);
        1
    })
    .unwrap_or(0)
}

/// # Safety
/// `name_ptr`/`name_len` per `str_arg`.
#[no_mangle]
pub unsafe extern "C" fn lx_set_var(h: i32, name_ptr: *const u8, name_len: usize, raw: i32) -> i32 {
    let name = str_arg(name_ptr, name_len);
    with_engine(h, |s| s.engine.set_var(name, Fx::from_raw(raw)) as i32).unwrap_or(0)
}

/// Install an arbitrary pixel map: `count` coordinate tuples of `dims`
/// (2 or 3) raw-16.16 values each, tightly packed [x y (z)] per pixel.
/// The engine normalizes per axis (any units in, world 0..1 out).
///
/// # Safety
/// `ptr` must point to `count * dims` valid i32s.
#[no_mangle]
pub unsafe extern "C" fn lx_set_map(h: i32, dims: u32, ptr: *const i32, count: usize) {
    let dims = dims.clamp(2, 3) as u8;
    let raw = core::slice::from_raw_parts(ptr, count * dims as usize);
    with_engine(h, |s| {
        let coords: Vec<[Fx; 3]> = (0..count)
            .map(|i| {
                let at = i * dims as usize;
                [
                    Fx::from_raw(raw[at]),
                    Fx::from_raw(raw[at + 1]),
                    if dims == 3 { Fx::from_raw(raw[at + 2]) } else { Fx::ZERO },
                ]
            })
            .collect();
        s.engine.set_map_vec(dims, coords);
    });
}

/// Install a W×H 2D grid map (row-major, matching the strip order).
#[no_mangle]
pub extern "C" fn lx_set_map_grid(h: i32, w: u32, grid_h: u32) {
    with_engine(h, |s| {
        let n = s.engine.pixel_count();
        let w = w.max(1);
        // rows: what the caller says, else enough to cover the strip
        let rows = if grid_h >= 1 { grid_h } else { n.div_ceil(w) };
        s.engine.set_grid_map(w.min(u16::MAX as u32) as u16, rows.min(u16::MAX as u32) as u16);
    });
}

#[no_mangle]
pub extern "C" fn lx_set_wall_clock(h: i32, unix_seconds: f64) {
    with_engine(h, |s| s.engine.set_wall_clock(unix_seconds as i64));
}

/// Wall clock (unix seconds, timezone already applied) for engines created
/// by FUTURE `lx_new` calls — top-level init reads it, which a post-`lx_new`
/// `lx_set_wall_clock` is too late for (Gitea #104). Call before compiling.
#[no_mangle]
pub extern "C" fn lx_set_default_wall_clock(unix_seconds: f64) {
    DEFAULT_WALL_CLOCK.store(unix_seconds as i64, Ordering::Relaxed);
}

/// 1 if the pattern binds any sensor-board variable (frequencyData,
/// energyAverage, …) — the UI uses this to decide whether to run audio
/// capture at all.
#[no_mangle]
pub extern "C" fn lx_wants_sensors(h: i32) -> i32 {
    with_engine(h, |s| s.engine.wants_sensors() as i32).unwrap_or(0)
}

/// The geometry the compiled pattern asks for: 0 = strip, 2 = 2D grid
/// (render2D, or renderFrame + a coordinate/grid-space bulk op), 3 = 3D point
/// cloud. The playground picks its default preview rig from this rather than
/// from the gallery manifest, so a pasted/imported/device-loaded pattern gets
/// the same rig a gallery pick would (Gitea #372).
#[no_mangle]
pub extern "C" fn lx_preferred_dims(h: i32) -> i32 {
    with_engine(h, |s| s.engine.preferred_dims() as i32).unwrap_or(0)
}

/// The dimensionality the pattern DECLARES: `0` = dimensionless (only
/// `renderFrame`, and it paints in index space — native on every Layout),
/// `1` = `render(index)`, `2` = `render2D` or a grid-space `renderFrame`,
/// `3` = `render3D`.
///
/// This, not [`lx_preferred_dims`], is what a projection surface asks:
/// `preferred_dims` answers "does this pattern want a map installed", where
/// `render` and `renderFrame` are the same `0`, while a picker has to tell a
/// 1D pattern (projectable along an axis) from a dimensionless one (nothing
/// to project). Collapsing the two gave `library/fairies.js` a `1D · by index`
/// caption and along-x/along-y options that changed nothing (2026-09-20).
#[no_mangle]
pub extern "C" fn lx_pattern_dims(h: i32) -> i32 {
    with_engine(h, |s| s.engine.pattern_dims() as i32).unwrap_or(0)
}

/// Install the 1D Layout (no map) — the counterpart of `lx_set_map_grid` for
/// a preview rig that really is a strip. Without it a 2D-only pattern keeps
/// the fabricated ceil(√n) grid it is built with and the 2D→1D projections
/// can never come into play (Gitea #473).
#[no_mangle]
pub extern "C" fn lx_set_strip_layout(h: i32) {
    with_engine(h, |s| s.engine.set_strip_layout());
}

/// The Layout's dimensionality as the engine sees it: 1 = strip, 2 = matrix
/// or 2D map, 3 = 3D map. Unaffected by any projection in force.
#[no_mangle]
pub extern "C" fn lx_layout_dims(h: i32) -> i32 {
    with_engine(h, |s| s.engine.layout_dims() as i32).unwrap_or(1)
}

/// Install the projection defaults (Gitea #473): the `proj1d`/`proj2d`/
/// `proj3d` triple as `ProjectionMode` codes (`index`=0, `x`=1, `y`=2,
/// `z`=3, `xy`=4, `xz`=5, `yz`=6 — the same numbers the firmware and the
/// mirror use). An unknown code leaves that field at its default. Returns 1
/// on success, 0 for a bad handle.
#[no_mangle]
pub extern "C" fn lx_set_projection(h: i32, one: u32, two: u32, three: u32) -> i32 {
    use luxel_core::projection::{Projection, ProjectionMode};
    let pick = |v: u32, dflt: ProjectionMode| {
        u8::try_from(v)
            .ok()
            .and_then(ProjectionMode::from_u8)
            .unwrap_or(dflt)
    };
    let d = Projection::DEFAULT;
    let p = Projection::new(
        pick(one, d.proj1d),
        pick(two, d.proj2d),
        pick(three, d.proj3d),
    );
    with_engine(h, |s| {
        s.engine.set_projection(p);
        1
    })
    .unwrap_or(0)
}

/// The installed triple, packed one byte per field:
/// `proj1d | proj2d << 8 | proj3d << 16`. -1 for a bad handle.
#[no_mangle]
pub extern "C" fn lx_projection(h: i32) -> i32 {
    with_engine(h, |s| {
        let p = s.engine.projection();
        p.proj1d.as_u8() as i32 | (p.proj2d.as_u8() as i32) << 8 | (p.proj3d.as_u8() as i32) << 16
    })
    .unwrap_or(-1)
}

/// The projection choices that mean anything for a pattern of
/// `pattern_dims` on a Layout of `layout_dims` (1/2/3 — 0 reads as 1), as
/// JSON in the response buffer: `[{"mode":"x","label":"Along x"}, …]`.
/// Empty for a native pair. Returns the number of options.
///
/// Engine-independent on purpose: a UI builds its pickers from the engine's
/// own table (§5.4d) instead of restating it.
#[no_mangle]
pub extern "C" fn lx_projection_options(pattern_dims: u32, layout_dims: u32) -> i32 {
    use luxel_core::projection::{projection_label, projection_options};
    let (p, l) = (pattern_dims.min(255) as u8, layout_dims.min(255) as u8);
    let opts = projection_options(p, l);
    let mut out = String::from("[");
    for (i, m) in opts.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"mode\":\"{}\",\"code\":{},\"label\":\"{}\"}}",
            m.as_str(),
            m.as_u8(),
            projection_label(*m, p, l)
        ));
    }
    out.push(']');
    set_response(out);
    opts.len() as i32
}

/// What the pattern actually sees once the projection is applied, as JSON in
/// the response buffer: `{"pixelCount":64,"patternDims":1,"layoutDims":2,
/// "w":64,"h":1,"mode":"x","label":"Along x","compatible":true}` (`mode` is
/// null when the pattern is native, and also when it is incompatible —
/// `compatible` false, a Layout asked to show more dimensions than it has,
/// Gitea #538). Returns 1, or 0 for a bad handle.
#[no_mangle]
pub extern "C" fn lx_effective_geometry(h: i32) -> i32 {
    use luxel_core::projection::projection_label;
    with_engine(h, |s| {
        let g = s.engine.effective_geometry();
        let mode = match g.mode {
            Some(m) => format!(
                "\"{}\",\"label\":\"{}\"",
                m.as_str(),
                projection_label(m, g.pattern_dims, g.layout_dims)
            ),
            None => String::from("null,\"label\":null"),
        };
        set_response(format!(
            "{{\"pixelCount\":{},\"patternDims\":{},\"layoutDims\":{},\"w\":{},\"h\":{},\
             \"mode\":{mode},\"compatible\":{}}}",
            g.pixel_count, g.pattern_dims, g.layout_dims, g.w, g.h, g.compatible
        ));
        1
    })
    .unwrap_or(0)
}

/// Inject one sensor frame as 43 packed raw-16.16 i32s:
/// [0..32) frequencyData, [32] energyAverage, [33] maxFrequencyMagnitude,
/// [34] maxFrequency (Hz), [35] light, [36..39) accelerometer,
/// [39..44) analogInputs. Shorter buffers leave the tail fields zero.
///
/// # Safety
/// `ptr` must point to `len` valid i32s (lx_alloc buffers are align-1, so
/// the values are read unaligned).
#[no_mangle]
pub unsafe extern "C" fn lx_set_sensors(h: i32, ptr: *const i32, len: usize) {
    use luxel_core::engine::SensorFrame;
    let at = |i: usize| {
        if i < len {
            Fx::from_raw(ptr.add(i).read_unaligned())
        } else {
            Fx::ZERO
        }
    };
    let mut s = SensorFrame::default();
    for i in 0..32 {
        s.frequency_data[i] = at(i);
    }
    s.energy_average = at(32);
    s.max_frequency_magnitude = at(33);
    s.max_frequency = at(34);
    s.light = at(35);
    for i in 0..3 {
        s.accelerometer[i] = at(36 + i);
    }
    for i in 0..5 {
        s.analog_inputs[i] = at(39 + i);
    }
    with_engine(h, |slot| slot.engine.set_sensors(&s));
}

/// Queue one external event `[type, x, y, value]` (raw 16.16 each) for the
/// pattern to read via `readEvent`. Bounded drop-oldest queue; a pattern
/// that never reads events just lets them age out.
#[no_mangle]
pub extern "C" fn lx_push_event(h: i32, t: i32, x: i32, y: i32, v: i32) {
    with_engine(h, |s| {
        s.engine
            .push_event([Fx::from_raw(t), Fx::from_raw(x), Fx::from_raw(y), Fx::from_raw(v)])
    });
}

/// Drive a digital input pin so `digitalRead(pin)` reports an injected level
/// instead of the pin's `pinMode` idle level (Gitea #177 item 2) — the
/// stand-in for real GPIO the playground, the port-review harness and
/// snap.mjs use to press a button deterministically.
///
/// `level`: 0 = LOW, > 0 = HIGH, < 0 = release (back to the idle level).
/// Returns 1 when the pin was in range and the state was stored, 0 otherwise
/// (unknown handle, or a pin above the tracked window) — a typo'd pin is
/// otherwise indistinguishable from a stuck input.
#[no_mangle]
pub extern "C" fn lx_set_pin(h: i32, pin: i32, level: i32) -> i32 {
    let want = if level < 0 { None } else { Some(level > 0) };
    with_engine(h, |s| s.engine.set_pin(pin, want) as i32).unwrap_or(0)
}

/// The level `digitalRead(pin)` would report right now (1 HIGH, 0 LOW),
/// injected or idle — lets a host show the pin state it is driving.
#[no_mangle]
pub extern "C" fn lx_pin_read(h: i32, pin: i32) -> i32 {
    with_engine(h, |s| s.engine.pin_read(pin) as i32).unwrap_or(0)
}

/// Half of the 64-bit "pins this pattern touched" mask — bit `p` set means
/// the pattern named pin `p + 32 * half` in a `pinMode` or `digitalRead`
/// (Gitea #205). `half`: 0 = pins 0..31, 1 = pins 32..63; anything else is 0.
/// Two i32 halves because the wasm C ABI has no u64 return.
///
/// Pin numbers are runtime values, so a host cannot learn this by reading the
/// bytecode: the playground polls this to decide whether to show a pin panel
/// at all, and which pins go in it.
#[no_mangle]
pub extern "C" fn lx_pins_used(h: i32, half: i32) -> i32 {
    mask_half(with_engine(h, |s| s.engine.pins_used()).unwrap_or(0), half)
}

/// Same packing as [`lx_pins_used`], for the pins that idle HIGH (a `pinMode`
/// pull-up) — what `digitalRead` reports while nothing drives the pin. A host
/// needs it to know which way "press" should move a pin.
#[no_mangle]
pub extern "C" fn lx_pins_idle_high(h: i32, half: i32) -> i32 {
    mask_half(with_engine(h, |s| s.engine.pins_idle_high()).unwrap_or(0), half)
}

/// Drive an analog input pin so `analogRead(pin)` / `touchRead(pin)` report
/// an injected value instead of 0 (Gitea #206) — the analog half of the
/// pin-injection ABI, used by the playground's pin sliders and by the
/// port-review harness to sweep a pot.
///
/// `value` is a raw 16.16 fixed-point number, clamped to 0..1 (the range both
/// builtins report); 0 releases the pin back to its undriven reading. Returns
/// 1 when the pin was in range and the value was stored, 0 otherwise.
#[no_mangle]
pub extern "C" fn lx_set_analog_pin(h: i32, pin: i32, value: i32) -> i32 {
    with_engine(h, |s| s.engine.set_analog_pin(pin, Fx::from_raw(value)) as i32).unwrap_or(0)
}

/// The value `analogRead(pin)` / `touchRead(pin)` would report right now, as
/// a raw 16.16 number — lets a host show back the value it is driving.
#[no_mangle]
pub extern "C" fn lx_analog_read(h: i32, pin: i32) -> i32 {
    with_engine(h, |s| s.engine.analog_read(pin).raw()).unwrap_or(0)
}

/// Same packing as [`lx_pins_used`], for the pins the pattern has read with
/// `analogRead`/`touchRead` (Gitea #206) — which pins deserve a slider.
#[no_mangle]
pub extern "C" fn lx_analog_pins_used(h: i32, half: i32) -> i32 {
    mask_half(with_engine(h, |s| s.engine.analog_pins_used()).unwrap_or(0), half)
}

/// Slice a 64-bit pin mask into the 32-bit half the caller asked for.
fn mask_half(mask: u64, half: i32) -> i32 {
    match half {
        0 => mask as u32 as i32,
        1 => (mask >> 32) as u32 as i32,
        _ => 0,
    }
}

// ---- map programs (this engine emits coordinates, not colors) ----

/// Turn this engine into a map-program runner (per-pixel `plot(x, y[, z])`).
#[no_mangle]
pub extern "C" fn lx_enable_map_mode(h: i32) {
    with_engine(h, |s| s.engine.enable_map_mode());
}

/// Run (or resume) the map program over every pixel, collecting coordinates.
/// Returns 1 if it suspended at a debug stop (resume with `lx_debug_step`), 0
/// when finished. Runtime errors surface via `lx_take_error`.
#[no_mangle]
pub extern "C" fn lx_run_map(h: i32) -> i32 {
    with_engine(h, |s| s.engine.run_map() as i32).unwrap_or(0)
}

/// Dimensionality (2 or 3) of the collected map.
#[no_mangle]
pub extern "C" fn lx_map_dims(h: i32) -> i32 {
    with_engine(h, |s| s.engine.map().0 as i32).unwrap_or(2)
}

/// Number of collected coordinates.
#[no_mangle]
pub extern "C" fn lx_map_count(h: i32) -> i32 {
    with_engine(h, |s| s.engine.map().1.len() as i32).unwrap_or(0)
}

/// Pointer to the collected coordinates as tightly packed raw-16.16 [x y z]
/// triples (count·3 i32s). Valid until the next call.
#[no_mangle]
pub extern "C" fn lx_map_coords(h: i32) -> *const i32 {
    with_engine(h, |s| {
        let (_, coords) = s.engine.map();
        s.map_buf.clear();
        for c in coords {
            s.map_buf.push(c[0].raw());
            s.map_buf.push(c[1].raw());
            s.map_buf.push(c[2].raw());
        }
        s.map_buf.as_ptr()
    })
    .unwrap_or(std::ptr::null())
}

/// Refresh the RGB copy of the engine's current pixel buffer (for redrawing
/// partially-rendered frames while paused) and return its pointer.
#[no_mangle]
pub extern "C" fn lx_pixels(h: i32) -> *const u8 {
    with_engine(h, |s| {
        s.pixels.clear();
        for px in s.engine.pixels() {
            s.pixels.extend_from_slice(px);
        }
        s.pixels.as_ptr()
    })
    .unwrap_or(std::ptr::null())
}

// ---- the device output chain (Gitea #466) ----

/// Field order of `lx_outpipe_set`'s settings buffer. Kept as a named list
/// because the TypeScript wrapper (`web/src/lib/luxel.ts`) builds it by the
/// same indices.
const OUTPIPE_FIELDS: usize = 11;

/// Configure the device output chain for this engine — everything
/// `GET /api/output` reports, plus the two facts the device knows and the
/// wire does not (its brightness and its per-board current model).
///
/// `ptr`/`len` describe a packed i32 array:
///
/// | index | field | units |
/// |---|---|---|
/// | 0 | colour order | 0..5, `outpipe::ColorOrder` codes (`/api/output` `order` name → index) |
/// | 1 | output gamma | tenths (22 = γ2.2); 0 and 10 are off — `/api/output` `gamma` verbatim |
/// | 2 | power cap | mA; 0 = none — `capMa` verbatim |
/// | 3 | device blur | percent — `blur` verbatim |
/// | 4 | device glow | percent — `glow` verbatim |
/// | 5 | palette amount | percent — `paletteAmount` verbatim |
/// | 6 | device brightness | 0..31 — `GET /api/brightness` |
/// | 7 | brightness curve | tenths; 0 and 10 are off — `brightCurve` verbatim |
/// | 8 | power model | 0 = strip, 1 = HUB75 panel |
/// | 9 | panel scan | panel rows / 2; read only when [8] is 1 |
/// | 10 | palette stop count | followed by that many `pos, r, g, b` quads (0..255 each) — `/api/output` `palette` is exactly that flat array |
///
/// Returns 1 on success, 0 for a bad handle or a short buffer.
///
/// The cooked palette LUT is re-cooked only when the STOP LIST actually
/// changes, so calling this every frame with unchanged settings is cheap —
/// but it is meant to be called when the device's settings change.
///
/// # Safety
/// `ptr` must point to `len` valid i32s (lx_alloc buffers are align-1, so the
/// values are read unaligned).
#[no_mangle]
pub unsafe extern "C" fn lx_outpipe_set(h: i32, ptr: *const i32, len: usize) -> i32 {
    if len < OUTPIPE_FIELDS {
        return 0;
    }
    let at = |i: usize| ptr.add(i).read_unaligned();
    let byte = |i: usize| at(i).clamp(0, 255) as u8;
    let n_stops = at(10).max(0) as usize;
    if len < OUTPIPE_FIELDS + n_stops * 4 {
        return 0;
    }
    let mut stops: Vec<(u8, [u8; 3])> = Vec::with_capacity(n_stops);
    for i in 0..n_stops {
        let o = OUTPIPE_FIELDS + i * 4;
        stops.push((byte(o), [byte(o + 1), byte(o + 2), byte(o + 3)]));
    }
    let brightness = at(6).clamp(0, 31) as u8;
    let curve = byte(7);
    let model = if at(8) == 1 {
        outpipe::PowerModel::Hub75 { scan: at(9).clamp(1, u16::MAX as i32) as u16 }
    } else {
        outpipe::PowerModel::Strip
    };
    with_engine(h, |s| {
        // Bump the epoch only when the stops really moved: the chain re-cooks
        // its 256-entry luma->colour table on an epoch change, and a caller
        // that re-sends the same settings must not pay for that.
        if s.chain_stops != stops {
            s.chain_stops = stops;
            s.chain_settings.palette_epoch = s.chain_settings.palette_epoch.wrapping_add(1);
        }
        s.chain_settings.order = outpipe::ColorOrder(byte(0).min(5));
        s.chain_settings.gamma_tenths = byte(1);
        s.chain_settings.cap_ma = at(2).max(0) as u32;
        s.chain_settings.blur_pct = byte(3).min(100);
        s.chain_settings.glow_pct = byte(4).min(100);
        s.chain_settings.palette_pct = byte(5).min(100);
        s.chain_brightness5 = outpipe::curve_brightness(brightness, curve);
        s.chain_model = model;
        1
    })
    .unwrap_or(0)
}

/// Run the configured device output chain over the engine's CURRENT frame and
/// return a pointer to the result (pixelCount·3 bytes) — what the wire would
/// carry, as opposed to `lx_frame`'s raw engine output.
///
/// The engine's own `Engine::grid()` supplies the geometry, so blur and glow
/// are two-dimensional exactly when the device's would be. With every stage
/// off this is the engine frame verbatim and costs one branch.
///
/// Call after `lx_frame`; the buffer is valid until the next call.
#[no_mangle]
pub extern "C" fn lx_outpipe(h: i32) -> *const u8 {
    with_engine(h, |s| {
        let EngineSlot { engine, chain, chain_settings, chain_stops, outpipe_px, .. } = s;
        let grid = engine.grid();
        let wire = chain.apply(
            engine.pixels(),
            chain_settings,
            s.chain_brightness5,
            grid,
            s.chain_model,
            || chain_stops.clone(),
        );
        outpipe_px.clear();
        for px in wire {
            outpipe_px.extend_from_slice(px);
        }
        outpipe_px.as_ptr()
    })
    .unwrap_or(std::ptr::null())
}

/// Bytes the device chain is holding for this engine — the 3 B/px scratch
/// plus whichever LUTs are cooked, 0 while every stage is off (Gitea
/// #446/#476). Exposed so the playground can show the same number the device
/// reports losing from `heap_free`.
#[no_mangle]
pub extern "C" fn lx_outpipe_bytes(h: i32) -> i32 {
    with_engine(h, |s| s.chain.resident_bytes() as i32).unwrap_or(0)
}

// ---- debugger ----

#[no_mangle]
pub extern "C" fn lx_debug_enable(h: i32, on: i32) {
    with_engine(h, |s| s.engine.debug_set_enabled(on != 0));
}

/// Replace the breakpoint set (1-based source lines). Writes the resolved
/// lines to the response buffer as a JSON array.
///
/// # Safety
/// `ptr`/`len` must describe `len` u32 line numbers in linear memory.
#[no_mangle]
pub unsafe extern "C" fn lx_debug_set_breakpoints(h: i32, ptr: *const u32, len: usize) {
    // lx_alloc gives align-1 buffers; read the u32s unaligned
    let lines: Vec<u32> = (0..len).map(|i| ptr.add(i).read_unaligned()).collect();
    with_engine(h, |s| {
        let resolved = s.engine.debug_set_breakpoints(&lines);
        let items: Vec<String> = resolved.iter().map(|l| l.to_string()).collect();
        set_response(format!("[{}]", items.join(",")));
    });
}

#[no_mangle]
pub extern "C" fn lx_debug_pause(h: i32) {
    with_engine(h, |s| s.engine.debug_pause());
}

#[no_mangle]
pub extern "C" fn lx_debug_paused(h: i32) -> i32 {
    with_engine(h, |s| s.engine.debug_paused() as i32).unwrap_or(0)
}

/// Resume with a step plan: 0=continue, 1=over, 2=into, 3=out.
/// Returns 1 if still paused afterwards.
#[no_mangle]
pub extern "C" fn lx_debug_step(h: i32, kind: u32) -> i32 {
    let kind = match kind {
        1 => StepKind::Over,
        2 => StepKind::Into,
        3 => StepKind::Out,
        _ => StepKind::Continue,
    };
    with_engine(h, |s| s.engine.debug_step(kind) as i32).unwrap_or(0)
}

/// Debug snapshot as JSON:
/// {"paused":bool, "line":n, "col":n, "pixel":n|null,
///  "stack":[{"name":…, "line":n, "col":n,
///            "locals":[{"name":…, "raw":n} | {"name":…, "array":len} |
///                      {"name":…, "fn":idx}]}]}
#[no_mangle]
pub extern "C" fn lx_debug_state(h: i32) -> i32 {
    with_engine(h, |s| {
        if !s.engine.debug_paused() {
            set_response("{\"paused\":false}".to_string());
            return 1;
        }
        let (line, col, pixel) = s.engine.debug_location().unwrap_or((0, 0, None));
        let mut out = format!(
            "{{\"paused\":true,\"line\":{line},\"col\":{col},\"pixel\":{},\"stack\":[",
            pixel.map(|p| p.to_string()).unwrap_or("null".to_string())
        );
        for (i, f) in s.engine.debug_stack().iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"name\":\"{}\",\"line\":{},\"col\":{},\"locals\":[",
                json_escape(&f.name),
                f.line,
                f.col
            ));
            for (j, (name, value)) in f.locals.iter().enumerate() {
                if j > 0 {
                    out.push(',');
                }
                out.push_str(&named_value_json(&s.engine, name, *value));
            }
            out.push_str("]}");
        }
        out.push_str("],\"globals\":[");
        for (i, (name, value)) in s.engine.debug_globals().iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&named_value_json(&s.engine, name, *value));
        }
        out.push_str("]}");
        set_response(out);
        1
    })
    .unwrap_or(0)
}

fn named_value_json(engine: &Engine, name: &str, value: Value) -> String {
    let name = json_escape(name);
    match value {
        Value::Num(v) => format!("{{\"name\":\"{name}\",\"raw\":{}}}", v.raw()),
        Value::Arr(id) => format!("{{\"name\":\"{name}\",\"array\":{}}}", engine.array_len(id)),
        Value::Fun(idx) | Value::Builtin(idx) => {
            format!("{{\"name\":\"{name}\",\"fn\":{idx}}}")
        }
    }
}

/// All user-defined globals with current values, as a JSON array (hover
/// inspection while running; the paused snapshot embeds the same data).
#[no_mangle]
pub extern "C" fn lx_globals(h: i32) -> i32 {
    with_engine(h, |s| {
        let mut out = String::from("[");
        for (i, (name, value)) in s.engine.debug_globals().iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&named_value_json(&s.engine, name, *value));
        }
        out.push(']');
        set_response(out);
        1
    })
    .unwrap_or(0)
}

/// What the editor tells you about the JIT, for the pattern this handle is
/// running (Gitea #627, docs/jit-design.md §4a). One call after every
/// successful compile; JSON in the response buffer, always 1:
///
/// ```json
/// {"jit":{"eligible":false,
///         "reason":{"kind":"callbacks","name":"arrayMutate",
///                   "line":3,"col":3,"message":"…"}},
///  "dyn":[{"name":"heat","scope":"global","fn":"","line":3,"col":3,
///          "cause":"assign-merge","message":"`heat` is assigned …"}],
///  "stats":{"typed_slots":12,"total_slots":14}}
/// ```
///
/// `reason` is absent when `eligible` is true. `scope` is `global`,
/// `local` or `ret` (a function's return value); `fn` is the function the
/// slot lives in, empty for a global. Positions are 1-based and are `0`
/// only for a program with no debug info, which a browser compile never is.
#[no_mangle]
pub extern "C" fn lx_kinds(h: i32) -> i32 {
    use luxel_core::jitlint::{cause_id, dyn_lints, jit_eligibility, slot_stats};
    with_engine(h, |s| {
        let prog = s.engine.program();
        // `compile()` always infers; `infer` is the belt-and-braces path for
        // a program that arrived some other way.
        let kinds = match prog.kinds.clone() {
            Some(k) => k,
            None => luxel_core::kinds::infer(prog),
        };
        let mut out = String::from("{\"jit\":{");
        match jit_eligibility(prog, &kinds) {
            Ok(()) => out.push_str("\"eligible\":true"),
            Err(r) => {
                let (line, col) = r.pos();
                out.push_str(&format!(
                    "\"eligible\":false,\"reason\":{{\"kind\":\"{}\",\"name\":\"{}\",\
                     \"line\":{line},\"col\":{col},\"message\":\"{}\"}}",
                    r.id(),
                    json_escape(r.name()),
                    json_escape(&r.text()),
                ));
            }
        }
        out.push_str("},\"dyn\":[");
        for (i, l) in dyn_lints(prog, &kinds).iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"name\":\"{}\",\"scope\":\"{}\",\"fn\":\"{}\",\"line\":{},\"col\":{},\
                 \"cause\":\"{}\",\"message\":\"{}\"}}",
                json_escape(&l.name),
                l.scope.id(),
                json_escape(&l.fn_name),
                l.line,
                l.col,
                cause_id(l.cause),
                json_escape(&l.message),
            ));
        }
        let stats = slot_stats(prog, &kinds);
        out.push_str(&format!(
            "],\"stats\":{{\"typed_slots\":{},\"total_slots\":{}}}}}",
            stats.typed, stats.total
        ));
        set_response(out);
        1
    })
    .unwrap_or(0)
}

// ---- scene compositor (Gitea #477/#478/#481/#482) ----
//
// The playground has never had a crossfade, let alone a layer stack: one
// engine per handle, one frame, straight to the canvas. A scene preview that
// blended in JS would diverge from the device by the whole compositor, so the
// blend lives in `luxel_core::compose` and this is the plumbing — the same
// move `lx_outpipe_set`/`lx_outpipe` made for the output chain.
//
// A compositor owns its layer stack and its clocks; the ENGINES it draws are
// ordinary handles, bound per layer. Pattern layers are stepped through
// `Engine::frame`, exactly as `lx_frame` does (frame-rate cap and time
// scaling included). A SPRITE layer holds no engine at all since Gitea
// #740: it holds an `LXSP` RECORD, pushed in per layer with
// `lx_comp_sprite`, and the compositor reads its texels straight out of
// those bytes.
//
// The C ABI this section adds:
//
// ```text
// lx_comp_new(w: u32, h: u32) -> i32                        // handle, or -1
// lx_comp_free(ch: i32)
// lx_comp_set(ch, ptr, len) -> i32                          // scene wire; -1 + reason
// lx_comp_bind(ch, layer: u32, engine_handle: i32)           // PATTERN layers only
// lx_comp_sprite(ch, layer: u32, ptr, len) -> i32            // LXSP record; len 0 clears
// lx_comp_text(ch, layer: u32, ptr, len)
// lx_comp_layer_count(ch) -> u32
// lx_comp_frame(ch, delta_raw: i32) -> *const u8             // w·h·3, or null
// ```

use luxel_core::compose::{Compositor, SceneDriver, SceneHost, SpriteView};
use luxel_core::outpipe::GridMap;
use luxel_core::scene::Scene;

struct CompSlot {
    comp: Compositor,
    scene: Scene,
    /// Engine handle per layer; -1 = unbound. PATTERN layers only.
    bind: Vec<i32>,
    /// The `LXSP` record a SPRITE layer draws, per layer (`None` = nothing
    /// bound). Sized by `lx_comp_set` alongside `bind`; pushed in by
    /// `lx_comp_sprite`, which is the only writer.
    sprites: Vec<Option<Vec<u8>>>,
    px: Vec<[u8; 3]>,
    out: Vec<u8>,
    /// The SHARED full-frame driver (Gitea #732) — the same walk the device
    /// runs, and the owner of the sub-millisecond remainder of the frame
    /// deltas, so a 60 fps caller's 16.67 ms steps do not round the scroll
    /// and sprite clocks down.
    driver: SceneDriver,
}

/// The playground's side of [`SceneHost`]: a layer's engine is an ordinary
/// engine handle, bound per layer.
///
/// [`SceneHost::text`] is left at its default `None` — the JS host resolves
/// clock and slot text and pushes it in through `lx_comp_text`, which is
/// the contract (docs/spec/scenes.md §2: the compositor reads no wall
/// clock).
struct WasmHost<'a> {
    bind: &'a [i32],
    sprites: &'a [Option<Vec<u8>>],
    engines: std::sync::MutexGuard<'a, Vec<Option<EngineSlot>>>,
}

impl WasmHost<'_> {
    fn slot_mut(&mut self, layer: usize) -> Option<&mut EngineSlot> {
        let h = *self.bind.get(layer)?;
        if h < 0 {
            return None;
        }
        self.engines.get_mut(h as usize)?.as_mut()
    }
}

impl SceneHost for WasmHost<'_> {
    fn pattern_frame(&mut self, layer: usize, delta: Fx) -> Option<&[[u8; 3]]> {
        // the same call `lx_frame` makes — frame-rate cap, time scaling and all
        Some(self.slot_mut(layer)?.engine.frame(delta))
    }

    fn sprite(&mut self, layer: usize) -> Option<SpriteView<'_>> {
        // validated once by `lx_comp_sprite`; per frame only the shape
        SpriteView::parse_trusted(self.sprites.get(layer)?.as_deref()?)
    }
}

static COMPOSITORS: Mutex<Vec<Option<CompSlot>>> = Mutex::new(Vec::new());

fn with_comp<R>(ch: i32, f: impl FnOnce(&mut CompSlot) -> R) -> Option<R> {
    if ch < 0 {
        return None;
    }
    COMPOSITORS.lock().unwrap().get_mut(ch as usize)?.as_mut().map(f)
}

/// Create a compositor over a `w`x`h` ROW-MAJOR grid — the same geometry
/// `lx_set_map_grid` gives an engine, so a layer's frame and the composite
/// share a pixel order. Returns a handle ≥ 0.
#[no_mangle]
pub extern "C" fn lx_comp_new(w: u32, h: u32) -> i32 {
    let grid = GridMap {
        w: w.min(u16::MAX as u32) as u16,
        h: h.min(u16::MAX as u32) as u16,
        serpentine: false,
    };
    let slot = CompSlot {
        comp: Compositor::new(grid),
        scene: Scene::default(),
        bind: Vec::new(),
        sprites: Vec::new(),
        px: Vec::new(),
        out: Vec::new(),
        driver: SceneDriver::new(),
    };
    let mut comps = COMPOSITORS.lock().unwrap();
    match comps.iter().position(|c| c.is_none()) {
        Some(i) => {
            comps[i] = Some(slot);
            i as i32
        }
        None => {
            comps.push(Some(slot));
            (comps.len() - 1) as i32
        }
    }
}

#[no_mangle]
pub extern "C" fn lx_comp_free(ch: i32) {
    if ch < 0 {
        return;
    }
    if let Some(slot) = COMPOSITORS.lock().unwrap().get_mut(ch as usize) {
        *slot = None;
    }
}

/// Install a scene from its wire block (`docs/spec/scenes.md`). Returns 0,
/// or -1 with the parse error — `scene: line N: …`, the same string the
/// device's API returns — in the response buffer. Engine bindings AND
/// sprite records are cleared, and both tables are sized to the new
/// layer count.
///
/// # Safety
/// `ptr`/`len` per `str_arg`.
#[no_mangle]
pub unsafe extern "C" fn lx_comp_set(ch: i32, ptr: *const u8, len: usize) -> i32 {
    let wire = str_arg(ptr, len);
    match luxel_core::scene::parse(wire) {
        Ok(scene) => with_comp(ch, |c| {
            c.comp.set_scene(&scene);
            c.bind = vec![-1; scene.layers.len()];
            c.sprites = vec![None; scene.layers.len()];
            c.scene = scene;
            0
        })
        .unwrap_or(-1),
        Err(msg) => {
            set_response(msg);
            -1
        }
    }
}

/// Bind an engine handle to a PATTERN layer's renderer. `-1` unbinds.
///
/// Sprite layers have no engine since Gitea #740 — they take a record
/// through [`lx_comp_sprite`], and binding an engine to one does nothing.
#[no_mangle]
pub extern "C" fn lx_comp_bind(ch: i32, layer: u32, engine_handle: i32) {
    with_comp(ch, |c| {
        if let Some(b) = c.bind.get_mut(layer as usize) {
            *b = engine_handle;
        }
    });
}

/// Give a SPRITE layer its `LXSP` record (Gitea #740). The bytes are COPIED
/// into the compositor slot, so the caller may free its buffer immediately.
///
/// `len == 0` clears the layer (it then draws nothing). Returns 0 on
/// success and -1 with the reason from `luxel_core::sprite::check` — the
/// same `sprite: …` sentence `POST /api/sprites` answers with — in the
/// response buffer. A bad record leaves whatever the layer already had.
///
/// # Safety
/// `ptr` must be valid for `len` bytes (or `len` must be 0).
#[no_mangle]
pub unsafe extern "C" fn lx_comp_sprite(ch: i32, layer: u32, ptr: *const u8, len: usize) -> i32 {
    if len == 0 {
        return with_comp(ch, |c| {
            if let Some(s) = c.sprites.get_mut(layer as usize) {
                *s = None;
            }
            0
        })
        .unwrap_or(-1);
    }
    if ptr.is_null() {
        set_response(String::from("sprite: record is too short"));
        return -1;
    }
    let bytes = std::slice::from_raw_parts(ptr, len);
    if let Err(why) = luxel_core::sprite::check(bytes) {
        set_response(String::from(why));
        return -1;
    }
    with_comp(ch, |c| match c.sprites.get_mut(layer as usize) {
        Some(s) => {
            *s = Some(bytes.to_vec());
            0
        }
        // A layer index past the stack is not a record error, so it gets no
        // `sprite: …` sentence — just the -1 a bad handle gets.
        None => -1,
    })
    .unwrap_or(-1)
}

/// Set a text layer's resolved string. Clock and slot sources are the
/// HOST's to resolve — the compositor never reads a wall clock.
///
/// # Safety
/// `ptr`/`len` per `str_arg`.
#[no_mangle]
pub unsafe extern "C" fn lx_comp_text(ch: i32, layer: u32, ptr: *const u8, len: usize) {
    let s = str_arg(ptr, len);
    with_comp(ch, |c| c.comp.set_text(layer as usize, s));
}

#[no_mangle]
pub extern "C" fn lx_comp_layer_count(ch: i32) -> u32 {
    with_comp(ch, |c| c.comp.layer_count() as u32).unwrap_or(0) as u32
}

/// Step every bound pattern engine, composite the whole stack bottom → top
/// and return a pointer to the result (w·h·3 RGB bytes), or null. Copy it
/// out before the next call. Feed it through `lx_outpipe` on an engine
/// configured with the device's chain to see what the wire would carry.
///
/// The walk is `luxel_core::compose::SceneDriver`, the SAME code the
/// device's render task runs (Gitea #732) — this function is the binding,
/// not a second driver. Clock and slot text stay the host's: resolve them
/// in JS and push them in with `lx_comp_text`.
#[no_mangle]
pub extern "C" fn lx_comp_frame(ch: i32, delta_raw: i32) -> *const u8 {
    let mut comps = COMPOSITORS.lock().unwrap();
    let Some(Some(c)) = comps.get_mut(ch as usize) else {
        return std::ptr::null();
    };
    let n = c.comp.grid().len();
    let CompSlot { comp, bind, sprites, px, out, driver, .. } = c;
    let mut host = WasmHost {
        bind: bind.as_slice(),
        sprites: sprites.as_slice(),
        engines: ENGINES.lock().unwrap(),
    };
    // The whole walk — buffer sizing, the remainder-carrying millisecond
    // clock, `advance` and the kind dispatch — is `luxel-core`'s, and is
    // the same code the device runs (Gitea #732).
    let drawn = driver.frame(comp, px, n, Fx::from_raw(delta_raw), &mut host);
    drop(host);
    if !drawn {
        // the driver could not size the frame — null, like a bad handle,
        // rather than a pointer to a buffer that is not w·h·3 bytes long
        return std::ptr::null();
    }

    out.clear();
    for p in px.iter() {
        out.extend_from_slice(p);
    }
    out.as_ptr()
}

/// `text::set_slot(n, s)` — the playground's stand-in for `POST /api/text`
/// (Gitea #485). Slots are device-level state, not per-engine, so this is a
/// free function like `lx_alloc` rather than a handle method: a pattern
/// drawing `textSlot(0)` in the preview sees what the UI typed, exactly as
/// it would on the device, and so does a scene's `slot` text layer.
///
/// # Safety
/// `ptr`/`len` per `str_arg`.
#[no_mangle]
pub unsafe extern "C" fn lx_text_slot_set(n: u32, ptr: *const u8, len: u32) {
    luxel_core::text::set_slot(n.min(255) as u8, str_arg(ptr, len as usize));
}
