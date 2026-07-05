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

use std::sync::Mutex;

use luxel_core::diag::line_col;
use luxel_core::engine::{ControlKind, Engine};
use luxel_core::fixed::Fx;
use luxel_core::vm::Value;

static ENGINES: Mutex<Vec<Option<EngineSlot>>> = Mutex::new(Vec::new());
static RESPONSE: Mutex<String> = Mutex::new(String::new());

struct EngineSlot {
    engine: Engine,
    src: String,
    pixels: Vec<u8>, // flattened RGB copy handed to JS
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
    match Engine::new(src, pixel_count, seed as u64) {
        Ok(engine) => {
            let slot = EngineSlot {
                engine,
                src: src.to_string(),
                pixels: vec![0; pixel_count as usize * 3],
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
                "{{\"line\":{line},\"col\":{col},\"message\":\"{}\"}}",
                json_escape(&d.message)
            ));
            -1
        }
    }
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
            // fn/pc → source line is future work (needs a source map);
            // report the message and function index for now
            set_response(format!(
                "{{\"message\":\"{}\",\"fn\":{},\"pc\":{}}}",
                json_escape(&e.message),
                e.fn_idx,
                e.pc
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
