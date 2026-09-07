//! Whole-frame ("bulk") pattern ops — the vocabulary of the `renderFrame`
//! entry point (`Engine::drive`'s `RunStage::Frame`).
//!
//! A `renderFrame` pattern runs ONCE per frame instead of once per pixel,
//! which removes the ~317–440 Xtensa cycles/px the per-pixel entry costs
//! (docs/boards.md "Second light"). To make that useful the pattern needs
//! native ops that touch many pixels per call: that is everything here.
//!
//! Three conventions run through the whole module:
//!
//! - **The frame buffer is lent, not copied.** `Engine` `mem::take`s its
//!   `pixels` into [`crate::vm::Vm::frame`] for the duration of the call
//!   and takes it back afterwards, so these ops write the real output
//!   buffer in RGB888 with no intermediate. Outside `renderFrame` the Vec
//!   is empty, which makes every op below a silent no-op — that is the
//!   whole guard, there is no mode flag.
//! - **The brush.** `hsv()`/`rgb()`/`paint()`/`oklch()` keep writing
//!   `Vm::pixel`; under `renderFrame` that slot is the current colour and
//!   the shape ops read it. Every existing colour builtin therefore works
//!   here for free. The engine resets it to black at the top of each
//!   frame; within a frame it is sticky.
//! - **Three spaces.** *Index* ops take a pixel index and work on any map
//!   or none. *Coordinate* ops are a predicate over each pixel's MAPPED
//!   (x, y) — normalized exactly as `render2D` sees them, transform
//!   included — so they are well defined on a sparse or irregular map.
//!   *Grid* ops need a real W×H grid and no-op without one.
//!
//! The coordinate ops have a grid fast path: with a grid map installed and
//! no transform active, they walk only the cells in the shape's bounding
//! box instead of every pixel. It is **bit-identical** to the generic scan
//! by construction — the fast path narrows the candidate set and then
//! evaluates the same predicate on the same coordinate, so it can only
//! skip pixels the predicate would have rejected. `FORCE_SCAN` forces the
//! generic path in tests and the two are asserted equal.

use alloc::format;
use alloc::string::String;

use crate::engine::quantize;
use crate::fixed::Fx;
use crate::fmath;
use crate::outpipe::GridMap;
use crate::vm::{cell_index, hsv_to_rgb, ArrView, MapData, Program, Value, Vm};

/// 0.5 — the mid-space fill for coordinate axes the map doesn't carry,
/// spelled raw so no softfloat conversion survives into the image.
const MID: Fx = Fx::from_raw(1 << 15);

/// Force the generic coordinate scan instead of the grid fast path. Test
/// hook only: on a non-test build `force_scan()` is a compile-time `false`
/// and the whole branch folds away. Flipping it can never change a result
/// (that is what the equivalence test proves), so a parallel test seeing
/// it set is harmless.
#[cfg(test)]
pub(crate) static FORCE_SCAN: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

#[inline]
fn force_scan() -> bool {
    #[cfg(test)]
    {
        FORCE_SCAN.load(core::sync::atomic::Ordering::Relaxed)
    }
    #[cfg(not(test))]
    {
        false
    }
}

// ---- argument helpers ----

#[inline]
fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).copied().unwrap_or_default()
}

#[inline]
fn num(args: &[Value], i: usize) -> Fx {
    arg(args, i).num()
}

// ---- blending ----

/// `mode` argument: 0 replace, 1 add (saturating), 2 max (lighten),
/// 3 keyed (a black source is transparent). Anything else reads as
/// replace, matching the "missing args read as 0" builtin convention.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Blend {
    Replace,
    Add,
    Max,
    Keyed,
}

#[inline]
fn blend(v: Fx) -> Blend {
    match v.to_int_trunc() {
        1 => Blend::Add,
        2 => Blend::Max,
        3 => Blend::Keyed,
        _ => Blend::Replace,
    }
}

#[inline]
fn put(dst: &mut [u8; 3], src: [u8; 3], mode: Blend) {
    match mode {
        Blend::Replace => *dst = src,
        Blend::Add => {
            for c in 0..3 {
                dst[c] = dst[c].saturating_add(src[c]);
            }
        }
        Blend::Max => {
            for c in 0..3 {
                dst[c] = dst[c].max(src[c]);
            }
        }
        Blend::Keyed => {
            if src != [0, 0, 0] {
                *dst = src;
            }
        }
    }
}

/// The current brush as output bytes.
#[inline]
fn brush(vm: &Vm) -> [u8; 3] {
    let [r, g, b] = vm.pixel;
    [quantize(r), quantize(g), quantize(b)]
}

/// The brush scaled by `k` BEFORE quantization — a soft edge keeps its
/// precision instead of losing it to the 8-bit floor twice.
#[inline]
fn brush_scaled(c: [Fx; 3], k: Fx) -> [u8; 3] {
    [quantize(c[0] * k), quantize(c[1] * k), quantize(c[2] * k)]
}

/// `i / n` in 16.16, on the 32-bit path while `i << 16` fits (every real
/// strip) — the i64 form is a ROM call per pixel on Xtensa.
#[inline]
fn unit_frac(i: u32, n: u32) -> Fx {
    if n == 0 {
        Fx::ZERO
    } else if i < 1 << 15 {
        Fx::from_raw(((i << 16) / n) as i32)
    } else {
        Fx::from_raw((((i as i64) << 16) / n as i64) as i32)
    }
}

// ---- array / scalar arguments ----

/// A per-pixel channel argument: one number broadcast to every pixel, or
/// an array read by pixel index. The scalar/array decision is made once
/// per call; `at` is a two-way branch, not a `Value` match.
enum Src<'a> {
    Scalar(Fx),
    Arr(ArrView<'a>),
}

impl Src<'_> {
    #[inline]
    fn at(&self, i: usize) -> Fx {
        match self {
            Src::Scalar(v) => *v,
            Src::Arr(a) => a.get(i).map_or(Fx::ZERO, |v| v.num()),
        }
    }

    /// How many pixels this argument can serve — a scalar serves all.
    #[inline]
    fn limit(&self) -> usize {
        match self {
            Src::Scalar(_) => usize::MAX,
            Src::Arr(a) => a.len(),
        }
    }
}

fn src<'a>(vm: &'a Vm, prog: &'a Program, v: Value, who: &str) -> Result<Src<'a>, String> {
    match v {
        Value::Num(x) => Ok(Src::Scalar(x)),
        Value::Arr(id) => match vm.array(prog, id) {
            Some(a) => Ok(Src::Arr(a)),
            None => Err(format!("{who}: bad array reference")),
        },
        _ => Err(format!("{who}: expected a number or an array")),
    }
}

// ---- the grid fast path ----

