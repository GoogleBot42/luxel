//! Output pipeline: per-installation transforms applied to the final RGB
//! frame just before protocol encoding — color-order remap (strips differ
//! from their datasheets constantly), a global gamma LUT, and a power cap
//! that scales frames down when their estimated current draw exceeds a
//! configured budget. Pure `no_std` so the firmware and mirror share it
//! and it's testable on the host.

/// Wire color order: which logical channel each output slot carries.
/// Codes are stable (flash-persisted).
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ColorOrder(pub u8);

impl ColorOrder {
    pub const RGB: ColorOrder = ColorOrder(0);
    const NAMES: [&'static str; 6] = ["rgb", "rbg", "grb", "gbr", "brg", "bgr"];
    const PERMS: [[usize; 3]; 6] = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];

    pub fn from_name(s: &str) -> Option<ColorOrder> {
        let lower = s.trim().to_ascii_lowercase();
        Self::NAMES
            .iter()
            .position(|n| *n == lower)
            .map(|i| ColorOrder(i as u8))
    }

    pub fn name(self) -> &'static str {
        Self::NAMES[(self.0 as usize).min(5)]
    }

    pub fn perm(self) -> [usize; 3] {
        Self::PERMS[(self.0 as usize).min(5)]
    }
}

/// Gamma LUT for `gamma_tenths`/10 (e.g. 22 → γ 2.2). 0 and 10 mean "off"
/// (identity would be wasted work — callers skip the LUT entirely).
pub fn gamma_lut(gamma_tenths: u8) -> [u8; 256] {
    use crate::fixed::Fx;
    let g = Fx::from_raw(gamma_tenths as i32 * 65536 / 10);
    let mut lut = [0u8; 256];
    for (i, slot) in lut.iter_mut().enumerate() {
        let v = Fx::from_raw(((i as i32) << 16) / 255);
        let out = crate::fmath::pow(v, g);
        *slot = ((out.clamp(Fx::ZERO, Fx::ONE).raw() as i64 * 255) >> 16) as u8;
    }
    lut[255] = 255; // full stays full regardless of rounding
    lut
}

/// Perceptual brightness curve for the master dimmer: `brightness5` (0–31)
/// reshaped by `curve_tenths`/10 so a linear slider *feels* linear. 0 and 10
/// mean "off" (identity). γ > 1 pushes the useful range up the slider (the
/// usual fix for "everything above 20% looks the same"); γ < 1 does the
/// opposite. A non-zero input never curves to 0 — a lit strip must stay lit.
///
/// This is the *control* transfer, not the *content* one: [gamma_lut] shapes
/// every pixel's channels, this shapes only the global dimmer.
pub fn curve_brightness(brightness5: u8, curve_tenths: u8) -> u8 {
    use crate::fixed::Fx;
    let b = brightness5 & 0x1F;
    if curve_tenths == 0 || curve_tenths == 10 || b == 0 || b >= 31 {
        return b;
    }
    let g = Fx::from_raw(curve_tenths as i32 * 65536 / 10);
    let v = Fx::from_raw(((b as i32) << 16) / 31);
    let out = crate::fmath::pow(v, g).clamp(Fx::ZERO, Fx::ONE);
    (((out.raw() as i64 * 31 + 32768) >> 16) as u8).max(1)
}

/// Rec.601-ish luma of a frame pixel, 0–255 — the index the palette-remap
/// stage looks colors up by.
pub fn luma(px: [u8; 3]) -> u8 {
    ((px[0] as u32 * 54 + px[1] as u32 * 183 + px[2] as u32 * 19) >> 8) as u8
}

/// Stop cap for a *device-level* output palette (the persisted one, not a
/// pattern's `setOutputPalette`, which is bounded by the pattern's own
/// array budget). The cooked table has 256 entries, so 32 stops is already
/// finer than the output can show, and it keeps the flash record small.
/// Defined here so the firmware, the native mirror and the web UI can't
/// disagree about the limit.
pub const MAX_OUTPUT_PALETTE_STOPS: usize = 32;

