//! Scenes on 1-D strips (Gitea #985): a strip is an n×1 grid in index
//! order, so a box is a SEGMENT (`x`, `w`), `flipx` reverses it, a tiled
//! 1-row sprite repeats along it, and text draws nothing.
//!
//! Pattern layers here are real engines over a real 150 px strip — the
//! frame a pattern writes on a strip is index order, which is exactly what
//! the n×1 grid addresses — driven through the shared `SceneDriver`.

use luxel_core::arena::FrameVec;
use luxel_core::compose::{Compositor, SceneDriver, SceneHost, SpriteView};
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::outpipe::{strip_grid, GridMap};
use luxel_core::scene::{self, TextSource};

const N: usize = 150;

/// Red rising along the strip: pixel i is distinguishable from every other.
const RED_RAMP: &str = "export function render(index) { rgb(index / pixelCount, 0, 0) }";
/// Green rising along the strip.
const GREEN_RAMP: &str = "export function render(index) { rgb(0, index / pixelCount, .5) }";

struct Host {
    engines: Vec<Option<Engine>>,
    sprite: Option<Vec<u8>>,
}

impl SceneHost for Host {
    fn pattern_frame(&mut self, layer: usize, delta: Fx) -> Option<&[[u8; 3]]> {
        Some(self.engines.get_mut(layer)?.as_mut()?.frame(delta))
    }
    fn sprite(&mut self, _layer: usize) -> Option<SpriteView<'_>> {
        self.sprite.as_deref().and_then(SpriteView::parse)
    }
    fn text(&mut self, _layer: usize, _source: &TextSource) -> Option<&str> {
        Some("HELLO")
    }
}

fn engine(src: &str) -> Option<Engine> {
    Some(Engine::new(src, N as u32, 1).expect("compiles"))
}

/// One frame of `wire` on `grid`, layer `i` drawn from `engines[i]`.
fn composite(
    wire: &str,
    grid: GridMap,
    engines: Vec<Option<Engine>>,
    sprite: Option<Vec<u8>>,
) -> Vec<[u8; 3]> {
    let scene = scene::parse(wire).expect("scene parses");
    let mut comp = Compositor::new(grid);
    comp.set_scene(&scene);
    let mut host = Host { engines, sprite };
    let mut dst: FrameVec = luxel_core::arena::empty();
    assert!(SceneDriver::new().frame(&mut comp, &mut dst, grid.len(), Fx::ZERO, &mut host));
    dst.to_vec()
}

/// What a pattern alone writes on the strip — the reference a segment shows.
fn alone(src: &str) -> Vec<[u8; 3]> {
    engine(src).unwrap().frame(Fx::ZERO).to_vec()
}

fn strip() -> GridMap {
    strip_grid(0, N).expect("a 150 px strip is a row")
}

#[test]
fn a_strip_is_a_150x1_row() {
    assert_eq!(strip(), GridMap::new(150, 1, false));
}

#[test]
fn two_patterns_split_the_strip_into_segments() {
    // `h` 0 and `h` 1 both mean the one row
    let wire = "S 0000beef zones\n\
                L pat 0 0 75 0 normal 100 none fill 1\n\
                L pat 75 0 75 1 normal 100 none fill 1\n";
    let px = composite(
        wire,
        strip(),
        vec![engine(RED_RAMP), engine(GREEN_RAMP)],
        None,
    );
    let (red, green) = (alone(RED_RAMP), alone(GREEN_RAMP));
    // the left segment is the red pattern's own left half...
    assert_eq!(px[..75], red[..75]);
    // ...and the right one the green pattern's RIGHT half (a pattern layer's
    // box is a window onto a full-layout render, not a squeezed copy)
    assert_eq!(px[75..], green[75..]);
    // both segments are lit, and they are not the same picture
    assert!(px[1..75].iter().all(|p| p[0] > 0 && p[1] == 0));
    assert!(px[75..].iter().all(|p| p[1] > 0 && p[0] == 0));
}

