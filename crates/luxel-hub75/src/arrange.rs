//! Panel arrangement → the boot-time panel→pixel remap (Gitea #475).
//!
//! A HUB75 chain is one ribbon: the driver shifts a single row of
//! `pw * panels` pixels, `ph` tall, and the panels sit wherever the
//! installer hung them. [`luxel_core::layout::Matrix`] describes that
//! physical arrangement — `cols`×`rows` tiles, which corner the chain
//! starts from, whether it runs along rows or columns, whether it snakes,
//! and whether alternate lines are mounted upside-down — and this module
//! turns it into ONE lookup table:
//!
//! ```text
//! lut[driver pixel index] = engine pixel index      (or UNMAPPED)
//! ```
//!
//! so the engine keeps rendering one `cols*pw` × `rows*ph` row-major grid
//! and the compose path gathers through the table ([`crate::pack_remap`]).
//! Built once at boot — the arrangement is a `reboot_required` field of
//! `/api/layout` precisely so nothing has to re-derive it per frame.
//!
//! **The identity case costs nothing.** A single upright tile (and any
//! arrangement that happens to come out row-major, e.g. two 32-wide tiles
//! side by side wired `tl row`) produces `lut[i] == i`; the firmware checks
//! that with [`is_identity`], drops the table, and composes exactly the code
//! it composed before this module existed.
//!
//! **Chain order.** Panels are visited line by line. A *line* is a row of
//! tiles when `dir` is [`RunDir::Row`] and a column when it is
//! [`RunDir::Col`]; `start` says which corner line 0 begins at (and hence
//! which way the first line travels); `snake` reverses every odd line; and
//! `rot180` marks the tiles on odd lines as mounted rotated 180°, which is
//! how a serpentine wall is physically built (proposal §5.3, mockup S3c).

use luxel_core::layout::{Corner, Matrix, RunDir};

/// A driver pixel with no engine pixel behind it: outside the arrangement,
/// or past the end of the chain the framebuffer covers. Composed black.
pub const UNMAPPED: u16 = u16::MAX;

/// Largest engine pixel index a remap can name (`UNMAPPED` is the sentinel).
pub const MAX_INDEX: u32 = UNMAPPED as u32 - 1;

/// How many tiles the chain threads.
#[must_use]
pub fn chain_len(m: &Matrix) -> usize {
    m.cols as usize * m.rows as usize
}

/// How many LEADING tiles of the chain a `fb_w` × `fb_h` framebuffer can
/// actually shift out: the chain is one ribbon `pw` wide per tile, so the
/// framebuffer covers `fb_w / pw` of them, and none at all if the tiles are
/// taller than it.
///
/// A board whose framebuffer is smaller than the configured chain drives
/// this prefix and leaves the rest of the arrangement dark — reported as
/// `drive` on `GET /api/layout` rather than silently pretended away. Raising
/// the framebuffer to fit a real chain is #401.
#[must_use]
pub fn driven_panels(m: &Matrix, fb_w: usize, fb_h: usize) -> usize {
    if m.pw == 0 || m.ph == 0 || m.ph as usize > fb_h {
        return 0;
    }
    chain_len(m).min(fb_w / m.pw as usize)
}

/// Which tile of the grid chain position `p` is, and whether it is mounted
/// rotated 180°. `None` past the end of the chain.
///
/// Returns `(cx, cy, rot)` in tile coordinates, `cx` increasing right and
/// `cy` increasing down — the same axes the engine grid uses.
#[must_use]
pub fn panel_cell(m: &Matrix, p: usize) -> Option<(u32, u32, bool)> {
    let (cols, rows) = (m.cols as usize, m.rows as usize);
    if p >= cols * rows || cols == 0 || rows == 0 {
        return None;
    }
    // Which corner line 0 starts at, as a pair of axis flips.
    let flip_x = matches!(m.start, Corner::Tr | Corner::Br);
    let flip_y = matches!(m.start, Corner::Bl | Corner::Br);
    // `line` walks across the lines, `k` walks along one.
    let run = match m.dir {
        RunDir::Row => cols,
        RunDir::Col => rows,
    };
    let (line, mut k) = (p / run, p % run);
    // A snaked chain comes back the other way along every odd line.
    if m.snake && line % 2 == 1 {
        k = run - 1 - k;
    }
    let (cx, cy) = match m.dir {
        RunDir::Row => (k, line),
        RunDir::Col => (line, k),
    };
    let cx = if flip_x { cols - 1 - cx } else { cx };
    let cy = if flip_y { rows - 1 - cy } else { cy };
    Some((cx as u32, cy as u32, m.rot180 && line % 2 == 1))
}

