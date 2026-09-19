//! `outpipe::DeviceChain` is byte-identical to the firmware's old
//! `apply_outpipe` (Gitea #466).
//!
//! The device output chain lived in `firmware/src/main.rs` — a `no_std` ESP
//! binary, so nothing about it was testable on a host and the playground could
//! not run it at all (previews diverged from the device by the whole Settings
//! chain; research/engine-constraints.md §8v). Lifting it into `luxel-core`
//! makes both true, and this file is the proof that the lift changed nothing:
//! `legacy_apply_outpipe` below is the pre-#466 body **copied verbatim** from
//! `main.rs`, with only the `shared::*` atomic reads replaced by parameters
//! (the firmware's globals do not exist here) and `crate::` paths adjusted.
//! Every case runs both and compares the wire frames byte for byte.
//!
//! When the chain changes, change BOTH — the legacy copy is a frozen oracle,
//! not shared code. If a future change is deliberately not byte-identical,
//! delete this file in the same commit rather than loosening the assertion.

use luxel_core::outpipe::{
    self, ChainSettings, ColorOrder, DeviceChain, GridMap, PowerModel,
};

// ---------------------------------------------------------------- the oracle

/// The pre-#466 `firmware/src/main.rs::apply_outpipe`, verbatim apart from
/// reading its settings from arguments instead of `shared::*` atomics.
#[allow(clippy::too_many_arguments)]
fn legacy_apply_outpipe<'a>(
    frame: &'a [[u8; 3]],
    pipe_buf: &'a mut Vec<[u8; 3]>,
    gamma_cache: &mut (u8, Option<Box<[u8; 256]>>),
    pal_cache: &mut (u32, Option<Box<[[u8; 3]; 256]>>),
    brightness5: u8,
    grid: Option<GridMap>,
    // ---- what the firmware read out of `shared::` ----
    order: u8,
    gamma: u8,
    cap: u32,
    blur_pct: u8,
    glow_pct: u8,
    pal_pct: u8,
    pal_epoch: u32,
    stops_src: &[(u8, [u8; 3])],
    power_model: PowerModel,
) -> &'a [[u8; 3]] {
    use luxel_core::fixed::Fx;
    let gamma_on = gamma > 0 && gamma != 10;
    let pal_on = pal_pct > 0;
    if order == 0 && !gamma_on && cap == 0 && blur_pct == 0 && glow_pct == 0 && !pal_on {
        return frame;
    }
    if gamma_on && gamma_cache.0 != gamma {
        *gamma_cache = (gamma, Some(Box::new(outpipe::gamma_lut(gamma))));
    }
    if pal_on && pal_cache.0 != pal_epoch {
        let stops = stops_src.to_vec();
        pal_cache.0 = pal_epoch;
        pal_cache.1 = if stops.is_empty() {
            None
        } else {
            let b = |v: u8| Fx::from_raw(((v as i32) << 16) / 255);
            let pal: Vec<(Fx, [Fx; 3])> = stops
                .iter()
                .map(|(p, c)| (b(*p), [b(c[0]), b(c[1]), b(c[2])]))
                .collect();
            let mut lut = Box::new([[0u8; 3]; 256]);
            outpipe::fill_palette_lut(&pal, &mut lut);
            Some(lut)
        };
    }
    pipe_buf.clear();
    pipe_buf.extend_from_slice(frame);
    if pal_on {
        if let Some(lut) = pal_cache.1.as_deref() {
            outpipe::palette_remap_frame(pipe_buf, lut, pal_pct as u32 * 256 / 100);
        }
    }
    let blur_k = blur_pct as u32 * 128 / 100;
    let glow_g = glow_pct as u32 * 256 / 100;
    match grid.filter(|g| g.len() == pipe_buf.len()) {
        Some(g) => {
            outpipe::blur_frame_grid(pipe_buf, &g, blur_k, 1);
            outpipe::glow_frame_grid(pipe_buf, &g, glow_g);
        }
        None => {
            outpipe::blur_frame(pipe_buf, blur_k, 1);
            outpipe::glow_frame(pipe_buf, glow_g);
        }
    }
    outpipe::apply(
        pipe_buf,
        ColorOrder(order),
        if gamma_on { gamma_cache.1.as_deref() } else { None },
        cap,
        brightness5,
        power_model,
    );
    pipe_buf
}

// ---------------------------------------------------------------- fixtures

/// A deterministic frame with structure the spatial stages can actually move:
/// bright specks on a dark field, so blur spreads and glow has something to
/// find. 16×8, which is a real grid and also a plausible strip.
fn frame(n: usize) -> Vec<[u8; 3]> {
    (0..n)
        .map(|i| {
            let s = (i * 2_654_435_761usize) % 1009;
            if s % 11 == 0 {
                [250, 30, 200]
            } else {
                [(s % 37) as u8, (s % 53) as u8, (s % 17) as u8]
            }
        })
        .collect()
}