/// A grid map, plus which coordinate axis runs along a row. `GridMap`
/// itself only records the wiring (`w`, `h`, serpentine), and
/// `outpipe::detect_grid` accepts a panel whose COLUMNS walk y and whose
/// rows walk x, so the axis is recovered here from the map's own
/// coordinates.
struct GridView<'a> {
    g: GridMap,
    map: &'a MapData,
    /// 0 = x varies along a row, 1 = y does.
    fast: usize,
}

impl GridView<'_> {
    #[inline]
    fn coord(&self, r: usize, c: usize) -> [Fx; 3] {
        self.map.coord(self.g.index(r, c))
    }

    /// The cells of one axis whose coordinate lies inside the bounding
    /// box. A grid's axis values are strictly monotonic so the answer is
    /// contiguous; taking min..max anyway means a degenerate map can only
    /// WIDEN the candidate set, never drop a pixel the scan would paint.
    fn span(&self, along_row: bool, bx: (Fx, Fx), by: (Fx, Fx)) -> Option<(usize, usize)> {
        let n = if along_row {
            self.g.w as usize
        } else {
            self.g.h as usize
        };
        let axis = if along_row { self.fast } else { 1 - self.fast };
        let (lo, hi) = if axis == 0 { bx } else { by };
        let mut first: Option<usize> = None;
        let mut last = 0;
        for k in 0..n {
            let v = if along_row {
                self.coord(0, k)[axis]
            } else {
                self.coord(k, 0)[axis]
            };
            if v >= lo && v <= hi {
                first.get_or_insert(k);
                last = k;
            }
        }
        first.map(|f| (f, last))
    }
}

/// The installed map as a grid, when every precondition for the fast path
/// holds: a grid whose extent is exactly the frame, a 2D map of the same
/// length, and no active transform (a transform moves coordinates out of
/// grid order, so those runs take the scan).
fn grid_view<'a>(vm: &'a Vm, n: usize) -> Option<GridView<'a>> {
    if force_scan() || vm.transform_active {
        return None;
    }
    let g = vm.frame_grid?;
    if g.is_empty() || g.len() != n {
        return None;
    }
    let map = vm.map.as_ref()?;
    if map.dims != 2 || map.len() != n {
        return None;
    }
    let fast = if map.grid.is_some() {
        0 // procedural row-major grid: x runs along a row by construction
    } else if g.w >= 2 {
        let a = map.coord(g.index(0, 0));
        let b = map.coord(g.index(0, 1));
        if a[0] != b[0] {
            0
        } else if a[1] != b[1] {
            1
        } else {
            return None;
        }
    } else {
        return None;
    };
    Some(GridView { g, map, fast })
}

/// Coordinates of pixel `i` exactly as `render2D` receives them.
#[inline]
fn coord_of(vm: &Vm, i: usize) -> [Fx; 3] {
    vm.apply_transform(vm.pixel_coords(i as u32, [MID; 3]))
}

/// Run `f` over every pixel inside the (inclusive) bounding box, through
/// the grid fast path when there is one and a full scan otherwise. `f`
/// receives the destination pixel and its mapped (x, y) and applies the
/// shape's own predicate — which is what keeps the two paths identical.
fn paint_shape<F>(vm: &mut Vm, bx: (Fx, Fx), by: (Fx, Fx), mut f: F)
where
    F: FnMut(&mut [u8; 3], Fx, Fx),
{
    if bx.1 < bx.0 || by.1 < by.0 {
        return;
    }
    let mut frame = core::mem::take(&mut vm.frame);
    let n = frame.len();
    if n != 0 {
        match grid_view(vm, n) {
            Some(gv) => {
                if let (Some((c0, c1)), Some((r0, r1))) =
                    (gv.span(true, bx, by), gv.span(false, bx, by))
                {
                    for r in r0..=r1 {
                        for c in c0..=c1 {
                            let p = gv.coord(r, c);
                            if p[0] >= bx.0 && p[0] <= bx.1 && p[1] >= by.0 && p[1] <= by.1 {
                                f(&mut frame[gv.g.index(r, c)], p[0], p[1]);
                            }
                        }
                    }
                }
            }
            None => {
                for (i, dst) in frame.iter_mut().enumerate() {
                    let p = coord_of(vm, i);
                    if p[0] >= bx.0 && p[0] <= bx.1 && p[1] >= by.0 && p[1] <= by.1 {
                        f(dst, p[0], p[1]);
                    }
                }
            }
        }
    }
    vm.frame = frame;
}

// ---- index space ----

/// `gridWidth()` / `gridHeight()`: the installed grid's dimensions, 0 when
/// the map is not a grid — so a pattern can branch instead of erroring.
///
/// A grid only has to COVER the frame, not match it exactly: the default
/// map is `ceil(√n)` wide, so a 60-pixel strip gets an 8×8 grid with four
/// unused cells in the last row. Those cells clip (see [`blit`]); the
/// dimensions are real and are reported. A grid SMALLER than the frame
/// cannot address every pixel, so it reads as no grid at all — which is
/// what keeps "`gridWidth()` == 0 → no grid-space ops" exactly true.
pub(crate) fn grid_dim(vm: &Vm, axis: usize) -> Fx {
    match vm.frame_grid {
        Some(g) if !g.is_empty() && g.len() >= vm.pixel_count as usize => {
            Fx::from_int(if axis == 0 { g.w as i32 } else { g.h as i32 })
        }
        _ => Fx::ZERO,
    }
}

/// `clear()`: the whole frame to black. The frame PERSISTS between
/// `renderFrame` calls (that is what makes `fade` + `setPixel` trails work
/// with no pattern-side array), so a pattern that wants a fresh canvas
/// clears it itself.
pub(crate) fn clear(vm: &mut Vm) -> Value {
    vm.frame.iter_mut().for_each(|p| *p = [0; 3]);
    Value::default()
}

/// `fill()`: the whole frame to the brush.
pub(crate) fn fill(vm: &mut Vm) -> Value {
    let c = brush(vm);
    vm.frame.iter_mut().for_each(|p| *p = c);
    Value::default()
}

/// `fade(k)`: every channel × k (clamped 0..1), floored — so a trail
/// decays to true black instead of parking on 1.
pub(crate) fn fade(vm: &mut Vm, args: &[Value]) -> Value {
    let k = num(args, 0).clamp(Fx::ZERO, Fx::ONE).raw() as u32;
    for p in vm.frame.iter_mut() {
        for c in p.iter_mut() {
            *c = ((*c as u32 * k) >> 16) as u8;
        }
    }
    Value::default()
}

/// `setPixel(i)`: the brush at index floor(i); out of range is a no-op.
pub(crate) fn set_pixel(vm: &mut Vm, args: &[Value]) -> Value {
    let c = brush(vm);
    let i = num(args, 0).to_int_floor();
    if i >= 0 && (i as usize) < vm.frame.len() {
        vm.frame[i as usize] = c;
    }
    Value::default()
}

