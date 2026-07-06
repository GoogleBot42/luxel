//! Bytecode program representation and the stack VM.
//!
//! Design notes:
//! - One scalar domain (`Fx`) plus reference values (arrays, functions,
//!   builtins), matching the PB model. Arithmetic on a reference treats it
//!   as 0 (permissive; TODO(oracle)).
//! - Arrays live in an arena and are never freed — re-binding a variable
//!   orphans the old array permanently, exactly like PB. Total element
//!   budget defaults to PB's 10,240.
//! - Runtime errors never panic or halt the engine: the first error per
//!   call is recorded (message + function + pc, `vmerr` style) and the
//!   callback aborts; the frame pipeline keeps running.
//! - Fuel and call-depth guards keep hostile/buggy patterns from hanging a
//!   host.
//!
//! The in-memory `Insn` enum IS the bytecode for now; the packed/serialized
//! encoding (the stable multi-frontend ABI) gets specified once the
//! instruction set survives contact with the corpus — see docs/spec/.

use alloc::string::String;
use alloc::vec::Vec;
use alloc::{format, vec};

use crate::fixed::Fx;
use crate::fmath;

// ---- program ----

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Insn {
    Const(Value),
    LoadG(u16),
    StoreG(u16), // pops value, pushes it back (assignment is an expression)
    LoadL(u8),
    StoreL(u8), // ditto
    LoadIdx,    // [arr idx] → [elem]
    StoreIdx,   // [arr idx val] → [val]
    ArrLen,     // [arr] → [len]
    NewArray(u16),
    Dup,
    Dup2, // [a b] → [a b a b]
    Pop,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Pow,
    Neg,
    Not,
    BitNot,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    Jmp(u32),
    JmpIfFalse(u32),     // pops
    JmpIfTruePeek(u32),  // ||: jump keeping the lhs value
    JmpIfFalsePeek(u32), // &&
    CallFn { fn_idx: u16, argc: u8 },
    CallBuiltin { b: u16, argc: u8 },
    CallValue { argc: u8 },
    Ret,
    RetNull,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Num(Fx),
    Arr(u32),
    Fun(u16),
    Builtin(u16),
}

impl Default for Value {
    fn default() -> Self {
        Value::Num(Fx::ZERO)
    }
}

impl Value {
    #[inline]
    pub fn num(self) -> Fx {
        match self {
            Value::Num(v) => v,
            _ => Fx::ZERO, // TODO(oracle): arithmetic on refs
        }
    }

