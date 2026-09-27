//! Image → `LXSP` sprite, for `luxel sprite import` (Gitea #784).
//!
//! THE TWIN of `web/src/lib/imageImport.ts`. Two implementations of one
//! wire-visible result, so they are pinned to the same checked-in goldens
//! (`web/tests/fixtures/*.lxsp`, asserted by `tests/spriteimport.rs` here and
//! by `web/tests/imageImport.test.mjs` there). Read that module before
//! changing anything in here: every arithmetic decision is shared contract.
//!
//! The three rules that keep the two in step:
//!   * rounding is ALWAYS `(x + 0.5).floor()`, never `f64::round` — Rust
//!     rounds halves away from zero and JS's `Math.round` rounds them up, so
//!     the two disagree on negatives. Everything rounded here is non-negative
//!     anyway; the spelling is what matters.
//!   * every sum is written in the same order as the TypeScript. Neither
//!     language reassociates f64, so identical order is identical bits.
//!   * ties break by INDEX — the bucket's position, the palette's — never by
//!     relying on a sort's stability.
//!
//! Decoding is the `image` crate's job (an established crate over a
//! hand-rolled decoder, per CLAUDE.md); this module never parses a container.
//! `luxel-core` stays out of it entirely: it is `no_std`, the firmware links
//! it, and nothing on a device decodes an image.

use std::collections::HashMap;

use luxel_core::sprite::{
    check, record_len, SPRITE_HDR, SPRITE_MAGIC, SPRITE_MAX_BYTES, SPRITE_MAX_COLORS,
    SPRITE_MAX_EDGE, SPRITE_MAX_FPS, SPRITE_MAX_FRAMES, SPRITE_MAX_NAME, SPRITE_VERSION,
};

/// What a "no delay" frame is actually shown at — GIF writes its delays in
/// hundredths and 0 or 1 means "as fast as you like", which every viewer shows
/// at 100 ms. A longer delay is honoured as written.
pub const SLOW_DELAY_MS: i64 = 100;
/// Delays up to this are the "no delay" case.
pub const NO_DELAY_MS: i64 = 10;
/// Alpha at or above this is opaque; below it the texel is index 0.
pub const DEFAULT_ALPHA_THRESHOLD: u16 = 128;

/// One decoded source frame: `w*h*4` RGBA bytes and how long it is shown.
pub struct SourceFrame {
    pub rgba: Vec<u8>,
    /// Frame delay in milliseconds as the container states it (0 = unstated).
    pub delay_ms: i64,
}

/// A decoded image: its natural size and one or more frames.
pub struct SourceImage {
    pub w: usize,
    pub h: usize,
    pub frames: Vec<SourceFrame>,
}

/// How the source is placed in the target box — CSS `object-fit`'s three
/// useful words.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FitMode {
    /// The whole image, proportions kept, transparent letterbox.
    Fit,
    /// Stretched to the box exactly, proportions ignored.
    Fill,
    /// Proportions kept, scaled to cover, the overflow cut off.
    Crop,
}

/// Nearest neighbour keeps pixel art crisp; area average is right for photos.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Resample {
    Nearest,
    Area,
}

/// Every knob the conversion has.
#[derive(Clone, Debug)]
pub struct Options {
    pub name: String,
    pub w: usize,
    pub h: usize,
    pub fit: FitMode,
    pub resample: Resample,
    pub colors: usize,
    pub alpha_threshold: u16,
    pub dither: bool,
    /// Keep source frame `i` when `i % keep_every == 0`. 1 = every frame.
    pub keep_every: usize,
    /// `None` = derive the rate from the frame delays.
    pub fps_override: Option<u8>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            name: String::from("Sprite"),
            w: 16,
            h: 16,
            fit: FitMode::Fit,
            resample: Resample::Area,
            colors: SPRITE_MAX_COLORS,
            alpha_threshold: DEFAULT_ALPHA_THRESHOLD,
            dither: false,
            keep_every: 1,
            fps_override: None,
        }
    }
}

/// A sprite, decoded — the Rust mirror of the web codec's `Sprite`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sprite {
    pub name: String,
    pub w: usize,
    pub h: usize,
    pub frames: usize,
    pub fps: u8,
    pub palette: Vec<[u8; 3]>,
    pub index: Vec<u8>,
}