/// `fillRange(i0, i1)`: the brush on [i0, i1) — exclusive end, clamped;
/// an empty or inverted range paints nothing.
pub(crate) fn fill_range(vm: &mut Vm, args: &[Value]) -> Value {
    let c = brush(vm);
    let n = vm.frame.len() as i64;
    let lo = (num(args, 0).to_int_floor() as i64).clamp(0, n);
    let hi = (num(args, 1).to_int_floor() as i64).clamp(0, n);
    if hi > lo {
        vm.frame[lo as usize..hi as usize]
            .iter_mut()
            .for_each(|p| *p = c);
    }
    Value::default()
}

/// `fillHSV(h, s, v)` / `fillRGB(r, g, b)`: each argument is a scalar OR
/// an array indexed by pixel index — the persistent-buffer readout that
/// 29 % of the library does per pixel (`hsv(hues[i], 1, vals[i])`), as one
/// call. An array shorter than the strip bounds the run and leaves the
/// tail untouched, matching the `arrayAdd` convention.
pub(crate) fn fill_hsv(vm: &mut Vm, prog: &Program, args: &[Value]) -> Result<Value, String> {
    fill_channels(vm, prog, args, true)
}

pub(crate) fn fill_rgb(vm: &mut Vm, prog: &Program, args: &[Value]) -> Result<Value, String> {
    fill_channels(vm, prog, args, false)
}

fn fill_channels(
    vm: &mut Vm,
    prog: &Program,
    args: &[Value],
    hsv: bool,
) -> Result<Value, String> {
    let mut frame = core::mem::take(&mut vm.frame);
    let r = fill_channels_into(vm, prog, args, hsv, &mut frame);
    vm.frame = frame;
    r
}

fn fill_channels_into(
    vm: &Vm,
    prog: &Program,
    args: &[Value],
    hsv: bool,
    frame: &mut [[u8; 3]],
) -> Result<Value, String> {
    let who = if hsv { "fillHSV" } else { "fillRGB" };
    let a = src(vm, prog, arg(args, 0), who)?;
    let b = src(vm, prog, arg(args, 1), who)?;
    let c = src(vm, prog, arg(args, 2), who)?;
    let n = frame
        .len()
        .min(a.limit())
        .min(b.limit())
        .min(c.limit());
    for (i, dst) in frame[..n].iter_mut().enumerate() {
        let (x, y, z) = (a.at(i), b.at(i), c.at(i));
        let [r, g, b] = if hsv {
            hsv_to_rgb(x, y, z)
        } else {
            [x, y, z]
        };
        *dst = [quantize(r), quantize(g), quantize(b)];
    }
    Ok(Value::default())
}

/// `fillGradient(h0, s0, v0, h1, s1, v1 [, axis])`: an HSV lerp across the
/// whole frame. `axis` 0 (the default) runs along the pixel index,
/// 1/2/3 along the mapped x/y/z. Hue is lerped UNWRAPPED and `hsv()`
/// wraps it, so `fillGradient(t, 1, 1, t + 1, 1, 1)` is the default
/// rainbow in one call.
pub(crate) fn fill_gradient(vm: &mut Vm, args: &[Value]) -> Value {
    let a = [num(args, 0), num(args, 1), num(args, 2)];
    let b = [num(args, 3), num(args, 4), num(args, 5)];
    let axis = num(args, 6).to_int_trunc();
    let mut frame = core::mem::take(&mut vm.frame);
    let n = frame.len();
    if n != 0 {
        let lerp = |t: Fx| {
            let c = [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
            ];
            let [r, g, bb] = hsv_to_rgb(c[0], c[1], c[2]);
            [quantize(r), quantize(g), quantize(bb)]
        };
        if (1..=3).contains(&axis) {
            let k = (axis - 1) as usize;
            for (i, dst) in frame.iter_mut().enumerate() {
                *dst = lerp(coord_of(vm, i)[k]);
            }
        } else {
            let last = (n - 1) as u32;
            for (i, dst) in frame.iter_mut().enumerate() {
                *dst = lerp(unit_frac(i as u32, last));
            }
        }
    }
    vm.frame = frame;
    Value::default()
}

// ---- coordinate space ----

/// `fillRect(x0, y0, x1, y1)`: the brush on every pixel whose mapped
/// coordinate is inside the (inclusive) rectangle, in any corner order.
pub(crate) fn fill_rect(vm: &mut Vm, args: &[Value]) -> Value {
    let (x0, y0) = (num(args, 0), num(args, 1));
    let (x1, y1) = (num(args, 2), num(args, 3));
    let c = brush(vm);
    paint_shape(
        vm,
        (x0.min(x1), x0.max(x1)),
        (y0.min(y1), y0.max(y1)),
        |dst, _, _| *dst = c,
    );
    Value::default()
}

/// `fillCircle(x, y, r)`: a hard-edged disc of the brush.
pub(crate) fn fill_circle(vm: &mut Vm, args: &[Value]) -> Value {
    let (cx, cy, r) = (num(args, 0), num(args, 1), num(args, 2));
    if r < Fx::ZERO {
        return Value::default();
    }
    let c = brush(vm);
    paint_shape(vm, (cx - r, cx + r), (cy - r, cy + r), |dst, x, y| {
        if fmath::hypot(x - cx, y - cy) <= r {
            *dst = c;
        }
    });
    Value::default()
}

/// `splat(x, y, r, mode)`: a soft disc — the brush × (1 − d/r), linear
/// falloff — blended with `mode`. The entity-loop primitive: `clear()`
/// plus N splats replaces a per-pixel `hypot` loop over N sprites.
pub(crate) fn splat(vm: &mut Vm, args: &[Value]) -> Value {
    let (cx, cy, r) = (num(args, 0), num(args, 1), num(args, 2));
    if r <= Fx::ZERO {
        return Value::default();
    }
    let mode = blend(num(args, 3));
    let c = vm.pixel;
    paint_shape(vm, (cx - r, cx + r), (cy - r, cy + r), |dst, x, y| {
        let d = fmath::hypot(x - cx, y - cy);
        if d >= r {
            return;
        }
        put(dst, brush_scaled(c, Fx::ONE - d / r), mode);
    });
    Value::default()
}

