//! Layer compositing — the one blend kernel every host runs.
//!
//! The firmware's crossfade (`blend_px`: a timed lerp between the outgoing
//! and the incoming engine) is the degenerate case of this module: two
//! layers, `Blend::Normal`, `Key::None`, opacity = the fade's progress.
//! Generalizing it is Gitea #477; the native text / sprite / colour layers
//! on top of it are #478 and #481.
//!
//! Everything is integer math on RGB888 in ROW-MAJOR CANVAS SPACE: a
//! [`Canvas`] carries the frame plus the [`GridMap`] that says how a
//! (row, col) cell maps onto a pixel index, exactly the way `bulk.rs`'s
//! `paste` addresses a frame. Serpentine wiring, panel rotation and the
//! output chain are then somebody else's problem, as they already are.
//!
//! Per pixel, with `B` beneath, `L` the layer and `α` =
//! `opacity/100 × key factor`:
//!
//! | key     | factor                      |
//! |---------|-----------------------------|
//! | none    | 1                           |
//! | black   | 0 if `L == [0,0,0]` else 1  |
//! | luma    | `luma(L)/255`               |
//!
//! | blend    | result                                    |
//! |----------|-------------------------------------------|
//! | normal   | `B + α(L−B)`                              |
//! | add      | `min(255, B + αL)`                        |
//! | lighten  | `max(B, αL)` per channel                  |
//! | multiply | `B·L/255`, faded back to `B` by `α`       |
//! | mask     | `B·luma(L)/255`, faded back to `B` by `α` |
//!
//! A keyed-out pixel (α = 0) leaves `B` untouched in every mode — which is
//! what makes `black` usable under `add` and `lighten` as well.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use crate::fixed::Fx;
use crate::outpipe::{luma, palette_remap_frame, GridMap};
use crate::scene::{
    Blend, Fit, Key, Keyframe, Layer, LayerBody, LayerKind, LayerStyle, Ramp, Scroll, TextLayer,
    TextSource,
};
use crate::text::{self, Font};

/// Alpha resolution: `α` is carried in 1/65536ths so the `opacity`
/// percentages the crossfade uses (0, 25, 50, 100 →  0, 16384, 32768,
/// 65536) are EXACT and `composite_frame` reproduces `blend_px` bit for
/// bit.
const ONE: i32 = 65536;

/// The destination frame plus the grid that addresses it.
pub struct Canvas<'a> {
    pub px: &'a mut [[u8; 3]],
    pub grid: &'a GridMap,
}

/// `opacity` (0..=100 percent) as a 0..=65536 alpha.
#[inline]
fn opacity_alpha(opacity: u8) -> i32 {
    opacity.min(100) as i32 * ONE / 100
}

/// The key factor for one source pixel, 0..=65536.
///
/// `Luma`'s `luma × 65536 / 255` is spelled as a multiply-add: `65536/255`
/// is `257 + 1/255`, so for `y < 255` the remainder never reaches 1 and the
/// exact quotient is `257·y`; `y == 255` is the single case that carries
/// (`257·255 + 1 == 65536`). Bit-for-bit the division it replaces, with no
/// divide left on a per-pixel path (Gitea #705).
#[inline(always)]
fn key_alpha(key: Key, src: [u8; 3]) -> i32 {
    match key {
        Key::None => ONE,
        Key::Black => {
            if src == [0, 0, 0] {
                0
            } else {
                ONE
            }
        }
        Key::Luma => {
            let y = luma(src) as i32;
            y * 257 + ((y + 1) >> 8)
        }
    }
}

/// The layer's opacity combined with one source pixel's key factor.
///
/// Was `(base as i64 * key_alpha(…) as i64) >> 16`, i.e. a 64-bit multiply
/// per pixel — a libcall on Xtensa. Both operands are ≤ `ONE`, so the only
/// product that does not fit 32 bits is `ONE × ONE`, and every case where
/// either side is `ONE` is just the other side (Gitea #705).
#[inline(always)]
fn layer_alpha(key: Key, src: [u8; 3], base: i32) -> i32 {
    match key {
        Key::None => base,
        _ => {
            let k = key_alpha(key, src);
            if k >= ONE {
                base
            } else if base >= ONE {
                k
            } else {
                ((base as u32 * k as u32) >> 16) as i32
            }
        }
    }
}

/// The blend kernel itself. `#[inline(always)]` so the per-row kernel
/// ([`blend_run`]) carries ONE copy of the five modes with the mode test
/// hoisted where the optimizer can see it is loop-invariant, instead of a
/// flash-resident call per pixel.
#[inline(always)]
fn blend_px_into(dst: &mut [u8; 3], src: [u8; 3], mode: Blend, alpha: i32) {
    if alpha <= 0 {
        return;
    }
    let a = alpha.min(ONE);
    match mode {
        // `b + ((l - b) * a >> 16)` is `(b*(65536-a) + l*a) >> 16`
        // rearranged; the shift is arithmetic, so both floor identically
        // and this IS firmware `blend_px` when a = t.
        Blend::Normal => {
            for c in 0..3 {
                let b = dst[c] as i32;
                dst[c] = (b + (((src[c] as i32 - b) * a) >> 16)) as u8;
            }
        }
        Blend::Add => {
            for c in 0..3 {
                let v = dst[c] as i32 + ((src[c] as i32 * a) >> 16);
                dst[c] = v.min(255) as u8;
            }
        }
        Blend::Lighten => {
            for c in 0..3 {
                let v = (src[c] as i32 * a) >> 16;
                dst[c] = dst[c].max(v as u8);
            }
        }
        Blend::Multiply => {
            for c in 0..3 {
                let b = dst[c] as i32;
                let m = b * src[c] as i32 / 255;
                dst[c] = (b + (((m - b) * a) >> 16)) as u8;
            }
        }
        Blend::Mask => {
            let y = luma(src) as i32;
            for c in 0..3 {
                let b = dst[c] as i32;
                let m = b * y / 255;
                dst[c] = (b + (((m - b) * a) >> 16)) as u8;
            }
        }
    }
}

/// Blend one source pixel into one destination pixel.
///
/// Out of line for the same reason `bulk::put` is: five modes inlined at
/// every call site is five modes' worth of image, several times over, on a
/// board with kilobytes of OTA slot left. Since Gitea #705 the hot paths go
/// through [`blend_run`] instead, which inlines the kernel ONCE; this
/// wrapper is what the cold callers (sprites, the firmware's crossfade)
/// link, so the five modes still exist in exactly two places.
#[inline(never)]
pub fn blend_px_mode(dst: &mut [u8; 3], src: [u8; 3], mode: Blend, alpha: i32) {
    blend_px_into(dst, src, mode, alpha)
}

/// One row of the layer's box blended into one row of the canvas.
///
/// A grid row is a CONTIGUOUS run of the frame — a serpentine row is the
/// same run walked backwards — so the caller resolves the wiring once per
/// row and the loop is two running offsets. No `GridMap::index`, no
/// division, no 64-bit multiply and no call per pixel: at 4096 px that is
/// 64 calls a frame where there were 4096 (Gitea #705).
///
/// `sstep` 0 with a one-element `srow` is the colour-wash case.
#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn blend_run(
    drow: &mut [[u8; 3]],
    doff: usize,
    dstep: isize,
    srow: &[[u8; 3]],
    soff: usize,
    sstep: isize,
    n: usize,
    mode: Blend,
    key: Key,
    base: i32,
) {
    let mut di = doff;
    let mut si = soff;
    for _ in 0..n {
        if let Some(&s) = srow.get(si) {
            let a = layer_alpha(key, s, base);
            if a > 0 {
                if let Some(d) = drow.get_mut(di) {
                    blend_px_into(d, s, mode, a);
                }
            }
        }
        di = di.wrapping_add(dstep as usize);
        si = si.wrapping_add(sstep as usize);
    }
}

/// Where one grid row lives in the frame: the row's start index, the offset
/// of column `c0` inside it, and the step per column. `forward` is false
/// when the caller walks the box's columns in reverse (a mirrored source).
#[inline]
fn row_walk(grid: &GridMap, row: i32, c0: i32, forward: bool) -> (usize, usize, isize) {
    let w = grid.w as usize;
    let rev = grid.serpentine && row & 1 == 1;
    let off = if rev { w - 1 - c0 as usize } else { c0 as usize };
    let step = (if rev { -1isize } else { 1 }) * (if forward { 1 } else { -1 });
    (row as usize * w, off, step)
}

/// The box-local index range on one axis for which BOTH the destination
/// cell (`b + k`) and the source cell (mirrored or not) are inside the
/// layout. Both constraints are intervals, so their intersection is the
/// one range the old per-cell `continue`s walked.
#[inline]
fn clip_axis(b: i32, span: i32, limit: i32, mirrored: bool) -> (i32, i32) {
    let mut lo = (-b).max(0);
    let mut hi = span.min(limit - b);
    if mirrored {
        lo = lo.max(b + span - limit);
        hi = hi.min(b + span);
    }
    (lo.max(0), hi.min(span))
}

/// A clip layer's bookkeeping for ONE draw (docs/spec/scenes.md §2 "Clip
/// layers"): `gate` is the coverage this layer is confined to, `cover`
/// records where it drew (α > 0) for a clip layer above it. Both 1 bit per
/// frame SLOT, so every wiring — row-major, serpentine, a tiled chain —
/// addresses them the same way. `Clip::none()` is every ordinary draw.
pub(crate) struct Clip<'a> {
    gate: Option<&'a [u8]>,
    cover: Option<&'a mut [u8]>,
}

impl Clip<'_> {
    pub(crate) fn none() -> Clip<'static> {
        Clip {
            gate: None,
            cover: None,
        }
    }
    #[inline]
    fn active(&self) -> bool {
        self.gate.is_some() || self.cover.is_some()
    }
    /// May this slot be drawn? Always, without a gate.
    #[inline]
    fn allows(&self, slot: usize) -> bool {
        self.gate
            .is_none_or(|g| g.get(slot >> 3).is_some_and(|b| b >> (slot & 7) & 1 != 0))
    }
    #[inline]
    fn mark(&mut self, slot: usize) {
        if let Some(b) = self.cover.as_deref_mut().and_then(|c| c.get_mut(slot >> 3)) {
            *b |= 1 << (slot & 7);
        }
    }
    /// The whole frame drew (an opaque full-layout copy).
    fn mark_all(&mut self) {
        if let Some(c) = self.cover.as_deref_mut() {
            c.fill(0xff);
        }
    }
}

/// Is this layer's composite a straight frame copy? Full layout, opaque,
/// unkeyed, unmirrored, `normal` — the base layer of almost every scene,
/// and exactly the `emit!` case scenes replaced (Gitea #705).
///
/// `GridMap::index` is a bijection on `0..len`, so "every cell takes the
/// source's value at the same cell" is `copy_from_slice`, whatever the
/// wiring.
#[inline]
fn is_frame_copy(style: &LayerStyle, grid: &GridMap, src_len: usize, dst_len: usize) -> bool {
    let n = grid.len();
    let (bx, by, bw, bh) = resolved_rect(style, grid);
    let (mx, my) = mirrors(style);
    style.opacity >= 100
        && matches!(style.blend, Blend::Normal)
        && matches!(style.key, Key::None)
        && !mx
        && !my
        && bx == 0
        && by == 0
        && bw == grid.w as i32
        && bh == grid.h as i32
        && src_len >= n
        && dst_len >= n
}

/// The layer's box resolved against a grid: `(x0, y0, w, h)` in cells,
/// before clipping. A zero `w`/`h` means the whole layout on that axis.
fn resolved_rect(style: &LayerStyle, grid: &GridMap) -> (i32, i32, i32, i32) {
    let w = if style.rect.w == 0 {
        grid.w as i32
    } else {
        style.rect.w as i32
    };
    let h = if style.rect.h == 0 {
        grid.h as i32
    } else {
        style.rect.h as i32
    };
    (style.rect.x as i32, style.rect.y as i32, w, h)
}

/// `rot180` is `flipx` and `flipy` together, so the three flags collapse
/// to two mirrors.
#[inline]
fn mirrors(style: &LayerStyle) -> (bool, bool) {
    (
        style.flipx ^ style.rot180,
        style.flipy ^ style.rot180,
    )
}

/// True when this style should not draw at all.
#[inline]
fn skip(style: &LayerStyle, grid: &GridMap) -> bool {
    !style.visible || style.opacity == 0 || grid.is_empty()
}

/// Composite a FULL-LAYOUT frame (a pattern layer's engine output) through
/// the layer's box.
///
/// v1 semantics for `fit` on a pattern layer: the box CLIPS a full-layout
/// render — the pattern still sees the whole grid, the layer just shows a
/// window onto it. Flips mirror the window's contents within the box.
pub fn composite_frame(dst: Canvas, src: &[[u8; 3]], style: &LayerStyle) {
    composite_frame_with(dst, src, style, &mut Clip::none());
}

fn composite_frame_with(dst: Canvas, src: &[[u8; 3]], style: &LayerStyle, clip: &mut Clip) {
    let grid = *dst.grid;
    if skip(style, &grid) {
        return;
    }
    // The fast path: a whole-layout opaque `normal` layer IS its source —
    // and covers every slot.
    if clip.gate.is_none() && is_frame_copy(style, &grid, src.len(), dst.px.len()) {
        let n = grid.len();
        dst.px[..n].copy_from_slice(&src[..n]);
        clip.mark_all();
        return;
    }
    blend_box(dst.px, &grid, src, style, false, clip);
}

/// Wash the layer's box with one colour. Colour layers carry no key — the
/// wash is the layer.
pub fn fill_color(dst: Canvas, rgb: [u8; 3], style: &LayerStyle) {
    fill_color_with(dst, rgb, style, &mut Clip::none());
}

fn fill_color_with(dst: Canvas, rgb: [u8; 3], style: &LayerStyle, clip: &mut Clip) {
    let grid = *dst.grid;
    if skip(style, &grid) {
        return;
    }
    blend_box(dst.px, &grid, &[rgb], style, true, clip);
}