    #[inline]
    pub fn truthy(self) -> bool {
        match self {
            Value::Num(v) => v.is_truthy(),
            _ => true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FnDef {
    pub name: String,
    pub params: u8,
    /// Total local slots including params.
    pub locals: u8,
    pub code: Vec<Insn>,
    /// Debug info: 1-based (line, col) per instruction; (0, 0) = unknown.
    pub pos: Vec<(u32, u32)>,
    /// Debug info: name per local slot (params first).
    pub local_names: Vec<String>,
}

impl FnDef {
    pub fn placeholder(name: String) -> FnDef {
        FnDef {
            name,
            params: 0,
            locals: 0,
            code: Vec::new(),
            pos: Vec::new(),
            local_names: Vec::new(),
        }
    }

    /// (line, col) of an instruction, if known.
    pub fn pos_at(&self, pc: u32) -> (u32, u32) {
        self.pos.get(pc as usize).copied().unwrap_or((0, 0))
    }
}

#[derive(Debug, Clone)]
pub struct GlobalDef {
    pub name: String,
    pub export: bool,
    pub init: Fx,
    /// Engine-provided constant (PI, pixelCount, GPIO names, …) — hidden
    /// from the debugger's globals pane.
    pub predefined: bool,
}

#[derive(Debug, Clone)]
pub struct Program {
    /// `fns[0]` is top-level initialization code.
    pub fns: Vec<FnDef>,
    pub globals: Vec<GlobalDef>,
    /// Exported functions (render, beforeRender, controls, …) by name.
    pub exported_fns: Vec<(String, u16)>,
    /// Global slot holding `pixelCount`.
    pub pixel_count_g: u16,
}

impl Program {
    pub fn exported_fn(&self, name: &str) -> Option<u16> {
        self.exported_fns
            .iter()
            .find(|(n, _)| n == name)
            .map(|&(_, i)| i)
    }

    pub fn global_index(&self, name: &str) -> Option<u16> {
        self.globals
            .iter()
            .position(|g| g.name == name)
            .map(|i| i as u16)
    }
}

// ---- builtins ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BKind {
    Impl(Builtin),
    /// Documented PB builtin we haven't implemented yet: resolves at compile
    /// time (so the corpus compiles) but raises a runtime error when called.
    Todo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    Abs,
    Floor,
    Ceil,
    Round,
    Trunc,
    Frac,
    Clamp,
    Min,
    Max,
    Mod,
    Sqrt,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Atan2,
    Pow,
    Exp,
    Log,
    Log2,
    Hypot,
    Hypot3,
    // Luxel extension builtins (not in Pixel Blaze). Pure math; adding
    // builtins can't break existing patterns.
    Map,
    Sign,
    Step,
    Saturate,
    Dist,
    Dist3,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutQuad,
    EaseInCubic,
    EaseOutCubic,
    EaseInOutCubic,
    Oklch,
    Oklab,
    Random,
    Prng,
    PrngSeed,
    Time,
    Wave,
    Square,
    Triangle,
    Mix,
    Smoothstep,
    BezierQuadratic,
    BezierCubic,
    Hsv,
    Rgb,
    Array,
    ArrayLength,
    ArraySum,
    ArrayForEach,
    ArrayMutate,
    ArrayMapTo,
    ArrayReduce,
    ArrayReplace,
    ArrayReplaceAt,
    ArraySort,
    ArraySortBy,
    // transforms & map
    ResetTransform,
    Transform,
    Translate,
    Scale,
    Rotate,
    Translate3D,
    Scale3D,
    RotateX,
    RotateY,
    RotateZ,
    PixelMapDimensions,
    Has2DMap,
    Has3DMap,
    MapPixels,
    // noise & palettes
    Perlin,
    PerlinFbm,
    PerlinRidge,
    PerlinTurbulence,
    SetPerlinWrap,
    SetPalette,
    Paint,
    // clock
    ClockYear,
    ClockMonth,
    ClockDay,
    ClockHour,
    ClockMinute,
    ClockSecond,
    ClockWeekday,
    // GPIO / sequencer / sync stubs (no-ops until real peripherals, M4/M5)
    PinMode,
    DigitalWrite,
    DigitalRead,
    AnalogRead,
    TouchRead,
    SequencerNext,
    SequencerGetMode,
    PlaylistGetPosition,
    PlaylistSetPosition,
    PlaylistGetLength,
    NodeId,
    // Luxel extension builtins, batch 2: tempo helpers, deterministic
    // hashing, array filters, vector math, value-returning color
    Beat,
    BeatSin,
    Hash,
    Hash2,
    Blur1D,
    Feedback,
    Dot,
    Dot3,
    AngleBetween,
    Rgb2Hsv,
    Hsv2Rgb,
    MixColors,
    Simplex2,
    Simplex3,
    SetGamma,
    // Luxel map programs: emit one coordinate per pixel (see engine map mode).
    Plot,
}

pub struct BuiltinDef {
    pub name: &'static str,
    pub kind: BKind,
}

macro_rules! b {
    ($name:literal, $v:ident) => {
        BuiltinDef {
            name: $name,
            kind: BKind::Impl(Builtin::$v),
        }
    };
    ($name:literal) => {
        BuiltinDef {
            name: $name,
            kind: BKind::Todo,
        }
    };
}

/// All documented builtins. Order is the builtin id — append only.
#[rustfmt::skip]
pub static BUILTINS: &[BuiltinDef] = &[
    b!("abs", Abs), b!("floor", Floor), b!("ceil", Ceil), b!("round", Round),
    b!("trunc", Trunc), b!("frac", Frac), b!("clamp", Clamp), b!("min", Min),
    b!("max", Max), b!("mod", Mod), b!("sqrt", Sqrt), b!("sin", Sin),
    b!("cos", Cos), b!("tan", Tan), b!("asin", Asin), b!("acos", Acos),
    b!("atan", Atan), b!("atan2", Atan2), b!("pow", Pow), b!("exp", Exp),
    b!("log", Log), b!("log2", Log2), b!("hypot", Hypot), b!("hypot3", Hypot3),
    // Luxel extensions
    b!("map", Map), b!("sign", Sign), b!("step", Step), b!("saturate", Saturate),
    b!("dist", Dist), b!("dist3", Dist3),
    b!("easeInQuad", EaseInQuad), b!("easeOutQuad", EaseOutQuad),
    b!("easeInOutQuad", EaseInOutQuad), b!("easeInCubic", EaseInCubic),
    b!("easeOutCubic", EaseOutCubic), b!("easeInOutCubic", EaseInOutCubic),
    b!("oklch", Oklch), b!("oklab", Oklab),
    b!("random", Random), b!("prng", Prng), b!("prngSeed", PrngSeed),
    b!("time", Time), b!("wave", Wave), b!("square", Square),
    b!("triangle", Triangle), b!("mix", Mix), b!("smoothstep", Smoothstep),
    b!("bezierQuadratic", BezierQuadratic), b!("bezierCubic", BezierCubic),
    b!("hsv", Hsv), b!("hsv24", Hsv), b!("rgb", Rgb),
    b!("array", Array), b!("arrayLength", ArrayLength), b!("arraySum", ArraySum),
    b!("arrayForEach", ArrayForEach), b!("arrayMutate", ArrayMutate),
    b!("arrayMapTo", ArrayMapTo), b!("arrayReduce", ArrayReduce),
    b!("arrayReplace", ArrayReplace), b!("arrayReplaceAt", ArrayReplaceAt),
    b!("arraySort", ArraySort), b!("arraySortBy", ArraySortBy),
    b!("perlin", Perlin), b!("perlinFbm", PerlinFbm), b!("perlinRidge", PerlinRidge),
    b!("perlinTurbulence", PerlinTurbulence), b!("setPerlinWrap", SetPerlinWrap),
    b!("resetTransform", ResetTransform), b!("transform", Transform),
    b!("translate", Translate), b!("scale", Scale), b!("rotate", Rotate),
    b!("translate3D", Translate3D), b!("scale3D", Scale3D), b!("rotateX", RotateX),
    b!("rotateY", RotateY), b!("rotateZ", RotateZ),
    b!("pixelMapDimensions", PixelMapDimensions), b!("has2DMap", Has2DMap),
    b!("has3DMap", Has3DMap), b!("mapPixels", MapPixels),
    b!("setPalette", SetPalette), b!("paint", Paint),
    b!("pinMode", PinMode), b!("digitalWrite", DigitalWrite),
    b!("digitalRead", DigitalRead), b!("analogRead", AnalogRead),
    b!("touchRead", TouchRead), b!("clockYear", ClockYear), b!("clockMonth", ClockMonth),
    b!("clockDay", ClockDay), b!("clockHour", ClockHour), b!("clockMinute", ClockMinute),
    b!("clockSecond", ClockSecond), b!("clockWeekday", ClockWeekday),
    b!("sequencerNext", SequencerNext), b!("sequencerGetMode", SequencerGetMode),
    b!("playlistGetPosition", PlaylistGetPosition),
    b!("playlistSetPosition", PlaylistSetPosition),
    b!("playlistGetLength", PlaylistGetLength), b!("nodeId", NodeId),
    // Luxel extensions, batch 2 (appended — table order is the builtin id)
    b!("beat", Beat), b!("beatSin", BeatSin),
    b!("hash", Hash), b!("hash2", Hash2),
    b!("blur1D", Blur1D), b!("feedback", Feedback),
    b!("dot", Dot), b!("dot3", Dot3), b!("angleBetween", AngleBetween),
    b!("rgb2hsv", Rgb2Hsv), b!("hsv2rgb", Hsv2Rgb), b!("mixColors", MixColors),
    b!("simplex2", Simplex2), b!("simplex3", Simplex3),
    b!("setGamma", SetGamma),
    b!("plot", Plot),
];

pub fn lookup_builtin(name: &str) -> Option<u16> {
    BUILTINS
        .iter()
        .position(|d| d.name == name)
        .map(|i| i as u16)
}

/// Method-form array API: `a.mutate(f)` etc.
pub fn lookup_method(name: &str) -> Option<u16> {
    let global = match name {
        "forEach" => "arrayForEach",
        "mutate" => "arrayMutate",
        "mapTo" => "arrayMapTo",
        "reduce" => "arrayReduce",
        "replace" => "arrayReplace", // TODO(oracle): offset form via method
        "sort" => "arraySort",
        "sortBy" => "arraySortBy",
        "sum" => "arraySum",
        _ => return None,
    };
    lookup_builtin(global)
}

// ---- VM ----

#[derive(Debug, Clone, PartialEq)]
pub struct VmError {
    pub message: String,
    pub fn_idx: u16,
    pub pc: u32,
    /// 1-based source location; (0, 0) if unknown.
    pub line: u32,
    pub col: u32,
}

/// A suspended (or active) pattern-function activation. `pc` points at the
/// next instruction to execute.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub fn_idx: u16,
    pub pc: u32,
    pub locals_base: u32,
    pub stack_base: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    Continue,
    Over,
    Into,
    Out,
}

#[derive(Debug, Default)]
pub struct DebugState {
    /// Resolved breakpoints as (fn_idx, pc).
    pub breakpoints: Vec<(u16, u32)>,
    step: Option<StepPlan>,
    pub pause_requested: bool,
    /// Skip checks once right after resuming so the paused instruction does
    /// not immediately re-trigger.
    skip_once: bool,
}

#[derive(Debug, Clone, Copy)]
struct StepPlan {
    kind: StepKind,
    depth: usize,
    fn_idx: u16,
    line: u32,
}

/// Result of a debuggable run: finished, or suspended awaiting resume().
#[derive(Debug)]
pub enum Outcome {
    Done(Value),
    Paused,
}

const MAX_DEPTH: usize = 48;
const MAX_STACK: usize = 1024;
const MAX_ARGS: usize = 16;
pub const DEFAULT_ARRAY_BUDGET: usize = 10_240;
const FUEL: u32 = 8_000_000;

pub struct Vm {
    pub globals: Vec<Value>,
    arrays: Vec<Vec<Value>>,
    array_elems: usize,
    pub array_budget: usize,
    stack: Vec<Value>,
    locals: Vec<Value>,
    frames: Vec<Frame>,
    /// Debugger state; None disables all checks (the fast path).
    pub dbg: Option<DebugState>,
    fuel: u32,
    /// Milliseconds since pattern start; the engine advances this.
    pub time_ms: u64,
    rng: u64,
    prng_state: u32,
    /// Set by hsv()/rgb() — the engine reads this after each render call.
    pub pixel: [Fx; 3],
    pub pixel_written: bool,
    /// Set by plot() in a map program — the engine reads this after each
    /// per-pixel map call to build the coordinate list. `plot_dims` is 2 or 3
    /// per the arg count of the last plot() this pixel.
    pub plot_coord: [Fx; 3],
    pub plot_dims: u8,
    pub plot_written: bool,
    /// Current coordinate transform (pre-multiplied ops; points transform in
    /// call order — the corpus `translate(-.5,-.5); rotate(θ)` idiom).
    pub transform: [[Fx; 4]; 4],
    pub transform_active: bool,
    transform_ops: u32,
    /// Installed pixel map (engine-set): dims (1/2/3) + normalized coords.
    pub map: Option<MapData>,
    /// Engine-set; used by mapPixels and the no-map 1D fallback.
    pub pixel_count: u32,
    palette: Vec<(Fx, [Fx; 3])>,
    perlin_wrap: [i32; 3],
    /// Wall-clock unix seconds (timezone-adjusted by the host); None → the
    /// clock builtins return 0. TODO(oracle): PB's no-time behavior.
    pub wall_unix: Option<i64>,
    /// Output gamma set by `setGamma(g)`; ZERO/ONE = off. The engine applies
    /// it after render, at quantization (Luxel post-process extension).
    pub post_gamma: Fx,
}

#[derive(Debug, Clone)]
pub struct MapData {
    pub dims: u8,
    pub coords: Vec<[Fx; 3]>,
}

pub const IDENTITY: [[Fx; 4]; 4] = {
    let o = Fx::ONE;
    let z = Fx::ZERO;
    [[o, z, z, z], [z, o, z, z], [z, z, o, z], [z, z, z, o]]
};

impl Vm {
    pub fn new(prog: &Program, seed: u64) -> Vm {
        Vm {
            globals: prog.globals.iter().map(|g| Value::Num(g.init)).collect(),
            arrays: Vec::new(),
            array_elems: 0,
            array_budget: DEFAULT_ARRAY_BUDGET,
            stack: Vec::new(),
            locals: Vec::new(),
            frames: Vec::new(),
            dbg: None,
            fuel: FUEL,
            time_ms: 0,
            transform: IDENTITY,
            transform_active: false,
            transform_ops: 0,
            map: None,
            pixel_count: 0,
            palette: Vec::new(),
            perlin_wrap: [256; 3],
            wall_unix: None,
            post_gamma: Fx::ZERO,
            rng: seed | 1,
            prng_state: 0xC0FFEE ^ (seed as u32) | 1,
            pixel: [Fx::ZERO; 3],
            pixel_written: false,
            plot_coord: [Fx::ZERO; 3],
            plot_dims: 0,
            plot_written: false,
        }
    }