/// Fill `lut` with the driver→engine remap for a `fb_w` × `fb_h`
/// framebuffer, row-major over driver pixels. Returns the number of leading
/// chain tiles the framebuffer drives (see [`driven_panels`]).
///
/// Every entry is written: driver pixels past the driven chain, past the
/// tile height, or belonging to a tile the arrangement does not have become
/// [`UNMAPPED`].
///
/// # Panics
/// If `lut` is not exactly `fb_w * fb_h` entries.
pub fn build_lut(lut: &mut [u16], m: &Matrix, fb_w: usize, fb_h: usize) -> usize {
    assert_eq!(lut.len(), fb_w * fb_h, "remap table length");
    lut.fill(UNMAPPED);
    let drive = driven_panels(m, fb_w, fb_h);
    if drive == 0 || m.pixels() > MAX_INDEX {
        return drive;
    }
    let (pw, ph, w) = (m.pw as usize, m.ph as usize, m.width() as usize);
    for p in 0..drive {
        let Some((cx, cy, rot)) = panel_cell(m, p) else { continue };
        let (ox, oy) = (cx as usize * pw, cy as usize * ph);
        for ly in 0..ph {
            for lx in 0..pw {
                // A tile mounted upside-down shows its far corner first.
                let (sx, sy) = if rot { (pw - 1 - lx, ph - 1 - ly) } else { (lx, ly) };
                let engine = (oy + sy) * w + ox + sx;
                lut[ly * fb_w + p * pw + lx] = engine as u16;
            }
        }
    }
    drive
}

/// Does this table leave every pixel where it was? Then the arrangement is
/// already what the compose path does natively and the table can be thrown
/// away — the whole point of building it eagerly rather than branching on
/// `cols == 1 && rows == 1`.
#[must_use]
pub fn is_identity(lut: &[u16]) -> bool {
    lut.iter().enumerate().all(|(i, &e)| e as usize == i)
}

