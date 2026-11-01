//! `setRenderSize(w, h, filter)` — a pattern's own render resolution
//! (docs/lang.md "Render resolution").
//!
//! The engine renders the pattern on a w×h grid and expands the small frame
//! onto the fixture through each fixture pixel's mapped (x, y): nearest is
//! `fillCanvas`'s geometry, bilinear is `canvasGet`'s. Most tests here are
//! EQUIVALENCES rather than goldens — a resized pattern against the same
//! pattern run natively on a fixture of the small size, or against a
//! `fillCanvas` of the same canvas — so they pin the semantics, not just
//! today's bytes. `bilinear_golden` pins the bytes too; tools/wasm-smoke.mjs
//! asserts the same ones through the wasm FFI.

use luxel_core::compose::{Compositor, SceneDriver, SceneHost, SpriteView};
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::outpipe::GridMap;
use luxel_core::projection::{Projection, ProjectionMode};
use luxel_core::scene::TextSource;
use luxel_core::vm::Value;

fn num(e: &Engine, name: &str) -> f64 {
    match e.var(name) {
        Some(Value::Num(v)) => v.to_f64(),
        other => panic!("{name}: {other:?}"),
    }
}

fn grid_engine(src: &str, w: u16, h: u16) -> Engine {
    let mut e = Engine::new(src, w as u32 * h as u32, 1).unwrap();
    e.set_grid_map(w, h);
    e
}

fn frames(e: &mut Engine, n: usize) -> Vec<[u8; 3]> {
    for _ in 0..n - 1 {
        e.frame(Fx::from_int(16));
    }
    let out = e.frame(Fx::from_int(16)).to_vec();
    assert!(e.take_error().is_none(), "pattern raised an error");
    out
}

/// Nearest block-expand of a row-major `sw × sh` frame onto `w × h`.
fn block_expand(small: &[[u8; 3]], sw: usize, sh: usize, w: usize, h: usize) -> Vec<[u8; 3]> {
    let mut out = Vec::new();
    for r in 0..h {
        for c in 0..w {
            out.push(small[(r * sh / h) * sw + c * sw / w]);
        }
    }
    out
}

/// A pattern body in each render entry, with and without the size call,
/// so one source text drives both sides of an equivalence.
fn sized(size: &str, body: &str) -> String {
    format!("{size}\n{body}")
}

const BODIES: &[&str] = &[
    // per-pixel 2D: coordinates, index and pixelCount all observable
    "export function render2D(index, x, y) { rgb(x, y, index / pixelCount) }",
    // per-pixel 1D on a grid map: render(index, x) receives the mapped x
    "export function render(index, x) { hsv(x + index / 7, 1, 1) }",
    // whole frame, grid space: gridWidth/gridIndex describe the small grid
    "export function renderBulk() { clear()\n hsv(0.3, 1, 1)\n \
     for (var r = 0; r < gridHeight(); r++) setPixel(gridIndex(r, r))\n \
     hsv(0.7, 1, 1)\n setPixel(gridIndex(gridWidth() - 1, 0)) }",
    // whole frame, persistence + a post chain running on the small frame
    "var t = 0\nexport function beforeRender(d) { t += 1\n setBlur(0.5)\n setGlow(0.3)\n setGamma(2.2) }\n\
     export function renderBulk() { fade(0.6)\n hsv(t / 5, 1, 1)\n setPixel(t % pixelCount) }",
    // per-pixel state: sized to the small grid, read back next frame
    "export function render2D(index, x, y) { var v = pixelState(index)\n \
     setPixelState(index, frac(v + x * 0.25 + 0.1))\n hsv(v, 1, y) }",
];

#[test]
fn nearest_on_a_grid_is_the_native_small_frame_block_expanded() {
    for body in BODIES {
        let mut small = grid_engine(body, 4, 4);
        let want = block_expand(&frames(&mut small, 5), 4, 4, 8, 8);
        let mut big = grid_engine(&sized("setRenderSize(4, 4)", body), 8, 8);
        let got = frames(&mut big, 5);
        assert_eq!(got.len(), 64);
        assert_eq!(got, want, "{body}");
        // a 3x upscale, not a power of two
        let mut small = grid_engine(body, 4, 4);
        let want = block_expand(&frames(&mut small, 5), 4, 4, 12, 12);
        let mut big = grid_engine(&sized("setRenderSize(4, 4)", body), 12, 12);
        assert_eq!(frames(&mut big, 5), want, "{body} at 12x12");
    }
}