    pub fn array(&self, id: u32) -> Option<&[Value]> {
        self.arrays.get(id as usize).map(|a| a.as_slice())
    }

    /// Read-only view of the (possibly suspended) call stack.
    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    /// Locals of a frame (for the debugger).
    pub fn frame_locals(&self, f: &Frame, count: usize) -> &[Value] {
        let b = f.locals_base as usize;
        &self.locals[b..(b + count).min(self.locals.len())]
    }

    /// Drop any suspended run (paused debug session being abandoned).
    pub fn clear_run(&mut self) {
        self.frames.clear();
        self.stack.clear();
        self.locals.clear();
    }

    fn err_at(&self, prog: &Program, message: String) -> VmError {
        match self.frames.last() {
            Some(f) => {
                // pc has advanced past the faulting instruction
                let pc = f.pc.saturating_sub(1);
                let (line, col) = prog.fns[f.fn_idx as usize].pos_at(pc);
                VmError {
                    message,
                    fn_idx: f.fn_idx,
                    pc,
                    line,
                    col,
                }
            }
            None => VmError {
                message,
                fn_idx: u16::MAX,
                pc: u32::MAX,
                line: 0,
                col: 0,
            },
        }
    }

    fn push_frame(&mut self, prog: &Program, fn_idx: u16, args: &[Value]) -> Result<(), VmError> {
        if self.frames.len() >= MAX_DEPTH {
            return Err(self.err_at(prog, "call depth exceeded".into()));
        }
        let f = &prog.fns[fn_idx as usize];
        let locals_base = self.locals.len() as u32;
        let params = f.params as usize;
        for i in 0..f.locals as usize {
            self.locals.push(if i < params {
                args.get(i).copied().unwrap_or_default()
            } else {
                Value::default()
            });
        }
        self.frames.push(Frame {
            fn_idx,
            pc: 0,
            locals_base,
            stack_base: self.stack.len() as u32,
        });
        Ok(())
    }

    fn pop_frame(&mut self) {
        if let Some(f) = self.frames.pop() {
            self.locals.truncate(f.locals_base as usize);
            self.stack.truncate(f.stack_base as usize);
        }
    }

    /// Host entry: run a function to completion. Runs on top of whatever is
    /// suspended below (init, control invocations, oracle helpers), so a
    /// paused debug session is never clobbered. Debug pausing does not apply.
    pub fn call(&mut self, prog: &Program, fn_idx: u16, args: &[Value]) -> Result<Value, VmError> {
        self.fuel = FUEL;
        if self.frames.is_empty() {
            self.stack.clear();
            self.locals.clear();
        }
        self.run_on_top(prog, fn_idx, args)
    }

    fn run_on_top(
        &mut self,
        prog: &Program,
        fn_idx: u16,
        args: &[Value],
    ) -> Result<Value, VmError> {
        let base = self.frames.len();
        self.push_frame(prog, fn_idx, args)?;
        let result = self.run(prog, base, false);
        match result {
            Ok(Outcome::Done(v)) => Ok(v),
            Ok(Outcome::Paused) => unreachable!("pausing is disabled for nested runs"),
            Err(e) => {
                while self.frames.len() > base {
                    self.pop_frame();
                }
                Err(e)
            }
        }
    }

    /// Start a debuggable top-level run (the engine's per-frame callbacks).
    pub fn start(
        &mut self,
        prog: &Program,
        fn_idx: u16,
        args: &[Value],
        debug: bool,
    ) -> Result<Outcome, VmError> {
        self.fuel = FUEL;
        self.clear_run();
        self.push_frame(prog, fn_idx, args)?;
        self.run_unwinding(prog, debug)
    }

    /// Resume a paused run, optionally with a stepping plan.
    pub fn resume(&mut self, prog: &Program, step: StepKind) -> Result<Outcome, VmError> {
        self.fuel = FUEL;
        let plan = match (step, self.frames.last()) {
            (StepKind::Continue, _) | (_, None) => None,
            (kind, Some(f)) => Some(StepPlan {
                kind,
                depth: self.frames.len(),
                fn_idx: f.fn_idx,
                line: prog.fns[f.fn_idx as usize].pos_at(f.pc).0,
            }),
        };
        if let Some(d) = self.dbg.as_mut() {
            d.step = plan;
            d.skip_once = true;
        }
        self.run_unwinding(prog, true)
    }

    fn run_unwinding(&mut self, prog: &Program, debug: bool) -> Result<Outcome, VmError> {
        match self.run(prog, 0, debug) {
            Err(e) => {
                self.clear_run();
                Err(e)
            }
            ok => ok,
        }
    }

    /// Should the debugger pause before executing (fi, pc)?
    fn debug_stop(&mut self, prog: &Program, fi: u16, pc: u32) -> bool {
        let depth = self.frames.len();
        let Some(d) = self.dbg.as_mut() else {
            return false;
        };
        if d.pause_requested {
            d.pause_requested = false;
            d.step = None;
            return true;
        }
        if d.skip_once {
            d.skip_once = false;
            return false;
        }
        if d.breakpoints.contains(&(fi, pc)) {
            d.step = None;
            return true;
        }
        if let Some(p) = d.step {
            let line = prog.fns[fi as usize].pos_at(pc).0;
            let stop = match p.kind {
                StepKind::Continue => false,
                StepKind::Over => {
                    depth < p.depth
                        || (depth == p.depth && line != 0 && (line != p.line || fi != p.fn_idx))
                }
                StepKind::Into => {
                    line != 0 && (line != p.line || fi != p.fn_idx || depth != p.depth)
                }
                StepKind::Out => depth < p.depth,
            };
            if stop {
                d.step = None;
                return true;
            }
        }
        false
    }

    fn pop_args(&mut self, argc: usize) -> ([Value; MAX_ARGS], usize) {
        let mut args = [Value::default(); MAX_ARGS];
        let n = argc.min(MAX_ARGS);
        for i in (0..n).rev() {
            args[i] = self.stack.pop().unwrap_or_default();
        }
        (args, n)
    }

