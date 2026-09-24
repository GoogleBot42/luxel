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

use crate::outpipe::{luma, palette_remap_frame, GridMap};
use crate::scene::{
    Blend, Fit, Key, Layer, LayerBody, LayerKind, LayerStyle, Ramp, Scroll, TextLayer, TextSource,
};
use crate::text::{self, Font};
use crate::vm::ArrView;

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
#[inline]
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
        Key::Luma => luma(src) as i32 * ONE / 255,
    }
}

/// Blend one source pixel into one destination pixel.
///
/// Out of line for the same reason `bulk::put` is: five modes inlined at
/// every call site is five modes' worth of image, several times over, on a
/// board with kilobytes of OTA slot left.
#[inline(never)]
pub fn blend_px_mode(dst: &mut [u8; 3], src: [u8; 3], mode: Blend, alpha: i32) {
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
    let grid = *dst.grid;
    if skip(style, &grid) {
        return;
    }
    let (bx, by, bw, bh) = resolved_rect(style, &grid);
    let (mx, my) = mirrors(style);
    let base = opacity_alpha(style.opacity);
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
            let a = (base as i64 * key_alpha(style.key, s) as i64 >> 16) as i32;
            if let Some(d) = dst.px.get_mut(grid.index(dr as usize, dc as usize)) {
                blend_px_mode(d, s, style.blend, a);
            }
        }
    }
}

/// Wash the layer's box with one colour. Colour layers carry no key — the
/// wash is the layer.
pub fn fill_color(dst: Canvas, rgb: [u8; 3], style: &LayerStyle) {
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

// ---- sprites ----

/// The `// @sprite …` header of a sprite-tagged pattern.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SpriteTag {
    pub w: u16,
    pub h: u16,
    pub frames: u16,
    /// Frames per second; 0 = static.
    pub fps: u8,
}

impl SpriteTag {
    /// Texels per frame.
    pub fn texels(&self) -> usize {
        self.w as usize * self.h as usize
    }
    /// Texels across every frame — the length each of the three channel
    /// arrays must have.
    pub fn len(&self) -> usize {
        self.texels() * self.frames as usize
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Largest sprite edge the tag may declare.
pub const SPRITE_MAX_EDGE: u16 = 64;
/// Highest frame rate the tag may declare.
pub const SPRITE_MAX_FPS: u8 = 30;

/// Parse the first line of a pattern source as a sprite tag:
/// `// @sprite w=<w> h=<h> frames=<n> fps=<f>`, in any key order. Returns
/// `None` for an ordinary pattern — this is how the compositor, the store
/// and the editor all tell a sprite from a pattern.
pub fn parse_sprite_tag(source: &str) -> Option<SpriteTag> {
    let line = source.lines().next()?.trim();
    let body = line.strip_prefix("//")?.trim_start();
    let body = body.strip_prefix("@sprite")?;
    let (mut w, mut h, mut frames, mut fps) = (0u32, 0u32, 1u32, 0u32);
    let mut seen_w = false;
    let mut seen_h = false;
    for tok in body.split_whitespace() {
        let (k, v) = tok.split_once('=')?;
        let v: u32 = v.parse().ok()?;
        match k {
            "w" => {
                w = v;
                seen_w = true;
            }
            "h" => {
                h = v;
                seen_h = true;
            }
            "frames" => frames = v,
            "fps" => fps = v,
            // an unknown key is a newer editor's: ignored, like every
            // other unknown field in these formats
            _ => {}
        }
    }
    if !seen_w || !seen_h {
        return None;
    }
    let max = SPRITE_MAX_EDGE as u32;
    if w == 0 || h == 0 || w > max || h > max || frames == 0 || fps > SPRITE_MAX_FPS as u32 {
        return None;
    }
    if (w * h * frames) as usize > u16::MAX as usize * 4 {
        return None;
    }
    Some(SpriteTag {
        w: w as u16,
        h: h as u16,
        frames: frames as u16,
        fps: fps as u8,
    })
}

/// A sprite's three HSV channel arrays plus its geometry — everything
/// [`blit_sprite`] needs, borrowed straight out of the compiled program's
/// const pool (on the device: memory-mapped flash, so a sprite costs no
/// RAM beyond its engine's tables).
pub struct SpriteView<'a> {
    pub w: u16,
    pub h: u16,
    pub frames: u16,
    pub fps: u8,
    /// Hue 0..1. Named `h_` because `h` is the height.
    pub h_: ArrView<'a>,
    pub s: ArrView<'a>,
    pub v: ArrView<'a>,
}

/// Build a [`SpriteView`] from an engine whose program is a sprite-tagged
/// pattern.
///
/// **Deviation from the Phase-B contract's `sprite_view(prog: &Program)`**
/// (reported): the three arrays are arena entries, not program sections,
/// so they are reachable only through the Vm — and the tag lives in the
/// SOURCE, which a `Program` does not carry. The engine has both. It is
/// never stepped: `Engine::new`/`from_program` runs top-level
/// initialization, which is what turns `var sprH = […]` into a const-pool
/// (`ArrView::Const`) array, and that is all this reads.
pub fn sprite_view<'a>(engine: &'a crate::engine::Engine, source: &str) -> Option<SpriteView<'a>> {
    let tag = parse_sprite_tag(source)?;
    let h_ = engine.global_array("sprH")?;
    let s = engine.global_array("sprS")?;
    let v = engine.global_array("sprV")?;
    let n = tag.len();
    if h_.len() < n || s.len() < n || v.len() < n {
        return None;
    }
    Some(SpriteView {
        w: tag.w,
        h: tag.h,
        frames: tag.frames,
        fps: tag.fps,
        h_,
        s,
        v,
    })
}

