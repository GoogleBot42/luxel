//! Host micro-benchmark for the scene compositor's per-frame kernels
//! (`luxel_core::compose`, Gitea #705).
//!
//! The panel is the truth — an x86 number only says whether a change moved
//! the right way, and the device's costs (a flash-resident call, a 64-bit
//! multiply libcall, an instruction cache) have no host analogue. What this
//! pins down is the SHAPE of the cost: which of the compositor's cases walk
//! a pixel at a time and which collapse to a row copy.
//!
//!   cargo test -p luxel-cli --release --test composebench -- --nocapture
//!
//! Every case is measured twice: once through the shipping kernel and once
//! through `naive_*` below, which is `compose.rs` EXACTLY as it stood before
//! #705 (two `GridMap::index` calls, a per-pixel `key_alpha` division, a
//! 64-bit alpha multiply and an out-of-line `blend_px_mode` call per pixel).
//! So the "before" column stays measurable after the change has landed.
//!
//! `CB_PIXELS` (default 4096 = the Seengreat 64x64 panel), `CB_FRAMES`
//! (default 200) and `CB_GRID` (`WxH`) select the rig. Always `--release`:
//! a debug build measures the bounds checks, not the kernel.

use std::hint::black_box;
use std::time::Instant;

use luxel_core::compose::{blend_px_mode, composite_frame, fill_color, Canvas, Compositor};
use luxel_core::outpipe::{luma, GridMap};
use luxel_core::scene::{Blend, Key, LayerStyle, Rect};

const ONE: i32 = 65536;

fn env_usize(k: &str, default: usize) -> usize {
    std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// A deterministic frame with every interesting channel value in it (0 and
/// 255 drive the key and the saturating arms).
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

// ---- the pre-#705 kernels, kept as the "before" column ----

fn resolved_rect(style: &LayerStyle, grid: &GridMap) -> (i32, i32, i32, i32) {
    let w = if style.rect.w == 0 { grid.w as i32 } else { style.rect.w as i32 };
    let h = if style.rect.h == 0 { grid.h as i32 } else { style.rect.h as i32 };
    (style.rect.x as i32, style.rect.y as i32, w, h)
}

fn mirrors(style: &LayerStyle) -> (bool, bool) {
    (style.flipx ^ style.rot180, style.flipy ^ style.rot180)
}

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

fn naive_composite(px: &mut [[u8; 3]], grid: &GridMap, src: &[[u8; 3]], style: &LayerStyle) {
    let (bx, by, bw, bh) = resolved_rect(style, grid);
    let (mx, my) = mirrors(style);
    let base = style.opacity.min(100) as i32 * ONE / 100;
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
            if let Some(d) = px.get_mut(grid.index(dr as usize, dc as usize)) {
                blend_px_mode(d, s, style.blend, a);
            }
        }
    }
}

fn naive_fill(px: &mut [[u8; 3]], grid: &GridMap, rgb: [u8; 3], style: &LayerStyle) {
    let (bx, by, bw, bh) = resolved_rect(style, grid);
    let a = style.opacity.min(100) as i32 * ONE / 100;
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
            if let Some(d) = px.get_mut(grid.index(dr as usize, dc as usize)) {
                blend_px_mode(d, rgb, style.blend, a);
            }
        }
    }
}

/// Best-of-5 wall time per frame, in microseconds. Best rather than mean
/// for the reason .claude/rules/vm-bytecode.md gives: host throughput noise
/// only ever costs time, so the maximum throughput is the least biased
/// estimator.
fn time_us(frames: usize, mut run: impl FnMut()) -> f64 {
    let mut best = f64::MAX;
    for _ in 0..5 {
        let t0 = Instant::now();
        for _ in 0..frames {
            run();
        }
        let us = t0.elapsed().as_secs_f64() * 1e6 / frames as f64;
        if us < best {
            best = us;
        }
    }
    best
}

