//! `setTrail(decay, mode)` — the afterglow stage at the head of the post
//! chain (docs/lang.md "the post-process chain").

use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;
use luxel_core::vm::Value;

fn run(src: &str, n: u32, frames: usize) -> Vec<Vec<[u8; 3]>> {
    let mut e = Engine::new(src, n, 1).unwrap();
    let out = (0..frames).map(|_| e.frame(Fx::from_int(16)).to_vec()).collect();
    assert!(e.take_error().is_none());
    out
}

/// The stage's arithmetic, spelled independently: decay in 1/256ths.
fn k(decay: f64) -> u32 {
    ((Fx::from_f64(decay).raw() as i64 * 256 + 32_768) >> 16) as u32
}

#[test]
fn off_by_default_and_off_at_zero() {
    let body = "export var f = 0\nexport function beforeRender(d) { f += 1 }\n\
        export function render(i) { rgb((i + f) % 2, 0, 0) }";
    let plain = run(body, 6, 4);
    let zero = run(&format!("setTrail(0)\n{body}"), 6, 4);
    assert_eq!(plain, zero);
}

#[test]
fn max_mode_is_a_decaying_afterglow() {
    // one lit pixel walks along an 8-pixel strip; behind it, max(new, prev·k)
    let src = "setTrail(0.75)\nexport var f = 0\nexport function beforeRender(d) { f += 1 }\n\
        export function render(i) { rgb(i == (f - 1) % 8, 0, 0) }";
    let frames = run(src, 8, 6);
    let kk = k(0.75);
    let mut want = [0u32; 8];
    for (f, got) in frames.iter().enumerate() {
        for (i, w) in want.iter_mut().enumerate() {
            let new = if i == f % 8 { 255 } else { 0 };
            *w = if f == 0 { new } else { new.max((*w * kk) >> 8) };
        }
        let red: Vec<u32> = got.iter().map(|p| p[0] as u32).collect();
        assert_eq!(red, want.to_vec(), "frame {f}");
    }
    // the head is full, the tail strictly fades
    let last = &frames[5];
    assert!(last[5][0] == 255 && last[4][0] > last[3][0] && last[3][0] > last[2][0]);
}

#[test]
fn add_mode_accumulates_and_clamps() {
    let src = "setTrail(0.5, 1)\nexport function render(i) { rgb(100 / 255, 0, 0) }";
    let frames = run(src, 1, 8);
    let input = frames[0][0][0] as u32; // the pattern's own byte, untrailed
    let mut v = 0u32;
    for (f, got) in frames.iter().enumerate() {
        v = if f == 0 { input } else { (input + ((v * 128) >> 8)).min(255) };
        assert_eq!(got[0][0] as u32, v, "frame {f}");
    }
    // and a bright input saturates rather than wrapping
    let hot = run("setTrail(0.9, 1)\nexport function render(i) { rgb(1, 1, 1) }", 1, 3);
    assert_eq!(hot[2][0], [255, 255, 255]);
}

#[test]
fn returns_the_previous_decay_and_zero_releases() {
    let src = "export var a = -1\nexport var b = -1\n\
        a = setTrail(0.5)\nb = setTrail(0.25)\n\
        export var on = 1\n\
        export var f = 0\nexport function beforeRender(d) { f += 1\n if (!on) setTrail(0) }\n\
        export function render(i) { rgb(f == 1, 0, 0) }";
    let mut e = Engine::new(src, 2, 1).unwrap();
    assert_eq!(e.var("a"), Some(Value::Num(Fx::ZERO)));
    assert_eq!(e.var("b"), Some(Value::Num(Fx::from_f64(0.5))));
    e.frame(Fx::from_int(16));
    assert_eq!(e.frame(Fx::from_int(16))[0][0], ((255 * k(0.25)) >> 8) as u8);
    e.set_var("on", Fx::ZERO);
    // off: the pattern's own colour, immediately
    assert_eq!(e.frame(Fx::from_int(16))[0], [0, 0, 0]);
}

#[test]
fn the_trail_is_of_the_patterns_own_colours_not_the_graded_frame() {
    // gamma after the trail: the output is gamma(trail), and the trail
    // memory never sees gamma — so it equals the ungraded trail run, graded
    let body = "export var f = 0\nexport function beforeRender(d) { f += 1 }\n\
        export function render(i) { rgb(i == (f - 1) % 6, 0.5 * (i == 0), 0) }";
    let graded = run(&format!("setTrail(0.8)\nsetGamma(2.2)\n{body}"), 6, 6);
    let plain = run(&format!("setTrail(0.8)\n{body}"), 6, 6);
    // gamma's table as the engine builds it, read off a graded ramp
    let ramp = "export function render(i) { rgb(i / 255, 0, 0) }";
    let raw = run(ramp, 256, 1);
    let curved = run(&format!("setGamma(2.2)\n{ramp}"), 256, 1);
    let mut lut = [0u8; 256];
    for (r, c) in raw[0].iter().zip(curved[0].iter()) {
        lut[r[0] as usize] = c[0];
    }
    for (g, p) in graded.iter().zip(plain.iter()) {
        let want: Vec<[u8; 3]> =
            p.iter().map(|c| [lut[c[0] as usize], lut[c[1] as usize], lut[c[2] as usize]]).collect();
        assert_eq!(g, &want);
    }
}

#[test]
fn render_bulk_and_render_size_trail_too() {
    // a whole-frame pattern that clears every frame still leaves a trail
    let src = "setTrail(0.5)\nexport var f = 0\nexport function beforeRender(d) { f += 1 }\n\
        export function renderBulk() { clear()\n rgb(1, 1, 1)\n setPixel(f - 1) }";
    let frames = run(src, 4, 3);
    assert_eq!(frames[2][1][0], ((255 * k(0.5)) >> 8) as u8);
    assert_eq!(frames[2][0][0], ((((255 * k(0.5)) >> 8) * k(0.5)) >> 8) as u8);
    // under setRenderSize the stage runs on the small frame: a resized
    // trail on 8x8 is the native 4x4 trail block-expanded
    let body = "setTrail(0.6)\nexport var f = 0\nexport function beforeRender(d) { f += 1 }\n\
        export function render2D(i, x, y) { hsv(x, 1, i == f % 16) }";
    let mut small = Engine::new(body, 16, 1).unwrap();
    small.set_grid_map(4, 4);
    let mut big = Engine::new(&format!("setRenderSize(4, 4)\n{body}"), 64, 1).unwrap();
    big.set_grid_map(8, 8);
    for _ in 0..5 {
        let s = small.frame(Fx::from_int(16)).to_vec();
        let b = big.frame(Fx::from_int(16)).to_vec();
        for r in 0..8 {
            for c in 0..8 {
                assert_eq!(b[r * 8 + c], s[(r / 2) * 4 + c / 2]);
            }
        }
    }
}