/// What the conversion produced, and everything a readout says about it.
pub struct ImportResult {
    pub sprite: Sprite,
    pub bytes: usize,
    pub over_cap: bool,
    pub skipped_frames: usize,
    pub dropped_frames: usize,
    pub colors_used: usize,
    pub colors_seen: usize,
    pub fps: u8,
}

/// A source frame's delay, normalised.
pub fn normalized_delay(delay_ms: i64) -> i64 {
    if delay_ms <= NO_DELAY_MS {
        SLOW_DELAY_MS
    } else {
        delay_ms
    }
}

/// A file name as a sprite name: no extension, no path, 1..=64 bytes.
pub fn sprite_name(file_name: &str) -> String {
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or("");
    // the LAST dot-segment goes, wherever it is — `.png` alone leaves nothing,
    // and a trailing dot with no extension after it is not an extension. This
    // is `/\.[^.]+$/` in the TypeScript, spelled out.
    let stem = match base.rfind('.') {
        Some(at) if at + 1 < base.len() => &base[..at],
        _ => base,
    };
    let clean: String = stem
        .chars()
        .map(|c| if c == '\r' || c == '\n' || c == '\t' { ' ' } else { c })
        .collect();
    let mut s = clean.trim().to_string();
    if s.is_empty() {
        return String::from("Sprite");
    }
    while s.len() > SPRITE_MAX_NAME {
        s.pop();
        if s.is_empty() {
            return String::from("Sprite");
        }
    }
    s
}

/// The target size an import starts at: the long edge scaled to 64, or to the
/// fixture it is being drawn for when that is smaller.
pub fn default_target_size(sw: usize, sh: usize, panel: Option<(usize, usize)>) -> (usize, usize) {
    let edge = match panel {
        Some((pw, ph)) if pw > 0 && ph > 0 => pw.max(ph),
        _ => SPRITE_MAX_EDGE,
    };
    let cap = edge.clamp(1, SPRITE_MAX_EDGE);
    let long = sw.max(sh).max(1);
    if long <= cap {
        return (sw.clamp(1, cap), sh.clamp(1, cap));
    }
    let s = cap as f64 / long as f64;
    (
        clamp_round(sw as f64 * s, 1, cap),
        clamp_round(sh as f64 * s, 1, cap),
    )
}

/// Every option clamped to what the record can hold.
pub fn resolve_options(o: &Options) -> Options {
    Options {
        name: sprite_name(&o.name),
        w: o.w.clamp(1, SPRITE_MAX_EDGE),
        h: o.h.clamp(1, SPRITE_MAX_EDGE),
        fit: o.fit,
        resample: o.resample,
        colors: o.colors.clamp(1, SPRITE_MAX_COLORS),
        alpha_threshold: o.alpha_threshold.clamp(1, 255),
        dither: o.dither,
        keep_every: o.keep_every.max(1),
        fps_override: o.fps_override.map(|f| f.min(SPRITE_MAX_FPS)),
    }
}

/// The source frame indices `keep_every` keeps, before the 255-frame cap.
pub fn kept_indices(frame_count: usize, keep_every: usize) -> Vec<usize> {
    let every = keep_every.max(1);
    (0..frame_count).step_by(every).collect()
}

/// The rate the kept frames play at.
pub fn derived_fps(src: &SourceImage, keep_every: usize, kept_count: usize) -> u8 {
    if kept_count <= 1 || src.frames.is_empty() {
        return 0;
    }
    let mut total = 0i64;
    for f in &src.frames {
        total += normalized_delay(f.delay_ms);
    }
    let mean = (total as f64 / src.frames.len() as f64) * keep_every.max(1) as f64;
    if !(mean > 0.0) {
        return SPRITE_MAX_FPS;
    }
    clamp_round(1000.0 / mean, 1, SPRITE_MAX_FPS as usize) as u8
}