    /// The interpreter loop over the explicit frame stack. Returns when the
    /// stack unwinds back to `base` frames (Done) or a debug stop fires
    /// (Paused — only when `debug`). Frames/locals/stack stay intact while
    /// paused so the debugger can inspect and resume.
    fn run(&mut self, prog: &Program, base: usize, debug: bool) -> Result<Outcome, VmError> {
        macro_rules! fail {
            ($msg:expr) => {
                return Err(self.err_at(prog, $msg.into()))
            };
        }
        macro_rules! push {
            ($v:expr) => {{
                if self.stack.len() >= MAX_STACK {
                    fail!("value stack overflow");
                }
                self.stack.push($v);
            }};
        }
        macro_rules! pop {
            () => {
                match self.stack.pop() {
                    Some(v) => v,
                    None => fail!("stack underflow (compiler bug)"),
                }
            };
        }
        macro_rules! binnum {
            ($op:tt) => {{
                let b = pop!().num();
                let a = pop!().num();
                push!(Value::Num(a $op b));
            }};
        }
        macro_rules! bincmp {
            ($op:tt) => {{
                let b = pop!().num();
                let a = pop!().num();
                push!(Value::Num(if a $op b { Fx::ONE } else { Fx::ZERO }));
            }};
        }
        macro_rules! set_pc {
            ($t:expr) => {
                self.frames.last_mut().expect("frame").pc = $t
            };
        }

        loop {
            let (fi, pc, lbase) = {
                let f = self.frames.last().expect("frame");
                (f.fn_idx, f.pc, f.locals_base as usize)
            };
            if debug && self.debug_stop(prog, fi, pc) {
                return Ok(Outcome::Paused);
            }
            let insn = match prog.fns[fi as usize].code.get(pc as usize) {
                Some(&i) => i,
                None => Insn::RetNull, // fell off the end
            };
            set_pc!(pc + 1);
            if self.fuel == 0 {
                fail!("execution limit exceeded (infinite loop?)");
            }
            self.fuel -= 1;
            match insn {
                Insn::Const(v) => push!(v),
                Insn::LoadG(i) => push!(self.globals[i as usize]),
                Insn::StoreG(i) => {
                    let v = pop!();
                    self.globals[i as usize] = v;
                    push!(v);
                }
                Insn::LoadL(i) => push!(self.locals[lbase + i as usize]),
                Insn::StoreL(i) => {
                    let v = pop!();
                    self.locals[lbase + i as usize] = v;
                    push!(v);
                }
                // Index semantics oracle-confirmed on fw 3.67: fractional
                // indices truncate (reads and variable-index writes — stock
                // patterns like sparks depend on it); anything out of range
                // (negative or ≥ length) is a runtime error that aborts
                // execution. Known divergence: PB aborts on a fractional
                // *literal* index write (`a[1.5] = 9`), a compiler-path
                // quirk we deliberately don't copy — we truncate uniformly.
                Insn::LoadIdx => {
                    let idx = pop!().num();
                    let arr = pop!();
                    let Value::Arr(a) = arr else {
                        fail!("indexing a non-array value")
                    };
                    if idx.raw() < 0 {
                        fail!("array index out of bounds");
                    }
                    let i = idx.to_int_trunc() as usize;
                    match self.arrays[a as usize].get(i) {
                        Some(v) => push!(*v),
                        None => fail!("array index out of bounds"),
                    }
                }
                Insn::StoreIdx => {
                    let val = pop!();
                    let idx = pop!().num();
                    let arr = pop!();
                    let Value::Arr(a) = arr else {
                        fail!("indexing a non-array value")
                    };
                    if idx.raw() < 0 {
                        fail!("array index out of bounds");
                    }
                    let i = idx.to_int_trunc() as usize;
                    match self.arrays[a as usize].get_mut(i) {
                        Some(slot) => *slot = val,
                        None => fail!("array index out of bounds"),
                    }
                    push!(val);
                }
                Insn::ArrLen => {
                    let arr = pop!();
                    let Value::Arr(a) = arr else {
                        fail!(".length of a non-array value")
                    };
                    push!(Value::Num(Fx::from_int(
                        self.arrays[a as usize].len() as i32
                    )));
                }
                Insn::NewArray(n) => {
                    let n = n as usize;
                    let mut elems = vec![Value::default(); n];
                    for i in (0..n).rev() {
                        elems[i] = pop!();
                    }
                    match self.alloc_array(elems) {
                        Ok(v) => push!(v),
                        Err(m) => fail!(m),
                    }
                }
                Insn::Dup => {
                    let v = *self.stack.last().unwrap_or(&Value::default());
                    push!(v);
                }
                Insn::Dup2 => {
                    let n = self.stack.len();
                    if n < 2 {
                        fail!("stack underflow (compiler bug)");
                    }
                    let a = self.stack[n - 2];
                    let b = self.stack[n - 1];
                    push!(a);
                    push!(b);
                }
                Insn::Pop => {
                    pop!();
                }
                Insn::Add => binnum!(+),
                Insn::Sub => binnum!(-),
                Insn::Mul => binnum!(*),
                Insn::Div => binnum!(/),
                Insn::Rem => binnum!(%),
                Insn::Pow => {
                    let b = pop!().num();
                    let a = pop!().num();
                    push!(Value::Num(fmath::pow(a, b)));
                }
                Insn::Neg => {
                    let v = pop!().num();
                    push!(Value::Num(-v));
                }
                Insn::Not => {
                    let v = pop!();
                    push!(Value::Num(if v.truthy() { Fx::ZERO } else { Fx::ONE }));
                }
                Insn::BitNot => {
                    let v = pop!().num();
                    push!(Value::Num(!v));
                }
                Insn::BitAnd => binnum!(&),
                Insn::BitOr => binnum!(|),
                Insn::BitXor => binnum!(^),
                Insn::Shl => binnum!(<<),
                Insn::Shr => binnum!(>>),
                Insn::Lt => bincmp!(<),
                Insn::Le => bincmp!(<=),
                Insn::Gt => bincmp!(>),
                Insn::Ge => bincmp!(>=),
                Insn::Eq => {
                    let b = pop!();
                    let a = pop!();
                    push!(Value::Num(if value_eq(a, b) { Fx::ONE } else { Fx::ZERO }));
                }
                Insn::Ne => {
                    let b = pop!();
                    let a = pop!();
                    push!(Value::Num(if value_eq(a, b) { Fx::ZERO } else { Fx::ONE }));
                }
                Insn::Jmp(t) => set_pc!(t),
                Insn::JmpIfFalse(t) => {
                    if !pop!().truthy() {
                        set_pc!(t);
                    }
                }
                Insn::JmpIfTruePeek(t) => {
                    let v = *self.stack.last().unwrap_or(&Value::default());
                    if v.truthy() {
                        set_pc!(t);
                    }
                }
                Insn::JmpIfFalsePeek(t) => {
                    let v = *self.stack.last().unwrap_or(&Value::default());
                    if !v.truthy() {
                        set_pc!(t);
                    }
                }
                Insn::CallFn { fn_idx: f, argc } => {
                    let (args, n) = self.pop_args(argc as usize);
                    self.push_frame(prog, f, &args[..n])?;
                }
                Insn::CallBuiltin { b, argc } => {
                    match self.call_builtin(prog, b, argc as usize) {
                        Ok(v) => push!(v),
                        Err(mut e) => {
                            // attribute to this site if the builtin didn't
                            if e.pc == u32::MAX {
                                e = self.err_at(prog, core::mem::take(&mut e.message));
                            }
                            return Err(e);
                        }
                    }
                }
                Insn::CallValue { argc } => {
                    let n = self.stack.len();
                    let argc = argc as usize;
                    if n < argc + 1 {
                        fail!("stack underflow (compiler bug)");
                    }
                    let callee = self.stack.remove(n - argc - 1);
                    match callee {
                        Value::Fun(f) => {
                            let (args, n) = self.pop_args(argc);
                            self.push_frame(prog, f, &args[..n])?;
                        }
                        Value::Builtin(b) => match self.call_builtin(prog, b, argc) {
                            Ok(v) => push!(v),
                            Err(mut e) => {
                                if e.pc == u32::MAX {
                                    e = self.err_at(prog, core::mem::take(&mut e.message));
                                }
                                return Err(e);
                            }
                        },
                        _ => fail!("call of a non-function value"),
                    }
                }
                Insn::Ret | Insn::RetNull => {
                    let v = if matches!(insn, Insn::Ret) {
                        pop!()
                    } else {
                        Value::default()
                    };
                    self.pop_frame();
                    if self.frames.len() == base {
                        return Ok(Outcome::Done(v));
                    }
                    push!(v);
                }
            }
        }
    }

    pub fn alloc_array(&mut self, elems: Vec<Value>) -> Result<Value, &'static str> {
        if self.array_elems + elems.len() > self.array_budget {
            return Err("array element budget exceeded (arrays are never freed)");
        }
        self.array_elems += elems.len();
        self.arrays.push(elems);
        Ok(Value::Arr((self.arrays.len() - 1) as u32))
    }

    fn next_random(&mut self) -> u32 {
        // splitmix64
        self.rng = self.rng.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        (z ^ (z >> 31)) as u32
    }

