//! Test cards for transcribing a wall (Gitea #920).
//!
//! Describing a HUB75 wall as a rule (start corner, run direction, snake, a
//! turn per line parity) means working out which rule reproduces what is
//! hanging there. What the installer actually knows is per panel: where it
//! sits, which panel it is along the ribbon, which way it is turned. So the
//! device can draw two cards instead of the pattern:
//!
//! - [`panels`] — straight into the DRIVER's blocks, remap bypassed: every
//!   physical panel shows its ribbon number (1 = the panel the ribbon
//!   enters) and an arrow pointing to its own native top. The user reads
//!   that off the wall into the arrangement editor, cell by cell.
//! - [`cells`] — in ENGINE space, through the live remap: every grid cell
//!   shows its ribbon number and an up arrow. When the transcription is
//!   right, every panel shows its number upright; a wrong cell or turn is
//!   visible as which panel shows the wrong number, or a sideways one.
//!
//! The glyphs are a 3x5 digit font and a 5-wide chevron, scaled by an
//! integer so a 64-pixel tile reads from across the room and a 16-pixel
//! tile still fits. No `alloc`; the caller owns the frame.

use luxel_core::layout::{Matrix, Tile};

/// 3x5 digits, one byte per row, bit 2 = left column.
const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111], // 0
    [0b010, 0b110, 0b010, 0b010, 0b111], // 1
    [0b111, 0b001, 0b111, 0b100, 0b111], // 2
    [0b111, 0b001, 0b111, 0b001, 0b111], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b111, 0b001, 0b111], // 5
    [0b111, 0b100, 0b111, 0b101, 0b111], // 6
    [0b111, 0b001, 0b001, 0b010, 0b010], // 7
    [0b111, 0b101, 0b111, 0b101, 0b111], // 8
    [0b111, 0b101, 0b111, 0b001, 0b111], // 9
];

/// The number's colour, the arrow's, and the tile's frame — chosen to be
/// unmistakable at any brightness and to survive a wrong colour order.
pub const NUMBER: [u8; 3] = [255, 255, 255];
pub const ARROW: [u8; 3] = [0, 200, 255];
pub const FRAME: [u8; 3] = [40, 40, 40];

/// A frame of `w × h` pixels the caller owns, addressed by (x, y).
struct Canvas<'a> {
    px: &'a mut [[u8; 3]],
    w: usize,
    h: usize,
}

impl Canvas<'_> {
    fn plot(&mut self, x: isize, y: isize, c: [u8; 3]) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
            return;
        }
        if let Some(p) = self.px.get_mut(y as usize * self.w + x as usize) {
            *p = c;
        }
    }

    /// A filled `s × s` block at `(x, y)` — one scaled font pixel.
    fn block(&mut self, x: isize, y: isize, s: isize, c: [u8; 3]) {
        for dy in 0..s {
            for dx in 0..s {
                self.plot(x + dx, y + dy, c);
            }
        }
    }
}

/// The integer scale that fits a number of `digits` digits and the arrow
/// above it into a `tw × th` tile, at least 1.
fn scale_for(tw: usize, th: usize, digits: usize) -> isize {
    // width: digits × 3 + (digits − 1) gaps, plus a 1-cell margin each side
    let need_w = digits * 4 + 1;
    // height: arrow 3 + gap 1 + digits 5, plus a 1-cell margin each side
    let need_h = 3 + 1 + 5 + 2;
    let s = (tw / need_w).min(th / need_h);
    s.max(1) as isize
}