/// THE conversion. Nothing here refuses: an over-cap result comes back with
/// `over_cap` true and its real byte count.
pub fn import_sprite(src: &SourceImage, opts: &Options) -> ImportResult {
    let o = resolve_options(opts);
    let sw = src.w.max(1);
    let sh = src.h.max(1);

    let kept = kept_indices(src.frames.len(), o.keep_every);
    let dropped_frames = kept.len().saturating_sub(SPRITE_MAX_FRAMES);
    let use_frames: Vec<usize> = kept.iter().copied().take(SPRITE_MAX_FRAMES).collect();
    let skipped_frames = src.frames.len().saturating_sub(kept.len());

    // 1. FIT
    let n = o.w * o.h;
    let frames = use_frames.len().max(1);
    let mut rgb = vec![0u8; (n * frames).max(1) * 3];
    let mut on = vec![0u8; (n * frames).max(1)];
    let rects = placement(sw, sh, o.w, o.h, o.fit);
    let blank = Vec::new();
    for (f, &si) in use_frames.iter().enumerate() {
        let frame = src.frames.get(si).map(|x| &x.rgba).unwrap_or(&blank);
        resample_frame(frame, sw, sh, &o, &rects, &mut rgb, &mut on, f * n);
    }

    // 2. HISTOGRAM, in first-appearance order
    let hist = histogram(&rgb, &on, n * frames);

    // 3. QUANTIZE
    let palette = if hist.count.len() <= o.colors {
        hist.colors()
    } else {
        median_cut(&hist, o.colors)
    };

    // 4. MAP
    let index = if o.dither {
        map_dithered(&rgb, &on, o.w, o.h, frames, &palette)
    } else {
        map_nearest(&rgb, &on, n * frames, &palette)
    };

    let fps = match o.fps_override {
        Some(f) => {
            if use_frames.len() <= 1 {
                0
            } else {
                f
            }
        }
        None => derived_fps(src, o.keep_every, use_frames.len()),
    };
    let colors_seen = hist.count.len();
    let sprite = compact_palette(Sprite {
        name: o.name.clone(),
        w: o.w,
        h: o.h,
        frames,
        fps,
        palette,
        index,
    });
    let bytes = encode(&sprite).len();
    ImportResult {
        bytes,
        over_cap: bytes > SPRITE_MAX_BYTES,
        skipped_frames,
        dropped_frames,
        colors_used: sprite.palette.len(),
        colors_seen,
        fps: sprite.fps,
        sprite,
    }
}

// ---- the record ----

/// The `LXSP` bytes for `sprite` — the mirror of the web codec's
/// `encodeSprite`, and the only writer of the format outside the browser.
pub fn encode(s: &Sprite) -> Vec<u8> {
    let name = name_bytes(&s.name);
    let w = s.w.clamp(1, SPRITE_MAX_EDGE);
    let h = s.h.clamp(1, SPRITE_MAX_EDGE);
    let frames = s.frames.clamp(1, SPRITE_MAX_FRAMES);
    let colors = s.palette.len().min(SPRITE_MAX_COLORS);
    let mut out = Vec::with_capacity(record_len(name.len(), colors, w, h, frames));
    out.extend_from_slice(&SPRITE_MAGIC);
    out.push(SPRITE_VERSION);
    out.push(w as u8);
    out.push(h as u8);
    out.push(frames as u8);
    out.push(s.fps.min(SPRITE_MAX_FPS));
    out.push(colors as u8);
    out.push(name.len() as u8);
    out.push(0); // flags
    out.extend_from_slice(&name);
    for c in s.palette.iter().take(colors) {
        out.extend_from_slice(c);
    }
    for i in 0..w * h * frames {
        let k = s.index.get(i).copied().unwrap_or(0);
        out.push(if k as usize > colors { 0 } else { k });
    }
    out
}

/// The name as 1..=64 UTF-8 bytes, never split mid-code-point.
fn name_bytes(name: &str) -> Vec<u8> {
    let mut s = name.trim().to_string();
    if s.is_empty() {
        s = String::from("Sprite");
    }
    while s.len() > SPRITE_MAX_NAME {
        s.pop();
        if s.is_empty() {
            return b"Sprite".to_vec();
        }
    }
    s.into_bytes()
}

/// A colour the drawing is actually made of, and the index that paints it.
fn used_colors(s: &Sprite) -> Vec<(u8, [u8; 3])> {
    let mut seen = [false; 256];
    let mut out = Vec::new();
    for &k in &s.index {
        if k == 0 || seen[k as usize] {
            continue;
        }
        match s.palette.get(k as usize - 1) {
            Some(rgb) => {
                seen[k as usize] = true;
                out.push((k, *rgb));
            }
            None => continue,
        }
    }
    out
}