/// One sprite texel as output bytes — the same HSV → RGB888 path
/// `bulk::blit` quantizes a texel through, so a sprite drawn natively and
/// the same sprite drawn by its own `blit` call agree byte for byte.
#[inline(never)]
fn sprite_texel(sp: &SpriteView, i: usize) -> [u8; 3] {
    let num = |a: &ArrView, i: usize| a.get(i).map_or(crate::fixed::Fx::ZERO, |v| v.num());
    let rgb = crate::vm::hsv_to_rgb(num(&sp.h_, i), num(&sp.s, i), num(&sp.v, i));
    [
        crate::engine::quantize(rgb[0]),
        crate::engine::quantize(rgb[1]),
        crate::engine::quantize(rgb[2]),
    ]
}

/// Blit one frame of a sprite into the layer's box. Sprites are ALWAYS
/// black-keyed (`v == 0` quantizes to `[0,0,0]`), whatever the record's
/// `key` says — that is the format's transparency and the `blit` mode-3
/// rule the library sprites already rely on.
///
/// `fit`: `fill` and `contain` place the sprite 1:1 at the box origin;
/// `tile` repeats it across the box.
pub fn blit_sprite(dst: Canvas, sprite: &SpriteView, frame: u16, style: &LayerStyle) {
    let grid = *dst.grid;
    if skip(style, &grid) || sprite.w == 0 || sprite.h == 0 || sprite.frames == 0 {
        return;
    }
    let (bx, by, bw, bh) = resolved_rect(style, &grid);
    let (sw, sh) = (sprite.w as i32, sprite.h as i32);
    let tile = style.fit == Fit::Tile;
    let (mx, my) = mirrors(style);
    let a0 = opacity_alpha(style.opacity);
    let base = (frame % sprite.frames) as usize * sprite.w as usize * sprite.h as usize;
    let span_w = if tile { bw } else { sw.min(bw) };
    let span_h = if tile { bh } else { sh.min(bh) };
    for j in 0..span_h {
        let dr = by + if my { bh - 1 - j } else { j };
        if dr < 0 || dr >= grid.h as i32 {
            continue;
        }
        let sy = j.rem_euclid(sh);
        for i in 0..span_w {
            let dc = bx + if mx { bw - 1 - i } else { i };
            if dc < 0 || dc >= grid.w as i32 {
                continue;
            }
            let sx = i.rem_euclid(sw);
            let px = sprite_texel(sprite, base + (sy * sw + sx) as usize);
            if px == [0, 0, 0] {
                continue; // the format's transparency
            }
            if let Some(d) = dst.px.get_mut(grid.index(dr as usize, dc as usize)) {
                blend_px_mode(d, px, style.blend, a0);
            }
        }
    }
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
    let grid = *dst.grid;
    if skip(style, &grid) || text.is_empty() {
        return;
    }
    let (bx, by, bw, _bh) = resolved_rect(style, &grid);
    let tw = text::width(text, layer.font) as i32;
    let anchor = match layer.align {
        crate::scene::Align::Left => bx,
        crate::scene::Align::Center => bx + (bw - tw) / 2,
        crate::scene::Align::Right => bx + bw - tw,
    };
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
    composite_frame(
        Canvas {
            px: dst.px,
            grid: dst.grid,
        },
        scratch,
        &keyed,
    );
}

