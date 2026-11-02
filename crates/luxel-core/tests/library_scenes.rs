//! Every shipped scene (`library/scenes/*.scene`) through the real
//! compositor, on a 64x64 panel and on a 150 px strip (Gitea #985).
//!
//! The scenes reference library patterns and sprites by slug. Patterns are
//! stood in for by a deterministic synthetic frame per layer (this suite is
//! about the COMPOSITOR, and compiling 41 scenes' worth of 2D patterns would
//! only make it slow), while sprites are the real art: `library/sprites/
//! <slug>.sprite` read through a small Rust copy of the text grammar in
//! library/README.md and encoded as the `LXSP` record the device stores.
//!
//! Two things are pinned:
//!
//! * every scene parses and composites on both shapes without a panic, and
//! * on a strip a text layer contributes NOTHING (no glyph fits one row,
//!   docs/spec/scenes.md §2) — the scene draws exactly what it draws with
//!   its text layers hidden.
//!
//! `LUXEL_SCENE_DIGESTS=1 cargo test -p luxel-core --test library_scenes --
//! --nocapture` also prints an FNV-1a digest of each scene's frames on a
//! 64x64, a 32x32 and a 128x128 panel and on a 150 px strip. That is how
//! #985 showed its `w = 0` rule for tiled sprites leaves every shipped scene
//! byte-identical (the same file, run against `origin/master` and against
//! the branch, printed the same lines), and how #997 showed the same for
//! its `U` design size on every scene that does not declare one.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use luxel_core::arena::FrameVec;
use luxel_core::compose::{Compositor, SceneDriver, SceneHost, SpriteView};
use luxel_core::fixed::Fx;
use luxel_core::outpipe::GridMap;
use luxel_core::scene::{self, LayerBody, LayerKind, Scene, TextSource};

/// Frame deltas in raw 16.16 ms: a first frame, 60 fps, and some long steps
/// so keyframe tracks, sprite clocks and scrolls all move.
const DELTAS: &[i32] = &[
    0,
    1_092_267,
    250 << 16,
    1000 << 16,
    3333 << 16,
    7 << 16,
    1_092_267,
];

fn library() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../library")
}

fn fnv(bytes: &[u8], mut h: u64) -> u64 {
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// The `.sprite` text grammar (library/README.md "Sprites"), just enough of
/// it to turn shipped art into an `LXSP` record: `name:`, `fps:`, a
/// `palette:` of `<char> <rrggbb>` lines, then `frame` blocks of rows where
/// `.` is index 0. The real parser (and its error reporting) is
/// `parseSpriteSource` in the web tree; the shipped files are already
/// gated through it by `check-sprite-scene-library.mjs`.
fn sprite_record(src: &str) -> Vec<u8> {
    let (mut name, mut fps) = (String::new(), 0u8);
    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut codes: HashMap<char, u8> = HashMap::new();
    let mut frames: Vec<Vec<String>> = Vec::new();
    let mut in_palette = false;
    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "frame" {
            frames.push(Vec::new());
            continue;
        }
        if let Some(rows) = frames.last_mut() {
            rows.push(line.to_string());
            continue;
        }
        if line == "palette:" {
            in_palette = true;
            continue;
        }
        if in_palette {
            let mut it = line.split_whitespace();
            let ch = it.next().unwrap().chars().next().unwrap();
            let hex = it.next().unwrap();
            let v = u32::from_str_radix(hex, 16).unwrap();
            palette.push([(v >> 16) as u8, (v >> 8) as u8, v as u8]);
            codes.insert(ch, palette.len() as u8);
            continue;
        }
        let (k, v) = line.split_once(':').unwrap();
        match k.trim() {
            "name" => name = v.trim().to_string(),
            "fps" => fps = v.trim().parse().unwrap(),
            other => panic!("unknown sprite header {other}"),
        }
    }
    let (h, w) = (frames[0].len(), frames[0][0].chars().count());
    let mut rec = Vec::new();
    rec.extend_from_slice(b"LXSP");
    rec.push(1);
    rec.extend_from_slice(&[
        w as u8,
        h as u8,
        frames.len() as u8,
        fps,
        palette.len() as u8,
        name.len() as u8,
        0,
    ]);
    rec.extend_from_slice(name.as_bytes());
    for c in &palette {
        rec.extend_from_slice(c);
    }
    for f in &frames {
        for row in f {
            for ch in row.chars() {
                rec.push(if ch == '.' { 0 } else { codes[&ch] });
            }
        }
    }
    assert!(
        SpriteView::parse(&rec).is_some(),
        "sprite {name} encodes to a valid record"
    );
    rec
}

/// One shipped scene, its `@pat/` / `@spr/` references rewritten to stable
/// 8-hex ids (the generator's job — any distinct ids will do here), plus the
/// sprite record each sprite id names.
struct Shipped {
    slug: String,
    scene: Scene,
    sprites: HashMap<String, Vec<u8>>,
}

