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

#[derive(Debug, Clone, PartialEq)]
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
}

#[derive(Debug, Clone)]
pub struct GlobalDef {
    pub name: String,
    pub export: bool,
    pub init: Fx,
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
    // Documented, not yet implemented (M3+: transforms/maps/palettes/perlin;
    // M4: clock/sequencer/sync; M5: GPIO). Compile fine, error at runtime.
    b!("perlin"), b!("perlinFbm"), b!("perlinRidge"), b!("perlinTurbulence"),
    b!("setPerlinWrap"), b!("resetTransform"), b!("transform"), b!("translate"),
    b!("scale"), b!("rotate"), b!("translate3D"), b!("scale3D"), b!("rotateX"),
    b!("rotateY"), b!("rotateZ"), b!("pixelMapDimensions"), b!("has2DMap"),
    b!("has3DMap"), b!("mapPixels"), b!("setPalette"), b!("paint"),
    b!("pinMode"), b!("digitalWrite"), b!("digitalRead"), b!("analogRead"),
    b!("touchRead"), b!("clockYear"), b!("clockMonth"), b!("clockDay"),
    b!("clockHour"), b!("clockMinute"), b!("clockSecond"), b!("clockWeekday"),
    b!("sequencerNext"), b!("sequencerGetMode"), b!("playlistGetPosition"),
    b!("playlistSetPosition"), b!("playlistGetLength"), b!("nodeId"),
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
}

const MAX_DEPTH: u32 = 48;
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
    depth: u32,
    fuel: u32,
    /// Milliseconds since pattern start; the engine advances this.
    pub time_ms: u64,
    rng: u64,
    prng_state: u32,
    /// Set by hsv()/rgb() — the engine reads this after each render call.
    pub pixel: [Fx; 3],
    pub pixel_written: bool,
}

impl Vm {
    pub fn new(prog: &Program, seed: u64) -> Vm {
        Vm {
            globals: prog.globals.iter().map(|g| Value::Num(g.init)).collect(),
            arrays: Vec::new(),
            array_elems: 0,
            array_budget: DEFAULT_ARRAY_BUDGET,
            stack: Vec::new(),
            locals: Vec::new(),
            depth: 0,
            fuel: FUEL,
            time_ms: 0,
            rng: seed | 1,
            prng_state: 0xC0FFEE ^ (seed as u32) | 1,
            pixel: [Fx::ZERO; 3],
            pixel_written: false,
        }
    }

    pub fn array(&self, id: u32) -> Option<&[Value]> {
        self.arrays.get(id as usize).map(|a| a.as_slice())
    }

    /// Call a function by index (host entry point — refuels and clears the
    /// stack, so a prior aborted call can't poison this one).
    pub fn call(&mut self, prog: &Program, fn_idx: u16, args: &[Value]) -> Result<Value, VmError> {
        self.fuel = FUEL;
        self.stack.clear();
        self.locals.clear();
        self.depth = 0;
        self.call_fn(prog, fn_idx, args)
    }

    fn call_fn(&mut self, prog: &Program, fn_idx: u16, args: &[Value]) -> Result<Value, VmError> {
        let err = |m: &str, pc: usize| VmError {
            message: m.into(),
            fn_idx,
            pc: pc as u32,
        };
        if self.depth >= MAX_DEPTH {
            return Err(err("call depth exceeded", 0));
        }
        self.depth += 1;
        let f = &prog.fns[fn_idx as usize];
        let base = self.locals.len();
        let params = f.params as usize;
        for i in 0..f.locals as usize {
            let v = if i < params {
                args.get(i).copied().unwrap_or_default()
            } else {
                Value::default()
            };
            self.locals.push(v);
        }
        let result = self.exec(prog, fn_idx, base);
        self.locals.truncate(base);
        self.depth -= 1;
        result
    }

    fn exec(&mut self, prog: &Program, fn_idx: u16, lbase: usize) -> Result<Value, VmError> {
        let code = &prog.fns[fn_idx as usize].code;
        let mut pc = 0usize;

        macro_rules! fail {
            ($msg:expr) => {
                return Err(VmError {
                    message: $msg.into(),
                    fn_idx,
                    pc: pc as u32,
                })
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

        loop {
            if self.fuel == 0 {
                fail!("execution limit exceeded (infinite loop?)");
            }
            self.fuel -= 1;
            let Some(insn) = code.get(pc) else {
                return Ok(Value::default()); // fell off the end
            };
            match *insn {
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
                // Index semantics oracle-confirmed on fw 3.67: reads truncate
                // a fractional index but any out-of-range access (negative,
                // ≥ length, and for WRITES even an in-bounds fractional
                // index) is a runtime error that aborts execution.
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
                    if idx.raw() < 0 || idx.frac() != Fx::ZERO {
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
                Insn::Jmp(t) => {
                    pc = t as usize;
                    continue;
                }
                Insn::JmpIfFalse(t) => {
                    if !pop!().truthy() {
                        pc = t as usize;
                        continue;
                    }
                }
                Insn::JmpIfTruePeek(t) => {
                    let v = *self.stack.last().unwrap_or(&Value::default());
                    if v.truthy() {
                        pc = t as usize;
                        continue;
                    }
                }
                Insn::JmpIfFalsePeek(t) => {
                    let v = *self.stack.last().unwrap_or(&Value::default());
                    if !v.truthy() {
                        pc = t as usize;
                        continue;
                    }
                }
                Insn::CallFn { fn_idx: f, argc } => {
                    let v = self.dispatch_call(prog, Value::Fun(f), argc as usize)?;
                    push!(v);
                }
                Insn::CallBuiltin { b, argc } => {
                    match self.call_builtin(prog, b, argc as usize) {
                        Ok(v) => push!(v),
                        Err(mut e) => {
                            // attribute to this site if the builtin didn't
                            if e.pc == u32::MAX {
                                e.fn_idx = fn_idx;
                                e.pc = pc as u32;
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
                    let v = self.dispatch_call(prog, callee, argc)?;
                    push!(v);
                }
                Insn::Ret => return Ok(pop!()),
                Insn::RetNull => return Ok(Value::default()),
            }
            pc += 1;
        }
    }

    /// Pop `argc` args and invoke a function/builtin value.
    fn dispatch_call(
        &mut self,
        prog: &Program,
        callee: Value,
        argc: usize,
    ) -> Result<Value, VmError> {
        let mut args = [Value::default(); MAX_ARGS];
        let argc = argc.min(MAX_ARGS);
        for i in (0..argc).rev() {
            args[i] = self.stack.pop().unwrap_or_default();
        }
        match callee {
            Value::Fun(f) => self.call_fn(prog, f, &args[..argc]),
            Value::Builtin(b) => {
                for a in &args[..argc] {
                    self.stack.push(*a);
                }
                self.call_builtin(prog, b, argc)
            }
            _ => Err(VmError {
                message: "call of a non-function value".into(),
                fn_idx: u16::MAX,
                pc: u32::MAX,
            }),
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
        }
    }

    /// Call a function value with explicit args (used by array HOFs).
    fn dispatch_direct(
        &mut self,
        prog: &Program,
        callee: Value,
        args: &[Value],
    ) -> Result<Value, VmError> {
        match callee {
            Value::Fun(f) => self.call_fn(prog, f, args),
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

/// HSV → RGB in pure fixed point. Hue wraps (negative wraps backward),
/// saturation/value clamp to 0..1. TODO(oracle): compare rounding against PB.
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
