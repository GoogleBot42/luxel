//! One test per cell of the §5.4d projection table (Gitea #473).
//!
//! Each pattern writes the argument under test straight into the pixel, so
//! the assertion reads the coordinate the engine actually handed it:
//!   rgb(x, y, z) → [floor(x·255), floor(y·255), floor(z·255)]
//! `MID` is the mid-space fill (0.5 → 127) an unprojected axis gets.

use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::projection::{Projection, ProjectionMode};
use luxel_core::vm::Value;

/// quantize(0.5) — the mid-space fill a coordinate the projection does not
/// feed comes out as.
const MID: u8 = 127;

/// rgb(x, y, z) per pixel: the three coordinate arguments, verbatim.
const COORDS_2D: &str = "export function render2D(index, x, y) { rgb(x, y, 0) }";
const COORDS_3D: &str = "export function render3D(index, x, y, z) { rgb(x, y, z) }";
/// index and pixelCount, each scaled so they survive quantization exactly.
const INDEX_1D: &str =
    "export function render(index) { rgb(index / 8, pixelCount / 8, 0) }";

fn q(v: f64) -> u8 {
    (Fx::from_f64(v).raw().clamp(0, 65_536) * 255 >> 16) as u8
}

/// The pixel byte for cell `v` of `n` cells along a Layout axis — the
/// engine normalizes a map's axis to 0..65535/65536 (`MapData::coord`,
/// `Engine::set_map_vec`), so the last cell is 254, not 255.
fn qcell(v: usize, n: usize) -> u8 {
    if n <= 1 {
        return 0;
    }
    let span = n as i64 - 1;
    let raw = (v as i64 * 65_535 + span / 2) / span;
    ((raw * 255) >> 16) as u8
}

/// A W×H×D lattice as an explicit 3D coordinate map (x fastest, then y, then z).
fn lattice(w: u32, h: u32, d: u32) -> Vec<[Fx; 3]> {
    let mut out = Vec::new();
    for z in 0..d {
        for y in 0..h {
            for x in 0..w {
                out.push([
                    Fx::from_int(x as i32),
                    Fx::from_int(y as i32),
                    Fx::from_int(z as i32),
                ]);
            }
        }
    }
    out
}

fn with_proj(src: &str, n: u32, pattern_dims: u8, mode: ProjectionMode) -> Engine {
    let mut e = Engine::new(src, n, 1).unwrap();
    let mut p = Projection::DEFAULT;
    p.set(pattern_dims, mode);
    e.set_projection(p);
    e
}

// ---- 1D patterns on a 2D Layout: by index · along x · along y ----

#[test]
fn one_d_on_grid_by_index_is_unchanged() {
    let mut e = with_proj(INDEX_1D, 8, 1, ProjectionMode::Index);
    e.set_grid_map(4, 2);
    let mut p = Projection::DEFAULT;
    p.set(1, ProjectionMode::Index);
    e.set_projection(p);
    let g = e.effective_geometry();
    assert_eq!(g.pixel_count, 8, "by index renders every Layout pixel");
    assert_eq!(g.mode, Some(ProjectionMode::Index));
    let px = e.frame(Fx::ZERO).to_vec();
    for (i, p) in px.iter().enumerate() {
        assert_eq!(p[0], q(i as f64 / 8.0), "pixel {i} index");
        assert_eq!(p[1], q(1.0), "pixelCount stays 8");
    }
}

#[test]
fn one_d_on_grid_along_x_renders_one_row_and_replicates() {
    // `calls` counts render invocations: the engine win, measured.
    let src = "export var calls = 0\n\
               export function render(index) { calls = calls + 1\n rgb(index / 4, pixelCount / 8, 0) }";
    let mut e = with_proj(src, 8, 1, ProjectionMode::X);
    e.set_grid_map(4, 2);
    let g = e.effective_geometry();
    assert_eq!(g.pixel_count, 4, "pixelCount reads as the strip length");
    assert_eq!(g.pattern_dims, 1);
    assert_eq!(g.layout_dims, 2);
    assert_eq!(g.mode, Some(ProjectionMode::X));
    let px = e.frame(Fx::ZERO).to_vec();
    assert_eq!(e.var("calls"), Some(Value::Num(Fx::from_int(4))));
    for row in 0..2usize {
        for col in 0..4usize {
            let p = px[row * 4 + col];
            assert_eq!(p[0], q(col as f64 / 4.0), "row {row} col {col}");
            assert_eq!(p[1], q(4.0 / 8.0), "pixelCount reads 4");
        }
    }
}

