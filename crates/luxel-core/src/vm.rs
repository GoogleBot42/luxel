//! Bytecode program representation and the stack VM.
//!
//! Design notes:
//! - One scalar domain (`Fx`) plus reference values (arrays, functions,
//!   builtins), matching the PB model. Arithmetic on a reference treats it
//!   as 0 (oracle-verified 2026-07-07: refs act as 0 in math on PB too).
//! - Arrays live in an arena and are never freed — re-binding a variable
//!   orphans the old array permanently, exactly like PB. Total element
//!   budget defaults to PB's 10,236 units (len+4 per array).
//! - Runtime errors never panic or halt the engine: the first error per
//!   call is recorded (message + function + pc, `vmerr` style) and the
//!   callback aborts; the frame pipeline keeps running.
//! - Fuel and call-depth guards keep hostile/buggy patterns from hanging a
//!   host.
//!
//! The VM executes LXBC bytecode IN PLACE: `Program.code` is the flat byte
//! encoding (docs/spec/bytecode.md), `pc` is a function-relative byte
//! offset, and jump operands are byte offsets too. Nothing is materialized
//! per instruction — a decoded Program costs roughly its blob size, which
//! is what lets 50–80 KB-of-heap devices run real patterns (like PB, whose
//! device VM also runs its bytecode directly). Every host — firmware,
//! wasm, native — runs THIS interpreter, so semantics can't drift between
//! the browser preview and the strip. The decoder (`bytecode::decode`)
//! establishes every invariant the loop trusts: operand indices in range,
//! jump targets on instruction boundaries, argc capped.

use alloc::collections::VecDeque;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::arena::ArrVec;
use crate::fixed::Fx;
use crate::fmath;

// ---- program ----

/// A VM value. Eight bytes either way, but the payload widths decide the
/// DISCRIMINANT's width, and that is load-bearing on Xtensa (Gitea #312):
/// with `u16` payloads rustc lays the tag out as a `u16`, so every `match`
/// on a `Value` — and every `Option<Value>` niche test that `Vec::pop`
/// leaves behind — compiles to `l32i; l32r 0xffff; and; b*i` instead of
/// `l32i; b*i`. Xtensa has no 32-bit immediate form, so the mask costs a
/// literal-pool LOAD, and the register pressure in `Vm::run` means it is
/// re-loaded at every use: the `Add` arm alone carried four of them. All
/// payloads 32-bit ⇒ a `u32` tag ⇒ the mask and the literal disappear.
/// Keep them 32-bit; `fn_idx`/builtin ids stay `u16` everywhere else.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Num(Fx),
    Arr(u32),
    Fun(u32),
    Builtin(u32),
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
            _ => Fx::ZERO, // oracle-verified: refs act as 0 in arithmetic
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
    /// This function's code: `Program.words[code_start..code_start+code_len]`
    /// (WORD indices). `pc` and jump operands are word indices relative to
    /// `code_start`.
    pub code_start: u32,
    pub code_len: u32,
    /// Debug info: source-position RUNS keyed by fn-relative word index —
    /// (start_pc, line, col), sorted by pc, each run extending to the
    /// next. Statement-granular, so a handful of entries per function.
    /// Empty in lean decodes (device) — pos_at then reports (0, 0).
    pub pos: Vec<(u32, u32, u32)>,
    /// Debug info: name per local slot (params first).
    pub local_names: Vec<String>,
}

impl FnDef {
    /// (line, col) at a fn-relative word index, if known.
    pub fn pos_at(&self, pc: u32) -> (u32, u32) {
        match self.pos.partition_point(|&(off, _, _)| off <= pc) {
            0 => (0, 0),
            i => {
                let (_, line, col) = self.pos[i - 1];
                (line, col)
            }
        }
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

/// The program's word region: every function's instruction words plus the
/// constant pool, exactly as the LXBC blob stores them (little-endian
/// `u32`s). Owned by a host that decoded a blob into RAM, or BORROWED from
/// a `'static` blob — a memory-mapped flash slot on the device — in which
/// case the code and constants cost no RAM at all. Both deref to `[u32]`;
/// nothing downstream cares which.
#[derive(Debug, Clone)]
pub enum Words {
    Owned(Vec<u32>),
    Static(&'static [u32]),
}

impl core::ops::Deref for Words {
    type Target = [u32];
    #[inline(always)]
    fn deref(&self) -> &[u32] {
        match self {
            Words::Owned(v) => v,
            Words::Static(s) => s,
        }
    }
}

/// One constant-pool array: `len` raw 16.16 words at `Program.words[start..]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoolEntry {
    pub start: u32,
    pub len: u32,
}

#[derive(Debug, Clone)]
pub struct Program {
    /// Code + constant pool, one 4-aligned region of u32 words (see
    /// `FnDef.code_start` and [`PoolEntry`]). Instruction words carry
    /// RUNTIME builtin ids; the decoder proved every id against the blob's
    /// by-name import table.
    pub words: Words,
    /// Constant-array pool (the blob's "data section"): every all-numeric
    /// array literal, DEDUPLICATED by content — pixel-art patterns repeat
    /// the same rows/triplets hundreds of times. Each entry is a range of
    /// raw `Fx` words in `words`; `ConstArr` instructions allocate arena
    /// entries that INDEX into this pool until first mutation
    /// (copy-on-write in [`Vm::arr_mut`]).
    pub pool: Vec<PoolEntry>,
    /// `fns[0]` is top-level initialization code.
    pub fns: Vec<FnDef>,
    pub globals: Vec<GlobalDef>,
    /// Exported functions (render, beforeRender, controls, …) by name.
    pub exported_fns: Vec<(String, u16)>,
    /// `assert()` messages (deduplicated; default = the condition's source
    /// text). Kept even by lean decodes — they're user-facing error text,
    /// not debug info.
    pub assert_msgs: Vec<String>,
    /// Global slot holding `pixelCount`.
    pub pixel_count_g: u16,
}

impl Program {
    /// The raw words of const-pool entry `d` (decoder-validated range).
    #[inline]
    pub fn pool_words(&self, d: u32) -> &[u32] {
        let p = self.pool[d as usize];
        &self.words[p.start as usize..(p.start + p.len) as usize]
    }

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
    EaseOutBack,
    EaseOutElastic,
    EaseOutBounce,
    // Luxel extension builtins, batch 3: 2D canvases + bulk array math
    Blur2D,
    ArrayAdd,
    ArraySub,
    ArrayMix,
    CanvasSet,
    CanvasGet,
    EventCount,
    ReadEvent,
    // Luxel extension builtins, batch 5: canvas accumulate + determinism
    // + in-pattern timing controls
    CanvasAdd,
    RandomSeed,
    TimeScale,
    SetFrameRate,
    // Luxel extension builtins, batch 6: the rest of the global
    // post-process chain (setGamma is the first stage, batch 2)
    SetBlur,
    SetGlow,
    SetOutputPalette,
    // Luxel extension builtins, batch 7: the rest of the standard thirty
    // easings (the quad/cubic trios and the "out" springs are above).
    EaseInSine,
    EaseOutSine,
    EaseInOutSine,
    EaseInQuart,
    EaseOutQuart,
    EaseInOutQuart,
    EaseInQuint,
    EaseOutQuint,
    EaseInOutQuint,
    EaseInExpo,
    EaseOutExpo,
    EaseInOutExpo,
    EaseInCirc,
    EaseOutCirc,
    EaseInOutCirc,
    EaseInBack,
    EaseInOutBack,
    EaseInElastic,
    EaseInOutElastic,
    EaseInBounce,
    EaseInOutBounce,
    // Luxel extension builtins, batch 8: the engine-owned per-pixel state
    // buffer (double-buffered feedback without hand-rolled arrays).
    PixelState,
    SetPixelState,
    // Luxel extension builtins, batch 9: curl noise (analytic-derivative
    // simplex — see noise::simplex2_grad).
    Curl2,
    Curl3,
    // Luxel extension builtins, batch 10: whole-frame ("bulk") ops for the
    // `renderFrame` entry — see `crate::bulk`. They write the engine's
    // frame buffer directly and are no-ops anywhere else.
    GridWidth,
    GridHeight,
    Clear,
    FillAll,
    Fade,
    SetPixel,
    FillRange,
    FillHsv,
    FillRgb,
    FillGradient,
    FillRect,
    FillCircle,
    Splat,
    DrawLine,
    FillCanvas,
    Blit,
    // Luxel extension builtins, batch 11: the ops #373 measured out of the
    // first two `renderFrame` conversions — a lattice noise fill, a
    // palette-space canvas fill, a linear 2D stencil and the reduction the
    // stencil needs to stay ahead of a second bytecode pass.
    FillNoise2D,
    FillNoise3D,
    PaintCanvas,
    Stencil2D,
    ArrayMaxAbs,
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
    // familiar aliases: fract=frac, lerp=mix, length/length3=hypot
    b!("fract", Frac), b!("lerp", Mix), b!("length", Hypot), b!("length3", Hypot3),
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
    // springy easings (the polynomial ones are up with the other eases)
    b!("easeOutBack", EaseOutBack), b!("easeOutElastic", EaseOutElastic),
    b!("easeOutBounce", EaseOutBounce),
    // Luxel extensions, batch 3 (appended): 2D canvases + bulk array math.
    // arrayScale(a, k) is feedback(a, k) under its general-purpose name.
    b!("blur2D", Blur2D),
    b!("arrayAdd", ArrayAdd), b!("arraySub", ArraySub),
    b!("arrayScale", Feedback), b!("arrayMix", ArrayMix),
    b!("canvasSet", CanvasSet), b!("canvasGet", CanvasGet),
    // Luxel extensions, batch 4 (appended): external event injection.
    b!("eventCount", EventCount), b!("readEvent", ReadEvent),
    // Luxel extensions, batch 5 (appended): canvas accumulate, seedable
    // `random`, in-pattern clock/frame-rate controls.
    b!("canvasAdd", CanvasAdd), b!("randomSeed", RandomSeed),
    b!("timeScale", TimeScale), b!("setFrameRate", SetFrameRate),
    // Luxel extensions, batch 6 (appended): the global post-process chain
    // beyond setGamma — frame stages the engine runs after render().
    b!("setBlur", SetBlur), b!("setGlow", SetGlow),
    b!("setOutputPalette", SetOutputPalette),
    // Luxel extensions, batch 7 (appended): the remaining easings, so the
    // full standard thirty (ten families × in/out/in-out) are builtins.
    b!("easeInSine", EaseInSine), b!("easeOutSine", EaseOutSine),
    b!("easeInOutSine", EaseInOutSine),
    b!("easeInQuart", EaseInQuart), b!("easeOutQuart", EaseOutQuart),
    b!("easeInOutQuart", EaseInOutQuart),
    b!("easeInQuint", EaseInQuint), b!("easeOutQuint", EaseOutQuint),
    b!("easeInOutQuint", EaseInOutQuint),
    b!("easeInExpo", EaseInExpo), b!("easeOutExpo", EaseOutExpo),
    b!("easeInOutExpo", EaseInOutExpo),
    b!("easeInCirc", EaseInCirc), b!("easeOutCirc", EaseOutCirc),
    b!("easeInOutCirc", EaseInOutCirc),
    b!("easeInBack", EaseInBack), b!("easeInOutBack", EaseInOutBack),
    b!("easeInElastic", EaseInElastic), b!("easeInOutElastic", EaseInOutElastic),
    b!("easeInBounce", EaseInBounce), b!("easeInOutBounce", EaseInOutBounce),
    // Luxel extensions, batch 8 (appended): the per-pixel state buffer.
    b!("pixelState", PixelState), b!("setPixelState", SetPixelState),
    // Luxel extensions, batch 9 (appended): curl noise.
    b!("curl2", Curl2), b!("curl3", Curl3),
    // Luxel extensions, batch 10 (appended): the whole-frame bulk ops that
    // back the `renderFrame` entry (crate::bulk). Ids 166..=181.
    b!("gridWidth", GridWidth), b!("gridHeight", GridHeight),
    b!("clear", Clear), b!("fill", FillAll), b!("fade", Fade),
    b!("setPixel", SetPixel), b!("fillRange", FillRange),
    b!("fillHSV", FillHsv), b!("fillRGB", FillRgb),
    b!("fillGradient", FillGradient),
    b!("fillRect", FillRect), b!("fillCircle", FillCircle),
    b!("splat", Splat), b!("drawLine", DrawLine),
    b!("fillCanvas", FillCanvas), b!("blit", Blit),
    // Luxel extensions, batch 11 (appended): the ops #373 measured out of
    // the aurora-2d and raindrops-2d `renderFrame` conversions — a lattice
    // noise fill, a palette-space canvas fill, a linear 2D stencil and the
    // reduction the stencil needs. Ids 182..=186.
    b!("fillNoise2D", FillNoise2D), b!("fillNoise3D", FillNoise3D),
    b!("paintCanvas", PaintCanvas),
    b!("stencil2D", Stencil2D), b!("arrayMaxAbs", ArrayMaxAbs),
];

/// Channels the per-pixel state buffer can hold (`setPixelState(i, ch, v)`
/// with `ch` in `0..MAX_STATE_CHANNELS`) — enough for a colour plus one
/// scalar; the cap bounds what a pattern can lazily allocate.
pub const MAX_STATE_CHANNELS: usize = 4;

/// The engine-owned per-pixel state buffer behind `pixelState` /
/// `setPixelState`: `channels × n` fixed-point values, double-buffered.
/// Reads come from `front` (last frame's committed values, so every read in
/// a frame sees the same consistent snapshot — neighbours included); writes
/// land in `back`; [`Vm::pixel_state_commit`] swaps them at the end of each
/// frame and copies the new front over the new back so unwritten pixels
/// carry over. Channel-major layout (`ch * n + i`) so adding a channel is
/// an append, not a re-interleave.
///
/// Exists only after the first `setPixelState` call — a pattern that never
/// writes state pays nothing (reads return 0 without allocating).
struct PixelState {
    n: usize,
    channels: usize,
    front: Vec<Fx>,
    back: Vec<Fx>,
}

impl PixelState {
    /// Bytes the two buffers hold (what's charged to the arena byte budget).
    fn bytes(n: usize, channels: usize) -> usize {
        2 * n * channels * core::mem::size_of::<Fx>()
    }
}

/// `#[inline(never)]`: this is a linear scan of the whole table with a
/// `memcmp` per entry, and it is called from name LISTS (the decoder's
/// import table, the compiler, the engine's coordinate-bulk-op probe).
/// Inlined it was emitted eight times in one function alone. Nothing that
/// calls it is on a per-pixel path.
#[inline(never)]
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
        "replace" => "arrayReplace", // oracle-verified: a.replace(2,9) writes from index 0
        "sort" => "arraySort",
        "sortBy" => "arraySortBy",
        "sum" => "arraySum",
        _ => return None,
    };
    lookup_builtin(global)
}

/// Shared by the three bounce easings: piecewise parabolas, n1 = 7.5625,
/// d1 = 2.75 (the standard fit). "in" and "in-out" are reflections of it.
fn ease_out_bounce(t: Fx) -> Fx {
    let n1 = Fx::from_f64(7.5625);
    let d1 = Fx::from_f64(2.75);
    if t < Fx::ONE / d1 {
        n1 * t * t
    } else if t < Fx::from_int(2) / d1 {
        let u = t - Fx::from_f64(1.5) / d1;
        n1 * u * u + Fx::from_f64(0.75)
    } else if t < Fx::from_f64(2.5) / d1 {
        let u = t - Fx::from_f64(2.25) / d1;
        n1 * u * u + Fx::from_f64(0.9375)
    } else {
        let u = t - Fx::from_f64(2.625) / d1;
        n1 * u * u + Fx::from_f64(0.984375)
    }
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
    /// A failed `assert()` — a declared configuration invariant, not a
    /// bug. The engine blocks rendering for the pattern's lifetime (the
    /// fix is a config change, which rebuilds the engine).
    pub is_assert: bool,
}

/// VM resource-guard messages. Kept as consts so [VmError::is_resource_guard]
/// can't drift from the `fail!` sites that raise them.
pub(crate) const ERR_STACK_OVERFLOW: &str = "value stack overflow";
pub(crate) const ERR_STACK_UNDERFLOW: &str = "stack underflow (compiler bug)";
pub(crate) const ERR_EXEC_LIMIT: &str = "execution limit exceeded (infinite loop?)";