/// Drop palette entries nothing paints with, renumbering the index — the
/// mirror of `compactPalette`, EARLY RETURN INCLUDED: when every entry is used
/// the palette keeps its order rather than being re-sorted into
/// first-appearance order, and the two languages must agree about that.
pub fn compact_palette(s: Sprite) -> Sprite {
    let used = used_colors(&s);
    if used.len() == s.palette.len() {
        return s;
    }
    let mut remap = [0u8; 256];
    for (at, (k, _)) in used.iter().enumerate() {
        remap[*k as usize] = at as u8 + 1;
    }
    let index = s.index.iter().map(|&k| remap[k as usize]).collect();
    Sprite {
        palette: used.iter().map(|(_, rgb)| *rgb).collect(),
        index,
        ..s
    }
}

// ---- fit ----

#[derive(Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

struct Rects {
    src: Rect,
    dst: Rect,
}

fn placement(sw: usize, sh: usize, tw: usize, th: usize, fit: FitMode) -> Rects {
    let (swf, shf, twf, thf) = (sw as f64, sh as f64, tw as f64, th as f64);
    let whole = Rect { x: 0.0, y: 0.0, w: swf, h: shf };
    let box_ = Rect { x: 0.0, y: 0.0, w: twf, h: thf };
    if fit == FitMode::Fill {
        return Rects { src: whole, dst: box_ };
    }
    let sa = swf / shf;
    let ta = twf / thf;
    if fit == FitMode::Crop {
        let mut cw = swf;
        let mut ch = shf;
        if sa > ta {
            cw = shf * ta;
        } else {
            ch = swf / ta;
        }
        return Rects {
            src: Rect { x: (swf - cw) / 2.0, y: (shf - ch) / 2.0, w: cw, h: ch },
            dst: box_,
        };
    }
    let mut dw = twf;
    let mut dh = thf;
    if sa > ta {
        dh = twf / sa;
    } else {
        dw = thf * sa;
    }
    Rects {
        src: whole,
        dst: Rect { x: (twf - dw) / 2.0, y: (thf - dh) / 2.0, w: dw, h: dh },
    }
}

#[allow(clippy::too_many_arguments)]
fn resample_frame(
    rgba: &[u8],
    sw: usize,
    sh: usize,
    o: &Options,
    rects: &Rects,
    rgb: &mut [u8],
    on: &mut [u8],
    at: usize,
) {
    let (src, dst) = (rects.src, rects.dst);
    for y in 0..o.h {
        let cy = y as f64 + 0.5;
        for x in 0..o.w {
            let cx = x as f64 + 0.5;
            let i = at + y * o.w + x;
            if cx < dst.x || cx >= dst.x + dst.w || cy < dst.y || cy >= dst.y + dst.h {
                continue;
            }
            let got = match o.resample {
                Resample::Area => area_sample(rgba, sw, sh, &src, &dst, x, y),
                Resample::Nearest => nearest_sample(rgba, sw, sh, &src, &dst, cx, cy),
            };
            let Some(px) = got else { continue };
            if (px[3] as u16) < o.alpha_threshold {
                continue;
            }
            on[i] = 1;
            rgb[i * 3] = px[0];
            rgb[i * 3 + 1] = px[1];
            rgb[i * 3 + 2] = px[2];
        }
    }
}

fn pixel(rgba: &[u8], at: usize) -> [u8; 4] {
    [
        rgba.get(at).copied().unwrap_or(0),
        rgba.get(at + 1).copied().unwrap_or(0),
        rgba.get(at + 2).copied().unwrap_or(0),
        rgba.get(at + 3).copied().unwrap_or(0),
    ]
}

fn nearest_sample(
    rgba: &[u8],
    sw: usize,
    sh: usize,
    src: &Rect,
    dst: &Rect,
    cx: f64,
    cy: f64,
) -> Option<[u8; 4]> {
    let u = src.x + ((cx - dst.x) * src.w) / dst.w;
    let v = src.y + ((cy - dst.y) * src.h) / dst.h;
    let sx = clamp_floor(u, 0, sw.saturating_sub(1));
    let sy = clamp_floor(v, 0, sh.saturating_sub(1));
    Some(pixel(rgba, (sy * sw + sx) * 4))
}