#[test]
fn pixel_count_and_the_grid_describe_the_small_grid() {
    let src = "setRenderSize(5, 3)\n\
        export var pc = pixelCount\n\
        var buf = array(pixelCount)\n\
        export var bl = arrayLength(buf)\n\
        export var gw = 0\nexport var gh = 0\nexport var later = 0\n\
        export function beforeRender(d) { gw = gridWidth()\n gh = gridHeight()\n later = pixelCount }\n\
        export function renderBulk() { fill() }";
    let mut e = grid_engine(src, 10, 6);
    // `pixelCount` follows the call at once — the top-level array is 15 long
    assert_eq!(num(&e, "pc"), 15.0);
    assert_eq!(num(&e, "bl"), 15.0);
    e.frame(Fx::from_int(16));
    assert_eq!((num(&e, "gw"), num(&e, "gh"), num(&e, "later")), (5.0, 3.0, 15.0));
    let g = e.effective_geometry();
    assert_eq!((g.pixel_count, g.w, g.h), (15, 5, 3));
    // the host still sees the fixture
    assert_eq!(e.pixel_count(), 60);
    assert_eq!(e.pixels().len(), 60);
}

#[test]
fn zero_zero_restores_native_and_the_size_can_change_per_frame() {
    let src = "export var size = 4\nexport var pc = 0\n\
        export function beforeRender(d) { setRenderSize(size, size)\n pc = pixelCount }\n\
        export function render2D(index, x, y) { rgb(x, y, 0) }";
    let mut e = grid_engine(src, 8, 8);
    let native = frames(&mut grid_engine("export function render2D(index, x, y) { rgb(x, y, 0) }", 8, 8), 1);
    let four = e.frame(Fx::from_int(16)).to_vec();
    assert_eq!(num(&e, "pc"), 16.0);
    assert_ne!(four, native);
    e.set_var("size", Fx::ZERO);
    assert_eq!(e.frame(Fx::from_int(16)).to_vec(), native);
    assert_eq!(num(&e, "pc"), 64.0);
    assert_eq!(e.effective_geometry().pixel_count, 64);
    e.set_var("size", Fx::from_int(2));
    let two = e.frame(Fx::from_int(16)).to_vec();
    assert_eq!(num(&e, "pc"), 4.0);
    let mut small = grid_engine("export function render2D(index, x, y) { rgb(x, y, 0) }", 2, 2);
    assert_eq!(two, block_expand(&frames(&mut small, 1), 2, 2, 8, 8));
    assert_eq!(e.pixels().len(), 64);
}

#[test]
fn a_size_past_the_cap_is_an_error_and_renders_native() {
    let src = "setRenderSize(200, 200)\nexport function render(i) { rgb(1, 0, 0) }";
    let mut e = Engine::new(src, 8, 1).unwrap();
    let err = e.take_error().expect("cap refusal");
    assert!(err.message.contains("16384"), "{}", err.message);
    assert_eq!(e.frame(Fx::from_int(16)).to_vec(), vec![[255, 0, 0]; 8]);
}

#[test]
fn a_strip_stretches_n_pixels_along_itself() {
    // 1D Layout, no map: setRenderSize(n, 1) renders n and stretches
    let body = "export function render(index) { rgb(index / pixelCount, 0, 0) }";
    let mut e = Engine::new(&sized("setRenderSize(4, 1)", body), 12, 1).unwrap();
    let got = frames(&mut e, 1);
    let mut small = Engine::new(body, 4, 1).unwrap();
    let s = frames(&mut small, 1);
    let want: Vec<_> = (0..12).map(|i| s[i / 3]).collect();
    assert_eq!(got, want);
    assert_eq!(e.effective_geometry().pixel_count, 4);
    // and on a 2D panel the strip is laid along x, every row the same
    let mut p = grid_engine(&sized("setRenderSize(4, 1)", body), 8, 3);
    let got = frames(&mut p, 1);
    for r in 0..3 {
        for c in 0..8 {
            assert_eq!(got[r * 8 + c], s[c / 2]);
        }
    }
}