    fn next_prng(&mut self) -> u32 {
        // xorshift32. TODO(oracle): PB's prng sequence is unknown.
        let mut x = self.prng_state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.prng_state = x;
        x
    }

    fn scale_random(r: u32, max: Fx) -> Value {
        let m = max.raw().max(0) as u64;
        Value::Num(Fx::from_raw(((r as u64 * m) >> 32) as i32))
    }

    fn call_builtin(&mut self, prog: &Program, id: u16, argc: usize) -> Result<Value, VmError> {
        let no_site = |message: String| VmError {
            message,
            fn_idx: u16::MAX,
            pc: u32::MAX,
            line: 0,
            col: 0,
        };
        let def = &BUILTINS[id as usize];
        let builtin = match def.kind {
            BKind::Impl(b) => b,
            BKind::Todo => {
                // pop args, then report
                for _ in 0..argc {
                    self.stack.pop();
                }
                return Err(no_site(format!(
                    "builtin `{}` is not implemented yet",
                    def.name
                )));
            }
        };
        let mut args = [Value::default(); MAX_ARGS];
        let argc = argc.min(MAX_ARGS);
        for i in (0..argc).rev() {
            args[i] = self.stack.pop().unwrap_or_default();
        }
        let a = |i: usize| args.get(i).copied().unwrap_or_default();
        let n = |i: usize| a(i).num();
        use Builtin::*;
        let num = |v: Fx| Ok(Value::Num(v));
        match builtin {
            Abs => num(n(0).abs()),
            Floor => num(n(0).floor()),
            Ceil => num(n(0).ceil()),
            Round => num(n(0).round()),
            Trunc => num(n(0).trunc()),
            Frac => num(n(0).frac()),
            Clamp => num(n(0).clamp(n(1), n(2))),
            Min => num(n(0).min(n(1))),
            Max => num(n(0).max(n(1))),
            Mod => num(n(0).mod_floor(n(1))),
            Sqrt => num(fmath::sqrt(n(0))),
            Sin => num(fmath::sin(n(0))),
            Cos => num(fmath::cos(n(0))),
            Tan => num(fmath::tan(n(0))),
            Asin => num(fmath::asin(n(0))),
            Acos => num(fmath::acos(n(0))),
            Atan => num(fmath::atan(n(0))),
            Atan2 => num(fmath::atan2(n(0), n(1))),
            Pow => num(fmath::pow(n(0), n(1))),
            Exp => num(fmath::exp(n(0))),
            Log => num(fmath::ln(n(0))),
            Log2 => num(fmath::log2(n(0))),
            Hypot => num(fmath::hypot(n(0), n(1))),
            Hypot3 => num(fmath::hypot3(n(0), n(1), n(2))),
            // map(x, inLo, inHi, outLo, outHi): re-range x; degenerate input
            // range maps to outLo (avoids div-by-zero surprises).
            Map => {
                let (x, ilo, ihi, olo, ohi) = (n(0), n(1), n(2), n(3), n(4));
                let d = ihi - ilo;
                num(if d == Fx::ZERO {
                    olo
                } else {
                    olo + (x - ilo) * (ohi - olo) / d
                })
            }
            Sign => num(if n(0) > Fx::ZERO {
                Fx::ONE
            } else if n(0) < Fx::ZERO {
                -Fx::ONE
            } else {
                Fx::ZERO
            }),
            // step(edge, x): 0 below the edge, 1 at/above it (GLSL order)
            Step => num(if n(1) < n(0) { Fx::ZERO } else { Fx::ONE }),
            Saturate => num(n(0).clamp(Fx::ZERO, Fx::ONE)),
            Dist => num(fmath::hypot(n(2) - n(0), n(3) - n(1))),
            Dist3 => num(fmath::hypot3(n(3) - n(0), n(4) - n(1), n(5) - n(2))),
            // easing on t (typically 0..1); polynomial forms, no clamping
            // (callers control the domain, matching smoothstep's contract)
            EaseInQuad => num(n(0) * n(0)),
            EaseOutQuad => {
                let t = n(0);
                num(t * (Fx::from_int(2) - t))
            }
            EaseInOutQuad => {
                let t = n(0);
                num(if t < Fx::from_raw(1 << 15) {
                    Fx::from_int(2) * t * t
                } else {
                    // -1 + (4 - 2t)·t
                    -Fx::ONE + (Fx::from_int(4) - Fx::from_int(2) * t) * t
                })
            }
            EaseInCubic => {
                let t = n(0);
                num(t * t * t)
            }
            EaseOutCubic => {
                let u = n(0) - Fx::ONE;
                num(u * u * u + Fx::ONE)
            }
            EaseInOutCubic => {
                let t = n(0);
                num(if t < Fx::from_raw(1 << 15) {
                    Fx::from_int(4) * t * t * t
                } else {
                    let u = t + t - Fx::from_int(2);
                    u * u * u / Fx::from_int(2) + Fx::ONE
                })
            }
            Random => {
                let r = self.next_random();
                Ok(Self::scale_random(r, n(0)))
            }
            Prng => {
                let r = self.next_prng();
                Ok(Self::scale_random(r, n(0)))
            }
            PrngSeed => {
                let old = self.prng_state;
                let s = n(0).raw() as u32;
                self.prng_state = if s == 0 { 1 } else { s };
                num(Fx::from_raw(old as i32))
            }
            Time => {
                // period in ms happens to equal the interval's raw value:
                // 65.536 s · interval = 65536 ms · interval.
                let period = n(0).raw().max(0) as u64;
                if period == 0 {
                    return num(Fx::ZERO);
                }
                let t = self.time_ms % period;
                num(Fx::from_raw(((t << 16) / period) as i32))
            }
            Wave => num(Fx::from_raw(
                (fmath::sin_turns(n(0)).raw() + Fx::ONE.raw()) >> 1,
            )),
            Square => {
                let duty = if argc >= 2 { n(1) } else { Fx::from_f64(0.5) };
                let t = n(0).mod_floor(Fx::ONE);
                num(if t < duty { Fx::ONE } else { Fx::ZERO })
            }
            Triangle => {
                let t = n(0).mod_floor(Fx::ONE);
                let half = Fx::from_raw(1 << 15);
                num(if t < half {
                    t + t
                } else {
                    (Fx::ONE - t) + (Fx::ONE - t)
                })
            }
            Mix => num(n(0) + (n(1) - n(0)) * n(2)),
            Smoothstep => {
                let (lo, hi, v) = (n(0), n(1), n(2));
                let d = hi - lo;
                let t = if d == Fx::ZERO {
                    Fx::ZERO
                } else {
                    ((v - lo) / d).clamp(Fx::ZERO, Fx::ONE)
                };
                num(t * t * (Fx::from_int(3) - (t + t)))
            }
            BezierQuadratic => {
                let (t, p0, p1, p2) = (n(0), n(1), n(2), n(3));
                let u = Fx::ONE - t;
                num(u * u * p0 + Fx::from_int(2) * u * t * p1 + t * t * p2)
            }
            BezierCubic => {
                let (t, p0, p1, p2, p3) = (n(0), n(1), n(2), n(3), n(4));
                let u = Fx::ONE - t;
                num(u * u * u * p0
                    + Fx::from_int(3) * u * u * t * p1
                    + Fx::from_int(3) * u * t * t * p2
                    + t * t * t * p3)
            }
            Hsv => {
                self.pixel = hsv_to_rgb(n(0), n(1), n(2));
                self.pixel_written = true;
                Ok(Value::default())
            }
            Rgb => {
                self.pixel = [
                    n(0).clamp(Fx::ZERO, Fx::ONE),
                    n(1).clamp(Fx::ZERO, Fx::ONE),
                    n(2).clamp(Fx::ZERO, Fx::ONE),
                ];
                self.pixel_written = true;
                Ok(Value::default())
            }
            // plot(x, y) or plot(x, y, z): map programs emit one coordinate
            // per pixel; the engine (map mode) reads plot_coord after the call.
            Plot => {
                self.plot_coord = [n(0), n(1), if argc >= 3 { n(2) } else { Fx::ZERO }];
                self.plot_dims = if argc >= 3 { 3 } else { 2 };
                self.plot_written = true;
                Ok(Value::default())
            }
            Oklch => {
                self.pixel = crate::color::oklch_to_rgb(n(0), n(1), n(2));
                self.pixel_written = true;
                Ok(Value::default())
            }
            Oklab => {
                self.pixel = crate::color::oklab_to_rgb(n(0), n(1), n(2));
                self.pixel_written = true;
                Ok(Value::default())
            }
            Array => {
                let len = n(0).to_int_trunc().max(0) as usize;
                self.alloc_array(vec![Value::default(); len])
                    .map_err(|m| no_site(m.into()))
            }
            ArrayLength => {
                let Value::Arr(arr) = a(0) else {
                    return Err(no_site("arrayLength of a non-array".into()));
                };
                num(Fx::from_int(self.arrays[arr as usize].len() as i32))
            }
            ArraySum => {
                let Value::Arr(arr) = a(0) else {
                    return Err(no_site("arraySum of a non-array".into()));
                };
                let mut sum = Fx::ZERO;
                for v in &self.arrays[arr as usize] {
                    sum = sum + v.num();
                }
                num(sum)
            }
            ArrayForEach | ArrayMutate => {
                let Value::Arr(arr) = a(0) else {
                    return Err(no_site("array method on a non-array".into()));
                };
                let f = a(1);
                let mutate = builtin == ArrayMutate;
                let mut i = 0usize;
                while i < self.arrays[arr as usize].len() {
                    let v = self.arrays[arr as usize][i];
                    let r = self.dispatch_direct(
                        prog,
                        f,
                        &[v, Value::Num(Fx::from_int(i as i32)), a(0)],
                    )?;
                    if mutate {
                        if let Some(slot) = self.arrays[arr as usize].get_mut(i) {
                            *slot = r;
                        }
                    }
                    i += 1;
                }
                Ok(a(0))
            }
            ArrayMapTo => {
                let (Value::Arr(src), Value::Arr(dst)) = (a(0), a(1)) else {
                    return Err(no_site("arrayMapTo needs two arrays".into()));
                };
                let f = a(2);
                let mut i = 0usize;
                while i < self.arrays[src as usize].len() && i < self.arrays[dst as usize].len() {
                    let v = self.arrays[src as usize][i];
                    let r = self.dispatch_direct(
                        prog,
                        f,
                        &[v, Value::Num(Fx::from_int(i as i32)), a(0)],
                    )?;
                    self.arrays[dst as usize][i] = r;
                    i += 1;
                }
                Ok(a(1))
            }
            ArrayReduce => {
                let Value::Arr(arr) = a(0) else {
                    return Err(no_site("arrayReduce of a non-array".into()));
                };
                let f = a(1);
                let mut acc = a(2);
                let mut i = 0usize;
                while i < self.arrays[arr as usize].len() {
                    let v = self.arrays[arr as usize][i];
                    acc = self.dispatch_direct(
                        prog,
                        f,
                        &[acc, v, Value::Num(Fx::from_int(i as i32)), a(0)],
                    )?;
                    i += 1;
                }
                Ok(acc)
            }
            ArrayReplace | ArrayReplaceAt => {
                let Value::Arr(arr) = a(0) else {
                    return Err(no_site("arrayReplace of a non-array".into()));
                };
                let (off, first) = if builtin == ArrayReplaceAt {
                    (n(1).to_int_trunc().max(0) as usize, 2)
                } else {
                    (0, 1)
                };
                for (j, arg) in args[first..argc].iter().enumerate() {
                    if let Some(slot) = self.arrays[arr as usize].get_mut(off + j) {
                        *slot = *arg;
                    }
                }
                Ok(a(0))
            }
            ArraySort | ArraySortBy => {
                let Value::Arr(arr) = a(0) else {
                    return Err(no_site("arraySort of a non-array".into()));
                };
                let cmp = a(1);
                let by = builtin == ArraySortBy;
                let mut data = core::mem::take(&mut self.arrays[arr as usize]);
                let mut err = None;
                // insertion sort (documented as not stable; small arrays)
                'outer: for i in 1..data.len() {
                    let key = data[i];
                    let mut j = i;
                    while j > 0 {
                        let before = if by {
                            match self.dispatch_direct(prog, cmp, &[data[j - 1], key]) {
                                Ok(r) => r.num() > Fx::ZERO,
                                Err(e) => {
                                    err = Some(e);
                                    break 'outer;
                                }
                            }
                        } else {
                            data[j - 1].num() > key.num()
                        };
                        if !before {
                            break;
                        }
                        data[j] = data[j - 1];
                        j -= 1;
                    }
                    data[j] = key;
                }
                self.arrays[arr as usize] = data;
                match err {
                    Some(e) => Err(e),
                    None => Ok(a(0)),
                }
            }
            // ---- coordinate transforms (see field docs for conventions) ----
            ResetTransform => {
                self.transform = IDENTITY;
                self.transform_active = false;
                self.transform_ops = 0;
                Ok(Value::default())
            }
            Transform => {
                let mut m = IDENTITY;
                for (r, row) in m.iter_mut().enumerate() {
                    for (c, cell) in row.iter_mut().enumerate() {
                        *cell = n(r * 4 + c);
                    }
                }
                self.push_op(m).map_err(|e| no_site(e.into()))?;
                Ok(Value::default())
            }
            Translate => {
                let mut m = IDENTITY;
                m[0][3] = n(0);
                m[1][3] = n(1);
                self.push_op(m).map_err(|e| no_site(e.into()))?;
                Ok(Value::default())
            }
            Translate3D => {
                let mut m = IDENTITY;
                m[0][3] = n(0);
                m[1][3] = n(1);
                m[2][3] = n(2);
                self.push_op(m).map_err(|e| no_site(e.into()))?;
                Ok(Value::default())
            }
            Scale => {
                let mut m = IDENTITY;
                m[0][0] = n(0);
                m[1][1] = n(1);
                self.push_op(m).map_err(|e| no_site(e.into()))?;
                Ok(Value::default())
            }
            Scale3D => {
                let mut m = IDENTITY;
                m[0][0] = n(0);
                m[1][1] = n(1);
                m[2][2] = n(2);
                self.push_op(m).map_err(|e| no_site(e.into()))?;
                Ok(Value::default())
            }
            Rotate | RotateZ => {
                self.push_op(rotation(2, n(0)))
                    .map_err(|e| no_site(e.into()))?;
                Ok(Value::default())
            }
            RotateX => {
                self.push_op(rotation(0, n(0)))
                    .map_err(|e| no_site(e.into()))?;
                Ok(Value::default())
            }
            RotateY => {
                self.push_op(rotation(1, n(0)))
                    .map_err(|e| no_site(e.into()))?;
                Ok(Value::default())
            }
            // ---- map introspection ----
            PixelMapDimensions => num(Fx::from_int(
                self.map.as_ref().map(|m| m.dims as i32).unwrap_or(0),
            )),
            Has2DMap => num(if self.map.as_ref().map(|m| m.dims) == Some(2) {
                Fx::ONE
            } else {
                Fx::ZERO
            }),
            Has3DMap => num(if self.map.as_ref().map(|m| m.dims) == Some(3) {
                Fx::ONE
            } else {
                Fx::ZERO
            }),
            MapPixels => {
                let f = a(0);
                for i in 0..self.pixel_count {
                    let p = self.pixel_coords(i, [Fx::ZERO; 3]);
                    let p = self.apply_transform(p);
                    self.dispatch_direct(
                        prog,
                        f,
                        &[
                            Value::Num(Fx::from_int(i as i32)),
                            Value::Num(p[0]),
                            Value::Num(p[1]),
                            Value::Num(p[2]),
                        ],
                    )?;
                }
                Ok(Value::default())
            }
            // ---- noise ----
            Perlin => num(crate::noise::perlin(
                n(0),
                n(1),
                n(2),
                n(3),
                self.perlin_wrap,
            )),
            PerlinFbm => num(crate::noise::fbm(
                n(0),
                n(1),
                n(2),
                n(3),
                n(4),
                n(5),
                self.perlin_wrap,
            )),
            PerlinRidge => num(crate::noise::ridge(
                n(0),
                n(1),
                n(2),
                n(3),
                n(4),
                n(5),
                n(6),
                self.perlin_wrap,
            )),
            PerlinTurbulence => num(crate::noise::turbulence(
                n(0),
                n(1),
                n(2),
                n(3),
                n(4),
                n(5),
                self.perlin_wrap,
            )),
            SetPerlinWrap => {
                for (i, w) in self.perlin_wrap.iter_mut().enumerate() {
                    *w = n(i).to_int_trunc().clamp(2, 256);
                }
                Ok(Value::default())
            }
            // ---- palettes ----
            SetPalette => {
                let Value::Arr(arr) = a(0) else {
                    return Err(no_site("setPalette needs an array".into()));
                };
                let data = &self.arrays[arr as usize];
                let mut pal = Vec::new();
                let mut i = 0;
                while i + 3 < data.len() {
                    pal.push((
                        data[i].num(),
                        [data[i + 1].num(), data[i + 2].num(), data[i + 3].num()],
                    ));
                    i += 4;
                }
                self.palette = pal;
                Ok(Value::default())
            }
            Paint => {
                let v = n(0).mod_floor(Fx::ONE);
                let b = if argc >= 2 { n(1) } else { Fx::ONE };
                let rgb = self.palette_lookup(v);
                let b = b.clamp(Fx::ZERO, Fx::ONE);
                self.pixel = [rgb[0] * b, rgb[1] * b, rgb[2] * b];
                self.pixel_written = true;
                Ok(Value::default())
            }
            // ---- clock (host-provided wall time) ----
            ClockYear | ClockMonth | ClockDay | ClockHour | ClockMinute | ClockSecond
            | ClockWeekday => {
                let Some(unix) = self.wall_unix else {
                    return num(Fx::ZERO); // TODO(oracle): PB with no time source
                };
                let c = civil_from_unix(unix);
                num(Fx::from_int(match builtin {
                    ClockYear => c.year,
                    ClockMonth => c.month,
                    ClockDay => c.day,
                    ClockHour => c.hour,
                    ClockMinute => c.minute,
                    ClockSecond => c.second,
                    _ => c.weekday_sun1,
                }))
            }
            // ---- GPIO / sequencer / sync stubs: silent no-ops until real
            // peripherals exist (M4/M5); inputs read 0 ----
            PinMode | DigitalWrite | PlaylistSetPosition | SequencerNext => Ok(Value::default()),
            DigitalRead | AnalogRead | TouchRead | SequencerGetMode | PlaylistGetPosition
            | PlaylistGetLength | NodeId => num(Fx::ZERO),
            // ---- Luxel extensions, batch 2 ----
            // beat(bpm): sawtooth beat phase 0..1 at `bpm` on the engine
            // clock — FastLED-style tempo without real audio
            Beat => num(self.beat_phase(n(0))),
            // beatSin(bpm, lo = 0, hi = 1): sine oscillation lo..hi at bpm
            BeatSin => {
                let lo = if argc >= 2 { n(1) } else { Fx::ZERO };
                let hi = if argc >= 3 { n(2) } else { Fx::ONE };
                let s = fmath::sin_turns(self.beat_phase(n(0)));
                let unit = Fx::from_raw((s.raw() + Fx::ONE.raw()) >> 1);
                num(lo + (hi - lo) * unit)
            }
            // hash(x) / hash2(x, y): deterministic 0..1 from the raw bits —
            // stable per-pixel randomness (sparkle that doesn't reshuffle
            // every frame). Same input, same output, on every device.
            Hash => num(hash_unit(n(0).raw() as u32)),
            Hash2 => num(hash_unit(
                (n(0).raw() as u32).wrapping_add(hash32(n(1).raw() as u32)),
            )),
            // blur1D(arr, radius): in-place box blur, window 2·radius+1,
            // edges clamped; returns the array. radius < 1 is a no-op.
            Blur1D => {
                let Value::Arr(arr) = a(0) else {
                    return Err(no_site("blur1D of a non-array".into()));
                };
                let r = n(1).to_int_trunc().max(0) as usize;
                let idx = arr as usize;
                let len = self.arrays[idx].len();
                if r > 0 && len > 0 {
                    // prefix sums in raw i64 — exact, no overflow at 10K els
                    let mut pre = alloc::vec::Vec::with_capacity(len + 1);
                    pre.push(0i64);
                    for v in &self.arrays[idx] {
                        pre.push(pre.last().unwrap() + v.num().raw() as i64);
                    }
                    for i in 0..len {
                        let lo = i.saturating_sub(r);
                        let hi = (i + r).min(len - 1);
                        let avg = (pre[hi + 1] - pre[lo]) / (hi - lo + 1) as i64;
                        self.arrays[idx][i] = Value::Num(Fx::from_raw(avg as i32));
                    }
                }
                Ok(a(0))
            }
            // feedback(arr, decay): arr[i] *= decay in place; the trails/
            // glow decay loop as one call. Returns the array.
            Feedback => {
                let Value::Arr(arr) = a(0) else {
                    return Err(no_site("feedback of a non-array".into()));
                };
                let decay = n(1);
                for slot in self.arrays[arr as usize].iter_mut() {
                    *slot = Value::Num(slot.num() * decay);
                }
                Ok(a(0))
            }
            // dot(x1,y1, x2,y2) / dot3(x1,y1,z1, x2,y2,z2)
            Dot => num(n(0) * n(2) + n(1) * n(3)),
            Dot3 => num(n(0) * n(3) + n(1) * n(4) + n(2) * n(5)),
            // angleBetween(x1,y1, x2,y2): signed angle from v1 to v2 in
            // radians (positive = counter-clockwise), like atan2
            AngleBetween => {
                let cross = n(0) * n(3) - n(1) * n(2);
                let dotp = n(0) * n(2) + n(1) * n(3);
                num(fmath::atan2(cross, dotp))
            }
            // Value-returning color conversions: write [x,y,z] into the
            // caller's `out` array (first 3 slots) and return it — callers
            // reuse one array, so render loops don't grow the arena.
            Hsv2Rgb => {
                let rgb = hsv_to_rgb(n(0), n(1), n(2));
                self.write3(a(3), rgb).map_err(|m| no_site(m.into()))?;
                Ok(a(3))
            }
            Rgb2Hsv => {
                let hsv = rgb_to_hsv(n(0), n(1), n(2));
                self.write3(a(3), hsv).map_err(|m| no_site(m.into()))?;
                Ok(a(3))
            }
            // simplex2(x, y, seed = 0) / simplex3(x, y, z, seed = 0):
            // simplex noise in ~[-1, 1] — smoother than perlin, no axis
            // artifacts. The lattice does not wrap (setPerlinWrap N/A).
            Simplex2 => num(crate::noise::simplex2(n(0), n(1), n(2))),
            Simplex3 => num(crate::noise::simplex3(n(0), n(1), n(2), n(3))),
            // setGamma(g): output gamma applied after render (2.0–2.8 makes
            // LED fades perceptually even). g <= 0 or g == 1 turns it off.
            SetGamma => {
                self.post_gamma = n(0).max(Fx::ZERO);
                Ok(Value::default())
            }
            // mixColors(r1,g1,b1, r2,g2,b2, t, out): blend two RGB colors in
            // OKLab — perceptually even, no muddy midpoints
            MixColors => {
                let c = crate::color::mix_oklab([n(0), n(1), n(2)], [n(3), n(4), n(5)], n(6));
                self.write3(a(7), c).map_err(|m| no_site(m.into()))?;
                Ok(a(7))
            }
        }
    }

    /// Fractional beat position at `bpm` on the engine clock (0..1 sawtooth).
    fn beat_phase(&self, bpm: Fx) -> Fx {
        // beats = ms·bpm/60000; with bpm in 16.16 the low 16 bits of the
        // quotient are exactly the fractional beat
        let phase = (self.time_ms as u128 * bpm.raw().max(0) as u128 / 60_000) & 0xFFFF;
        Fx::from_raw(phase as i32)
    }

    /// Write three numbers into the first three slots of `out`.
    fn write3(&mut self, out: Value, vals: [Fx; 3]) -> Result<(), &'static str> {
        let Value::Arr(arr) = out else {
            return Err("`out` must be an array");
        };
        let slots = &mut self.arrays[arr as usize];
        if slots.len() < 3 {
            return Err("`out` array needs length >= 3");
        }
        for (slot, v) in slots.iter_mut().zip(vals) {
            *slot = Value::Num(v);
        }
        Ok(())
    }

    /// Pre-multiply an op onto the current transform: points transform in
    /// call order (`translate(-.5,-.5); rotate(θ)` rotates about the center,
    /// per the universal corpus idiom). Capped at 31 stacked ops like PB.
    /// TODO(oracle): verify order/sign/cap by probe when the device is back.
    fn push_op(&mut self, op: [[Fx; 4]; 4]) -> Result<(), &'static str> {
        if self.transform_ops >= 31 {
            return Err("too many stacked transforms (max 31)");
        }
        self.transform_ops += 1;
        self.transform_active = true;
        self.transform = mat_mul(&op, &self.transform);
        Ok(())
    }

    /// Coordinates for pixel `i`: the installed map, else the 1D fallback
    /// (x = i/pixelCount, remaining axes from `fill`).
    pub fn pixel_coords(&self, i: u32, fill: [Fx; 3]) -> [Fx; 3] {
        match &self.map {
            Some(m) => {
                let c = m.coords.get(i as usize).copied().unwrap_or([Fx::ZERO; 3]);
                match m.dims {
                    1 => [c[0], fill[1], fill[2]],
                    2 => [c[0], c[1], fill[2]],
                    _ => c,
                }
            }
            None => {
                let x =
                    Fx::from_raw((((i as i64) << 16) / (self.pixel_count.max(1) as i64)) as i32);
                [x, fill[1], fill[2]]
            }
        }
    }

    /// Apply the current transform to a point (affine 4×4, w ignored).
    pub fn apply_transform(&self, p: [Fx; 3]) -> [Fx; 3] {
        if !self.transform_active {
            return p;
        }
        let m = &self.transform;
        let mut out = [Fx::ZERO; 3];
        for (r, o) in out.iter_mut().enumerate() {
            *o = m[r][0] * p[0] + m[r][1] * p[1] + m[r][2] * p[2] + m[r][3];
        }
        out
    }

    fn palette_lookup(&self, v: Fx) -> [Fx; 3] {
        // no palette installed → grayscale ramp. TODO(oracle).
        if self.palette.is_empty() {
            return [v, v, v];
        }
        let first = self.palette[0];
        let last = self.palette[self.palette.len() - 1];
        if v <= first.0 {
            return first.1;
        }
        if v >= last.0 {
            return last.1; // clamp at the ends. TODO(oracle): wrap?
        }
        for w in self.palette.windows(2) {
            let (p0, c0) = w[0];
            let (p1, c1) = w[1];
            if v <= p1 {
                let span = p1 - p0;
                let t = if span == Fx::ZERO {
                    Fx::ZERO
                } else {
                    (v - p0) / span
                };
                let mut out = [Fx::ZERO; 3];
                for (i, o) in out.iter_mut().enumerate() {
                    *o = c0[i] + (c1[i] - c0[i]) * t;
                }
                return out;
            }
        }
        last.1
    }

    /// Call a function value with explicit args (used by array HOFs and
    /// mapPixels). Runs to completion — debug pausing never fires inside a
    /// builtin callback (documented v1 limitation).
    fn dispatch_direct(
        &mut self,
        prog: &Program,
        callee: Value,
        args: &[Value],
    ) -> Result<Value, VmError> {
        match callee {
            Value::Fun(f) => self.run_on_top(prog, f, args),
            Value::Builtin(b) => {
                for v in args {
                    self.stack.push(*v);
                }
                self.call_builtin(prog, b, args.len())
            }
            _ => Err(VmError {
                message: "callback is not a function".into(),
                fn_idx: u16::MAX,
                pc: u32::MAX,
                line: 0,
                col: 0,
            }),
        }
    }
}