/// Alpha-weighted average over the source box the target pixel covers, with
/// TRUE area weights.
fn area_sample(
    rgba: &[u8],
    sw: usize,
    sh: usize,
    src: &Rect,
    dst: &Rect,
    x: usize,
    y: usize,
) -> Option<[u8; 4]> {
    let xf = x as f64;
    let yf = y as f64;
    let ux0 = src.x + ((xf - dst.x) * src.w) / dst.w;
    let ux1 = src.x + ((xf + 1.0 - dst.x) * src.w) / dst.w;
    let uy0 = src.y + ((yf - dst.y) * src.h) / dst.h;
    let uy1 = src.y + ((yf + 1.0 - dst.y) * src.h) / dst.h;
    let ax0 = 0.0f64.max(ux0);
    let ax1 = (sw as f64).min(ux1);
    let ay0 = 0.0f64.max(uy0);
    let ay1 = (sh as f64).min(uy1);
    if !(ax1 > ax0) || !(ay1 > ay0) {
        return nearest_sample(rgba, sw, sh, src, dst, xf + 0.5, yf + 0.5);
    }
    let mut sr = 0.0f64;
    let mut sg = 0.0f64;
    let mut sb = 0.0f64;
    let mut sa = 0.0f64;
    let mut sarea = 0.0f64;
    let py0 = ay0.floor() as i64;
    let py1 = (py0 + 1).max(ay1.ceil() as i64);
    let px0 = ax0.floor() as i64;
    let px1 = (px0 + 1).max(ax1.ceil() as i64);
    for py in py0..py1 {
        if py < 0 || py >= sh as i64 {
            continue;
        }
        let wy = (py as f64 + 1.0).min(ay1) - (py as f64).max(ay0);
        if !(wy > 0.0) {
            continue;
        }
        for px in px0..px1 {
            if px < 0 || px >= sw as i64 {
                continue;
            }
            let wx = (px as f64 + 1.0).min(ax1) - (px as f64).max(ax0);
            if !(wx > 0.0) {
                continue;
            }
            let area = wx * wy;
            let p = pixel(rgba, (py as usize * sw + px as usize) * 4);
            let wa = area * p[3] as f64;
            sr += p[0] as f64 * wa;
            sg += p[1] as f64 * wa;
            sb += p[2] as f64 * wa;
            sa += wa;
            sarea += area;
        }
    }
    if !(sarea > 0.0) {
        return None;
    }
    let avg_a = sa / sarea;
    if !(sa > 0.0) {
        return Some([0, 0, 0, clamp_round(avg_a, 0, 255) as u8]);
    }
    Some([
        clamp_round(sr / sa, 0, 255) as u8,
        clamp_round(sg / sa, 0, 255) as u8,
        clamp_round(sb / sa, 0, 255) as u8,
        clamp_round(avg_a, 0, 255) as u8,
    ])
}

// ---- quantize ----

/// Distinct opaque colours and how often each occurs, in first-appearance
/// order. Parallel vectors, because the median cut sorts indices into them.
struct Histogram {
    r: Vec<u8>,
    g: Vec<u8>,
    b: Vec<u8>,
    count: Vec<u64>,
}

impl Histogram {
    fn colors(&self) -> Vec<[u8; 3]> {
        (0..self.count.len()).map(|i| [self.r[i], self.g[i], self.b[i]]).collect()
    }

    fn packed(&self, i: usize) -> u32 {
        ((self.r[i] as u32) << 16) | ((self.g[i] as u32) << 8) | self.b[i] as u32
    }
}

fn histogram(rgb: &[u8], on: &[u8], texels: usize) -> Histogram {
    let mut h = Histogram { r: Vec::new(), g: Vec::new(), b: Vec::new(), count: Vec::new() };
    let mut seen: HashMap<u32, usize> = HashMap::new();
    for i in 0..texels {
        if on.get(i).copied().unwrap_or(0) == 0 {
            continue;
        }
        let r = rgb.get(i * 3).copied().unwrap_or(0);
        let g = rgb.get(i * 3 + 1).copied().unwrap_or(0);
        let b = rgb.get(i * 3 + 2).copied().unwrap_or(0);
        let key = ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
        match seen.get(&key) {
            Some(&at) => h.count[at] += 1,
            None => {
                seen.insert(key, h.count.len());
                h.r.push(r);
                h.g.push(g);
                h.b.push(b);
                h.count.push(1);
            }
        }
    }
    h
}