/// `drawLine(x0, y0, x1, y1, w, mode)`: a capsule — the brush ×
/// (1 − d/w) where d is the distance to the segment. A hard line is a
/// tiny `w` with mode 0.
pub(crate) fn draw_line(vm: &mut Vm, args: &[Value]) -> Value {
    let (ax, ay) = (num(args, 0), num(args, 1));
    let (bx, by) = (num(args, 2), num(args, 3));
    let w = num(args, 4);
    if w <= Fx::ZERO {
        return Value::default();
    }
    let mode = blend(num(args, 5));
    let c = vm.pixel;
    // Direction as a UNIT vector, computed once: the per-pixel projection
    // is then two multiplies and a clamp, with no divide in the loop (a
    // 64-bit one would be a ROM call on Xtensa).
    let (ex, ey) = (bx - ax, by - ay);
    let len = fmath::hypot(ex, ey);
    let (ux, uy) = if len > Fx::ZERO {
        (ex / len, ey / len)
    } else {
        (Fx::ZERO, Fx::ZERO)
    };
    paint_shape(
        vm,
        (ax.min(bx) - w, ax.max(bx) + w),
        (ay.min(by) - w, ay.max(by) + w),
        |dst, x, y| {
            let (dx, dy) = (x - ax, y - ay);
            let t = (dx * ux + dy * uy).clamp(Fx::ZERO, len);
            let d = fmath::hypot(dx - ux * t, dy - uy * t);
            if d >= w {
                return;
            }
            put(dst, brush_scaled(c, Fx::ONE - d / w), mode);
        },
    );
    Value::default()
}

/// `fillCanvas(hArr, sArr, vArr, w, h)`: sample a w×h row-major canvas
/// (three parallel arrays, each of which may be a scalar) at each pixel's
/// mapped (x, y) with NEAREST sampling — the
/// `canvas[floor(y·H)·W + floor(x·W)]` idiom of the 2D-canvas patterns,
/// as one call. On a procedural grid of exactly w×h it degenerates to a
/// copy through the index map.
pub(crate) fn fill_canvas(vm: &mut Vm, prog: &Program, args: &[Value]) -> Result<Value, String> {
    let mut frame = core::mem::take(&mut vm.frame);
    let r = fill_canvas_into(vm, prog, args, &mut frame);
    vm.frame = frame;
    r
}

fn fill_canvas_into(
    vm: &Vm,
    prog: &Program,
    args: &[Value],
    frame: &mut [[u8; 3]],
) -> Result<Value, String> {
    let a = src(vm, prog, arg(args, 0), "fillCanvas")?;
    let b = src(vm, prog, arg(args, 1), "fillCanvas")?;
    let c = src(vm, prog, arg(args, 2), "fillCanvas")?;
    let (cw, ch) = (num(args, 3).to_int_trunc(), num(args, 4).to_int_trunc());
    if cw < 1 || ch < 1 || frame.is_empty() {
        return Ok(Value::default());
    }
    let (cw, ch) = (cw as usize, ch as usize);
    let texel = |i: usize| {
        let [r, g, bb] = hsv_to_rgb(a.at(i), b.at(i), c.at(i));
        [quantize(r), quantize(g), quantize(bb)]
    };
    // Straight copy only on a PROCEDURAL grid: `detect_grid` accepts a
    // coordinate map whose axis values are merely monotonic, not evenly
    // spaced, and there `cell_index(coord(c)) == c` does not have to hold.
    let direct = grid_view(vm, frame.len()).filter(|gv| {
        gv.map.grid.is_some() && gv.g.w as usize == cw && gv.g.h as usize == ch
    });
    match direct {
        Some(gv) => {
            for r in 0..ch {
                for col in 0..cw {
                    frame[gv.g.index(r, col)] = texel(r * cw + col);
                }
            }
        }
        None => {
            for (i, dst) in frame.iter_mut().enumerate() {
                let p = coord_of(vm, i);
                *dst = texel(cell_index(p[1], ch) * cw + cell_index(p[0], cw));
            }
        }
    }
    Ok(Value::default())
}

// ---- grid space ----

/// `blit(hArr, sArr, vArr, w, h, col, row, mode)`: paste a w×h canvas
/// with its top-left at integer cell (col, row), clipped to the grid, and
/// blended with `mode` (3 = keyed, so black is transparent). Sprites,
/// text and scrolling are `blit(..., col - t, row, 3)`. Without a grid
/// map this is a no-op, not an error — `gridWidth()` is how a pattern
/// finds out.
///
/// The grid only has to COVER the frame: `ceil(√n)` over-provisions
/// whenever the strip is not a rectangle (60 px → 8×8), and the tail
/// cells of the last row have no pixel behind them, so they are skipped.
pub(crate) fn blit(vm: &mut Vm, prog: &Program, args: &[Value]) -> Result<Value, String> {
    let mut frame = core::mem::take(&mut vm.frame);
    let r = blit_into(vm, prog, args, &mut frame);
    vm.frame = frame;
    r
}

fn blit_into(
    vm: &Vm,
    prog: &Program,
    args: &[Value],
    frame: &mut [[u8; 3]],
) -> Result<Value, String> {
    let a = src(vm, prog, arg(args, 0), "blit")?;
    let b = src(vm, prog, arg(args, 1), "blit")?;
    let c = src(vm, prog, arg(args, 2), "blit")?;
    let (cw, ch) = (num(args, 3).to_int_trunc(), num(args, 4).to_int_trunc());
    let Some(g) = vm.frame_grid.filter(|g| !g.is_empty() && g.len() >= frame.len()) else {
        return Ok(Value::default());
    };
    if cw < 1 || ch < 1 {
        return Ok(Value::default());
    }
    let (col0, row0) = (
        num(args, 5).to_int_floor() as i64,
        num(args, 6).to_int_floor() as i64,
    );
    let mode = blend(num(args, 7));
    let (cw, ch) = (cw as i64, ch as i64);
    // Clip the SOURCE rectangle to the grid up front rather than skipping
    // cells inside the loop: a pattern is free to pass a 30000-wide canvas
    // or an offset a million cells away, and neither may cost more work
    // than the grid has cells.
    let sr0 = (-row0).max(0);
    let sr1 = ch.min(g.h as i64 - row0);
    let sc0 = (-col0).max(0);
    let sc1 = cw.min(g.w as i64 - col0);
    for sr in sr0..sr1 {
        for sc in sc0..sc1 {
            let i = (sr * cw + sc) as usize;
            let [r, gg, bb] = hsv_to_rgb(a.at(i), b.at(i), c.at(i));
            let px = [quantize(r), quantize(gg), quantize(bb)];
            let cell = g.index((row0 + sr) as usize, (col0 + sc) as usize);
            // The over-provisioned tail of the last row addresses no pixel.
            if let Some(dst) = frame.get_mut(cell) {
                put(dst, px, mode);
            }
        }
    }
    Ok(Value::default())
}

#[cfg(all(test, feature = "frontend"))]
mod tests {
    use super::*;
    use crate::engine::Engine;
    use alloc::vec;
    use alloc::vec::Vec;
    use core::sync::atomic::Ordering;

    /// What map to install before running.
    enum Rig {
        /// No map at all (1D fallback coordinates).
        Strip,
        /// The zero-heap procedural row-major grid (`set_grid_map`).
        Grid(u16, u16),
        /// An explicit coordinate map — `detect_grid` decides whether it
        /// reads as a grid.
        Coords(Vec<[Fx; 3]>),
    }