/// Draw one tile's label — its 1-based number and a chevron pointing
/// `turns` quarter turns clockwise from up — into the `tw × th` region at
/// `(ox, oy)` of the canvas, with a 1-pixel frame around the region.
fn label(c: &mut Canvas, ox: usize, oy: usize, tw: usize, th: usize, number: usize, turns: u8) {
    let (ox, oy, tw_i, th_i) = (ox as isize, oy as isize, tw as isize, th as isize);
    // the frame: the tile's edge, so a dark panel still shows where it is
    for x in 0..tw_i {
        c.plot(ox + x, oy, FRAME);
        c.plot(ox + x, oy + th_i - 1, FRAME);
    }
    for y in 0..th_i {
        c.plot(ox, oy + y, FRAME);
        c.plot(ox + tw_i - 1, oy + y, FRAME);
    }
    let mut digits = [0u8; 3];
    let mut n = number.min(999);
    let mut count = 0;
    loop {
        digits[count] = (n % 10) as u8;
        count += 1;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    let s = scale_for(tw, th, count);
    // the glyph block: `count` digits, centred; the arrow centred above
    let text_w = (count as isize * 4 - 1) * s;
    let total_h = (3 + 1 + 5) * s;
    let x0 = ox + (tw_i - text_w) / 2;
    let y0 = oy + (th_i - total_h) / 2;
    for (i, d) in digits[..count].iter().rev().enumerate() {
        let gx = x0 + i as isize * 4 * s;
        let gy = y0 + 4 * s;
        for (row, bits) in DIGITS[usize::from(*d)].iter().enumerate() {
            for col in 0..3 {
                if bits & (0b100 >> col) != 0 {
                    c.block(gx + col * s, gy + row as isize * s, s, NUMBER);
                }
            }
        }
    }
    // the chevron: 5 wide, 3 tall, pointing up, then turned about the
    // tile's centre with the digits — the ARROW is what the user matches
    // against the panel's real top, so it must turn with the mount
    let ax = ox + tw_i / 2;
    let ay = y0 + s + s / 2;
    let up: [(isize, isize); 9] = [(0, -1), (-1, 0), (1, 0), (-2, 1), (2, 1), (0, 0), (0, 1), (-1, 1), (1, 1)];
    let (cx, cy) = (ox + tw_i / 2, oy + th_i / 2);
    for (dx, dy) in up {
        // the arrow's own pixel, then the whole glyph rotated about the
        // tile centre so it points to the panel's native top
        let (px, py) = (ax + dx * s, ay + dy * s);
        let (rx, ry) = rotate(px - cx, py - cy, turns);
        c.block(cx + rx, cy + ry, s, ARROW);
    }
}

/// `(x, y)` about the origin, `turns` quarter turns clockwise (y down).
fn rotate(x: isize, y: isize, turns: u8) -> (isize, isize) {
    match turns & 3 {
        1 => (-y, x),
        2 => (-x, -y),
        3 => (y, -x),
        _ => (x, y),
    }
}

/// The PANELS card, in driver space: `frame` is `pw · drive` wide and `ph`
/// tall (the driver's row of `drive` blocks), and block `b` is the panel at
/// chain position `drive − 1 − b` — the first block clocked out lands on
/// the far end of the chain ([`crate::arrange`]). Each block gets its
/// 1-based ribbon number and an arrow to its native top (upright, since the
/// block IS the panel's own pixel order). Zero pixels first.
pub fn panels(frame: &mut [[u8; 3]], pw: usize, ph: usize, drive: usize) {
    let w = pw * drive;
    if frame.len() < w * ph || pw == 0 || ph == 0 || drive == 0 {
        return;
    }
    frame[..w * ph].fill([0; 3]);
    let mut c = Canvas { px: &mut frame[..w * ph], w, h: ph };
    for b in 0..drive {
        let position = drive - 1 - b;
        label(&mut c, b * pw, 0, pw, ph, position + 1, 0);
    }
}

/// The CELLS card, in engine space: `frame` is the `m.width() × m.height()`
/// grid, and every cell gets the 1-based ribbon number of the tile that
/// fills it (per `tiles`, in ribbon order) with an UP arrow — so through a
/// correct remap each panel shows its own number upright. Cells no tile
/// names stay dark. Zero pixels first.
pub fn cells(frame: &mut [[u8; 3]], m: &Matrix, tiles: &[Tile]) {
    let (w, h) = (m.width() as usize, m.height() as usize);
    let (pw, ph) = (m.pw as usize, m.ph as usize);
    if frame.len() < w * h || pw == 0 || ph == 0 {
        return;
    }
    frame[..w * h].fill([0; 3]);
    let mut c = Canvas { px: &mut frame[..w * h], w, h };
    for (p, t) in tiles.iter().enumerate() {
        let (cx, cy) = (usize::from(t.cx), usize::from(t.cy));
        if cx >= m.cols as usize || cy >= m.rows as usize {
            continue;
        }
        label(&mut c, cx * pw, cy * ph, pw, ph, p + 1, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;
    use std::vec::Vec;

    fn lit(frame: &[[u8; 3]], c: [u8; 3]) -> usize {
        frame.iter().filter(|p| **p == c).count()
    }

    /// The panels card labels block b with position drive-1-b: on a 4x1 the
    /// leftmost block (the far end of the chain) reads "4", the rightmost
    /// (the IN panel) "1".
    #[test]
    fn the_panels_card_numbers_blocks_from_the_in_panel() {
        let (pw, ph, drive) = (16usize, 16usize, 4usize);
        let mut frame = vec![[0u8; 3]; pw * drive * ph];
        panels(&mut frame, pw, ph, drive);
        // every block has a frame, a number and an arrow
        for b in 0..drive {
            let block: Vec<[u8; 3]> =
                (0..ph).flat_map(|y| (0..pw).map(move |x| (x, y))).map(|(x, y)| frame[y * pw * drive + b * pw + x]).collect();
            assert!(lit(&block, NUMBER) > 0, "block {b} has a number");
            assert!(lit(&block, ARROW) == 9, "block {b} has the 9-pixel chevron at scale 1");
        }
        // block 0 ("4") and block 3 ("1") differ; "1" has fewer lit pixels than "4"
        let count = |b: usize| -> usize {
            (0..ph).map(|y| (0..pw).filter(|x| frame[y * pw * drive + b * pw + x] == NUMBER).count()).sum()
        };
        assert!(count(3) < count(0), "'1' is thinner than '4': {} vs {}", count(3), count(0));
        // the arrow points up in every block: its tip is above its base
        let tip_y = (0..ph).find(|y| (0..pw).any(|x| frame[y * pw * drive + x] == ARROW)).unwrap();
        let base_y = (0..ph).rev().find(|y| (0..pw).any(|x| frame[y * pw * drive + x] == ARROW)).unwrap();
        assert!(tip_y < base_y);
    }

    /// The cells card puts tile p's number in tile p's CELL, whatever order
    /// the chain visits them, with an up arrow; unnamed cells stay dark.
    #[test]
    fn the_cells_card_labels_cells_by_ribbon_position() {
        let mut m = Matrix::single(16, 16);
        m.cols = 2;
        m.rows = 2;
        // Jeremy's wall: IN top-right, down, back up the left
        let tiles = [
            Tile { cx: 1, cy: 0, turns: 1 },
            Tile { cx: 1, cy: 1, turns: 1 },
            Tile { cx: 0, cy: 1, turns: 3 },
            Tile { cx: 0, cy: 0, turns: 3 },
        ];
        let mut frame = vec![[0u8; 3]; 32 * 32];
        cells(&mut frame, &m, &tiles);
        fn cell(frame: &[[u8; 3]], cx: usize, cy: usize) -> Vec<[u8; 3]> {
            (0..16).flat_map(|y| (0..16).map(move |x| (x, y))).map(|(x, y)| frame[(cy * 16 + y) * 32 + cx * 16 + x]).collect()
        }
        // every cell labelled, and "1" (top-right) thinner than "4" (top-left)
        assert!(lit(&cell(&frame, 1, 0), NUMBER) < lit(&cell(&frame, 0, 0), NUMBER));
        assert_eq!(lit(&cell(&frame, 1, 0), ARROW), 9);
        // three tiles only: the fourth cell is dark but for its frame
        let mut frame = vec![[0u8; 3]; 32 * 32];
        cells(&mut frame, &m, &tiles[..3]);
        assert_eq!(lit(&cell(&frame, 0, 0), NUMBER), 0);
        assert_eq!(lit(&cell(&frame, 0, 0), ARROW), 0);
    }

    /// A turned label's arrow points where the panel's top went: 90° puts
    /// the chevron's tip on the RIGHT edge of the tile.
    #[test]
    fn the_arrow_turns_with_the_tile() {
        let mut frame = vec![[0u8; 3]; 16 * 16];
        let mut c = Canvas { px: &mut frame, w: 16, h: 16 };
        label(&mut c, 0, 0, 16, 16, 1, 1);
        let arrow: Vec<(usize, usize)> =
            (0..16).flat_map(|y| (0..16).map(move |x| (x, y))).filter(|&(x, y)| frame[y * 16 + x] == ARROW).collect();
        assert_eq!(arrow.len(), 9);
        let max_x = arrow.iter().map(|p| p.0).max().unwrap();
        let min_x = arrow.iter().map(|p| p.0).min().unwrap();
        // the tip is the single pixel furthest right
        assert_eq!(arrow.iter().filter(|p| p.0 == max_x).count(), 1);
        assert!(max_x > 8 && min_x >= 8, "the whole chevron sits in the right half: {min_x}..{max_x}");
    }

    #[test]
    fn a_tiny_tile_still_fits_its_label() {
        let mut frame = vec![[0u8; 3]; 8 * 8 * 2];
        panels(&mut frame, 8, 8, 2);
        assert!(lit(&frame, NUMBER) > 0);
        assert_eq!(lit(&frame, ARROW), 18);
        // and a huge one scales up rather than drawing a 3x5 speck
        let mut frame = vec![[0u8; 3]; 64 * 64];
        panels(&mut frame, 64, 64, 1);
        assert!(lit(&frame, ARROW) > 9 * 16, "scaled chevron: {}", lit(&frame, ARROW));
    }
}