impl VmError {
    /// True for the VM's own resource guards (step limit, value-stack
    /// bounds) rather than a language-level runtime error. The engine keeps
    /// these frame-fatal: a pattern-level error aborts only the current
    /// handler invocation (PB blast radius, tools/oracle/oob-probes.mjs),
    /// but re-running a stuck handler per pixel would multiply the step
    /// limit by pixel_count each frame and starve the firmware watchdog.
    pub fn is_resource_guard(&self) -> bool {
        matches!(
            self.message.as_str(),
            ERR_STACK_OVERFLOW | ERR_STACK_UNDERFLOW | ERR_EXEC_LIMIT
        )
    }
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

/// Argument slots a render entry can take: `index, x, y, z`.
pub const PIXEL_ARGS: usize = 4;

/// The loop-invariant half of a per-pixel `render` pass: which function, how
/// many local slots its frame needs, and how many of them the caller's
/// argument array fills. Built once per frame by [`Vm::begin_pixel_pass`]
/// and consumed by [`Vm::render_pixel`] (Gitea #260).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelPlan {
    fn_idx: u16,
    nlocals: u32,
    nargs: u32,
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

/// Byte-ledger cost of a const-backed arena entry (the enum slot + Rc
/// bookkeeping; the element data itself is shared with the program).
const CONST_ENTRY_COST: usize = 32;

const MAX_DEPTH: usize = 48;
const MAX_STACK: usize = 1024;
const MAX_ARGS: usize = 16;
/// Arg slots for the in-loop builtin fast path (the hot builtins take ≤ 3).
const FAST_ARGS: usize = 4;
/// PB's element ledger, oracle-bisected (fw 3.67, 2026-08-29): every array
/// costs its length plus a 4-unit header against a 10,236-unit budget —
/// equivalently a 40 KiB pool of 4-byte elements with 16-byte headers, 16
/// bytes pre-consumed. Pinned by the boundary probes in
/// docs/research/04-oracle-findings.md (single max 10,232; 5113+5113 ok,
/// 5116+5116 abort; 98 frames of per-frame `array(100)`).
pub const DEFAULT_ARRAY_BUDGET: usize = 10_236;
pub const ARRAY_HEADER_UNITS: usize = 4;

/// The per-array header is what bounds the arena *slot* vector: a
/// zero-length array still charges `ARRAY_HEADER_UNITS`, so no pattern can
/// push more than `array_budget / ARRAY_HEADER_UNITS` entries no matter how
/// many `array(0)` / `[]` values it allocates per frame. Dropping the header
/// to 0 would make `while (1) t = array(0)` grow `Vm::arrays` until the host
/// OOMs — the element budget is the only cap on hosts, where
/// `array_byte_budget` is `usize::MAX` (Gitea #124).
const _: () = assert!(ARRAY_HEADER_UNITS > 0);

/// Hard cap on arena ENTRIES, independent of the element ledger.
///
/// The slot vector (`Vm::arrays`) is bookkeeping, not pattern data: it stays
/// on the ordinary allocator — internal DRAM on a device — even when array
/// *storage* is routed to an external arena (`crate::arena`, Gitea #253).
/// Until that existed, `array_budget / ARRAY_HEADER_UNITS` was the only
/// thing bounding it, so a raised element budget would have let
/// `while (1) t = array(0)` grow the slot vector into an OOM. This cap is
/// exactly that old bound at the PB-compat budget, so it can never fire
/// before the element ledger on a board that doesn't raise it — no host,
/// wasm or existing-device behaviour changes.
pub const MAX_ARENA_SLOTS: usize = DEFAULT_ARRAY_BUDGET / ARRAY_HEADER_UNITS;

const FUEL: u32 = 8_000_000;

/// One arena array: owned storage, or an index into the program's
/// const-array pool (until first mutation — copy-on-write). Every `[…]`
/// literal occurrence keeps its own arena identity either way: writing
/// through one handle never affects another. Pool indices are
/// decoder-validated, like every other id the VM trusts.
#[derive(Debug, Clone)]
pub enum ArrRepr {
    Owned(ArrVec<Value>),
    Const(u32),
}

impl Default for ArrRepr {
    fn default() -> Self {
        ArrRepr::Owned(crate::arena::empty())
    }
}

impl ArrRepr {
    #[inline]
    fn view<'a>(&'a self, prog: &'a Program) -> ArrView<'a> {
        match self {
            ArrRepr::Owned(v) => ArrView::Owned(v),
            ArrRepr::Const(d) => ArrView::Const(prog.pool_words(*d)),
        }
    }
}

/// Read-only view of an arena array: owned `Value`s, or the raw 16.16
/// words of a const-pool entry (every element a `Num`) read straight from
/// the program's word region — which on the device is flash.
#[derive(Clone, Copy)]
pub enum ArrView<'a> {
    Owned(&'a [Value]),
    Const(&'a [u32]),
}

impl<'a> ArrView<'a> {
    #[inline]
    pub fn len(&self) -> usize {
        match self {
            ArrView::Owned(v) => v.len(),
            ArrView::Const(w) => w.len(),
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn get(&self, i: usize) -> Option<Value> {
        match self {
            ArrView::Owned(v) => v.get(i).copied(),
            ArrView::Const(w) => w.get(i).map(|&x| Value::Num(Fx::from_raw(x as i32))),
        }
    }

    /// Element `i`; panics past the end like slice indexing.
    #[inline]
    pub fn at(&self, i: usize) -> Value {
        match self {
            ArrView::Owned(v) => v[i],
            ArrView::Const(w) => Value::Num(Fx::from_raw(w[i] as i32)),
        }
    }

    pub fn iter(self) -> impl Iterator<Item = Value> + 'a {
        let (o, c) = match self {
            ArrView::Owned(v) => (Some(v.iter()), None),
            ArrView::Const(w) => (None, Some(w.iter())),
        };
        o.into_iter()
            .flatten()
            .copied()
            .chain(c.into_iter().flatten().map(|&x| Value::Num(Fx::from_raw(x as i32))))
    }

    /// Materialize as owned `Value`s (a const entry decodes its words).
    pub fn to_vec(&self) -> Vec<Value> {
        self.iter().collect()
    }
}

pub struct Vm {
    pub globals: Vec<Value>,
    arrays: Vec<ArrRepr>,
    array_elems: usize,
    /// PB-compat element budget (10,236 units, each array costing len+4 —
    /// arrays are never freed; see DEFAULT_ARRAY_BUDGET).
    pub array_budget: usize,
    /// Actual bytes charged so far (elements × 8 + per-array overhead).
    array_bytes: usize,
    /// Device-RAM byte budget for the arena; `usize::MAX` on hosts. Byte-
    /// accurate so one big array (8 B/element) isn't taxed for the Vec
    /// overhead only swarms of tiny arrays pay.
    pub array_byte_budget: usize,
    stack: Vec<Value>,
    locals: Vec<Value>,
    frames: Vec<Frame>,
    /// Debugger state; None disables all checks (the fast path).
    pub dbg: Option<DebugState>,
    fuel: u32,
    /// Word index (fn-relative) of the instruction currently executing in
    /// the top frame — error attribution (the frame's own pc has already
    /// advanced past it).
    insn_start: u32,
    /// Milliseconds since pattern start; the engine advances this.
    pub time_ms: u64,
    rng: u64,
    prng_state: u32,
    /// Last seed handed to `randomSeed` — its return value (the state
    /// itself is 64-bit and doesn't round-trip through an `Fx`).
    random_seed: Fx,
    /// `timeScale(s)`: the engine multiplies each frame's real delta by
    /// this before advancing the pattern clock. ONE = real time, ZERO
    /// freezes the clock. Never negative.
    pub time_scale: Fx,
    /// `setFrameRate(fps)`: minimum real ms between pattern renders, in
    /// 16.16 (0 = uncapped). Enforced by the engine, which holds the
    /// previous frame while under it. See [`MAX_FRAME_PERIOD_RAW`].
    pub frame_min_raw: u64,
    /// Last fps handed to `setFrameRate` — its return value (0 = uncapped).
    frame_cap_fps: Fx,
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
    /// The engine's frame buffer, lent to the VM (by move — never copied)
    /// for the duration of a `renderFrame` call so the bulk builtins can
    /// write RGB888 straight into it. Empty at every other moment, which
    /// is what makes every bulk op a no-op outside the whole-frame entry.
    pub frame: Vec<[u8; 3]>,
    /// The installed map read as a regular W×H grid, when it is one —
    /// mirrors `Engine::grid` and is maintained at map install, not per
    /// frame. `gridWidth`/`gridHeight` report it and the coordinate-space
    /// bulk ops use it to walk only a shape's bounding box.
    pub frame_grid: Option<crate::outpipe::GridMap>,
    /// Engine-set; used by mapPixels and the no-map 1D fallback.
    pub pixel_count: u32,
    /// `pixelState`/`setPixelState` storage — `None` until a pattern's
    /// first write allocates it (see [`PixelState`]).
    pixel_state: Option<alloc::boxed::Box<PixelState>>,
    palette: Vec<(Fx, [Fx; 3])>,
    /// Arena id backing the palette. On PB `setPalette(arr)` holds a LIVE
    /// reference: later writes through `arr` change what `paint()` looks
    /// up, with no second `setPalette` call (oracle-probed via
    /// fast-palette-blending, 2026-08-29). We keep the id and rebuild the
    /// cooked stops lazily when the backing array is mutated.
    palette_src: Option<u32>,
    palette_dirty: bool,
    perlin_wrap: [i32; 3],
    /// Wall-clock unix seconds (timezone-adjusted by the host); None → the
    /// clock builtins return 0. With-time civil conversion is oracle-exact
    /// (2026-08-22); the no-time case is UNTESTABLE on a configured PB
    /// (can't unset its clock via the public API), so 0 stays our choice.
    pub wall_unix: Option<i64>,
    /// Output gamma set by `setGamma(g)`; ZERO/ONE = off. Last stage of the
    /// post-process chain the engine runs after render (Luxel extension).
    pub post_gamma: Fx,
    /// `setBlur(amount, passes)`: neighbor weight 0..1 (ZERO = off) and the
    /// pass count, both applied to the finished frame in pixel-index order.
    pub post_blur: Fx,
    pub post_blur_passes: u8,
    /// `setGlow(amount)`: light-bleed bloom strength 0..1 (ZERO = off).
    pub post_glow: Fx,
    /// `setOutputPalette(pal, amount)`: recolor the finished frame by luma.
    /// Empty = off. `post_palette_epoch` bumps on every install so the
    /// engine knows to rebuild its 256-entry lookup.
    pub post_palette: Vec<(Fx, [Fx; 3])>,
    pub post_palette_amount: Fx,
    pub post_palette_epoch: u32,
    /// Injected external events `[type, x, y, value]`, drained FIFO by
    /// `readEvent`. The engine pushes (bounded — see `MAX_EVENTS`); a
    /// pattern switch rebuilds the VM, which clears the queue.
    pub events: VecDeque<[Fx; 4]>,
    /// Bit per pin (0..63): the last `pinMode` for that pin asked for an
    /// internal pull-up. There is no real GPIO yet, so `digitalRead` reports
    /// the pin's *idle* level, and a pulled-up pin idles HIGH — a
    /// button-to-ground pattern must read "not pressed", not "held forever"
    /// (Gitea #177 item 1). Everything else idles LOW, as before.
    pin_pullup: u64,
    /// Bit per pin (0..63): an external host is DRIVING this pin, so
    /// `digitalRead` reports `pin_level` for it instead of the `pinMode` idle
    /// level — the pin-injection ABI (Gitea #177 item 2, [`Vm::set_pin`]).
    /// A driven pin stays driven across later `pinMode` calls: the injection
    /// stands in for a wire, and a wire does not come loose because the
    /// pattern reconfigured the pad.
    pin_driven: u64,
    /// Bit per pin (0..63): the injected level, meaningful only where
    /// `pin_driven` is set.
    pin_level: u64,
    /// Bit per pin (0..63): the PATTERN has touched this pin — named it in a
    /// `pinMode` or `digitalRead`. Pin numbers are runtime values, so this is
    /// the only way a host can know which pins are worth offering a control
    /// for; the playground gates its pin panel on it (Gitea #205). Sticky for
    /// the life of the VM, so a pin read inside a rare branch does not make
    /// the control flicker in and out.
    pin_used: u64,
    /// The last `pinMode` value per pin (0..63), verbatim — Arduino/ESP32
    /// bits: 1 INPUT, 2 OUTPUT, 4 pull-up, 8 pull-down, 16 open-drain. Zero
    /// = never configured. The firmware reads this to configure the real
    /// pad (Gitea #177 item 4); `pin_pullup` stays the fast idle-level mask.
    pin_mode: [u8; MAX_TRACKED_PIN as usize + 1],
    /// Bit per pin (0..63): the level the pattern last `digitalWrite`d. The
    /// engine never drives anything itself — a host (the firmware) copies
    /// these onto pads the pattern configured as OUTPUT. LOW until written,
    /// which is also what Arduino leaves a freshly-OUTPUT pin at.
    pin_out_level: u64,
    /// Bit per pin (0..63): the PATTERN has read this pin with `analogRead` or
    /// `touchRead`. Same purpose as `pin_used` on the digital surface — a host
    /// cannot know which analog pins a pattern samples without watching it run
    /// (Gitea #206). Sticky for the life of the VM.
    analog_used: u64,
    /// Injected `analogRead`/`touchRead` value per pin, as a **16.15 code**
    /// (`Fx` raw >> 1) so the whole 0..1 range including exactly 1.0 fits a
    /// `u16`: 128 bytes of VM state, no allocation. Both builtins share one
    /// table — they are distinct peripherals on real silicon, but no board
    /// wires an ADC and a touch pad to the same pad, and one injected "what
    /// does this pin read" value is all a host has to offer either way.
    /// Zero (the default) is what an undriven analog pin reads, so releasing
    /// a pin is simply writing 0 — there is no separate driven bit.
    analog_level: [u16; MAX_TRACKED_PIN as usize + 1],
    /// Dynamic opcode counters — `profile` feature only (host tooling;
    /// see the `prof` module at the bottom of this file).
    #[cfg(feature = "profile")]
    prof: prof::Profile,
    /// (fn, end word index, opcode) of the previously executed
    /// instruction, for the statically-adjacent bigram/trigram counts.
    #[cfg(feature = "profile")]
    prof_prev: Option<(u16, u32, u8)>,
    #[cfg(feature = "profile")]
    prof_prev2: Option<(u16, u32, u8)>,
    /// Was `prof_prev` itself statically adjacent to `prof_prev2`?
    #[cfg(feature = "profile")]
    prof_prev_seq: bool,
}

/// Highest pin number `pin_pullup` can track. Above it `pinMode` is still a
/// no-op and `digitalRead` still reads 0 — ESP32 tops out at GPIO 39, so the
/// window covers every pin a real board exposes.
pub const MAX_TRACKED_PIN: i32 = 63;

/// Arduino/ESP32 `pinMode` pull-up bit — `INPUT_PULLUP` (5) is `INPUT` (1)
/// with this set, so masking also honours a hand-built `INPUT | 4`.
/// Constant values are oracle-probed from PB fw 3.67 (see `compile.rs`).
const PIN_MODE_PULLUP: i32 = 0x04;

/// Event-queue capacity: when full the oldest event is dropped, so the
/// freshest input wins and a pattern that never reads can't leak.
pub const MAX_EVENTS: usize = 32;

/// `setBlur`'s pass ceiling. Each pass is another O(pixels) sweep of the
/// finished frame; 8 is already a very wide blur and keeps the worst case
/// bounded on an ESP32.
pub const MAX_BLUR_PASSES: i32 = 8;

/// Longest period `setFrameRate` can ask for, 60 s in 16.16 ms. A pattern
/// asking for 1/3600 fps would otherwise stop rendering for an hour and
/// look like a hang; the cap keeps the worst case explainable.
pub const MAX_FRAME_PERIOD_RAW: u64 = 60_000 << 16;

/// Sample a stop list at `v`, PB's `paint` semantics: below the first stop
/// clamps to its color, exactly the last stop yields that color, past it is
/// BLACK (the ends are asymmetric — oracle-verified, fw 3.67, 2026-08-22).
/// An empty palette is the grayscale ramp `[v, v, v]` (also oracle-verified:
/// a pattern with no `setPalette` paints exactly that).
/// Where `paint(x)` samples the palette. PB semantics (ramp-palette pixel
/// oracle, 2026-07-08): the position wraps as floored-frac(x) EXACTLY
/// (1.25 → 0.25, −0.5 → 0.5), with two measured edge artifacts — x == 1
/// stays at the palette end, and whole numbers ≥ 2 land at 254/255 (just
/// under it). Pathological inputs, but pinned to match the device byte for
/// byte, which is why `paintCanvas` shares this function rather than
/// re-deriving it.
pub(crate) fn paint_pos(x: Fx) -> Fx {
    let frac = x.mod_floor(Fx::ONE);
    if frac == Fx::ZERO && x >= Fx::ONE {
        if x == Fx::ONE {
            Fx::ONE
        } else {
            Fx::from_raw(65535) // 1−ε: matches both probe palettes
        }
    } else {
        frac
    }
}

pub fn sample_palette(pal: &[(Fx, [Fx; 3])], v: Fx) -> [Fx; 3] {
    if pal.is_empty() {
        return [v, v, v];
    }
    let first = pal[0];
    let last = pal[pal.len() - 1];
    if v <= first.0 {
        return first.1;
    }
    if v == last.0 {
        return last.1;
    }
    if v > last.0 {
        return [Fx::ZERO; 3];
    }
    for w in pal.windows(2) {
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

#[derive(Debug, Clone)]
pub struct MapData {
    pub dims: u8,
    /// Per-pixel normalized coordinates — empty when `grid` is set.
    pub coords: Vec<[Fx; 3]>,
    /// Procedural row-major grid, `(w, h)`: coordinates are computed per
    /// pixel on read and nothing is stored. A 64x64 panel's map is 48 KB as
    /// `coords` — the whole idle heap on the S3 panel board — and zero bytes
    /// here (Gitea #258). Normalization matches [`Engine::set_map`] exactly
    /// (0..65535/65536 per axis, `(v·65535 + span/2)/span`).
    pub grid: Option<(u16, u16)>,
}

impl MapData {
    /// A procedural `w`×`h` row-major grid (2D).
    pub fn grid(w: u16, h: u16) -> MapData {
        MapData { dims: 2, coords: Vec::new(), grid: Some((w.max(1), h.max(1))) }
    }

    /// Number of pixels the map covers.
    pub fn len(&self) -> usize {
        match self.grid {
            Some((w, h)) => w as usize * h as usize,
            None => self.coords.len(),
        }
    }

    /// Normalized coordinate of pixel `i`; zeros past the end of the map.
    #[inline]
    pub fn coord(&self, i: usize) -> [Fx; 3] {
        match self.grid {
            Some((w, h)) => {
                let (w, h) = (w as usize, h as usize);
                let (col, row) = (i % w, i / w);
                if row >= h {
                    return [Fx::ZERO; 3];
                }
                #[inline]
                fn norm(v: usize, n: usize) -> Fx {
                    if n <= 1 {
                        Fx::ZERO
                    } else {
                        let span = n as i64 - 1;
                        Fx::from_raw(((v as i64 * 65_535 + span / 2) / span) as i32)
                    }
                }
                [norm(col, w), norm(row, h), Fx::ZERO]
            }
            None => self.coords.get(i).copied().unwrap_or([Fx::ZERO; 3]),
        }
    }
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
            array_bytes: 0,
            array_byte_budget: usize::MAX,
            stack: Vec::new(),
            locals: Vec::new(),
            frames: Vec::new(),
            dbg: None,
            fuel: FUEL,
            insn_start: 0,
            time_ms: 0,
            transform: IDENTITY,
            transform_active: false,
            transform_ops: 0,
            map: None,
            frame: Vec::new(),
            frame_grid: None,
            pixel_count: 0,
            pixel_state: None,
            palette: Vec::new(),
            palette_src: None,
            palette_dirty: false,
            perlin_wrap: [256; 3],
            wall_unix: None,
            post_gamma: Fx::ZERO,
            post_blur: Fx::ZERO,
            post_blur_passes: 1,
            post_glow: Fx::ZERO,
            post_palette: Vec::new(),
            post_palette_amount: Fx::ONE,
            post_palette_epoch: 0,
            events: VecDeque::new(),
            pin_pullup: 0,
            pin_driven: 0,
            pin_level: 0,
            pin_used: 0,
            pin_mode: [0; MAX_TRACKED_PIN as usize + 1],
            pin_out_level: 0,
            analog_used: 0,
            analog_level: [0; MAX_TRACKED_PIN as usize + 1],
            #[cfg(feature = "profile")]
            prof: prof::Profile::default(),
            #[cfg(feature = "profile")]
            prof_prev: None,
            #[cfg(feature = "profile")]
            prof_prev2: None,
            #[cfg(feature = "profile")]
            prof_prev_seq: false,
            rng: seed | 1,
            prng_state: 0xC0FFEE ^ (seed as u32) | 1,
            random_seed: Fx::ZERO,
            time_scale: Fx::ONE,
            frame_min_raw: 0,
            frame_cap_fps: Fx::ZERO,
            pixel: [Fx::ZERO; 3],
            pixel_written: false,
            plot_coord: [Fx::ZERO; 3],
            plot_dims: 0,
            plot_written: false,
        }
    }

    /// Number of live arena entries. Never freed, so this only grows within
    /// a Vm — bounded by `array_budget / ARRAY_HEADER_UNITS`.
    pub fn arena_slots(&self) -> usize {
        self.arrays.len()
    }

    /// Units charged against `array_budget` (PB's element ledger).
    pub fn arena_elems(&self) -> usize {
        self.array_elems
    }

    /// Bytes charged against `array_byte_budget`.
    pub fn arena_bytes(&self) -> usize {
        self.array_bytes
    }

    /// Bytes the per-pixel state buffer currently holds — 0 until a pattern
    /// calls `setPixelState`, and included in [`arena_bytes`].
    pub fn pixel_state_bytes(&self) -> usize {
        self.pixel_state
            .as_ref()
            .map_or(0, |s| PixelState::bytes(s.n, s.channels))
    }

    /// The pixel count the state buffer is sized to. `pixel_count` is
    /// engine-set only after top-level init has run (mapPixels is a no-op
    /// during init by design), but a pattern may legitimately seed state
    /// at top level — so fall back to the `pixelCount` global then.
    fn state_pixel_count(&self, prog: &Program) -> usize {
        if self.pixel_count > 0 {
            self.pixel_count as usize
        } else {
            self.globals[prog.pixel_count_g as usize]
                .num()
                .to_int_trunc()
                .max(0) as usize
        }
    }

    /// Make sure the state buffer exists and holds channel `ch`, growing it
    /// (or creating it) on demand. Budget-checked BEFORE reserving and with
    /// fallible reservations, like `alloc_array_zeroed`: on a small-heap
    /// device an oversized buffer is a recorded runtime error, never an
    /// allocator panic. Growth keeps every existing channel's values —
    /// channel-major layout makes the new channels a plain append.
    fn pixel_state_ensure(&mut self, prog: &Program, ch: usize) -> Result<(), &'static str> {
        let want = ch + 1;
        let (n, have) = match &self.pixel_state {
            Some(s) if s.channels >= want => return Ok(()),
            Some(s) => (s.n, s.channels),
            None => (self.state_pixel_count(prog), 0),
        };
        let delta = PixelState::bytes(n, want) - PixelState::bytes(n, have);
        self.charge_array_bytes(delta)?;
        let extra = n * (want - have);
        match &mut self.pixel_state {
            Some(s) => {
                if s.front.try_reserve_exact(extra).is_err()
                    || s.back.try_reserve_exact(extra).is_err()
                {
                    return Err("out of memory for pixel state");
                }
                s.front.resize(n * want, Fx::ZERO);
                s.back.resize(n * want, Fx::ZERO);
                s.channels = want;
            }
            None => {
                let mut front: Vec<Fx> = Vec::new();
                let mut back: Vec<Fx> = Vec::new();
                if front.try_reserve_exact(extra).is_err() || back.try_reserve_exact(extra).is_err()
                {
                    return Err("out of memory for pixel state");
                }
                front.resize(extra, Fx::ZERO);
                back.resize(extra, Fx::ZERO);
                self.pixel_state = Some(alloc::boxed::Box::new(PixelState {
                    n,
                    channels: want,
                    front,
                    back,
                }));
            }
        }
        self.array_bytes += delta;
        Ok(())
    }

    /// End-of-frame handoff for the state buffer: this frame's writes
    /// become next frame's reads, and pixels nobody wrote keep their value.
    /// A no-op (one branch) when no pattern has allocated state.
    pub fn pixel_state_commit(&mut self) {
        if let Some(s) = &mut self.pixel_state {
            core::mem::swap(&mut s.front, &mut s.back);
            s.back.copy_from_slice(&s.front);
        }
    }

    pub fn array<'a>(&'a self, prog: &'a Program, id: u32) -> Option<ArrView<'a>> {
        self.arrays.get(id as usize).map(|a| a.view(prog))
    }

    /// Mutable view of an array (sensor-frame injection writes in place).
    /// A const-backed array is materialized first (copy-on-write); on
    /// allocation failure this returns None rather than panicking.
    pub fn array_mut(&mut self, prog: &Program, id: u32) -> Option<&mut [Value]> {
        self.arr_mut(prog, id).ok().map(|v| v.as_mut_slice())
    }

    /// Drive a digital input pin from OUTSIDE the pattern — the pin-injection
    /// ABI (Gitea #177 item 2). `Some(true)`/`Some(false)` holds the pin HIGH
    /// or LOW no matter what `pinMode` asked for; `None` releases it back to
    /// its idle level (HIGH under a pull-up, LOW otherwise). Returns false for
    /// a pin outside `0..=MAX_TRACKED_PIN`, where there is nowhere to store
    /// the state — a silent no-op would read exactly like a stuck input.
    ///
    /// This is the injection surface a host uses to stand in for real GPIO
    /// (which does not exist yet — #177 item 4): the playground, the port
    /// review harness and the CLI mirror all drive buttons through it.
    pub fn set_pin(&mut self, pin: i32, level: Option<bool>) -> bool {
        if !(0..=MAX_TRACKED_PIN).contains(&pin) {
            return false;
        }
        let bit = 1u64 << pin;
        match level {
            Some(true) => {
                self.pin_driven |= bit;
                self.pin_level |= bit;
            }
            Some(false) => {
                self.pin_driven |= bit;
                self.pin_level &= !bit;
            }
            None => {
                self.pin_driven &= !bit;
                self.pin_level &= !bit;
            }
        }
        true
    }

    /// The level `digitalRead(pin)` reports: the injected level while a host
    /// drives the pin, otherwise the `pinMode` idle level.
    pub fn pin_read(&self, pin: i32) -> bool {
        if !(0..=MAX_TRACKED_PIN).contains(&pin) {
            return false;
        }
        let bit = 1u64 << pin;
        let src = if self.pin_driven & bit != 0 {
            self.pin_level
        } else {
            self.pin_pullup
        };
        src & bit != 0
    }

    /// Bit per pin (0..63): the pattern has named this pin in a `pinMode` or
    /// `digitalRead`, so a host has something real to offer a control for
    /// (Gitea #205). Empty for a pattern that never touches GPIO.
    pub fn pins_used(&self) -> u64 {
        self.pin_used
    }

    /// Bit per pin (0..63): the level `digitalRead` reports for an UNDRIVEN
    /// pin — set means the pin idles HIGH (a `pinMode` pull-up). A host uses
    /// it to decide which way "pressing" the pin should move it.
    pub fn pins_idle_high(&self) -> u64 {
        self.pin_pullup
    }

    /// The last `pinMode` value for `pin`, verbatim (1 INPUT, 2 OUTPUT,
    /// 5 INPUT_PULLUP, 9 INPUT_PULLDOWN, 18 OUTPUT_OPEN_DRAIN), or 0 if the
    /// pattern never configured it. Out-of-window pins read 0.
    pub fn pin_mode(&self, pin: i32) -> u8 {
        if !(0..=MAX_TRACKED_PIN).contains(&pin) {
            return 0;
        }
        self.pin_mode[pin as usize]
    }

    /// Bit per pin (0..63): the level the pattern last `digitalWrite`d —
    /// what a host copies onto the real pad for pins configured OUTPUT.
    pub fn pins_out_high(&self) -> u64 {
        self.pin_out_level
    }

    /// Drive an analog input pin from OUTSIDE the pattern, so
    /// `analogRead(pin)` / `touchRead(pin)` report `value` instead of 0 — the
    /// analog half of the pin-injection ABI (Gitea #206). `value` is clamped
    /// to the 0..1 range both builtins are documented to return and stored as
    /// a 16.15 code, so it round-trips exactly for every value a pattern
    /// literal can express. Writing [`Fx::ZERO`] releases the pin: an
    /// undriven analog pin reads 0.
    ///
    /// Returns false for a pin outside `0..=MAX_TRACKED_PIN`, where there is
    /// nowhere to store the value — the same "a typo'd pin must not look like
    /// a stuck input" contract as [`Vm::set_pin`].
    pub fn set_analog_pin(&mut self, pin: i32, value: Fx) -> bool {
        if !(0..=MAX_TRACKED_PIN).contains(&pin) {
            return false;
        }
        let raw = value.raw().clamp(0, Fx::ONE.raw());
        self.analog_level[pin as usize] = (raw >> 1) as u16;
        true
    }

    /// The value `analogRead(pin)` / `touchRead(pin)` report: the injected
    /// value, or [`Fx::ZERO`] while nothing has driven the pin.
    pub fn analog_read(&self, pin: i32) -> Fx {
        if !(0..=MAX_TRACKED_PIN).contains(&pin) {
            return Fx::ZERO;
        }
        Fx::from_raw((self.analog_level[pin as usize] as i32) << 1)
    }

    /// Bit per pin (0..63): the pattern has named this pin in an `analogRead`
    /// or `touchRead`, so a host has something real to offer a slider for
    /// (Gitea #206). Separate from [`Vm::pins_used`], though a pin shows up in
    /// both if a pattern reads it through both surfaces.
    pub fn analog_pins_used(&self) -> u64 {
        self.analog_used
    }

    /// Read view by id — arena ids come from the VM itself, so `id` is
    /// always valid at these call sites (matches the old direct indexing).
    #[inline]
    fn arr<'a>(&'a self, prog: &'a Program, id: u32) -> ArrView<'a> {
        self.arrays[id as usize].view(prog)
    }

    /// Mutable storage by id, materializing const-backed arrays
    /// (copy-on-write). Fails only if the copy can't be allocated.
    fn arr_mut(&mut self, prog: &Program, id: u32) -> Result<&mut ArrVec<Value>, &'static str> {
        if self.palette_src == Some(id) {
            self.palette_dirty = true;
        }
        if let ArrRepr::Const(d) = self.arrays[id as usize] {
            let data: &[u32] = prog.pool_words(d);
            // The const data was never on the byte ledger (it is shared with
            // the program); the owned copy joins it now, replacing the
            // entry's CONST_ENTRY_COST. The ELEMENTS are already charged —
            // `alloc_const_array` counted them — so only the byte half is
            // checked here, and it is checked BEFORE anything is reserved: a
            // promotion at the cap must error, not overshoot the budget
            // (Gitea #132). The error is a plain pattern-level runtime error
            // like the OOM below, so the blast radius stays PB-shaped
            // (Gitea #84): the current handler invocation aborts, nothing more.
            let delta = Self::array_cost(data.len()) - CONST_ENTRY_COST;
            self.charge_array_bytes(delta)?;
            let mut owned: ArrVec<Value> = crate::arena::empty();
            if owned.try_reserve_exact(data.len()).is_err() {
                return Err("out of memory for array");
            }
            owned.extend(data.iter().map(|&w| Value::Num(Fx::from_raw(w as i32))));
            self.array_bytes += delta;
            self.arrays[id as usize] = ArrRepr::Owned(owned);
        }
        match &mut self.arrays[id as usize] {
            ArrRepr::Owned(v) => Ok(v),
            ArrRepr::Const(_) => unreachable!("materialized above"),
        }
    }

    /// Simultaneous mutable-dst + read-only-src views for the bulk array
    /// ops (`arrayAdd` and friends). `dst` is materialized (copy-on-write)
    /// first. The ids must differ — callers handle `dst == src` themselves
    /// (each op has a cheap closed form for the aliased case).
    fn arr_pair<'a>(
        &'a mut self,
        prog: &'a Program,
        dst: u32,
        src: u32,
    ) -> Result<(&'a mut [Value], ArrView<'a>), &'static str> {
        debug_assert_ne!(dst, src);
        self.arr_mut(prog, dst)?;
        let (d, s) = (dst as usize, src as usize);
        let (dslot, sslot) = if d < s {
            let (lo, hi) = self.arrays.split_at_mut(s);
            (&mut lo[d], &hi[0])
        } else {
            let (lo, hi) = self.arrays.split_at_mut(d);
            (&mut hi[0], &lo[s])
        };
        let ArrRepr::Owned(dv) = dslot else {
            unreachable!("materialized above")
        };
        Ok((dv.as_mut_slice(), sslot.view(prog)))
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