#[test]
fn flipx_reverses_a_segment_in_place() {
    let wire = "S 0000beef flip\n\
                L pat 75 0 75 0 normal 100 none fill 3\n";
    let px = composite(wire, strip(), vec![engine(GREEN_RAMP)], None);
    let green = alone(GREEN_RAMP);
    assert!(
        px[..75].iter().all(|p| *p == [0, 0, 0]),
        "outside the segment is untouched"
    );
    for k in 0..75 {
        assert_eq!(px[75 + k], green[149 - k], "segment cell {k}");
    }
}

/// An 8×1 bulb: a lit bulb, a gap, a dim bulb, a gap… (index 0 transparent).
fn bulbs() -> Vec<u8> {
    let mut v = b"LXSP".to_vec();
    v.push(1);
    v.extend_from_slice(&[8, 1, 1, 0, 2, 5, 0]);
    v.extend_from_slice(b"bulbs");
    v.extend_from_slice(&[255, 200, 80, 90, 20, 10]);
    v.extend_from_slice(&[1, 0, 0, 2, 2, 0, 0, 1]);
    v
}

fn bulb(i: usize) -> [u8; 3] {
    match [1, 0, 0, 2, 2, 0, 0, 1][i % 8] {
        1 => [255, 200, 80],
        2 => [90, 20, 10],
        _ => [0, 0, 0],
    }
}

#[test]
fn a_tiled_8x1_sprite_with_w_0_covers_the_whole_strip() {
    let wire = "S 0000beef bulbs\n\
                L sprite 0 0 0 0 normal 100 none tile 1\n\
                I 00000001\n";
    let px = composite(wire, strip(), vec![None], Some(bulbs()));
    for (i, p) in px.iter().enumerate() {
        assert_eq!(*p, bulb(i), "pixel {i}");
    }
    // from an offset origin the tiling starts at `x` and still runs to the end
    let wire = "S 0000beef bulbs\n\
                L sprite 3 0 0 1 normal 100 none tile 1\n\
                I 00000001\n";
    let px = composite(wire, strip(), vec![None], Some(bulbs()));
    assert!(px[..3].iter().all(|p| *p == [0, 0, 0]));
    for (i, p) in px.iter().enumerate().skip(3) {
        assert_eq!(*p, bulb(i - 3), "pixel {i}");
    }
}

#[test]
fn a_text_layer_is_black_on_a_strip() {
    let wire = "S 0000beef text\n\
                L text 0 0 0 0 normal 100 none fill 1\n\
                T lit HELLO\n\
                F tiny ffffff l none 0\n";
    let px = composite(wire, strip(), vec![None], None);
    assert!(px.iter().all(|p| *p == [0, 0, 0]), "no glyph fits one row");
    // ...so a clip layer over it has no coverage either
    let clipped = format!("{wire}L color 0 0 0 0 normal 100 none fill 1\nK ff0000\nX clip\n");
    let px = composite(&clipped, strip(), vec![None, None], None);
    assert!(px.iter().all(|p| *p == [0, 0, 0]));
    // the control: the same layer on a 16x8 panel does draw
    let px = composite(wire, GridMap::new(16, 8, false), vec![None], None);
    assert!(px.iter().any(|p| *p != [0, 0, 0]));
}

#[test]
fn ramps_masks_and_blends_work_on_segments() {
    // a ramped red base, a colour wash multiplied over its right half
    let wire = "S 0000beef ramp\n\
                L pat 0 0 0 0 normal 100 none fill 1\n\
                R 100 0:000000 255:00ff00\n\
                L color 75 0 75 0 multiply 100 none fill 1\n\
                K 808080\n";
    let px = composite(wire, strip(), vec![engine(RED_RAMP), None], None);
    // the ramp turned red brightness into green
    assert!(px[10..75]
        .iter()
        .all(|p| p[0] == 0 && p[1] > 0 && p[2] == 0));
    // and the multiply halved it only inside its segment
    let base = composite(
        "S 0000beef ramp\nL pat 0 0 0 0 normal 100 none fill 1\nR 100 0:000000 255:00ff00\n",
        strip(),
        vec![engine(RED_RAMP)],
        None,
    );
    assert_eq!(px[..75], base[..75]);
    for i in 75..N {
        assert_eq!(px[i][1], (base[i][1] as u32 * 128 / 255) as u8, "pixel {i}");
    }
}