    /// A W×H row-major (or serpentine) map in host coordinate units.
    fn grid_coords(w: usize, h: usize, serpentine: bool) -> Vec<[Fx; 3]> {
        let mut v = Vec::new();
        for r in 0..h {
            for c in 0..w {
                let x = if serpentine && r % 2 == 1 { w - 1 - c } else { c };
                v.push([Fx::from_int(x as i32), Fx::from_int(r as i32), Fx::ZERO]);
            }
        }
        v
    }

    /// A map that is emphatically NOT a grid: pixels on a circle.
    fn ring_coords(n: usize) -> Vec<[Fx; 3]> {
        (0..n)
            .map(|i| {
                let a = unit_frac(i as u32, n as u32);
                [fmath::cos_turns(a), fmath::sin_turns(a), Fx::ZERO]
            })
            .collect()
    }

    fn build(src: &str, n: u32, rig: &Rig) -> Engine {
        let mut e = Engine::new(src, n, 1).expect("compile");
        match rig {
            Rig::Strip => {}
            Rig::Grid(w, h) => e.set_grid_map(*w, *h),
            Rig::Coords(c) => assert!(e.set_map(2, c)),
        }
        e
    }

    /// Render `count` frames of `src` and return the last one.
    fn frames(src: &str, n: u32, rig: &Rig, count: usize) -> Vec<[u8; 3]> {
        let mut e = build(src, n, rig);
        for _ in 1..count {
            e.frame(Fx::from_int(10));
        }
        let out = e.frame(Fx::from_int(10)).to_vec();
        assert!(e.last_error.is_none(), "{:?}", e.last_error);
        out
    }

    fn frame1(src: &str, n: u32, rig: &Rig) -> Vec<[u8; 3]> {
        frames(src, n, rig, 1)
    }

    /// Wrap a body in the whole-frame entry.
    fn rf(body: &str) -> alloc::string::String {
        alloc::format!("export function renderFrame() {{\n{body}\n}}")
    }

    // ---- index space ----

    #[test]
    fn fill_fade_and_clear() {
        let px = frame1(&rf("rgb(1, .5, 0)\n fill()"), 3, &Rig::Strip);
        assert_eq!(px, vec![[255, 127, 0]; 3]);
        // fade multiplies the OUTPUT bytes and floors, so a trail reaches
        // true black instead of parking at 1
        let px = frame1(&rf("rgb(1, .5, 0)\n fill()\n fade(.5)"), 3, &Rig::Strip);
        assert_eq!(px, vec![[127, 63, 0]; 3]);
        let px = frame1(&rf("rgb(1, 1, 1)\n fill()\n clear()"), 3, &Rig::Strip);
        assert_eq!(px, vec![[0, 0, 0]; 3]);
        // fade(0) is exact black, fade(1) leaves the frame alone
        let px = frame1(&rf("rgb(1, 1, 1)\n fill()\n fade(0)"), 2, &Rig::Strip);
        assert_eq!(px, vec![[0, 0, 0]; 2]);
        let px = frame1(&rf("rgb(1, 1, 1)\n fill()\n fade(2)"), 2, &Rig::Strip);
        assert_eq!(px, vec![[255, 255, 255]; 2]);
    }

    #[test]
    fn brush_starts_black_each_frame_and_is_sticky_within_one() {
        // nothing set this frame → the brush is black, not last frame's red
        let px = frames(&rf("if (time(1) < 0) { rgb(1, 0, 0) }\n fill()"), 2, &Rig::Strip, 3);
        assert_eq!(px, vec![[0, 0, 0]; 2]);
        // one colour call serves every later op in the same frame
        let px = frame1(&rf("rgb(0, 1, 0)\n setPixel(0)\n setPixel(1)"), 2, &Rig::Strip);
        assert_eq!(px, vec![[0, 255, 0]; 2]);
    }

    #[test]
    fn set_pixel_and_fill_range() {
        let px = frame1(&rf("rgb(1, 0, 0)\n setPixel(2)"), 4, &Rig::Strip);
        assert_eq!(px, vec![[0, 0, 0], [0, 0, 0], [255, 0, 0], [0, 0, 0]]);
        // floor, and out of range is a silent no-op
        let px = frame1(
            &rf("rgb(1, 0, 0)\n setPixel(1.9)\n setPixel(-1)\n setPixel(99)"),
            4,
            &Rig::Strip,
        );
        assert_eq!(px[1], [255, 0, 0]);
        assert_eq!(px[0], [0, 0, 0]);
        // [i0, i1): exclusive end, clamped, inverted ranges paint nothing
        let px = frame1(&rf("rgb(0, 0, 1)\n fillRange(1, 3)"), 4, &Rig::Strip);
        assert_eq!(px, vec![[0, 0, 0], [0, 0, 255], [0, 0, 255], [0, 0, 0]]);
        let px = frame1(&rf("rgb(0, 0, 1)\n fillRange(-5, 99)"), 3, &Rig::Strip);
        assert_eq!(px, vec![[0, 0, 255]; 3]);
        let px = frame1(&rf("rgb(0, 0, 1)\n fillRange(3, 1)"), 4, &Rig::Strip);
        assert_eq!(px, vec![[0, 0, 0]; 4]);
    }

    #[test]
    fn fill_hsv_and_rgb_take_scalars_or_arrays() {
        // all scalars: a constant fill
        let px = frame1(&rf("fillHSV(0, 1, 1)"), 3, &Rig::Strip);
        assert_eq!(px, vec![[255, 0, 0]; 3]);
        let px = frame1(&rf("fillRGB(1, .5, 0)"), 2, &Rig::Strip);
        assert_eq!(px, vec![[255, 127, 0]; 2]);
        // arrays index by pixel; a scalar broadcasts alongside them
        let src = "vals = array(4)\n\
                   export function renderFrame() {\n\
                     vals[0] = 0\n vals[1] = .25\n vals[2] = .5\n vals[3] = .75\n\
                     fillHSV(vals, 1, 1)\n\
                   }";
        let px = frame1(src, 4, &Rig::Strip);
        assert_eq!(
            px,
            vec![[255, 0, 0], [127, 255, 0], [0, 255, 255], [127, 0, 255]]
        );
        // a short array bounds the run and leaves the tail untouched
        let src = "vals = array(2)\n\
                   export function renderFrame() {\n\
                     rgb(0, 0, 1)\n fill()\n\
                     vals[0] = 1\n vals[1] = 1\n\
                     fillRGB(vals, 0, 0)\n\
                   }";
        let px = frame1(src, 4, &Rig::Strip);
        assert_eq!(px, vec![[255, 0, 0], [255, 0, 0], [0, 0, 255], [0, 0, 255]]);
    }