/// Median cut down to `max` colours — deterministic to the last tie: the
/// bucket split next is the one with the most texels (ties by position), the
/// axis is the widest (ties r, then g, then b), entries are ordered by that
/// channel with the packed colour as the tie-break, and the cut is where the
/// running texel count first reaches half.
fn median_cut(h: &Histogram, max: usize) -> Vec<[u8; 3]> {
    let n = h.count.len();
    if n == 0 {
        return Vec::new();
    }
    let mut buckets: Vec<Vec<usize>> = vec![(0..n).collect()];
    while buckets.len() < max {
        let mut pick: Option<usize> = None;
        let mut best: i128 = -1;
        for (i, b) in buckets.iter().enumerate() {
            if b.len() < 2 {
                continue;
            }
            let total = bucket_count(h, b) as i128;
            if total > best {
                best = total;
                pick = Some(i);
            }
        }
        let Some(pick) = pick else { break };
        let axis = widest_axis(h, &buckets[pick]);
        let chan = |i: usize| -> u8 {
            match axis {
                0 => h.r[i],
                1 => h.g[i],
                _ => h.b[i],
            }
        };
        let mut sorted = buckets[pick].clone();
        sorted.sort_by(|&p, &q| chan(p).cmp(&chan(q)).then(h.packed(p).cmp(&h.packed(q))));
        let total = bucket_count(h, &sorted);
        let mut acc = 0u64;
        let mut cut = 1usize;
        for i in 0..sorted.len() - 1 {
            acc += h.count[sorted[i]];
            cut = i + 1;
            if acc * 2 >= total {
                break;
            }
        }
        let right = sorted.split_off(cut);
        buckets[pick] = sorted;
        buckets.insert(pick + 1, right);
    }
    buckets.iter().map(|b| representative(h, b)).collect()
}

fn bucket_count(h: &Histogram, b: &[usize]) -> u64 {
    b.iter().map(|&i| h.count[i]).sum()
}

/// 0 = red, 1 = green, 2 = blue — the widest extent, ties in that order.
fn widest_axis(h: &Histogram, b: &[usize]) -> usize {
    let mut lo = [255u8; 3];
    let mut hi = [0u8; 3];
    for &i in b {
        let c = [h.r[i], h.g[i], h.b[i]];
        for k in 0..3 {
            if c[k] < lo[k] {
                lo[k] = c[k];
            }
            if c[k] > hi[k] {
                hi[k] = c[k];
            }
        }
    }
    let mut axis = 0;
    let mut span = hi[0] as i32 - lo[0] as i32;
    for k in 1..3 {
        let s = hi[k] as i32 - lo[k] as i32;
        if s > span {
            span = s;
            axis = k;
        }
    }
    axis
}

fn representative(h: &Histogram, b: &[usize]) -> [u8; 3] {
    let mut sr = 0f64;
    let mut sg = 0f64;
    let mut sb = 0f64;
    let mut sc = 0f64;
    for &i in b {
        let c = h.count[i] as f64;
        sr += h.r[i] as f64 * c;
        sg += h.g[i] as f64 * c;
        sb += h.b[i] as f64 * c;
        sc += c;
    }
    if sc == 0.0 {
        return [0, 0, 0];
    }
    [
        clamp_round(sr / sc, 0, 255) as u8,
        clamp_round(sg / sc, 0, 255) as u8,
        clamp_round(sb / sc, 0, 255) as u8,
    ]
}

/// The palette entry closest to `(r, g, b)` in plain RGB distance, as a
/// 1-based index; ties go to the lower entry.
pub fn nearest_index(palette: &[[u8; 3]], r: i32, g: i32, b: i32) -> u8 {
    if palette.is_empty() {
        return 0;
    }
    let mut best = 0usize;
    let mut best_d = i32::MAX;
    for (k, c) in palette.iter().enumerate() {
        let dr = r - c[0] as i32;
        let dg = g - c[1] as i32;
        let db = b - c[2] as i32;
        let d = dr * dr + dg * dg + db * db;
        if d < best_d {
            best_d = d;
            best = k;
        }
    }
    best as u8 + 1
}

fn map_nearest(rgb: &[u8], on: &[u8], texels: usize, palette: &[[u8; 3]]) -> Vec<u8> {
    let mut index = vec![0u8; texels];
    let mut memo: HashMap<u32, u8> = HashMap::new();
    for i in 0..texels {
        if on.get(i).copied().unwrap_or(0) == 0 {
            continue;
        }
        let r = rgb[i * 3];
        let g = rgb[i * 3 + 1];
        let b = rgb[i * 3 + 2];
        let key = ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
        let k = match memo.get(&key) {
            Some(&k) => k,
            None => {
                let k = nearest_index(palette, r as i32, g as i32, b as i32);
                memo.insert(key, k);
                k
            }
        };
        index[i] = k;
    }
    index
}