/// The general path both kernels share: clip the box once per axis, then
/// hand each row to [`blend_run`] as the contiguous run it is.
///
/// `wash` means `src` is ONE colour rather than a frame — the source offset
/// and step are then 0, and neither the mirrors nor the key apply (a colour
/// layer carries no key; the wash IS the layer). One function rather than
/// two because the row walk is the whole body and the firmware pays for
/// every copy of it.
fn blend_box(
    dst: &mut [[u8; 3]],
    grid: &GridMap,
    src: &[[u8; 3]],
    style: &LayerStyle,
    wash: bool,
    clip: &mut Clip,
) {
    let (bx, by, bw, bh) = resolved_rect(style, grid);
    let (mx, my) = if wash { (false, false) } else { mirrors(style) };
    let key = if wash { Key::None } else { style.key };
    let base = opacity_alpha(style.opacity);
    let (j0, j1) = clip_axis(by, bh, grid.h as i32, my);
    let (i0, i1) = clip_axis(bx, bw, grid.w as i32, mx);
    if j1 <= j0 || i1 <= i0 {
        return;
    }
    // A tiled grid (a HUB75 chain, Gitea #948) has no contiguous rows:
    // every cell goes through `index` on both sides. The row-run path below
    // stays for the row-major and serpentine grids every other host has. A
    // clip layer and the base it clips to take this per-slot path too: the
    // masks are per slot, and an ordinary layer's row kernel stays as it was.
    if grid.tiling.is_some() || clip.active() {
        // In frame-slot runs (#953), so the destination is written in
        // order; an unmirrored source cell IS the destination's slot.
        let rows = ((by + j0) as usize, (by + j1) as usize);
        let cols = ((bx + i0) as usize, (bx + i1) as usize);
        grid.rect_runs(rows, cols, &mut |run| {
            for k in 0..run.n {
                let slot = run.slot + k;
                if !clip.allows(slot) {
                    continue;
                }
                let s = if wash {
                    src[0]
                } else if !mx && !my {
                    match src.get(slot) {
                        Some(&s) => s,
                        None => continue,
                    }
                } else {
                    let (dr, dc) = run.cell(k);
                    let (j, i) = (dr as i32 - by, dc as i32 - bx);
                    let sj = (by + if my { bh - 1 - j } else { j }) as usize;
                    let sc = (bx + if mx { bw - 1 - i } else { i }) as usize;
                    match src.get(grid.index(sj, sc)) {
                        Some(&s) => s,
                        None => continue,
                    }
                };
                let a = layer_alpha(key, s, base);
                if a > 0 {
                    if let Some(d) = dst.get_mut(slot) {
                        blend_px_into(d, s, style.blend, a);
                        clip.mark(slot);
                    }
                }
            }
        });
        return;
    }
    let cols = (i1 - i0) as usize;
    let dc0 = bx + i0;
    let sc0 = bx + if mx { bw - 1 - i0 } else { i0 };
    let w = grid.w as usize;
    for j in j0..j1 {
        let (ds, doff, dstep) = row_walk(grid, by + j, dc0, true);
        // The source row: a frame's own row, or the single wash colour.
        // A frame shorter than the grid loses whole rows rather than the
        // tail of one — nothing on any host renders into a ragged buffer,
        // and it keeps the run below free of per-pixel range math.
        let (srow, soff, sstep) = if wash {
            (src, 0usize, 0isize)
        } else {
            let sj = by + if my { bh - 1 - j } else { j };
            let (ss, so, st) = row_walk(grid, sj, sc0, !mx);
            match src.get(ss..ss + w) {
                Some(r) => (r, so, st),
                None => continue,
            }
        };
        let Some(drow) = dst.get_mut(ds..ds + w) else {
            continue;
        };
        blend_run(drow, doff, dstep, srow, soff, sstep, cols, style.blend, key, base);
    }
}

// ---- sprites ----

/// The sprite record's reader lives in [`crate::sprite`]; it is re-exported
/// here because a sprite layer is a COMPOSITOR concept and every host
/// reaches it through this module.
///
/// A sprite is no longer a pattern: there is no engine, no const pool and
/// no `// @sprite` tag line anywhere on this path (Gitea #740). The record
/// is palette-indexed bytes — flash on the device, the wire body in the
/// console, `localStorage` in the playground — and index 0 is the
/// transparency key, so an opaque BLACK texel draws.
pub use crate::sprite::{
    record_len, SpriteView, SPRITE_HDR, SPRITE_MAGIC, SPRITE_MAX_BYTES, SPRITE_MAX_COLORS,
    SPRITE_MAX_EDGE, SPRITE_MAX_FPS, SPRITE_MAX_FRAMES, SPRITE_MAX_NAME, SPRITE_VERSION,
};

/// Blit one frame of a sprite into the layer's box.
///
/// Transparency is the record's, not the canvas': [`SpriteView::texel`]
/// answers `None` for index 0 and only for index 0, so a texel that
/// happens to be black is painted like any other colour. Whatever the
/// layer's `key` says is therefore irrelevant here — the format already
/// decided which texels exist.
///
/// Fit (contract §2, Gitea #741 item 29):
///
/// * **no box** (`rect.w == 0 || rect.h == 0`) — the sprite's NATURAL size
///   at `(x, y)`, one texel per cell, clipped by the layout.
/// * **`fill`** — nearest-neighbour stretch of the frame to the whole box.
/// * **`contain`** — nearest-neighbour uniform scale, the largest that fits
///   inside the box, centred in it.
/// * **`tile`** — the frame repeated 1:1 across the box.
///
/// `flipx`/`flipy`/`rot180` mirror within the DRAWN extent (the sprite
/// itself when it is placed naturally or contained, the box when it fills
/// or tiles), so a mirrored sprite never moves.
pub fn blit_sprite(dst: Canvas, sprite: &SpriteView, frame: u8, style: &LayerStyle) {
    blit_sprite_with(dst, sprite, frame, style, &mut Clip::none());
}

fn blit_sprite_with(dst: Canvas, sprite: &SpriteView, frame: u8, style: &LayerStyle, clip: &mut Clip) {
    let grid = *dst.grid;
    if skip(style, &grid) || sprite.is_empty() {
        return;
    }
    let (sw, sh) = (sprite.w as i32, sprite.h as i32);
    if sw <= 0 || sh <= 0 {
        return;
    }
    let (bx, by, bw, bh) = resolved_rect(style, &grid);
    // An unset box is not "the whole layout" for a sprite (that is what
    // `resolved_rect` means for a pattern layer): it is the sprite's own
    // size at the layer's origin.
    let natural = style.rect.w == 0 || style.rect.h == 0;
    let tile = !natural && style.fit == Fit::Tile;
    // The extent actually drawn, as an offset inside the box plus a size.
    let (ox, oy, dw, dh) = if natural {
        (0, 0, sw, sh)
    } else {
        match style.fit {
            Fit::Fill | Fit::Tile => (0, 0, bw, bh),
            Fit::Contain => {
                // the larger uniform scale that still fits: compare the
                // aspect ratios as a cross product, no division
                let (dw, dh) = if bw * sh <= bh * sw {
                    (bw, (sh * bw / sw).max(1))
                } else {
                    ((sw * bh / sh).max(1), bh)
                };
                ((bw - dw) / 2, (bh - dh) / 2, dw, dh)
            }
        }
    };
    if dw <= 0 || dh <= 0 {
        return;
    }
    let (mx, my) = mirrors(style);
    let a0 = opacity_alpha(style.opacity);
    let base = (frame as usize % sprite.frames as usize) * sprite.texels();
    // The drawn extent clipped to the grid, walked in frame-slot runs
    // (#953) so a tiled chain is written in order.
    let (y0, x0) = (by + oy, bx + ox);
    let rows = (y0.max(0) as usize, (y0 + dh).clamp(0, grid.h as i32) as usize);
    let cols = (x0.max(0) as usize, (x0 + dw).clamp(0, grid.w as i32) as usize);
    let px_of = |j: i32, i: i32| {
        let jj = if my { dh - 1 - j } else { j };
        let sy = if tile {
            jj.rem_euclid(sh)
        } else if dh == sh {
            jj
        } else {
            (jj * sh / dh).min(sh - 1)
        };
        let ii = if mx { dw - 1 - i } else { i };
        let sx = if tile {
            ii.rem_euclid(sw)
        } else if dw == sw {
            ii
        } else {
            (ii * sw / dw).min(sw - 1)
        };
        // `None` is index 0: the record's transparency
        sprite.texel(base + (sy * sw + sx) as usize)
    };
    grid.rect_runs(rows, cols, &mut |run| {
        for k in 0..run.n {
            let slot = run.slot + k;
            if !clip.allows(slot) {
                continue;
            }
            let (dr, dc) = run.cell(k);
            let Some(px) = px_of(dr as i32 - y0, dc as i32 - x0) else {
                continue;
            };
            if let Some(d) = dst.px.get_mut(slot) {
                blend_px_mode(d, px, style.blend, a0);
                clip.mark(slot);
            }
        }
    });
}

// ---- text ----

/// Draw a text layer into `scratch` (a full-frame buffer the caller owns)
/// and composite it through the layer's box, black-keyed.
///
/// `scroll_px` is the current scroll offset in whole pixels, along
/// whichever axis [`Scroll`] names; [`Compositor::advance`] keeps it.
pub fn draw_text_layer(
    dst: Canvas,
    scratch: &mut [[u8; 3]],
    text: &str,
    layer: &TextLayer,
    style: &LayerStyle,
    scroll_px: i32,
) {
    draw_text_with(dst, scratch, text, layer, style, scroll_px, &mut Clip::none());
}

#[allow(clippy::too_many_arguments)]
fn draw_text_with(
    dst: Canvas,
    scratch: &mut [[u8; 3]],
    text: &str,
    layer: &TextLayer,
    style: &LayerStyle,
    scroll_px: i32,
    clip: &mut Clip,
) {
    let grid = *dst.grid;
    if skip(style, &grid) || text.is_empty() {
        return;
    }
    let (bx, by, bw, _bh) = resolved_rect(style, &grid);
    let tw = text::width(text, layer.font) as i32;
    let anchor = bx + align_off(layer.align, bw, tw);
    let (ox, oy) = if layer.scroll.vertical() {
        (0, scroll_px)
    } else {
        (scroll_px, 0)
    };
    for p in scratch.iter_mut() {
        *p = [0, 0, 0];
    }
    text::draw(
        scratch,
        &grid,
        anchor + ox,
        by + oy,
        text,
        layer.font,
        layer.color,
    );
    // The glyphs are the layer; the space around them is not. Text is
    // therefore always black-keyed, like a sprite.
    let keyed = LayerStyle {
        key: Key::Black,
        ..*style
    };
    composite_frame_with(
        Canvas {
            px: dst.px,
            grid: dst.grid,
        },
        scratch,
        &keyed,
        clip,
    );
}

// ---- the host-facing compositor ----

/// Per-layer runtime state: the clocks and caches the wire record does not
/// carry.
struct LayerRt {
    kind: LayerKind,
    /// The style this frame draws with: the `L` line's, moved by the
    /// keyframe track when the layer has one.
    style: LayerStyle,
    /// The `L` line's own style — what a track's `=` keeps.
    base: LayerStyle,
    /// The keyframe track (`M` lines), empty for a layer that stands still.
    keys: Vec<Keyframe>,
    /// The track's clock in ms, kept below its loop period.
    key_ms: u32,
    /// The sprite frame the track holds right now, if its current key
    /// names one.
    key_frame: Option<u8>,
    /// `X clip`: confined to the coverage of the layer below.
    clip: bool,
    text: TextLayer,
    /// Host-resolved string for this frame (clock/slot are the host's job;
    /// a `lit` source seeds it at [`Compositor::set_scene`]).
    resolved: String,
    color: [u8; 3],
    ramp: Option<Ramp>,
    /// Cooked ramp LUT behind the compositor's scene epoch.
    lut: Option<(u32, Box<[[u8; 3]; 256]>)>,
    /// Scroll phase in milli-pixels, so an integer speed still moves
    /// smoothly at 60 fps.
    scroll_mpx: i64,
    /// Sprite frame clock, in ms since the scene was set.
    sprite_ms: u32,
    /// The layer's frame-rate override (the scene's `A` line), `None` to
    /// play the record's own [`SpriteView::fps`].
    sprite_fps: Option<u8>,
    /// What this slot is showing, for phase carry-over across a
    /// [`Compositor::set_scene`] — see [`layer_ident`].
    ident: u64,
}

/// Identity of one layer for phase carry-over: the scene it belongs to,
/// the layer's kind and its name, FNV-1a'd into a word.
///
/// Hashed rather than stored, because `LayerRt` is per-layer resident RAM
/// on a device that counts bytes and a second owned `String` per layer
/// would not earn its keep. A collision costs a scrolling caption the wrong
/// starting phase and nothing else.
fn layer_ident(scene_id: &str, l: &Layer) -> u64 {
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |b: u8| {
        h ^= b as u64;
        h = h.wrapping_mul(PRIME);
    };
    for b in scene_id.as_bytes() {
        mix(*b);
    }
    mix(0xff);
    mix(l.kind() as u8);
    for b in l.name.as_bytes() {
        mix(*b);
    }
    h
}

/// Owns everything a host must keep between frames to draw a scene: the
/// layer stack, the scroll and sprite clocks, the per-layer ramp LUTs and
/// ONE shared 3 B/px scratch buffer (allocated on first use, released with
/// the scene).
///
/// The scratch is a [`crate::arena::FrameVec`]: on a board with an external
/// arena it comes out of PSRAM beside the engine frames it is composited
/// with, so a text layer costs the internal heap nothing there (Gitea #702's
/// residue — the 12,288 B a 64x64 text layer took out of a heap that had
/// ~12 KB left, which is why the text drew nothing). Everywhere else the
/// arena IS the global allocator and nothing changes.
///
/// The host drives layers bottom → top, calling [`Compositor::pattern_layer`]
/// with that layer's engine frame or [`Compositor::native_layer`] for the
/// text / sprite / colour layers.
pub struct Compositor {
    grid: GridMap,
    layers: Vec<LayerRt>,
    scratch: crate::arena::FrameVec,
    epoch: u32,
    /// The clip coverage mask, 1 bit per slot packed 24 to a `[u8; 3]` —
    /// a [`crate::arena::FrameVec`], so it is sized by the same fallible
    /// [`scratch_for`] as the text scratch and lives in the arena with it.
    /// Allocated the first time a scene with a clip layer draws its base.
    cover: crate::arena::FrameVec,
    /// Which layer's coverage `cover` holds THIS frame ([`COVER_NONE`]
    /// after every [`Compositor::advance`], [`COVER_FAILED`] when the base
    /// could not get the mask). A clip layer whose base did not draw this
    /// frame draws nothing.
    cover_of: u8,
}