#[test]
fn a_resized_pattern_picks_the_entry_its_own_grid_calls_for() {
    // render AND render2D: a w×h size runs the 2D entry even on a strip,
    // a w×1 size the 1D one even on a panel
    let src = |size: &str| {
        format!(
            "{size}\nexport function render(i) {{ rgb(1, 0, 0) }}\n\
             export function render2D(i, x, y) {{ rgb(0, 1, 0) }}"
        )
    };
    let mut strip = Engine::new(&src("setRenderSize(3, 3)"), 9, 1).unwrap();
    assert_eq!(frames(&mut strip, 1)[0], [0, 255, 0]);
    let mut panel = grid_engine(&src("setRenderSize(9, 1)"), 3, 3);
    assert_eq!(frames(&mut panel, 1)[0], [255, 0, 0]);
}

/// A sparse, irregular map: a ring of 24 points.
fn ring(n: usize) -> Vec<[Fx; 3]> {
    (0..n)
        .map(|i| {
            let a = i as f64 / n as f64 * std::f64::consts::TAU;
            [Fx::from_f64(0.5 + 0.45 * a.cos()), Fx::from_f64(0.5 + 0.4 * a.sin()), Fx::ZERO]
        })
        .collect()
}

#[test]
fn any_map_samples_the_small_grid_exactly_like_fill_canvas() {
    let canvas = "var hs = array(16)\nfor (var k = 0; k < 16; k++) hs[k] = k / 16\n";
    for filter in [0, 1] {
        let resized = format!(
            "setRenderSize(4, 4, {filter})\nexport function render2D(index, x, y) {{ hsv(index / 16, 1, 1) }}"
        );
        let canvas_fill = format!(
            "{canvas}export function renderBulk() {{ fillCanvas(hs, 1, 1, 4, 4, {filter}) }}"
        );
        let mut rigs: Vec<(u32, Box<dyn Fn(&mut Engine)>)> = vec![
            (24, Box::new(|e: &mut Engine| assert!(e.set_map(2, &ring(24))))),
            (64, Box::new(|e: &mut Engine| e.set_grid_map(8, 8))),
            (60, Box::new(|e: &mut Engine| e.set_grid_map(10, 6))),
        ];
        // a serpentine coordinate map
        rigs.push((
            30,
            Box::new(|e: &mut Engine| {
                let mut m = Vec::new();
                for r in 0..5 {
                    for k in 0..6 {
                        let c = if r % 2 == 1 { 5 - k } else { k };
                        m.push([Fx::from_int(c), Fx::from_int(r), Fx::ZERO]);
                    }
                }
                assert!(e.set_map(2, &m));
            }),
        ));
        for (n, install) in &rigs {
            let mut a = Engine::new(&resized, *n, 1).unwrap();
            install(&mut a);
            let mut b = Engine::new(&canvas_fill, *n, 1).unwrap();
            install(&mut b);
            assert_eq!(frames(&mut a, 1), frames(&mut b, 1), "filter {filter}, {n} px");
        }
    }
}

/// Bilinear bytes, pinned: a 2×2 frame of pure colours onto a 4×4 grid.
/// tools/wasm-smoke.mjs asserts the same 48 bytes through the FFI.
const BILINEAR_SRC: &str = "setRenderSize(2, 2, 1)\n\
    export function render2D(index, x, y) { rgb(index == 0 || index == 3, index == 1, index >= 2) }";

#[test]
fn bilinear_golden() {
    let mut e = grid_engine(BILINEAR_SRC, 4, 4);
    let got: Vec<u8> = frames(&mut e, 1).into_iter().flatten().collect();
    assert_eq!(
        got,
        [
            255, 0, 0, 212, 42, 0, 42, 212, 0, 0, 255, 0, //
            212, 0, 42, 183, 35, 42, 70, 176, 42, 42, 212, 42, //
            42, 0, 212, 70, 7, 212, 183, 35, 212, 212, 42, 212, //
            0, 0, 255, 42, 0, 255, 212, 0, 255, 255, 0, 255,
        ]
    );
}