/// Parse the device-palette wire form both servers accept: whitespace-
/// separated integers, `<amount_pct> <pos> <r> <g> <b> …`, components
/// 0..=255 and amount 0..=100 — the flat `[pos,r,g,b,…]` shape the
/// `setOutputPalette` builtin takes, in the byte domain the flash record
/// stores. Returns `(amount_pct, stops)` or the message to report.
#[allow(clippy::type_complexity)]
pub fn parse_palette_stops(body: &str) -> Result<(u8, alloc::vec::Vec<(u8, [u8; 3])>), &'static str> {
    let mut it = body.split_whitespace();
    let amount_pct: u8 = it
        .next()
        .and_then(|v| v.parse().ok())
        .filter(|a| *a <= 100)
        .ok_or("first token must be the blend amount, 0..=100 percent")?;
    // one pass, no intermediate Vec: the firmware pays for every extra
    // iterator/collect shape in image bytes
    let mut stops: alloc::vec::Vec<(u8, [u8; 3])> = alloc::vec::Vec::new();
    let mut group = [0u8; 4];
    let mut n = 0usize;
    for tok in it {
        group[n] = tok
            .parse::<u8>()
            .map_err(|_| "palette components must be integers 0..=255")?;
        n += 1;
        if n == 4 {
            if stops.len() == MAX_OUTPUT_PALETTE_STOPS {
                return Err("too many palette stops (max 32)");
            }
            stops.push((group[0], [group[1], group[2], group[3]]));
            n = 0;
        }
    }
    if n != 0 || stops.is_empty() {
        return Err("expected groups of 4: <pos> <r> <g> <b>");
    }
    // sample_palette walks the list in order — an unsorted one would sample
    // nonsense rather than fail, so reject it at the door
    if stops.windows(2).any(|w| w[0].0 > w[1].0) {
        return Err("palette stops must be in ascending position order");
    }
    Ok((amount_pct, stops))
}

/// Cook the luma → color table [palette_remap_frame] indexes: entry `i` is
/// the stop list sampled at `i/255`, quantized the way the engine quantizes
/// pixels. Filled in place so neither caller puts 768 bytes on its stack —
/// the engine caches one behind its palette epoch, the firmware one behind
/// the device-palette epoch.
pub fn fill_palette_lut(pal: &[(crate::fixed::Fx, [crate::fixed::Fx; 3])], lut: &mut [[u8; 3]; 256]) {
    use crate::fixed::Fx;
    // floor(v·255), matching engine::quantize (PB-exact, oracle-checked)
    let q = |v: Fx| ((v.clamp(Fx::ZERO, Fx::ONE).raw() as i64 * 255) >> 16) as u8;
    for (i, slot) in lut.iter_mut().enumerate() {
        let c = crate::vm::sample_palette(pal, Fx::from_raw(((i as i32) << 16) / 255));
        *slot = [q(c[0]), q(c[1]), q(c[2])];
    }
}

/// Recolor a finished frame through a 256-entry palette LUT indexed by
/// [luma]: the pattern's *structure* survives, its hues are replaced.
/// `amount` is the blend in 1/256ths (256 = full replace, 0 = no-op).
pub fn palette_remap_frame(frame: &mut [[u8; 3]], lut: &[[u8; 3]; 256], amount: u32) {
    if amount == 0 {
        return;
    }
    let a = amount.min(256);
    for px in frame.iter_mut() {
        let target = lut[luma(*px) as usize];
        if a >= 256 {
            *px = target;
            continue;
        }
        for c in 0..3 {
            px[c] = ((px[c] as u32 * (256 - a) + target[c] as u32 * a) >> 8) as u8;
        }
    }
}

/// 3-tap blur along the pixel index, in place and allocation-free (the one
/// value it needs to remember is the previous pixel's pre-blur color).
/// `k` is each neighbor's weight in 1/256ths, 0–128: 128 is a pure neighbor
/// average, 64 the classic 1-2-1 kernel, 0 a no-op. Ends clamp (the last
/// pixel stands in for its own missing neighbor). `passes` widens the
/// effective radius at O(n) each.
///
/// Index space, not map space: on a strip that is physical order, on a
/// serpentine matrix it follows the wiring path.
pub fn blur_frame(frame: &mut [[u8; 3]], k: u32, passes: u8) {
    let k = k.min(128);
    if k == 0 || frame.len() < 2 {
        return;
    }
    let center = 256 - 2 * k;
    let last = frame.len() - 1;
    for _ in 0..passes.max(1) {
        let mut prev = frame[0];
        for i in 0..frame.len() {
            let cur = frame[i];
            let next = frame[(i + 1).min(last)];
            for c in 0..3 {
                let v = cur[c] as u32 * center + prev[c] as u32 * k + next[c] as u32 * k;
                frame[i][c] = ((v + 128) >> 8) as u8;
            }
            prev = cur;
        }
    }
}

/// A regular W×H grid recovered from an installed pixel map, so the spatial
/// stages can spread light in *map* space instead of along the wiring. Six
/// bytes and `Copy`: the layout is fully described by its dimensions plus
/// whether alternate rows run backwards, so there is no per-pixel neighbour
/// table to build, keep, or invalidate — and nothing to allocate per frame.
///
/// "Row" is whichever axis the pixel indices walk first; a column-wired panel
/// is the same structure transposed, and a separable blur treats both axes
/// alike, so the distinction never reaches the kernels.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GridMap {
    /// Pixels per row (the run the index walks before the other axis moves).
    pub w: u16,
    /// Number of rows.
    pub h: u16,
    /// Serpentine wiring: odd rows run backwards along the row axis.
    pub serpentine: bool,
}