/// `Compositor::cover_of`: no base has drawn this frame.
const COVER_NONE: u8 = 0xff;
/// `Compositor::cover_of`: the base could not get its mask — clip layers
/// draw unclipped.
const COVER_FAILED: u8 = 0xfe;

impl Compositor {
    pub fn new(grid: GridMap) -> Self {
        Compositor {
            grid,
            layers: Vec::new(),
            scratch: crate::arena::empty(),
            epoch: 0,
            cover: crate::arena::empty(),
            cover_of: COVER_NONE,
        }
    }

    /// Point the compositor at a different layout (a console whose device
    /// geometry changed). Clears the scratch, which is grid-sized.
    pub fn set_grid(&mut self, grid: GridMap) {
        if self.grid != grid {
            self.grid = grid;
            self.scratch = crate::arena::empty();
            self.cover = crate::arena::empty();
            self.cover_of = COVER_NONE;
        }
    }

    pub fn grid(&self) -> GridMap {
        self.grid
    }

    /// Rebuild the layer runtime state from a scene record. Invalidates
    /// every cached ramp LUT.
    ///
    /// A layer whose [`layer_ident`] is unchanged at its index KEEPS its
    /// scroll phase and sprite clock. Hosts re-install a scene for reasons
    /// that have nothing to do with the clocks — the scene editor rebuilds
    /// the wire on every keystroke — and restarting a scroll on each edit
    /// reads as a stutter in an otherwise steady crawl (Gitea #733).
    /// Everything else about the layer is rebuilt, so an edit to the text,
    /// the speed or the direction still takes effect immediately.
    pub fn set_scene(&mut self, scene: &crate::scene::Scene) {
        self.epoch = self.epoch.wrapping_add(1);
        let had = self.layers.len();
        for (i, l) in scene.layers.iter().enumerate() {
            let mut rt = LayerRt::from(l);
            rt.ident = layer_ident(&scene.id, l);
            if i < had {
                if self.layers[i].ident == rt.ident {
                    rt.scroll_mpx = self.layers[i].scroll_mpx;
                    rt.sprite_ms = self.layers[i].sprite_ms;
                    rt.key_ms = self.layers[i].key_ms;
                }
                self.layers[i] = rt;
            } else {
                self.layers.push(rt);
            }
        }
        self.layers.truncate(scene.layers.len());
        // A host draws the first frame of a freshly set scene before any
        // `advance`: the tracks have to be where their clocks say already.
        self.advance(0);
    }

    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    pub fn layer_kinds(&self) -> impl Iterator<Item = LayerKind> + '_ {
        self.layers.iter().map(|l| l.kind)
    }

    /// One layer's kind, without borrowing the compositor for the length of
    /// a walk — what [`SceneDriver::frame`] dispatches on. (Collecting
    /// `layer_kinds` into a `Vec` first, as the wasm binding used to, is a
    /// per-frame allocation in the render loop.)
    pub fn layer_kind(&self, layer: usize) -> Option<LayerKind> {
        self.layers.get(layer).map(|l| l.kind)
    }

    /// The text a layer will draw this frame. The host resolves
    /// [`TextSource::Clock`] and [`TextSource::Slot`]; a `lit` layer needs
    /// no call.
    pub fn set_text(&mut self, layer: usize, s: &str) {
        if let Some(l) = self.layers.get_mut(layer) {
            l.resolved.clear();
            // Render-loop allocation, so fallible (Gitea #702): a frame that
            // cannot afford 64 bytes draws no text rather than rebooting.
            let want = s.len().min(crate::scene::MAX_NAME);
            if want > l.resolved.capacity() && l.resolved.try_reserve_exact(want).is_err() {
                return;
            }
            push_truncated(&mut l.resolved, s, crate::scene::MAX_NAME);
        }
    }

    /// The text source a layer reads, so the host knows what to resolve.
    pub fn text_source(&self, layer: usize) -> Option<&TextSource> {
        self.layers
            .get(layer)
            .filter(|l| l.kind == LayerKind::Text)
            .map(|l| &l.text.source)
    }

    /// Advance the scroll, sprite-frame and keyframe clocks, and move every
    /// keyed layer to where its track puts it now.
    pub fn advance(&mut self, dt_ms: u32) {
        self.cover_of = COVER_NONE;
        for l in self.layers.iter_mut() {
            if let Some(last) = l.keys.last() {
                let period = last.ms;
                l.key_ms = if period == 0 {
                    0
                } else {
                    // kept below the period so the clock never saturates
                    ((l.key_ms as u64 + dt_ms as u64) % period as u64) as u32
                };
                let at = eval_keys(&l.keys, &l.base, l.key_ms);
                l.style.rect.x = at.x;
                l.style.rect.y = at.y;
                l.style.opacity = at.opacity;
                l.key_frame = at.frame;
            }
            if l.kind == LayerKind::Text && l.text.scroll != Scroll::None && l.text.speed > 0 {
                l.scroll_mpx += l.text.speed as i64 * dt_ms as i64;
            }
            if l.kind == LayerKind::Sprite {
                l.sprite_ms = l.sprite_ms.saturating_add(dt_ms);
            }
        }
    }

    /// The clip role of `layer` this frame, as the masks its draw uses —
    /// `None` when it must not draw at all (a clip layer whose base drew
    /// nothing this frame).
    ///
    /// A BASE is a non-clip layer with a clip layer directly above it: its
    /// mask is cleared and records where it draws. A clip layer reads the
    /// mask of the nearest non-clip layer below it, so a run of clip layers
    /// all clip to one base, as in Photoshop. A clip layer with nothing but
    /// clip layers below it (or none — the first layer) draws unclipped, and
    /// so does one whose base could not afford its mask (#702's rule: a
    /// treatment the heap cannot pay for is dropped, the layer is not).
    #[inline(never)]
    fn clip_for<'c>(
        layers: &[LayerRt],
        cover: &'c mut crate::arena::FrameVec,
        cover_of: &mut u8,
        n: usize,
        layer: usize,
    ) -> Option<Clip<'c>> {
        let is_clip = |i: usize| layers.get(i).is_some_and(|l| l.clip);
        if is_clip(layer) {
            let Some(base) = (0..layer).rev().find(|&i| !is_clip(i)) else {
                return Some(Clip::none());
            };
            return match *cover_of {
                COVER_FAILED => Some(Clip::none()),
                b if b as usize == base => Some(Clip {
                    gate: Some(cover.as_flattened()),
                    cover: None,
                }),
                _ => None,
            };
        }
        if !is_clip(layer + 1) {
            return Some(Clip::none());
        }
        if layer >= COVER_FAILED as usize || !scratch_for(cover, n.div_ceil(24)) {
            *cover_of = COVER_FAILED;
            return Some(Clip::none());
        }
        let bits = cover.as_flattened_mut();
        bits.fill(0);
        *cover_of = layer as u8;
        Some(Clip {
            gate: None,
            cover: Some(bits),
        })
    }

    /// Clear `dst` to black — the base every scene composites onto.
    pub fn begin(&mut self, dst: &mut [[u8; 3]]) {
        for p in dst.iter_mut() {
            *p = [0, 0, 0];
        }
    }

    /// Composite one pattern layer's engine frame: the layer's colour ramp
    /// first (on a scratch copy — the engine's own buffer is never
    /// written), then the blend.
    pub fn pattern_layer(&mut self, dst: &mut [[u8; 3]], layer: usize, src: &[[u8; 3]]) {
        let grid = self.grid;
        let epoch = self.epoch;
        let Compositor {
            layers,
            scratch,
            cover,
            cover_of,
            ..
        } = self;
        let Some(mut clip) = Self::clip_for(layers, cover, cover_of, grid.len(), layer) else {
            return;
        };
        let Some(l) = layers.get_mut(layer) else {
            return;
        };
        let style = l.style;
        let amount = match &l.ramp {
            Some(r) if r.pct > 0 && r.stops.len() >= 2 => r.pct as u32 * 256 / 100,
            _ => 0,
        };
        if amount > 0 {
            let LayerRt { ramp, lut, .. } = l;
            if let Some(r) = ramp.as_ref() {
                ensure_lut(r, lut, epoch);
            }
        }
        let mut from = src;
        if let Some((_, lut)) = l.lut.as_ref().filter(|_| amount > 0) {
            // When the composite is a straight copy the ramp can run in
            // place on the destination: no scratch frame at all, which is
            // 3 B/px this scene then never has to hold (Gitea #705).
            // `palette_remap_frame` is per-pixel and independent, so the
            // result is identical either way.
            let n = grid.len();
            if clip.gate.is_none() && is_frame_copy(&style, &grid, src.len(), dst.len()) {
                dst[..n].copy_from_slice(&src[..n]);
                palette_remap_frame(&mut dst[..n], lut, amount);
                clip.mark_all();
                return;
            }
            // No heap for the scratch this frame (#702) leaves `from` the
            // engine's frame: the ramp is a colour treatment, so drop the
            // TREATMENT rather than the layer — an unramped layer beats a
            // reboot.
            if scratch_for(scratch, src.len()) {
                scratch.copy_from_slice(src);
                palette_remap_frame(scratch, lut, amount);
                from = scratch;
            }
        }
        composite_frame_with(Canvas { px: dst, grid: &grid }, from, &style, &mut clip);
    }

    /// Composite one native layer — text, sprite or colour. Pass the
    /// sprite layer's [`SpriteView`]; the other kinds ignore it.
    pub fn native_layer(&mut self, dst: &mut [[u8; 3]], layer: usize, sprite: Option<&SpriteView>) {
        let grid = self.grid;
        let Compositor {
            layers,
            scratch,
            cover,
            cover_of,
            ..
        } = self;
        let Some(mut clip) = Self::clip_for(layers, cover, cover_of, grid.len(), layer) else {
            return;
        };
        let Some(l) = layers.get(layer) else {
            return;
        };
        let canvas = Canvas { px: dst, grid: &grid };
        match l.kind {
            LayerKind::Color => fill_color_with(canvas, l.color, &l.style, &mut clip),
            LayerKind::Sprite => {
                let Some(sp) = sprite else { return };
                // The fps clock rule lives with the record
                // ([`SpriteView::frame_at_fps`]) so the device, the mirror
                // and the playground cannot each round it differently. The
                // LAYER's `A` rate outranks the record's own, and `A 0`
                // holds frame 0 even for an animated record.
                let fps = l.sprite_fps.unwrap_or(sp.fps);
                // ...and a keyframe that names a frame outranks both
                let frame = match l.key_frame {
                    Some(f) => f,
                    None => sp.frame_at_fps(l.sprite_ms, fps),
                };
                blit_sprite_with(canvas, sp, frame, &l.style, &mut clip);
            }
            LayerKind::Text => {
                let (_, _, bw, bh) = resolved_rect(&l.style, &grid);
                let scroll = scroll_offset(&l.text, &l.resolved, bw, bh, l.scroll_mpx);
                // A grid-sized scratch — 12,288 B on a 64x64 panel — taken
                // INSIDE the host's render loop. Infallibly, that is an
                // allocator panic, i.e. a device reboot, and it is what took
                // the Seengreat panel down on 2026-09-24 (Gitea #702).
                if !scratch_for(scratch, grid.len()) {
                    return;
                }
                draw_text_with(canvas, scratch, &l.resolved, &l.text, &l.style, scroll, &mut clip);
            }
            LayerKind::Pattern => {}
        }
    }

    /// Bytes of the MAIN heap this compositor is holding — the shared frame
    /// scratch (unless an external arena holds it, see
    /// [`crate::arena::frames_external`]) plus the cooked ramp LUTs. The
    /// number a device loses from `heap_free` for a scene.
    pub fn resident_bytes(&self) -> usize {
        let scratch = if crate::arena::frames_external() { 0 } else { self.scratch.capacity() * 3 };
        scratch
            + self.layers.iter().filter(|l| l.lut.is_some()).count() * 768
            + self.layers.len() * core::mem::size_of::<LayerRt>()
            + self.layers.iter().map(|l| l.keys.capacity()).sum::<usize>()
                * core::mem::size_of::<Keyframe>()
            + if crate::arena::frames_external() { 0 } else { self.cover.capacity() * 3 }
    }
}

/// Where a keyframe track puts its layer at `t_ms` — the box origin, the
/// opacity and the sprite frame to hold, if any.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct KeyState {
    pub x: i16,
    pub y: i16,
    pub opacity: u8,
    pub frame: Option<u8>,
}

/// Evaluate a keyframe track (docs/spec/scenes.md §1 "Keyframes").
///
/// The LAST key's `ms` is the loop period, so `t_ms` is taken modulo it.
/// Before the first key the first key's values hold; between two keys the
/// values run from the earlier to the later along the LATER key's ease; a
/// key's `frame` holds from that key until the next one. `=` (a `None`)
/// is `base`'s value. Positions round to whole layout pixels (v1); the
/// opacity clamps to 0..=100 under an overshooting ease.
///
/// An empty track is `base` itself.
pub fn eval_keys(keys: &[Keyframe], base: &LayerStyle, t_ms: u32) -> KeyState {
    let at = |k: &Keyframe| {
        (
            k.x.unwrap_or(base.rect.x) as i32,
            k.y.unwrap_or(base.rect.y) as i32,
            k.opacity.unwrap_or(base.opacity) as i32,
        )
    };
    let Some(last) = keys.last() else {
        return KeyState {
            x: base.rect.x,
            y: base.rect.y,
            opacity: base.opacity,
            frame: None,
        };
    };
    let t = if last.ms == 0 { 0 } else { t_ms % last.ms };
    // the latest key at or before t; before the first key, the first holds
    let i = keys.iter().rposition(|k| k.ms <= t).unwrap_or(0);
    let k0 = &keys[i];
    let (mut x, mut y, mut o) = at(k0);
    // `i` is the LAST key at or before t, so the next one is strictly later
    if let Some(k1) = keys.get(i + 1).filter(|_| k0.ms <= t) {
        // u in 16.16, 0 ≤ u < 1; the u64 is one divide per keyed layer per
        // frame, never per pixel
        let u = (((t - k0.ms) as u64) << 16) / (k1.ms - k0.ms) as u64;
        let e = k1.ease.apply(Fx::from_raw(u as i32)).raw() as i64;
        let (x1, y1, o1) = at(k1);
        let lerp = |a: i32, b: i32| a + (((b - a) as i64 * e + 0x8000) >> 16) as i32;
        x = lerp(x, x1);
        y = lerp(y, y1);
        o = lerp(o, o1);
    }
    let clamp16 = |v: i32| v.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
    KeyState {
        x: clamp16(x),
        y: clamp16(y),
        opacity: o.clamp(0, 100) as u8,
        frame: k0.frame,
    }
}