#[test]
fn one_d_on_grid_along_y_renders_one_column_and_replicates() {
    let src = "export function render(index) { rgb(index / 2, pixelCount / 8, 0) }";
    let mut e = with_proj(src, 8, 1, ProjectionMode::Y);
    e.set_grid_map(4, 2);
    assert_eq!(e.effective_geometry().pixel_count, 2);
    let px = e.frame(Fx::ZERO).to_vec();
    for row in 0..2usize {
        for col in 0..4usize {
            assert_eq!(px[row * 4 + col][0], q(row as f64 / 2.0), "row {row}");
        }
    }
}

#[test]
fn one_d_along_an_axis_replicates_through_a_serpentine_map() {
    // 4x2 wired back and forth: the replicate reads coordinates, not indices.
    let coords: Vec<[Fx; 3]> = (0..8)
        .map(|i| {
            let row = i / 4;
            let col = if row == 1 { 3 - (i % 4) } else { i % 4 };
            [Fx::from_int(col), Fx::from_int(row), Fx::ZERO]
        })
        .collect();
    let src = "export function render(index) { rgb(index / 4, 0, 0) }";
    let mut e = with_proj(src, 8, 1, ProjectionMode::X);
    assert!(e.set_map(2, &coords));
    assert_eq!(e.effective_geometry().pixel_count, 4, "detected grid gives w");
    let px = e.frame(Fx::ZERO).to_vec();
    for i in 0..8usize {
        let col = if i / 4 == 1 { 3 - (i % 4) } else { i % 4 };
        assert_eq!(px[i][0], q(col as f64 / 4.0), "pixel {i}");
    }
}

// ---- 1D patterns on a 3D Layout: + along z ----

#[test]
fn one_d_on_lattice_along_z() {
    let src = "export function render(index) { rgb(index / 2, pixelCount / 8, 0) }";
    let mut e = with_proj(src, 8, 1, ProjectionMode::Z);
    assert!(e.set_map(3, &lattice(2, 2, 2)));
    assert_eq!(e.layout_dims(), 3);
    // no cell count for a 3D map: the strip is as long as the Layout
    assert_eq!(e.effective_geometry().pixel_count, 8);
    let px = e.frame(Fx::ZERO).to_vec();
    // z = 0 for the first plane, 1 for the second → strip index 0 and 7
    for i in 0..4 {
        assert_eq!(px[i][0], q(0.0), "z=0 plane, pixel {i}");
    }
    for i in 4..8 {
        assert_eq!(px[i][0], q(7.0 / 2.0), "z=1 plane, pixel {i}");
    }
}

// ---- 2D patterns on a 1D Layout: middle row · middle column ----

#[test]
fn two_d_on_strip_middle_row() {
    let mut e = with_proj(COORDS_2D, 4, 2, ProjectionMode::X);
    e.set_strip_layout();
    assert_eq!(e.layout_dims(), 1);
    assert_eq!(e.effective_geometry().mode, Some(ProjectionMode::X));
    let px = e.frame(Fx::ZERO).to_vec();
    for (i, p) in px.iter().enumerate() {
        assert_eq!(p[0], q(i as f64 / 4.0), "x walks the strip");
        assert_eq!(p[1], MID, "y pinned to the middle row");
    }
}

#[test]
fn two_d_on_strip_middle_column() {
    let mut e = with_proj(COORDS_2D, 4, 2, ProjectionMode::Y);
    e.set_strip_layout();
    let px = e.frame(Fx::ZERO).to_vec();
    for (i, p) in px.iter().enumerate() {
        assert_eq!(p[0], MID, "x pinned to the middle column");
        assert_eq!(p[1], q(i as f64 / 4.0), "y walks the strip");
    }
}

#[test]
fn render_frame_on_strip_gets_a_row_grid() {
    let src = "export var w = 0\nexport var h = 0\n\
               export function renderFrame() { w = gridWidth()\n h = gridHeight()\n fillRect(0, 0, 1, 1) }";
    let mut e = with_proj(src, 6, 2, ProjectionMode::X);
    e.set_strip_layout();
    let g = e.effective_geometry();
    assert_eq!((g.w, g.h), (6, 1), "a whole-frame pattern gets a w×1 grid");
    e.frame(Fx::ZERO);
    assert_eq!(e.var("w"), Some(Value::Num(Fx::from_int(6))));
    assert_eq!(e.var("h"), Some(Value::Num(Fx::from_int(1))));

    let mut p = Projection::DEFAULT;
    p.set(2, ProjectionMode::Y);
    e.set_projection(p);
    let g = e.effective_geometry();
    assert_eq!((g.w, g.h), (1, 6), "middle column transposes it");
}