impl GridMap {
    pub fn len(&self) -> usize {
        self.w as usize * self.h as usize
    }

    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    /// Pixel index of grid cell (`row`, `col`). Both must be in range.
    #[inline]
    pub fn index(&self, row: usize, col: usize) -> usize {
        let w = self.w as usize;
        let col = if self.serpentine && row & 1 == 1 {
            w - 1 - col
        } else {
            col
        };
        row * w + col
    }
}

/// Recover a [GridMap] from an installed 2D pixel map, or `None` when the
/// layout isn't a regular grid walked row by row (then the callers keep their
/// index-space behavior).
///
/// Accepts any monotonic coordinate values — raw pattern units or the
/// engine's per-axis normalized ones, since only equality and ordering are
/// compared. What it requires is that the map really is a grid: contiguous
/// runs of pixels sharing one coordinate on the slow axis, every run the same
/// length and carrying the same fast-axis values (forwards, or backwards for
/// every other run), with both axes' values strictly monotonic so that
/// "neighbouring cell" means "neighbouring in space".
///
/// A globally mirrored or flipped grid needs no special case: the blur kernels
/// are symmetric and clamp at the edges, so mirroring the column or row
/// numbering describes the same neighbourhood. That is why `serpentine` is one
/// bit rather than "which rows are reversed".
pub fn detect_grid(dims: u8, coords: &[[crate::fixed::Fx; 3]]) -> Option<GridMap> {
    let n = coords.len();
    if dims != 2 || n < 4 || n > u16::MAX as usize {
        return None;
    }
    let at = |i: usize, axis: usize| coords[i][axis].raw();
    // Which axis moves between the first two pixels? That one runs along a
    // row; the other one only changes when a row ends.
    let (fast, slow) = if at(1, 1) == at(0, 1) && at(1, 0) != at(0, 0) {
        (0usize, 1usize)
    } else if at(1, 0) == at(0, 0) && at(1, 1) != at(0, 1) {
        (1usize, 0usize)
    } else {
        return None;
    };
    let w = (1..n).find(|&i| at(i, slow) != at(0, slow))?;
    let h = n / w;
    if w < 2 || h < 2 || w * h != n {
        return None;
    }
    // Row 0 defines the column coordinates; they must be strictly monotonic.
    let cols_ascend = at(1, fast) > at(0, fast);
    for c in 1..w {
        if (at(c, fast) > at(c - 1, fast)) != cols_ascend || at(c, fast) == at(c - 1, fast) {
            return None;
        }
    }
    let rows_ascend = at(w, slow) > at(0, slow);
    // Row 1 decides the wiring; every later row has to agree. Row 0 is the
    // reference direction by construction, which is what factors the mirror
    // out: a panel wired entirely backwards reads as progressive.
    let serpentine = at(w, fast) == at(w - 1, fast);
    for r in 0..h {
        let base = r * w;
        let row_slow = at(base, slow);
        if r > 0 {
            let prev = at(base - w, slow);
            if row_slow == prev || (row_slow > prev) != rows_ascend {
                return None; // rows out of spatial order
            }
        }
        let reversed = serpentine && r & 1 == 1;
        for c in 0..w {
            let want = if reversed { w - 1 - c } else { c };
            if at(base + c, fast) != at(want, fast) || at(base + c, slow) != row_slow {
                return None;
            }
        }
    }
    Some(GridMap {
        w: w as u16,
        h: h as u16,
        serpentine,
    })
}

/// Light-bleed bloom along the pixel index: each pixel takes the brighter of
/// itself and `g`/256 of its brightest neighbor, so highlights spread without
/// the frame losing energy the way [blur_frame] does. `g` 0 is a no-op.
/// Allocation-free, one pass, same index-space caveat as [blur_frame].
pub fn glow_frame(frame: &mut [[u8; 3]], g: u32) {
    let g = g.min(256);
    if g == 0 || frame.len() < 2 {
        return;
    }
    let last = frame.len() - 1;
    let mut prev = frame[0];
    for i in 0..frame.len() {
        let cur = frame[i];
        let next = frame[(i + 1).min(last)];
        for c in 0..3 {
            let bleed = ((prev[c].max(next[c]) as u32 * g) >> 8) as u8;
            frame[i][c] = cur[c].max(bleed);
        }
        prev = cur;
    }
}

/// Grid cell → pixel index for a sweep along rows (`along_rows`) or down
/// columns: `line` picks the line, `i` the position along it. One helper for
/// both axes so the kernels compile to a single copy of their inner loop
/// rather than one per axis — 370 B of image, measured.
#[inline]
fn cell(grid: &GridMap, along_rows: bool, line: usize, i: usize) -> usize {
    if along_rows {
        grid.index(line, i)
    } else {
        grid.index(i, line)
    }
}

/// Line count and length for a sweep along rows or down columns.
#[inline]
fn sweep(grid: &GridMap, along_rows: bool) -> (usize, usize) {
    let (w, h) = (grid.w as usize, grid.h as usize);
    if along_rows {
        (h, w)
    } else {
        (w, h)
    }
}