// ---- the shared full-frame driver (Gitea #732) ----

/// The per-layer facts only the HOST knows, for [`SceneDriver::frame`].
///
/// Everything else about a frame — sizing the destination, turning a frame
/// delta into whole milliseconds, advancing the clocks and walking the
/// stack bottom → top — belongs to the driver, so it is the same code on
/// the device, in the `luxel serve` mirror and in the browser.
///
/// Before #732 each host wrote that walk itself and the copies had already
/// drifted: the wasm binding carried a sub-millisecond accumulator and the
/// firmware truncated `delta` to whole milliseconds every frame, so a
/// caption scrolled measurably slower on the panel than in the preview that
/// was supposed to be showing the panel.
pub trait SceneHost {
    /// Layer `i`'s engine frame for this step, or `None` when the layer has
    /// no engine to draw from — unbound in the console, over budget or
    /// undecodable on the device. Such a layer simply does not draw.
    ///
    /// `delta` is the ENGINE step, not the compositor's whole milliseconds:
    /// hand it to `Engine::frame` unchanged, frame-rate cap and time
    /// scaling included.
    fn pattern_frame(&mut self, layer: usize, delta: Fx) -> Option<&[[u8; 3]]>;

    /// Layer `i`'s sprite record, parsed in place ([`SpriteView::parse`])
    /// out of wherever the host keeps it: mapped flash on the device, an
    /// in-memory record in the mirror and the playground. `None` draws
    /// nothing — an unbound layer, or an id the store no longer holds.
    fn sprite(&mut self, layer: usize) -> Option<SpriteView<'_>>;

    /// The string text layer `i` draws this frame.
    ///
    /// **Clock and slot text are the HOST's to resolve** — the compositor
    /// reads no wall clock and no slot table (docs/spec/scenes.md §2).
    /// `None` leaves the layer's text as it stands, which is what a `lit`
    /// layer (seeded by [`Compositor::set_scene`]) wants, and also what a
    /// host that pushes resolved text in out of band wants (the wasm
    /// binding's `lx_comp_text`).
    fn text(&mut self, layer: usize, source: &TextSource) -> Option<&str> {
        let _ = (layer, source);
        None
    }
}

/// The one full-frame scene driver: every host's render loop is this call.
///
/// It owns the sub-millisecond remainder of the frame deltas, so the
/// compositor's scroll and sprite clocks advance at the true frame rate
/// rather than the truncated one — at 60 fps, 16.67 ms steps used to reach
/// the device's clocks as 16 ms, a 4 % slow crawl that no host could see
/// without comparing against another host.
#[derive(Default)]
pub struct SceneDriver {
    /// Raw 16.16 milliseconds not yet handed to [`Compositor::advance`].
    dt_acc: i32,
}

impl SceneDriver {
    pub const fn new() -> Self {
        SceneDriver { dt_acc: 0 }
    }

    /// Whole milliseconds for a `delta`-long step, carrying the remainder
    /// into the next one.
    pub fn step_ms(&mut self, delta: Fx) -> u32 {
        self.dt_acc = self.dt_acc.saturating_add(delta.raw().max(0));
        let ms = (self.dt_acc >> 16).max(0);
        self.dt_acc -= ms << 16;
        ms as u32
    }

    /// Composite the whole stack into `dst` (`n` pixels, left black where
    /// nothing draws) and return whether the frame was drawn.
    ///
    /// `false` means `dst` could not be sized: on a board whose largest
    /// free block is a few kilobytes the host's staging buffer is exactly
    /// the allocation that fails, and a frame this device cannot afford is
    /// a frame not drawn, not a reboot (Gitea #702, #728). The capacity
    /// survives, so this is one `try_reserve_exact` per activation and a
    /// compare per frame after that — and the `resize` that follows it
    /// leaves `dst` black, which is what [`Compositor::begin`] would do.
    pub fn frame<H: SceneHost + ?Sized>(
        &mut self,
        comp: &mut Compositor,
        dst: &mut crate::arena::FrameVec,
        n: usize,
        delta: Fx,
        host: &mut H,
    ) -> bool {
        let ms = self.step_ms(delta);
        comp.advance(ms);
        for i in 0..comp.layer_count() {
            // `text_source` is `None` for every non-text layer, so this is
            // the text-layer filter as well.
            let Some(source) = comp.text_source(i) else {
                continue;
            };
            if let Some(s) = host.text(i, source) {
                comp.set_text(i, s);
            }
        }
        dst.clear();
        if dst.try_reserve_exact(n).is_err() {
            return false;
        }
        dst.resize(n, [0, 0, 0]);
        for i in 0..comp.layer_count() {
            match comp.layer_kind(i) {
                Some(LayerKind::Pattern) => {
                    if let Some(frame) = host.pattern_frame(i, delta) {
                        comp.pattern_layer(dst, i, frame);
                    }
                }
                Some(LayerKind::Sprite) => {
                    let view = host.sprite(i);
                    comp.native_layer(dst, i, view.as_ref());
                }
                Some(LayerKind::Text) | Some(LayerKind::Color) => {
                    comp.native_layer(dst, i, None)
                }
                None => {}
            }
        }
        true
    }
}

/// Size the shared scratch to `n` pixels without ever panicking.
///
/// Every caller is inside the host's RENDER LOOP, where `luxel-core`'s
/// ordinary `Vec::resize` is an allocator panic — a device reboot — on a
/// heap a scene has already filled (Gitea #702). `false` means "this frame
/// has no scratch"; each caller says what it draws instead.
///
/// The capacity survives, so this is one reservation per scene and a
/// compare per frame after it.
fn scratch_for(scratch: &mut crate::arena::FrameVec, n: usize) -> bool {
    if scratch.len() == n {
        return true;
    }
    scratch.clear();
    if n > scratch.capacity() && scratch.try_reserve_exact(n).is_err() {
        return false;
    }
    scratch.resize(n, [0, 0, 0]);
    true
}

/// Cook a layer's ramp into a 256-entry luma → colour LUT, cached behind
/// the compositor's scene epoch (the `DeviceChain`/`Engine` idiom).
///
/// Fallible for the same reason [`scratch_for`] is: it runs on the first
/// frame a ramped layer draws. A cook that cannot be afforded leaves `lut`
/// as it was, and `pattern_layer` composites the layer unramped.
fn ensure_lut(ramp: &Ramp, lut: &mut Option<(u32, Box<[[u8; 3]; 256]>)>, epoch: u32) {
    if lut.as_ref().map(|(e, _)| *e) == Some(epoch) {
        return;
    }
    // byte domain → 16.16 0..1, the scaling DeviceChain uses for the
    // device palette (no fixed-point divide on this path)
    let b = |v: u8| crate::fixed::Fx::from_raw(((v as i32) << 16) / 255);
    let mut pal: Vec<(crate::fixed::Fx, [crate::fixed::Fx; 3])> = Vec::new();
    if pal.try_reserve_exact(ramp.stops.len()).is_err() {
        return;
    }
    pal.extend(
        ramp.stops
            .iter()
            .map(|(p, c)| (b(*p), [b(c[0]), b(c[1]), b(c[2])])),
    );
    let mut flat: Vec<[u8; 3]> = Vec::new();
    if flat.try_reserve_exact(256).is_err() {
        return;
    }
    flat.resize(256, [0, 0, 0]);
    let Ok(mut cooked) = <Box<[[u8; 3]; 256]>>::try_from(flat.into_boxed_slice()) else {
        return;
    };
    crate::outpipe::fill_palette_lut(&pal, &mut cooked);
    *lut = Some((epoch, cooked));
}

