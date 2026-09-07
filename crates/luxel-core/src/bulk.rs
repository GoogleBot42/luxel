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
pub(crate) fn grid_dim(vm: &Vm, axis: usize) -> Fx {
    match vm.frame_grid {
        Some(g) if !g.is_empty() => Fx::from_int(if axis == 0 {
            g.w as i32
        } else {
            g.h as i32
        }),
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
    let Some(g) = vm.frame_grid.filter(|g| !g.is_empty() && g.len() == frame.len()) else {
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
            put(&mut frame[cell], px, mode);
        }
    }
    Ok(Value::default())
}