#[test]
fn compose_bench() {
    let want = env_usize("CB_PIXELS", 4096);
    let frames = env_usize("CB_FRAMES", 200);
    let (w, h) = match std::env::var("CB_GRID").ok().and_then(|s| {
        let (a, b) = s.split_once('x')?;
        Some((a.parse::<u16>().ok()?, b.parse::<u16>().ok()?))
    }) {
        Some(g) => g,
        None => {
            let side = (1..=1024u16)
                .find(|s| (*s as usize) * (*s as usize) >= want)
                .unwrap_or(64);
            (side, side)
        }
    };
    let grid = GridMap { w, h, serpentine: true };
    let n = grid.len();
    let src = frame(n, 0x5eed_0001);
    let mut dst = frame(n, 0xc0ff_ee01);

    println!();
    println!("compose bench — {w}x{h} = {n} px, serpentine, best of 5 x {frames} frames");
    println!();
    println!("| case | before us | after us | after ns/px | speedup |");
    println!("|---|---:|---:|---:|---:|");
    let row = |name: &str, before: f64, after: f64| {
        println!(
            "| {name} | {before:.1} | {after:.1} | {:.2} | {:.1}x |",
            after * 1000.0 / n as f64,
            before / after.max(1e-9)
        );
    };

    // (a) the base layer of almost every scene: full layout, normal, 100 %,
    // no key, no flips — the case that replaced a bare `emit!`.
    let plain = LayerStyle::default();
    row(
        "full layout / normal / 100 % / no key",
        time_us(frames, || {
            naive_composite(black_box(&mut dst), &grid, black_box(&src), &plain)
        }),
        time_us(frames, || {
            composite_frame(
                Canvas { px: black_box(&mut dst), grid: &grid },
                black_box(&src),
                &plain,
            )
        }),
    );

    // (b) a clipped box with add + a luma key — the general path, with
    // every per-pixel term live.
    let keyed = LayerStyle {
        rect: Rect { x: 3, y: 5, w: w - 9, h: h - 7 },
        blend: Blend::Add,
        key: Key::Luma,
        opacity: 70,
        ..LayerStyle::default()
    };
    row(
        "clipped box / add / luma key / 70 %",
        time_us(frames, || {
            naive_composite(black_box(&mut dst), &grid, black_box(&src), &keyed)
        }),
        time_us(frames, || {
            composite_frame(
                Canvas { px: black_box(&mut dst), grid: &grid },
                black_box(&src),
                &keyed,
            )
        }),
    );

    // full layout, but mirrored — the general path over every pixel, with
    // both runs walking backwards.
    let mirrored = LayerStyle { flipx: true, flipy: true, ..LayerStyle::default() };
    row(
        "full layout / normal / mirrored",
        time_us(frames, || {
            naive_composite(black_box(&mut dst), &grid, black_box(&src), &mirrored)
        }),
        time_us(frames, || {
            composite_frame(
                Canvas { px: black_box(&mut dst), grid: &grid },
                black_box(&src),
                &mirrored,
            )
        }),
    );

    // a full-layout black-keyed layer — what a text layer composites
    // through, and what a scene's upper pattern layer usually asks for.
    let blackkey = LayerStyle { key: Key::Black, ..LayerStyle::default() };
    row(
        "full layout / normal / black key",
        time_us(frames, || {
            naive_composite(black_box(&mut dst), &grid, black_box(&src), &blackkey)
        }),
        time_us(frames, || {
            composite_frame(
                Canvas { px: black_box(&mut dst), grid: &grid },
                black_box(&src),
                &blackkey,
            )
        }),
    );

    // a colour wash over the whole layout — the native layer scenes stack
    // under everything else.
    let wash = LayerStyle { blend: Blend::Multiply, opacity: 60, ..LayerStyle::default() };
    row(
        "colour wash / multiply / 60 %",
        time_us(frames, || {
            naive_fill(black_box(&mut dst), &grid, [9, 200, 71], &wash)
        }),
        time_us(frames, || {
            fill_color(
                Canvas { px: black_box(&mut dst), grid: &grid },
                [9, 200, 71],
                &wash,
            )
        }),
    );

    // (c) a ramped pattern layer, through the Compositor: the LUT is cooked
    // once, the frame remapped every frame. "before" is the scratch copy +
    // remap + composite the fast path replaces with remap-in-place.
    let ramped = luxel_core::scene::parse(concat!(
        "S 0000000a bench\n",
        "L pat 0 0 0 0 normal 100 none fill 1\n",
        "I 0123abcd\n",
        "R 70 0:000000 128:00ff40 255:ff00ff\n",
    ))
    .expect("ramped scene");
    let mut comp = Compositor::new(grid);
    comp.set_scene(&ramped);
    // a clipped copy of the same scene never takes the in-place path, so it
    // measures the old shape
    let ramped_box = luxel_core::scene::parse(concat!(
        "S 0000000b bench\n",
        "L pat 0 0 0 0 normal 99 none fill 1\n",
        "I 0123abcd\n",
        "R 70 0:000000 128:00ff40 255:ff00ff\n",
    ))
    .expect("ramped scene");
    let mut comp_old = Compositor::new(grid);
    comp_old.set_scene(&ramped_box);
    row(
        "ramped pattern layer (Compositor)",
        time_us(frames, || {
            comp_old.pattern_layer(black_box(&mut dst), 0, black_box(&src))
        }),
        time_us(frames, || {
            comp.pattern_layer(black_box(&mut dst), 0, black_box(&src))
        }),
    );
    println!(
        "  (compositor resident: {} B in place vs {} B with the scratch)",
        comp.resident_bytes(),
        comp_old.resident_bytes()
    );
    println!();

    // Keep the optimizer honest.
    assert!(dst.iter().any(|p| *p != [0, 0, 0]));
}