/// Append `s` to `out`, truncated to `max` bytes on a char boundary.
fn push_truncated(out: &mut String, s: &str, max: usize) {
    if s.len() <= max {
        out.push_str(s);
        return;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    out.push_str(&s[..end]);
}

/// Where an unscrolled string sits inside its box, relative to the box's
/// left edge. Shared with [`scroll_offset`] so the two agree on "home":
/// the running arms subtract it straight back out, and [`Scroll::Bounce`]
/// keeps it, because bouncing is motion *about* home.
#[inline]
fn align_off(align: crate::scene::Align, bw: i32, tw: i32) -> i32 {
    match align {
        crate::scene::Align::Left => 0,
        crate::scene::Align::Center => (bw - tw) / 2,
        crate::scene::Align::Right => bw - tw,
    }
}

/// The scroll offset in whole pixels for the current phase.
///
/// Phase 0 puts the string at the edge it ENTERS from, fully outside the
/// box, for all four running directions. Starting it at the alignment
/// anchor instead is what made left- and centre-aligned text "suddenly
/// fully appear on screen" rather than scroll in (Gitea #733).
///
/// The caller adds this to the alignment anchor, so each running arm
/// subtracts [`align_off`] back off: a `left` scroll is one sweep from the
/// right edge to off the left edge, identical for all three alignments.
fn scroll_offset(t: &TextLayer, text: &str, bw: i32, bh: i32, phase_mpx: i64) -> i32 {
    if t.scroll == Scroll::None || t.speed == 0 {
        return 0;
    }
    let px = (phase_mpx / 1000) as i32;
    let tw = text::width(text, t.font) as i32;
    let a = align_off(t.align, bw, tw);
    let lh = line_h(t.font);
    match t.scroll {
        Scroll::None => 0,
        // wrap over the text plus the box so the string leaves the box
        // entirely before it comes back
        Scroll::Left => bw - a - px.rem_euclid((tw + bw).max(1)),
        Scroll::Right => px.rem_euclid((tw + bw).max(1)) - tw - a,
        Scroll::Up => bh - px.rem_euclid((bh + lh).max(1)),
        Scroll::Down => px.rem_euclid((bh + lh).max(1)) - lh,
        // Ping-pong between the two extreme positions at which the string
        // and the box still overlap completely. A string that OVERFLOWS
        // sweeps its overflow, as it always did; one that FITS sweeps the
        // slack *inside* the box instead of standing still, which is the
        // "bounce mode does nothing" report (Gitea #733).
        Scroll::Bounce => {
            let slack = bw - tw;
            let travel = slack.abs();
            if travel == 0 {
                return -a;
            }
            let p = px.rem_euclid(travel * 2);
            let tri = if p <= travel { p } else { travel * 2 - p };
            (if slack >= 0 { tri } else { -tri }) - a
        }
    }
}

/// Nominal line height per font — the vertical scroll wrap distance.
/// Matches the PSF2 cell heights C2 ships.
fn line_h(f: Font) -> i32 {
    match f {
        Font::Tiny => 6,
        Font::Regular => 7,
        Font::Large => 8,
    }
}

impl From<&Layer> for LayerRt {
    fn from(l: &Layer) -> LayerRt {
        let mut rt = LayerRt {
            kind: l.kind(),
            style: l.style,
            base: l.style,
            keys: l.keys.clone(),
            key_ms: 0,
            key_frame: None,
            clip: l.clip,
            text: TextLayer::default(),
            resolved: String::new(),
            color: [0, 0, 0],
            ramp: None,
            lut: None,
            scroll_mpx: 0,
            sprite_ms: 0,
            sprite_fps: None,
            // `set_scene` is the only thing that can know the scene id, so
            // it fills this in; a bare `From` is "no identity yet".
            ident: 0,
        };
        match &l.body {
            LayerBody::Pattern(p) => rt.ramp = p.ramp.clone(),
            LayerBody::Text(t) => {
                rt.text = t.clone();
                if let TextSource::Lit(v) = &t.source {
                    push_truncated(&mut rt.resolved, v, crate::scene::MAX_NAME);
                }
            }
            LayerBody::Color(c) => rt.color = *c,
            LayerBody::Sprite { fps, .. } => rt.sprite_fps = *fps,
        }
        rt
    }
}

#[cfg(test)]
mod tests {
    /// The scratch is what #702 rebooted the panel over, so its shape is a
    /// test: ONE reservation per scene, a compare per frame after it, and
    /// never a fresh allocation just because the grid shrank.
    #[test]
    fn the_render_scratch_is_reserved_once_and_then_only_compared() {
        let mut sc: crate::arena::FrameVec = crate::arena::empty();
        assert!(scratch_for(&mut sc, 4096));
        assert_eq!(sc.len(), 4096);
        let cap = sc.capacity();
        assert!(scratch_for(&mut sc, 4096));
        assert_eq!(sc.capacity(), cap, "a second frame must not reallocate");
        assert!(scratch_for(&mut sc, 16));
        assert_eq!(sc.len(), 16);
        assert_eq!(sc.capacity(), cap, "a smaller grid reuses the allocation");
        assert!(scratch_for(&mut sc, 0));
        assert!(sc.is_empty());
    }

    use super::*;
    use crate::scene::{Align, Rect};
    use alloc::vec;

    fn grid(w: u16, h: u16, serpentine: bool) -> GridMap {
        GridMap::new(w, h, serpentine)
    }

    fn style(blend: Blend, opacity: u8, key: Key) -> LayerStyle {
        LayerStyle {
            blend,
            opacity,
            key,
            ..LayerStyle::default()
        }
    }

    /// The firmware crossfade kernel, verbatim (`firmware/src/main.rs`
    /// `blend_px`) — the thing this module has to generalize without
    /// changing.
    fn blend_px(a: [u8; 3], b: [u8; 3], t: i32) -> [u8; 3] {
        let mix = |x: u8, y: u8| (((x as i32) * (65536 - t) + (y as i32) * t) >> 16) as u8;
        [mix(a[0], b[0]), mix(a[1], b[1]), mix(a[2], b[2])]
    }

    /// Composite `src` over `dst` on a 1x1 grid and read the result.
    fn one(dst: [u8; 3], src: [u8; 3], st: &LayerStyle) -> [u8; 3] {
        let g = grid(1, 1, false);
        let mut px = [dst];
        composite_frame(
            Canvas {
                px: &mut px,
                grid: &g,
            },
            &[src],
            st,
        );
        px[0]
    }

    // ---- the kernels ----

    #[test]
    fn normal_at_full_opacity_replaces() {
        let st = style(Blend::Normal, 100, Key::None);
        assert_eq!(one([9, 9, 9], [1, 2, 3], &st), [1, 2, 3]);
    }

    #[test]
    fn a_two_layer_stack_reproduces_the_crossfade_exactly() {
        // The #477 promise: the persistent blend IS the timed one.
        let a = [200, 17, 3];
        let b = [8, 250, 129];
        for pct in [0u8, 25, 50, 100] {
            let t = pct as i32 * 65536 / 100;
            let g = grid(2, 2, false);
            let mut px = [[0u8; 3]; 4];
            // base layer: the outgoing frame, opaque
            composite_frame(
                Canvas {
                    px: &mut px,
                    grid: &g,
                },
                &[a; 4],
                &style(Blend::Normal, 100, Key::None),
            );
            assert_eq!(px[0], a, "base layer must land verbatim");
            // top layer: the incoming frame at the fade's progress
            composite_frame(
                Canvas {
                    px: &mut px,
                    grid: &g,
                },
                &[b; 4],
                &style(Blend::Normal, pct, Key::None),
            );
            assert_eq!(px[0], blend_px(a, b, t), "at {pct}%");
        }
    }

    #[test]
    fn lighten_never_clips_and_never_darkens() {
        let st = style(Blend::Lighten, 100, Key::None);
        for b in [0u8, 1, 127, 254, 255] {
            for l in [0u8, 1, 127, 254, 255] {
                let got = one([b, b, b], [l, l, l], &st);
                assert_eq!(got, [b.max(l); 3], "b={b} l={l}");
            }
        }
        // a half-opacity lighten scales the source, never the destination
        let half = style(Blend::Lighten, 50, Key::None);
        assert_eq!(one([100, 0, 0], [200, 200, 200], &half), [100, 100, 100]);
    }

    #[test]
    fn multiply_by_white_is_the_identity() {
        let st = style(Blend::Multiply, 100, Key::None);
        for b in 0u8..=255 {
            assert_eq!(one([b, b, b], [255, 255, 255], &st), [b; 3], "b={b}");
        }
        // and by black it is black
        assert_eq!(one([200, 100, 50], [0, 0, 0], &st), [0, 0, 0]);
    }

    #[test]
    fn mask_scales_the_base_by_the_source_luma() {
        let st = style(Blend::Mask, 100, Key::None);
        for src in [[0, 0, 0], [255, 255, 255], [40, 200, 9], [255, 0, 0]] {
            let y = luma(src) as i32;
            let b = [200u8, 100, 33];
            let want = [
                (200 * y / 255) as u8,
                (100 * y / 255) as u8,
                (33 * y / 255) as u8,
            ];
            assert_eq!(one(b, src, &st), want, "src={src:?}");
        }
    }

    #[test]
    fn add_saturates_at_255() {
        let st = style(Blend::Add, 100, Key::None);
        assert_eq!(one([200, 10, 0], [100, 10, 0], &st), [255, 20, 0]);
        let half = style(Blend::Add, 50, Key::None);
        assert_eq!(one([10, 0, 0], [100, 0, 0], &half), [60, 0, 0]);
    }

    #[test]
    fn a_black_key_leaves_the_base_untouched_in_every_mode() {
        // Pure black is the source's transparency, so it must be a no-op
        // even in the modes where an unkeyed black would already change
        // nothing (add, lighten) and in the ones where it would wipe the
        // base out (multiply, mask).
        let base = [200u8, 100, 33];
        // (mode, a source that DOES change the base — proof the key, not
        // the mode, is what spared it above)
        let probes: &[(Blend, [u8; 3])] = &[
            (Blend::Normal, [1, 1, 1]),
            (Blend::Add, [1, 1, 1]),
            (Blend::Lighten, [255, 255, 255]),
            (Blend::Multiply, [1, 1, 1]),
            (Blend::Mask, [1, 1, 1]),
        ];
        for (mode, probe) in probes {
            let st = style(*mode, 100, Key::Black);
            assert_eq!(one(base, [0, 0, 0], &st), base, "{mode:?} keyed");
            assert_ne!(one(base, *probe, &st), base, "{mode:?} probe");
            // …and without the key, black is NOT spared where it matters
            let unkeyed = style(*mode, 100, Key::None);
            if matches!(mode, Blend::Multiply | Blend::Mask | Blend::Normal) {
                assert_eq!(one(base, [0, 0, 0], &unkeyed), [0, 0, 0], "{mode:?} unkeyed");
            }
        }
    }

    #[test]
    fn a_luma_key_makes_alpha_the_source_luma() {
        let st = style(Blend::Normal, 100, Key::Luma);
        let base = [0u8, 0, 0];
        for src in [[0, 0, 0], [8, 8, 8], [130, 20, 90], [255, 255, 255]] {
            let a = luma(src) as i32 * 65536 / 255;
            let want = [
                ((src[0] as i32 * a) >> 16) as u8,
                ((src[1] as i32 * a) >> 16) as u8,
                ((src[2] as i32 * a) >> 16) as u8,
            ];
            assert_eq!(one(base, src, &st), want, "src={src:?}");
        }
        // luma 255 is a full replace, luma 0 is fully transparent
        assert_eq!(one([9, 9, 9], [255, 255, 255], &st), [255, 255, 255]);
        assert_eq!(one([9, 9, 9], [0, 0, 0], &st), [9, 9, 9]);
    }

    #[test]
    fn zero_opacity_and_invisible_draw_nothing() {
        let mut st = style(Blend::Normal, 0, Key::None);
        assert_eq!(one([7, 7, 7], [255, 0, 0], &st), [7, 7, 7]);
        st.opacity = 100;
        st.visible = false;
        assert_eq!(one([7, 7, 7], [255, 0, 0], &st), [7, 7, 7]);
    }

    // ---- geometry ----

    fn wash(g: GridMap, rect: Rect) -> Vec<[u8; 3]> {
        let mut px = vec![[0u8; 3]; g.len()];
        let st = LayerStyle {
            rect,
            ..LayerStyle::default()
        };
        fill_color(
            Canvas {
                px: &mut px,
                grid: &g,
            },
            [255, 255, 255],
            &st,
        );
        px
    }

    #[test]
    fn a_zero_sized_box_means_the_whole_layout() {
        let g = grid(4, 4, false);
        let px = wash(g, Rect { x: 0, y: 0, w: 0, h: 0 });
        assert!(px.iter().all(|p| *p == [255, 255, 255]));
    }

    #[test]
    fn boxes_clip_at_every_edge() {
        let g = grid(4, 4, false);
        // straddling each of the four edges, plus wholly outside
        let cases: &[(Rect, &[usize])] = &[
            (Rect { x: -2, y: 0, w: 3, h: 1 }, &[0]),
            (Rect { x: 3, y: 0, w: 4, h: 1 }, &[3]),
            (Rect { x: 0, y: -2, w: 1, h: 3 }, &[0]),
            (Rect { x: 0, y: 3, w: 1, h: 9 }, &[12]),
            (Rect { x: 9, y: 9, w: 4, h: 4 }, &[]),
            (Rect { x: -9, y: -9, w: 4, h: 4 }, &[]),
        ];
        for (rect, lit) in cases {
            let px = wash(g, *rect);
            let got: Vec<usize> = px
                .iter()
                .enumerate()
                .filter(|(_, p)| **p != [0, 0, 0])
                .map(|(i, _)| i)
                .collect();
            assert_eq!(got, *lit, "rect {rect:?}");
        }
    }

    #[test]
    fn cells_address_the_frame_through_the_grid_map() {
        // Row 1 of a serpentine 4-wide grid runs backwards, so canvas cell
        // (row 1, col 3) is pixel index 4 — composite in ROW-MAJOR canvas
        // space and let the wiring stay the grid's business.
        let g = grid(4, 4, true);
        let px = wash(g, Rect { x: 3, y: 1, w: 1, h: 1 });
        let lit: Vec<usize> = px
            .iter()
            .enumerate()
            .filter(|(_, p)| **p != [0, 0, 0])
            .map(|(i, _)| i)
            .collect();
        assert_eq!(lit, vec![4]);
        assert_eq!(g.index(1, 3), 4);
        // the row-major twin puts it at 7
        let rm = grid(4, 4, false);
        let px = wash(rm, Rect { x: 3, y: 1, w: 1, h: 1 });
        let lit: Vec<usize> = px
            .iter()
            .enumerate()
            .filter(|(_, p)| **p != [0, 0, 0])
            .map(|(i, _)| i)
            .collect();
        assert_eq!(lit, vec![7]);
    }

    #[test]
    fn flips_mirror_within_the_box() {
        let g = grid(4, 1, false);
        let src = [[1u8, 0, 0], [2, 0, 0], [3, 0, 0], [4, 0, 0]];
        let run = |flipx: bool, flipy: bool, rot180: bool| {
            let mut px = [[0u8; 3]; 4];
            let st = LayerStyle {
                flipx,
                flipy,
                rot180,
                ..LayerStyle::default()
            };
            composite_frame(
                Canvas {
                    px: &mut px,
                    grid: &g,
                },
                &src,
                &st,
            );
            px.map(|p| p[0])
        };
        assert_eq!(run(false, false, false), [1, 2, 3, 4]);
        assert_eq!(run(true, false, false), [4, 3, 2, 1]);
        // rot180 on a single row is flipx; and it cancels an explicit flipx
        assert_eq!(run(false, false, true), [4, 3, 2, 1]);
        assert_eq!(run(true, false, true), [1, 2, 3, 4]);
        assert_eq!(run(false, true, false), [1, 2, 3, 4]);
    }

    // ---- sprites ----

    /// Assemble an `LXSP` record by hand — the same shape
    /// `crate::sprite`'s own suite and the web codec build, written out
    /// here so a bug in one cannot hide in the other.
    fn rec(name: &str, w: u8, h: u8, frames: u8, fps: u8, pal: &[[u8; 3]], index: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&SPRITE_MAGIC);
        v.push(SPRITE_VERSION);
        v.extend_from_slice(&[w, h, frames, fps, pal.len() as u8, name.len() as u8, 0]);
        v.extend_from_slice(name.as_bytes());
        for c in pal {
            v.extend_from_slice(c);
        }
        v.extend_from_slice(index);
        v
    }

    const RED: [u8; 3] = [255, 0, 0];
    const BLK: [u8; 3] = [0, 0, 0];
    const BG: [u8; 3] = [9, 9, 9];

    /// A 2x2 checker: red / transparent / transparent / opaque BLACK.
    fn checker() -> Vec<u8> {
        rec("check", 2, 2, 1, 0, &[RED, BLK], &[1, 0, 0, 2])
    }

    fn draw(record: &[u8], w: u16, h: u16, style: &LayerStyle) -> Vec<[u8; 3]> {
        let g = grid(w, h, false);
        let sp = SpriteView::parse(record).expect("record parses");
        let mut px = vec![BG; w as usize * h as usize];
        blit_sprite(
            Canvas {
                px: &mut px,
                grid: &g,
            },
            &sp,
            0,
            style,
        );
        px
    }

    fn boxed(x: i16, y: i16, w: u16, h: u16, fit: Fit) -> LayerStyle {
        LayerStyle {
            rect: Rect { x, y, w, h },
            fit,
            ..LayerStyle::default()
        }
    }

    #[test]
    fn an_unset_box_places_the_sprite_at_its_natural_size() {
        // 2x2 sprite at (1,1) on a 4x4 grid, one texel per cell
        let px = draw(&checker(), 4, 4, &boxed(1, 1, 0, 0, Fit::Fill));
        let at = |x: usize, y: usize| px[y * 4 + x];
        assert_eq!(at(1, 1), RED);
        assert_eq!(at(2, 1), BG, "index 0 is transparent");
        assert_eq!(at(1, 2), BG);
        // the load-bearing change from the sprite-tagged pattern format:
        // an opaque black texel DRAWS
        assert_eq!(at(2, 2), BLK, "opaque black is a colour, not the key");
        // nothing outside the sprite's own 2x2 was touched
        assert_eq!(at(0, 0), BG);
        assert_eq!(at(3, 3), BG);
    }

    #[test]
    fn fill_stretches_the_frame_to_the_box() {
        // a 2x2 checker into a 4x4 box gives 2x2 blocks
        let px = draw(&checker(), 4, 4, &boxed(0, 0, 4, 4, Fit::Fill));
        let at = |x: usize, y: usize| px[y * 4 + x];
        for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            assert_eq!(at(x, y), RED, "({x},{y})");
        }
        for (x, y) in [(2, 2), (3, 2), (2, 3), (3, 3)] {
            assert_eq!(at(x, y), BLK, "({x},{y})");
        }
        for (x, y) in [(2, 0), (3, 1), (0, 2), (1, 3)] {
            assert_eq!(at(x, y), BG, "({x},{y}) is the transparent quadrant");
        }
    }

    #[test]
    fn contain_scales_uniformly_and_centres() {
        // a 2x2 sprite in a 6x4 box: the uniform fit is 4x4, centred, so
        // one blank column each side and 2x2 blocks inside
        let px = draw(&checker(), 6, 4, &boxed(0, 0, 6, 4, Fit::Contain));
        let at = |x: usize, y: usize| px[y * 6 + x];
        for y in 0..4 {
            assert_eq!(at(0, y), BG, "left margin row {y}");
            assert_eq!(at(5, y), BG, "right margin row {y}");
        }
        assert_eq!(at(1, 0), RED);
        assert_eq!(at(2, 1), RED);
        assert_eq!(at(3, 2), BLK);
        assert_eq!(at(4, 3), BLK);
        assert_eq!(at(3, 0), BG, "the transparent quadrant stays transparent");
        // a box the sprite's own aspect ratio is a plain stretch
        let px = draw(&checker(), 4, 4, &boxed(0, 0, 4, 4, Fit::Contain));
        assert_eq!(px, draw(&checker(), 4, 4, &boxed(0, 0, 4, 4, Fit::Fill)));
    }

    #[test]
    fn tile_repeats_the_frame_across_the_box() {
        let px = draw(&checker(), 4, 4, &boxed(0, 0, 4, 4, Fit::Tile));
        let at = |x: usize, y: usize| px[y * 4 + x];
        for y in 0..4 {
            for x in 0..4 {
                let want = match (x % 2, y % 2) {
                    (0, 0) => RED,
                    (1, 1) => BLK,
                    _ => BG,
                };
                assert_eq!(at(x, y), want, "({x},{y})");
            }
        }
    }

    #[test]
    fn flips_mirror_the_drawn_extent_not_its_position() {
        let flipped = LayerStyle {
            flipx: true,
            ..boxed(1, 1, 0, 0, Fit::Fill)
        };
        let px = draw(&checker(), 4, 4, &flipped);
        let at = |x: usize, y: usize| px[y * 4 + x];
        // row 0 of the sprite is [red, transparent] → mirrored to
        // [transparent, red] inside the SAME 2x2 at (1,1)
        assert_eq!(at(1, 1), BG);
        assert_eq!(at(2, 1), RED);
        assert_eq!(at(1, 2), BLK);
        assert_eq!(at(2, 2), BG);
        assert_eq!(at(0, 0), BG, "the sprite did not move");
    }

    /// A host for [`SceneDriver`] holding one sprite record — the frame
    /// clock has to come out of the driver, not out of a second copy of
    /// the fps rule.
    struct SpriteHost(Vec<u8>);

    impl SceneHost for SpriteHost {
        fn pattern_frame(&mut self, _layer: usize, _delta: Fx) -> Option<&[[u8; 3]]> {
            None
        }
        fn sprite(&mut self, _layer: usize) -> Option<SpriteView<'_>> {
            SpriteView::parse(&self.0)
        }
    }

    #[test]
    fn the_frame_clock_runs_through_the_driver() {
        // 1x2 sprite, two frames at 10 fps: frame 0 lights texel 0, frame 1
        // lights texel 1
        let record = rec("blink", 2, 1, 2, 10, &[RED], &[1, 0, 0, 1]);
        let scene = crate::scene::parse(
            "S 0000000a s\nL sprite 0 0 0 0 normal 100 none fill 1\nI 5b17e5ef\n",
        )
        .unwrap();
        let mut comp = Compositor::new(grid(2, 1, false));
        comp.set_scene(&scene);
        let mut driver = SceneDriver::new();
        let mut host = SpriteHost(record);
        let mut px: crate::arena::FrameVec = crate::arena::empty();
        // 60 fps steps, so the driver's remainder carry is exercised too
        let step = Fx::from_raw((1000 << 16) / 60);
        let mut seen: Vec<usize> = Vec::new();
        for _ in 0..13 {
            assert!(driver.frame(&mut comp, &mut px, 2, step, &mut host));
            seen.push(if px[0] == RED { 0 } else { 1 });
        }
        // 16.67 ms a frame: texel 0 for the first 6 frames (0..100 ms),
        // texel 1 for the next 6, then back
        assert_eq!(seen, vec![0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 0]);
    }

    /// The scene's `A <fps>` outranks the record's own rate, through the
    /// same driver clock.
    #[test]
    fn the_layers_fps_override_outranks_the_record() {
        // the same 2-frame, 10 fps record as above
        let record = rec("blink", 2, 1, 2, 10, &[RED], &[1, 0, 0, 1]);
        // frames drawn over `ms` of playback at 60 fps steps
        let run = |binding: &str, frames: usize| -> Vec<usize> {
            let mut wire =
                String::from("S 0000000a s\nL sprite 0 0 0 0 normal 100 none fill 1\nI 5b17e5ef\n");
            wire.push_str(binding);
            let scene = crate::scene::parse(&wire).expect("parses");
            let mut comp = Compositor::new(grid(2, 1, false));
            comp.set_scene(&scene);
            let mut driver = SceneDriver::new();
            let mut host = SpriteHost(record.clone());
            let mut px: crate::arena::FrameVec = crate::arena::empty();
            let step = Fx::from_raw((1000 << 16) / 60);
            let mut seen = Vec::new();
            for _ in 0..frames {
                assert!(driver.frame(&mut comp, &mut px, 2, step, &mut host));
                seen.push(if px[0] == RED { 0 } else { 1 });
            }
            seen
        };
        // `A 2` over a 10 fps record: 500 ms a frame, i.e. 30 steps of 16.67 ms
        let seen = run("A 2\n", 62);
        assert_eq!(seen[..30], [0; 30], "frame 0 holds for the first 500 ms");
        assert_eq!(seen[30..60], [1; 30], "then frame 1 for 500 ms");
        assert_eq!(seen[60..], [0, 0], "and it wraps");
        // `A 0` is a still: frame 0 forever, whatever the record says
        assert_eq!(run("A 0\n", 90), vec![0; 90]);
        // absent: the record's own 10 fps, unchanged from the test above
        assert_eq!(
            run("", 13),
            vec![0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 0],
            "no A line leaves the record in charge"
        );
        // …and an override ABOVE the record's rate speeds it up
        assert_eq!(run("A 30\n", 5), vec![0, 0, 1, 1, 0], "30 fps: ~33 ms a frame");
    }

    #[test]
    fn a_sprite_layer_with_no_record_draws_nothing() {
        let all_clear = rec("blank", 2, 2, 1, 0, &[], &[0, 0, 0, 0]);
        let px = draw(&all_clear, 2, 2, &LayerStyle::default());
        assert!(px.iter().all(|p| *p == BG));
    }

    // ---- the compositor ----

    #[test]
    fn begin_clears_and_colour_layers_wash() {
        let scene = crate::scene::parse(concat!(
            "S 0000000a s\n",
            "L color 0 0 0 0 normal 100 none fill 1\n",
            "K 102030\n",
            "L color 1 0 1 1 normal 100 none fill 1\n",
            "K ffffff\n",
        ))
        .unwrap();
        let mut c = Compositor::new(grid(2, 1, false));
        c.set_scene(&scene);
        assert_eq!(c.layer_count(), 2);
        let kinds: Vec<LayerKind> = c.layer_kinds().collect();
        assert_eq!(kinds, vec![LayerKind::Color, LayerKind::Color]);
        let mut px = vec![[7u8, 7, 7]; 2];
        c.begin(&mut px);
        assert_eq!(px, vec![[0, 0, 0], [0, 0, 0]]);
        c.native_layer(&mut px, 0, None);
        c.native_layer(&mut px, 1, None);
        assert_eq!(px, vec![[0x10, 0x20, 0x30], [255, 255, 255]]);
    }

    #[test]
    fn a_ramp_recolours_the_layer_before_it_composites() {
        // a two-stop black→red ramp at 100 % maps luma onto red
        let scene = crate::scene::parse(concat!(
            "S 0000000a s\n",
            "L pat 0 0 0 0 normal 100 none fill 1\n",
            "I 0123abcd\n",
            "R 100 0:000000 255:ff0000\n",
        ))
        .unwrap();
        let mut c = Compositor::new(grid(2, 1, false));
        c.set_scene(&scene);
        let src = [[255u8, 255, 255], [0, 0, 0]];
        let mut px = vec![[0u8; 3]; 2];
        c.begin(&mut px);
        c.pattern_layer(&mut px, 0, &src);
        assert_eq!(px[0], [255, 0, 0]);
        assert_eq!(px[1], [0, 0, 0]);
        assert!(c.resident_bytes() >= 768, "the cooked LUT is resident");

        // without a ramp the engine frame passes through untouched
        let plain = crate::scene::parse(
            "S 0000000a s\nL pat 0 0 0 0 normal 100 none fill 1\nI 0123abcd\n",
        )
        .unwrap();
        c.set_scene(&plain);
        c.begin(&mut px);
        c.pattern_layer(&mut px, 0, &src);
        assert_eq!(px, vec![[255, 255, 255], [0, 0, 0]]);
    }

    #[test]
    fn set_text_truncates_on_a_char_boundary() {
        let scene = crate::scene::parse(
            "S 0000000a s\nL text 0 0 0 0 normal 100 none fill 1\nT slot 0\n",
        )
        .unwrap();
        let mut c = Compositor::new(grid(8, 8, false));
        c.set_scene(&scene);
        assert!(matches!(c.text_source(0), Some(TextSource::Slot(0))));
        let long: String = core::iter::repeat('\u{2603}').take(40).collect(); // 120 bytes
        // 64 is not a char boundary in this string, so it backs off to 63
        let mut cut = String::new();
        push_truncated(&mut cut, &long, crate::scene::MAX_NAME);
        assert_eq!(cut.len(), 63);
        assert_eq!(cut.chars().count(), 21);

        c.set_text(0, &long);
        let mut px = vec![[0u8; 3]; 64];
        c.begin(&mut px);
        c.native_layer(&mut px, 0, None);
        // the snowman is outside the ASCII sheet, so every glyph draws `?` — what
        // matters here is that the truncated text reached the kernel at all
        assert!(px.iter().any(|p| *p != [0, 0, 0]));
    }

    #[test]
    fn scroll_phase_advances_only_for_a_scrolling_layer() {
        let t = TextLayer {
            scroll: Scroll::Left,
            speed: 10,
            ..TextLayer::default()
        };
        // "HI" is 12 px in the default face, so the wrap is 12 + the box
        assert_eq!(text::width("HI", t.font), 12);
        // phase 0 is the string just off the RIGHT edge of an 8 px box
        assert_eq!(scroll_offset(&t, "HI", 8, 8, 0), 8);
        assert_eq!(scroll_offset(&t, "HI", 8, 8, 3_000), 5);
        assert_eq!(scroll_offset(&t, "HI", 8, 8, 19_000), -11);
        assert_eq!(scroll_offset(&t, "HI", 8, 8, 20_000), 8); // wrapped
        let still = TextLayer::default();
        assert_eq!(scroll_offset(&still, "HI", 8, 8, 9_999), 0);
    }

    /// Gitea #733 (1): a string that FITS its box used to make `bounce`
    /// return a constant 0 — "bounce mode does nothing".
    #[test]
    fn bounce_oscillates_whether_or_not_the_text_fits() {
        let bounce = |align, bw, phase_mpx| {
            let t = TextLayer {
                scroll: Scroll::Bounce,
                speed: 10,
                align,
                ..TextLayer::default()
            };
            scroll_offset(&t, "HI", bw, 8, phase_mpx)
        };
        // "HI" is 12 px. In a 20 px box it has 8 px of slack and sweeps it.
        let fits: Vec<i32> = (0..=16).map(|p| bounce(Align::Left, 20, p * 1000)).collect();
        assert_eq!(
            fits,
            vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 7, 6, 5, 4, 3, 2, 1, 0],
            "a fitting string must ping-pong across the slack, not sit still"
        );
        assert!(fits.iter().all(|o| (0..=8).contains(o)), "and stay in the box");

        // Centred, the same sweep is measured from the centred anchor, so
        // the string still covers exactly the same 8 px of travel.
        let mid: Vec<i32> = (0..=16).map(|p| bounce(Align::Center, 20, p * 1000)).collect();
        assert_eq!(mid, fits.iter().map(|o| o - 4).collect::<Vec<_>>());

        // An OVERFLOWING string keeps the old behaviour: it sweeps its
        // overflow, and never uncovers an edge of the box.
        let over: Vec<i32> = (0..=8).map(|p| bounce(Align::Left, 8, p * 1000)).collect();
        assert_eq!(over, vec![0, -1, -2, -3, -4, -3, -2, -1, 0]);

        // A string that exactly fills its box has nowhere to go.
        assert_eq!(bounce(Align::Left, 12, 5_000), 0);
    }

    /// Gitea #733 (2): "the text just suddenly fully appears on screen".
    /// Every running direction must start the string OUTSIDE the box.
    #[test]
    fn a_running_scroll_starts_fully_off_the_box() {
        let bw = 16;
        let bh = 8;
        for align in [Align::Left, Align::Center, Align::Right] {
            for (dir, entering) in [
                (Scroll::Left, bw),   // in from the right edge
                (Scroll::Right, -12), // in from the left edge, text is 12 px
            ] {
                let t = TextLayer {
                    scroll: dir,
                    speed: 10,
                    align,
                    ..TextLayer::default()
                };
                // the offset is added to the ALIGNMENT ANCHOR, so that is
                // what "off the box" has to be measured from
                let x = align_off(align, bw, 12) + scroll_offset(&t, "HI", bw, bh, 0);
                assert_eq!(x, entering, "{dir:?} at {align:?}");
                // fully outside: either right of the last column, or left
                // of the first by the whole string
                assert!(x >= bw || x + 12 <= 0, "{dir:?} at {align:?}: x={x}");
            }
            // and the sweep is the same path whatever the alignment
            let t = TextLayer {
                scroll: Scroll::Left,
                speed: 10,
                align,
                ..TextLayer::default()
            };
            let path: Vec<i32> = (0..28)
                .map(|p| align_off(align, bw, 12) + scroll_offset(&t, "HI", bw, bh, p * 1000))
                .collect();
            assert_eq!(path[0], bw);
            assert_eq!(*path.last().unwrap(), bw - 27);
        }
        // vertical: `by + scroll` is the text's top, with no align term
        let up = TextLayer {
            scroll: Scroll::Up,
            speed: 10,
            ..TextLayer::default()
        };
        assert_eq!(scroll_offset(&up, "HI", bw, bh, 0), bh, "up starts below");
        let down = TextLayer {
            scroll: Scroll::Down,
            speed: 10,
            ..TextLayer::default()
        };
        assert_eq!(
            scroll_offset(&down, "HI", bw, bh, 0),
            -line_h(Font::Regular),
            "down starts above"
        );
    }

    /// Gitea #733 (3): the editor rebuilds the wire on every keystroke, so
    /// a `set_scene` that does not change a layer's identity must not snap
    /// its scroll back to the start.
    #[test]
    fn scroll_phase_survives_a_set_scene_that_keeps_the_layer() {
        let wire = |text: &str| {
            crate::scene::parse(&alloc::format!(
                concat!(
                    "S 0000000a s\n",
                    "L text 0 0 0 0 normal 100 none fill 1\n",
                    "N Caption\n",
                    "T lit {}\n",
                    "F regular ffffff l left 10\n",
                ),
                text
            ))
            .unwrap()
        };
        let mut c = Compositor::new(grid(16, 8, false));
        c.set_scene(&wire("HI"));
        c.advance(500); // 10 px/s for 500 ms = 5 px
        assert_eq!(c.layers[0].scroll_mpx, 5_000);

        // the user types another character: same scene, same layer, new text
        c.set_scene(&wire("HIT"));
        assert_eq!(
            c.layers[0].scroll_mpx, 5_000,
            "an edit that leaves the layer in place must not restart the scroll"
        );
        // ... and the edit still took effect
        assert_eq!(c.layers[0].resolved, "HIT");

        // renaming the layer IS a new layer, and starts over
        let renamed = crate::scene::parse(concat!(
            "S 0000000a s\n",
            "L text 0 0 0 0 normal 100 none fill 1\n",
            "N Other\n",
            "T lit HIT\n",
            "F regular ffffff l left 10\n",
        ))
        .unwrap();
        c.set_scene(&renamed);
        assert_eq!(c.layers[0].scroll_mpx, 0);

        // so is the same-shaped layer in a DIFFERENT scene
        c.advance(500);
        let other_scene = crate::scene::parse(concat!(
            "S 0000000b s\n",
            "L text 0 0 0 0 normal 100 none fill 1\n",
            "N Other\n",
            "T lit HIT\n",
            "F regular ffffff l left 10\n",
        ))
        .unwrap();
        c.set_scene(&other_scene);
        assert_eq!(c.layers[0].scroll_mpx, 0);

        // and a layer that changes KIND under the same name starts over too
        let colour = crate::scene::parse(concat!(
            "S 0000000b s\n",
            "L color 0 0 0 0 normal 100 none fill 1\n",
            "N Other\n",
        ))
        .unwrap();
        c.set_scene(&colour);
        assert_eq!(c.layer_count(), 1);
        assert_eq!(c.layers[0].sprite_ms, 0);
    }

    /// The phase is milli-pixels advanced by ELAPSED TIME, so the position
    /// at a given wall-clock time does not depend on how the frames were
    /// chopped up. Pinning that is what lets #733's jitter report be
    /// blamed on the phase RESET rather than on the clock.
    #[test]
    fn the_scroll_phase_is_frame_rate_independent() {
        let scene = crate::scene::parse(concat!(
            "S 0000000a s\n",
            "L text 0 0 0 0 normal 100 none fill 1\n",
            "T lit HI\n",
            "F regular ffffff l left 37\n",
        ))
        .unwrap();
        let run = |steps: &[u32]| {
            let mut c = Compositor::new(grid(16, 8, false));
            c.set_scene(&scene);
            for dt in steps {
                c.advance(*dt);
            }
            c.layers[0].scroll_mpx
        };
        let smooth: Vec<u32> = vec![10; 120];
        let lumpy = [1u32, 200, 3, 47, 99, 150, 2, 98, 200, 1, 199];
        assert_eq!(smooth.iter().sum::<u32>(), 1200);
        assert_eq!(lumpy.iter().sum::<u32>(), 1000);
        assert_eq!(run(&smooth), 37 * 1200);
        assert_eq!(run(&lumpy), 37 * 1000);
    }

    #[test]
    fn align_is_carried_by_the_record() {
        assert_eq!(Align::Center.as_str(), "c");
    }

    // ---- the #705 fast paths ----

    /// `composite_frame` exactly as it was before Gitea #705: two
    /// `GridMap::index` calls, a per-pixel `key_alpha` with its division and
    /// a 64-bit alpha multiply, per cell. The ONLY oracle that matters for
    /// the rewrite — the kernels above are pinned bit-for-bit by the
    /// truth-table tests, and this pins the geometry and the alpha algebra.
    fn composite_frame_naive(dst: Canvas, src: &[[u8; 3]], style: &LayerStyle) {
        let grid = *dst.grid;
        if skip(style, &grid) {
            return;
        }
        let (bx, by, bw, bh) = resolved_rect(style, &grid);
        let (mx, my) = mirrors(style);
        let base = opacity_alpha(style.opacity);
        let key_alpha_div = |key: Key, s: [u8; 3]| -> i32 {
            match key {
                Key::None => ONE,
                Key::Black => {
                    if s == [0, 0, 0] {
                        0
                    } else {
                        ONE
                    }
                }
                Key::Luma => luma(s) as i32 * ONE / 255,
            }
        };
        for j in 0..bh {
            let dr = by + j;
            if dr < 0 || dr >= grid.h as i32 {
                continue;
            }
            let sj = by + if my { bh - 1 - j } else { j };
            if sj < 0 || sj >= grid.h as i32 {
                continue;
            }
            for i in 0..bw {
                let dc = bx + i;
                if dc < 0 || dc >= grid.w as i32 {
                    continue;
                }
                let si = bx + if mx { bw - 1 - i } else { i };
                if si < 0 || si >= grid.w as i32 {
                    continue;
                }
                let Some(&s) = src.get(grid.index(sj as usize, si as usize)) else {
                    continue;
                };
                let a = (base as i64 * key_alpha_div(style.key, s) as i64 >> 16) as i32;
                if let Some(d) = dst.px.get_mut(grid.index(dr as usize, dc as usize)) {
                    blend_px_mode(d, s, style.blend, a);
                }
            }
        }
    }

    /// The same for `fill_color`.
    fn fill_color_naive(dst: Canvas, rgb: [u8; 3], style: &LayerStyle) {
        let grid = *dst.grid;
        if skip(style, &grid) {
            return;
        }
        let (bx, by, bw, bh) = resolved_rect(style, &grid);
        let a = opacity_alpha(style.opacity);
        for j in 0..bh {
            let dr = by + j;
            if dr < 0 || dr >= grid.h as i32 {
                continue;
            }
            for i in 0..bw {
                let dc = bx + i;
                if dc < 0 || dc >= grid.w as i32 {
                    continue;
                }
                if let Some(d) = dst.px.get_mut(grid.index(dr as usize, dc as usize)) {
                    blend_px_mode(d, rgb, style.blend, a);
                }
            }
        }
    }

    /// Deterministic filler — every channel value appears, including the
    /// 0 and 255 the key and the luma rounding turn on.
    fn frame(n: usize, seed: u32) -> Vec<[u8; 3]> {
        let mut s = seed | 1;
        (0..n)
            .map(|_| {
                let mut c = [0u8; 3];
                for ch in c.iter_mut() {
                    s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    *ch = match (s >> 24) & 7 {
                        0 => 0,
                        1 => 255,
                        _ => (s >> 16) as u8,
                    };
                }
                c
            })
            .collect()
    }

    #[test]
    fn the_fast_paths_are_bit_identical_to_the_naive_kernel() {
        let rects: &[Rect] = &[
            Rect { x: 0, y: 0, w: 0, h: 0 },   // the whole layout
            Rect { x: 1, y: 2, w: 4, h: 3 },   // a clipped box
            Rect { x: -3, y: -2, w: 6, h: 5 }, // straddling the origin
            Rect { x: 4, y: 3, w: 9, h: 9 },   // straddling the far corner
            Rect { x: 40, y: 40, w: 4, h: 4 }, // wholly outside
            Rect { x: 0, y: 0, w: 1, h: 1 },   // one cell
        ];
        let modes = [Blend::Normal, Blend::Add, Blend::Lighten, Blend::Multiply, Blend::Mask];
        let keys = [Key::None, Key::Black, Key::Luma];
        let mut cases = 0usize;
        let mut grids = alloc::vec::Vec::new();
        for (w, h) in [(8u16, 6u16), (7, 7), (1, 9)] {
            for serpentine in [false, true] {
                grids.push(grid(w, h, serpentine));
            }
        }
        // HUB75 walls in driver order (#948): the tiled paths (#953)
        for turns in [[3u8, 3, 1, 1], [0, 2, 1, 3]] {
            let chain = [(1, 0, turns[0]), (1, 1, turns[1]), (0, 1, turns[2]), (0, 0, turns[3])];
            grids.push(GridMap::tiled(crate::outpipe::Tiling::new(4, 4, 2, 2, 0, &chain).unwrap().leak()));
        }
        for g in grids {
            {
                let (w, h, serpentine) = (g.w, g.h, g.serpentine);
                let n = g.len();
                let src = frame(n, 0x5eed_0001 ^ (w as u32) << 8);
                let under = frame(n, 0xc0ff_ee01 ^ (h as u32) << 8);
                for rect in rects {
                    for &blend in &modes {
                        for &key in &keys {
                            for &opacity in &[0u8, 1, 37, 99, 100] {
                                for flags in 0u8..8 {
                                    let st = LayerStyle {
                                        rect: *rect,
                                        blend,
                                        opacity,
                                        key,
                                        flipx: flags & 1 != 0,
                                        flipy: flags & 2 != 0,
                                        rot180: flags & 4 != 0,
                                        ..LayerStyle::default()
                                    };
                                    let mut fast = under.clone();
                                    let mut naive = under.clone();
                                    composite_frame(
                                        Canvas { px: &mut fast, grid: &g },
                                        &src,
                                        &st,
                                    );
                                    composite_frame_naive(
                                        Canvas { px: &mut naive, grid: &g },
                                        &src,
                                        &st,
                                    );
                                    assert_eq!(
                                        fast, naive,
                                        "composite {w}x{h} serp={serpentine} {rect:?} \
                                         {blend:?} {key:?} op={opacity} flags={flags}"
                                    );
                                    let mut fast = under.clone();
                                    let mut naive = under.clone();
                                    fill_color(
                                        Canvas { px: &mut fast, grid: &g },
                                        [9, 200, 71],
                                        &st,
                                    );
                                    fill_color_naive(
                                        Canvas { px: &mut naive, grid: &g },
                                        [9, 200, 71],
                                        &st,
                                    );
                                    assert_eq!(
                                        fast, naive,
                                        "fill {w}x{h} serp={serpentine} {rect:?} \
                                         {blend:?} op={opacity} flags={flags}"
                                    );
                                    cases += 2;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(cases > 10_000, "the matrix shrank: {cases}");
    }

    /// The luma key's divide-free factor must equal the division it
    /// replaced at every one of the 256 luma values — the rounding IS the
    /// wire format here.
    #[test]
    fn the_luma_key_factor_has_no_division_and_no_rounding_drift() {
        for y in 0u32..=255 {
            let src = [y as u8, y as u8, y as u8];
            // luma of a grey is the grey itself (54+183+19 == 256)
            assert_eq!(luma(src) as u32, y, "luma({y})");
            assert_eq!(key_alpha(Key::Luma, src), (y as i32) * ONE / 255, "y={y}");
        }
        assert_eq!(key_alpha(Key::Luma, [255, 255, 255]), ONE);
        assert_eq!(key_alpha(Key::Luma, [0, 0, 0]), 0);
    }

    /// The combined alpha must match the 64-bit expression it replaced at
    /// every opacity and every key factor.
    #[test]
    fn the_combined_alpha_never_needs_64_bits() {
        for opacity in 0u8..=100 {
            let base = opacity_alpha(opacity);
            for y in 0u32..=255 {
                let src = [y as u8, y as u8, y as u8];
                for key in [Key::None, Key::Black, Key::Luma] {
                    let want = (base as i64 * key_alpha(key, src) as i64 >> 16) as i32;
                    assert_eq!(layer_alpha(key, src, base), want, "op={opacity} y={y} {key:?}");
                }
            }
        }
    }

    /// A ramped full-layout layer takes the in-place path and must agree
    /// with the scratch one it replaced.
    #[test]
    fn an_in_place_ramp_matches_the_scratch_ramp() {
        let full = crate::scene::parse(concat!(
            "S 0000000a s\n",
            "L pat 0 0 0 0 normal 100 none fill 1\n",
            "I 0123abcd\n",
            "R 60 0:000000 128:00ff40 255:ff00ff\n",
        ))
        .unwrap();
        // the same ramp on a CLIPPED box, which cannot take the fast path
        let boxed = crate::scene::parse(concat!(
            "S 0000000b s\n",
            "L pat 1 1 3 2 normal 100 none fill 1\n",
            "I 0123abcd\n",
            "R 60 0:000000 128:00ff40 255:ff00ff\n",
        ))
        .unwrap();
        let g = grid(5, 4, true);
        let src = frame(g.len(), 0x1234_5678);
        for scene in [&full, &boxed] {
            let mut c = Compositor::new(g);
            c.set_scene(scene);
            let mut px = vec![[0u8; 3]; g.len()];
            c.pattern_layer(&mut px, 0, &src);
            // the oracle: remap a copy, then composite it the naive way
            let mut want = vec![[0u8; 3]; g.len()];
            let mut cooked = src.clone();
            let mut lut: Option<(u32, Box<[[u8; 3]; 256]>)> = None;
            let ramp = match &scene.layers[0].body {
                LayerBody::Pattern(p) => p.ramp.clone().unwrap(),
                _ => unreachable!(),
            };
            ensure_lut(&ramp, &mut lut, 7);
            palette_remap_frame(&mut cooked, &lut.unwrap().1, 60 * 256 / 100);
            composite_frame_naive(
                Canvas { px: &mut want, grid: &g },
                &cooked,
                &scene.layers[0].style,
            );
            assert_eq!(px, want, "scene {}", scene.id);
        }
        // and the full-layout case really did avoid the scratch
        let mut c = Compositor::new(g);
        c.set_scene(&full);
        let mut px = vec![[0u8; 3]; g.len()];
        c.pattern_layer(&mut px, 0, &src);
        assert_eq!(
            c.resident_bytes(),
            768 + core::mem::size_of::<LayerRt>(),
            "an in-place ramp holds the LUT and no frame scratch"
        );
    }

    /// Gitea #787: the per-layer scene ramp is the SAME stage as the device
    /// output palette, so a ramp whose last stop is below 255 clamps here
    /// too — and byte for byte the same way, because `ensure_lut` cooks
    /// through `outpipe::fill_palette_lut` rather than re-deriving it.
    #[test]
    fn a_scene_ramp_clamps_above_its_last_stop_like_the_device_palette() {
        let stops: Vec<(u8, [u8; 3])> =
            vec![(0, [27, 14, 58]), (96, [194, 58, 107]), (176, [247, 224, 138])];
        let ramp = Ramp { pct: 100, stops: stops.clone() };
        let mut lut: Option<(u32, Box<[[u8; 3]; 256]>)> = None;
        ensure_lut(&ramp, &mut lut, 1);
        let cooked = lut.unwrap().1;

        // the plateau: every entry above the last stop continues its colour
        let top = cooked[176];
        assert_ne!(top, [0, 0, 0], "the last stop's colour, not black");
        for i in 176..=255usize {
            assert_eq!(cooked[i], top, "scene ramp entry {i}");
        }
        assert_ne!(cooked[96], top, "inside the ramp is still interpolated");

        // and the device path cooks the identical table from the same stops
        let b = |v: u8| crate::fixed::Fx::from_raw(((v as i32) << 16) / 255);
        let pal: Vec<(crate::fixed::Fx, [crate::fixed::Fx; 3])> = stops
            .iter()
            .map(|(p, c)| (b(*p), [b(c[0]), b(c[1]), b(c[2])]))
            .collect();
        let mut device = [[0u8; 3]; 256];
        crate::outpipe::fill_palette_lut(&pal, &mut device);
        assert_eq!(*cooked, device, "scene ramp == device palette, byte for byte");
    }

    // ---- keyframes (`M` lines) ----

    fn track(wire_keys: &str) -> Vec<Keyframe> {
        let mut wire = String::from("S 0000000a k\nL color 7 8 1 1 normal 60 none fill 1\n");
        wire.push_str(wire_keys);
        crate::scene::parse(&wire).expect("parses").layers.remove(0).keys
    }

    fn base_style() -> LayerStyle {
        LayerStyle {
            rect: Rect { x: 7, y: 8, w: 1, h: 1 },
            opacity: 60,
            ..LayerStyle::default()
        }
    }

    fn at(keys: &[Keyframe], t: u32) -> (i16, i16, u8, Option<u8>) {
        let k = eval_keys(keys, &base_style(), t);
        (k.x, k.y, k.opacity, k.frame)
    }

    #[test]
    fn every_ease_lands_exactly_on_its_keys() {
        for e in crate::scene::Ease::ALL {
            let mut w = String::from("M 0 0 0 0 linear\nM 1000 10 -20 50 ");
            w.push_str(e.as_str());
            w.push_str("\nM 2000 0 0 0 linear\n");
            let keys = track(&w);
            assert_eq!(at(&keys, 0), (0, 0, 0, None), "{e:?} at its first key");
            assert_eq!(at(&keys, 1000), (10, -20, 50, None), "{e:?} at its own key");
            // the last key's time is the period: 2000 IS 0 again
            assert_eq!(at(&keys, 2000), (0, 0, 0, None), "{e:?} loops");
            assert_eq!(at(&keys, 3000), (10, -20, 50, None), "{e:?} second lap");
        }
    }

    #[test]
    fn the_segment_follows_the_ease_of_the_key_it_ends_at() {
        let lin = track("M 0 0 0 0 linear\nM 1000 10 20 100 linear\nM 2000 0 0 0 step\n");
        assert_eq!(at(&lin, 500), (5, 10, 50, None), "linear halfway");
        // step holds the previous key right up to the next one
        assert_eq!(at(&lin, 1999), (10, 20, 100, None), "step holds");
        // `in` is slow off the mark, `out` fast
        let i = track("M 0 0 0 0 linear\nM 1000 100 0 0 in\n");
        let o = track("M 0 0 0 0 linear\nM 1000 100 0 0 out\n");
        assert!(at(&i, 250).0 < 25 && at(&o, 250).0 > 25);
        // `back` overshoots the target in position; the opacity it carries
        // along is clamped to 0..100 instead
        let b = track("M 0 0 0 0 linear\nM 1000 100 0 100 back\nM 1001 100 0 100 linear\n");
        let peak = (0..1000).map(|t| at(&b, t)).max_by_key(|s| s.0).unwrap();
        assert!(peak.0 > 100, "back overshoots: {peak:?}");
        assert_eq!(peak.2, 100, "opacity clamps at 100: {peak:?}");
    }

    #[test]
    fn equals_keeps_the_l_lines_value() {
        let keys = track("M 0 = 0 = linear\nM 100 = 10 = linear\n");
        for t in [0, 30, 99] {
            let (x, _, o, _) = at(&keys, t);
            assert_eq!((x, o), (7, 60), "x and opacity are the L line's at {t}");
        }
        assert_eq!(at(&keys, 50).1, 5);
    }

    #[test]
    fn before_the_first_key_it_holds_and_a_single_key_is_static() {
        let keys = track("M 500 10 0 0 linear\nM 1000 20 0 0 linear\n");
        assert_eq!(at(&keys, 100).0, 10, "the first key holds until it is reached");
        assert_eq!(at(&keys, 750).0, 15);
        let one = track("M 0 3 4 = linear 2\n");
        for t in [0, 1, 99_999] {
            assert_eq!(at(&one, t), (3, 4, 60, Some(2)), "a period-0 track is a pose");
        }
    }

    #[test]
    fn a_keys_frame_holds_until_the_next_key() {
        let keys = track("M 0 0 0 0 linear 3\nM 100 0 0 0 linear\nM 200 0 0 0 linear 1\nM 300 0 0 0 linear\n");
        assert_eq!(at(&keys, 0).3, Some(3));
        assert_eq!(at(&keys, 99).3, Some(3));
        assert_eq!(at(&keys, 100).3, None, "no frame: the clock is back in charge");
        assert_eq!(at(&keys, 250).3, Some(1));
    }

    /// The compositor moves the layer: a 1x1 wash walking a 4x1 strip.
    #[test]
    fn the_compositor_moves_a_keyed_layer() {
        let scene = crate::scene::parse(concat!(
            "S 0000000a walk\n",
            "L color 0 0 1 1 normal 100 none fill 1\n",
            "K ffffff\n",
            "M 0 0 0 = linear\n",
            "M 300 3 0 = linear\n",
            "M 400 3 0 = step\n",
        ))
        .unwrap();
        let mut comp = Compositor::new(grid(4, 1, false));
        comp.set_scene(&scene);
        let mut lit = Vec::new();
        for _ in 0..5 {
            let mut px = vec![[0u8; 3]; 4];
            comp.native_layer(&mut px, 0, None);
            lit.push(px.iter().position(|p| *p == [255, 255, 255]).unwrap());
            comp.advance(100);
        }
        // 0 ms, 100, 200, 300 (at the key), 400 = the period → home again
        assert_eq!(lit, vec![0, 1, 2, 3, 0]);
        // a re-install of the same scene keeps the track's clock (#733's
        // rule for the scroll and sprite clocks): 100 ms now, +200
        comp.advance(200);
        comp.set_scene(&scene);
        let mut px = vec![[0u8; 3]; 4];
        comp.native_layer(&mut px, 0, None);
        assert_eq!(px[3], [255, 255, 255], "still at 300 ms after set_scene");
    }

    /// A key's frame outranks the sprite's clock, through the driver.
    #[test]
    fn a_keyframe_frame_outranks_the_sprite_clock() {
        // the 2-frame, 10 fps blink record from the fps tests
        let record = rec("blink", 2, 1, 2, 10, &[RED], &[1, 0, 0, 1]);
        let scene = crate::scene::parse(concat!(
            "S 0000000a s\n",
            "L sprite 0 0 0 0 normal 100 none fill 1\n",
            "I 5b17e5ef\n",
            "M 0 = = = linear 1\n",
            "M 500 = = = linear\n",
            "M 1000 = = = linear\n",
        ))
        .unwrap();
        let mut comp = Compositor::new(grid(2, 1, false));
        comp.set_scene(&scene);
        let mut driver = SceneDriver::new();
        let mut host = SpriteHost(record);
        let mut px: crate::arena::FrameVec = crate::arena::empty();
        let step = Fx::from_raw(100 << 16);
        let mut seen = Vec::new();
        for _ in 0..10 {
            assert!(driver.frame(&mut comp, &mut px, 2, step, &mut host));
            seen.push(if px[0] == RED { 0 } else { 1 });
        }
        // 100..400 ms: frame 1 held; 500..900: the 10 fps clock (frame =
        // sprite_ms/100 mod 2, and sprite_ms runs from the scene's start);
        // 1000 ms is the period, so the held frame is back
        assert_eq!(seen, vec![1, 1, 1, 1, 1, 0, 1, 0, 1, 1]);
    }

    // ---- clip layers (`X clip`) ----

    /// Draw a scene's native layers bottom → top onto black, the way the
    /// driver does, with `sprite` handed to every sprite layer.
    fn stack(wire: &str, w: u16, h: u16, sprite: Option<&[u8]>) -> Vec<[u8; 3]> {
        let scene = crate::scene::parse(wire).expect("parses");
        let mut comp = Compositor::new(grid(w, h, false));
        comp.set_scene(&scene);
        let mut px = vec![[0u8; 3]; w as usize * h as usize];
        let view = sprite.and_then(SpriteView::parse);
        for i in 0..comp.layer_count() {
            comp.native_layer(&mut px, i, view.as_ref());
        }
        px
    }

    const GRN: [u8; 3] = [0, 255, 0];
    const BLU: [u8; 3] = [0, 0, 255];

    #[test]
    fn a_clip_layer_draws_only_where_the_layer_below_did() {
        let head = concat!(
            "S 0000000a c\n",
            "L color 0 0 0 0 normal 100 none fill 1\n",
            "K ff0000\n",
            "L color 1 0 2 1 normal 100 none fill 1\n",
            "K 00ff00\n",
            "L color 0 0 0 0 normal 100 none fill 1\n",
            "K 0000ff\n",
        );
        let unclipped = stack(head, 4, 1, None);
        assert_eq!(unclipped, vec![BLU; 4], "without X the wash covers everything");
        let mut wire = String::from(head);
        wire.push_str("X clip\n");
        assert_eq!(stack(&wire, 4, 1, None), vec![RED, BLU, BLU, RED], "inside the green box only");
    }

    #[test]
    fn a_clip_over_text_fills_exactly_the_glyphs() {
        let text = concat!(
            "S 0000000a t\n",
            "L text 0 0 0 0 normal 100 none fill 1\n",
            "T lit HI\n",
        );
        let glyphs = stack(text, 16, 8, None);
        let lit: Vec<bool> = glyphs.iter().map(|p| *p != [0, 0, 0]).collect();
        assert!(lit.iter().any(|&b| b), "the text drew something");
        let mut wire = String::from(text);
        wire.push_str("L color 0 0 0 0 normal 100 none fill 1\nK 0000ff\nX clip\n");
        let filled = stack(&wire, 16, 8, None);
        for (i, p) in filled.iter().enumerate() {
            assert_eq!(*p, if lit[i] { BLU } else { [0, 0, 0] }, "pixel {i}");
        }
    }

    #[test]
    fn a_clip_over_a_sprite_follows_its_texels_not_its_colours() {
        // the checker: red, TRANSPARENT, transparent, opaque BLACK
        let wire = concat!(
            "S 0000000a s\n",
            "L sprite 0 0 0 0 normal 100 none fill 1\n",
            "I 5b17e5ef\n",
            "L color 0 0 0 0 normal 100 none fill 1\n",
            "K 00ff00\n",
            "X clip\n",
        );
        let px = stack(wire, 2, 2, Some(&checker()));
        // the opaque black texel is coverage like any other; index 0 is not
        assert_eq!(px, vec![GRN, [0, 0, 0], [0, 0, 0], GRN]);
        // …and a base with no record drew nothing, so neither does its clip
        assert_eq!(stack(wire, 2, 2, None), vec![[0, 0, 0]; 4]);
    }

    #[test]
    fn clip_on_the_first_layer_is_a_no_op_and_a_run_shares_one_base() {
        let first = "S 0000000a f\nL color 0 0 0 0 normal 100 none fill 1\nK 00ff00\nX clip\n";
        assert_eq!(stack(first, 2, 1, None), vec![GRN; 2], "nothing below: unclipped");
        // base = a 1-px box; two clip layers above it both clip to IT
        let run = concat!(
            "S 0000000a r\n",
            "L color 1 0 1 1 normal 100 none fill 1\n",
            "K ff0000\n",
            "L color 0 0 0 0 normal 100 none fill 1\n",
            "K 00ff00\n",
            "X clip\n",
            "L color 0 0 0 0 add 100 none fill 1\n",
            "K 0000ff\n",
            "X clip\n",
        );
        assert_eq!(stack(run, 3, 1, None), vec![[0, 0, 0], [0, 255, 255], [0, 0, 0]]);
    }

    /// A pattern base, keyed black, through `pattern_layer` and the driver's
    /// frame clock: the mask is per FRAME, so a clip layer never reads the
    /// coverage its base drew a frame earlier.
    #[test]
    fn a_keyed_pattern_base_clips_through_the_driver() {
        struct Host(Vec<[u8; 3]>, bool);
        impl SceneHost for Host {
            fn pattern_frame(&mut self, _l: usize, _d: Fx) -> Option<&[[u8; 3]]> {
                self.1.then_some(&self.0[..])
            }
            fn sprite(&mut self, _l: usize) -> Option<SpriteView<'_>> {
                None
            }
        }
        let scene = crate::scene::parse(concat!(
            "S 0000000a p\n",
            "L pat 0 0 0 0 normal 100 black fill 1\n",
            "I 0123abcd\n",
            "L color 0 0 0 0 normal 100 none fill 1\n",
            "K 00ff00\n",
            "X clip\n",
        ))
        .unwrap();
        let mut comp = Compositor::new(grid(3, 1, false));
        comp.set_scene(&scene);
        let mut driver = SceneDriver::new();
        let mut host = Host(vec![[0, 0, 0], [9, 9, 9], [0, 0, 0]], true);
        let mut px: crate::arena::FrameVec = crate::arena::empty();
        let step = Fx::from_raw(16 << 16);
        assert!(driver.frame(&mut comp, &mut px, 3, step, &mut host));
        assert_eq!(&px[..], &[[0, 0, 0], GRN, [0, 0, 0]]);
        // the engine has no frame this step: the base draws nothing, and the
        // clip layer must not reuse last frame's mask
        host.1 = false;
        assert!(driver.frame(&mut comp, &mut px, 3, step, &mut host));
        assert_eq!(&px[..], &[[0, 0, 0]; 3]);
    }

    #[test]
    fn an_unclipped_scene_never_takes_the_mask() {
        let scene = crate::scene::parse(
            "S 0000000a u\nL color 0 0 0 0 normal 100 none fill 1\nK 00ff00\n",
        )
        .unwrap();
        let mut comp = Compositor::new(grid(64, 64, false));
        comp.set_scene(&scene);
        let mut px = vec![[0u8; 3]; 4096];
        comp.native_layer(&mut px, 0, None);
        assert_eq!(comp.cover.capacity(), 0);
    }
}