    #[test]
    fn fill_hsv_rejects_a_non_array_non_number() {
        let src = "export function f() { }\n\
                   export function renderFrame() { fillHSV(f, 1, 1) }";
        let mut e = build(src, 2, &Rig::Strip);
        e.frame(Fx::from_int(10));
        let err = e.take_error().expect("a type error");
        assert!(err.message.contains("fillHSV"), "{}", err.message);
        // the buffer came back intact even though the op errored
        assert_eq!(e.pixels().len(), 2);
    }

    #[test]
    fn fill_gradient_runs_along_the_index_by_default() {
        // t = i/(n-1): both ENDS of the gradient land on a pixel
        let px = frame1(&rf("fillGradient(0, 1, 1, 1, 1, 1)"), 4, &Rig::Strip);
        assert_eq!(px[0], [255, 0, 0]);
        assert_eq!(px[3], [255, 0, 0]); // hue 1 wraps back to red
        assert_eq!(px[1], [0, 255, 0]); // hue 1/3
        assert_eq!(px[2], [0, 0, 255]); // hue 2/3
        // a single pixel is t = 0, not a divide by zero
        let px = frame1(&rf("fillGradient(0, 1, 1, 1, 1, 1)"), 1, &Rig::Strip);
        assert_eq!(px[0], [255, 0, 0]);
    }

    #[test]
    fn fill_gradient_axis_follows_the_map() {
        // axis 2 = mapped y: every cell of a row shares a colour
        let px = frame1(&rf("fillGradient(0, 1, 1, 1, 1, 1, 2)"), 16, &Rig::Grid(4, 4));
        for r in 0..4 {
            for c in 0..4 {
                assert_eq!(px[r * 4 + c], px[r * 4], "row {r} is not uniform");
            }
        }
        assert_eq!(px[0], [255, 0, 0]);
        assert_ne!(px[4], px[0]);
        // axis 1 = mapped x: every cell of a column shares a colour
        let px = frame1(&rf("fillGradient(0, 1, 1, 1, 1, 1, 1)"), 16, &Rig::Grid(4, 4));
        for r in 0..4 {
            for c in 0..4 {
                assert_eq!(px[r * 4 + c], px[c], "column {c} is not uniform");
            }
        }
    }

    // ---- coordinate space ----

    #[test]
    fn fill_rect_on_a_grid_a_ring_and_a_strip() {
        // the lower-left quadrant of a 4x4 panel
        let px = frame1(&rf("rgb(1, 0, 0)\n fillRect(0, 0, .5, .5)"), 16, &Rig::Grid(4, 4));
        let lit: Vec<usize> = (0..16).filter(|&i| px[i] == [255, 0, 0]).collect();
        assert_eq!(lit, vec![0, 1, 4, 5]);
        // an irregular map has no grid; the predicate is still per pixel
        let ring = ring_coords(8);
        let px = frame1(
            &rf("rgb(1, 0, 0)\n fillRect(0, 0, .5, .5)"),
            8,
            &Rig::Coords(ring.clone()),
        );
        let n_lit = px.iter().filter(|p| **p == [255, 0, 0]).count();
        assert!(n_lit > 0 && n_lit < 8, "{n_lit} of 8 lit");
        // mapless: y is the mid-space 0.5 fill, x is index-normalized —
        // exactly what render2D would receive
        let src = "export function render(i) { }\n\
                   export function renderFrame() { rgb(1, 0, 0)\n fillRect(0, .4, .5, .6) }";
        let px = frame1(src, 4, &Rig::Strip);
        // bounds are INCLUSIVE, so x = 0.5 is inside
        assert_eq!(px, vec![[255, 0, 0], [255, 0, 0], [255, 0, 0], [0, 0, 0]]);
    }

    #[test]
    fn fill_circle_is_a_hard_disc() {
        // radius 0.3 about the centre of a 5x5 panel: the plus-shape
        let px = frame1(
            &rf("rgb(0, 0, 1)\n fillCircle(.5, .5, .3)"),
            25,
            &Rig::Grid(5, 5),
        );
        let lit: Vec<usize> = (0..25).filter(|&i| px[i] != [0, 0, 0]).collect();
        assert_eq!(lit, vec![7, 11, 12, 13, 17]);
        // a negative radius paints nothing
        let px = frame1(&rf("rgb(0, 0, 1)\n fillCircle(.5, .5, -1)"), 25, &Rig::Grid(5, 5));
        assert!(px.iter().all(|p| *p == [0, 0, 0]));
    }

    #[test]
    fn splat_falls_off_linearly_and_honours_modes() {
        // centre cell gets the full brush, the ring at d = 0.25 gets half
        let px = frame1(
            &rf("rgb(1, 1, 1)\n splat(.5, .5, .5, 0)"),
            25,
            &Rig::Grid(5, 5),
        );
        assert_eq!(px[12], [255, 255, 255]);
        assert_eq!(px[11], [127, 127, 127]);
        assert_eq!(px[0], [0, 0, 0]); // corner is d = 0.707 > r
        // add saturates rather than wrapping
        let px = frame1(
            &rf("rgb(1, 1, 1)\n splat(.5, .5, .5, 0)\n splat(.5, .5, .5, 1)"),
            25,
            &Rig::Grid(5, 5),
        );
        assert_eq!(px[11], [254, 254, 254]);
        assert_eq!(px[12], [255, 255, 255]);
        // max keeps the brighter of the two
        let px = frame1(
            &rf("rgb(1, 1, 1)\n splat(.5, .5, .5, 0)\n rgb(.2, .2, .2)\n splat(.5, .5, .5, 2)"),
            25,
            &Rig::Grid(5, 5),
        );
        assert_eq!(px[12], [255, 255, 255]);
        // keyed leaves the destination where the source is black
        let px = frame1(
            &rf("rgb(1, 0, 0)\n fill()\n rgb(0, 0, 0)\n splat(.5, .5, .5, 3)"),
            25,
            &Rig::Grid(5, 5),
        );
        assert_eq!(px[12], [255, 0, 0]);
    }

    #[test]
    fn draw_line_is_a_capsule() {
        // a horizontal line across the middle row of a 5x5 panel
        let px = frame1(
            &rf("rgb(1, 1, 1)\n drawLine(0, .5, 1, .5, .2, 0)"),
            25,
            &Rig::Grid(5, 5),
        );
        for c in 0..5 {
            assert_eq!(px[10 + c], [255, 255, 255], "middle row cell {c}");
        }
        for (c, p) in px[..5].iter().enumerate() {
            assert_eq!(*p, [0, 0, 0], "top row cell {c} should be clear");
        }
        // a zero-length segment degenerates to a disc, not a no-op
        let px = frame1(
            &rf("rgb(1, 1, 1)\n drawLine(.5, .5, .5, .5, .3, 0)"),
            25,
            &Rig::Grid(5, 5),
        );
        assert_eq!(px[12], [255, 255, 255]);
        assert_eq!(px[0], [0, 0, 0]);
        // width 0 paints nothing
        let px = frame1(
            &rf("rgb(1, 1, 1)\n drawLine(0, .5, 1, .5, 0, 0)"),
            25,
            &Rig::Grid(5, 5),
        );
        assert!(px.iter().all(|p| *p == [0, 0, 0]));
    }

