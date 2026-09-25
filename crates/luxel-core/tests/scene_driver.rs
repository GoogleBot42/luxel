//! The SHARED full-frame scene driver (Gitea #732).
//!
//! `compose::SceneDriver::frame` is the one walk every host runs: the
//! firmware's render task, the `luxel serve` mirror and the wasm
//! playground. Each of them used to hand-write its own, and the copies had
//! already drifted — the playground carried the sub-millisecond remainder
//! of each frame delta and the device truncated it away, so a caption
//! crawled ~4 % slower on the panel than in the preview that was supposed
//! to be showing the panel.
//!
//! These tests pin the driver frame by frame over a delta sequence that is
//! deliberately NOT whole milliseconds (60 fps = 16.666… ms), so a host
//! that grows a walk of its own, or an "optimization" that drops the
//! accumulator, shows up here rather than on a bench board.

use luxel_core::compose::{Compositor, SceneDriver, SceneHost, SpriteView};
use luxel_core::fixed::Fx;
use luxel_core::outpipe::GridMap;
use luxel_core::scene::TextSource;

const W: u16 = 16;
const H: u16 = 8;
const N: usize = (W as usize) * (H as usize);

/// 60 fps in raw 16.16 milliseconds: 16.666… ms, the delta that exposed
/// the drift. `16` truncates, `17` overshoots; only the carried remainder
/// is right.
const DT60: i32 = 1_092_267;

fn grid() -> GridMap {
    GridMap { w: W, h: H, serpentine: false }
}

/// A scene with one of every layer kind, so the dispatch arm for each is
/// exercised: a colour wash, a pattern layer the host renders, a sprite
/// layer the host has no sprite for, and a scrolling `slot` caption the
/// HOST resolves (the compositor reads no slot table — docs/spec/scenes.md
/// §2).
const WIRE: &str = "S 0123abcd Driver\n\
L color 0 0 0 0 normal 100 none fill 1\n\
K 000020\n\
L pat 0 0 0 0 add 100 none fill 1\n\
L sprite 0 0 0 0 normal 100 black fill 1\n\
L text 0 0 0 0 normal 100 none fill 1\n\
T slot 3\n\
F tiny 00ff00 l left 90\n";

/// A host with no engines at all: the pattern layer's frame is synthesised
/// from the step number, so the test needs no compiler and no bytecode and
/// still proves the driver stepped the layer exactly once per frame.
#[derive(Default)]
struct FakeHost {
    step: u32,
    frame: Vec<[u8; 3]>,
    /// Every `(layer, delta_raw)` the driver asked a pattern frame for.
    pattern_calls: Vec<(usize, i32)>,
    /// Every layer the driver asked a sprite for.
    sprite_calls: Vec<usize>,
    /// Every `(layer, slot)` the driver asked text for.
    text_calls: Vec<(usize, u8)>,
    caption: String,
}

impl SceneHost for FakeHost {
    fn pattern_frame(&mut self, layer: usize, delta: Fx) -> Option<&[[u8; 3]]> {
        self.pattern_calls.push((layer, delta.raw()));
        self.step += 1;
        let k = self.step;
        self.frame.clear();
        for i in 0..N {
            self.frame.push([
                (k.wrapping_mul(7) % 200) as u8,
                (i as u8).wrapping_mul(3),
                0,
            ]);
        }
        Some(&self.frame)
    }

    fn sprite(&mut self, layer: usize) -> Option<SpriteView<'_>> {
        self.sprite_calls.push(layer);
        None
    }

    fn text(&mut self, layer: usize, source: &TextSource) -> Option<&str> {
        match source {
            TextSource::Slot(k) => {
                self.text_calls.push((layer, *k));
                Some(&self.caption)
            }
            // `lit` is seeded by `set_scene`; a host that pushes text in out
            // of band (the wasm binding) returns `None` for everything.
            _ => None,
        }
    }
}

fn scene() -> luxel_core::scene::Scene {
    luxel_core::scene::parse(WIRE).expect("scene parses")
}

/// FNV-1a over the composite, plus the number of pixels that are not the
/// bare colour wash — a compact but sharp signature of one frame.
fn sig(px: &[[u8; 3]]) -> (u32, u32) {
    let mut h: u32 = 0x811c_9dc5;
    let mut lit = 0;
    for p in px {
        if *p != [0, 0, 0x20] {
            lit += 1;
        }
        for b in p {
            h ^= *b as u32;
            h = h.wrapping_mul(0x0100_0193);
        }
    }
    (h, lit)
}