    /// [`Vm::err_at`] for a static message, out of line and cold — see the
    /// `fail!` macro in [`Vm::run`].
    #[inline(never)]
    #[cold]
    fn err_static(&self, prog: &Program, message: &str) -> VmError {
        self.err_at(prog, message.into())
    }

    fn err_at(&self, prog: &Program, message: String) -> VmError {
        match self.frames.last() {
            Some(f) => {
                // the frame's pc has advanced past the faulting instruction;
                // the dispatch loop records each instruction's start offset
                let pc = self.insn_start;
                let (line, col) = prog.fns[f.fn_idx as usize].pos_at(pc);
                VmError {
                    message,
                    fn_idx: f.fn_idx,
                    pc,
                    line,
                    col,
                    is_assert: false,
                }
            }
            None => VmError {
                message,
                fn_idx: u16::MAX,
                pc: u32::MAX,
                line: 0,
                col: 0,
                is_assert: false,
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

    /// Hoist the loop-invariant half of a per-pixel `render` pass out of the
    /// pixel loop, and size the frame storage so no pixel allocates. Drops
    /// any suspended run, exactly as the [`Vm::start`] it replaces did.
    ///
    /// `argc` is how many of the caller's argument slots are live for this
    /// entry (2 for `render(index, x)`, 3 for `render2D`, 4 for `render3D`).
    pub fn begin_pixel_pass(&mut self, prog: &Program, fn_idx: u16, argc: usize) -> PixelPlan {
        self.clear_run();
        let (params, locals) = match prog.fns.get(fn_idx as usize) {
            Some(f) => (f.params as usize, f.locals as usize),
            None => (0, 0),
        };
        // `push_frame` filled `locals` slots and pushed one `Frame`; reserve
        // both once so `render_pixel` never grows a Vec (Gitea #260).
        self.locals.reserve(locals);
        self.frames.reserve(1);
        PixelPlan {
            fn_idx,
            nlocals: locals as u32,
            // push_frame fills slot i from `args.get(i)` while i < params and
            // defaults the rest, so exactly this many slots come from args.
            nargs: params.min(argc).min(locals) as u32,
        }
    }

    /// One pixel's `render` call. Semantically identical to
    /// `start(prog, plan.fn_idx, &args[..argc], false)` for the plan's
    /// function — same fuel reset, same locals (args first, defaults after),
    /// same `clear_run` on error — with the per-call work that
    /// [`Vm::begin_pixel_pass`] resolved once hoisted out of the loop. The
    /// debugger and map mode keep using `start`/`resume`.
    ///
    /// An empty `render(index)` cost ~430 Xtensa cycles of pure entry before
    /// this: most of it `render_args`, `push_frame` and a `Result<Outcome,
    /// VmError>` travelling through memory once per pixel (Gitea #260).
    #[inline]
    pub fn render_pixel(
        &mut self,
        prog: &Program,
        plan: &PixelPlan,
        args: &[Value; PIXEL_ARGS],
    ) -> Result<(), VmError> {
        self.fuel = FUEL;
        let n = plan.nlocals as usize;
        let k = (plan.nargs as usize).min(PIXEL_ARGS);
        self.stack.clear();
        self.locals.clear();
        // `push_frame`'s locals loop, minus its per-call re-derivation of the
        // shape. Written as one pass rather than `resize` + `copy_from_slice`
        // because those compile to a ROM `memset` and a ROM `memcpy` call for
        // the one or two words a render frame actually holds.
        #[allow(clippy::needless_range_loop)] // indexing IS the point: slots
        // past `k` take the default, not an argument
        for i in 0..n {
            self.locals
                .push(if i < k { args[i] } else { Value::default() });
        }
        self.frames.clear();
        self.frames.push(Frame {
            fn_idx: plan.fn_idx,
            pc: 0,
            locals_base: 0,
            stack_base: 0,
        });
        match self.run(prog, 0, false) {
            Ok(_) => Ok(()),
            Err(e) => {
                self.clear_run();
                Err(e)
            }
        }
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

    /// The debugger's whole per-instruction obligation in one call:
    /// publish the frame pc (so an inspector sees where execution is) and
    /// decide whether to pause. `#[cold]` + `#[inline(never)]` keeps it and
    /// everything it reaches OUT of [`Vm::run`]'s dispatch loop — see the
    /// call site.
    #[cold]
    #[inline(never)]
    fn debug_step(&mut self, prog: &Program, pc: u32) -> bool {
        let Some(f) = self.frames.last_mut() else {
            return false;
        };
        f.pc = pc;
        // the running function IS the top frame's — reading it here rather
        // than taking it as an argument keeps one more value out of the
        // dispatch loop's register set (Gitea #312)
        let fi = f.fn_idx;
        self.debug_stop(prog, fi, pc)
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

    /// The body of `LoadIdx`, shared with its two fused forms (Gitea #261).
    /// One function so the arena read is written once; the compiler is free
    /// to inline it into all three arms, which it does.
    ///
    /// Do NOT make this `#[inline(never)]` to shrink the dispatch loop: it
    /// buys nothing on Xtensa (the three indexing arms come out the same
    /// length or two instructions LONGER) and costs the host a lot — an
    /// array-heavy pattern lost 18 % and `colourful-fireflies` 21 % on
    /// x86, which the wasm playground would pay too (Gitea #312).
    #[inline]
    fn index_read(&mut self, prog: &Program, arr: Value, idx: Fx) -> Result<Value, &'static str> {
        let Value::Arr(a) = arr else {
            return Err("indexing a non-array value");
        };
        if idx.raw() < 0 {
            return Err("array index out of bounds");
        }
        let i = idx.to_int_trunc() as usize;
        match self.arr(prog, a).get(i) {
            Some(v) => Ok(v),
            None => Err("array index out of bounds"),
        }
    }

    /// A failed `assert()`, built out of line: `format!` drags the whole
    /// formatting machinery in with it, and an assertion fires at most once
    /// per pattern (Gitea #312).
    #[cold]
    #[inline(never)]
    fn assert_failed(&mut self, prog: &Program, m: u16) -> VmError {
        // decoder-validated: m < assert_msgs.len()
        let px = self.globals[prog.pixel_count_g as usize]
            .num()
            .to_int_trunc();
        let mut e = self.err_at(
            prog,
            alloc::format!(
                "pattern requires: {} (pixelCount = {px})",
                prog.assert_msgs[m as usize]
            ),
        );
        e.is_assert = true;
        e
    }

    /// The out-of-line half of `CallBuiltin` (and its constant-argument
    /// fusions, Gitea #261): everything `builtin_fast` could not answer in
    /// the loop, plus the error attribution. Kept out of line so the two
    /// dispatch arms stay small — it is the cold path by construction.
    #[inline(never)]
    fn call_builtin_slow(
        &mut self,
        prog: &Program,
        b: u16,
        argc: usize,
    ) -> Result<Value, VmError> {
        self.call_builtin(prog, b, argc).map_err(|mut e| {
            // attribute to the call site if the builtin did not
            if e.pc == u32::MAX {
                e = self.err_at(prog, core::mem::take(&mut e.message));
            }
            e
        })
    }

    /// A user-function call: pop the arguments and push the callee's frame.
    /// Out of line (Gitea #312) — the 128-byte `[Value; MAX_ARGS]` buffer and
    /// its zero-init lived in [`Vm::run`]'s stack frame and were emitted
    /// twice, for a step that happens once per CALL, never per instruction.
    #[inline(never)]
    fn enter_call(&mut self, prog: &Program, f: u16, argc: usize) -> Result<(), VmError> {
        let mut args = [Value::default(); MAX_ARGS];
        let n = self.pop_args_into(&mut args, argc);
        self.push_frame(prog, f, &args[..n])
    }

    /// Pop `argc` values into the caller's buffer (no 128-byte array
    /// returned by value); returns how many slots are meaningful.
    #[inline(always)]
    fn pop_args_into(&mut self, args: &mut [Value; MAX_ARGS], argc: usize) -> usize {
        let n = argc.min(MAX_ARGS);
        for i in (0..n).rev() {
            args[i] = self.stack.pop().unwrap_or_default();
        }
        n
    }

    /// The interpreter loop over the explicit frame stack. Returns when the
    /// stack unwinds back to `base` frames (Done) or a debug stop fires
    /// (Paused — only when `debug`). Frames/locals/stack stay intact while
    /// paused so the debugger can inspect and resume.
    ///
    /// `iram-vm` (Gitea #312) puts this function in the Xtensa/RISC-V
    /// chips' `.rwtext` — internal SRAM, executed without going through the
    /// flash cache. It is what `esp_hal::ram` expands to, spelled by hand
    /// because luxel-core does not (and must not) depend on esp-hal. The
    /// feature is off by default and MUST stay that way for hosts: on a
    /// host target `.rwtext` is just a stray section name, and on a device
    /// it spends ~16 KB of the scarcest memory there is.
    #[cfg_attr(feature = "iram-vm", link_section = ".rwtext")]
    #[cfg_attr(feature = "iram-vm", inline(never))]
    fn run(&mut self, prog: &Program, base: usize, debug: bool) -> Result<Outcome, VmError> {
        // Two pieces of per-instruction bookkeeping that used to be FIELDS of
        // `self`, written on every single dispatch (Gitea #312). `fuel` cost a
        // load, a compare, a decrement and a store; `insn_start` cost a store;
        // together ~7 cycles of a ~94-cycle op on the S3, and only the error
        // paths ever read either. As locals they stay in registers, and the
        // handful of exits publish them: every `return` inside the loop, plus
        // the two sites that can re-enter the VM through an array callback.
        // Declared here, ahead of the macros, because a `macro_rules!` body
        // resolves its identifiers in the scope of its DEFINITION.
        let mut fuel = self.fuel;
        #[allow(unused_assignments)] // the seed is never read; the loop writes it first
        let mut insn_at: u32 = 0;
        // Every failure site goes through ONE cold, out-of-line helper: the
        // `String` construction inlined at ~45 sites otherwise bloats the
        // dispatch loop and costs the hot path registers (Gitea #261 — the
        // superinstruction arms added another twenty of them).
        macro_rules! fail {
            ($msg:expr) => {{
                self.insn_start = insn_at;
                self.fuel = fuel;
                return Err(self.err_static(prog, $msg));
            }};
        }
        macro_rules! push {
            ($v:expr) => {{
                if self.stack.len() >= MAX_STACK {
                    fail!(ERR_STACK_OVERFLOW);
                }
                self.stack.push($v);
            }};
        }
        macro_rules! pop {
            () => {
                match self.stack.pop() {
                    Some(v) => v,
                    None => fail!(ERR_STACK_UNDERFLOW),
                }
            };
        }
        // Binary arithmetic in place (Gitea #312). `pop; pop; push` costs
        // THREE writes of the Vec's length field plus a `MAX_STACK` check
        // the shape cannot need — the stack shrinks by one, so it cannot
        // overflow. Rewriting the top two slots and truncating once is the
        // same operation with one length write and no check: −4 Xtensa
        // instructions and a branch out of the ~25-instruction `Add` arm.
        macro_rules! binnum {
            ($op:tt) => {{
                let n = self.stack.len();
                if n < 2 {
                    fail!(ERR_STACK_UNDERFLOW);
                }
                let b = self.stack[n - 1].num();
                let a = self.stack[n - 2].num();
                self.stack[n - 2] = Value::Num(a $op b);
                self.stack.truncate(n - 1);
            }};
        }
        macro_rules! bincmp {
            ($op:tt) => {{
                let n = self.stack.len();
                if n < 2 {
                    fail!(ERR_STACK_UNDERFLOW);
                }
                let b = self.stack[n - 1].num();
                let a = self.stack[n - 2].num();
                self.stack[n - 2] = Value::Num(if a $op b { Fx::ONE } else { Fx::ZERO });
                self.stack.truncate(n - 1);
            }};
        }
        /// Replace the top of the stack in place — the `pop; …; push` shape
        /// with no net depth change (Gitea #312). Same reasoning as
        /// `binnum!`: no length write, no `MAX_STACK` check.
        macro_rules! replace_top {
            (|$v:ident| $new:expr) => {{
                let n = self.stack.len();
                if n == 0 {
                    fail!(ERR_STACK_UNDERFLOW);
                }
                let $v = self.stack[n - 1];
                self.stack[n - 1] = $new;
            }};
        }
        /// Read the top of the stack without removing it, for the arms that
        /// leave it there (`StoreL`, `StoreG`).
        macro_rules! peek_top {
            () => {{
                let n = self.stack.len();
                if n == 0 {
                    fail!(ERR_STACK_UNDERFLOW);
                }
                self.stack[n - 1]
            }};
        }
        // One builtin call: the hot-builtin fast path stays INSIDE the loop
        // (that is the whole point of `builtin_fast` — Gitea #260), the rest
        // goes out of line. A macro, not a method, because the fused
        // constant-argument forms (Gitea #261) need the same fast path and
        // calling through a function costs ~15 % of interpreter throughput.
        macro_rules! call_builtin {
            ($b:expr, $argc:expr) => {{
                let b: u16 = $b;
                let argc: usize = $argc;
                let mut fast = None;
                if argc <= FAST_ARGS {
                    if let BKind::Impl(bi) = BUILTINS[b as usize].kind {
                        let len = self.stack.len();
                        if len >= argc {
                            // Read the (at most four) arguments straight out
                            // of the stack into an array passed BY VALUE.
                            // The old `&a[..argc]` slice took the array's
                            // address, which pinned it to the frame and made
                            // the fill a ROM `memset` + `memcpy` pair on
                            // every builtin call; by value it stays in
                            // registers (Gitea #312).
                            let s = &self.stack[len - argc..];
                            let a: [Value; FAST_ARGS] = [
                                s.first().copied().unwrap_or(Value::Num(Fx::ZERO)),
                                s.get(1).copied().unwrap_or(Value::Num(Fx::ZERO)),
                                s.get(2).copied().unwrap_or(Value::Num(Fx::ZERO)),
                                s.get(3).copied().unwrap_or(Value::Num(Fx::ZERO)),
                            ];
                            if let Some(v) = self.builtin_fast(bi, a, argc) {
                                self.stack.truncate(len - argc);
                                fast = Some(v);
                            }
                        }
                    }
                }
                match fast {
                    Some(v) => push!(v),
                    None => {
                        // `call_builtin` can re-enter the VM through an array
                        // callback, and attributes its own errors from
                        // `insn_start`: publish both, then take the fuel back.
                        self.insn_start = insn_at;
                        self.fuel = fuel;
                        let r = self.call_builtin_slow(prog, b, argc);
                        fuel = self.fuel;
                        match r {
                            Ok(v) => push!(v),
                            Err(e) => return Err(e),
                        }
                    }
                }
            }};
        }

        use crate::bytecode::{enc, op};
        // Two-level loop: the outer level (re)loads the frame context —
        // function, code slice, locals base, pc — and the inner level
        // dispatches instructions against those locals. The frame's `pc`
        // field is written back only where something else can observe it:
        // before a call (so the return lands after it) and at a debug stop.
        // Re-deriving `prog.fns[..]`, the code slice and the frame pc on
        // every instruction was a sizeable share of dispatch on Xtensa
        // (Gitea #260).
        'frame: loop {
            let (fi, pc, lbase) = {
                let f = self.frames.last().expect("frame");
                (f.fn_idx, f.pc, f.locals_base as usize)
            };
            #[cfg(not(feature = "profile"))]
            let _ = fi; // only the profiler still needs it per instruction
            let fdef = &prog.fns[fi as usize];
            let code: &[u32] =
                &prog.words[fdef.code_start as usize..(fdef.code_start + fdef.code_len) as usize];
            let mut at = pc as usize;
            loop {
                // ONE cold, out-of-line call (Gitea #312). Inlined, this
                // dragged `debug_stop` AND the `pos_at` binary search it
                // calls — some 300 bytes — in between the loop head and the
                // fetch, so the not-taken `debug` test was a taken branch on
                // every dispatch and the register pressure spilled `debug`
                // itself to the frame. A paused-at-every-instruction session
                // is not a throughput path; the fast path is.
                if debug && self.debug_step(prog, at as u32) {
                    self.fuel = fuel;
                    return Ok(Outcome::Paused);
                }
                insn_at = at as u32;
                // One word per instruction: opcode in the low byte, operand
                // field above it (see bytecode::enc). The decoder validated
                // every operand and jump target, so the fallbacks here are
                // unreachable; they exist so a logic bug degrades to a
                // runtime error instead of a panic.
                // `at` advances unconditionally so the two arms MERGE on a
                // value, not on control flow: the `match … { Some => {at+=1;
                // w} None => … }` shape cost an extra `j` and a `mov` per
                // dispatch on Xtensa (Gitea #312). Overshooting `at` in the
                // fell-off-the-end case is harmless — `RetNull` returns.
                let w = if at < code.len() {
                    code[at]
                } else {
                    enc::bare(op::RET_NULL)
                };
                at += 1;
                let opcode = enc::opcode(w);
                #[cfg(feature = "profile")]
                self.prof_record(fi, insn_at, opcode);
                if fuel == 0 {
                    fail!(ERR_EXEC_LIMIT);
                }
                fuel -= 1;
                match opcode {
                    // Opcode 0 is not emitted by any compiler, but naming it
                    // makes the match's range start at 0 and saves the
                    // `addi -1` that rebases the jump table (Gitea #312).
                    0 => fail!("unknown opcode (corrupt bytecode?)"),
                    op::CONST_NUM => {
                        // the immediate is the next word
                        let raw = code.get(at).copied().unwrap_or(0);
                        at += 1;
                        push!(Value::Num(Fx::from_raw(raw as i32)))
                    }
                    op::CONST_FUN => {
                        push!(Value::Fun(enc::imm16(w) as u32))
                    }
                    op::CONST_BUILTIN => {
                        push!(Value::Builtin(enc::imm16(w) as u32))
                    }
                    op::LOAD_G => {
                        push!(self.globals[enc::imm16(w) as usize])
                    }
                    op::STORE_G => {
                        // Assignment is an expression: the value stays on the
                        // stack, so this is a peek, not pop+push (Gitea #312).
                        self.globals[enc::imm16(w) as usize] = peek_top!();
                    }
                    op::LOAD_L => {
                        push!(self.locals[lbase + enc::imm8(w) as usize])
                    }
                    op::STORE_L => {
                        self.locals[lbase + enc::imm8(w) as usize] = peek_top!();
                    }
                    // Index semantics oracle-confirmed on fw 3.67: fractional
                    // indices truncate (reads and writes alike, literal and
                    // variable index — stock patterns like sparks depend on
                    // it), and the bounds check runs on the TRUNCATED index, so
                    // `a[3.5]` on a 3-slot array is out of range. Anything out
                    // of range (negative or ≥ length) is a runtime error that
                    // aborts execution, leaving the array untouched: PB does
                    // not clamp, wrap or silently no-op (Gitea #107, re-probed
                    // 2026-08-29 with tools/oracle/oob-probes.mjs — that probe
                    // also retired the old "PB aborts on a fractional *literal*
                    // index write" note, which does not reproduce).
                    op::LOAD_IDX => {
                        let idx = pop!().num();
                        let arr = pop!();
                        match self.index_read(prog, arr, idx) {
                            Ok(v) => push!(v),
                            Err(m) => fail!(m),
                        }
                    }
                    op::STORE_IDX => {
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
                        match self.arr_mut(prog, a) {
                            Ok(v) => match v.get_mut(i) {
                                Some(slot) => *slot = val,
                                None => fail!("array index out of bounds"),
                            },
                            Err(m) => fail!(m),
                        }
                        push!(val);
                    }
                    op::ARR_LEN => {
                        let arr = pop!();
                        let Value::Arr(a) = arr else {
                            fail!(".length of a non-array value")
                        };
                        push!(Value::Num(Fx::from_int(self.arr(prog, a).len() as i32)));
                    }
                    op::NEW_ARRAY => {
                        let n = enc::imm16(w) as usize;
                        // budget-first: the elements are popped into the slot
                        // only once the (fallible) allocation succeeded
                        match self.alloc_array_zeroed(n) {
                            Ok(v) => {
                                let Value::Arr(id) = v else { unreachable!() };
                                for i in (0..n).rev() {
                                    let e = pop!();
                                    // freshly allocated ⇒ always Owned
                                    if let ArrRepr::Owned(vs) = &mut self.arrays[id as usize] {
                                        vs[i] = e;
                                    }
                                }
                                push!(v);
                            }
                            Err(m) => fail!(m),
                        }
                    }
                    op::CONST_ARR => {
                        let d = enc::imm16(w) as u32;
                        // decoder-validated: d < pool.len()
                        let len = prog.pool[d as usize].len as usize;
                        match self.alloc_const_array(d, len) {
                            Ok(v) => push!(v),
                            Err(m) => fail!(m),
                        }
                    }
                    op::ASSERT => {
                        let m = enc::imm16(w);
                        if !pop!().truthy() {
                            self.insn_start = insn_at;
                            self.fuel = fuel;
                            return Err(self.assert_failed(prog, m));
                        }
                    }
                    op::DUP => {
                        let v = *self.stack.last().unwrap_or(&Value::default());
                        push!(v);
                    }
                    op::DUP2 => {
                        let n = self.stack.len();
                        if n < 2 {
                            fail!(ERR_STACK_UNDERFLOW);
                        }
                        let a = self.stack[n - 2];
                        let b = self.stack[n - 1];
                        push!(a);
                        push!(b);
                    }
                    op::POP => {
                        pop!();
                    }
                    op::ADD => {
                        binnum!(+)
                    }
                    op::SUB => {
                        binnum!(-)
                    }
                    op::MUL => {
                        binnum!(*)
                    }
                    op::DIV => {
                        binnum!(/)
                    }
                    op::REM => {
                        binnum!(%)
                    }
                    op::POW => {
                        let b = pop!().num();
                        let a = pop!().num();
                        push!(Value::Num(fmath::pow(a, b)));
                    }
                    op::NEG => {
                        let v = pop!().num();
                        push!(Value::Num(-v));
                    }
                    op::NOT => {
                        let v = pop!();
                        push!(Value::Num(if v.truthy() { Fx::ZERO } else { Fx::ONE }));
                    }
                    op::BIT_NOT => {
                        let v = pop!().num();
                        push!(Value::Num(!v));
                    }
                    op::BIT_AND => {
                        binnum!(&)
                    }
                    op::BIT_OR => {
                        binnum!(|)
                    }
                    op::BIT_XOR => {
                        binnum!(^)
                    }
                    op::SHL => {
                        binnum!(<<)
                    }
                    op::SHR => {
                        binnum!(>>)
                    }
                    op::LT => {
                        bincmp!(<)
                    }
                    op::LE => {
                        bincmp!(<=)
                    }
                    op::GT => {
                        bincmp!(>)
                    }
                    op::GE => {
                        bincmp!(>=)
                    }
                    op::EQ => {
                        let b = pop!();
                        let a = pop!();
                        push!(Value::Num(if value_eq(a, b) { Fx::ONE } else { Fx::ZERO }));
                    }
                    op::NE => {
                        let b = pop!();
                        let a = pop!();
                        push!(Value::Num(if value_eq(a, b) { Fx::ZERO } else { Fx::ONE }));
                    }
                    op::JMP => {
                        at = enc::imm24(w) as usize;
                    }
                    op::JMP_IF_FALSE => {
                        if !pop!().truthy() {
                            at = enc::imm24(w) as usize;
                        }
                    }
                    op::JMP_IF_TRUE_PEEK => {
                        let v = *self.stack.last().unwrap_or(&Value::default());
                        if v.truthy() {
                            at = enc::imm24(w) as usize;
                        }
                    }
                    op::JMP_IF_FALSE_PEEK => {
                        let v = *self.stack.last().unwrap_or(&Value::default());
                        if !v.truthy() {
                            at = enc::imm24(w) as usize;
                        }
                    }
                    op::CALL_FN => {
                        let f = enc::imm16(w);
                        let argc = enc::argc(w);
                        // the return lands after this instruction
                        self.frames.last_mut().expect("frame").pc = at as u32;
                        // `push_frame` attributes "call depth exceeded" from
                        // `insn_start`, and cannot see the local copies
                        self.insn_start = insn_at;
                        self.fuel = fuel;
                        self.enter_call(prog, f, argc as usize)?;
                        continue 'frame;
                    }
                    op::CALL_BUILTIN => {
                        let b = enc::imm16(w);
                        let argc = enc::argc(w) as usize;
                        #[cfg(feature = "profile")]
                        self.prof_builtin(b);
                        call_builtin!(b, argc);
                    }
                    op::CALL_VALUE => {
                        let argc = enc::imm8(w) as usize;
                        let n = self.stack.len();
                        if n < argc + 1 {
                            fail!(ERR_STACK_UNDERFLOW);
                        }
                        let callee = self.stack.remove(n - argc - 1);
                        match callee {
                            Value::Fun(f) => {
                                self.frames.last_mut().expect("frame").pc = at as u32;
                                self.insn_start = insn_at;
                                self.fuel = fuel;
                                self.enter_call(prog, f as u16, argc)?;
                                continue 'frame;
                            }
                            Value::Builtin(b) => {
                                #[cfg(feature = "profile")]
                                self.prof_builtin(b as u16);
                                self.insn_start = insn_at;
                                self.fuel = fuel;
                                let r = self.call_builtin(prog, b as u16, argc);
                                fuel = self.fuel;
                                match r {
                                    Ok(v) => push!(v),
                                    Err(mut e) => {
                                        if e.pc == u32::MAX {
                                            e = self.err_at(prog, core::mem::take(&mut e.message));
                                        }
                                        return Err(e);
                                    }
                                }
                            }
                            _ => fail!("call of a non-function value"),
                        }
                    }
                    op::RET | op::RET_NULL => {
                        let v = if opcode == op::RET {
                            pop!()
                        } else {
                            Value::default()
                        };
                        self.pop_frame();
                        if self.frames.len() == base {
                            self.fuel = fuel;
                            return Ok(Outcome::Done(v));
                        }
                        push!(v);
                        continue 'frame;
                    }
                    // ---- superinstructions (Gitea #261) ----
                    //
                    // Each arm is EXACTLY the base sequence named in
                    // bytecode::op, executed in the same order with the
                    // same error messages; the compiler's peephole only
                    // ever emits one where the base sequence was
                    // statically adjacent, inside one statement, with no
                    // branch landing in the middle. The only difference
                    // is that the whole thing costs one dispatch and one
                    // unit of fuel.
                    //
                    // Stack-limit note: the base sequence's intermediate
                    // pushes are elided, so the peak depth is one or two
                    // slots lower. The arms below re-check MAX_STACK
                    // against that same peak, so a pattern that would
                    // have overflowed still overflows, with the same
                    // message.
                    op::STORE_L_POP => {
                        // StoreL n; Pop
                        let v = pop!();
                        self.locals[lbase + enc::imm8(w) as usize] = v;
                    }
                    op::STORE_G_POP => {
                        // StoreG n; Pop
                        let v = pop!();
                        self.globals[enc::imm16(w) as usize] = v;
                    }
                    op::LOAD_LL => {
                        // LoadL a; LoadL b
                        if self.stack.len() + 1 >= MAX_STACK {
                            fail!(ERR_STACK_OVERFLOW);
                        }
                        self.stack.push(self.locals[lbase + enc::imm8(w) as usize]);
                        self.stack.push(self.locals[lbase + enc::imm8b(w) as usize]);
                    }
                    op::LOAD_LG => {
                        // LoadL a; LoadG g
                        if self.stack.len() + 1 >= MAX_STACK {
                            fail!(ERR_STACK_OVERFLOW);
                        }
                        self.stack.push(self.locals[lbase + enc::imm8(w) as usize]);
                        self.stack.push(self.globals[enc::imm16hi(w) as usize]);
                    }
                    op::LOAD_GL => {
                        // LoadG g; LoadL a
                        if self.stack.len() + 1 >= MAX_STACK {
                            fail!(ERR_STACK_OVERFLOW);
                        }
                        self.stack.push(self.globals[enc::imm16(w) as usize]);
                        self.stack.push(self.locals[lbase + enc::argc(w) as usize]);
                    }
                    op::LOAD_L_IDX => {
                        // LoadL a; LoadIdx — the elided LoadL push still
                        // has to hit the same stack limit
                        if self.stack.len() >= MAX_STACK {
                            fail!(ERR_STACK_OVERFLOW);
                        }
                        let idx = self.locals[lbase + enc::imm8(w) as usize].num();
                        let arr = pop!();
                        match self.index_read(prog, arr, idx) {
                            Ok(v) => push!(v),
                            Err(m) => fail!(m),
                        }
                    }
                    op::LOAD_G_L_IDX => {
                        // LoadG g; LoadL a; LoadIdx
                        if self.stack.len() + 1 >= MAX_STACK {
                            fail!(ERR_STACK_OVERFLOW);
                        }
                        let arr = self.globals[enc::imm16(w) as usize];
                        let idx = self.locals[lbase + enc::argc(w) as usize].num();
                        match self.index_read(prog, arr, idx) {
                            Ok(v) => push!(v),
                            Err(m) => fail!(m),
                        }
                    }
                    op::CALL_BUILTIN_C | op::CALL_BUILTIN_CC => {
                        // Const c[, Const c2]; CallBuiltin b, argc — the
                        // base sequence's Const pushes, elided into the
                        // instruction's trailing immediate words
                        let nconst = if opcode == op::CALL_BUILTIN_C { 1 } else { 2 };
                        if self.stack.len() + nconst > MAX_STACK {
                            fail!(ERR_STACK_OVERFLOW);
                        }
                        for _ in 0..nconst {
                            let c = Fx::from_raw(code.get(at).copied().unwrap_or(0) as i32);
                            at += 1;
                            self.stack.push(Value::Num(c));
                        }
                        let b = enc::imm16(w);
                        let argc = enc::argc(w) as usize;
                        #[cfg(feature = "profile")]
                        self.prof_builtin(b);
                        call_builtin!(b, argc);
                    }
                    op::CONST_OP => {
                        // Const c; <binop> — net stack change 0, so it is a
                        // top replacement, not pop+push (Gitea #312).
                        let c = Fx::from_raw(code.get(at).copied().unwrap_or(0) as i32);
                        at += 1;
                        replace_top!(|a| Value::Num(binop_const(enc::imm8(w), a, c)));
                    }
                    op::LOAD_L_CONST_OP => {
                        // LoadL a; Const c; <binop>
                        if self.stack.len() + 1 >= MAX_STACK {
                            fail!(ERR_STACK_OVERFLOW);
                        }
                        let c = Fx::from_raw(code.get(at).copied().unwrap_or(0) as i32);
                        at += 1;
                        let a = self.locals[lbase + enc::imm8(w) as usize];
                        push!(Value::Num(binop_const(enc::imm8b(w), a, c)));
                    }
                    op::LOAD_G_CONST_OP => {
                        // LoadG g; Const c; <binop>
                        if self.stack.len() + 1 >= MAX_STACK {
                            fail!(ERR_STACK_OVERFLOW);
                        }
                        let c = Fx::from_raw(code.get(at).copied().unwrap_or(0) as i32);
                        at += 1;
                        let a = self.globals[enc::imm16(w) as usize];
                        push!(Value::Num(binop_const(enc::argc(w), a, c)));
                    }
                    op::CMP_JF => {
                        // <cmp>; JmpIfFalse t  (target in the next word)
                        let t = code.get(at).copied().unwrap_or(0) as usize;
                        at += 1;
                        let b = pop!();
                        let a = pop!();
                        if !binop(enc::imm8(w), a, b).truthy() {
                            at = t;
                        }
                    }
                    op::POP_RET_NULL => {
                        // Pop; RetNull
                        pop!();
                        self.pop_frame();
                        if self.frames.len() == base {
                            self.fuel = fuel;
                            return Ok(Outcome::Done(Value::default()));
                        }
                        push!(Value::default());
                        continue 'frame;
                    }
                    _ => fail!("unknown opcode (corrupt bytecode?)"),
                }
            }
        }
    }

    /// Real arena cost of an array: elements plus Vec header + allocator
    /// overhead (what many tiny nested [r,g,b] arrays actually pay).
    fn array_cost(len: usize) -> usize {
        len * core::mem::size_of::<Value>() + 32
    }

    fn charge_array(&mut self, len: usize, bytes: usize) -> Result<(), &'static str> {
        if self.array_elems + len + ARRAY_HEADER_UNITS > self.array_budget {
            return Err("array element budget exceeded (arrays are never freed)");
        }
        // The slot vector lives on the ordinary allocator whatever the arena
        // does; see MAX_ARENA_SLOTS.
        if self.arrays.len() >= MAX_ARENA_SLOTS {
            return Err("array element budget exceeded (arrays are never freed)");
        }
        self.charge_array_bytes(bytes)
    }

    /// The byte half of [`Vm::charge_array`], for bytes added to an arena
    /// entry whose elements are already on the element ledger — i.e. the
    /// const→owned copy-on-write promotion in [`Vm::arr_mut`] (Gitea #132).
    /// Re-checking the element budget there would demand a spurious extra
    /// header's worth of headroom for an entry that allocates no new slot.
    fn charge_array_bytes(&mut self, bytes: usize) -> Result<(), &'static str> {
        if self.array_bytes + bytes > self.array_byte_budget {
            return Err("array memory budget exceeded (pattern too large for this device)");
        }
        Ok(())
    }

    pub fn alloc_array(&mut self, elems: ArrVec<Value>) -> Result<Value, &'static str> {
        self.charge_array(elems.len(), Self::array_cost(elems.len()))?;
        self.array_elems += elems.len() + ARRAY_HEADER_UNITS;
        self.array_bytes += Self::array_cost(elems.len());
        self.arrays.push(ArrRepr::Owned(elems));
        Ok(Value::Arr((self.arrays.len() - 1) as u32))
    }

