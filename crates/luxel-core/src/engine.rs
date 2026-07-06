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
use crate::vm::{DebugState, MapData, Outcome, Program, StepKind, Value, Vm, VmError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RenderKind {
    R1(u16),
    R2(u16),
    R3(u16),
}

/// Where a debug-paused frame pipeline is suspended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RunStage {
    Before,
    Pixel(u32),
}

/// One entry of the paused call stack, for debugger UIs.
#[derive(Clone, Debug)]
pub struct DebugFrame {
    pub name: String,
    pub fn_idx: u16,
    pub pc: u32,
    pub line: u32,
    pub col: u32,
    pub locals: Vec<(String, Value)>,
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
    debug_enabled: bool,
    /// Some(stage) while the pipeline is suspended at a debug stop.
    run_stage: Option<RunStage>,
    cur_delta: Fx,
    /// 256-entry output curve for `setGamma` — rebuilt only when the value
    /// changes, so the per-pixel cost is one table lookup, not a pow().
    gamma_lut: Option<alloc::boxed::Box<[u8; 256]>>,
    gamma_lut_for: Fx,
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
            debug_enabled: false,
            run_stage: None,
            cur_delta: Fx::ZERO,
            gamma_lut: None,
            gamma_lut_for: Fx::ZERO,
        })
    }

    // ---- debugger ----

    /// Enable/disable debugging. Disabling abandons any paused run.
    pub fn debug_set_enabled(&mut self, on: bool) {
        if on == self.debug_enabled {
            return;
        }
        self.debug_enabled = on;
        if on {
            self.vm.dbg = Some(DebugState::default());
        } else {
            self.vm.dbg = None;
            self.vm.clear_run();
            self.run_stage = None;
        }
    }

    /// Set breakpoints by 1-based source line; returns the lines that
    /// resolved to code (for gutter feedback). Replaces the previous set.
    /// Install breakpoints by 1-based source line. A line with no
    /// instructions (blank, comment, brace) snaps forward to the nearest
    /// executable line; a line past all code is dropped. Returns the
    /// resolved lines so UIs can move their markers accordingly.
    pub fn debug_set_breakpoints(&mut self, lines: &[u32]) -> Vec<u32> {
        let mut pcs = Vec::new();
        let mut resolved = Vec::new();
        for &line in lines {
            // nearest executable line >= requested
            let mut target: Option<u32> = None;
            for f in &self.prog.fns {
                for &(l, _) in &f.pos {
                    if l >= line && l != 0 {
                        target = Some(target.map_or(l, |t| t.min(l)));
                    }
                }
            }
            let Some(t) = target else { continue };
            for (fi, f) in self.prog.fns.iter().enumerate() {
                if let Some(pc) = f.pos.iter().position(|&(l, _)| l == t) {
                    pcs.push((fi as u16, pc as u32));
                }
            }
            if !resolved.contains(&t) {
                resolved.push(t);
            }
        }
        resolved.sort_unstable();
        if let Some(d) = self.vm.dbg.as_mut() {
            d.breakpoints = pcs;
        }
        resolved
    }

    /// Request a pause at the next instruction of the next/current frame.
    pub fn debug_pause(&mut self) {
        if let Some(d) = self.vm.dbg.as_mut() {
            d.pause_requested = true;
        }
    }

    pub fn debug_paused(&self) -> bool {
        self.run_stage.is_some()
    }

    /// Resume a paused run with the given stepping behavior. Execution flows
    /// across callback boundaries: stepping past the end of render(i) lands
    /// at the top of render(i+1). Returns whether still paused.
    pub fn debug_step(&mut self, kind: StepKind) -> bool {
        if self.run_stage.is_some() {
            self.drive(Some(kind));
        }
        self.run_stage.is_some()
    }

    /// (line, col, pixel-index-if-in-render) of the paused position.
    pub fn debug_location(&self) -> Option<(u32, u32, Option<u32>)> {
        self.run_stage?;
        let f = self.vm.frames().last()?;
        let (line, col) = self.prog.fns[f.fn_idx as usize].pos_at(f.pc);
        let pixel = match self.run_stage {
            Some(RunStage::Pixel(i)) => Some(i),
            _ => None,
        };
        Some((line, col, pixel))
    }

    /// The paused call stack, innermost frame first, with named locals.
    pub fn debug_stack(&self) -> Vec<DebugFrame> {
        let frames = self.vm.frames();
        frames
            .iter()
            .enumerate()
            .rev()
            .map(|(i, f)| {
                let def = &self.prog.fns[f.fn_idx as usize];
                // the top frame's pc is next-to-execute; parents' point past
                // their call instruction
                let pc = if i == frames.len() - 1 {
                    f.pc
                } else {
                    f.pc.saturating_sub(1)
                };
                let (line, col) = def.pos_at(pc);
                let locals = def
                    .local_names
                    .iter()
                    .cloned()
                    .zip(self.vm.frame_locals(f, def.locals as usize).iter().copied())
                    .collect();
                DebugFrame {
                    name: def.name.clone(),
                    fn_idx: f.fn_idx,
                    pc,
                    line,
                    col,
                    locals,
                }
            })
            .collect()
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

    /// All user-defined globals with current values (debugger scope pane —
    /// implicit assignments create globals, so this is where most pattern
    /// state lives). Predefined constants are filtered out.
    pub fn debug_globals(&self) -> Vec<(String, Value)> {
        self.prog
            .globals
            .iter()
            .enumerate()
            .filter(|(_, g)| !g.predefined)
            .map(|(i, g)| (g.name.clone(), self.vm.globals[i]))
            .collect()
    }

    /// Length of a VM array by id (debugger display).
    pub fn array_len(&self, id: u32) -> usize {
        self.vm.array(id).map(|a| a.len()).unwrap_or(0)
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
        if self.run_stage.is_some() {
            // paused at a debug stop mid-frame: time frozen, pixels as-is
            return &self.pixels;
        }
        if delta_ms.raw() > 0 {
            self.time_acc += delta_ms.raw() as u64;
        }
        self.vm.time_ms = self.time_acc >> 16;
        self.cur_delta = delta_ms;
        self.run_stage = Some(RunStage::Before);
        self.drive(None);
        &self.pixels
    }

    /// Advance the (resumable) frame pipeline until it finishes the frame or
    /// suspends at a debug stop. `resume` continues a paused VM run with the
    /// given step plan; None starts the next stage fresh.
    fn drive(&mut self, mut resume: Option<StepKind>) {
        loop {
            let Some(stage) = self.run_stage else { return };
            let outcome = if let Some(k) = resume.take() {
                self.vm.resume(&self.prog, k)
            } else {
                match stage {
                    RunStage::Before => match self.before {
                        Some(b) => self.vm.start(
                            &self.prog,
                            b,
                            &[Value::Num(self.cur_delta)],
                            self.debug_enabled,
                        ),
                        None => Ok(Outcome::Done(Value::default())),
                    },
                    RunStage::Pixel(i) => {
                        let Some(render) = self.render else {
                            self.pixels.iter_mut().for_each(|p| *p = [0; 3]);
                            self.run_stage = None;
                            return;
                        };
                        self.vm.pixel = [Fx::ZERO; 3];
                        self.vm.pixel_written = false;
                        let (fn_idx, args, argc) = self.render_args(render, i);
                        self.vm
                            .start(&self.prog, fn_idx, &args[..argc], self.debug_enabled)
                    }
                }
            };
            match outcome {
                Err(e) => {
                    // record once, blank the rest of the frame, move on
                    self.last_error = Some(e);
                    if let Some(RunStage::Pixel(i)) = self.run_stage {
                        for p in i as usize..self.pixel_count as usize {
                            self.pixels[p] = [0; 3];
                        }
                    }
                    self.run_stage = None;
                    return;
                }
                Ok(Outcome::Paused) => return,
                Ok(Outcome::Done(_)) => match stage {
                    RunStage::Before => {
                        if self.render.is_none() {
                            self.pixels.iter_mut().for_each(|p| *p = [0; 3]);
                            self.run_stage = None;
                            return;
                        }
                        if self.pixel_count == 0 {
                            self.run_stage = None;
                            return;
                        }
                        self.run_stage = Some(RunStage::Pixel(0));
                    }
                    RunStage::Pixel(i) => {
                        let [r, g, b] = self.vm.pixel;
                        let mut px = [quantize(r), quantize(g), quantize(b)];
                        if let Some(lut) = self.gamma_lut() {
                            px = [lut[px[0] as usize], lut[px[1] as usize], lut[px[2] as usize]];
                        }
                        self.pixels[i as usize] = px;
                        if i + 1 < self.pixel_count {
                            self.run_stage = Some(RunStage::Pixel(i + 1));
                        } else {
                            self.run_stage = None;
                            return;
                        }
                    }
                },
            }
        }
    }

    fn render_args(&self, render: RenderKind, i: u32) -> (u16, [Value; 4], usize) {
        let mid = Fx::from_raw(1 << 15); // 0.5, mid-space fill for missing dims
        let p = self.vm.pixel_coords(i, [mid; 3]);
        // transforms apply to 2D/3D coordinates (TODO(oracle): 1D x?)
        let p = match render {
            RenderKind::R1(_) => p,
            _ => self.vm.apply_transform(p),
        };
        let args = [
            Value::Num(Fx::from_int(i as i32)),
            Value::Num(p[0]),
            Value::Num(p[1]),
            Value::Num(p[2]),
        ];
        match render {
            RenderKind::R1(f) => (f, args, 2),
            RenderKind::R2(f) => (f, args, 3),
            RenderKind::R3(f) => (f, args, 4),
        }
    }

    pub fn pixels(&self) -> &[[u8; 3]] {
        &self.pixels
    }

    /// The output curve for the current `setGamma` value (rebuilt on change;
    /// gamma 0/1 disables). 255 always maps to 255.
    fn gamma_lut(&mut self) -> Option<&[u8; 256]> {
        let g = self.vm.post_gamma;
        if g == Fx::ZERO || g == Fx::ONE {
            self.gamma_lut = None;
            self.gamma_lut_for = g;
            return None;
        }
        if self.gamma_lut.is_none() || self.gamma_lut_for != g {
            let mut lut = alloc::boxed::Box::new([0u8; 256]);
            for (i, slot) in lut.iter_mut().enumerate() {
                let v = Fx::from_raw(((i as i32) << 16) / 255);
                *slot = quantize(crate::fmath::pow(v, g));
            }
            lut[255] = 255;
            self.gamma_lut = Some(lut);
            self.gamma_lut_for = g;
        }
        self.gamma_lut.as_deref()
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