// ---- the millisecond accumulator ----

#[test]
fn step_ms_carries_the_sub_millisecond_remainder() {
    let mut d = SceneDriver::new();
    let seq: Vec<u32> = (0..12).map(|_| d.step_ms(Fx::from_raw(DT60))).collect();
    // 16.666… ms: the remainder lands the extra millisecond on every third
    // frame, which truncation never does.
    assert_eq!(seq, vec![16, 17, 17, 16, 17, 17, 16, 17, 17, 16, 17, 17]);

    // A second of 60 fps frames is a second of scroll, to the millisecond.
    let mut d = SceneDriver::new();
    let total: u32 = (0..60).map(|_| d.step_ms(Fx::from_raw(DT60))).sum();
    assert_eq!(total, 1000, "60 × 16.666 ms must be one second, not 960 ms");

    // What the firmware did before #732 — `(delta.raw() >> 16)` per frame —
    // for contrast. This is the bug the shared driver closed.
    let truncated: u32 = (0..60).map(|_| (DT60 >> 16) as u32).sum();
    assert_eq!(truncated, 960);
}

#[test]
fn step_ms_is_monotone_and_never_panics() {
    let mut d = SceneDriver::new();
    // a negative delta (a clock that went backwards) contributes nothing
    assert_eq!(d.step_ms(Fx::from_raw(-5_000_000)), 0);
    assert_eq!(d.step_ms(Fx::from_raw(65_536)), 1);
    // and a wildly long frame saturates instead of wrapping
    assert!(d.step_ms(Fx::from_raw(i32::MAX)) > 0);
    assert_eq!(d.step_ms(Fx::from_raw(i32::MAX)), 32_767);
}

// ---- the frame-by-frame pin ----

/// Twelve frames of the scene above at 60 fps, `(fnv1a, lit_pixels)` each.
///
/// REGENERATE by running the test with `--nocapture` after an intentional
/// change; a golden that moves without one means a host walk has drifted
/// or the compositor's kernels have changed under it.
const GOLDEN: [(u32, u32); 12] = [
    (0xccd1f362, 128),
    (0x6db9e2ec, 128),
    (0x00d397cc, 128),
    (0x7c1875b2, 128),
    (0x7c5ca45f, 128),
    (0x4b83e62f, 128),
    (0x8c0e421b, 128),
    (0xc8c75b2f, 128),
    (0x4bb403fb, 128),
    (0x17460b3b, 128),
    (0xb0c1ba3b, 128),
    (0x094df08e, 128),
];

fn run(frames: usize) -> (Vec<(u32, u32)>, FakeHost, Compositor, Vec<[u8; 3]>) {
    let mut comp = Compositor::new(grid());
    comp.set_scene(&scene());
    let mut driver = SceneDriver::new();
    let mut host = FakeHost { caption: String::from("HI"), ..FakeHost::default() };
    let mut dst: Vec<[u8; 3]> = Vec::new();
    let mut out = Vec::new();
    for _ in 0..frames {
        assert!(driver.frame(&mut comp, &mut dst, N, Fx::from_raw(DT60), &mut host));
        assert_eq!(dst.len(), N);
        out.push(sig(&dst));
    }
    (out, host, comp, dst)
}

#[test]
fn the_shared_driver_output_is_pinned() {
    let (sigs, host, _, _) = run(GOLDEN.len());
    for (i, (got, want)) in sigs.iter().zip(GOLDEN.iter()).enumerate() {
        assert_eq!(
            got, want,
            "frame {i}: driver output moved (got {:#010x}/{}, want {:#010x}/{})",
            got.0, got.1, want.0, want.1
        );
    }
    // the walk is bottom → top, once per layer per frame, and it asked the
    // host for exactly the things only the host knows
    assert_eq!(host.pattern_calls.len(), GOLDEN.len());
    assert!(host.pattern_calls.iter().all(|&(l, d)| l == 1 && d == DT60));
    assert_eq!(host.sprite_calls, vec![2; GOLDEN.len()]);
    assert_eq!(host.text_calls, vec![(3, 3); GOLDEN.len()]);
}

