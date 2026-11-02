# Scenes that fit any panel — the design size `U` (Gitea #997, "G3")

Status: **decided and built** (Jeremy approved 2026-11-02). The engine,
codecs and spec shipped first; the console (Layout chooser, marquee, preview
size) and the re-authoring of the shipped scenes follow as their own PRs
under #997. The normative text is `docs/spec/scenes.md` §2 "Design size";
this file keeps the problem and the reasoning.

## 1. Problem

A box is `L <type> x y w h …` in **layout pixels**, and `w`/`h` 0 was the only
thing that adapted to the panel. Glyphs are fixed PSF2 cells (largest 5×8)
and a natural-size sprite is drawn 1:1. The library rule "authored for 64×64,
every placed box inside 32×32" therefore put every composition in the
top-left 32×32 of a bigger panel:

| class | example scenes | at 128×128 | at 32×32 |
|---|---|---|---|
| full-layout only (`0 0 0 0`) | fireflies-at-dusk, tidepool | fine | fine |
| absolute text + 8-row glyphs | fire-clock, analog-date, neon-sign (19 in all) | text is a sliver in the top ~20 rows | fine |
| boxes / natural sprites in the 32 corner | campfire, lighthouse, heartbeat (15) | top-left quarter or sixteenth | fine, the "64 extras" off-panel |
| authored at 64 | aquarium, bouncing-logo, sprite-parade (6) | top-left quarter | partly off-panel |
| explicit 64×64 tile/fill box | spark-field, vignette-tint (4) | quarter-panel texture | top-left quarter of it |