/// Floyd–Steinberg, per frame, in INTEGER arithmetic — the one way two
/// languages can agree on it. Off by default.
fn map_dithered(
    rgb: &[u8],
    on: &[u8],
    w: usize,
    h: usize,
    frames: usize,
    palette: &[[u8; 3]],
) -> Vec<u8> {
    let n = w * h;
    let mut index = vec![0u8; n * frames.max(1)];
    for f in 0..frames {
        let mut cur = [vec![0i32; w], vec![0i32; w], vec![0i32; w]];
        let mut next = [vec![0i32; w], vec![0i32; w], vec![0i32; w]];
        for y in 0..h {
            for c in 0..3 {
                cur[c].copy_from_slice(&next[c]);
                next[c].iter_mut().for_each(|v| *v = 0);
            }
            for x in 0..w {
                let i = f * n + y * w + x;
                if on.get(i).copied().unwrap_or(0) == 0 {
                    continue;
                }
                let mut want = [0i32; 3];
                for c in 0..3 {
                    want[c] = (rgb[i * 3 + c] as i32 + cur[c][x]).clamp(0, 255);
                }
                let k = nearest_index(palette, want[0], want[1], want[2]);
                index[i] = k;
                let got = palette.get(k as usize - 1).copied().unwrap_or([0, 0, 0]);
                for c in 0..3 {
                    let d = want[c] - got[c] as i32;
                    if d == 0 {
                        continue;
                    }
                    if x + 1 < w {
                        cur[c][x + 1] += (d * 7) / 16;
                    }
                    if x > 0 {
                        next[c][x - 1] += (d * 3) / 16;
                    }
                    next[c][x] += (d * 5) / 16;
                    if x + 1 < w {
                        next[c][x + 1] += d / 16;
                    }
                }
            }
        }
    }
    index
}

// ---- the 16 KiB cap, and the two knobs that get under it ----

/// How many bytes a record of this shape would take.
///
/// The CLI never needs it — it always has the real record — but the browser's
/// readout is drawn from `plannedBytes` before the pipeline runs, and the twins
/// agreeing about the arithmetic is asserted by `tests/spriteimport.rs`.
#[allow(dead_code)]
pub fn planned_bytes(o: &Options, frames: usize) -> usize {
    SPRITE_HDR
        + sprite_name(&o.name).len()
        + 3 * o.colors.min(SPRITE_MAX_COLORS)
        + o.w * o.h * frames.max(1)
}

/// What [`fit_under_cap`] changed, as one sentence.
pub struct CapFix {
    pub options: Options,
    /// Empty when nothing had to change.
    pub said: String,
}

/// The two knobs, turned far enough — `keep_every` first (frames are what a
/// record spends its bytes on), then the palette, and the target size only as
/// the last resort.
pub fn fit_under_cap(src: &SourceImage, opts: &Options, max_bytes: usize) -> CapFix {
    let o = resolve_options(opts);
    let total = src.frames.len();
    let fixed = SPRITE_HDR + o.name.len();
    let mut said: Vec<String> = Vec::new();

    let mut next = o.clone();
    let frames_now = kept_indices(total, next.keep_every).len().min(SPRITE_MAX_FRAMES);
    if fixed + 3 * next.colors + next.w * next.h * frames_now <= max_bytes {
        return CapFix { options: next, said: String::new() };
    }

    // 1. frames
    let room = max_bytes.saturating_sub(fixed + 3 * next.colors);
    let per_frame = next.w * next.h;
    let fits = room / per_frame.max(1);
    if fits >= 1 && total > 1 {
        let cap = fits.min(SPRITE_MAX_FRAMES);
        let every = total.div_ceil(cap).max(1);
        if every > next.keep_every {
            next.keep_every = every;
            said.push(format!("keeping every {} frame", ordinal(every)));
        }
    }
    let mut frames = kept_indices(total, next.keep_every).len().min(SPRITE_MAX_FRAMES);
    if fixed + 3 * next.colors + per_frame * frames.max(1) <= max_bytes {
        return CapFix { options: next, said: said.join(" and ") };
    }

    // 2. colours
    let left = max_bytes.saturating_sub(fixed + per_frame * frames.max(1));
    let colors = left / 3;
    if colors >= 2 {
        next.colors = next.colors.min(colors);
        said.push(format!("{} colours", next.colors));
        return CapFix { options: next, said: said.join(" and ") };
    }

    // 3. size
    let (ow, oh) = (next.w, next.h);
    let mut w = next.w;
    let mut h = next.h;
    while w > 1 && h > 1 && fixed + 6 + w * h * frames.max(1) > max_bytes {
        w = (w - 1).max(1);
        h = clamp_round((oh * w) as f64 / ow as f64, 1, SPRITE_MAX_EDGE);
        frames = kept_indices(total, next.keep_every).len().min(SPRITE_MAX_FRAMES);
    }
    let room2 = max_bytes.saturating_sub(fixed + w * h * frames.max(1));
    next.w = w;
    next.h = h;
    next.colors = (room2 / 3).clamp(1, SPRITE_MAX_COLORS);
    said.push(format!("{w}×{h} and {} colours", next.colors));
    CapFix { options: next, said: said.join(", ") }
}