// ---- the host-facing compositor ----

/// Per-layer runtime state: the clocks and caches the wire record does not
/// carry.
struct LayerRt {
    kind: LayerKind,
    style: LayerStyle,
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
}

/// Owns everything a host must keep between frames to draw a scene: the
/// layer stack, the scroll and sprite clocks, the per-layer ramp LUTs and
/// ONE shared 3 B/px scratch buffer (allocated on first use, released with
/// the scene).
///
/// The host drives layers bottom → top, calling [`Compositor::pattern_layer`]
/// with that layer's engine frame or [`Compositor::native_layer`] for the
/// text / sprite / colour layers.
pub struct Compositor {
    grid: GridMap,
    layers: Vec<LayerRt>,
    scratch: Vec<[u8; 3]>,
    epoch: u32,
}

impl Compositor {
    pub fn new(grid: GridMap) -> Self {
        Compositor {
            grid,
            layers: Vec::new(),
            scratch: Vec::new(),
            epoch: 0,
        }
    }

    /// Point the compositor at a different layout (a console whose device
    /// geometry changed). Clears the scratch, which is grid-sized.
    pub fn set_grid(&mut self, grid: GridMap) {
        if self.grid != grid {
            self.grid = grid;
            self.scratch = Vec::new();
        }
    }

    pub fn grid(&self) -> GridMap {
        self.grid
    }