    #[test]
    fn fill_canvas_samples_nearest() {
        // a 2x2 canvas on a 4x4 panel: each texel covers a 2x2 block
        let src = "hs = array(4)\n\
                   export function renderFrame() {\n\
                     hs[0] = 0\n hs[1] = .3333\n hs[2] = .6667\n hs[3] = 0\n\
                     fillCanvas(hs, 1, 1, 2, 2)\n\
                   }";
        let px = frame1(src, 16, &Rig::Grid(4, 4));
        for (block, cells) in [
            (0usize, [0usize, 1, 4, 5]),
            (1, [2, 3, 6, 7]),
            (2, [8, 9, 12, 13]),
            (3, [10, 11, 14, 15]),
        ] {
            for i in cells {
                assert_eq!(px[i], px[cells[0]], "block {block} cell {i}");
            }
        }
        assert_eq!(px[0], [255, 0, 0]);
        assert_ne!(px[2], px[0]);
        // a degenerate canvas paints nothing
        let px = frame1(&rf("fillCanvas(0, 1, 1, 0, 0)"), 16, &Rig::Grid(4, 4));
        assert!(px.iter().all(|p| *p == [0, 0, 0]));
    }

    // ---- grid space ----

    #[test]
    fn grid_dims_report_the_installed_grid() {
        let src = "export var gw, gh\n\
                   export function renderFrame() { gw = gridWidth()\n gh = gridHeight() }";
        let mut e = build(src, 12, &Rig::Grid(4, 3));
        e.frame(Fx::from_int(10));
        assert_eq!(e.var("gw"), Some(Value::Num(Fx::from_int(4))));
        assert_eq!(e.var("gh"), Some(Value::Num(Fx::from_int(3))));
        // a ring is not a grid: 0, so a pattern can branch instead of erroring
        let mut e = build(src, 8, &Rig::Coords(ring_coords(8)));
        e.frame(Fx::from_int(10));
        assert_eq!(e.var("gw"), Some(Value::Num(Fx::ZERO)));
        assert_eq!(e.var("gh"), Some(Value::Num(Fx::ZERO)));
    }

    #[test]
    fn blit_pastes_clips_and_keys() {
        // a 2x1 red/green sprite at cell (1, 1) of a 4x4 panel
        let src = "hs = array(2)\n\
                   export function renderFrame() {\n\
                     hs[0] = 0\n hs[1] = .3333\n\
                     blit(hs, 1, 1, 2, 1, 1, 1, 0)\n\
                   }";
        let px = frame1(src, 16, &Rig::Grid(4, 4));
        assert_eq!(px[5], [255, 0, 0]);
        assert_eq!(px[6], [0, 255, 0]);
        assert_eq!(px[4], [0, 0, 0]);
        // negative / off-grid placement clips instead of wrapping or erroring
        let src = "hs = array(2)\n\
                   export function renderFrame() {\n\
                     hs[0] = 0\n hs[1] = .3333\n\
                     blit(hs, 1, 1, 2, 1, -1, 2, 0)\n\
                   }";
        let px = frame1(src, 16, &Rig::Grid(4, 4));
        assert_eq!(px[8], [0, 255, 0]); // only the second texel landed
        assert_eq!(px[9], [0, 0, 0]);
        // keyed: a black texel leaves the destination alone
        let src = "vs = array(2)\n\
                   export function renderFrame() {\n\
                     rgb(0, 0, 1)\n fill()\n\
                     vs[0] = 0\n vs[1] = 1\n\
                     blit(0, 0, vs, 2, 1, 0, 0, 3)\n\
                   }";
        let px = frame1(src, 16, &Rig::Grid(4, 4));
        assert_eq!(px[0], [0, 0, 255]);
        assert_eq!(px[1], [255, 255, 255]);
        // without a grid it is a no-op, not an error
        let px = frame1(
            &rf("rgb(1, 1, 1)\n blit(0, 0, 1, 2, 2, 0, 0, 0)"),
            8,
            &Rig::Coords(ring_coords(8)),
        );
        assert!(px.iter().all(|p| *p == [0, 0, 0]));
    }

    #[test]
    fn blit_cost_is_bounded_by_the_grid_not_the_canvas() {
        // A 30000x30000 canvas may cost no more than the grid has cells:
        // the source rectangle is clipped up front, so this is 16 texels,
        // not 900 million. (Test-visible only as "it finishes".)
        let px = frame1(
            &rf("blit(0, 0, 1, 30000, 30000, -20000, -20000, 0)"),
            16,
            &Rig::Grid(4, 4),
        );
        assert_eq!(px, vec![[255, 255, 255]; 16]);
        // fully off the grid on the other side: nothing painted
        let px = frame1(
            &rf("blit(0, 0, 1, 30000, 30000, 4, 0, 0)"),
            16,
            &Rig::Grid(4, 4),
        );
        assert_eq!(px, vec![[0, 0, 0]; 16]);
    }

    /// The dims-reporting pattern the two grid-coverage tests share.
    const DIMS: &str = "export var gw, gh\n\
                        export function renderFrame() { gw = gridWidth()\n gh = gridHeight() }";

    #[test]
    fn grid_ops_run_on_an_over_provisioned_grid_and_clip_the_tail() {
        // 60 pixels is not a rectangle, so the default map is ceil(√60) = 8
        // wide by 8 tall: 64 cells over a 60-pixel frame. The grid COVERS
        // the frame, so the grid ops run and the dims are reported; the
        // four cells past the end of the last row simply clip.
        let mut e = build(DIMS, 60, &Rig::Strip);
        e.frame(Fx::from_int(10));
        assert_eq!(e.var("gw"), Some(Value::Num(Fx::from_int(8))));
        assert_eq!(e.var("gh"), Some(Value::Num(Fx::from_int(8))));
        // a whole-grid blit paints every pixel — and does not index past 59
        let px = frame1(&rf("blit(0, 0, 1, 8, 8, 0, 0, 0)"), 60, &Rig::Strip);
        assert_eq!(px, vec![[255, 255, 255]; 60]);
        // last row: cell (7, 3) is pixel 59, cell (7, 4) is off the end
        let px = frame1(&rf("blit(0, 0, 1, 1, 1, 3, 7, 0)"), 60, &Rig::Strip);
        assert_eq!(px[59], [255, 255, 255]);
        assert_eq!(px.iter().filter(|p| **p != [0, 0, 0]).count(), 1);
        let px = frame1(&rf("blit(0, 0, 1, 1, 1, 4, 7, 0)"), 60, &Rig::Strip);
        assert!(px.iter().all(|p| *p == [0, 0, 0]));
    }