Concrete breaks: **fire-clock** (#1011) put its digits on rows 10–17, which
on 128×128 the bottom-rooted flames never reach (55 lit pixels in the soak).
**lighthouse**'s radar sweeps about the PANEL centre while the lamp sat at
(16,16). **analog-date**'s caption sat below the hub only on 32×32.
**vignette-tint** covered one quarter of a 128 panel.

## 2. The wire

```
S - Fire Clock
U 32 32                       ← coordinates are in a 32×32 design space
L text 0 16 0 0 normal 100 none fill 1
T clock HH:MM
F large 5a1404 c none 0
L pat 0 0 0 0 lighten 100 none fill 1
I @pat/perlin-fire
X clip
```

* `U <w> <h>` — scene-level, between the `S` and the first `L`, two integers
  1..=4096. Boxes and `M` positions below it are design units, mapped onto the
  grid once at `set_scene` / `set_grid`. Natural-size sprites, tile cells,
  glyphs and scroll speed are magnified by an **integer** zoom.
* `X abs` — a new word on the existing `X` line: this ONE layer keeps grid
  pixels and 1:1 content. Canonical form is one `X` line, `clip` before
  `abs` (`X clip abs`).
* JSON: `"space":[32,32]` after `"name"` (absent when unset), `"abs":true`
  right after `"clip"`.
* Cost: `U 32 32\n` is 8 B per scene and `X abs\n` 6 B per pinned layer,
  inside the 3,840 B store. Integers throughout, no floats.

**Safe on older firmware.** Since the first scene commit (30938c29,
2026-09-24) both parsers ignore a binding line that arrives before the first
`L`, and `X` ignores unknown words, so an older host draws a `U` scene at its
design numbers 1:1 — exactly the picture it had before. (The spec used to say
such a line was an error; the code never did, and the spec is fixed.)

## 3. Semantics

Inputs: the design size `(rw, rh)` and the canvas `(gw, gh)` (`n`×1 on a
strip, #985).

```
no U line, or the layer has X abs  →  identity: today's code path, untouched
strip regime  (rh == 1 || gh == 1):  f = gw/rw (rational), ox = 0; y and h are not mapped
panel regime:
   z0 = min(gw / rw, gh / rh)                      integer division
   z0 ≥ 1 → f = z0                                 integer zoom
   z0 = 0 → f = min(gw/rw, gh/rh) (rational, by cross-multiplying)
   ox = (gw − ⌊rw·f⌋) / 2,  oy = (gh − ⌊rh·f⌋) / 2   centre the design area
z = max(1, ⌊f⌋)    zooms: natural sprites (sw·z × sh·z), tile cells, glyph cells and
                   advance, line height, scroll speed (design px/s × z)
map(v) = ⌊v·f⌋     i64, floor division (monotone for negative v)
placed axis (n > 0): a = o + map(v),  b = o + map(v + n),  size = max(1, b − a)
zero axis   (n = 0) that means the WHOLE LAYOUT (pattern, colour, text, tile sprite):
                     origin = map(v) with NO centring offset, size 0
natural-size sprite (fill/contain with w or h 0): origin = o + map(v) on both
                     axes — its zero axis is the sprite's size, not the layout
M key x/y:  mapped exactly like the L origin on that axis, before interpolation;
            `=` stays `=` (the mapped L origin); a keyed layer keeps its L box's
            mapped size
```

* **Edges are mapped, not sizes**, so adjacent design boxes stay adjacent with
  no gap and no overlap at any `f`.
* **`max(1, …)` matters**: a resolved size of 0 would flip a box to "whole
  layout".
* **Below the design size** (`z0` = 0): positions and box-fit content shrink,
  pixels never do — natural sprites and glyphs stay 1:1.
* **Mixed scenes**: absolute and design-unit layers mix freely through `X abs`.
* A zero axis is scaled but not offset, so on a panel letterboxed VERTICALLY a
  zero-`h` text box at design `y` 16 sits 16·f from the panel top rather than
  the design area's; an author who wants it inside the area gives the box an `h`.
* **Backwards compatibility**: a scene without `U` early-returns
  (`Place::IDENTITY`) and renders byte-identically, proven by #985's
  `crates/luxel-core/tests/library_scenes.rs` digests at 64×64, 32×32, 128×128
  and 150×1 on master and on the branch.

Implementation as built: `Scene.space: Option<(u16,u16)>`, `Layer.abs: bool`;
`compose::Place { x: Axis, y: Axis, zoom }` resolved in `Compositor::set_scene`
and re-resolved by `set_grid`; each layer's drawn style is its `L` style mapped
once, and a keyed layer maps its two current keys per frame before the lerp
(no mapped copy of the track is stored). The zoom reaches `blit_sprite_with`
and `draw_text_with` as an argument; `text::draw_scaled` makes each ink bit a
z×z block. `Compositor::layer_rect` / `layer_zoom` (wasm
`lx_comp_layer_rect → [x, y, w, h, zoom]`) give the console the resolved box —
also the fix #996 item 1 needs.

### Worked examples

**A. fire-clock** (`U 32 32`, `L text 0 16 0 0`, large "12:34", 30 px advance, `c`):

| canvas | f, z | box | text anchor | glyph rows |
|---|---|---|---|---|
| 32×32 | 1, 1 | y16, full width | x1 | 16–23 |
| 64×64 | 2, 2 | y32 | x2 | 32–47 |
| 128×128 | 4, 4 | y64 | x4 | 64–95: the hot lower-middle (today 10–17) |
| 150×1 | strip | text draws nothing (#985), so the clip fire draws nothing | | |

**B. campfire** (`U 32 32`: embers box `8 0 16 28`, fire sprite natural 16×14 at `8 17`, ground `0 30 0 0`):

| canvas | embers box | fire sprite | ground |
|---|---|---|---|
| 32×32 | [8,24)×[0,28) | (8,17) 16×14 | rows 30+ |
| 64×64 | [16,48)×[0,56) | (16,34) 32×28 | rows 60+ |
| 128×128 | [32,96)×[0,112) | (32,68) 64×56 | rows 120+ |
| 64×32 | z0=1, ox=16: [24,40)×[0,28) | (24,17) 16×14 | rows 30+, full width (zero axis) |

**C. neon-sign glow at 128** (`U 32 32`, `L text -1 9 0 0`): x is a zero axis,
so its origin is map(−1) = −4 and the box is the real width; the glyphs are 4×
and centre-aligned, so the glow copy sits one scaled pixel (4 px) left of the tube.

**D. strip zones** (`U 150 1`, `L pat 0 0 75 0` and `L pat 75 0 75 0 … 3`):

| strip | f | zone 1 | zone 2 |
|---|---|---|---|
| 150 | 1 | [0,75) | [75,150) |
| 300 | 2 | [0,150) | [150,300) |
| 61 | 61/150 | [0,30) | [30,61), gapless |

**E. sunrise keyframes at 128** (`U 32 32`): the box `4 3 22 22 contain` maps to
(16,12) 88×88 and `contain` fits the 11×11 sun at 8×. `M 0 0 24 0 linear` maps
to (0,96) and `M 20000 8 24 0 in` to (32,96); the rise is interpolated in grid
pixels.

**F. silent-night snow, `X abs`**: `L sprite 0 0 0 0 … tile 1` + `X abs` — the
16×16 snowfall tile repeats 1:1 over the whole 128×128, so the flakes stay fine;
the tree beside it (`4 6`, 24×26) maps to (16,24) 96×104.

## 4. Why not percent fields, anchors or a 0..1 space

* **Percent fields** (`L text 0 31% 0 0`) put a new token INSIDE the `L` line,
  which an older host's `parse_int` rejects; `firmware/src/scenes.rs` then logs
  "stored blob rejected" and starts with no scenes, and the next POST overwrites
  the blob. An OTA rollback to the other slot — which the boot-loop guard can
  trigger on its own — would therefore wipe every stored scene. That rules out
  every L-line encoding. Percent steps are also 1.28 px at 128 and 3 px on a
  300 px strip, they do nothing for glyph or sprite size (fire-clock's digits
  would move but stay 8 rows tall), and Rect and Keyframe would need per-field
  flag bits.
* **Anchors** (`c`, `b`, `tr`) centre a fixed-size composition but do not scale
  it: on 128×128 everything is still tiny, just in the middle. They are worth
  having only for slack on non-square panels, so they are a follow-up ticket.
* **`U rel`, 0..1 fixed point per axis**: unreadable numbers
  (`L text 0 20480 0 0`), each axis scaled on its own distorts the aspect on
  64×32, and pixel art gets no integer zoom.
* **One integer for everything on the panel** instead: boxes, sprites and
  glyphs scale together, so a fire box stays over its fire sprite and a scrim
  behind its text, and a composition that fits its design size fits every
  integer multiple of it. Every shipped scene's numbers are already a valid
  design space (its authoring size), so converting one is mostly one line.

## 5. Follow-ups (separate PRs / tickets)

* Console (#997 PR 2): a "Layout" chooser (Pixels / Fit 32×32 / Fit 64×64 /
  Fit this panel) in the scene editor header, "Pin to pixels" (`X abs`) per
  layer, BoxRow in design units with a `×z on this panel` hint, the marquee
  drawn from `lx_comp_layer_rect` and drags snapped to design cells, a
  "Preview at" size select; mock frame S7j first.
* Library batch (#997 PR 3): `U 32 32` on 26 scenes, `U 64 64` on 8, three left
  absolute on purpose, four unchanged; the gate checks every placed non-`abs`
  box lies inside the design area.
* Tickets: per-layer anchors for non-square slack; the rest of G4 (`fit
  contain` on text, pattern `fontScale(n)`, a big-digit face, `°`); G8 text
  sources; G14 text outline/shadow/glow.