#[test]
fn bilinear_is_the_integer_lerp_of_the_four_cells() {
    // an independent spelling of the kernel: texel centres at (i + 0.5)/n,
    // `a + ((b - a)·t >> 16)`, x then y
    let src = "setRenderSize(3, 2, 1)\nexport function render2D(index, x, y) { rgb(index / 5, x, y) }";
    let mut small = grid_engine(
        "export function render2D(index, x, y) { rgb(index / 5, x, y) }",
        3,
        2,
    );
    let s = frames(&mut small, 1);
    for (w, h) in [(7u16, 5u16), (12, 4), (3, 2)] {
        let mut e = grid_engine(src, w, h);
        let got = frames(&mut e, 1);
        let norm = |v: usize, n: usize| -> i64 {
            if n <= 1 { 0 } else { (v as i64 * 65_535 + (n as i64 - 1) / 2) / (n as i64 - 1) }
        };
        let axis = |x: i64, n: usize| {
            let pos = x * n as i64 - (1 << 15);
            let (i, t) = (pos >> 16, pos & 0xFFFF);
            let last = n as i64 - 1;
            (i.clamp(0, last) as usize, (i + 1).clamp(0, last) as usize, t as i32)
        };
        let lerp = |a: i32, b: i32, t: i32| a + (((b - a) * t) >> 16);
        for r in 0..h as usize {
            for c in 0..w as usize {
                let (c0, c1, tx) = axis(norm(c, w as usize), 3);
                let (r0, r1, ty) = axis(norm(r, h as usize), 2);
                let mut want = [0u8; 3];
                for k in 0..3 {
                    let top = lerp(s[r0 * 3 + c0][k] as i32, s[r0 * 3 + c1][k] as i32, tx);
                    let bot = lerp(s[r1 * 3 + c0][k] as i32, s[r1 * 3 + c1][k] as i32, tx);
                    want[k] = lerp(top, bot, ty) as u8;
                }
                assert_eq!(got[r * w as usize + c], want, "{w}x{h} at ({c}, {r})");
            }
        }
    }
}

#[test]
fn the_size_outranks_projection() {
    // a 1D pattern laid along x by projection: with setRenderSize in force
    // the projection is not, and the output is the plain resized one
    let body = "export function render(index) { rgb(index / pixelCount, 0, 0) }";
    let src = sized("setRenderSize(4, 1)", body);
    let mut plain = grid_engine(&src, 8, 8);
    let want = frames(&mut plain, 1);
    let mut p = Projection::DEFAULT;
    p.set(1, ProjectionMode::Y);
    let mut projected = grid_engine(&src, 8, 8);
    projected.set_projection(p);
    assert_eq!(frames(&mut projected, 1), want);
    assert_eq!(projected.effective_projection(), None);
    // and without the size the same projection is in force
    let mut native = grid_engine(body, 8, 8);
    native.set_projection(p);
    native.frame(Fx::from_int(16));
    assert_eq!(native.effective_projection(), Some(ProjectionMode::Y));
}

#[test]
fn pixel_state_is_sized_to_the_small_grid() {
    let body = "export function render2D(index, x, y) { setPixelState(index, x)\n rgb(pixelState(index), 0, 0) }";
    let mut big = grid_engine(&sized("setRenderSize(4, 4)", body), 16, 16);
    frames(&mut big, 3);
    let mut small = grid_engine(body, 4, 4);
    frames(&mut small, 3);
    assert_eq!(big.pixel_state_bytes(), small.pixel_state_bytes());
    assert!(big.pixel_state_bytes() > 0);
}