/// One 3-tap blur sweep over every row (or column) of the grid.
/// Allocation-free: the only state is the previous cell's pre-blur color,
/// exactly as in [blur_frame].
fn blur_axis(frame: &mut [[u8; 3]], grid: &GridMap, along_rows: bool, k: u32) {
    let (lines, len) = sweep(grid, along_rows);
    if len < 2 {
        return;
    }
    let center = 256 - 2 * k;
    for line in 0..lines {
        let mut prev = frame[cell(grid, along_rows, line, 0)];
        for i in 0..len {
            let idx = cell(grid, along_rows, line, i);
            let cur = frame[idx];
            let next = frame[cell(grid, along_rows, line, (i + 1).min(len - 1))];
            for c in 0..3 {
                let v = cur[c] as u32 * center + prev[c] as u32 * k + next[c] as u32 * k;
                frame[idx][c] = ((v + 128) >> 8) as u8;
            }
            prev = cur;
        }
    }
}

/// [blur_frame] in map space: a separable 2D blur over an installed grid,
/// rows then columns, so a spike spreads into a soft disc instead of smearing
/// along the wiring and folding back at every row end. Same `k`/`passes`
/// meaning as [blur_frame] — one pass is one row sweep plus one column sweep,
/// so the 2D kernel is the 1D one squared. Allocation-free; the caller must
/// have checked `grid.len() == frame.len()`.
pub fn blur_frame_grid(frame: &mut [[u8; 3]], grid: &GridMap, k: u32, passes: u8) {
    let k = k.min(128);
    if k == 0 || frame.len() < 2 || grid.len() != frame.len() {
        return;
    }
    for _ in 0..passes.max(1) {
        blur_axis(frame, grid, true, k);
        blur_axis(frame, grid, false, k);
    }
}

/// [glow_frame] in map space: the same brightest-neighbour bleed run along
/// rows and then columns, so a highlight blooms in both axes (the corner
/// cells pick it up at `g`²/256, a natural round falloff). Allocation-free;
/// the caller must have checked `grid.len() == frame.len()`.
pub fn glow_frame_grid(frame: &mut [[u8; 3]], grid: &GridMap, g: u32) {
    let g = g.min(256);
    if g == 0 || frame.len() < 2 || grid.len() != frame.len() {
        return;
    }
    glow_axis(frame, grid, true, g);
    glow_axis(frame, grid, false, g);
}

/// One bleed sweep over every row (or column) — [glow_frame]'s inner loop
/// with the index taken through the grid.
fn glow_axis(frame: &mut [[u8; 3]], grid: &GridMap, along_rows: bool, g: u32) {
    let (lines, len) = sweep(grid, along_rows);
    if len < 2 {
        return;
    }
    for line in 0..lines {
        let mut prev = frame[cell(grid, along_rows, line, 0)];
        for i in 0..len {
            let idx = cell(grid, along_rows, line, i);
            let cur = frame[idx];
            let next = frame[cell(grid, along_rows, line, (i + 1).min(len - 1))];
            for c in 0..3 {
                let bleed = ((prev[c].max(next[c]) as u32 * g) >> 8) as u8;
                frame[idx][c] = cur[c].max(bleed);
            }
            prev = cur;
        }
    }
}

/// Per-output current model for the power cap — a HUB75 matrix draws very
/// differently from an addressable strip.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PowerModel {
    /// Addressable strips: every pixel's LEDs conduct simultaneously,
    /// ~20 mA per full channel per pixel (the WS2812-class rule of thumb).
    Strip,
    /// HUB75 matrix: rows are time-multiplexed — only one row-pair per
    /// scan group conducts at any instant — so average draw is the strip
    /// estimate divided by the scan ratio (`scan` = panel rows / 2; 32
    /// for a 1/32-scan 64-row panel). Deliberately conservative: lands
    /// ~2x a typical 64x64 panel's rated full-white draw, and a power
    /// cap should overestimate.
    Hub75 { scan: u16 },
}

/// Estimated frame current in mA under `model`, scaled by the effective
/// brightness.
pub fn estimate_ma(frame: &[[u8; 3]], brightness5: u8, model: PowerModel) -> u32 {
    let sum: u64 = frame
        .iter()
        .map(|px| px[0] as u64 + px[1] as u64 + px[2] as u64)
        .sum();
    let strip = (sum * 20 * (brightness5 & 0x1F) as u64) / (255 * 31);
    match model {
        PowerModel::Strip => strip as u32,
        PowerModel::Hub75 { scan } => (strip / scan.max(1) as u64) as u32,
    }
}