#[test]
fn an_index_space_render_frame_is_a_strip_pattern() {
    // `renderFrame` + `fillHSV` never asked for a geometry: it stays 1D, gets
    // no fabricated grid on a strip, and is never strip-projected on a matrix
    // (a whole-frame pattern owns the buffer).
    let src = "export function renderFrame() { fillHSV(0, 0, 0, 1, 1, 1) }";
    let mut e = Engine::new(src, 8, 1).unwrap();
    assert_eq!(e.layout_dims(), 1, "no fabricated grid");
    assert_eq!(e.effective_geometry().pattern_dims, 1);
    assert_eq!(e.effective_projection(), None);

    e.set_grid_map(4, 2);
    let mut p = Projection::DEFAULT;
    p.set(1, ProjectionMode::X);
    e.set_projection(p);
    let g = e.effective_geometry();
    assert_eq!(g.pixel_count, 8, "not strip-rendered");
    assert_eq!((g.w, g.h), (4, 2), "and it keeps the Layout's grid");
    assert_eq!(e.frame(Fx::ZERO).len(), 8);
}

// ---- 2D patterns on a 3D Layout: repeat along z · y · x ----

#[test]
fn two_d_on_lattice_repeat_along_each_axis() {
    // 2x2x2: coordinates normalize to 0 and ~1 on every axis.
    let cases = [
        // (mode, which lattice axes feed the pattern's x and y)
        (ProjectionMode::Z, (0usize, 1usize)),
        (ProjectionMode::Y, (0, 2)),
        (ProjectionMode::X, (1, 2)),
    ];
    for (mode, (ax, ay)) in cases {
        let mut e = with_proj(COORDS_2D, 8, 2, mode);
        assert!(e.set_map(3, &lattice(2, 2, 2)));
        assert_eq!(e.effective_geometry().mode, Some(mode));
        let px = e.frame(Fx::ZERO).to_vec();
        for i in 0..8usize {
            let cell = [i % 2, (i / 2) % 2, i / 4];
            assert_eq!(px[i][0], qcell(cell[ax], 2), "{mode} pixel {i} x");
            assert_eq!(px[i][1], qcell(cell[ay], 2), "{mode} pixel {i} y");
        }
    }
}

// ---- 3D patterns on a 1D Layout: a line through the centre ----

#[test]
fn three_d_on_strip_line_along_each_axis() {
    for (mode, axis) in [
        (ProjectionMode::X, 0usize),
        (ProjectionMode::Y, 1),
        (ProjectionMode::Z, 2),
    ] {
        let mut e = with_proj(COORDS_3D, 4, 3, mode);
        e.set_strip_layout();
        assert_eq!(e.layout_dims(), 1);
        let px = e.frame(Fx::ZERO).to_vec();
        for (i, p) in px.iter().enumerate() {
            for a in 0..3 {
                let want = if a == axis { q(i as f64 / 4.0) } else { MID };
                assert_eq!(p[a], want, "{mode} pixel {i} axis {a}");
            }
        }
    }
}

// ---- 3D patterns on a 2D Layout: slice xy · xz · yz ----

#[test]
fn three_d_on_grid_slices() {
    for (mode, slot) in [
        (ProjectionMode::Xy, [Some(0usize), Some(1), None]),
        (ProjectionMode::Xz, [Some(0), None, Some(1)]),
        (ProjectionMode::Yz, [None, Some(0), Some(1)]),
    ] {
        let mut e = with_proj(COORDS_3D, 8, 3, mode);
        e.set_grid_map(4, 2);
        assert_eq!(e.effective_geometry().mode, Some(mode));
        let px = e.frame(Fx::ZERO).to_vec();
        for i in 0..8usize {
            let layout = [qcell(i % 4, 4), qcell(i / 4, 2)];
            for a in 0..3 {
                let want = match slot[a] {
                    Some(k) => layout[k],
                    None => MID,
                };
                assert_eq!(px[i][a], want, "{mode} pixel {i} axis {a}");
            }
        }
    }
}

