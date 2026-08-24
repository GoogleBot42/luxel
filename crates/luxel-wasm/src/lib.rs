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
    match Engine::new_at(src, pixel_count, seed as u64, default_wall_clock()) {
        Ok(engine) => {
            let slot = EngineSlot {
                engine,
                src: src.to_string(),
                pixels: vec![0; pixel_count as usize * 3],
                map_buf: Vec::new(),
                bc: Vec::new(),
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
/// allocator (Gitea #15).
///
/// The sequence mirrors `firmware/src/main.rs`'s `Msg::Code` arm exactly:
/// the whole LXP envelope (name + SOURCE + bytecode) stays resident while
/// `deserialize_lean` runs — that overlap is the real peak for a big pattern,
/// not the engine — then it is dropped; then `try_budgeted_engine` builds the
/// engine with the array budget the device would derive from `heap_free`;
/// then frames render (the array arena settles in the first few). The peak
/// live bytes across that whole window is what the device's post-load floor
/// check sees.
///
/// `envelope_len` is the byte length of the LXP1 envelope that will actually
/// be uploaded (`web/src/lib/device.ts` `lxpEnvelope`). Pass 0 and only the
/// bytecode is assumed resident.
///
/// `heap_free` is the device's `/api/status` `heap_free` — free heap while
/// the CURRENT pattern is still loaded, which is the right baseline because
/// the firmware builds the new engine before releasing the old one.
///
/// Leaves JSON in the response buffer and returns 0:
/// `{"peak":B,"budget":B,"headroom":B,"floor":B,"vmerr":string|null}`
/// Returns -1 (with `{"message":…}`) if the blob will not even decode —
/// which is itself a device-relevant answer.
///
/// `vmerr` reports ONLY the device-specific array *byte* budget failure. The
/// PB-compat 10 240-*element* budget and ordinary runtime errors are the same
/// on every host, so the local preview already shows them; repeating them
/// here would blame the device for a pattern that is simply broken.
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
) -> i32 {
    use luxel_core::budget;

    let blob = std::slice::from_raw_parts(blob_ptr, blob_len);
    let heap_free = heap_free as usize;
    let arena = budget::array_budget(heap_free);

    // Take the baseline BEFORE anything for this pattern is allocated, then
    // arm the peak tracker. Single-threaded wasm and a synchronous call, so
    // nothing else can allocate inside the window. (`blob` itself was
    // allocated by the caller before this point, so it sits in the baseline
    // and is not double-counted against the envelope stand-in below.)
    let base = LIVE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);

    let vmerr = {
        // Stand-in for the uploaded envelope the device is still holding.
        let env = alloc_bytes(envelope_len.max(blob_len));
        let prog = match luxel_core::bytecode::deserialize_lean(blob) {
            Ok(p) => p,
            Err(e) => {
                set_response(format!("{{\"message\":\"{}\"}}", json_escape(&e.to_string())));
                return -1;
            }
        };
        drop(env);
        let mut eng =
            Engine::from_program_budgeted_at(prog, pixel_count, 1, arena, default_wall_clock());
        // Take the INIT error before rendering: top-level `array(...)` calls
        // are where the arena runs out, and the frames that follow would
        // overwrite that with the downstream "not an array" confusion.
        let init_err = eng.take_error();
        for _ in 0..3 {
            let _ = eng.frame(Fx::from_f64(16.7));
        }
        init_err
            .or_else(|| eng.take_error())
            .map(|e| e.message)
            // vm.rs's byte-budget message; see the doc comment above.
            .filter(|m| m.contains("pattern too large for this device"))
    };

    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(base);
    set_response(format!(
        "{{\"peak\":{},\"budget\":{},\"headroom\":{},\"floor\":{},\"vmerr\":{}}}",
        peak,
        arena,
        budget::load_headroom(heap_free),
        budget::RUNTIME_FLOOR,
        match &vmerr {
            Some(m) => format!("\"{}\"", json_escape(m)),
            None => String::from("null"),
        }
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
                        .unwrap_or(&[])
                        .iter()
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
        s.engine.set_map(dims, &coords);
    });
}

/// Install a W×H 2D grid map (row-major, matching the strip order).
#[no_mangle]
pub extern "C" fn lx_set_map_grid(h: i32, w: u32, grid_h: u32) {
    with_engine(h, |s| {
        let n = s.engine.pixel_count();
        let w = w.max(1);
        let _ = grid_h;
        let coords: Vec<[Fx; 3]> = (0..n)
            .map(|i| {
                [
                    Fx::from_int((i % w) as i32),
                    Fx::from_int((i / w) as i32),
                    Fx::ZERO,
                ]
            })
            .collect();
        s.engine.set_map(2, &coords);
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