    /// Arena entry sharing a const-pool array (copy-on-write). Elements
    /// still count against the PB-compat element budget; bytes only for
    /// the entry itself — the data is shared with the program.
    fn alloc_const_array(&mut self, d: u32, len: usize) -> Result<Value, &'static str> {
        self.charge_array(len, CONST_ENTRY_COST)?;
        self.array_elems += len + ARRAY_HEADER_UNITS;
        self.array_bytes += CONST_ENTRY_COST;
        self.arrays.push(ArrRepr::Const(d));
        Ok(Value::Arr((self.arrays.len() - 1) as u32))
    }

    /// Budget-checked zero-filled array allocation: the budgets are verified
    /// BEFORE any memory is reserved, and the reservation itself is
    /// fallible — on a small-heap device a huge `array(n)` must be a
    /// recorded runtime error, never an allocator panic (= reboot).
    fn alloc_array_zeroed(&mut self, len: usize) -> Result<Value, &'static str> {
        self.charge_array(len, Self::array_cost(len))?;
        let mut elems: ArrVec<Value> = crate::arena::empty();
        if elems.try_reserve_exact(len).is_err() {
            return Err("out of memory for array");
        }
        elems.resize(len, Value::default());
        self.array_elems += len + ARRAY_HEADER_UNITS;
        self.array_bytes += Self::array_cost(len);
        self.arrays.push(ArrRepr::Owned(elems));
        Ok(Value::Arr((self.arrays.len() - 1) as u32))
    }

    /// `random()`'s generator: **splitmix64**, low 32 bits of each output.
    /// Pinned (docs/lang.md "Determinism and seeding", sequence asserted by
    /// `semantics::random_seed_pins_the_documented_sequence`) so that
    /// `randomSeed(s)` gives the identical stream on every Luxel build —
    /// firmware, playground WASM, CLI. Counter-based, so a low-entropy
    /// seed is fine: the finalizer decorrelates adjacent states.
    fn next_random(&mut self) -> u32 {
        self.rng = self.rng.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.rng;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        (z ^ (z >> 31)) as u32
    }

    /// `prng()`'s generator: **xorshift32** (Marsaglia 13/17/5), state
    /// returned whole. Pinned by test like `next_random`; the state is
    /// 32 bits, so `prngSeed`'s return value round-trips exactly.
    /// Diverges from PB by design — PB's generator is an unidentified
    /// float-based state machine (docs/lang.md "Known divergences").
    fn next_prng(&mut self) -> u32 {
        let mut x = self.prng_state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.prng_state = x;
        x
    }

    /// Scale a 32-bit draw into `[0, max)` by multiply-and-keep-high-word,
    /// with max's RAW 16.16 word taken UNSIGNED and the result
    /// reinterpreted signed — PB-exact, oracle-verified (fw 3.67,
    /// 2026-08-23). For max > 0 this is plain uniform `[0, max)`. A
    /// NEGATIVE max (e.g. `random(0xffff)` — 0xffff wraps to -1.0 in
    /// 16.16, on PB too) becomes a huge unsigned word, so draws span the
    /// whole signed range: measured [-32764, 32766] on the oracle. Corpus
    /// patterns lean on that for full-width PRNG seeds; clamping the max
    /// to 0 here made them collapse to a constant 0 (Gitea #105).
    fn scale_random(r: u32, max: Fx) -> Value {
        let m = max.raw() as u32 as u64;
        Value::Num(Fx::from_raw(((r as u64 * m) >> 32) as u32 as i32))
    }

    /// The hot, infallible builtins — everything a per-pixel `render` calls
    /// in a typical pattern. The dispatch loop calls this straight from the
    /// stack (no 16-slot args array, no `Result`, no jump into the 20 KB
    /// [`Vm::call_builtin`]); `call_builtin` delegates here first so the
    /// semantics live in exactly one place. `None` = not a fast builtin.
    #[inline(always)]
    fn builtin_fast(
        &mut self,
        builtin: Builtin,
        args: [Value; FAST_ARGS],
        argc: usize,
    ) -> Option<Value> {
        // Same rule as the old `args: &[Value]` of length `argc`: an index
        // at or past the argument count reads as 0.
        let n = |i: usize| {
            if i < argc {
                args[i].num()
            } else {
                Fx::ZERO
            }
        };
        use Builtin::*;
        match builtin {
            Abs => Some(Value::Num(n(0).abs())),
            Floor => Some(Value::Num(n(0).floor())),
            Ceil => Some(Value::Num(n(0).ceil())),
            Round => Some(Value::Num(n(0).round())),
            Trunc => Some(Value::Num(n(0).trunc())),
            Frac => Some(Value::Num(n(0).frac())),
            Clamp => Some(Value::Num(n(0).clamp(n(1), n(2)))),
            Min => Some(Value::Num(n(0).min(n(1)))),
            Max => Some(Value::Num(n(0).max(n(1)))),
            Mod => Some(Value::Num(n(0).mod_floor(n(1)))),
            Sqrt => Some(Value::Num(fmath::sqrt(n(0)))),
            Sin => Some(Value::Num(fmath::sin(n(0)))),
            Cos => Some(Value::Num(fmath::cos(n(0)))),
            Random => {
                let r = self.next_random();
                Some(Self::scale_random(r, n(0)))
            }
            Prng => {
                let r = self.next_prng();
                Some(Self::scale_random(r, n(0)))
            }
            Time => {
                // period in ms happens to equal the interval's raw value:
                // 65.536 s · interval = 65536 ms · interval.
                let period = n(0).raw().max(0) as u32;
                if period == 0 {
                    return Some(Value::Num(Fx::ZERO));
                }
                // The clock stays below 2^32 ms for 49 days, so every
                // realistic call takes the all-32-bit path; the u64 form
                // (two ROM calls, `__umoddi3` + `__udivdi3`) is out of line
                // behind `time_phase_u64`. `time()` is called per pixel by
                // most patterns.
                let v = if self.time_ms <= u32::MAX as u64 {
                    time_phase32(self.time_ms as u32, period)
                } else {
                    time_phase_u64(self.time_ms, period)
                };
                Some(Value::Num(Fx::from_raw(v as i32)))
            }
            Wave => Some(Value::Num(Fx::from_raw(
                (fmath::sin_turns(n(0)).raw() + Fx::ONE.raw()) >> 1,
            ))),
            Square => {
                // raw 1<<15 == Fx::from_f64(0.5), spelled so no softfloat
                // conversion can survive into the image
                let duty = if argc >= 2 { n(1) } else { Fx::from_raw(1 << 15) };
                let t = n(0).wrap_unit();
                Some(Value::Num(if t < duty { Fx::ONE } else { Fx::ZERO }))
            }
            Triangle => {
                let t = n(0).wrap_unit();
                let half = Fx::from_raw(1 << 15);
                Some(Value::Num(if t < half {
                    t + t
                } else {
                    (Fx::ONE - t) + (Fx::ONE - t)
                }))
            }
            Mix => Some(Value::Num(n(0) + (n(1) - n(0)) * n(2))),
            Hsv => {
                self.pixel = hsv_to_rgb(n(0), n(1), n(2));
                self.pixel_written = true;
                Some(Value::default())
            }
            Rgb => {
                self.pixel = [
                    n(0).clamp(Fx::ZERO, Fx::ONE),
                    n(1).clamp(Fx::ZERO, Fx::ONE),
                    n(2).clamp(Fx::ZERO, Fx::ONE),
                ];
                self.pixel_written = true;
                Some(Value::default())
            }
            _ => None,
        }
    }

    /// The out-of-line builtin entry: resolve the id, pop the arguments
    /// once, then walk the three-tier ladder — `builtin_fast` (in-loop
    /// arms, inlined here so the semantics live in one place),
    /// [`Vm::builtin_hot`] (per-pixel arms), [`Vm::builtin_cold`]
    /// (everything else).
    ///
    /// `iram-builtins` (Gitea #312/#328): `.rwtext` placement for this
    /// function and `builtin_hot` — the two tiers a per-pixel `render`
    /// actually executes. `builtin_cold` deliberately stays in flash.
    #[cfg_attr(feature = "iram-builtins", link_section = ".rwtext")]
    #[cfg_attr(feature = "iram-builtins", inline(never))]
    fn call_builtin(&mut self, prog: &Program, id: u16, argc: usize) -> Result<Value, VmError> {
        let no_site = |message: String| VmError {
            message,
            fn_idx: u16::MAX,
            pc: u32::MAX,
            line: 0,
            col: 0,
            is_assert: false,
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
        let argc = self.pop_args_into(&mut args, argc);
        // `builtin_fast` reads at most the first four arguments and treats
        // an index at or past `argc` as 0 — the same contract the old
        // `&args[..argc]` slice had, so a call with more than FAST_ARGS
        // arguments still resolves here rather than falling through to the
        // `unreachable!` arm at the bottom of `builtin_cold`.
        let fast: [Value; FAST_ARGS] = [args[0], args[1], args[2], args[3]];
        if let Some(v) = self.builtin_fast(builtin, fast, argc) {
            return Ok(v);
        }
        self.builtin_hot(prog, id, builtin, &args, argc)
    }

    /// Tier 2 of the builtin ladder (Gitea #328): the ~30 builtins a real
    /// pattern calls PER PIXEL that `builtin_fast` cannot answer in the
    /// dispatch loop — the transcendentals, the noise family, `dist`/`hypot`,
    /// `paint`, `canvasGet`, `pixelState`. Splitting them out of the ~19 KB
    /// arm soup makes the per-pixel native footprint ~2 KB instead of ~19 KB;
    /// [`Vm::builtin_cold`] holds the other 90 arms and is reached only
    /// through this function's `_` arm.
    ///
    /// The tiering is measured, not guessed: `tools/profile-library.mjs` over
    /// all 299 `library/` patterns puts a 40x gap between the least-used arm
    /// here (>= 0.6 calls/px in the pattern that uses it) and the most-used
    /// arm in `builtin_cold` (<= 0.016 calls/px). Tier 1 + tier 2 answer
    /// 99.4 % of every builtin call the library makes.
    #[cfg_attr(feature = "iram-builtins", link_section = ".rwtext")]
    #[inline(never)]
    fn builtin_hot(
        &mut self,
        prog: &Program,
        id: u16,
        builtin: Builtin,
        args: &[Value; MAX_ARGS],
        argc: usize,
    ) -> Result<Value, VmError> {
        let no_site = |message: String| VmError {
            message,
            fn_idx: u16::MAX,
            pc: u32::MAX,
            line: 0,
            col: 0,
            is_assert: false,
        };
        let a = |i: usize| args.get(i).copied().unwrap_or_default();
        let n = |i: usize| a(i).num();
        use Builtin::*;
        let num = |v: Fx| Ok(Value::Num(v));
        match builtin {
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
            Oklch => {
                self.pixel = crate::color::oklch_to_rgb(n(0), n(1), n(2));
                self.pixel_written = true;
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
            Paint => {
                let v = paint_pos(n(0));
                let b = if argc >= 2 { n(1) } else { Fx::ONE };
                let rgb = self.palette_lookup(prog, v);
                let b = b.clamp(Fx::ZERO, Fx::ONE);
                self.pixel = [rgb[0] * b, rgb[1] * b, rgb[2] * b];
                self.pixel_written = true;
                Ok(Value::default())
            }
            // hash(x) / hash2(x, y): deterministic 0..1 from the raw bits —
            // stable per-pixel randomness (sparkle that doesn't reshuffle
            // every frame). Same input, same output, on every device.
            Hash => num(hash_unit(n(0).raw() as u32)),
            Hash2 => num(hash_unit(
                (n(0).raw() as u32).wrapping_add(hash32(n(1).raw() as u32)),
            )),
            // dot(x1,y1, x2,y2) / dot3(x1,y1,z1, x2,y2,z2)
            Dot => num(n(0) * n(2) + n(1) * n(3)),
            Dot3 => num(n(0) * n(3) + n(1) * n(4) + n(2) * n(5)),
            // simplex2(x, y, seed = 0) / simplex3(x, y, z, seed = 0):
            // simplex noise in ~[-1, 1] — smoother than perlin, no axis
            // artifacts. The lattice does not wrap (setPerlinWrap N/A).
            Simplex2 => num(crate::noise::simplex2(n(0), n(1), n(2))),
            Simplex3 => num(crate::noise::simplex3(n(0), n(1), n(2), n(3))),
            // canvasGet(buf, w, x, y): bilinear sample of the canvas at
            // normalized (x, y). Texel centers sit at (i + 0.5)/w — a read
            // at a cell's center returns exactly what canvasSet put there;
            // between centers it blends the 4 neighbors (edges clamp, so
            // out-of-range coordinates read the border). Free upscaling
            // for canvas patterns on larger maps.
            CanvasGet => {
                let Value::Arr(arr) = a(0) else {
                    return Err(no_site("canvasGet of a non-array".into()));
                };
                let w = n(1).to_int_trunc();
                if w < 1 {
                    return num(Fx::ZERO);
                }
                let w = w as usize;
                let data = self.arr(prog, arr);
                let h = data.len() / w;
                if h < 1 {
                    return num(Fx::ZERO);
                }
                let (c0, c1, tx) = sample_axis(n(2), w);
                let (r0, r1, ty) = sample_axis(n(3), h);
                let at = |r: usize, c: usize| data.at(r * w + c).num().raw() as i64;
                let lerp = |a: i64, b: i64, t: i64| a + (((b - a) * t) >> 16);
                let top = lerp(at(r0, c0), at(r0, c1), tx);
                let bot = lerp(at(r1, c0), at(r1, c1), tx);
                num(Fx::from_raw(lerp(top, bot, ty) as i32))
            }
            // ---- Luxel extensions, batch 8 ----
            // pixelState(index[, ch]): last frame's committed state for a
            // pixel. Reads never allocate — a pattern that only reads gets
            // 0 and pays nothing — and out-of-range indices/channels read
            // 0 so neighbour taps at the strip's ends need no clamping.
            PixelState => {
                let i = n(0).to_int_trunc();
                let ch = if argc >= 2 { n(1).to_int_trunc() } else { 0 };
                let v = match &self.pixel_state {
                    Some(s)
                        if i >= 0
                            && (i as usize) < s.n
                            && ch >= 0
                            && (ch as usize) < s.channels =>
                    {
                        s.front[ch as usize * s.n + i as usize]
                    }
                    _ => Fx::ZERO,
                };
                num(v)
            }
            _ => self.builtin_cold(prog, id, builtin, args, argc),
        }
    }

    /// Tier 3: every builtin no pattern calls per pixel — easings, beziers,
    /// the array family, transforms, palette/post-process setters, GPIO,
    /// clock, events, canvas writes, buffer ops. `#[cold]` and out of line
    /// so its ~17 KB never competes with `Vm::run` for the flash
    /// instruction cache (docs/firmware.md, "Code placement").
    #[cold]
    #[inline(never)]
    fn builtin_cold(
        &mut self,
        prog: &Program,
        id: u16,
        builtin: Builtin,
        args: &[Value; MAX_ARGS],
        argc: usize,
    ) -> Result<Value, VmError> {
        let def = &BUILTINS[id as usize];
        let no_site = |message: String| VmError {
            message,
            fn_idx: u16::MAX,
            pc: u32::MAX,
            line: 0,
            col: 0,
            is_assert: false,
        };
        let a = |i: usize| args.get(i).copied().unwrap_or_default();
        let n = |i: usize| a(i).num();
        use Builtin::*;
        let num = |v: Fx| Ok(Value::Num(v));
        match builtin {
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
            // 1 + c3·(t-1)³ + c1·(t-1)² with c1 = 1.70158 (the classic ~10%
            // overshoot constant), c3 = c1 + 1
            EaseOutBack => {
                let u = n(0) - Fx::ONE;
                let c1 = Fx::from_f64(1.70158);
                let c3 = c1 + Fx::ONE;
                num(Fx::ONE + c3 * u * u * u + c1 * u * u)
            }
            // 2^(-10t)·sin((10t - 0.75)·2π/3) + 1, endpoints pinned exactly
            EaseOutElastic => {
                let t = n(0);
                num(if t <= Fx::ZERO {
                    Fx::ZERO
                } else if t >= Fx::ONE {
                    Fx::ONE
                } else {
                    let ten_t = Fx::from_int(10) * t;
                    let decay = fmath::pow(Fx::from_int(2), -ten_t);
                    // sin's argument in turns: (10t - 0.75) / 3
                    let s = fmath::sin_turns((ten_t - Fx::from_f64(0.75)) / Fx::from_int(3));
                    decay * s + Fx::ONE
                })
            }
            // piecewise parabolas (see ease_out_bounce)
            EaseOutBounce => num(ease_out_bounce(n(0))),
            // --- batch 7: the rest of the standard thirty easings. Same
            // contract as the ones above: polynomial/analytic forms on t,
            // no clamping except where the reference pins the endpoints.
            // sine: 1 - cos(t·π/2) etc, in turns (π/2 rad = 1/4 turn)
            EaseInSine => num(Fx::ONE - fmath::cos_turns(n(0) / Fx::from_int(4))),
            EaseOutSine => num(fmath::sin_turns(n(0) / Fx::from_int(4))),
            EaseInOutSine => {
                let c = fmath::cos_turns(n(0) / Fx::from_int(2));
                num((Fx::ONE - c) / Fx::from_int(2))
            }
            EaseInQuart => {
                let t = n(0);
                num(t * t * t * t)
            }
            EaseOutQuart => {
                let u = Fx::ONE - n(0);
                num(Fx::ONE - u * u * u * u)
            }
            EaseInOutQuart => {
                let t = n(0);
                num(if t < Fx::from_raw(1 << 15) {
                    Fx::from_int(8) * t * t * t * t
                } else {
                    // 1 - (2 - 2t)⁴/2
                    let u = Fx::from_int(2) - t - t;
                    Fx::ONE - u * u * u * u / Fx::from_int(2)
                })
            }
            EaseInQuint => {
                let t = n(0);
                num(t * t * t * t * t)
            }
            EaseOutQuint => {
                let u = Fx::ONE - n(0);
                num(Fx::ONE - u * u * u * u * u)
            }
            EaseInOutQuint => {
                let t = n(0);
                num(if t < Fx::from_raw(1 << 15) {
                    Fx::from_int(16) * t * t * t * t * t
                } else {
                    let u = Fx::from_int(2) - t - t;
                    Fx::ONE - u * u * u * u * u / Fx::from_int(2)
                })
            }
            // exponential: 2^(10t-10) / 1 - 2^(-10t), endpoints pinned exactly
            // so the curve starts at 0 and ends at 1
            EaseInExpo => {
                let t = n(0);
                num(if t <= Fx::ZERO {
                    Fx::ZERO
                } else {
                    fmath::pow(Fx::from_int(2), Fx::from_int(10) * t - Fx::from_int(10))
                })
            }
            EaseOutExpo => {
                let t = n(0);
                num(if t >= Fx::ONE {
                    Fx::ONE
                } else {
                    Fx::ONE - fmath::pow(Fx::from_int(2), -(Fx::from_int(10) * t))
                })
            }
            EaseInOutExpo => {
                let t = n(0);
                num(if t <= Fx::ZERO {
                    Fx::ZERO
                } else if t >= Fx::ONE {
                    Fx::ONE
                } else if t < Fx::from_raw(1 << 15) {
                    fmath::pow(Fx::from_int(2), Fx::from_int(20) * t - Fx::from_int(10))
                        / Fx::from_int(2)
                } else {
                    let e = fmath::pow(Fx::from_int(2), Fx::from_int(10) - Fx::from_int(20) * t);
                    (Fx::from_int(2) - e) / Fx::from_int(2)
                })
            }
            // circular: the unit circle's quarter arcs
            EaseInCirc => {
                let t = n(0);
                num(Fx::ONE - fmath::sqrt(Fx::ONE - t * t))
            }
            EaseOutCirc => {
                let u = n(0) - Fx::ONE;
                num(fmath::sqrt(Fx::ONE - u * u))
            }
            EaseInOutCirc => {
                let t = n(0);
                num(if t < Fx::from_raw(1 << 15) {
                    let u = t + t;
                    (Fx::ONE - fmath::sqrt(Fx::ONE - u * u)) / Fx::from_int(2)
                } else {
                    let u = Fx::from_int(2) - t - t;
                    (fmath::sqrt(Fx::ONE - u * u) + Fx::ONE) / Fx::from_int(2)
                })
            }
            // back: c3·t³ - c1·t² (anticipates below 0 before pulling away),
            // with c1 = 1.70158 as in easeOutBack; the in-out form uses the
            // published c2 = c1·1.525
            EaseInBack => {
                let t = n(0);
                let c1 = Fx::from_f64(1.70158);
                let c3 = c1 + Fx::ONE;
                num(c3 * t * t * t - c1 * t * t)
            }
            EaseInOutBack => {
                let t = n(0);
                let c2 = Fx::from_f64(1.70158) * Fx::from_f64(1.525);
                num(if t < Fx::from_raw(1 << 15) {
                    let u = t + t;
                    u * u * ((c2 + Fx::ONE) * u - c2) / Fx::from_int(2)
                } else {
                    let u = t + t - Fx::from_int(2);
                    (u * u * ((c2 + Fx::ONE) * u + c2) + Fx::from_int(2)) / Fx::from_int(2)
                })
            }
            // elastic: the mirror/in-out partners of easeOutElastic, same
            // 2π/3 and 2π/4.5 periods (in turns: /3 and /4.5)
            EaseInElastic => {
                let t = n(0);
                num(if t <= Fx::ZERO {
                    Fx::ZERO
                } else if t >= Fx::ONE {
                    Fx::ONE
                } else {
                    let ten_t = Fx::from_int(10) * t;
                    let grow = fmath::pow(Fx::from_int(2), ten_t - Fx::from_int(10));
                    let s = fmath::sin_turns((ten_t - Fx::from_f64(10.75)) / Fx::from_int(3));
                    -(grow * s)
                })
            }
            EaseInOutElastic => {
                let t = n(0);
                num(if t <= Fx::ZERO {
                    Fx::ZERO
                } else if t >= Fx::ONE {
                    Fx::ONE
                } else {
                    let twenty_t = Fx::from_int(20) * t;
                    let s = fmath::sin_turns((twenty_t - Fx::from_f64(11.125)) / Fx::from_f64(4.5));
                    if t < Fx::from_raw(1 << 15) {
                        let grow = fmath::pow(Fx::from_int(2), twenty_t - Fx::from_int(10));
                        -(grow * s) / Fx::from_int(2)
                    } else {
                        let decay = fmath::pow(Fx::from_int(2), Fx::from_int(10) - twenty_t);
                        decay * s / Fx::from_int(2) + Fx::ONE
                    }
                })
            }
            // bounce: the standard reflections of ease_out_bounce
            EaseInBounce => num(Fx::ONE - ease_out_bounce(Fx::ONE - n(0))),
            EaseInOutBounce => {
                let t = n(0);
                num(if t < Fx::from_raw(1 << 15) {
                    (Fx::ONE - ease_out_bounce(Fx::ONE - t - t)) / Fx::from_int(2)
                } else {
                    (Fx::ONE + ease_out_bounce(t + t - Fx::ONE)) / Fx::from_int(2)
                })
            }
            PrngSeed => {
                let old = self.prng_state;
                let s = n(0).raw() as u32;
                self.prng_state = if s == 0 { 1 } else { s };
                num(Fx::from_raw(old as i32))
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
            // plot(x, y) or plot(x, y, z): map programs emit one coordinate
            // per pixel; the engine (map mode) reads plot_coord after the call.
            Plot => {
                self.plot_coord = [n(0), n(1), if argc >= 3 { n(2) } else { Fx::ZERO }];
                self.plot_dims = if argc >= 3 { 3 } else { 2 };
                self.plot_written = true;
                Ok(Value::default())
            }
            Oklab => {
                self.pixel = crate::color::oklab_to_rgb(n(0), n(1), n(2));
                self.pixel_written = true;
                Ok(Value::default())
            }
            Array => {
                let len = n(0).to_int_trunc().max(0) as usize;
                self.alloc_array_zeroed(len).map_err(|m| no_site(m.into()))
            }
            ArrayLength => {
                let Value::Arr(arr) = a(0) else {
                    return Err(no_site("arrayLength of a non-array".into()));
                };
                num(Fx::from_int(self.arr(prog, arr).len() as i32))
            }
            ArraySum => {
                let Value::Arr(arr) = a(0) else {
                    return Err(no_site("arraySum of a non-array".into()));
                };
                let mut sum = Fx::ZERO;
                for v in self.arr(prog, arr).iter() {
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
                while i < self.arr(prog, arr).len() {
                    let v = self.arr(prog, arr).at(i);
                    let r = self.dispatch_direct(
                        prog,
                        f,
                        &[v, Value::Num(Fx::from_int(i as i32)), a(0)],
                    )?;
                    if mutate {
                        if let Some(slot) = self
                            .arr_mut(prog, arr)
                            .map_err(|m| no_site(m.into()))?
                            .get_mut(i)
                        {
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
                while i < self.arr(prog, src).len() && i < self.arr(prog, dst).len() {
                    let v = self.arr(prog, src).at(i);
                    let r = self.dispatch_direct(
                        prog,
                        f,
                        &[v, Value::Num(Fx::from_int(i as i32)), a(0)],
                    )?;
                    if let Some(slot) = self
                        .arr_mut(prog, dst)
                        .map_err(|m| no_site(m.into()))?
                        .get_mut(i)
                    {
                        *slot = r;
                    }
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
                while i < self.arr(prog, arr).len() {
                    let v = self.arr(prog, arr).at(i);
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
                // Oracle #107 (fw 3.67, tools/oracle/oob-probes.mjs Q8):
                // this splat is bounds-checked as a WHOLE SPAN, unlike the
                // silent per-element drop this used to do. If
                // `offset + count` runs past the end it is an ordinary
                // runtime error and the array is left completely untouched —
                // not even the in-bounds prefix lands (Q8a/Q8b/Q8e/Q8f);
                // `offset + count == length` is the accepted boundary (Q8g).
                // A NEGATIVE offset does not error and does not clamp to
                // slot 0: the whole splat shifts down, so only the values
                // that land at a valid index are stored (Q8c/Q8d). PB
                // reaches that state by storing through a negative index —
                // memory-unsafe, and it hangs outright once several values
                // fall below zero — so we match the observable in-range half
                // and simply skip the rest.
                let (off, first) = if builtin == ArrayReplaceAt {
                    (n(1).to_int_trunc() as isize, 2)
                } else {
                    (0, 1)
                };
                // `get` rather than `args[first..argc]`: with fewer args than
                // the offset form's arity (`arrayReplaceAt(b)`) that range is
                // inverted and indexing panics — a VM panic on ordinary
                // pattern source, which on device is a reboot. PB drops the
                // call as a no-op instead (missing args are nothing to
                // splat), so an empty slice is the PB-shaped answer.
                let vals = args.get(first..argc).unwrap_or(&[]);
                {
                    let slots = self.arr_mut(prog, arr).map_err(|m| no_site(m.into()))?;
                    let count = vals.len() as isize;
                    if off.saturating_add(count) > slots.len() as isize {
                        return Err(no_site("array index out of bounds".into()));
                    }
                    for (j, arg) in vals.iter().enumerate() {
                        let i = off + j as isize;
                        if i >= 0 {
                            slots[i as usize] = *arg;
                        }
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
                self.arr_mut(prog, arr).map_err(|m| no_site(m.into()))?; // materialize (CoW)
                let ArrRepr::Owned(mut data) = core::mem::take(&mut self.arrays[arr as usize])
                else {
                    unreachable!("materialized above")
                };
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
                self.arrays[arr as usize] = ArrRepr::Owned(data);
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
                self.palette_src = Some(arr);
                self.palette_dirty = false;
                self.rebuild_palette(prog, arr);
                Ok(Value::default())
            }
            // ---- clock (host-provided wall time) ----
            ClockYear | ClockMonth | ClockDay | ClockHour | ClockMinute | ClockSecond
            | ClockWeekday => {
                let Some(unix) = self.wall_unix else {
                    return num(Fx::ZERO); // no-time untestable on the oracle; 0 is our choice
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
            // ---- GPIO: the engine keeps the pattern's view of every pin
            // (mode, written level, injected/read level) and never touches
            // a pad itself. A host syncs it with real silicon between frames
            // (the firmware, Gitea #177 item 4) or drives it by hand (the
            // playground / verify harness). Sequencer / sync stubs stay
            // silent no-ops. ----
            // pinMode records the mode byte (for the firmware) and the
            // pull-up bit (for `digitalRead`'s idle level, Gitea #177).
            PinMode => {
                let pin = n(0).to_int_trunc();
                if (0..=MAX_TRACKED_PIN).contains(&pin) {
                    let bit = 1u64 << pin;
                    let mode = n(1).to_int_trunc();
                    self.pin_used |= bit;
                    self.pin_mode[pin as usize] = mode.clamp(0, 255) as u8;
                    if mode & PIN_MODE_PULLUP != 0 {
                        self.pin_pullup |= bit;
                    } else {
                        self.pin_pullup &= !bit;
                    }
                }
                Ok(Value::default())
            }
            // digitalWrite(pin, level): remembered per pin for a host to copy
            // onto the pad. Any non-zero level is HIGH (PB: `HIGH` is 1).
            DigitalWrite => {
                let pin = n(0).to_int_trunc();
                if (0..=MAX_TRACKED_PIN).contains(&pin) {
                    let bit = 1u64 << pin;
                    self.pin_used |= bit;
                    if n(1) != Fx::ZERO {
                        self.pin_out_level |= bit;
                    } else {
                        self.pin_out_level &= !bit;
                    }
                }
                Ok(Value::default())
            }
            // An injected level wins (a host is standing in for the wire —
            // `Vm::set_pin`); with nothing driving it, the pin sits at
            // whatever its configured bias holds it at: HIGH under a pull-up,
            // LOW otherwise (INPUT, INPUT_PULLDOWN, outputs, unconfigured).
            DigitalRead => {
                let pin = n(0).to_int_trunc();
                if (0..=MAX_TRACKED_PIN).contains(&pin) {
                    self.pin_used |= 1u64 << pin;
                }
                let high = self.pin_read(pin);
                num(if high { Fx::ONE } else { Fx::ZERO })
            }
            PlaylistSetPosition | SequencerNext => Ok(Value::default()),
            // Analog inputs read whatever a host injected for the pin, and 0
            // while nothing has (Gitea #206) — the same shape as
            // `DigitalRead`, minus the pull-up idle level: an undriven
            // ADC/touch pad resting at 0 is a defensible reading.
            AnalogRead | TouchRead => {
                let pin = n(0).to_int_trunc();
                if (0..=MAX_TRACKED_PIN).contains(&pin) {
                    self.analog_used |= 1u64 << pin;
                }
                num(self.analog_read(pin))
            }
            SequencerGetMode | PlaylistGetPosition | PlaylistGetLength | NodeId => num(Fx::ZERO),
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
            // blur1D(arr, radius): in-place box blur, window 2·radius+1,
            // edges clamped; returns the array. radius < 1 is a no-op.
            Blur1D => {
                let Value::Arr(arr) = a(0) else {
                    return Err(no_site("blur1D of a non-array".into()));
                };
                let r = n(1).to_int_trunc().max(0) as usize;
                // materialize up front (copy-on-write) — this writes in place
                let data = self.arr_mut(prog, arr).map_err(|m| no_site(m.into()))?;
                // Sliding window, O(radius) scratch — see blur1d_inplace.
                // The prefix-sum version this replaced wanted 8 bytes per
                // ELEMENT, i.e. 32 KiB for a pixelCount-sized array on a
                // 64x64 panel: more than a loaded device has spare, so
                // blur1D simply failed there (Gitea #296; before that an
                // infallible Vec aborted the firmware outright — "memory
                // allocation of 32776 bytes failed", 2026-09-06, #295).
                if blur1d_inplace(data, r).is_err() {
                    return Err(no_site("out of memory for blur1D".into()));
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
                for slot in self
                    .arr_mut(prog, arr)
                    .map_err(|m| no_site(m.into()))?
                    .iter_mut()
                {
                    *slot = Value::Num(slot.num() * decay);
                }
                Ok(a(0))
            }
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
                self.write3(prog, a(3), rgb)
                    .map_err(|m| no_site(m.into()))?;
                Ok(a(3))
            }
            Rgb2Hsv => {
                let hsv = rgb_to_hsv(n(0), n(1), n(2));
                self.write3(prog, a(3), hsv)
                    .map_err(|m| no_site(m.into()))?;
                Ok(a(3))
            }
            // curl2(x, y, out, seed = 0) / curl3(x, y, z, out, seed = 0):
            // the curl of a simplex potential, written into out[0..2] /
            // out[0..3] and returned (in place, like arrayAdd/mixColors).
            // Divergence-free by construction, so advecting particles along
            // it gives swirling flow with no sources or sinks. Components
            // are noise DERIVATIVES: they run to about +/-6.4, not +/-1.
            Curl2 => {
                let (u, v) = crate::noise::curl2(n(0), n(1), n(3));
                self.write2(prog, a(2), [u, v])
                    .map_err(|m| no_site(format!("curl2: {m}")))?;
                Ok(a(2))
            }
            Curl3 => {
                let (u, v, w) = crate::noise::curl3(n(0), n(1), n(2), n(4));
                self.write3(prog, a(3), [u, v, w])
                    .map_err(|m| no_site(format!("curl3: {m}")))?;
                Ok(a(3))
            }
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
                self.write3(prog, a(7), c).map_err(|m| no_site(m.into()))?;
                Ok(a(7))
            }
            // ---- Luxel extensions, batch 4: external event injection ----
            // eventCount(): injected events waiting to be read.
            EventCount => num(Fx::from_int(self.events.len() as i32)),
            // readEvent(out): pop the oldest injected event into out[0..4]
            // = [type, x, y, value] and return 1; return 0 (out untouched)
            // when the queue is empty. Idiom: while (readEvent(ev)) { … }
            ReadEvent => {
                if self.events.is_empty() {
                    return num(Fx::ZERO);
                }
                let Value::Arr(arr) = a(0) else {
                    return Err(no_site("readEvent: `out` must be an array".into()));
                };
                let slots = self.arr_mut(prog, arr).map_err(|m| no_site(m.into()))?;
                if slots.len() < 4 {
                    return Err(no_site("readEvent: `out` array needs length >= 4".into()));
                }
                let ev = self.events.pop_front().unwrap_or_default();
                let slots = self.arr_mut(prog, arr).map_err(|m| no_site(m.into()))?;
                for (slot, v) in slots.iter_mut().zip(ev) {
                    *slot = Value::Num(v);
                }
                num(Fx::ONE)
            }
            // ---- Luxel extensions, batch 3: 2D canvases + bulk array math ----
            // blur2D(arr, w, h, radius): separable in-place box blur over
            // the first w×h elements (row-major), window 2·radius+1 per
            // axis, edges clamped like blur1D; returns the array. Any of
            // w/h/radius < 1 is a no-op.
            Blur2D => {
                let Value::Arr(arr) = a(0) else {
                    return Err(no_site("blur2D of a non-array".into()));
                };
                let (w, h, r) = (
                    n(1).to_int_trunc(),
                    n(2).to_int_trunc(),
                    n(3).to_int_trunc(),
                );
                if w >= 1 && h >= 1 && r >= 1 {
                    let (w, h, r) = (w as usize, h as usize, r as usize);
                    let data = self.arr_mut(prog, arr).map_err(|m| no_site(m.into()))?;
                    if data.len() < w * h {
                        return Err(no_site(format!(
                            "blur2D: array shorter than w\u{d7}h ({} < {})",
                            data.len(),
                            w * h
                        )));
                    }
                    // one reusable prefix-sum line (raw i64 — exact sums)
                    let mut pre: alloc::vec::Vec<i64> = alloc::vec::Vec::new();
                    if pre.try_reserve_exact(w.max(h) + 1).is_err() {
                        return Err(no_site("out of memory for blur2D".into()));
                    }
                    // horizontal pass, then vertical: a separable box blur
                    for row in 0..h {
                        let base = row * w;
                        pre.clear();
                        pre.push(0i64);
                        for i in 0..w {
                            pre.push(pre[i] + data[base + i].num().raw() as i64);
                        }
                        for i in 0..w {
                            let lo = i.saturating_sub(r);
                            let hi = (i + r).min(w - 1);
                            let avg = (pre[hi + 1] - pre[lo]) / (hi - lo + 1) as i64;
                            data[base + i] = Value::Num(Fx::from_raw(avg as i32));
                        }
                    }
                    for col in 0..w {
                        pre.clear();
                        pre.push(0i64);
                        for i in 0..h {
                            pre.push(pre[i] + data[i * w + col].num().raw() as i64);
                        }
                        for i in 0..h {
                            let lo = i.saturating_sub(r);
                            let hi = (i + r).min(h - 1);
                            let avg = (pre[hi + 1] - pre[lo]) / (hi - lo + 1) as i64;
                            data[i * w + col] = Value::Num(Fx::from_raw(avg as i32));
                        }
                    }
                }
                Ok(a(0))
            }
            // arrayAdd/arraySub(dst, src): element-wise dst ±= src over the
            // shorter of the two lengths; arrayMix(dst, src, t): dst +=
            // (src − dst)·t, unclamped like mix(). All in place, returning
            // dst — one VM call instead of an interpreted per-element loop.
            // (arrayScale(a, k) is the Feedback arm above under its
            // general-purpose alias.)
            ArrayAdd | ArraySub | ArrayMix => {
                let (Value::Arr(dst), Value::Arr(src)) = (a(0), a(1)) else {
                    return Err(no_site(format!("{} needs two arrays", def.name)));
                };
                let t = n(2);
                if dst == src {
                    // closed forms for the aliased call
                    for slot in self
                        .arr_mut(prog, dst)
                        .map_err(|m| no_site(m.into()))?
                        .iter_mut()
                    {
                        *slot = Value::Num(match builtin {
                            ArrayAdd => slot.num() + slot.num(),
                            ArraySub => Fx::ZERO,
                            _ => slot.num(), // mix(x, x, t) = x
                        });
                    }
                } else {
                    let (d, s) = self
                        .arr_pair(prog, dst, src)
                        .map_err(|m| no_site(m.into()))?;
                    for (dv, sv) in d.iter_mut().zip(s.iter()) {
                        let (x, y) = (dv.num(), sv.num());
                        *dv = Value::Num(match builtin {
                            ArrayAdd => x + y,
                            ArraySub => x - y,
                            _ => x + (y - x) * t,
                        });
                    }
                }
                Ok(a(0))
            }
            // canvasSet(buf, w, x, y, v): write v at the cell under
            // normalized (x, y) on a row-major w-wide canvas (h = len/w
            // rows). Coordinates clamp to the edges — no OOB frame-abort,
            // no `* 15.99` footgun (x = 1 lands in the last column).
            // Returns v.
            CanvasSet => {
                let Value::Arr(arr) = a(0) else {
                    return Err(no_site("canvasSet of a non-array".into()));
                };
                let w = n(1).to_int_trunc();
                let v = a(4);
                if w >= 1 {
                    let w = w as usize;
                    let (x, y) = (n(2), n(3));
                    let data = self.arr_mut(prog, arr).map_err(|m| no_site(m.into()))?;
                    let h = data.len() / w;
                    if h >= 1 {
                        data[cell_index(y, h) * w + cell_index(x, w)] = v;
                    }
                }
                Ok(v)
            }
            // ---- Luxel extensions, batch 5 ----
            // canvasAdd(buf, w, x, y, v): `cell += v` at the same
            // edge-clamped floor(x·w) cell canvasSet writes — particle
            // deposits without the manual read-modify-write. Returns the
            // cell's new value (like `+=` in JS); a degenerate canvas
            // (w < 1, or fewer than w elements) writes nothing and
            // returns v, exactly where canvasSet returns v.
            CanvasAdd => {
                let Value::Arr(arr) = a(0) else {
                    return Err(no_site("canvasAdd of a non-array".into()));
                };
                let w = n(1).to_int_trunc();
                let v = n(4);
                if w >= 1 {
                    let w = w as usize;
                    let (x, y) = (n(2), n(3));
                    let data = self.arr_mut(prog, arr).map_err(|m| no_site(m.into()))?;
                    let h = data.len() / w;
                    if h >= 1 {
                        let i = cell_index(y, h) * w + cell_index(x, w);
                        let sum = data[i].num() + v;
                        data[i] = Value::Num(sum);
                        return num(sum);
                    }
                }
                num(v)
            }
            // setPixelState(index, v) / setPixelState(index, ch, v): write
            // next frame's state. The first call allocates the buffer
            // (budget-checked); an out-of-range index writes nothing; a
            // channel outside 0..MAX_STATE_CHANNELS is a runtime error.
            // Returns v, like an assignment.
            SetPixelState => {
                let i = n(0).to_int_trunc();
                let (ch, v) = if argc >= 3 {
                    (n(1).to_int_trunc(), n(2))
                } else {
                    (0, n(1))
                };
                if ch < 0 || ch as usize >= MAX_STATE_CHANNELS {
                    return Err(no_site(format!(
                        "setPixelState channel {ch} out of range (0..{})",
                        MAX_STATE_CHANNELS - 1
                    )));
                }
                let ch = ch as usize;
                self.pixel_state_ensure(prog, ch)
                    .map_err(|m| no_site(m.into()))?;
                if let Some(s) = &mut self.pixel_state {
                    if i >= 0 && (i as usize) < s.n {
                        s.back[ch * s.n + i as usize] = v;
                    }
                }
                num(v)
            }
            // randomSeed(seed): pin `random()`'s stream — same seed, same
            // sequence on every Luxel build (synced installations). The
            // splitmix64 state becomes the seed's raw 16.16 word, so
            // fractional seeds are distinct and the generator's finalizer
            // handles the low entropy. Returns the previous seed (0 if the
            // stream was never seeded).
            RandomSeed => {
                let old = self.random_seed;
                let s = n(0);
                self.random_seed = s;
                self.rng = s.raw() as u32 as u64;
                num(old)
            }
            // timeScale(s): run the pattern-visible clock at s × real time
            // (0.25 = slow-mo, 0 = frozen, 2 = double speed). The engine
            // scales the frame delta before advancing `time_ms`, so
            // time()/beat()/beforeRender's delta all follow. Negative
            // scales clamp to 0. Returns the previous scale.
            TimeScale => {
                let old = self.time_scale;
                self.time_scale = n(0).max(Fx::ZERO);
                num(old)
            }
            // setFrameRate(fps): cap how often the pattern is evaluated.
            // The engine holds the last frame until 1000/fps ms of real
            // time have passed, then runs beforeRender with the whole
            // accumulated delta. fps <= 0 removes the cap; the period is
            // clamped to MAX_FRAME_PERIOD_RAW. Returns the previous cap
            // (0 = uncapped).
            SetFrameRate => {
                let old = self.frame_cap_fps;
                let fps = n(0).max(Fx::ZERO);
                self.frame_cap_fps = fps;
                self.frame_min_raw = if fps.raw() <= 0 {
                    0
                } else {
                    // (1000 ms << 16) / fps, in 16.16 ms: the fps operand
                    // is itself 16.16, hence the << 32 numerator.
                    ((1000u64 << 32) / fps.raw() as u64).min(MAX_FRAME_PERIOD_RAW)
                };
                num(old)
            }
            // ---- Luxel extensions, batch 6: post-process chain stages ----
            // The engine runs these over the finished frame, in chain order
            // (palette remap → blur → glow → gamma), once per frame — not
            // per pixel. All are off by default and cost nothing unset.
            //
            // setBlur(amount, passes = 1): 3-tap blur along the pixel index.
            // amount 0..1 is each neighbor's share (0.5 = the 1-2-1 kernel,
            // 1 = pure neighbor average); passes 1..8 widens the radius.
            SetBlur => {
                let old = self.post_blur;
                self.post_blur = n(0).clamp(Fx::ZERO, Fx::ONE);
                if argc >= 2 {
                    self.post_blur_passes = n(1).to_int_trunc().clamp(1, MAX_BLUR_PASSES) as u8;
                }
                num(old)
            }
            // setGlow(amount): light-bleed bloom — every pixel takes the
            // brighter of itself and `amount` of its brightest neighbor, so
            // highlights spread without the frame losing energy.
            SetGlow => {
                let old = self.post_glow;
                self.post_glow = n(0).clamp(Fx::ZERO, Fx::ONE);
                num(old)
            }
            // setOutputPalette(pal, amount = 1): recolor the finished frame
            // by luma through `pal` (setPalette's flat [pos,r,g,b,…] form),
            // blending `amount` of the way. Any non-array argument (e.g. 0)
            // clears the stage. Snapshotted, not live like setPalette: the
            // engine cooks a 256-entry table on install.
            SetOutputPalette => {
                self.post_palette_epoch = self.post_palette_epoch.wrapping_add(1);
                self.post_palette_amount = if argc >= 2 {
                    n(1).clamp(Fx::ZERO, Fx::ONE)
                } else {
                    Fx::ONE
                };
                match a(0) {
                    Value::Arr(arr) => {
                        let data = self.arr(prog, arr);
                        let mut pal = Vec::new();
                        let mut i = 0;
                        while i + 3 < data.len() {
                            pal.push((
                                data.at(i).num(),
                                [data.at(i + 1).num(), data.at(i + 2).num(), data.at(i + 3).num()],
                            ));
                            i += 4;
                        }
                        self.post_palette = pal;
                    }
                    _ => self.post_palette = Vec::new(),
                }
                Ok(Value::default())
            }
            // ---- Luxel extensions, batch 10: whole-frame bulk ops ----
            // The `renderFrame` entry's vocabulary (crate::bulk). Each one
            // writes the engine's frame buffer — lent to `Vm::frame` for
            // the duration of the call — directly in RGB888; outside that
            // entry the buffer is empty and they all no-op. Bodies live in
            // `bulk` so this dispatcher stays thin. They are TIER 3
            // (`builtin_cold`) by construction: a `renderFrame` pattern calls
            // each of these once per FRAME, never per pixel, and their bodies
            // are far too big to drag through the hot tiers' cache/IRAM
            // budget (Gitea #328, docs/firmware.md "Code placement").
            // `&args[..argc]` keeps the "missing args read as 0" convention
            // the `bulk` helpers assume.
            GridWidth => num(crate::bulk::grid_dim(self, 0)),
            GridHeight => num(crate::bulk::grid_dim(self, 1)),
            Clear => Ok(crate::bulk::clear(self)),
            FillAll => Ok(crate::bulk::fill(self)),
            Fade => Ok(crate::bulk::fade(self, &args[..argc])),
            SetPixel => Ok(crate::bulk::set_pixel(self, &args[..argc])),
            FillRange => Ok(crate::bulk::fill_range(self, &args[..argc])),
            FillHsv => crate::bulk::fill_hsv(self, prog, &args[..argc]).map_err(no_site),
            FillRgb => crate::bulk::fill_rgb(self, prog, &args[..argc]).map_err(no_site),
            FillGradient => Ok(crate::bulk::fill_gradient(self, &args[..argc])),
            FillRect => Ok(crate::bulk::fill_rect(self, &args[..argc])),
            FillCircle => Ok(crate::bulk::fill_circle(self, &args[..argc])),
            Splat => Ok(crate::bulk::splat(self, &args[..argc])),
            DrawLine => Ok(crate::bulk::draw_line(self, &args[..argc])),
            FillCanvas => crate::bulk::fill_canvas(self, prog, &args[..argc]).map_err(no_site),
            Blit => crate::bulk::blit(self, prog, &args[..argc]).map_err(no_site),
            // ---- Luxel extensions, batch 11 (Gitea #373) ----
            // fillNoise2D(dst, w, h, sx, sy, ox, oy, seed) and
            // fillNoise3D(dst, w, h, sx, sy, ox, oy, z, seed): fill the
            // first w×h elements of a row-major canvas with simplex noise
            // sampled on a regular lattice —
            //   dst[r*w + c] = simplex2/3(c*sx + ox, r*sy + oy[, z], seed)
            // — and return dst. The arguments are assembled with the same
            // `Fx` multiply-then-add a bytecode loop would emit, so the
            // result is EXACTLY what the interpreted loop produces; what
            // the op removes is the interpreter around the noise, not the
            // noise (see the test in `noise`). Row-major and map-free on
            // purpose: `dst` is a plain array, so it composes with
            // `fillCanvas`, `blur2D` and the array math.
            //
            // One body, two names: the 3D arm shifts `seed` by one slot
            // and calls `simplex3`. A `kind` argument selecting perlin was
            // considered and left out — `perlin`'s octaves/lacunarity/gain
            // would add three more slots and a second inner loop for a
            // family no library pattern samples on a lattice.
            FillNoise2D | FillNoise3D => {
                let three = builtin == FillNoise3D;
                let Value::Arr(arr) = a(0) else {
                    return Err(no_site(format!("{} of a non-array", def.name)));
                };
                let (w, h) = (n(1).to_int_trunc(), n(2).to_int_trunc());
                let (sx, sy, ox, oy) = (n(3), n(4), n(5), n(6));
                let (z, seed) = if three { (n(7), n(8)) } else { (Fx::ZERO, n(7)) };
                if w >= 1 && h >= 1 {
                    let (w, h) = (w as usize, h as usize);
                    let data = self.arr_mut(prog, arr).map_err(|m| no_site(m.into()))?;
                    if data.len() < w * h {
                        return Err(no_site(format!(
                            "{}: array shorter than w\u{d7}h ({} < {})",
                            def.name,
                            data.len(),
                            w * h
                        )));
                    }
                    for r in 0..h {
                        let y = Fx::from_int(r as i32) * sy + oy;
                        let base = r * w;
                        for c in 0..w {
                            let x = Fx::from_int(c as i32) * sx + ox;
                            data[base + c] = Value::Num(if three {
                                crate::noise::simplex3(x, y, z, seed)
                            } else {
                                crate::noise::simplex2(x, y, seed)
                            });
                        }
                    }
                }
                Ok(a(0))
            }
            // stencil2D(dst, src, w, h, kSelf, kEdge, kDiag):
            //   dst[i] += kSelf*src[i] + kEdge*(W+E+N+S) + kDiag*(the four
            //   diagonals)
            // over the first w×h elements of two row-major canvases, with
            // the border MIRRORED (clamped index) so a wave reaches the
            // outermost row and reflects instead of dying one cell short.
            // `src` is never written and `dst` accumulates, which is what
            // makes the two-buffer water recurrence one call:
            // `next = 2*now - before + c2*laplacian(now)` is
            // `arrayScale(before, -1)` then this with kSelf = 2 - 4*c2 and
            // kEdge = c2. kDiag = 0 is the 5-point Laplacian, kDiag =
            // kEdge the 9-point one, kSelf = 1 / kEdge = 0 a copy.
            //
            // Neighbours are summed W, E, N, S (then NW, NE, SW, SE) —
            // fixed-point addition is exact until it saturates, so the
            // order only matters at the rail, but it is pinned so an
            // equivalent bytecode loop can be written to match.
            Stencil2D => {
                let (Value::Arr(dst), Value::Arr(src)) = (a(0), a(1)) else {
                    return Err(no_site("stencil2D needs two arrays".into()));
                };
                if dst == src {
                    // A stencil reads neighbours it has already written,
                    // so an aliased call is a different (and unstated)
                    // operation, not a closed form. Say so.
                    return Err(no_site("stencil2D: dst and src must differ".into()));
                }
                let (w, h) = (n(2).to_int_trunc(), n(3).to_int_trunc());
                let (kself, kedge, kdiag) = (n(4), n(5), n(6));
                if w >= 1 && h >= 1 {
                    let (w, h) = (w as usize, h as usize);
                    let (d, s) = self
                        .arr_pair(prog, dst, src)
                        .map_err(|m| no_site(m.into()))?;
                    if d.len() < w * h || s.len() < w * h {
                        return Err(no_site(format!(
                            "stencil2D: array shorter than w\u{d7}h ({} < {})",
                            d.len().min(s.len()),
                            w * h
                        )));
                    }
                    let at = |i: usize| s.get(i).map_or(Fx::ZERO, |v| v.num());
                    for y in 0..h {
                        let row = y * w;
                        let up = if y > 0 { row - w } else { row };
                        let dn = if y + 1 < h { row + w } else { row };
                        for x in 0..w {
                            let i = row + x;
                            let l = if x > 0 { x - 1 } else { x };
                            let r = if x + 1 < w { x + 1 } else { x };
                            let mut v = d[i].num() + kself * at(i);
                            v = v + kedge * (at(row + l) + at(row + r) + at(up + x) + at(dn + x));
                            if kdiag != Fx::ZERO {
                                v = v
                                    + kdiag
                                        * (at(up + l) + at(up + r) + at(dn + l) + at(dn + r));
                            }
                            d[i] = Value::Num(v);
                        }
                    }
                }
                Ok(a(0))
            }
            // arrayMaxAbs(a): the largest |a[i]|, 0 for an empty array.
            // Ships with stencil2D on purpose (#373 §3): the peak-amplitude
            // reduction the water recurrence fuses into its own loop is what
            // lets a pool flatten to exactly nothing, and a native stencil
            // without it would hand the win straight back to a second
            // bytecode pass over the array. The INDEX form (argmax, which
            // five library patterns want to recycle the oldest slot) is a
            // separate op and is not this one — see the ticket.
            ArrayMaxAbs => {
                let Value::Arr(arr) = a(0) else {
                    return Err(no_site("arrayMaxAbs of a non-array".into()));
                };
                let mut m = Fx::ZERO;
                for v in self.arr(prog, arr).iter() {
                    let x = v.num();
                    let x = if x < Fx::ZERO { Fx::ZERO - x } else { x };
                    if x > m {
                        m = x;
                    }
                }
                num(m)
            }
            // paintCanvas(vArr, w, h [, bArr]): fillCanvas's geometry with
            // paint()'s colour — the installed palette sampled at vArr[i],
            // times bArr[i] (or 1), through each pixel's mapped (x, y).
            // The palette lookup is the same `sample_palette` the
            // interpreter's `paint` calls, so a cell is byte-exact against
            // `paint(v, b)` + `setPixel(i)`, and ONE array covers a panel
            // where fillCanvas's three would not fit the element budget.
            PaintCanvas => {
                // `setPalette` holds a live reference (oracle 2026-08-29):
                // re-cook before the fill so a pattern that wrote through
                // the installed array this frame sees its own change, the
                // way `palette_lookup` does per call.
                self.palette_refresh(prog);
                crate::bulk::paint_canvas(self, prog, &args[..argc]).map_err(no_site)
            }
            _ => unreachable!("handled by builtin_fast"),
        }
    }

    /// Fractional beat position at `bpm` on the engine clock (0..1 sawtooth).
    fn beat_phase(&self, bpm: Fx) -> Fx {
        // beats = ms·bpm/60000; with bpm in 16.16 the low 16 bits of the
        // quotient are exactly the fractional beat
        let phase = (self.time_ms as u128 * bpm.raw().max(0) as u128 / 60_000) & 0xFFFF;
        Fx::from_raw(phase as i32)
    }

    /// Write two numbers into the first two slots of `out`.
    fn write2(&mut self, prog: &Program, out: Value, vals: [Fx; 2]) -> Result<(), &'static str> {
        let Value::Arr(arr) = out else {
            return Err("`out` must be an array");
        };
        let slots = self.arr_mut(prog, arr)?;
        if slots.len() < 2 {
            return Err("`out` array needs length >= 2");
        }
        for (slot, v) in slots.iter_mut().zip(vals) {
            *slot = Value::Num(v);
        }
        Ok(())
    }

    /// Write three numbers into the first three slots of `out`.
    fn write3(&mut self, prog: &Program, out: Value, vals: [Fx; 3]) -> Result<(), &'static str> {
        let Value::Arr(arr) = out else {
            return Err("`out` must be an array");
        };
        let slots = self.arr_mut(prog, arr)?;
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
    /// per the universal corpus idiom). Order/sign/cap all oracle-verified
    /// (fw 3.67, 2026-08-22): ops past the 31st are silently IGNORED on PB
    /// (no error, no abort), so we drop them too. `resetTransform()` clears
    /// the count.
    fn push_op(&mut self, op: [[Fx; 4]; 4]) -> Result<(), &'static str> {
        if self.transform_ops >= 31 {
            return Ok(()); // PB caps silently at 31 stacked ops
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
                let c = m.coord(i as usize);
                match m.dims {
                    1 => [c[0], fill[1], fill[2]],
                    2 => [c[0], c[1], fill[2]],
                    _ => c,
                }
            }
            None => {
                let n = self.pixel_count.max(1);
                // 32-bit divide while `i << 16` fits (every real strip):
                // the i64 form is a ROM call per pixel on Xtensa
                let x = if i < 1 << 15 {
                    Fx::from_raw(((i << 16) / n) as i32)
                } else {
                    Fx::from_raw((((i as i64) << 16) / (n as i64)) as i32)
                };
                [x, fill[1], fill[2]]
            }
        }
    }

    /// Apply the current transform to a point (affine 4×4, w ignored).
    #[inline(never)]
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

    /// Re-cook `self.palette` from the backing array. Called by setPalette
    /// and again whenever the array was mutated since the last lookup.
    fn rebuild_palette(&mut self, prog: &Program, arr: u32) {
        let data = self.arr(prog, arr);
        let mut pal = Vec::new();
        let mut i = 0;
        while i + 3 < data.len() {
            pal.push((
                data.at(i).num(),
                [data.at(i + 1).num(), data.at(i + 2).num(), data.at(i + 3).num()],
            ));
            i += 4;
        }
        self.palette = pal;
    }

    fn palette_lookup(&mut self, prog: &Program, v: Fx) -> [Fx; 3] {
        self.palette_refresh(prog);
        sample_palette(&self.palette, v)
    }

    /// Re-cook the installed palette if the pattern has written through the
    /// array since the last lookup. `setPalette` holds a LIVE reference on
    /// PB (oracle, 2026-08-29): writes through the installed array change
    /// later lookups with no second `setPalette` call, and `arr_mut` flags
    /// the mutation. Split out of [`palette_lookup`] so `paintCanvas` can
    /// do it once and then read [`Vm::palette`] per cell.
    pub(crate) fn palette_refresh(&mut self, prog: &Program) {
        if self.palette_dirty {
            self.palette_dirty = false;
            if let Some(arr) = self.palette_src {
                self.rebuild_palette(prog, arr);
            }
        }
    }

    /// The cooked palette, as `paintCanvas` reads it after a refresh.
    pub(crate) fn palette(&self) -> &[(Fx, [Fx; 3])] {
        &self.palette
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
            Value::Fun(f) => self.run_on_top(prog, f as u16, args),
            Value::Builtin(b) => {
                for v in args {
                    self.stack.push(*v);
                }
                self.call_builtin(prog, b as u16, args.len())
            }
            _ => Err(VmError {
                message: "callback is not a function".into(),
                fn_idx: u16::MAX,
                pc: u32::MAX,
                line: 0,
                col: 0,
                is_assert: false,
            }),
        }
    }
}

/// The numeric core: every sub-opcode with both operands already reduced
/// to numbers. One out-of-line table shared by all four fused arms, entered
/// and left entirely in registers — an `Fx` is an i32, where a `Value` costs
/// a second register for the tag both ways (Gitea #312). `#[inline]` so
/// each of its two out-of-line wrappers gets the table directly: a nested
/// call would cost the fused arms a SECOND Xtensa window transition
/// (`entry`/`retw`), which is exactly what plain `#[inline]` produced at
/// `opt-level="s"` — LLVM declined a 107-instruction body at two call
/// sites and the const-fused arms each grew by three instructions and a
/// window. `always` costs ~110 bytes of duplicated table.
#[inline(always)]
/// In-place box blur over `data`: element `i` becomes the truncated integer
/// mean of the raw 16.16 values in `[i-r, i+r]`, clamped to the ends. `r == 0`
/// or an empty slice is a no-op.
///
/// Sliding window rather than a prefix-sum array (Gitea #296): the running sum
/// only needs the originals still inside a later window — indices `i+1-r ..= i`,
/// which are exactly the ones already overwritten — so the scratch is
/// `min(r + 1, len)` i64s (a few dozen bytes at the radius 1–8 patterns use)
/// instead of `len + 1` (32 KiB for a 4096-px panel buffer). Bit-identical to
/// the prefix-sum version: the window sums are the same exact i64 integers and
/// the same truncating division, just accumulated incrementally.
///
/// `Err(())` means the (small) scratch allocation failed; the array is
/// untouched in that case.
fn blur1d_inplace(data: &mut [Value], r: usize) -> Result<(), ()> {
    let len = data.len();
    if r == 0 || len == 0 {
        return Ok(());
    }
    // Ring of originals that have been overwritten but are still inside some
    // later window. Never larger than the array itself, so this can only be
    // cheaper than the old prefix sum.
    let cap = (r + 1).min(len);
    let mut ring: alloc::vec::Vec<i64> = alloc::vec::Vec::new();
    ring.try_reserve_exact(cap).map_err(|_| ())?;
    ring.resize(cap, 0i64);

    let (mut lo, mut hi) = (0usize, r.min(len - 1));
    let mut sum: i64 = data[..=hi].iter().map(|v| v.num().raw() as i64).sum();
    // ring cursors kept by hand — `i % cap` would be a hardware divide per
    // element, and `lo` only ever advances one slot at a time
    let (mut wp, mut rp) = (0usize, 0usize);
    for i in 0..len {
        // stash the original before it is clobbered; `lo` never falls further
        // than `i - r` behind, so `cap` slots keep every value still needed
        ring[wp] = data[i].num().raw() as i64;
        wp += 1;
        if wp == cap {
            wp = 0;
        }
        let avg = sum / (hi - lo + 1) as i64;
        data[i] = Value::Num(Fx::from_raw(avg as i32));
        if i + 1 == len {
            break;
        }
        let (nlo, nhi) = ((i + 1).saturating_sub(r), (i + 1 + r).min(len - 1));
        if nlo > lo {
            sum -= ring[rp]; // leaving the window: already overwritten
            rp += 1;
            if rp == cap {
                rp = 0;
            }
        }
        if nhi > hi {
            sum += data[nhi].num().raw() as i64; // entering: still original
        }
        lo = nlo;
        hi = nhi;
    }
    Ok(())
}

fn binop_fx(sub: u8, a: Fx, b: Fx) -> Fx {
    use crate::bytecode::op;
    let t = |c: bool| if c { Fx::ONE } else { Fx::ZERO };
    match sub {
        op::ADD => a + b,
        op::SUB => a - b,
        op::MUL => a * b,
        op::DIV => a / b,
        op::REM => a % b,
        op::POW => fmath::pow(a, b),
        op::BIT_AND => a & b,
        op::BIT_OR => a | b,
        op::BIT_XOR => a ^ b,
        op::SHL => a << b,
        op::SHR => a >> b,
        op::LT => t(a < b),
        op::LE => t(a <= b),
        op::GT => t(a > b),
        op::GE => t(a >= b),
        // both operands are `Num` here, and `value_eq` on two `Num`s is
        // exactly `Fx` equality
        op::EQ => t(a == b),
        op::NE => t(a != b),
        // unreachable: bytecode::walk_word rejects every other sub-opcode
        _ => Fx::ZERO,
    }
}

/// One two-operand value op, by its base opcode — the arithmetic and
/// comparison arms of [`Vm::run`] factored out so the fused `<cmp>;
/// JmpIfFalse` superinstruction (Gitea #261) cannot drift from the
/// sequence it replaces. `a` is the deeper operand (pushed first), `b` the
/// shallower, exactly as the base pair pops them. The decoder rejects any
/// sub-opcode outside this set. Also the semantic reference the compiler's
/// constant folder evaluates against (`compile::peephole`, and the drift
/// test that pins the two together), which is why it keeps the `Value`
/// signature the folder needs.
#[inline(never)]
pub(crate) fn binop(sub: u8, a: Value, b: Value) -> Value {
    use crate::bytecode::op;
    Value::Num(match sub {
        // reference identity, not numeric equality — the one place the
        // operands' kinds matter
        op::EQ => {
            if value_eq(a, b) {
                Fx::ONE
            } else {
                Fx::ZERO
            }
        }
        op::NE => {
            if value_eq(a, b) {
                Fx::ZERO
            } else {
                Fx::ONE
            }
        }
        _ => binop_fx(sub, a.num(), b.num()),
    })
}

/// `binop` with the right operand taken from the instruction word's
/// literal — the `Const c; <op>` fusions (CONST_OP, LOAD_L_CONST_OP,
/// LOAD_G_CONST_OP). Nothing here needs the literal wrapped in a `Value`:
/// a non-`Num` left operand can only make EQ false and NE true (`value_eq`
/// is false across kinds), and every other sub-op sees `a.num() == 0`.
#[inline(never)]
fn binop_const(sub: u8, a: Value, c: Fx) -> Fx {
    use crate::bytecode::op;
    match a {
        Value::Num(x) => binop_fx(sub, x, c),
        _ => match sub {
            op::EQ => Fx::ZERO,
            op::NE => Fx::ONE,
            _ => binop_fx(sub, Fx::ZERO, c),
        },
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
/// Oracle-verified (fw 3.67, 2026-08-22): rotateX/Y/Z are all CCW for
/// +angle, right-handed, matching these matrices exactly.
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
/// saturation/value clamp to 0..1. Rounding oracle-verified 2026-07-08:
/// all 21 rgb/hsv cases bit-exact after the floor(v*255) quantization fix.
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

/// Cell index for a normalized coordinate on an n-cell axis:
/// floor(x·n) clamped to 0..n−1 (so x = 1.0 lands in the last cell,
/// with no `* 15.99` fudge). Exact in i64 raw — no 16.16 overflow.
pub(crate) fn cell_index(x: Fx, n: usize) -> usize {
    let i = (x.raw() as i64 * n as i64) >> 16; // arithmetic shift = floor
    i.clamp(0, n as i64 - 1) as usize
}

/// One axis of a bilinear canvas sample: texel centers sit at
/// (i + 0.5)/n, so the sample position is x·n − ½. Returns the two
/// edge-clamped texel indices and the 16-bit blend fraction between
/// them (coordinates past the borders clamp to the border texel).
fn sample_axis(x: Fx, n: usize) -> (usize, usize, i64) {
    let pos = x.raw() as i64 * n as i64 - (1i64 << 15); // 16.16
    let (i, t) = (pos >> 16, pos & 0xFFFF);
    let last = n as i64 - 1;
    (
        i.clamp(0, last) as usize,
        (i + 1).clamp(0, last) as usize,
        t,
    )
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

/// `time()`'s phase in 16.16: `floor(((now % period) << 16) / period)`,
/// computed entirely in 32-bit registers.
///
/// `period` is a 16.16 raw clamped to `[1, 2^31)`, so `now % period` is a
/// hardware 32-bit divide and the remainder is `< period`. Scaling it by
/// 2^16 needs 48 bits, which the ≤ 2^16 case gets for free (`t << 16` then
/// fits u32) and the general case gets from 16 restoring-division steps:
/// `x < period ≤ 2^31` at the top of every step, so `x << 1` cannot
/// overflow, and the quotient is exactly the same word the 64-bit
/// `(t << 16) / period` produces (Gitea #312).
#[inline]
fn time_phase32(now_ms: u32, period: u32) -> u32 {
    let t = now_ms % period;
    if period <= 1 << 16 {
        return (t << 16) / period;
    }
    let mut x = t;
    let mut q = 0u32;
    for _ in 0..16 {
        x <<= 1;
        q <<= 1;
        if x >= period {
            x -= period;
            q += 1;
        }
    }
    q
}

/// [`time_phase32`] past 2^32 ms (49 days) of uptime — the only shape that
/// still needs the ROM's 64-bit divide, kept out of the dispatch loop.
#[cold]
#[inline(never)]
fn time_phase_u64(now_ms: u64, period: u32) -> u32 {
    let p = period as u64;
    let t = now_ms % p;
    ((t << 16) / p) as u32
}

#[cfg_attr(feature = "iram-math", link_section = ".rwtext")]
#[cfg_attr(feature = "iram-math", inline(never))]
pub fn hsv_to_rgb(h: Fx, s: Fx, v: Fx) -> [Fx; 3] {
    let s = s.clamp(Fx::ZERO, Fx::ONE);
    let v = v.clamp(Fx::ZERO, Fx::ONE);
    let h6 = h.wrap_unit().raw() * 6; // [0, 6) in 16-frac; < 2^19, fits i32
    let sector = h6 >> 16; // 0..5
    let f = Fx::from_raw(h6 & 0xFFFF);
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

// ---- dynamic opcode profiler (host tooling only) ----
//
// Gated behind the `profile` cargo feature, which nothing on the device
// path enables: firmware depends on luxel-core with `default-features =
// false` and never names `profile`, so not one counter, field or branch of
// this exists in an ESP32 image. The CLI turns it on (see luxel-cli's
// `profile` feature) so `luxel bench --profile` can rank what the
// interpreter actually executes — the input for choosing superinstructions
// (Gitea #261).
#[cfg(feature = "profile")]
pub mod prof {
    use alloc::collections::BTreeMap;

    /// Dynamic execution counts for one run.
    #[derive(Clone)]
    pub struct Profile {
        /// Executions per opcode.
        pub ops: [u64; 256],
        /// Executions of a **statically adjacent** opcode pair — the
        /// second instruction is the first's fall-through successor, in
        /// the same function, reached without a jump. That is exactly the
        /// adjacency a compiler-side peephole can fuse.
        pub bigrams: BTreeMap<(u8, u8), u64>,
        pub trigrams: BTreeMap<(u8, u8, u8), u64>,
        /// Calls per builtin runtime id (`BUILTINS` index).
        pub builtins: BTreeMap<u16, u64>,
        /// Total instructions executed.
        pub insns: u64,
    }

    impl Default for Profile {
        fn default() -> Self {
            Profile {
                ops: [0; 256],
                bigrams: BTreeMap::new(),
                trigrams: BTreeMap::new(),
                builtins: BTreeMap::new(),
                insns: 0,
            }
        }
    }

    impl Profile {
        pub fn merge(&mut self, other: &Profile) {
            for (a, b) in self.ops.iter_mut().zip(other.ops.iter()) {
                *a += b;
            }
            for (k, v) in &other.bigrams {
                *self.bigrams.entry(*k).or_insert(0) += v;
            }
            for (k, v) in &other.trigrams {
                *self.trigrams.entry(*k).or_insert(0) += v;
            }
            for (k, v) in &other.builtins {
                *self.builtins.entry(*k).or_insert(0) += v;
            }
            self.insns += other.insns;
        }
    }
}

#[cfg(feature = "profile")]
impl Vm {
    /// The counters accumulated so far.
    pub fn profile(&self) -> &prof::Profile {
        &self.prof
    }

    /// Zero the counters (the CLI drops init/first-frame noise this way).
    pub fn profile_reset(&mut self) {
        self.prof = prof::Profile::default();
        self.prof_prev = None;
        self.prof_prev2 = None;
        self.prof_prev_seq = false;
    }

    /// Record one instruction. `pc` is its fn-relative word index.
    #[inline]
    fn prof_record(&mut self, fi: u16, pc: u32, opcode: u8) {
        let len = if opcode == crate::bytecode::op::CONST_NUM {
            2
        } else {
            1
        };
        self.prof.ops[opcode as usize] += 1;
        self.prof.insns += 1;
        // Statically adjacent iff the previous instruction ended exactly
        // where this one starts, in the same function.
        let seq = matches!(self.prof_prev, Some((f, end, _)) if f == fi && end == pc);
        if seq {
            let (_, _, p) = self.prof_prev.expect("seq implies prev");
            *self.prof.bigrams.entry((p, opcode)).or_insert(0) += 1;
            if self.prof_prev_seq {
                if let Some((_, _, p2)) = self.prof_prev2 {
                    *self.prof.trigrams.entry((p2, p, opcode)).or_insert(0) += 1;
                }
            }
        }
        self.prof_prev2 = self.prof_prev;
        self.prof_prev_seq = seq;
        self.prof_prev = Some((fi, pc + len, opcode));
    }

    #[inline]
    fn prof_builtin(&mut self, b: u16) {
        *self.prof.builtins.entry(b).or_insert(0) += 1;
    }
}

#[cfg(test)]
mod blur1d_tests {
    use super::{blur1d_inplace, Value};
    use crate::fixed::Fx;
    use alloc::vec::Vec;

    /// The pre-#296 implementation, kept verbatim as the reference: a full
    /// `len + 1` prefix-sum array read window-by-window. The sliding-window
    /// version must agree with it bit for bit.
    fn blur1d_prefix_reference(data: &mut [Value], r: usize) {
        let len = data.len();
        if r > 0 && len > 0 {
            let mut pre: Vec<i64> = Vec::with_capacity(len + 1);
            pre.push(0i64);
            for v in data.iter() {
                pre.push(pre.last().unwrap() + v.num().raw() as i64);
            }
            for i in 0..len {
                let lo = i.saturating_sub(r);
                let hi = (i + r).min(len - 1);
                let avg = (pre[hi + 1] - pre[lo]) / (hi - lo + 1) as i64;
                data[i] = Value::Num(Fx::from_raw(avg as i32));
            }
        }
    }

    fn xorshift(state: &mut u32) -> u32 {
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        *state = x;
        x
    }

    #[test]
    fn sliding_window_matches_prefix_sum_reference() {
        // lengths span the degenerate ends and the 4096-px panel that
        // motivated #296; radii span "no-op", what patterns use, and radii
        // far wider than the array (every window clamps to the whole array)
        let lens = [1usize, 2, 3, 4, 5, 7, 8, 16, 17, 63, 64, 255, 256, 4096];
        let radii = [0usize, 1, 2, 3, 4, 5, 8, 16, 63, 255, 4095, 4096, 100_000];
        let mut seed = 0x1234_5678u32;
        for &len in lens.iter() {
            for &r in radii.iter() {
                for trial in 0..3 {
                    let mut src: Vec<Value> = Vec::with_capacity(len);
                    for i in 0..len {
                        let raw = match trial {
                            // 16.16 values in a plausible 0..1 pattern range
                            0 => (xorshift(&mut seed) % 65_536) as i32,
                            // signed, wide magnitude — exercises truncation
                            // toward zero in the integer division
                            1 => xorshift(&mut seed) as i32 / 4,
                            // a sparse impulse train (comets.js's shape)
                            _ => {
                                if i % 13 == 0 {
                                    i32::from(Fx::ONE.raw() != 0) * Fx::ONE.raw()
                                } else {
                                    0
                                }
                            }
                        };
                        src.push(Value::Num(Fx::from_raw(raw)));
                    }
                    let mut want = src.clone();
                    blur1d_prefix_reference(&mut want, r);
                    let mut got = src.clone();
                    blur1d_inplace(&mut got, r).expect("scratch alloc");
                    for i in 0..len {
                        assert_eq!(
                            got[i].num().raw(),
                            want[i].num().raw(),
                            "len {} radius {} trial {} index {}",
                            len,
                            r,
                            trial,
                            i
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn empty_and_zero_radius_are_no_ops() {
        let mut empty: Vec<Value> = Vec::new();
        blur1d_inplace(&mut empty, 4).expect("scratch alloc");
        assert!(empty.is_empty());
        let mut one = alloc::vec![Value::Num(Fx::from_raw(7)), Value::Num(Fx::from_raw(9))];
        blur1d_inplace(&mut one, 0).expect("scratch alloc");
        assert_eq!(one[0].num().raw(), 7);
        assert_eq!(one[1].num().raw(), 9);
    }
}