// ---- native pairs project nothing ----

#[test]
fn native_pairs_have_no_projection() {
    let mut e = Engine::new(COORDS_2D, 8, 1).unwrap();
    e.set_grid_map(4, 2);
    assert_eq!(e.layout_dims(), 2);
    assert_eq!(e.effective_projection(), None);
    let g = e.effective_geometry();
    assert_eq!((g.pattern_dims, g.layout_dims, g.pixel_count), (2, 2, 8));

    let mut e = Engine::new(INDEX_1D, 8, 1).unwrap();
    assert_eq!(e.layout_dims(), 1);
    assert_eq!(e.effective_projection(), None);
    e.frame(Fx::ZERO);
}

#[test]
fn every_default_is_a_no_op() {
    // The default triple must reproduce the pre-projection engine on every
    // pair, so upgrading a device changes nothing until someone picks.
    const FRAME_BULK: &str = "export function renderFrame() { fillRect(0, 0, 1, 1) }";
    const FRAME_INDEX: &str = "export function renderFrame() { fillHSV(0, 0, 0, 1, 1, 1) }";
    for (src, install) in [
        (COORDS_2D, 1u8),
        (COORDS_3D, 1),
        (INDEX_1D, 2),
        (COORDS_3D, 2),
        (COORDS_2D, 3),
        (INDEX_1D, 3),
        (FRAME_BULK, 1),
        (FRAME_BULK, 3),
        (FRAME_INDEX, 1),
        (FRAME_INDEX, 2),
        (FRAME_INDEX, 3),
    ] {
        let mut before = Engine::new(src, 8, 1).unwrap();
        let mut after = Engine::new(src, 8, 1).unwrap();
        for e in [&mut before, &mut after] {
            match install {
                1 => e.set_strip_layout(),
                2 => e.set_grid_map(4, 2),
                _ => {
                    e.set_map(3, &lattice(2, 2, 2));
                }
            }
        }
        after.set_projection(Projection::DEFAULT);
        assert_eq!(
            before.frame(Fx::ZERO),
            after.frame(Fx::ZERO),
            "{src} on a {install}D Layout"
        );
        // …and "no-op" means the pattern's own view is untouched: every
        // render call the Layout has, and the grid the Layout installed.
        let g = after.effective_geometry();
        assert_eq!(g.pixel_count, 8, "{src} on a {install}D Layout: pixelCount");
        let want = match (install, src) {
            (2, _) => (4, 2),
            // the one deliberate addition: a grid-space `renderFrame` on a
            // strip Layout gets the w×1 grid §5.4d gives it, which no
            // pre-projection engine could reach (nothing installed the 1D
            // Layout under a bulk pattern — it got the fabricated √n grid)
            (1, s) if s == FRAME_BULK => (8, 1),
            _ => (0, 0),
        };
        assert_eq!((g.w, g.h), want, "{src} on a {install}D Layout: grid");
    }
}

#[test]
fn switching_projection_restores_the_layout() {
    let src = "export function render(index) { rgb(index / 4, pixelCount / 8, 0) }";
    let mut e = Engine::new(src, 8, 1).unwrap();
    e.set_grid_map(4, 2);
    let native = e.frame(Fx::ZERO).to_vec();

    let mut p = Projection::DEFAULT;
    p.set(1, ProjectionMode::X);
    e.set_projection(p);
    assert_eq!(e.effective_geometry().pixel_count, 4);
    e.frame(Fx::ZERO);

    e.set_projection(Projection::DEFAULT);
    let g = e.effective_geometry();
    assert_eq!(g.pixel_count, 8, "pixelCount goes back to the Layout's");
    assert_eq!((g.w, g.h), (4, 2), "and so does the grid");
    assert_eq!(e.installed_map().map(|m| m.dims), Some(2));
    assert_eq!(e.frame(Fx::ZERO), &native[..]);
}

#[test]
fn map_mode_is_never_projected() {
    let src = "export function render(index) { plot(index / 8, 0.5) }";
    let mut e = Engine::new(src, 8, 1).unwrap();
    e.set_grid_map(4, 2);
    let mut p = Projection::DEFAULT;
    p.set(1, ProjectionMode::X);
    e.set_projection(p);
    e.enable_map_mode();
    assert!(!e.run_map());
    let (dims, coords) = e.map();
    assert_eq!(dims, 2);
    assert_eq!(coords.len(), 8, "a map program always plots every pixel");
}