fn value_eq(a: Value, b: Value) -> bool {
    match (a, b) {
        (Value::Num(x), Value::Num(y)) => x == y,
        (Value::Arr(x), Value::Arr(y)) => x == y,
        (Value::Fun(x), Value::Fun(y)) => x == y,
        (Value::Builtin(x), Value::Builtin(y)) => x == y,
        _ => false,
    }
}

fn mat_mul(a: &[[Fx; 4]; 4], b: &[[Fx; 4]; 4]) -> [[Fx; 4]; 4] {
    let mut out = [[Fx::ZERO; 4]; 4];
    for r in 0..4 {
        for c in 0..4 {
            let mut acc = Fx::ZERO;
            for (k, bk) in b.iter().enumerate() {
                acc = acc + a[r][k] * bk[c];
            }
            out[r][c] = acc;
        }
    }
    out
}

/// Rotation about axis 0=X, 1=Y, 2=Z, counterclockwise for +angle (radians).
/// TODO(oracle): direction convention unverified.
fn rotation(axis: usize, angle: Fx) -> [[Fx; 4]; 4] {
    let c = fmath::cos(angle);
    let s = fmath::sin(angle);
    let mut m = IDENTITY;
    match axis {
        0 => {
            m[1][1] = c;
            m[1][2] = -s;
            m[2][1] = s;
            m[2][2] = c;
        }
        1 => {
            m[0][0] = c;
            m[0][2] = s;
            m[2][0] = -s;
            m[2][2] = c;
        }
        _ => {
            m[0][0] = c;
            m[0][1] = -s;
            m[1][0] = s;
            m[1][1] = c;
        }
    }
    m
}