/// Apply the pipeline in place: gamma LUT (if any), color-order remap, and
/// the power cap (uniform scale when the estimate exceeds `cap_ma`; 0 = no
/// cap). Returns the scale numerator/256 applied (255+ = none) for status.
pub fn apply(
    frame: &mut [[u8; 3]],
    order: ColorOrder,
    lut: Option<&[u8; 256]>,
    cap_ma: u32,
    brightness5: u8,
    model: PowerModel,
) -> u32 {
    if let Some(lut) = lut {
        for px in frame.iter_mut() {
            for c in px.iter_mut() {
                *c = lut[*c as usize];
            }
        }
    }
    if order != ColorOrder::RGB {
        let p = order.perm();
        for px in frame.iter_mut() {
            *px = [px[p[0]], px[p[1]], px[p[2]]];
        }
    }
    let mut scale = 256u32;
    if cap_ma > 0 {
        let est = estimate_ma(frame, brightness5, model);
        if est > cap_ma {
            scale = (cap_ma * 256 / est).max(1);
            for px in frame.iter_mut() {
                for c in px.iter_mut() {
                    *c = ((*c as u32 * scale) >> 8) as u8;
                }
            }
        }
    }
    scale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_round_trip_and_remap() {
        assert_eq!(ColorOrder::from_name("GRB").unwrap().0, 2);
        assert_eq!(ColorOrder(5).name(), "bgr");
        assert!(ColorOrder::from_name("xyz").is_none());
        let mut f = [[10, 20, 30]];
        apply(&mut f, ColorOrder::from_name("grb").unwrap(), None, 0, 31, PowerModel::Strip);
        assert_eq!(f[0], [20, 10, 30]); // wire gets G,R,B
        let mut f = [[10, 20, 30]];
        apply(&mut f, ColorOrder::from_name("bgr").unwrap(), None, 0, 31, PowerModel::Strip);
        assert_eq!(f[0], [30, 20, 10]);
    }

    #[test]
    fn gamma_curves_darken_midtones() {
        let lut = gamma_lut(22); // γ 2.2
        assert_eq!(lut[0], 0);
        assert_eq!(lut[255], 255);
        assert!(lut[128] < 60, "γ2.2 midpoint ≈ 55, got {}", lut[128]);
        // monotonic
        assert!(lut.windows(2).all(|w| w[0] <= w[1]));
        let mut f = [[128, 128, 128]];
        apply(&mut f, ColorOrder::RGB, Some(&lut), 0, 31, PowerModel::Strip);
        assert_eq!(f[0][0], lut[128]);
    }

    #[test]
    fn blur_spreads_a_spike_and_conserves_ends() {
        // k = 64 is the 1-2-1 kernel: 255 → 128 with 64 either side
        let mut f = [[0u8; 3], [0; 3], [255; 3], [0; 3], [0; 3]];
        blur_frame(&mut f, 64, 1);
        assert_eq!(f.map(|p| p[0]), [0, 64, 128, 64, 0]);
        // a second pass widens it further (the ends clamp, so light that
        // reaches pixel 0 stays there rather than falling off the strip)
        blur_frame(&mut f, 64, 1);
        assert_eq!(f.map(|p| p[0]), [16, 64, 96, 64, 16]);
        // k = 0 and single-pixel frames are no-ops
        let mut g = [[7u8, 8, 9], [1, 2, 3]];
        blur_frame(&mut g, 0, 4);
        assert_eq!(g, [[7, 8, 9], [1, 2, 3]]);
        // a flat frame survives a full-strength blur unchanged
        let mut flat = [[100u8; 3]; 6];
        blur_frame(&mut flat, 128, 3);
        assert_eq!(flat, [[100u8; 3]; 6]);
    }

    #[test]
    fn glow_bleeds_without_dimming() {
        let mut f = [[0u8; 3], [0; 3], [255; 3], [0; 3], [0; 3]];
        glow_frame(&mut f, 128); // neighbours at half strength
        assert_eq!(f.map(|p| p[0]), [0, 127, 255, 127, 0]);
        // unlike blur, the source pixel keeps its full value
        let mut g = [[0u8; 3], [200; 3], [0; 3]];
        glow_frame(&mut g, 0);
        assert_eq!(g.map(|p| p[0]), [0, 200, 0]);
    }

    /// A W×H map wired row by row, optionally serpentine, in the coordinate
    /// units `lx_set_map_grid` / the device map wire format produce.
    fn grid_coords(w: usize, h: usize, serpentine: bool) -> alloc::vec::Vec<[crate::fixed::Fx; 3]> {
        use crate::fixed::Fx;
        let mut v = alloc::vec::Vec::new();
        for r in 0..h {
            for c in 0..w {
                let x = if serpentine && r % 2 == 1 { w - 1 - c } else { c };
                v.push([Fx::from_int(x as i32), Fx::from_int(r as i32), Fx::ZERO]);
            }
        }
        v
    }

    fn red(frame: &[[u8; 3]], g: &GridMap, row: usize, col: usize) -> u8 {
        frame[g.index(row, col)][0]
    }

    #[test]
    fn detect_grid_reads_the_wiring() {
        let prog = detect_grid(2, &grid_coords(8, 4, false)).expect("progressive grid");
        assert_eq!(prog, GridMap { w: 8, h: 4, serpentine: false });
        let snake = detect_grid(2, &grid_coords(8, 4, true)).expect("serpentine grid");
        assert_eq!(snake, GridMap { w: 8, h: 4, serpentine: true });
        // the two disagree about where index 8 sits, and agree about index 0
        assert_eq!(prog.index(1, 0), 8);
        assert_eq!(snake.index(1, 0), 15);
        assert_eq!(prog.index(0, 0), snake.index(0, 0));
        // a column-wired panel is the same grid transposed (a separable blur
        // treats both axes alike, so nothing downstream cares which is which)
        let mut colwise = grid_coords(4, 8, false);
        for c in colwise.iter_mut() {
            c.swap(0, 1);
        }
        assert_eq!(
            detect_grid(2, &colwise),
            Some(GridMap { w: 4, h: 8, serpentine: false })
        );
        // an entirely backwards panel is a mirror, not a third wiring
        let mut mirrored = grid_coords(4, 4, false);
        for c in mirrored.iter_mut() {
            c[0] = crate::fixed::Fx::from_int(3) - c[0];
        }
        assert_eq!(
            detect_grid(2, &mirrored),
            Some(GridMap { w: 4, h: 4, serpentine: false })
        );
    }

    #[test]
    fn detect_grid_rejects_non_grids() {
        use crate::fixed::Fx;
        // a strip: one row, nothing to blur in a second axis
        let strip: alloc::vec::Vec<[Fx; 3]> = (0..16)
            .map(|i| [Fx::from_int(i), Fx::ZERO, Fx::ZERO])
            .collect();
        assert_eq!(detect_grid(2, &strip), None);
        // ragged: 10 pixels over a 4-wide layout
        let mut ragged = grid_coords(4, 3, false);
        ragged.truncate(10);
        assert_eq!(detect_grid(2, &ragged), None);
        // rows out of spatial order (0, 2, 1, 3) — index-adjacent rows that
        // aren't neighbours would blur the wrong cells together
        let src = grid_coords(4, 4, false);
        let mut scrambled = alloc::vec::Vec::new();
        for r in [0usize, 2, 1, 3] {
            scrambled.extend_from_slice(&src[r * 4..r * 4 + 4]);
        }
        assert_eq!(detect_grid(2, &scrambled), None);
        // a duplicated column value isn't a grid either
        let mut dup = grid_coords(4, 4, false);
        for r in 0..4 {
            dup[r * 4 + 2][0] = dup[r * 4 + 1][0];
        }
        assert_eq!(detect_grid(2, &dup), None);
        // 3D maps and degenerate sizes stay index-space
        assert_eq!(detect_grid(3, &grid_coords(4, 4, false)), None);
        assert_eq!(detect_grid(2, &grid_coords(2, 1, false)), None);
        // an arbitrary scatter (a circle-ish map) is not a grid
        let ring: alloc::vec::Vec<[Fx; 3]> = (0..12)
            .map(|i| [Fx::from_int(i % 5), Fx::from_int((i * 7) % 11), Fx::ZERO])
            .collect();
        assert_eq!(detect_grid(2, &ring), None);
    }

    #[test]
    fn grid_blur_spreads_in_both_axes() {
        let g = detect_grid(2, &grid_coords(5, 5, false)).unwrap();
        let mut f = alloc::vec![[0u8; 3]; 25];
        f[g.index(2, 2)] = [255; 3];
        blur_frame_grid(&mut f, &g, 64, 1);
        // rows then columns with the 1-2-1 kernel: the peak keeps a quarter,
        // the four orthogonal neighbours an eighth, the diagonals a sixteenth
        assert_eq!(red(&f, &g, 2, 2), 64);
        for (r, c) in [(1, 2), (3, 2), (2, 1), (2, 3)] {
            assert_eq!(red(&f, &g, r, c), 32, "orthogonal ({r},{c})");
        }
        for (r, c) in [(1, 1), (1, 3), (3, 1), (3, 3)] {
            assert_eq!(red(&f, &g, r, c), 16, "diagonal ({r},{c})");
        }
        // nothing two cells away, and a flat frame survives untouched
        assert_eq!(red(&f, &g, 0, 2), 0);
        let mut flat = alloc::vec![[100u8; 3]; 25];
        blur_frame_grid(&mut flat, &g, 128, 3);
        assert_eq!(flat, alloc::vec![[100u8; 3]; 25]);
    }

    #[test]
    fn grid_blur_has_no_row_end_seam() {
        // Progressive wiring: pixel 3 (end of row 0) and pixel 4 (start of
        // row 1) are index-adjacent but at opposite edges of the panel.
        let g = detect_grid(2, &grid_coords(4, 4, false)).unwrap();
        let mut f = alloc::vec![[0u8; 3]; 16];
        f[3] = [255; 3];
        blur_frame_grid(&mut f, &g, 64, 1);
        assert_eq!(red(&f, &g, 1, 0), 0, "light jumped the fold to the far edge");
        assert!(red(&f, &g, 1, 3) > 0, "light didn't spread down the column");
        // the index-space blur is exactly what does jump the fold
        let mut idx = alloc::vec![[0u8; 3]; 16];
        idx[3] = [255; 3];
        blur_frame(&mut idx, 64, 1);
        assert!(idx[4][0] > 0, "index-space blur is supposed to smear across");
    }

    #[test]
    fn grid_blur_is_symmetric_on_serpentine_wiring() {
        let g = detect_grid(2, &grid_coords(6, 6, true)).unwrap();
        let mut f = alloc::vec![[0u8; 3]; 36];
        f[g.index(3, 3)] = [255; 3];
        blur_frame_grid(&mut f, &g, 64, 2);
        // every mirror of the spike gets the same light regardless of which
        // direction its row happens to be wired
        for d in 1..=2usize {
            let up = red(&f, &g, 3 - d, 3);
            let down = red(&f, &g, 3 + d, 3);
            let left = red(&f, &g, 3, 3 - d);
            let right = red(&f, &g, 3, 3 + d);
            assert_eq!(up, down, "vertical asymmetry at distance {d}");
            assert_eq!(left, right, "horizontal asymmetry at distance {d}");
            assert_eq!(up, left, "axis asymmetry at distance {d}");
            assert!(up > 0, "no spread at distance {d}");
        }
        // and the same frame blurred in index space is nothing like it: the
        // wiring path spreads within a row and folds back at the row ends
        let mut idx = alloc::vec![[0u8; 3]; 36];
        idx[g.index(3, 3)] = [255; 3];
        blur_frame(&mut idx, 64, 2);
        assert_eq!(idx[g.index(1, 3)][0], 0, "index blur reached two rows up");
    }

    #[test]
    fn grid_glow_blooms_in_both_axes() {
        let g = detect_grid(2, &grid_coords(5, 5, false)).unwrap();
        let mut f = alloc::vec![[0u8; 3]; 25];
        f[g.index(2, 2)] = [255; 3];
        glow_frame_grid(&mut f, &g, 128);
        assert_eq!(red(&f, &g, 2, 2), 255, "glow must not dim its source");
        for (r, c) in [(1, 2), (3, 2), (2, 1), (2, 3)] {
            assert_eq!(red(&f, &g, r, c), 127, "orthogonal ({r},{c})");
        }
        for (r, c) in [(1, 1), (3, 3)] {
            assert_eq!(red(&f, &g, r, c), 63, "diagonal ({r},{c})");
        }
        // 0 is a no-op
        let before = f.clone();
        glow_frame_grid(&mut f, &g, 0);
        assert_eq!(f, before);
    }

    #[test]
    fn grid_stages_ignore_a_mismatched_frame() {
        // the grid describes a different pixel count than the frame has
        let g = GridMap { w: 8, h: 8, serpentine: false };
        let mut f = alloc::vec![[0u8; 3]; 25];
        f[12] = [255; 3];
        let before = f.clone();
        blur_frame_grid(&mut f, &g, 64, 1);
        glow_frame_grid(&mut f, &g, 128);
        assert_eq!(f, before, "kernels must bail rather than index out of range");
    }

    #[test]
    fn palette_remap_recolors_by_luma() {
        // luma of pure green = (255·183) >> 8 = 182
        assert_eq!(luma([0, 255, 0]), 182);
        assert_eq!(luma([255, 255, 255]), 255);
        assert_eq!(luma([0, 0, 0]), 0);
        // a table that turns every luma into "red at that level"
        let mut lut = [[0u8; 3]; 256];
        for (i, slot) in lut.iter_mut().enumerate() {
            *slot = [i as u8, 0, 0];
        }
        let mut f = [[0u8, 255, 0], [255, 255, 255]];
        palette_remap_frame(&mut f, &lut, 256);
        assert_eq!(f, [[182, 0, 0], [255, 0, 0]]);
        // half strength blends with the original
        let mut h = [[0u8, 255, 0]];
        palette_remap_frame(&mut h, &lut, 128);
        assert_eq!(h, [[91, 127, 0]]);
        // amount 0 is a no-op
        let mut z = [[0u8, 255, 0]];
        palette_remap_frame(&mut z, &lut, 0);
        assert_eq!(z, [[0, 255, 0]]);
    }

    #[test]
    fn palette_lut_matches_stop_sampling() {
        use crate::fixed::Fx;
        let b = |v: u8| Fx::from_raw(((v as i32) << 16) / 255);
        // black → red ramp
        let pal = [(b(0), [Fx::ZERO; 3]), (b(255), [Fx::ONE, Fx::ZERO, Fx::ZERO])];
        let mut lut = [[0u8; 3]; 256];
        fill_palette_lut(&pal, &mut lut);
        assert_eq!(lut[0], [0, 0, 0]);
        assert_eq!(lut[255], [255, 0, 0]);
        assert_eq!(lut[128][1..], [0, 0]);
        assert!(lut[128][0] > 120 && lut[128][0] < 136);
        // monotone in the ramp direction
        assert!(lut.windows(2).all(|w| w[0][0] <= w[1][0]));
    }

    #[test]
    fn device_palette_body_parses_and_validates() {
        let (amount, stops) = parse_palette_stops("50 0 255 0 0 128 0 0 255").unwrap();
        assert_eq!(amount, 50);
        assert_eq!(stops, alloc::vec![(0, [255, 0, 0]), (128, [0, 0, 255])]);
        // leading/inner whitespace is free-form
        assert!(parse_palette_stops("  100\n0 1 2 3\n").is_ok());
        // rejects: bad amount, ragged groups, empty list, unsorted, overflow,
        // and out-of-range components
        assert!(parse_palette_stops("101 0 1 2 3").is_err());
        assert!(parse_palette_stops("50 0 1 2").is_err());
        assert!(parse_palette_stops("50").is_err());
        assert!(parse_palette_stops("50 200 1 2 3 10 4 5 6").is_err());
        assert!(parse_palette_stops("50 0 1 2 300").is_err());
        let too_many = (0..=MAX_OUTPUT_PALETTE_STOPS)
            .map(|_| String::from(" 0 1 2 3"))
            .collect::<String>();
        assert!(parse_palette_stops(&alloc::format!("50{}", too_many)).is_err());
        // exactly at the cap is fine
        let at_cap = (0..MAX_OUTPUT_PALETTE_STOPS)
            .map(|_| String::from(" 0 1 2 3"))
            .collect::<String>();
        assert!(parse_palette_stops(&alloc::format!("50{}", at_cap)).is_ok());
    }

    #[test]
    fn brightness_curve_reshapes_the_dimmer() {
        // off (0 and 10) is identity, and so are the endpoints
        for b in 0..=31u8 {
            assert_eq!(curve_brightness(b, 0), b);
            assert_eq!(curve_brightness(b, 10), b);
        }
        assert_eq!(curve_brightness(0, 22), 0);
        assert_eq!(curve_brightness(31, 22), 31);
        // γ2.2 pushes the useful range up the slider: half-way is much dimmer
        let mid = curve_brightness(16, 22);
        assert!((6..=9).contains(&mid), "16/31 at γ2.2 = {mid}");
        // monotonic, and a lit strip never curves to off
        let mut prev = 0;
        for b in 1..=31u8 {
            let v = curve_brightness(b, 22);
            assert!(v >= 1, "brightness {b} curved to 0");
            assert!(v >= prev, "not monotonic at {b}: {prev} → {v}");
            prev = v;
        }
        // γ < 1 goes the other way
        assert!(curve_brightness(16, 5) > 16);
    }

    #[test]
    fn power_cap_scales_hot_frames() {
        // 100 full-white pixels at full brightness ≈ 100 × 60 mA = 6 A
        let mut f = [[255u8, 255, 255]; 100];
        assert!((estimate_ma(&f, 31, PowerModel::Strip) as i64 - 6000).abs() < 100);
        let scale = apply(&mut f, ColorOrder::RGB, None, 3000, 31, PowerModel::Strip);
        assert!(scale < 256);
        let after = estimate_ma(&f, 31, PowerModel::Strip);
        assert!(after <= 3050, "capped estimate = {after}");
        // dim frames pass untouched
        let mut dim = [[10u8, 0, 0]; 100];
        let s2 = apply(&mut dim, ColorOrder::RGB, None, 3000, 31, PowerModel::Strip);
        assert_eq!(s2, 256);
        assert_eq!(dim[0], [10, 0, 0]);
    }

    #[test]
    fn hub75_model_divides_by_scan_ratio() {
        // full-white 64x64 panel: strip math says 4096 × 60 mA ≈ 246 A;
        // 1/32-scan time multiplexing lands it near 7.7 A (≈2x a typical
        // panel's rated draw — conservative by design)
        let f = alloc::vec![[255u8, 255, 255]; 4096];
        let strip = estimate_ma(&f, 31, PowerModel::Strip);
        let panel = estimate_ma(&f, 31, PowerModel::Hub75 { scan: 32 });
        assert_eq!(panel, strip / 32);
        assert!((7_000..9_000).contains(&panel), "panel estimate = {panel}");
        // the cap engages against the panel model, not the strip one
        let mut hot = alloc::vec![[255u8, 255, 255]; 4096];
        let scale = apply(
            &mut hot,
            ColorOrder::RGB,
            None,
            4_000,
            31,
            PowerModel::Hub75 { scan: 32 },
        );
        assert!(scale < 256);
        let after = estimate_ma(&hot, 31, PowerModel::Hub75 { scan: 32 });
        assert!(after <= 4_050, "capped panel estimate = {after}");
    }
}
