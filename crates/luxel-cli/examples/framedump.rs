//! Frame digests of a pattern on a rig, for before/after byte-identity
//! checks across a source or engine change (Gitea #948: the renderFrame →
//! renderBulk port of 36 library patterns).
//!
//!   cargo run --release -p luxel-cli --example framedump -- <pattern.js> <rig> [frames] [delta_ms] [seed]
//!
//! `rig` is `WxH` (a procedural grid map) or `N` (a mapless strip). Prints
//! one FNV-1a digest per frame and a final one over the whole run; an error
//! on any frame is printed and ends the run. Same seed and delta on both
//! sides, so the digests are comparable by `diff`.

use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;

fn fnv(acc: &mut u64, bytes: &[u8]) {
    for &b in bytes {
        *acc ^= b as u64;
        *acc = acc.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: framedump <pattern.js> <WxH|N> [frames=12] [delta_ms=40] [seed=7]");
        std::process::exit(2);
    }
    let src = std::fs::read_to_string(&args[0]).expect("read pattern");
    let rig = &args[1];
    let frames: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(12);
    let delta: i32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(40);
    let seed: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(7);
    let (pixels, grid) = match rig.split_once('x') {
        Some((w, h)) => {
            let (w, h): (u32, u32) = (w.parse().expect("w"), h.parse().expect("h"));
            (w * h, Some((w as u16, h as u16)))
        }
        None => (rig.parse().expect("N"), None),
    };
    let mut e = match Engine::new(&src, pixels, seed) {
        Ok(e) => e,
        Err(d) => {
            println!("compile error: {}", d.message);
            std::process::exit(1);
        }
    };
    match grid {
        Some((w, h)) => e.set_grid_map(w, h),
        None => e.set_strip_layout(),
    }
    let mut all = 0xcbf2_9ce4_8422_2325u64;
    for f in 0..frames {
        let px = e.frame(Fx::from_int(delta));
        let mut d = 0xcbf2_9ce4_8422_2325u64;
        let bytes: Vec<u8> = px.iter().flat_map(|p| p.iter().copied()).collect();
        fnv(&mut d, &bytes);
        fnv(&mut all, &bytes);
        println!("frame {f:3} {d:016x}");
        if let Some(err) = e.take_error() {
            println!("error at frame {f}: {}", err.message);
            break;
        }
    }
    println!("all {all:016x}");
}