const STOPS: [(u8, [u8; 3]); 3] = [(0, [0, 0, 0]), (128, [0, 200, 40]), (255, [180, 0, 255])];

/// Every setting combination worth distinguishing, each named so a failure
/// says which stage diverged.
fn cases() -> Vec<(&'static str, ChainSettings)> {
    let base = ChainSettings::default();
    vec![
        ("all off", base),
        ("order grb", ChainSettings { order: ColorOrder(2), ..base }),
        ("gamma 2.2", ChainSettings { gamma_tenths: 22, ..base }),
        ("gamma 1.0 (off by value)", ChainSettings { gamma_tenths: 10, ..base }),
        ("cap 200 mA", ChainSettings { cap_ma: 200, ..base }),
        ("cap far above draw", ChainSettings { cap_ma: 20_000, ..base }),
        ("blur 50", ChainSettings { blur_pct: 50, ..base }),
        ("blur 100", ChainSettings { blur_pct: 100, ..base }),
        ("glow 40", ChainSettings { glow_pct: 40, ..base }),
        ("blur+glow", ChainSettings { blur_pct: 50, glow_pct: 40, ..base }),
        ("palette 100", ChainSettings { palette_pct: 100, palette_epoch: 7, ..base }),
        ("palette 35", ChainSettings { palette_pct: 35, palette_epoch: 7, ..base }),
        (
            "everything",
            ChainSettings {
                order: ColorOrder(5),
                gamma_tenths: 22,
                cap_ma: 300,
                blur_pct: 60,
                glow_pct: 45,
                palette_pct: 70,
                palette_epoch: 7,
            },
        ),
    ]
}

fn run_both(
    px: &[[u8; 3]],
    s: &ChainSettings,
    brightness5: u8,
    grid: Option<GridMap>,
    model: PowerModel,
) -> (Vec<[u8; 3]>, Vec<[u8; 3]>) {
    let mut chain = DeviceChain::new();
    let new = chain
        .apply(px, s, brightness5, grid, model, || STOPS.to_vec())
        .to_vec();

    let mut buf = Vec::new();
    let mut g = (0u8, None);
    let mut p = (u32::MAX, None);
    let old = legacy_apply_outpipe(
        px,
        &mut buf,
        &mut g,
        &mut p,
        brightness5,
        grid,
        s.order.0,
        s.gamma_tenths,
        s.cap_ma,
        s.blur_pct,
        s.glow_pct,
        s.palette_pct,
        s.palette_epoch,
        &STOPS,
        model,
    )
    .to_vec();
    (old, new)
}

// ---------------------------------------------------------------- the tests

#[test]
fn identical_on_a_strip() {
    let px = frame(128);
    for (name, s) in cases() {
        let (old, new) = run_both(&px, &s, 31, None, PowerModel::Strip);
        assert_eq!(old, new, "strip, {name}");
    }
}

#[test]
fn identical_on_a_grid() {
    let px = frame(128);
    let grid = Some(GridMap { w: 16, h: 8, serpentine: false });
    for (name, s) in cases() {
        let (old, new) = run_both(&px, &s, 31, grid, PowerModel::Strip);
        assert_eq!(old, new, "grid, {name}");
    }
}

#[test]
fn identical_on_a_serpentine_grid_and_a_panel_power_model() {
    let px = frame(128);
    let grid = Some(GridMap { w: 16, h: 8, serpentine: true });
    for (name, s) in cases() {
        let (old, new) = run_both(&px, &s, 17, grid, PowerModel::Hub75 { scan: 4 });
        assert_eq!(old, new, "serpentine panel, {name}");
    }
}

#[test]
fn identical_when_the_grid_does_not_match_the_frame() {
    // the firmware's `grid.filter(|g| g.len() == frame.len())` guard: a stale
    // grid must fall back to index space, not index out of bounds
    let px = frame(100);
    let grid = Some(GridMap { w: 16, h: 8, serpentine: false });
    for (name, s) in cases() {
        let (old, new) = run_both(&px, &s, 31, grid, PowerModel::Strip);
        assert_eq!(old, new, "mismatched grid, {name}");
    }
}

#[test]
fn identical_at_low_brightness_where_the_cap_bites_differently() {
    let px = frame(128);
    for b5 in [0u8, 1, 7, 31] {
        for (name, s) in cases() {
            let (old, new) = run_both(&px, &s, b5, None, PowerModel::Strip);
            assert_eq!(old, new, "brightness {b5}, {name}");
        }
    }
}

