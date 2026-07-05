//! Frame pipeline: compile a pattern, run its init code, then per frame run
//! `beforeRender(delta)` and the best-matching render function per pixel.
//!
//! Runtime errors never stop the engine — the first error of a frame is
//! recorded (PB's `vmerr` model) and rendering continues on the next frame.
//!
//! Maps aren't implemented yet (M3): the engine always has 1D geometry, so
//! render selection priority is `render` → `render3D` → `render2D`, with
//! missing coordinates filled with mid-space (0.5). TODO(oracle).

use alloc::string::String;
use alloc::vec::Vec;

use crate::compile::compile;
use crate::diag::Diagnostic;
use crate::fixed::Fx;
use crate::vm::{Program, Value, Vm, VmError};

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

        let before = prog.exported_fn("beforeRender");
        // 1D geometry: render → render3D → render2D
        let render = prog
            .exported_fn("render")
            .map(RenderKind::R1)
            .or_else(|| prog.exported_fn("render3D").map(RenderKind::R3))
            .or_else(|| prog.exported_fn("render2D").map(RenderKind::R2));

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
        let mid = Value::Num(Fx::from_raw(1 << 15)); // 0.5, mid-space fill
        for i in 0..self.pixel_count {
            let x = Fx::from_raw((((i as i64) << 16) / self.pixel_count.max(1) as i64) as i32);
            let all = [Value::Num(Fx::from_int(i as i32)), Value::Num(x), mid, mid];
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