/// Estimated panel rescan rate in Hz — what the UI shows as "estimated
/// refresh" and turns amber below 100 Hz.
///
/// Plain BCM: the driver shifts the whole chain once per bitplane per
/// address row, and plane `k` is repeated `2^k` times, so one full rescan is
///
/// ```text
///   rows_addressed * (2^planes - 1) * chain_width   pixel clocks
/// ```
///
/// with `chain_width = pw * panels` and `rows_addressed` the panel's scan
/// depth (`scan` when the Layout states one, else `ph / 2` — a HUB75 panel
/// drives two half-height rows at once through R1G1B1/R2G2B2).
///
/// Measured against the bench panel: 64×64 at 7 planes and 30 MHz gives
/// 32 · 127 · 64 = 260,096 clocks = **115.3 Hz**, and the metal reads
/// 115.3–115.5 (Gitea #255). The same formula at 20/40 MHz predicts
/// 76.9/153.8 against measured 76.9–77.0/153.5–154.0.
#[must_use]
pub fn est_hz(m: &Matrix, planes: u32, clock_hz: u32) -> u32 {
    let scan = if m.scan != 0 { m.scan as u32 } else { m.ph as u32 / 2 };
    let chain_w = m.pw as u32 * chain_len(m) as u32;
    let clocks = scan as u64 * ((1u64 << planes) - 1) * chain_w as u64;
    if clocks == 0 {
        return 0;
    }
    (clock_hz as u64 / clocks) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use luxel_core::layout::Matrix;
    use std::vec;
    use std::vec::Vec;

    /// Every corner × run direction × snake × rot180 the grammar allows.
    fn all_wirings() -> Vec<(Corner, RunDir, bool, bool)> {
        let mut v = Vec::new();
        for start in [Corner::Tl, Corner::Tr, Corner::Bl, Corner::Br] {
            for dir in [RunDir::Row, RunDir::Col] {
                for snake in [false, true] {
                    for rot180 in [false, true] {
                        v.push((start, dir, snake, rot180));
                    }
                }
            }
        }
        v
    }

    fn tiles(pw: u16, ph: u16, cols: u8, rows: u8, w: (Corner, RunDir, bool, bool)) -> Matrix {
        let mut m = Matrix::single(pw, ph);
        m.cols = cols;
        m.rows = rows;
        m.start = w.0;
        m.dir = w.1;
        m.snake = w.2;
        m.rot180 = w.3;
        m
    }

    /// The framebuffer a chain of `m` needs: one ribbon, all tiles wide.
    fn chain_fb(m: &Matrix) -> (usize, usize) {
        (m.pw as usize * chain_len(m), m.ph as usize)
    }

    #[test]
    fn a_single_upright_tile_is_the_identity() {
        let m = Matrix::single(64, 64);
        let mut lut = vec![0u16; 64 * 64];
        assert_eq!(build_lut(&mut lut, &m, 64, 64), 1);
        assert!(is_identity(&lut));
    }

    /// The case that makes "identity" worth checking rather than assuming:
    /// two 32-wide tiles side by side, wired plainly, ARE a 64-wide grid.
    #[test]
    fn two_tiles_wired_plainly_side_by_side_are_also_the_identity() {
        let m = tiles(32, 64, 2, 1, (Corner::Tl, RunDir::Row, false, false));
        let mut lut = vec![0u16; 64 * 64];
        assert_eq!(build_lut(&mut lut, &m, 64, 64), 2);
        assert!(is_identity(&lut));
    }

    /// …and starting the same chain from the other corner swaps the halves.
    #[test]
    fn starting_at_the_far_corner_swaps_the_halves() {
        let m = tiles(32, 64, 2, 1, (Corner::Tr, RunDir::Row, false, false));
        let mut lut = vec![0u16; 64 * 64];
        assert_eq!(build_lut(&mut lut, &m, 64, 64), 2);
        assert!(!is_identity(&lut));
        // driver column 0 shows engine column 32, and vice versa
        assert_eq!(lut[0], 32);
        assert_eq!(lut[32], 0);
        assert_eq!(lut[63], 31);
        assert_eq!(lut[63 * 64 + 0], 63 * 64 + 32);
    }

    /// Whatever the wiring, a chain the framebuffer covers must show every
    /// engine pixel exactly once — the property that makes the remap a
    /// rearrangement rather than a filter.
    #[test]
    fn every_wiring_is_a_permutation_of_the_grid() {
        for (cols, rows) in [(2u8, 2u8), (3, 2), (1, 1), (4, 1), (1, 3)] {
            for w in all_wirings() {
                let m = tiles(4, 4, cols, rows, w);
                let (fw, fh) = chain_fb(&m);
                let mut lut = vec![0u16; fw * fh];
                let drive = build_lut(&mut lut, &m, fw, fh);
                assert_eq!(drive, chain_len(&m), "{cols}x{rows} {w:?}");
                let mut seen = vec![false; m.pixels() as usize];
                for &e in &lut {
                    assert_ne!(e, UNMAPPED, "{cols}x{rows} {w:?}: hole in a covered chain");
                    assert!(!seen[e as usize], "{cols}x{rows} {w:?}: engine pixel {e} twice");
                    seen[e as usize] = true;
                }
                assert!(seen.iter().all(|s| *s), "{cols}x{rows} {w:?}: pixel never shown");
            }
        }
    }

    /// The chain must visit every tile exactly once, for every wiring.
    #[test]
    fn every_wiring_visits_every_tile_once() {
        for (cols, rows) in [(2u8, 2u8), (3, 2), (4, 3)] {
            for w in all_wirings() {
                let m = tiles(4, 4, cols, rows, w);
                let mut seen = vec![false; chain_len(&m)];
                for p in 0..chain_len(&m) {
                    let (cx, cy, _) = panel_cell(&m, p).expect("in chain");
                    let i = cy as usize * cols as usize + cx as usize;
                    assert!(!seen[i], "{cols}x{rows} {w:?}: tile ({cx},{cy}) twice");
                    seen[i] = true;
                }
                assert!(seen.iter().all(|s| *s), "{cols}x{rows} {w:?}");
                assert_eq!(panel_cell(&m, chain_len(&m)), None);
            }
        }
    }

    #[test]
    fn corners_place_the_first_tile() {
        for (start, want) in [
            (Corner::Tl, (0, 0)),
            (Corner::Tr, (2, 0)),
            (Corner::Bl, (0, 1)),
            (Corner::Br, (2, 1)),
        ] {
            let m = tiles(4, 4, 3, 2, (start, RunDir::Row, false, false));
            assert_eq!(panel_cell(&m, 0).unwrap(), (want.0, want.1, false), "{start:?}");
        }
    }

    #[test]
    fn a_snaked_row_chain_comes_back_the_other_way() {
        let m = tiles(4, 4, 3, 2, (Corner::Tl, RunDir::Row, true, true));
        let got: Vec<_> = (0..6).map(|p| panel_cell(&m, p).unwrap()).collect();
        assert_eq!(
            got,
            vec![
                (0, 0, false),
                (1, 0, false),
                (2, 0, false),
                // line 1 runs right-to-left, and its tiles hang upside-down
                (2, 1, true),
                (1, 1, true),
                (0, 1, true),
            ]
        );
    }

    #[test]
    fn a_column_chain_runs_down_before_across() {
        let m = tiles(4, 4, 3, 2, (Corner::Tl, RunDir::Col, false, false));
        // (cx,cy) as cx*10+cy: down column 0, then down column 1, then 2
        let got: Vec<_> = (0..6)
            .map(|p| {
                let (cx, cy, _) = panel_cell(&m, p).unwrap();
                cx * 10 + cy
            })
            .collect();
        assert_eq!(got, vec![0, 1, 10, 11, 20, 21]);
    }

    /// `rot180` is "alternate LINES are mounted upside-down", so a column
    /// chain flips columns, not rows.
    #[test]
    fn rot180_follows_the_lines_the_run_direction_defines() {
        let m = tiles(4, 4, 2, 2, (Corner::Tl, RunDir::Col, false, true));
        let rots: Vec<_> = (0..4).map(|p| panel_cell(&m, p).unwrap().2).collect();
        assert_eq!(rots, vec![false, false, true, true]);
    }

    #[test]
    fn a_rotated_tile_shows_its_far_corner_first() {
        let m = tiles(4, 4, 1, 2, (Corner::Tl, RunDir::Row, false, true));
        let (fw, fh) = chain_fb(&m);
        let mut lut = vec![0u16; fw * fh];
        build_lut(&mut lut, &m, fw, fh);
        // tile 1 is the bottom row (engine rows 4..8), mounted upside-down:
        // its first driver pixel is the grid's bottom-right corner.
        assert_eq!(lut[4], 7 * 4 + 3);
        assert_eq!(lut[3 * fw + 7], 4 * 4);
    }

    /// A framebuffer smaller than the chain drives the prefix that fits and
    /// leaves the rest unmapped — never a wrong pixel.
    #[test]
    fn a_chain_wider_than_the_framebuffer_drives_its_prefix() {
        let m = tiles(64, 64, 2, 1, (Corner::Tl, RunDir::Row, false, false));
        let mut lut = vec![0u16; 64 * 64];
        assert_eq!(build_lut(&mut lut, &m, 64, 64), 1);
        assert_eq!(driven_panels(&m, 64, 64), 1);
        // tile 0 is the left half of a 128-wide grid
        assert_eq!(lut[0], 0);
        assert_eq!(lut[63], 63);
        assert_eq!(lut[64], 128); // driver row 1 is engine row 1 of a 128-wide grid
        assert!(!is_identity(&lut));
    }

    #[test]
    fn tiles_taller_than_the_framebuffer_drive_nothing() {
        let m = tiles(32, 128, 2, 1, (Corner::Tl, RunDir::Row, false, false));
        let mut lut = vec![0u16; 64 * 64];
        assert_eq!(build_lut(&mut lut, &m, 64, 64), 0);
        assert!(lut.iter().all(|e| *e == UNMAPPED));
    }

    /// The bench panel, against metal: 115 Hz at 7 planes / 30 MHz, and the
    /// whole measured clock ladder (Gitea #255).
    #[test]
    fn the_refresh_estimate_matches_the_bench_panel() {
        let m = Matrix::single(64, 64);
        assert_eq!(est_hz(&m, 7, 30_000_000), 115); // measured 115.3-115.5
        assert_eq!(est_hz(&m, 7, 20_000_000), 76); // measured 76.9-77.0
        assert_eq!(est_hz(&m, 7, 40_000_000), 153); // measured 153.5-154.0
        // an eighth bitplane halves it, a sixth doubles it
        assert_eq!(est_hz(&m, 8, 30_000_000), 57);
        assert_eq!(est_hz(&m, 6, 30_000_000), 232);
    }

    /// The 128x128 target: four chained 64x64 tiles at 7 planes / 30 MHz is
    /// the ~29 Hz the #255 research predicted.
    #[test]
    fn a_four_tile_chain_is_a_quarter_of_the_refresh() {
        let m = tiles(64, 64, 2, 2, (Corner::Tl, RunDir::Row, true, true));
        assert_eq!(est_hz(&m, 7, 30_000_000), 28);
        // the arrangement does not change the shift length, only the count
        let line = tiles(64, 64, 4, 1, (Corner::Tl, RunDir::Row, false, false));
        assert_eq!(est_hz(&line, 7, 30_000_000), 28);
    }

    /// `scan` states the address depth when it is not simply `ph / 2`.
    #[test]
    fn the_scan_field_overrides_half_the_panel_height() {
        let mut m = Matrix::single(64, 32);
        assert_eq!(m.scan, 0);
        assert_eq!(est_hz(&m, 7, 30_000_000), 230); // 16 address rows
        m.scan = 8; // a 1/8-scan 64x32 panel: half the rows, twice the rate
        assert_eq!(est_hz(&m, 7, 30_000_000), 461);
        m.scan = 32;
        assert_eq!(est_hz(&m, 7, 30_000_000), 115);
    }
}