fn shipped() -> Vec<Shipped> {
    let dir = library().join("scenes");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("library/scenes")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "scene"))
        .collect();
    paths.sort();
    assert!(
        paths.len() >= 10,
        "found the shipped scenes ({})",
        paths.len()
    );
    paths
        .iter()
        .map(|p| {
            let slug = p.file_stem().unwrap().to_string_lossy().into_owned();
            let src = std::fs::read_to_string(p).unwrap();
            let mut sprites = HashMap::new();
            let mut wire = String::new();
            for line in src.lines() {
                if line.starts_with('#') {
                    continue;
                }
                if let Some(r) = line.strip_prefix("I @") {
                    let id = format!("{:08x}", fnv(r.as_bytes(), 0xcbf2_9ce4_8422_2325) as u32);
                    if let Some(spr) = r.strip_prefix("spr/") {
                        let path = library().join("sprites").join(format!("{spr}.sprite"));
                        let text =
                            std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("{path:?}"));
                        sprites.insert(id.clone(), sprite_record(&text));
                    }
                    wire.push_str(&format!("I {id}\n"));
                } else {
                    wire.push_str(line);
                    wire.push('\n');
                }
            }
            let scene = scene::parse(&wire).unwrap_or_else(|e| panic!("{slug}: {e}"));
            Shipped {
                slug,
                scene,
                sprites,
            }
        })
        .collect()
}

/// Synthetic pattern frames, real sprites, a fixed caption.
struct Host<'a> {
    shipped: &'a Shipped,
    n: usize,
    step: u32,
    frame: Vec<[u8; 3]>,
}

impl SceneHost for Host<'_> {
    fn pattern_frame(&mut self, layer: usize, _delta: Fx) -> Option<&[[u8; 3]]> {
        self.step += 1;
        let (l, s) = (layer as u32, self.step);
        self.frame.clear();
        for i in 0..self.n as u32 {
            self.frame.push([
                (i.wrapping_mul(7) ^ l.wrapping_mul(31)).wrapping_add(s * 13) as u8,
                (i / 3 + l * 50) as u8,
                (i.wrapping_mul(i) >> 4).wrapping_add(s) as u8,
            ]);
        }
        Some(&self.frame)
    }
    fn sprite(&mut self, layer: usize) -> Option<SpriteView<'_>> {
        match &self.shipped.scene.layers.get(layer)?.body {
            LayerBody::Sprite { id, .. } => SpriteView::parse(self.shipped.sprites.get(id)?),
            _ => None,
        }
    }
    fn text(&mut self, _layer: usize, _source: &TextSource) -> Option<&str> {
        Some("12:34")
    }
}

/// Every frame of `DELTAS`, concatenated.
fn render(shipped: &Shipped, scene: &Scene, grid: GridMap) -> Vec<[u8; 3]> {
    let n = grid.len();
    let mut comp = Compositor::new(grid);
    comp.set_scene(scene);
    let mut driver = SceneDriver::new();
    let mut dst: FrameVec = luxel_core::arena::empty();
    let mut host = Host {
        shipped,
        n,
        step: 0,
        frame: Vec::new(),
    };
    let mut out = Vec::new();
    for &d in DELTAS {
        assert!(driver.frame(&mut comp, &mut dst, n, Fx::from_raw(d), &mut host));
        out.extend_from_slice(&dst);
    }
    out
}

#[test]
fn every_shipped_scene_composites_on_a_panel() {
    let digests = std::env::var_os("LUXEL_SCENE_DIGESTS").is_some();
    for s in shipped() {
        // 64x64 is what the library is authored and gated on; 32 and 128
        // are the two other panel sizes on the bench (Gitea #997)
        for (w, h) in [(64u16, 64u16), (32, 32), (128, 128)] {
            let frames = render(&s, &s.scene, GridMap::new(w, h, false));
            assert_eq!(frames.len(), w as usize * h as usize * DELTAS.len());
            assert!(
                frames.iter().any(|p| *p != [0, 0, 0]),
                "{} draws something on {w}x{h}",
                s.slug
            );
            if digests {
                // the 64x64 line keeps its pre-#997 spelling so an older
                // run's output still diffs against this one
                let size = if w == 64 {
                    String::new()
                } else {
                    format!(" {w}x{h}")
                };
                println!(
                    "digest {}{size} {:016x}",
                    s.slug,
                    fnv(frames.as_flattened(), 0xcbf2_9ce4_8422_2325)
                );
            }
        }
    }
}

#[test]
fn on_a_strip_text_layers_draw_nothing() {
    let digests = std::env::var_os("LUXEL_SCENE_DIGESTS").is_some();
    let strip = GridMap::new(150, 1, false);
    for s in shipped() {
        let frames = render(&s, &s.scene, strip);
        assert_eq!(frames.len(), 150 * DELTAS.len());
        if digests {
            println!(
                "digest {} 150x1 {:016x}",
                s.slug,
                fnv(frames.as_flattened(), 0xcbf2_9ce4_8422_2325)
            );
        }
        if !s.scene.layers.iter().any(|l| l.kind() == LayerKind::Text) {
            continue;
        }
        let mut hidden = s.scene.clone();
        for l in hidden
            .layers
            .iter_mut()
            .filter(|l| l.kind() == LayerKind::Text)
        {
            l.style.visible = false;
        }
        assert!(
            frames == render(&s, &hidden, strip),
            "{}: a text layer drew on a one-row grid",
            s.slug
        );
    }
}