#[test]
fn events_keep_their_normalized_coordinates() {
    let src = "setRenderSize(4, 4)\nvar ev = array(4)\nexport var ex = 0\nexport var ey = 0\n\
        export function beforeRender(d) { if (eventCount() > 0) { readEvent(ev)\n ex = ev[1]\n ey = ev[2] } }\n\
        export function render2D(index, x, y) { rgb(x, y, 0) }";
    let mut e = grid_engine(src, 16, 16);
    e.push_event([Fx::ONE, Fx::from_f64(0.25), Fx::from_f64(0.75), Fx::ONE]);
    e.frame(Fx::from_int(16));
    assert_eq!((num(&e, "ex"), num(&e, "ey")), (0.25, 0.75));
}

/// A scene host whose one pattern layer is a REAL engine.
struct EngineHost {
    engine: Engine,
}

impl SceneHost for EngineHost {
    fn pattern_frame(&mut self, _layer: usize, delta: Fx) -> Option<&[[u8; 3]]> {
        Some(self.engine.frame(delta))
    }
    fn sprite(&mut self, _layer: usize) -> Option<SpriteView<'_>> {
        None
    }
    fn text(&mut self, _layer: usize, _source: &TextSource) -> Option<&str> {
        None
    }
}

#[test]
fn a_scene_layer_engine_with_a_render_size_just_works() {
    let body = "export function render2D(index, x, y) { hsv(x * 0.5 + y, 1, 1) }";
    let src = sized("setRenderSize(4, 4, 1)", body);
    let scene = luxel_core::scene::parse("S 0123abcd Sized\nL pat 0 0 0 0 normal 100 none fill 1\n").unwrap();
    let mut comp = Compositor::new(GridMap::new(8, 8, false));
    comp.set_scene(&scene);
    let mut driver = SceneDriver::new();
    let mut host = EngineHost { engine: grid_engine(&src, 8, 8) };
    let mut dst: luxel_core::arena::FrameVec = luxel_core::arena::empty();
    assert!(driver.frame(&mut comp, &mut dst, 64, Fx::from_int(16), &mut host));
    let mut solo = grid_engine(&src, 8, 8);
    assert_eq!(dst.to_vec(), frames(&mut solo, 1));
}

#[test]
fn fill_canvas_and_paint_canvas_take_a_bilinear_filter() {
    // paintCanvas through the same kernel: filter 1 differs from nearest
    // and matches a resized paint() pattern byte for byte
    let pal = "setPalette([0, 1, 0, 0, 1, 0, 0, 1])\n";
    let canvas = "var vs = array(4)\nvs[0] = 0\nvs[1] = 0.3\nvs[2] = 0.6\nvs[3] = 0.9\n";
    let mut near = grid_engine(&format!("{pal}{canvas}export function renderBulk() {{ paintCanvas(vs, 2, 2, 1, 0) }}"), 6, 6);
    let mut bil = grid_engine(&format!("{pal}{canvas}export function renderBulk() {{ paintCanvas(vs, 2, 2, 1, 1) }}"), 6, 6);
    let mut resized = grid_engine(
        &format!("{pal}{canvas}setRenderSize(2, 2, 1)\nexport function render2D(index, x, y) {{ paint(vs[index]) }}"),
        6,
        6,
    );
    let (n, b) = (frames(&mut near, 1), frames(&mut bil, 1));
    assert_ne!(n, b);
    assert_eq!(b, frames(&mut resized, 1));
}

#[test]
fn the_docs_worked_example_runs() {
    // docs/lang.md "Render resolution", verbatim
    let src = "setRenderSize(16, 16, 1)\n\
        var heat = array(pixelCount)\n\
        export function beforeRender(delta) {\n\
          for (var i = 0; i < pixelCount; i++) heat[i] *= 0.96\n\
          heat[floor(random(pixelCount))] = 1\n\
        }\n\
        export function render2D(index, x, y) {\n\
          hsv(0.02 + heat[index] * 0.1, 1, heat[index])\n\
        }";
    let mut e = grid_engine(src, 64, 64);
    let px = frames(&mut e, 30);
    assert_eq!(px.len(), 4096);
    assert!(px.iter().any(|p| p[0] > 0), "some heat shows");
    assert_eq!(e.effective_geometry().pixel_count, 256);
}