#[test]
fn all_off_returns_the_input_frame_untouched() {
    let px = frame(64);
    let mut chain = DeviceChain::new();
    let out = chain.apply(
        &px,
        &ChainSettings::default(),
        31,
        None,
        PowerModel::Strip,
        || STOPS.to_vec(),
    );
    assert_eq!(out, &px[..]);
}

// ---- the #446/#476 scratch lifecycle, now host-testable ----

#[test]
fn scratch_grows_on_the_first_active_frame_and_is_released_when_all_stages_go_off() {
    let px = frame(4096); // the S3 panel's count — 3 B/px = 12,288 B
    let mut chain = DeviceChain::new();
    let off = ChainSettings::default();
    let on = ChainSettings { blur_pct: 50, ..off };

    chain.apply(&px, &off, 31, None, PowerModel::Strip, || STOPS.to_vec());
    assert_eq!(chain.resident_bytes(), 0, "an untouched chain holds nothing");

    chain.apply(&px, &on, 31, None, PowerModel::Strip, || STOPS.to_vec());
    assert_eq!(chain.resident_bytes(), 12_288, "3 B/px at 4096 px");

    chain.apply(&px, &off, 31, None, PowerModel::Strip, || STOPS.to_vec());
    assert_eq!(chain.resident_bytes(), 0, "every stage off gives the scratch back");

    // and it re-grows, with the same result as a chain that never released
    let a = chain
        .apply(&px, &on, 31, None, PowerModel::Strip, || STOPS.to_vec())
        .to_vec();
    assert_eq!(chain.resident_bytes(), 12_288);
    let mut fresh = DeviceChain::new();
    let b = fresh
        .apply(&px, &on, 31, None, PowerModel::Strip, || STOPS.to_vec())
        .to_vec();
    assert_eq!(a, b, "a re-grown scratch renders the same frame");
}

#[test]
fn the_cooked_luts_are_released_too_and_rebuilt_correctly() {
    let px = frame(64);
    let mut chain = DeviceChain::new();
    let off = ChainSettings::default();
    let gamma = ChainSettings { gamma_tenths: 22, ..off };
    let pal = ChainSettings { palette_pct: 100, palette_epoch: 3, ..off };

    let g1 = chain
        .apply(&px, &gamma, 31, None, PowerModel::Strip, || STOPS.to_vec())
        .to_vec();
    assert_eq!(chain.resident_bytes(), 64 * 3 + 256);
    let p1 = chain
        .apply(&px, &pal, 31, None, PowerModel::Strip, || STOPS.to_vec())
        .to_vec();
    // gamma's table is still held (its setting only changed, it was not
    // re-cooked) — this is the pre-#466 behaviour, kept
    assert_eq!(chain.resident_bytes(), 64 * 3 + 256 + 768);

    chain.apply(&px, &off, 31, None, PowerModel::Strip, || STOPS.to_vec());
    assert_eq!(chain.resident_bytes(), 0);

    // re-enabling must re-cook, not silently skip the stage with an empty slot
    let g2 = chain
        .apply(&px, &gamma, 31, None, PowerModel::Strip, || STOPS.to_vec())
        .to_vec();
    assert_eq!(g1, g2, "gamma re-cooked after a release");
    chain.apply(&px, &off, 31, None, PowerModel::Strip, || STOPS.to_vec());
    let p2 = chain
        .apply(&px, &pal, 31, None, PowerModel::Strip, || STOPS.to_vec())
        .to_vec();
    assert_eq!(p1, p2, "palette re-cooked after a release");
}

#[test]
fn the_stop_list_is_only_fetched_when_the_epoch_moves() {
    let px = frame(32);
    let mut chain = DeviceChain::new();
    let pal = ChainSettings { palette_pct: 100, palette_epoch: 1, ..Default::default() };
    let mut fetches = 0;
    for _ in 0..5 {
        chain.apply(&px, &pal, 31, None, PowerModel::Strip, || {
            fetches += 1;
            STOPS.to_vec()
        });
    }
    assert_eq!(fetches, 1, "an unchanged palette re-cooks nothing");

    let moved = ChainSettings { palette_epoch: 2, ..pal };
    chain.apply(&px, &moved, 31, None, PowerModel::Strip, || {
        fetches += 1;
        STOPS.to_vec()
    });
    assert_eq!(fetches, 2);

    // an empty stop list still updates the epoch, or it would re-cook forever
    let moved2 = ChainSettings { palette_epoch: 3, ..pal };
    for _ in 0..3 {
        chain.apply(&px, &moved2, 31, None, PowerModel::Strip, || {
            fetches += 1;
            Vec::new()
        });
    }
    assert_eq!(fetches, 3);
}