fn ordinal(n: usize) -> String {
    match n {
        2 => String::from("2nd"),
        3 => String::from("3rd"),
        _ => format!("{n}th"),
    }
}

/// `floor(x + 0.5)`, clamped — the ONE rounding the twin pipelines share.
fn clamp_round(x: f64, lo: usize, hi: usize) -> usize {
    clamp_floor(x + 0.5, lo, hi)
}

fn clamp_floor(x: f64, lo: usize, hi: usize) -> usize {
    if !x.is_finite() {
        return lo;
    }
    let n = x.floor();
    if n < lo as f64 {
        lo
    } else if n > hi as f64 {
        hi
    } else {
        n as usize
    }
}

// ---- decoding (the `image` crate) ----

/// Read `path` as frames of RGBA. Animated GIF and WebP give every frame with
/// its delay; everything else gives one.
pub fn decode_file(path: &str) -> Result<SourceImage, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let format = image::guess_format(&bytes)
        .map_err(|_| format!("{path}: not an image this build can decode (PNG, GIF, JPEG, WebP, BMP)"))?;
    match format {
        image::ImageFormat::Gif => decode_frames(
            image::codecs::gif::GifDecoder::new(std::io::Cursor::new(&bytes))
                .map_err(|e| format!("{path}: {e}"))?,
            path,
        ),
        image::ImageFormat::WebP => {
            let dec = image::codecs::webp::WebPDecoder::new(std::io::Cursor::new(&bytes))
                .map_err(|e| format!("{path}: {e}"))?;
            if dec.has_animation() {
                decode_frames(dec, path)
            } else {
                decode_still(&bytes, path)
            }
        }
        _ => decode_still(&bytes, path),
    }
}

fn decode_frames<'a, D: image::AnimationDecoder<'a>>(
    dec: D,
    path: &str,
) -> Result<SourceImage, String> {
    let frames = dec.into_frames().collect_frames().map_err(|e| format!("{path}: {e}"))?;
    if frames.is_empty() {
        return Err(format!("{path}: the file holds no frames"));
    }
    let mut out = SourceImage { w: 0, h: 0, frames: Vec::new() };
    for f in frames {
        let (num, den) = f.delay().numer_denom_ms();
        let delay_ms = if den == 0 { 0 } else { (num / den) as i64 };
        let buf = f.into_buffer();
        out.w = buf.width() as usize;
        out.h = buf.height() as usize;
        out.frames.push(SourceFrame { rgba: buf.into_raw(), delay_ms });
    }
    Ok(out)
}

fn decode_still(bytes: &[u8], path: &str) -> Result<SourceImage, String> {
    let img = image::load_from_memory(bytes).map_err(|e| format!("{path}: {e}"))?;
    let rgba = img.to_rgba8();
    Ok(SourceImage {
        w: rgba.width() as usize,
        h: rgba.height() as usize,
        frames: vec![SourceFrame { rgba: rgba.into_raw(), delay_ms: 0 }],
    })
}

/// A record's own verdict on itself — what the device's routes would say.
pub fn check_record(bytes: &[u8]) -> Result<(), &'static str> {
    check(bytes)
}
