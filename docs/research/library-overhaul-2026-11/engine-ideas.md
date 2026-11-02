# Engine & scene features that would make the library markedly better

Scope: proposals only — nothing here is implemented. Grounded in `docs/lang.md`,
`docs/bulk-render.md`, `docs/spec/scenes.md`, `library/README.md`, the five
shipped scenes, `crates/luxel-core/src/{scene,compose,bulk,engine,vm,caps}.rs`,
`firmware/src/scenes.rs`, twelve sample patterns (aurora-2d, boids-2d,
fireworks-finale, doom-fire-2d, snake-2d-v2, pixelclock,
scrolling-text-marquee-2d, rainbow-smiley, us-flag-2d, emoji-animation-2,
sound-spectromatrix-render2d, reaction-diffusion-2d), the library-wide idiom
counts below, the per-pattern audit records in `libfix/audit/`, and the full open
Gitea list (`issues.txt`/`issues2.txt` beside this file).

**Size numbers are ESTIMATES** by analogy with measured PRs (#139 device palette
+4.3–5.1 KB firmware incl. routes; #140 two map-aware kernels +2.2 KB; "every
core PR adds a few hundred bytes of luxel.wasm"; #943's inliner 2.4 KB wasm).
Every PR must read the `tools/ci.sh` OTA-margin and bundle lines. The binding
budget is the **web bundle: 13,575 B headroom**, and it is spent by BOTH
luxel.wasm (all of luxel-core: engine, bulk, compose, scene parser) AND any
console UI a feature needs. OTA slots are ~20–25 % free on 4 MB boards
(265–327 KB), so firmware size is not what binds any item below.

## Library facts that drive the ranking

| idiom (grep over `library/*.js`, 308 patterns) | patterns |
|---|---|
| hand-rolled low-res canvas (`floor(y * 15.99) * 16 + …`, `canvasGet`) | **40** |
| `array(pixelCount)` buffers (trails, accumulators, ring buffers) | **68** |
| `atan2` per pixel (polar / kaleidoscope / radial) | **33** |
| `hypot`/`dist` per pixel (sprite/ball/point loops) | **62** |
| `kaleid`/`mirror` in source | **35** |
| sound (`frequencyData`) | 21 |
| own palette (`setPalette`) / hand-rolled palette managers | 11 + utility-palettes, fast-palette-blending |
| clocks (`clockHour`/`clockMinute`) | 6 |
| own bitmap font / glyph tables | marquee, rainbow-fonts, digital-rain, … (and **0** use `drawText`) |
| renderBulk already | 36 |

The two structural facts about scenes today: (1) every scene layer property is
**static** — the only things that move are text scroll and sprite frame clocks
(`Compositor::advance`); (2) a `pat` layer renders the **whole layout** and the
box only clips it (#783), so a 16×16 box costs a 4096-px engine.

## Ranked list

| # | feature | effort | library leverage | bundle risk |
|---|---|---|---|---|
| 1 | Pattern render resolution + smooth upscale (`setRenderSize`) | M | 40 canvas patterns simplified; 4–16× on 128×128 for any ambient pattern | ~1.5–2.5 KB wasm |
| 2 | Scripted scenes: keyframed layer motion (`M` lines) + a director (`setLayer`) | M + M | every scene; turns patterns-that-are-really-animations into scenes | ~1.5 KB wasm + ~2–3 KB UI |
| 3 | Palette system: `palette` control, shipped named palettes, OKLab LUT palettes, palette blend | M | dedupes colour-variant families; kills per-pixel `mixColors` (#935) | ~1 KB wasm + ~2–3 KB UI (+ palette data) |
| 4 | Clip layers: a layer clipped to the coverage of the layer below | S–M | pattern-filled text / sprites — the classic LED-sign look | ~0.5 KB wasm + toggle |
| 5 | `setTrail(decay)` afterglow post stage (+ per-layer trail) | S | ~25 of the 68 `array(pixelCount)` patterns lose their buffers | ~0.3 KB |
| 6 | Strip-space anti-aliased bulk ops (`splatIndex`, `gradientRange`, `mirrorIndex`) | S | comets/meteors/fireworks/scanners on strips without buffers or a fake grid | ~0.6 KB |
| 7 | Symmetry: `kaleido(n)` / `mirror` coordinate folds, then domain-reduced rendering | S → M | kaleidoscope family dedupes into "any pattern, kaleidoscoped"; 2–8× when reduced | ~0.4 KB (S) |
| 8 | Stored sprites drawn from pattern code (`drawSprite(sprite("heart"), …)`) | M | games and pixel-art patterns stop carrying inline art arrays | ~0.8 KB |
| 9 | fBm octaves on `fillNoise2D/3D` | S | the noise/plasma family's 2–3 interpreted octaves go native | ~0.2 KB |
| 10 | Unblock #373/#453: `arrayMul`, `arrayAffine`, `arrayLookup` | S each | reaction-diffusion, CA, wave sims fully native → can run 32–64² | ~0.4 KB total |
| 11 | Text: integer glyph scale (`F` scale / text-layer `fit`) + `num` text source from a pattern var | S + S | clocks/scoreboards legible on 64² and 128² | ~0.3 KB + UI |
| 12 | Per-layer speed (`V <pct>`) beside #793's fps cap | S | one scene line instead of a speed slider in every pattern | ~0.1 KB + UI |
| 13 | Cached polar coordinates — **measure first** | M | 33 atan2 patterns, *if* atan2+hypot still dominates after #938 | RAM: 8 B/px |

Existing tickets this list builds on rather than re-proposes: **#783** (pattern
layer box = its canvas — prerequisite for #2 moving a pattern layer
meaningfully), **#843** (per-layer render scale — the scene half of #1),
**#793** (per-layer fps cap), **#842** (layer engines on core 0), **#373 §2 /
#453** (element-wise array ops — #10 endorses them with new evidence),
**#840** (simplex kernel cost), **#935** (`mixColors` per pixel), **#487**
(user fonts / image layers), **#746** (scene export/import), **#13** "2D Text
and Overlays", **#12** "Image Display". **#352** "Layered compositor" is
already delivered by the scene system and looks closable.

---

## 1. Pattern render resolution + smooth upscale — `setRenderSize(w, h, filter)`

**What it unlocks.** The library's most repeated idiom is "simulate on a 16×16
canvas, then sample it per pixel with `floor(y * 15.99) * 16 + floor(x * 15.99)`"
— 40 patterns. Each one hand-rolls the canvas, the clamp fudge, the read-out
and (in the better ones) a `fillCanvas`/`paintCanvas` conversion with
`normAxis` reproductions (aurora-2d spends ~60 lines on exactly this). A
pattern that could simply *declare* its logical resolution would write ordinary
`render2D`/`renderBulk` code at 16×16 (or 32×32), with `gridWidth()`=16,
`gridIndex` trivial, `fillHSV(h, s, v)` in plain row-major order, and the engine
would upscale to the fixture:

- `filter 0` = nearest (crisp pixel art: snake, tetris, breakout, falling sand,
  rainbow-smiley, frogger, stacker, life/CA).
- `filter 1` = bilinear (soft fields: doom-fire, reaction-diffusion, boids,
  curl-flow, flow-field, 2d-fireworks-fade, typing-heatmap, raindrops, ice-floes,
  slime-mold, sound-spectromatrix, starfield, perlin-simplex-noise-2d).

The second win is **performance on the 128×128 chain**: any per-pixel ambient
pattern (aurora, coral-plasma, spinning-plasma, perlin-fire, ocean, voronoi,
mandelbrot, interference, sinc) can call `setRenderSize(64, 64, 1)` and pay a
quarter of the VM time (#929 has sinc at 2–9 fps at 16384 px), and the post
chain (blur/glow) runs on the small frame too. It also gives the "is the
16×16 canvas wasting a 64×64 panel?" audit complaint (curl-flow-2d,
2d-fireworks-fade "capped at 32×32 so a 64×64 panel is upscaled blur") a
one-line fix: raise the size, nothing else changes.

It is also the missing **bilinear bulk fill**: `docs/bulk-render.md` records that
2d-fireworks-fade "cannot take a bulk fill at all" because `fillCanvas` is
nearest-only and "a bilinear mode on `fillCanvas` … is the concrete ask on
#373". The same expand kernel serves both.

**Surface.**
```js
setRenderSize(w, h, filter = 0)   // top level or beforeRender; (0, 0) = native
// optional thin slice that can ship first:
fillCanvas(hArr, sArr, vArr, w, h, filter)   // filter 1 = bilinear
paintCanvas(vArr, w, h, bArr, filter)
```
Semantics to pin (mirror #843's): `pixelCount`, `gridWidth/Height`, the map
coordinates and `pixelCoord` all describe the small grid; events keep their
normalized 0..1 coordinates (they already are); a 1D pattern (`render`) is
unaffected (or: `setRenderSize(n, 1)` = render n pixels along the strip and
stretch — useful for 300-px strips running a 60-pixel-character pattern).
Only on a grid fixture; on a sparse map the small grid is sampled at each
pixel's mapped (x, y) — exactly what `canvasGet` does today, so every map works.

**Where.** `engine.rs` (the render-space vs frame-space split already exists for
projection: "Along x" renders 64 and replicates rows — `projection.rs`); the
expand kernel in `bulk.rs` beside the canvas block-expand (`canvas_fill`'s
integer-upscale path) plus a new bilinear arm; the post chain moves before the
expand. Scenes reuse the same kernel for #843 (`S <n>`), so build #1 and #843
as one engine mechanism with two front doors. Effort **M**.

**Risk.** ~1.5–2.5 KB luxel.wasm (one kernel + engine plumbing); no web UI
needed (it is a pattern builtin; hover docs come from the builtin table).
RAM: the pattern's own frame shrinks to w·h·3; the expanded full frame is the
one the engine already owns. Determinism: the bilinear arm must be pinned
byte-exact across hosts (integer math, like `canvasGet`).

**Becomes redundant / far cheaper.** The canvas plumbing in all 40 canvas
patterns (list: 2d-canvas-example, bouncing-balls-2d, flow-field-2d,
doom-fire-2d, doom-fire, digital-rain-2d, 2d-fireworks-fade,
cyclic-cellular-automata-2d, bouncy-boxes, tetrix-2d, fractal-flower,
breakout-2d, sound-spectromatrix-render2d, spirograph-2d, slime-mold-palette,
starfield-2d, raindrops-2d, wanderers, typing-heatmap-2d, sunrise-2d,
unstable-orbits-2d, tixy, rainbow-smiley, boids-2d, curl-flow-2d,
falling-sand-2d, ice-floes-2d, lissajous-curve-tracer, orv-christmas-tree,
perlin-simplex-noise-2d, reaction-diffusion-2d, scrolling-text-marquee-2d, …).
aurora-2d's whole Cell-Size/normAxis/paintCanvas machinery collapses to
`setRenderSize(gw / cell, gh / cell, 1)` + the per-pixel body. snake-2d and
snake-2d-v2 merge back into one simple `renderBulk` at 16×16.

---

## 2. Scripted scenes — Jeremy's example

Two complementary halves. Ship (a) first: it covers most authored motion with
no engine at all, which keeps engine-free scenes (heartbeat, loading) free.

### 2a. Keyframed layer motion — declarative `M` lines

**What it unlocks.** Sprites that move (the heart drifts up, the sun rises along
an arc, a caption slides in and holds), layers that fade in and out on a cycle
(a scene that "breathes" between two patterns — the crossfading.js and
shimmer-crossfade-2d patterns, which fake a two-pattern crossfade per pixel),
a pattern window that pans. Today all of this is impossible in a scene and is
why emoji/eye/flag patterns hand-roll animation timers in `beforeRender`.

**Surface.** A binding line per keyframe on any layer (unknown tags are ignored
by older hosts, so it is forward-compatible — unlike a new flag bit or blend
name, which today's parser *rejects*: `flags must be 0..15`):
```
M <ms> <x> <y> <opacity> <ease> [<frame>]
   ms       time of this key, from scene start; the LAST key's ms is the loop period
   x y      layer box origin in layout px (box size unchanged); `=` keeps the L value
   opacity  0..100, `=` keeps
   ease     linear | step | in | out | inout | sine | back | bounce | elastic
            (the existing easing builtins, applied to the segment ending here)
   frame    optional sprite frame to hold from this key (overrides the clock)
```
Example (sunrise.scene's sun rising and setting over 20 s):
```
L sprite 4 40 22 22 normal 100 none contain 1
I @spr/sun-spin
M 0     4 40 0   linear
M 3000  4 20 100 out
M 17000 = =  100 linear
M 20000 4 40 0   in
```
**Where.** `scene.rs` (parse/serialize/json, `Layer.keys: Vec<Key>`, ≤ 16
keys/layer to bound the 3,840 B scene blob), `Compositor::advance` evaluates
each layer's track into a per-frame `LayerStyle` copy (`LayerRt` already holds
per-layer clocks: `sprite_ms`, scroll phase), `composite_frame`/`blit_sprite`
already take a style per call. Easing from `fmath`/the easing builtins' Rust
kernels (shared, not duplicated). Web: a per-layer keyframe list in the scene
inspector (a timeline is nicer, a list is enough for v1). Effort **M** (S core,
M UI). Integer layout-pixel positions in v1; sub-pixel motion would need a
bilinear blit and can come later.

**Risk.** ~0.8–1.2 KB wasm (parser + evaluator); the UI is the cost — ~2 KB of
bundle for a minimal list editor. The library `.scene` files can use `M` lines
the day the core lands, before any UI.

### 2b. The director — a pattern that steers layers

**What it unlocks.** Reactive, procedural and stateful scenes: a sprite that
follows the beat (`frequencyData`) or bounces with real physics; a caption that
appears when an MQTT/HA event arrives (`readEvent`); layers that swap on the
hour (`clockHour`); the snake game's pattern layer moving a food *sprite* and
the scoreboard; a scene whose layers wander on curl noise. It is the general
answer to "scripted scenes" because it reuses the whole language (easings,
noise, time, beat, sensors, events, controls, the debugger) instead of
inventing a scene expression language.

**Surface.** Builtins that act only when the engine is a scene layer (silent
no-op otherwise, like the bulk ops outside `renderBulk`):
```js
layerCount()                         // layers in the running scene, 0 outside one
setLayer(i, x, y, opacity)           // move / fade layer i this frame (layout px, 0..100)
setLayerFrame(i, frame)              // hold sprite layer i on a frame (-1 = back to its clock)
setLayerVisible(i, on)
setLayerText(i, value, digits, decimals)   // a number into text layer i (see #11)
```
plus a new layer kind `L ctl …` (or `pat` with a `D` binding line meaning "run,
don't draw") so a pure director costs a program and globals but **no frame
buffer**. Any pattern layer may call `setLayer`, so a drawing pattern can also
drive its neighbours; overrides from engines apply in layer order and last
one wins per property. The director's own controls appear as scene controls
(`C` lines already work for any `pat` layer).

**Where.** VM: a small fixed `[LayerDyn; MAX_LAYERS]` override table in the VM
(4 entries, cleared per frame); `SceneHost::pattern_frame` returns, the driver
copies overrides into the compositor's per-frame styles before compositing the
layers above (so a director must sit below what it moves — the natural
"script at the bottom" convention); `compose.rs` already composites from a
per-call style. Firmware `scenes.rs` builds `ctl` layers with pixel count 0.
Builtin table: five appends (append-only rule). Effort **M**.

**Risk.** ~0.6–1 KB wasm, ~1 KB firmware; budget: a `ctl` layer is an engine
without a frame (~program + globals), so `caps.layers` accounting needs a
cheaper tier for it. Scheduling: the director must run before the layers it
moves in the same frame — with #842 offloading layer engines to core 0 that
ordering needs care (run the director first, on the render core).

**Becomes redundant / far cheaper (2a + 2b together).** emoji-animation-2,
blinky-eyes-2d, eye-of-sauron(-with-movement): face sprite + iris sprite moved
by keys/director instead of per-pixel ellipse math. crossfading,
shimmer-crossfade-2d, fast-palette-blending's crossfade: two pattern layers
with keyed opacity. rainbow-smiley, heart, us-flag (static art): sprite over a
pattern layer (with #4). scrolling-text-marquee-2d, marquee-chase,
rainbow-fonts: a text layer (already possible today — these are delete or
convert-to-scene now). 2d-clock-with-hand-color-pickers, rgbclock-2d,
sunrise-alarm-clock: clock text layer over an ambient pattern, the alarm ramp
as keyframes. bulk-sprite-scroll-2d: a sprite layer with two keys.

---

## 3. Palette system — `palette` control, named palettes, OKLab LUT, palette blend

**What it unlocks.** The biggest dedupe lever. Many library entries are one
idea in several colourways: autumn-colors / spring-colors /
christmas-rg-fade / halloween-color-twinkles / halloween-wavy-bands /
holiday-* / fire-red / fire-blue / perlin-fire-* / gradient-blue-purple-pink /
ryb-colors / 2-colors / twocolorhsvmix / color-blend. With a palette *control*
they collapse into a few patterns, each with a palette picker whose presets
include the seasonal looks. utility-palettes and fast-palette-blending (two
hand-rolled palette managers) become a builtin call. A paint()-based pattern
also becomes recolourable in a scene without the layer ramp's luma remap
(which loses the pattern's own hue structure).

The second win is cost: `mixColors` per pixel is ~47 µs/call on the panel
(#935: Breathing Gradient 770 ms/frame at 16384 px). A two-stop OKLab palette
cooked into a 256-entry LUT makes the same gradient one table read per pixel.

**Surface.**
```js
// control: the UI shows a palette chooser (shipped presets + custom stops);
// the engine hands the stops as the flat [pos, r, g, b, …] array setPalette takes
export function palettePickerColors(stops) { setPalette(stops) }   //# default=sunset

setPalette(arr, space = 0)     // space 1 = interpolate stops in OKLab, cooked to a 256-entry LUT
                               // (space 0 stays the PB-exact path, byte-for-byte)
paletteBlend(dst, palA, palB, t)   // stop-wise blend of two palettes into dst (crossfades)
```
Named presets ship as data (`library/palettes/*.pal` → `palettes.json`, the
sprite/scene generator pattern), referenced by name in `default=`. On the wire
a palette control value is a vector of raw 16.16 ints — the `C <name> <raw…>`
grammar the playlist and scenes already use for `hsvPicker`, so no wire change.

**Where.** `engine.rs` control-prefix table (`("palettePicker",
ControlKind::PalettePicker)`), `jsonview.rs`, `vm.rs` palette cook (LUT keyed
on the array's mutation epoch, the same invalidation `setPalette`'s live
reference already uses), `outpipe::fill_palette_lut` is the LUT cooker to
reuse (the scene ramp's), web: a palette picker control component (the ramp
editor from #787 already exists and could be reused for custom stops).
Effort **M**.

**Risk.** ~1 KB wasm; the picker UI and the preset data (~1.5–3 KB) are the
real bundle cost — reuse the ramp editor, keep presets ≤ 16. PB compatibility:
`space 0` must remain bit-exact (oracle-verified paint()).

**Becomes redundant / far cheaper.** The colour-variant families above merge;
breathing-gradient, beat-bounce, color-blend, twocolorhsvmix drop per-pixel
`mixColors`/HSV lerps for `paint()` from a 2-stop OKLab LUT; utility-palettes,
fast-palette-blending become teaching examples or go.

---

## 4. Clip layers — pattern-filled text and sprites

**What it unlocks.** The classic LED-sign effect — "the time drawn in fire",
rainbow text, a waving flag *inside* a heart sprite, aurora seen through a
logo — which today cannot be built: text draws one flat colour, and the `mask`
blend (`B·luma(L)`) leaves everything outside the mask untouched, so it cannot
confine a pattern to a shape. clock-over-aurora, heartbeat and sunrise could
each gain a pattern-filled variant.

**Surface.** A binding line `X clip` on a layer: its α is multiplied by the
coverage of the layer **directly below** (coverage = that layer's α > 0 after
its key/transparency). Stack: `[background pat] [text, normal] [fire pat, X
clip]` → fire inside the glyphs over the background. Photoshop's clipping
mask, which users already know. A binding line rather than flag bit 16
because today's parser rejects `flags > 15` (old firmware would refuse the
whole scene); an unknown binding line is ignored.

**Where.** `compose.rs`: record the coverage of the last composited layer into
a 1 bit/px mask (512 B at 4096 px, 2 KB at 16384 px, taken fallibly like
`scratch_for`), consult it in `blend_box`/`blend_run` for a clip layer;
`scene.rs` binding line; web: one toggle in the layer inspector. Effort
**S–M**.

**Risk.** ~0.4–0.6 KB wasm, a toggle's worth of UI. RAM as above, PSRAM on
arena boards.

**Becomes redundant.** rainbow-smiley (rainbow + smiley stencil, in-pattern),
rainbow-fonts, heart, scary-pumpkin/the-grinch-style "picture with a fill":
sprite + clipped pattern layer.

---

## 5. Afterglow post stage — `setTrail(decay, mode = max)`

**What it unlocks.** Trails for **any** pattern, including per-pixel `render`/
`render2D` ones, with no buffer: today a trail needs either `renderBulk`
persistence (`fade(k)`) or an `array(pixelCount)` per channel. 68 library
patterns allocate `array(pixelCount)`; a large share of them are trail/decay
buffers (meteor-shower keeps three = 48 KB at 4096 px; slowflies two plus a
peak-hold loop; marching-rainbow-buffered two; matrix-green-waterfall-1d;
bouncing-balls-hsv/rgb; color-twinkle-bounce; comets; rainbow-comet;
sparks*; firework-*). Also a scene-layer option `X trail 85` makes a moving
sprite (driven by #2) leave a comet tail.

**Surface.** `setTrail(decay)` — post-chain stage, first in the chain
(`trail → outputPalette → blur → glow → gamma`): out = max(new, prev·decay)
(mode 1 = add-then-clamp). Off by default; one comparison per frame when unset,
like the other stages. Device-level setting optional (Settings → Output).

**Where.** `engine.rs` post chain. The previous frame is already in the
engine's buffer when `render` writes pixel *i*, so the stage can blend at
write time with **no extra buffer** — except when gamma/palette stages are
active (the buffer then holds post-processed values), which needs a pre-chain
copy (3 B/px) or a documented "trail of the graded frame" semantics. Effort
**S**.

**Risk.** ~0.3 KB wasm. Under the same per-pixel loop as gamma — measure on
patbench (CLAUDE.md engine rule) that the unset case costs nothing.

---

## 6. Strip-space anti-aliased bulk ops

**What it unlocks.** Strip patterns cannot use `splat`/`drawLine`: they are
coordinate-space, and naming one gives a `renderBulk` pattern the default
`ceil(√n)` grid (bulk-render.md "Default map") — a strip pattern silently
becomes 2D. So fireworks-finale hand-rolls a sub-pixel `splat()` into three
`array(pixelCount)` RGB buffers; comets/meteors hand-roll tails. Index-space AA
ops with the brush fix that:

```js
splatIndex(pos, radius, mode)          // soft blob at fractional pixel pos, linear falloff
fillRangeAA(p0, p1, mode)              // [p0, p1) with fractional end coverage
gradientRange(p0, p1, h0, s0, v0, h1, s1, v1, mode)   // a comet tail in one call
mirrorIndex(kind)                      // 0: copy first half reversed onto the second; 1: wrap-ring symmetric
```
**Where.** `bulk.rs` index-space section; `mode` reuses the blend arms.
Effort **S** (four small kernels).

**Risk.** ~0.5–0.7 KB wasm; must stay out of the "names a coordinate-space op"
list so strips stay mapless.

**Becomes redundant / far cheaper.** fireworks-finale (three pixelCount buffers
→ none: `fade` + `splatIndex`), meteor-shower, comets, rainbow-comet,
bulk-comet-trails, firework-dust/nova/rocket-sparks, rainbow-rocket-sparks,
sparks, sparks-center, kitt, kitt-w-color-picker, cylon, scanner,
bouncing-balls-hsv/rgb, color-twinkle-bounce, pendulum-wave; the KITT/cylon/
scanner trio is a merge candidate once all three are `fade` + `splatIndex`.

---

## 7. Symmetry — coordinate folds first, domain-reduced rendering later

**What it unlocks.** 35 patterns mention kaleid/mirror; the ones that implement a coordinate fold
themselves (kaleidoscope-2d's `abs(mod(atan2(dy, dx) + spin, seg) - seg/2)`,
zoom-kaleidoscope, distance-function-kaleidoscope-2, sound-spectrokalidamandala,
sound-spectro-kalidastrip, butterfly-2d, block-reflections, opposites,
regenbogendrogen) would shrink to their source texture. As an engine fold, **any** pattern can be kaleidoscoped —
and in a scene that is a per-layer option ("Aurora, 6-fold"), which turns the
kaleidoscope family into one scene feature plus a few distinct source textures.

**Surface.**
```js
mirror(axisMask)        // transform op: fold x (1) and/or y (2) about the centre
kaleido(n, angle = 0)   // transform op: fold into n mirrored wedges about the centre
```
Scene: `Y kaleido 6` / `Y mirror 3` binding line on a `pat` layer (host calls
the same fold on that layer's engine).

**Step 1 (S):** a non-affine fold applied after the affine transform in
`Vm::apply_transform`'s caller — every pixel still renders, the pattern just
sees folded (x, y). **Step 2 (M):** domain-reduced rendering — the engine
renders only the fundamental domain's pixels (½, ¼, 1/n) and fills the rest
from a per-map u16 source-index table (8 KB at 4096 px, 32 KB at 16384 px —
PSRAM on the S3), a 2–8× saving on 128×128. Mirror on a regular grid is exact;
n-fold wedges are a nearest-pixel remap.

**Risk.** Step 1 ~0.3–0.4 KB wasm. Step 2 RAM as above; disable on boards
without the arena at large pixel counts.

---

## 8. Stored sprites drawn from pattern code — `drawSprite`

**What it unlocks.** Scene sprite layers are capped by `MAX_LAYERS = 4`; a game
or a flock needs N moving sprites. Today a pattern that wants pixel art
carries it as inline `sprH/sprS/sprV` arrays (bulk-sprite-scroll-2d) or draws
it procedurally (emoji-animation-2, blinky-eyes-2d, rainbow-smiley,
us-flag-2d stars, scary-pumpkin, nyan-lights, frogger-2d, breakout-2d,
tetrix-2d). With access to the sprite store, art is authored once as a
`.sprite` (diffable, palette-indexed, animated) and placed by code.

**Surface.** Reuse the text-literal mechanism (quoted literals are already
legal as text-builtin arguments and resolve to a handle):
```js
var heart = sprite("Heart")              // handle by name (library or store); 0 if absent
drawSprite(heart, col, row, frame, mode) // grid space, keyed by index 0, clipped; frame -1 = its own clock
spriteWidth(h) / spriteHeight(h) / spriteFrames(h)
```
**Where.** `bulk.rs` grid-space op over `sprite::SpriteView` (already
`no_std`, allocation-free, borrows the mapped record); the host resolves a
name to a record through a callback like `textSlot`; the device pins records a
pattern names the way a resident scene pins its sprites. Effort **M** (the
pinning and name resolution, not the blit).

**Risk.** ~0.6–0.9 KB wasm; a pattern that names a sprite has a store
dependency — `drawSprite` of an absent handle is a no-op, and scene export
(#746) must carry the dependency.

---

## 9. fBm octaves on `fillNoise2D/3D`

**What it unlocks.** The plasma/noise family (perlin-simplex-noise-2d,
coral-plasma, spinning-plasma-2d, perlin-fire*, ocean, lightning-clouds,
sound-lavablob, fireblobs, aurorashivers, oasis, glowflow) stacks 2–3 octaves of
simplex per pixel in interpreted code. `fillNoise` fills a canvas natively but
one octave at a time.

**Surface.** `fillNoise2D(dst, w, h, sx, sy, ox, oy, seed, octaves = 1, gain = 0.5)`
(and the 3D form) — each octave doubles frequency, scales by gain, seeds
`seed + k` like `perlinFbm`'s per-octave fields. Optional ridge flag later.
**Where.** `bulk.rs` noise fills. Effort **S**. Multiplies with #840 (kernel
cost) and #1 (render at lower resolution). **Risk.** ~0.2 KB.

---

## 10. Unblock #373 §2 / #453: element-wise array ops

Already filed; new evidence for priority. reaction-diffusion-2d's Gray–Scott
step is `a·b·b` plus two Laplacians: `stencil2D` exists for the Laplacian, but
without `arrayMul` the reaction term keeps the whole step interpreted, which is
why it stays at 16×16 ("at the small edge of what Gray–Scott needs"). With
`arrayMul(dst, src)`, `arrayAffine(dst, src, k, c)` and `arrayLookup(dst, src,
lut)` (element-wise table map — rule tables for CA) the whole step is ~10
native passes and RD/CA/wave sims can run at 32–64² — the reaction-diffusion,
cyclic-cellular-automata-2d, continuous-cellular-automata, cellular-automata-1d,
falling-sand-2d, ember-diffusion, ripples-2d, bulk-canvas-ripples-2d family.
#453 group A (firework-dust, sound-blinkfade, lissajous-curve-tracer,
unstable-orbits-2d, time-flies-2d, sound-rays, …) is blocked on exactly
`arrayAffine` + a multiply. Effort **S** each in the `arrayAdd` family. **Risk**
~0.4 KB total.

---

## 11. Text: legible size + numbers from patterns

**What it unlocks.** The largest font is 5×8. On a 64×64 panel a clock is a
strip across the middle; on 128×128 it is tiny. Clock scenes
(clock-over-aurora) and clock patterns (rgbclock-2d, 2d-clock-…,
sunrise-alarm-clock, pixelclock on a matrix) want 2–4× glyphs. And a game or
meter pattern has numbers worth showing (snake's score, a BPM, an
audio-volume-meter level) that a text layer cannot see.

**Surface.** Text layer: honour `fit` — `fill`/`contain` scale glyphs by the
largest integer that fits the box (nearest), `tile` unchanged; pattern side:
`fontScale(n)` modal like `font()`. Text source `T num <layer> <var> <digits>
<decimals>` reads a numeric exported var of pattern layer `<layer>` each frame
(resolved once at `set_scene` via `Engine::var`, which exists). Effort **S + S**
(`text.rs` draw loop, `compose::draw_text_layer`, `scene.rs`).
**Risk.** ~0.3 KB wasm + a source option in the text inspector.

---

## 12. Per-layer speed — `V <pct>`

A scene-side `timeScale` for a pattern layer (the engine already has
`timeScale`; the host calls it at build). Lets a scene run any pattern at 25 %
or 300 % without the pattern exporting a speed slider — many ports have none,
and "slow" variants are a dedupe target (slow-color-shift vs color-fade-pulse,
twinkle vs twinkle-2). Natural sibling of #793's fps cap; put both in one PR
and one inspector row. Effort **S**, ~0.1 KB + one UI field.

---

## 13. Cached polar coordinates — measure before building

33 patterns call `atan2` (often with `hypot`) per pixel. A per-map cached
(angle, radius) pair, exposed as `pixelPolar(i, out)` or a `renderPolar(index,
a, r)` entry, would remove both from the per-pixel body — but #938 already made
`atan2` a direct JIT entry, and the table costs 8 B/px (32 KB at 4096 px,
128 KB at 16384 px) and is invalidated by any transform. Measure the
atan2+hypot share on `tools/patbench.mjs` with 2d-spiral-twirls and
rainbow-pinwheel first; build only if it is still ≥ 30 % of their frame.
Effort **M**, low rank.

---

## Considered and not recommended

- **A native particle system** (`particlesStep`/`particlesDraw`). Integration
  over 50–150 particles in `beforeRender` is cheap; the per-frame cost in the
  particle patterns is *drawing* and *buffers*, which `splat` (2D), #6 (strips)
  and #5 (trails) remove without freezing one physics model into the VM.
- **A scene expression language** (a tiny formula per layer property). The
  director (#2b) gives the same power with the existing language, compiler,
  debugger and docs; a second language is a second parser in luxel.wasm.
- **Arbitrary-angle layer rotation.** Needs a rotate-blit kernel and
  sub-pixel sampling; a rotating element is better drawn by the pattern (its
  transform stack already rotates). Revisit after #2a if the scene library
  asks for it.
- **Scene-level beat clock.** Pattern layers share the activation instant and
  the engine clock; `beat(bpm)` already agrees across layers. Worth it only
  with real audio beat detection (#142/#119 hardware first).
- **Warp/displacement post stage** (us-flag-2d's cloth wave, heat shimmer).
  Real but niche; us-flag-2d does it fine in `render2D`.

## Suggested order

1. #1 thin slice (bilinear `fillCanvas`/`paintCanvas`) → full `setRenderSize`,
   built together with #843 as one mechanism.
2. #2a keyframes (core + `.scene` files first, UI after), then #783, then #2b.
3. #5 trail, #6 strip ops, #9 fBm, #10 array ops — four S PRs that the library
   rewrite can use immediately.
4. #3 palette system, #4 clip layers.
5. #7 step 1, #11, #12; #8 and #7 step 2 when the library work asks for them;
   #13 only after the measurement.
