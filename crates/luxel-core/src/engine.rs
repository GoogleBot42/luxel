//! Frame pipeline: compile a pattern, run its init code, then per frame run
//! `beforeRender(delta)` and the best-matching render function per pixel.
//!
//! Runtime errors never stop the engine — the first error of a frame is
//! recorded (PB's `vmerr` model) and rendering continues on the next frame.
//!
//! Pixel maps install via [`Engine::set_map`] (host-normalized to world
//! units 0..1 exclusive); render selection follows the documented priority
//! per map dimensionality, missing coordinates fill with mid-space (0.5),
//! and the transform stack applies to 2D/3D coordinates. TODO(oracle).

use alloc::string::String;
use alloc::vec::Vec;

use crate::compile::compile;
use crate::diag::Diagnostic;
use crate::fixed::Fx;
use crate::vm::{MapData, Program, Value, Vm, VmError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenderKind {
    R1(u16),
    R2(u16),
    R3(u16),
}

/// A UI control exported by the pattern (`export function sliderSpeed(v)`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Control {
    pub kind: ControlKind,
    /// Name with the prefix stripped (`sliderSpeed` → `Speed`).
    pub label: String,
    /// Full exported function name.
    pub name: String,
    fn_idx: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlKind {
    Slider,
    HsvPicker,
    RgbPicker,
    Toggle,
    Trigger,
    InputNumber,
    ShowNumber,
    Gauge,
}

const CONTROL_PREFIXES: &[(&str, ControlKind)] = &[
    ("slider", ControlKind::Slider),
    ("hsvPicker", ControlKind::HsvPicker),
    ("rgbPicker", ControlKind::RgbPicker),
    ("toggle", ControlKind::Toggle),
    ("trigger", ControlKind::Trigger),
    ("inputNumber", ControlKind::InputNumber),
    ("showNumber", ControlKind::ShowNumber),
    ("gauge", ControlKind::Gauge),
];

pub struct Engine {
    prog: Program,
    vm: Vm,
    pixel_count: u32,
    before: Option<u16>,
    render: Option<RenderKind>,
    controls: Vec<Control>,
    pixels: Vec<[u8; 3]>,
    /// Pattern-local time in ms, 16-frac for sub-ms delta accumulation.
    time_acc: u64,
    pub last_error: Option<VmError>,
}

impl Engine {
    /// Compile and initialize a pattern. Compile errors fail construction;
    /// runtime errors during init are recorded in `last_error` and the
    /// engine stays usable (PB shows vmerr and keeps going).
    pub fn new(src: &str, pixel_count: u32, seed: u64) -> Result<Engine, Diagnostic> {
        let prog = compile(src)?;
        let mut vm = Vm::new(&prog, seed);
        vm.globals[prog.pixel_count_g as usize] = Value::Num(Fx::from_int(pixel_count as i32));

        // Sensor-board bindings: until a peripheral provides them (M5),
        // exported sensor arrays are stubbed with zeros so sound/motion
        // patterns run dark instead of erroring. Scalars are already 0.
        // The pattern's own init may overwrite these.
        for (name, len) in [
            ("frequencyData", 32),
            ("accelerometer", 3),
            ("analogInputs", 5),
        ] {
            if let Some(i) = prog.global_index(name) {
                if prog.globals[i as usize].export {
                    if let Ok(v) = vm.alloc_array(alloc::vec![Value::default(); len]) {
                        vm.globals[i as usize] = v;
                    }
                }
            }
        }

        let mut last_error = None;
        if let Err(e) = vm.call(&prog, 0, &[]) {
            last_error = Some(e);
        }

        vm.pixel_count = pixel_count;
        let before = prog.exported_fn("beforeRender");
        let render = pick_render(&prog, 0);

        let mut controls = Vec::new();
        for (name, idx) in &prog.exported_fns {
            for (prefix, kind) in CONTROL_PREFIXES {
                if let Some(label) = name.strip_prefix(prefix) {
                    if !label.is_empty() {
                        controls.push(Control {
                            kind: *kind,
                            label: String::from(label),
                            name: name.clone(),
                            fn_idx: *idx,
                        });
                        break;
                    }
                }
            }
        }

        Ok(Engine {
            pixels: alloc::vec![[0u8; 3]; pixel_count as usize],
            prog,
            vm,
            pixel_count,
            before,
            render,
            controls,
            time_acc: 0,
            last_error,
        })
    }

    pub fn pixel_count(&self) -> u32 {
        self.pixel_count
    }

    /// Install a pixel map: one coordinate tuple per pixel, any units.
    /// Coordinates normalize per-axis into world units 0..1 (exclusive —
    /// quantized to u16/65536 like PB's map binary). Render selection
    /// re-picks by map dimensionality.
    pub fn set_map(&mut self, dims: u8, raw: &[[Fx; 3]]) {
        let n = (self.pixel_count as usize).min(raw.len());
        let mut coords = alloc::vec![[Fx::ZERO; 3]; n];
        for axis in 0..(dims as usize).min(3) {
            let mut min = i64::MAX;
            let mut max = i64::MIN;
            for c in raw[..n].iter() {
                min = min.min(c[axis].raw() as i64);
                max = max.max(c[axis].raw() as i64);
            }
            let span = max - min;
            for (i, c) in raw[..n].iter().enumerate() {
                let v = if span == 0 {
                    0
                } else {
                    ((c[axis].raw() as i64 - min) * 65_535 + span / 2) / span
                };
                coords[i][axis] = Fx::from_raw(v as i32);
            }
        }
        self.vm.map = Some(MapData { dims, coords });
        self.render = pick_render(&self.prog, dims);
    }

    /// Provide wall-clock time (unix seconds, timezone already applied) for
    /// the clock builtins.
    pub fn set_wall_clock(&mut self, unix_seconds: i64) {
        self.vm.wall_unix = Some(unix_seconds);
    }

    pub fn controls(&self) -> &[Control] {
        &self.controls
    }

    /// Invoke a control function with values (slider: 1 value, pickers: 3,
    /// trigger: 0). Output controls (showNumber/gauge) return their value.
    pub fn set_control(&mut self, name: &str, values: &[Fx]) -> Option<Fx> {
        let ctl = self
            .controls
            .iter()
            .find(|c| c.name == name || c.label == name)?;
        let fn_idx = ctl.fn_idx;
        let mut args = [Value::default(); 4];
        for (i, v) in values.iter().take(4).enumerate() {
            args[i] = Value::Num(*v);
        }
        match self
            .vm
            .call(&self.prog, fn_idx, &args[..values.len().min(4)])
        {
            Ok(v) => Some(v.num()),
            Err(e) => {
                self.last_error = Some(e);
                None
            }
        }
    }

    /// Read an exported variable.
    pub fn var(&self, name: &str) -> Option<Value> {
        let i = self.prog.global_index(name)?;
        if !self.prog.globals[i as usize].export {
            return None;
        }
        Some(self.vm.globals[i as usize])
    }

    /// Write an exported variable (the `setVars` surface).
    pub fn set_var(&mut self, name: &str, value: Fx) -> bool {
        match self.prog.global_index(name) {
            Some(i) if self.prog.globals[i as usize].export => {
                self.vm.globals[i as usize] = Value::Num(value);
                true
            }
            _ => false,
        }
    }

    /// Exported variable names (the var-watcher surface).
    pub fn exported_vars(&self) -> impl Iterator<Item = &str> {
        self.prog
            .globals
            .iter()
            .filter(|g| g.export)
            .map(|g| g.name.as_str())
    }

    /// Read an element of an exported array variable.
    pub fn var_array(&self, name: &str) -> Option<&[Value]> {
        match self.var(name)? {
            Value::Arr(id) => self.vm.array(id),
            _ => None,
        }
    }

    /// Advance time by `delta_ms` and render one frame.
    pub fn frame(&mut self, delta_ms: Fx) -> &[[u8; 3]] {
        if delta_ms.raw() > 0 {
            self.time_acc += delta_ms.raw() as u64;
        }
        self.vm.time_ms = self.time_acc >> 16;

        if let Some(b) = self.before {
            if let Err(e) = self.vm.call(&self.prog, b, &[Value::Num(delta_ms)]) {
                self.last_error = Some(e);
            }
        }

        let Some(render) = self.render else {
            self.pixels.iter_mut().for_each(|p| *p = [0; 3]);
            return &self.pixels;
        };
        let mid = Fx::from_raw(1 << 15); // 0.5, mid-space fill for missing dims
        for i in 0..self.pixel_count {
            let p = self.vm.pixel_coords(i, [mid; 3]);
            // transforms apply to 2D/3D coordinates (TODO(oracle): 1D x?)
            let p = match render {
                RenderKind::R1(_) => p,
                _ => self.vm.apply_transform(p),
            };
            let all = [
                Value::Num(Fx::from_int(i as i32)),
                Value::Num(p[0]),
                Value::Num(p[1]),
                Value::Num(p[2]),
            ];
            let (fn_idx, argc) = match render {
                RenderKind::R1(f) => (f, 2),
                RenderKind::R2(f) => (f, 3),
                RenderKind::R3(f) => (f, 4),
            };
            self.vm.pixel = [Fx::ZERO; 3];
            self.vm.pixel_written = false;
            match self.vm.call(&self.prog, fn_idx, &all[..argc]) {
                Ok(_) => {
                    let [r, g, b] = self.vm.pixel;
                    self.pixels[i as usize] = [quantize(r), quantize(g), quantize(b)];
                }
                Err(e) => {
                    // record once, blank the rest of the frame, move on
                    self.last_error = Some(e);
                    for p in i as usize..self.pixel_count as usize {
                        self.pixels[p] = [0; 3];
                    }
                    break;
                }
            }
        }
        &self.pixels
    }

    pub fn pixels(&self) -> &[[u8; 3]] {
        &self.pixels
    }

    /// Take and clear the recorded error (hosts poll this per frame).
    pub fn take_error(&mut self) -> Option<VmError> {
        self.last_error.take()
    }
}

/// Fx 0..1 → 0..255, round to nearest. TODO(oracle): PB's exact quantization
/// (and HDR paths) may differ.
fn quantize(v: Fx) -> u8 {
    ((v.clamp(Fx::ZERO, Fx::ONE).raw() as i64 * 255 + 32_768) >> 16) as u8
}

/// Render-function selection priority by map dimensionality (documented PB
/// behavior): no/1D map → render, render3D, render2D; 2D map → render2D,
/// render3D, render; 3D map → render3D, render2D, render.
fn pick_render(prog: &Program, dims: u8) -> Option<RenderKind> {
    let r1 = || prog.exported_fn("render").map(RenderKind::R1);
    let r2 = || prog.exported_fn("render2D").map(RenderKind::R2);
    let r3 = || prog.exported_fn("render3D").map(RenderKind::R3);
    match dims {
        2 => r2().or_else(r3).or_else(r1),
        3 => r3().or_else(r2).or_else(r1),
        _ => r1().or_else(r3).or_else(r2),
    }
}