struct Civil {
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    minute: i32,
    second: i32,
    /// Sunday = 1 … Saturday = 7 (PB convention).
    weekday_sun1: i32,
}

/// Unix seconds → civil date/time (Howard Hinnant's algorithm, integer-only).
/// The host pre-applies any timezone offset.
fn civil_from_unix(secs: i64) -> Civil {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as i32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as i32;
    let year = (if month <= 2 { y + 1 } else { y }) as i32;
    Civil {
        year,
        month,
        day,
        hour: (rem / 3600) as i32,
        minute: (rem % 3600 / 60) as i32,
        second: (rem % 60) as i32,
        weekday_sun1: ((days + 4).rem_euclid(7) + 1) as i32,
    }
}

/// HSV → RGB in pure fixed point. Hue wraps (negative wraps backward),
/// saturation/value clamp to 0..1. TODO(oracle): compare rounding against PB.
/// lowbias32 (Chris Wellons) — well-mixed 32-bit integer hash, the basis of
/// the deterministic `hash`/`hash2` builtins. Pinned: changing this changes
/// pattern output on every device, so treat it as part of the bytecode ABI.
fn hash32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x21f0_aaad);
    x ^= x >> 15;
    x = x.wrapping_mul(0xd35a_2d97);
    x ^= x >> 15;
    x
}