    /// Rebuild the layer runtime state from a scene record. Invalidates
    /// every cached ramp LUT.
    pub fn set_scene(&mut self, scene: &crate::scene::Scene) {
        self.epoch = self.epoch.wrapping_add(1);
        self.layers.clear();
        for l in &scene.layers {
            self.layers.push(LayerRt::from(l));
        }
    }

    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    pub fn layer_kinds(&self) -> impl Iterator<Item = LayerKind> + '_ {
        self.layers.iter().map(|l| l.kind)
    }

    /// The text a layer will draw this frame. The host resolves
    /// [`TextSource::Clock`] and [`TextSource::Slot`]; a `lit` layer needs
    /// no call.
    pub fn set_text(&mut self, layer: usize, s: &str) {
        if let Some(l) = self.layers.get_mut(layer) {
            l.resolved.clear();
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

    /// Advance the scroll and sprite-frame clocks.
    pub fn advance(&mut self, dt_ms: u32) {
        for l in self.layers.iter_mut() {
            if l.kind == LayerKind::Text && l.text.scroll != Scroll::None && l.text.speed > 0 {
                l.scroll_mpx += l.text.speed as i64 * dt_ms as i64;
            }
            if l.kind == LayerKind::Sprite {
                l.sprite_ms = l.sprite_ms.saturating_add(dt_ms);
            }
        }
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
        let Compositor { layers, scratch, .. } = self;
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
        let canvas = Canvas { px: dst, grid: &grid };
        match l.lut.as_ref().filter(|_| amount > 0) {
            Some((_, lut)) => {
                scratch.clear();
                scratch.extend_from_slice(src);
                palette_remap_frame(scratch, lut, amount);
                composite_frame(canvas, scratch, &style);
            }
            None => composite_frame(canvas, src, &style),
        }
    }

    /// Composite one native layer — text, sprite or colour. Pass the
    /// sprite layer's [`SpriteView`]; the other kinds ignore it.
    pub fn native_layer(&mut self, dst: &mut [[u8; 3]], layer: usize, sprite: Option<&SpriteView>) {
        let grid = self.grid;
        let Compositor { layers, scratch, .. } = self;
        let Some(l) = layers.get(layer) else {
            return;
        };
        let canvas = Canvas { px: dst, grid: &grid };
        match l.kind {
            LayerKind::Color => fill_color(canvas, l.color, &l.style),
            LayerKind::Sprite => {
                let Some(sp) = sprite else { return };
                let frame = if sp.fps == 0 || sp.frames <= 1 {
                    0
                } else {
                    ((l.sprite_ms as u64 * sp.fps as u64 / 1000) % sp.frames as u64) as u16
                };
                blit_sprite(canvas, sp, frame, &l.style);
            }
            LayerKind::Text => {
                let (_, _, bw, bh) = resolved_rect(&l.style, &grid);
                let scroll = scroll_offset(&l.text, &l.resolved, bw, bh, l.scroll_mpx);
                let n = grid.len();
                if scratch.len() != n {
                    scratch.clear();
                    scratch.resize(n, [0, 0, 0]);
                }
                draw_text_layer(canvas, scratch, &l.resolved, &l.text, &l.style, scroll);
            }
            LayerKind::Pattern => {}
        }
    }

    /// Bytes this compositor is holding — the shared frame scratch plus
    /// the cooked ramp LUTs. The number a device loses from `heap_free`
    /// for a scene.
    pub fn resident_bytes(&self) -> usize {
        self.scratch.capacity() * 3
            + self.layers.iter().filter(|l| l.lut.is_some()).count() * 768
            + self.layers.len() * core::mem::size_of::<LayerRt>()
    }
}

/// Cook a layer's ramp into a 256-entry luma → colour LUT, cached behind
/// the compositor's scene epoch (the `DeviceChain`/`Engine` idiom).
fn ensure_lut(ramp: &Ramp, lut: &mut Option<(u32, Box<[[u8; 3]; 256]>)>, epoch: u32) {
    if lut.as_ref().map(|(e, _)| *e) == Some(epoch) {
        return;
    }
    // byte domain → 16.16 0..1, the scaling DeviceChain uses for the
    // device palette (no fixed-point divide on this path)
    let b = |v: u8| crate::fixed::Fx::from_raw(((v as i32) << 16) / 255);
    let pal: Vec<(crate::fixed::Fx, [crate::fixed::Fx; 3])> = ramp
        .stops
        .iter()
        .map(|(p, c)| (b(*p), [b(c[0]), b(c[1]), b(c[2])]))
        .collect();
    let mut cooked = Box::new([[0u8; 3]; 256]);
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

/// The scroll offset in whole pixels for the current phase.
fn scroll_offset(t: &TextLayer, text: &str, bw: i32, bh: i32, phase_mpx: i64) -> i32 {
    if t.scroll == Scroll::None || t.speed == 0 {
        return 0;
    }
    let px = (phase_mpx / 1000) as i32;
    let tw = text::width(text, t.font) as i32;
    match t.scroll {
        Scroll::None => 0,
        // wrap over the text plus the box so the string leaves the box
        // entirely before it comes back
        Scroll::Left => -px.rem_euclid((tw + bw).max(1)),
        Scroll::Right => px.rem_euclid((tw + bw).max(1)) - tw,
        Scroll::Up => -px.rem_euclid((bh + line_h(t.font)).max(1)),
        Scroll::Down => px.rem_euclid((bh + line_h(t.font)).max(1)) - line_h(t.font),
        // ping-pong over the overflow; a string that fits does not move
        Scroll::Bounce => {
            let over = (tw - bw).max(0);
            if over == 0 {
                0
            } else {
                let p = px.rem_euclid(over * 2);
                -(if p <= over { p } else { over * 2 - p })
            }
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
            text: TextLayer::default(),
            resolved: String::new(),
            color: [0, 0, 0],
            ramp: None,
            lut: None,
            scroll_mpx: 0,
            sprite_ms: 0,
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
            LayerBody::Sprite { .. } => {}
        }
        rt
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::{Align, Rect};
    use alloc::vec;

    fn grid(w: u16, h: u16, serpentine: bool) -> GridMap {
        GridMap { w, h, serpentine }
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

    const SPRITE_SRC: &str = concat!(
        "// @sprite w=2 h=1 frames=2 fps=10\n",
        "var sprH = [0, 0, 0, 0]\n",
        "var sprS = [0, 0, 0, 0]\n",
        "var sprV = [1, 0, 0, 1]\n",
        "export function render(index) { hsv(0, 0, 0) }\n",
    );

    #[test]
    fn the_sprite_tag_parses_and_bounds_itself() {
        let t = parse_sprite_tag(SPRITE_SRC).unwrap();
        assert_eq!((t.w, t.h, t.frames, t.fps), (2, 1, 2, 10));
        assert_eq!(t.len(), 4);
        // key order is free and unknown keys are ignored
        assert!(parse_sprite_tag("// @sprite frames=1 h=8 w=8 fps=0 pal=16").is_some());
        // an ordinary pattern is not a sprite
        assert!(parse_sprite_tag("export function render(i) {}").is_none());
        assert!(parse_sprite_tag("// just a comment").is_none());
        // out-of-range tags are refused
        assert!(parse_sprite_tag("// @sprite w=65 h=8 frames=1 fps=0").is_none());
        assert!(parse_sprite_tag("// @sprite w=8 h=8 frames=0 fps=0").is_none());
        assert!(parse_sprite_tag("// @sprite w=8 h=8 frames=1 fps=31").is_none());
        assert!(parse_sprite_tag("// @sprite w=8 frames=1 fps=0").is_none());
    }

    /// The format's load-bearing compiler fact: a top-level numeric array
    /// literal interns into the const pool, so a sprite's pixels are read
    /// from the program's word region (flash, on the device) and the
    /// engine never has to run.
    #[cfg(feature = "frontend")]
    #[test]
    fn sprite_arrays_are_const_pool_entries_before_any_frame() {
        let e = crate::engine::Engine::new(SPRITE_SRC, 4, 1).unwrap();
        for name in ["sprH", "sprS", "sprV"] {
            let a = e.global_array(name).unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(a.len(), 4, "{name}");
            assert!(
                matches!(a, ArrView::Const(_)),
                "{name} must be a const-pool array, not an owned copy"
            );
        }
        let sp = sprite_view(&e, SPRITE_SRC).expect("sprite view");
        assert_eq!((sp.w, sp.h, sp.frames, sp.fps), (2, 1, 2, 10));
        // a pattern without the tag is not a sprite
        assert!(sprite_view(&e, "export function render(i) {}").is_none());
    }

    #[cfg(feature = "frontend")]
    #[test]
    fn a_sprite_blits_its_frames_black_keyed() {
        let e = crate::engine::Engine::new(SPRITE_SRC, 4, 1).unwrap();
        let sp = sprite_view(&e, SPRITE_SRC).unwrap();
        let g = grid(2, 1, false);
        let draw = |frame: u16| {
            let mut px = [[9u8, 9, 9]; 2];
            blit_sprite(
                Canvas {
                    px: &mut px,
                    grid: &g,
                },
                &sp,
                frame,
                &LayerStyle::default(),
            );
            px
        };
        // frame 0 is [lit, dark]; the dark texel (v = 0) is transparent, so
        // the base shows through
        assert_eq!(draw(0), [[255, 255, 255], [9, 9, 9]]);
        // frame 1 is [dark, lit]
        assert_eq!(draw(1), [[9, 9, 9], [255, 255, 255]]);
        // the frame index wraps
        assert_eq!(draw(2), draw(0));
        assert_eq!(draw(5), draw(1));
    }

    #[cfg(feature = "frontend")]
    #[test]
    fn the_sprite_clock_picks_the_frame() {
        let e = crate::engine::Engine::new(SPRITE_SRC, 4, 1).unwrap();
        let sp = sprite_view(&e, SPRITE_SRC).unwrap();
        let scene = crate::scene::parse(
            "S 0000000a s\nL sprite 0 0 0 0 normal 100 black fill 1\nI 0123abcd\n",
        )
        .unwrap();
        let mut c = Compositor::new(grid(2, 1, false));
        c.set_scene(&scene);
        let mut px = vec![[0u8; 3]; 2];
        // 10 fps ⇒ frame 0 for the first 100 ms, frame 1 for the next
        c.begin(&mut px);
        c.native_layer(&mut px, 0, Some(&sp));
        assert_eq!(px[0], [255, 255, 255]);
        c.advance(100);
        c.begin(&mut px);
        c.native_layer(&mut px, 0, Some(&sp));
        assert_eq!(px[0], [0, 0, 0]);
        assert_eq!(px[1], [255, 255, 255]);
        c.advance(100); // back to frame 0
        c.begin(&mut px);
        c.native_layer(&mut px, 0, Some(&sp));
        assert_eq!(px[0], [255, 255, 255]);
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
        assert_eq!(scroll_offset(&t, "HI", 8, 8, 0), 0);
        assert_eq!(scroll_offset(&t, "HI", 8, 8, 3_000), -3);
        assert_eq!(scroll_offset(&t, "HI", 8, 8, 19_000), -19);
        assert_eq!(scroll_offset(&t, "HI", 8, 8, 20_000), 0); // wrapped
        let still = TextLayer::default();
        assert_eq!(scroll_offset(&still, "HI", 8, 8, 9_999), 0);
    }

    #[test]
    fn align_is_carried_by_the_record() {
        assert_eq!(Align::Center.as_str(), "c");
    }
}