    #[test]
    fn a_grid_smaller_than_the_frame_is_no_grid_at_all() {
        // 4x4 = 16 cells cannot address a 60-pixel frame, so gridWidth()
        // reads 0 and the grid ops no-op: "gridWidth() == 0 → no
        // grid-space ops" stays exactly true.
        let mut e = build(DIMS, 60, &Rig::Grid(4, 4));
        e.frame(Fx::from_int(10));
        assert_eq!(e.var("gw"), Some(Value::Num(Fx::ZERO)));
        assert_eq!(e.var("gh"), Some(Value::Num(Fx::ZERO)));
        let px = frame1(&rf("blit(0, 0, 1, 4, 4, 0, 0, 0)"), 60, &Rig::Grid(4, 4));
        assert!(px.iter().all(|p| *p == [0, 0, 0]));
    }

    // ---- the fast path is the scan ----

    /// xorshift32 — a deterministic parameter sweep with no dev-dependency.
    struct Rng(u32);
    impl Rng {
        fn next(&mut self) -> u32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            self.0
        }
        /// A coordinate in roughly −0.25..1.25, so shapes straddle the edges.
        fn coord(&mut self) -> f64 {
            (self.next() % 1500) as f64 / 1000.0 - 0.25
        }
        fn radius(&mut self) -> f64 {
            (self.next() % 700) as f64 / 1000.0 + 0.01
        }
    }

    fn scan_and_fast(src: &str, n: u32, rig: &Rig) -> (Vec<[u8; 3]>, Vec<[u8; 3]>) {
        FORCE_SCAN.store(true, Ordering::Relaxed);
        let scan = frame1(src, n, rig);
        FORCE_SCAN.store(false, Ordering::Relaxed);
        let fast = frame1(src, n, rig);
        (scan, fast)
    }

    #[test]
    fn grid_fast_path_is_byte_identical_to_the_generic_scan() {
        let rigs = [
            ("procedural 8x8", Rig::Grid(8, 8), 64u32),
            ("procedural 8x32", Rig::Grid(8, 32), 256),
            ("procedural 1x9", Rig::Grid(1, 9), 9),
            ("coords 8x8", Rig::Coords(grid_coords(8, 8, false)), 64),
            ("coords 8x8 serpentine", Rig::Coords(grid_coords(8, 8, true)), 64),
            ("coords 8x32 serpentine", Rig::Coords(grid_coords(8, 32, true)), 256),
            ("coords 32x8", Rig::Coords(grid_coords(32, 8, false)), 256),
            ("ring (not a grid)", Rig::Coords(ring_coords(64)), 64),
        ];
        let mut rng = Rng(0x1234_5678);
        for (name, rig, n) in &rigs {
            for round in 0..12 {
                let mut body = alloc::string::String::from("clear()\n");
                for _ in 0..4 {
                    let (h, v) = (rng.coord().abs() % 1.0, 0.4 + rng.radius() * 0.5);
                    body.push_str(&alloc::format!("hsv({h:.4}, 1, {v:.4})\n"));
                    match rng.next() % 5 {
                        0 => body.push_str(&alloc::format!(
                            "fillRect({:.4}, {:.4}, {:.4}, {:.4})\n",
                            rng.coord(),
                            rng.coord(),
                            rng.coord(),
                            rng.coord()
                        )),
                        1 => body.push_str(&alloc::format!(
                            "fillCircle({:.4}, {:.4}, {:.4})\n",
                            rng.coord(),
                            rng.coord(),
                            rng.radius()
                        )),
                        2 => body.push_str(&alloc::format!(
                            "splat({:.4}, {:.4}, {:.4}, {})\n",
                            rng.coord(),
                            rng.coord(),
                            rng.radius(),
                            rng.next() % 4
                        )),
                        3 => body.push_str(&alloc::format!(
                            "drawLine({:.4}, {:.4}, {:.4}, {:.4}, {:.4}, {})\n",
                            rng.coord(),
                            rng.coord(),
                            rng.coord(),
                            rng.coord(),
                            rng.radius(),
                            rng.next() % 4
                        )),
                        _ => body.push_str(&alloc::format!(
                            "fillCanvas({:.4}, 1, 1, {}, {})\n",
                            rng.coord().abs() % 1.0,
                            1 + rng.next() % 9,
                            1 + rng.next() % 9
                        )),
                    }
                }
                let src = rf(&body);
                let (scan, fast) = scan_and_fast(&src, *n, rig);
                assert_eq!(scan, fast, "{name} round {round}:\n{src}");
            }
        }
    }

    #[test]
    fn a_transform_routes_the_fast_path_back_to_the_scan() {
        let src = rf("translate(.25, .375)\n\
                      rgb(1, 1, 0)\n fillRect(.1, .1, .9, .5)\n\
                      rgb(0, 1, 1)\n splat(.5, .5, .4, 1)");
        for (name, rig, n) in [
            ("procedural 8x8", Rig::Grid(8, 8), 64u32),
            ("coords 8x8 serpentine", Rig::Coords(grid_coords(8, 8, true)), 64),
        ] {
            let (scan, fast) = scan_and_fast(&src, n, &rig);
            assert_eq!(scan, fast, "{name}");
            // and the transform actually did something
            let plain = frame1(
                &rf("rgb(1, 1, 0)\n fillRect(.1, .1, .9, .5)\n\
                     rgb(0, 1, 1)\n splat(.5, .5, .4, 1)"),
                n,
                &rig,
            );
            assert_ne!(plain, fast, "{name}: the transform was ignored");
        }
    }

    #[test]
    fn fill_canvas_direct_copy_matches_the_scan() {
        // the straight-copy fast path only fires when the canvas is
        // exactly the grid, so pin that case specifically
        let src = "hs = array(64)\n\
                   export function renderFrame() {\n\
                     for (i = 0; i < 64; i++) { hs[i] = i / 64 }\n\
                     fillCanvas(hs, 1, 1, 8, 8)\n\
                   }";
        for (name, rig) in [
            ("procedural", Rig::Grid(8, 8)),
            ("coords", Rig::Coords(grid_coords(8, 8, false))),
            ("coords serpentine", Rig::Coords(grid_coords(8, 8, true))),
        ] {
            let (scan, fast) = scan_and_fast(src, 64, &rig);
            assert_eq!(scan, fast, "{name}");
            assert!(fast.iter().any(|p| *p != [0, 0, 0]), "{name}: nothing painted");
        }
    }
}