/// Hash to a uniform value in [0, 1).
fn hash_unit(x: u32) -> Fx {
    Fx::from_raw((hash32(x) & 0xFFFF) as i32)
}

/// Inverse of [hsv_to_rgb]: gamma-sRGB → [h, s, v], hue in turns (0..1).
pub fn rgb_to_hsv(r: Fx, g: Fx, b: Fx) -> [Fx; 3] {
    let r = r.clamp(Fx::ZERO, Fx::ONE);
    let g = g.clamp(Fx::ZERO, Fx::ONE);
    let b = b.clamp(Fx::ZERO, Fx::ONE);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let v = max;
    let s = if max == Fx::ZERO { Fx::ZERO } else { d / max };
    let six = Fx::from_int(6);
    let h6 = if d == Fx::ZERO {
        Fx::ZERO
    } else if max == r {
        ((g - b) / d).mod_floor(six)
    } else if max == g {
        (b - r) / d + Fx::from_int(2)
    } else {
        (r - g) / d + Fx::from_int(4)
    };
    [h6 / six, s, v]
}

pub fn hsv_to_rgb(h: Fx, s: Fx, v: Fx) -> [Fx; 3] {
    let s = s.clamp(Fx::ZERO, Fx::ONE);
    let v = v.clamp(Fx::ZERO, Fx::ONE);
    let h6 = h.mod_floor(Fx::ONE).raw() as i64 * 6; // [0, 6) in 16-frac
    let sector = (h6 >> 16) as i32; // 0..5
    let f = Fx::from_raw((h6 & 0xFFFF) as i32);
    let p = v * (Fx::ONE - s);
    let q = v * (Fx::ONE - s * f);
    let t = v * (Fx::ONE - s * (Fx::ONE - f));
    match sector {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}