#[test]
fn the_scroll_clock_runs_on_the_carried_milliseconds() {
    // 90 px/s over a second: with the remainder carried the scroll phase is
    // 90,000 milli-pixels, and the truncating walk reached only 86,400.
    // Drive a second of 60 fps and prove the frames differ from a run that
    // truncated every step.
    let (carried, ..) = run(60);
    let mut comp = Compositor::new(grid());
    comp.set_scene(&scene());
    let mut host = FakeHost { caption: String::from("HI"), ..FakeHost::default() };
    let mut dst: Vec<[u8; 3]> = Vec::new();
    let mut truncated = Vec::new();
    // the pre-#732 firmware walk, by hand
    for _ in 0..60 {
        comp.advance((DT60 >> 16) as u32);
        for i in 0..comp.layer_count() {
            if let Some(src) = comp.text_source(i) {
                if let Some(s) = host.text(i, &src.clone()) {
                    let s = s.to_string();
                    comp.set_text(i, &s);
                }
            }
        }
        dst.clear();
        dst.resize(N, [0, 0, 0]);
        for i in 0..comp.layer_count() {
            match comp.layer_kind(i) {
                Some(luxel_core::scene::LayerKind::Pattern) => {
                    let f = host.pattern_frame(i, Fx::from_raw(DT60)).unwrap().to_vec();
                    comp.pattern_layer(&mut dst, i, &f);
                }
                _ => comp.native_layer(&mut dst, i, None),
            }
        }
        truncated.push(sig(&dst));
    }
    assert_ne!(
        carried, truncated,
        "the accumulator must change what the panel shows — otherwise this \
         test proves nothing about the drift it closed"
    );
}

// ---- the fallible destination sizing (#702 / #728) ----

#[test]
fn a_destination_it_cannot_size_is_a_frame_not_drawn() {
    let mut comp = Compositor::new(grid());
    comp.set_scene(&scene());
    let mut driver = SceneDriver::new();
    let mut host = FakeHost { caption: String::from("HI"), ..FakeHost::default() };
    let mut dst: Vec<[u8; 3]> = Vec::new();
    // No allocator on earth satisfies this. The driver must REFUSE rather
    // than panic: on a board whose largest free block is a few kilobytes
    // the host's staging buffer is exactly the allocation that fails, and
    // a frame this device cannot afford is a frame not drawn, not a reboot.
    assert!(!driver.frame(&mut comp, &mut dst, isize::MAX as usize, Fx::from_raw(DT60), &mut host));
    assert!(dst.is_empty());
    // …and nothing was drawn, so no engine was stepped
    assert!(host.pattern_calls.is_empty());
    // the clocks still advanced, exactly as both hosts did before: a frame
    // that cannot be drawn is still a frame of elapsed time
    assert!(host.text_calls.len() == 1);

    // the next affordable frame draws normally
    assert!(driver.frame(&mut comp, &mut dst, N, Fx::from_raw(DT60), &mut host));
    assert_eq!(dst.len(), N);
}

#[test]
fn the_destination_is_sized_once_and_reused() {
    let (_, _, _, dst) = run(8);
    // one `try_reserve_exact` per activation, a compare per frame after it
    assert!(dst.capacity() >= N);
    assert_eq!(dst.len(), N);
}

// ---- #733: the scroll phase survives a re-install, through the driver ----

#[test]
fn re_setting_the_same_scene_keeps_the_scroll_phase() {
    let mut comp = Compositor::new(grid());
    comp.set_scene(&scene());
    let mut driver = SceneDriver::new();
    let mut host = FakeHost { caption: String::from("HI"), ..FakeHost::default() };
    let mut dst: Vec<[u8; 3]> = Vec::new();
    let mut sigs = Vec::new();
    for i in 0..GOLDEN.len() {
        // the scene editor rebuilds the wire on every keystroke; the crawl
        // must not restart (Gitea #733)
        if i == 5 {
            comp.set_scene(&scene());
        }
        assert!(driver.frame(&mut comp, &mut dst, N, Fx::from_raw(DT60), &mut host));
        sigs.push(sig(&dst));
    }
    assert_eq!(sigs, GOLDEN.to_vec(), "a re-install restarted the scroll");
}
