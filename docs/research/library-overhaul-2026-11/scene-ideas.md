# Library scenes & sprites — proposal

Scope: what example scenes and sprites the shipped library should carry, which
patterns should become scenes/sprites, which patterns earn their keep as ramped
base layers, and what the scene system cannot yet express.

Sources read: `docs/spec/scenes.md`, `library/README.md`, all 5 scenes and 11
sprites in `library/`, `docs/webui.md` (§ "Sprites and scenes get a Library
source"), `docs/design/webui-v2/proposal.md` §5.5/§5.5b + amendments A1–A9,
`crates/luxel-core/src/compose.rs` (text keying, `skip`), `outpipe::detect_grid`,
`docs/lang.md` (text, projection), the inventory, and ~30 contact sheets.

## 0. Ground rules these designs obey (and why)

Facts from the code that constrain every design below:

* **≤ 2 pattern layers per scene.** The console says *"luxel devices only
  support N pattern layers"* and N is 2 on the classic boards
  (`caps.layers`). Text, sprite and colour layers are free and hold no engine.
  Every scene below uses 0–2 `pat` layers.
* **Scenes draw NOTHING on a strip today.** `compose::skip` returns early on an
  empty `GridMap`, and `detect_grid` refuses `w < 2 || h < 2`, so a 1D strip
  (or a 3D/custom map) composites to black. The console hides the Scenes tab for
  non-matrix layouts. A strip WOUND into a matrix (zig-zag on a wall) is a grid
  and works. §1.I designs strip scenes against the missing feature (G1).
* **Text is always black-keyed** (`draw_text_layer` forces `Key::Black`). Two
  consequences: (a) a `000000` text colour is invisible — a "shadow" must be
  `101010` or similar; (b) **`mask` on a text layer is a no-op** (white glyph →
  `B·1 = B`, keyed-out background → `B` untouched). Proposal §5.5 promises
  "text set to Mask over a rainbow base = rainbow-filled letters"; the shipped
  code cannot do that. The working recipe is **white text on a black band +
  the pattern ABOVE it in `multiply`** (scene 15) — see gap G5.
* **Sprites never move.** Position is a static box; motion comes only from the
  sprite's own frames, `fit tile`, flips, and the `A <fps>` override (two copies
  of one sprite at different `A` rates de-sync). See gap G2.
* **Sprite `fit`:** box `w/h = 0` → natural size at `(x, y)`; with a box,
  `fill` stretches (nearest), `contain` scales uniformly and centres, `tile`
  repeats 1:1. `tile` needs an explicit box.
* **Ramps clamp at both ends** and key on luma, so a constant-brightness
  rainbow (`bulk-rainbow`, v=1) ramps into hue-luma banding, not structure.
  Good ramp sources are luma-rich (dark seams, glows, fire, noise).
* **1D patterns become 2D backdrops via `P`.** `P y` lays a `render(index)`
  pattern down the rows (row replicated) — `breathing-gradient` with `P y` is a
  sky gradient; `rainbow` with `P y` is horizontal rainbow stripes.
* **Design canvas = 32×32.** README: shipped scenes are authored for a 64×64
  panel but keep placed boxes inside 32×32 so they read on a smaller matrix;
  full-layout layers use `0 0 0 0`. (On 64×64 and 128×128 that parks the
  composition in a corner — gap G3.)
* **Only printable ASCII** draws (`°` → `?`), fonts: `tiny` 3×6 (4 px pitch, 8
  chars in 32 px), `regular` 5×7 (6 px pitch, 5 chars), `large` 5×8 (6 px).
* **Stems will move.** Pattern stems below are today's; the dedupe pass will
  rename/merge some (`@pat/<stem>` must be re-pointed after it — the gallery
  gate fails a dangling reference, so this cannot rot silently).
* Device scene store is **3840 B total** (`BLOB_MAX`); the blocks below are
  200–450 B each, so keep `N` names short and skip default lines.

`NEW` after a `@spr/` slug = a sprite proposed in §2. Raw control values are
16.16 (`0.5` → `32768`, `1` → `65536`).

---

## 1. Example scenes (30)

### A. Clocks and info displays

**1. Night Clock** — a bedside clock that does not light the room: a dim deep-blue
star sky, the time in amber-red (night-vision friendly). *Learns:* a ramp that
caps brightness (last stop is dim, and the ramp clamps above it), opacity on a
base.
```
S - Night Clock
L pat 0 0 0 0 normal 60 none fill 1
N Sky
I @pat/starfield-2d
R 100 0:000000 255:24345c
L text 0 12 0 0 normal 100 black fill 1
N Time
T clock hh:MM
F regular b04a14 c none 0
```

**2. Date & Time** — time large, date tiny under it, over a teal-ramped interference
field with a 55 % black **scrim** between them. *Learns:* a colour layer at partial
opacity as a legibility scrim; two clock formats; stacking text layers.
```
S - Date & Time
L pat 0 0 0 0 normal 100 none fill 1
I @pat/interference-2d
R 100 0:00080c 120:0a5c6b 220:4fd1c5 255:e6fffb
L color 0 0 0 0 normal 55 none fill 1
N Scrim
L text 0 7 0 0 normal 100 black fill 1
T clock HH:MM
F large ffffff c none 0
L text 0 18 0 0 normal 100 black fill 1
T clock MM-DD
F tiny 9fd8ff c none 0
```

**3. Message Board** — the household note panel. Slot 0 crawls along a black band
(`POST /api/text 0 Dinner at 7`, or an HA text entity); slot 1 is a static
headline beside an envelope icon. *Learns:* `T slot n`, a band box for legibility,
text from outside without editing anything.
- `pat curl-flow-2d` full, `R 60 0:000000 255:6a7bff` (calm, 60 % recolour)
- `color 0 20 0 10 normal 75` black band
- `sprite 1 2 0 0 normal` → `@spr/envelope` NEW
- `text 11 3 0 0` `T slot 1`, `F tiny ffd166 l none 0`
- `text 0 21 0 0` `T slot 0`, `F regular ffffff l left 20`

**4. Weather Card** — sky gradient, a weather glyph, temperature large, a crawling
detail line. Slot 0 = `"18C"`, slot 1 = `"RAIN 80% WIND 12"`. *Learns:* 1D pattern
as a backdrop via `P y`; `align r` in a sub-box; the `visible` flag (a hidden rain
layer the user toggles on). The glyph cannot follow the forecast yet (G7).
- `pat breathing-gradient` full, `P y`, `R 70 0:0b2a5b 255:6fb3ef`
- `sprite 1 2 0 0` → `@spr/cloud-sun` NEW
- `sprite 2 12 14 8 normal 100 none tile 0` → `@spr/rain-tile` NEW (flags 0 = hidden)
- `text 16 4 15 8` `T slot 0`, `F large ffffff r none 0`
- `text 0 24 0 0` `T slot 1`, `F tiny cfe8ff l left 10`

### B. Ambient: pattern + ramp recolours

**5. Tidepool** — slow deep-water interference with bright rings rippling through it.
*Learns:* a ramp turns a hue-cycling pattern into one palette; `add` puts a sparse
second pattern on top.
- `pat interference-2d` full, `R 100 0:000510 100:003a66 200:0f8fa8 255:9ff7ff`
- `pat ripples-2d` full `add 70`, `R 100 0:000000 255:d8fffa`

**6. Lava Lamp** — fire metaballs recoloured purple→magenta→orange over a plum
wash. *Learns:* `lighten` (the wash shows wherever the blobs are darker than it);
`R 85` keeps a little of the original fire.
- `color` full, `K 1a0026`
- `pat metaballs-of-fire-2d` full `lighten 100`,
  `R 85 0:000000 60:5b0a7a 150:ff3d81 220:ffb347 255:fff1c1`

**7. Witch's Cauldron** — green doom-fire boiling up out of a cauldron sprite.
*Learns:* same fire, different element, purely by ramp; a sprite occluding a
pattern (`normal`, index-0 transparency).
- `pat doom-fire-2d` full, `R 100 0:000000 80:0b3d0b 160:39ff14 230:c6ff8a 255:ffffff`
- `sprite 8 21 0 0` → `@spr/cauldron` NEW (2 frames bubbling)
- `text 0 1 0 0` `T lit DOUBLE DOUBLE`, `F tiny 9dff7a l left 10` (optional)

### C. Pattern masked by another pattern

**8. Rainbow Ripples** — a rainbow plasma visible only along expanding ripple rings.
*Learns:* `mask` = "show what's beneath only where this layer is bright" — the
shape comes from one pattern, the colour from another.
- `pat spinning-plasma-2d` full
- `pat ripples-2d` full `mask 100` (its own cyan tint is irrelevant — mask reads luma)

**9. Stained-Glass Fire** — fire seen through leaded-glass cells: bright cell centres,
dark seams. *Learns:* `mask` with a cellular pattern; a control override picks the
mask's style.
- `pat perlin-fire` full (or whichever fire survives the dedupe)
- `pat voronoi-2d` full `mask 100`, `C sliderStyle 65536` (glowing orbs),
  `C sliderPoints 458752` (7)

**10. Lighthouse** — a radar sweep reveals a picture and lets it fade, like a beam
across a coastline at night. *Learns:* a mask can sit over sprites and colour,
not just patterns; phosphor persistence becomes a "reveal".
- `color` full `K 0a1a2e` (sea)
- `sprite 0 0 32 32 normal 100 none fill 1` → `@spr/christmas-tree` NEW, or any
  32×32 picture (swap freely)
- `pat radar-2d` full `mask 100`

### D. Add-blended particle layers over a base

**11. Fireflies at Dusk** — a warm-to-indigo dusk gradient with fireflies blinking
over it. *Learns:* `add` of a sparse pattern over a DIM base (added over a bright
base, sparks vanish — the Spark Field lesson); 1D pattern laid down `P y`.
- `pat breathing-gradient` full `P y`, `R 100 0:05030f 120:2a1840 200:8a3b2e 255:d4733a`, opacity 70
- `pat fireflies` full `add 100`, `R 100 0:000000 255:e8ff6a`

**12. Spark Field** — EXISTS (tiled spark sprite added over twirls). Keep.

### E. Text marquees over patterns

**13. Happy Birthday** — confetti, a cake with flickering candles, the greeting
crawling, the name (slot 0) on top. *Learns:* a lit marquee plus a slot in one
scene; an animated sprite as the focal point.
- `pat chill-confetti` full opacity 70
- `sprite 8 3 0 0` → `@spr/cake` NEW
- `text 0 21 0 0` `T lit HAPPY BIRTHDAY`, `F regular ffffff l left 18`
- `text 0 0 0 0` `T slot 0`, `F tiny ffd166 c none 0`

**14. Neon Sign** — `OPEN` in hot pink with a soft glow. *Learns:* stacking free text
layers — two offset copies at 30 % `add` make a bloom; the crisp copy on top. Also
the black-key rule: a shadow/glow colour must not be exactly `000000`.
```
S - Neon Sign
L color 0 0 0 0 normal 100 none fill 1
K 06000a
L text -1 12 32 8 add 30 black fill 1
T lit OPEN
F large ff2d95 c none 0
L text 1 12 32 8 add 30 black fill 1
T lit OPEN
F large ff2d95 c none 0
L text 0 11 32 8 add 30 black fill 1
T lit OPEN
F large ff2d95 c none 0
L text 0 12 32 8 normal 100 black fill 1
T lit OPEN
F large ffc2e0 c none 0
```
(The flicker a real neon sign has needs G9.)

**15. Painted Letters** — rainbow-plasma-filled letters crawling across a band over an
aurora. *Learns:* the **multiply** fill recipe — white text on a black band, then a
pattern above it in `multiply`, boxed to the band: white × pattern = pattern, black ×
pattern = black. The canonical answer to "rainbow text" (and the honest
replacement for §5.5's Mask claim, G5).
```
S - Painted Letters
L pat 0 0 0 0 normal 100 none fill 1
I @pat/aurora-2d
L color 0 10 0 12 normal 100 none fill 1
N Band
L text 0 12 0 0 normal 100 black fill 1
T lit HELLO WORLD
F regular ffffff l left 16
L pat 0 10 0 12 multiply 100 none fill 1
I @pat/spinning-plasma-2d
```

### F. Sprite characters over scenery

**16. Aquarium** — deep water, plankton specks, swaying seaweed tiled along the
floor, two fish facing opposite ways, a bubble column. *Learns:* `tile` for
scenery, `flipx` to reuse one sprite, `A` to de-sync the copies.
- `pat interference-2d` full, `R 100 0:000510 100:003a66 200:0f8fa8 255:9ff7ff`
- `pat glitter` full `add 40`, `R 100 0:000000 255:bff9ff`
- `sprite 0 20 32 12 normal 100 none tile 1` → `@spr/seaweed` NEW
- `sprite 4 6 0 0` → `@spr/fish` NEW
- `sprite 17 13 0 0 … flags 3` (flipx) → `@spr/fish`, `A 3`
- `sprite 26 0 6 20 normal 100 none tile 1` → `@spr/bubbles-tile` NEW

**17. Campfire** — star sky, a crescent moon, a crackling campfire, embers rising.
*Learns:* a 6-frame sprite as the focal animation; a pattern ramped to ember
colours as particles.
- `pat starfield-2d` full, `R 100 0:000000 255:c8d4ff`
- `sprite 22 2 0 0` → `@spr/moon` NEW
- `pat fireflies` full `add 70`, `R 100 0:000000 255:ff9a3c` (embers)
- `sprite 8 17 0 0` → `@spr/campfire` NEW

**18. Space Cadet** — a warp starfield, a ringed planet, a rocket with a flickering
exhaust, a little alien. *Learns:* a control override (`sliderWarp` high) to
retune a base; several sprites at independent rates.
- `pat starfield-2d` full, `C sliderWarp 52429` (0.8)
- `sprite 18 3 0 0` → `@spr/planet` NEW
- `sprite 6 9 0 0` → `@spr/rocket` NEW
- `sprite 20 22 0 0` → `@spr/pixel-alien` NEW, `A 2`

### G. Holiday compositions

**19. Jack-o'-Lantern** — a carved pumpkin with fire flickering through the eyes and
grin; two bats flapping out of phase. Replaces `scary-pumpkin`. *Learns:* a sprite
with an OPAQUE near-black surround and TRANSPARENT holes, over a pattern clipped to
the same box — the pattern shows only through the carving.
- `pat perlin-fire` box `0 0 32 32` (clip window), `R 100 0:000000 90:7a1500 170:ff6a00 235:ffd166 255:fff3c4`
- `sprite 0 0 0 0` → `@spr/jack-o-lantern` NEW (32×32)
- `sprite 2 2 0 0` → `@spr/bat` NEW; `sprite 22 4 0 0 … flags 3` → `@spr/bat`, `A 6`

**20. Silent Night** — indigo sky, a twinkling tree, engine-free snowfall, a crawling
greeting. Replaces `orv-christmas-tree`'s scene. *Learns:* a **loop-tile sprite**
(a 16×16 tile whose 16 frames shift the flakes down 1 px, tiled over the panel) is
snowfall with no engine at all.
- `pat starfield-2d` full, `R 100 0:03041a 255:9fb4ff`
- `sprite 4 1 0 0` → `@spr/christmas-tree` NEW
- `sprite 0 0 64 64 normal 100 none tile 1` → `@spr/snowfall-tile` NEW
- `text 0 26 0 0` `T lit MERRY CHRISTMAS`, `F tiny ffd166 l left 12`

**21. New Year's Eve** — fireworks, a scrim band, the time to the second, the greeting.
*Learns:* `HH:MM:SS` fits 32 px only in `tiny`; a scrim band rather than a full
scrim keeps the show bright. (A real countdown needs G8.)
- `pat 2d-fireworks-fade` full
- `color 0 11 0 9 normal 60` black band
- `text 0 12 0 0` `T clock HH:MM:SS`, `F tiny ffffff c none 0`
- `text 0 24 0 0` `T lit HAPPY NEW YEAR`, `F tiny ffd166 l left 14`
- `sprite 2 1 0 0 add` → `@spr/firework-burst` NEW, `A 9`

**22. Fourth of July** — the waving flag dimmed under fireworks and two burst sprites
out of phase. *Learns:* a showcase pattern as a 45 %-opacity base; `add` of both a
pattern and sprites.
- `pat us-flag-2d` full opacity 45
- `pat firework-nova` full `add 100`
- `sprite 14 2 0 0 add` → `@spr/firework-burst`; `sprite 2 12 0 0 add … flags 3`, `A 7`

**23. Candle Row** — a row of flickering candles over a warm dim wash with a slow
glitter. For Advent, Hanukkah, Diwali, a birthday: the user sets the COUNT by the
box width. *Learns:* `tile` as a counter — a 4-px-wide candle cell tiled across a
28-px box is 7 candles; a menorah is two tiled boxes of 4 plus one raised single.
- `color` full `K 140804`
- `pat glitter` full `add 35`, `R 100 0:000000 255:ffcf7a`
- `sprite 2 16 28 12 normal 100 none tile 1` → `@spr/candle` NEW
- (menorah variant: `sprite 0 18 16 12 tile`, `sprite 14 14 0 0`, `sprite 18 18 16 12 tile`)

### H. Music / sound overlays

(Every one needs a sensor feed — `/api/sensors` from a PB sensor board or a host;
no board's mic is wired yet. They run dark otherwise.)

**24. Spectrum over Aurora** — a ramped spectrum strip along the bottom, added over
aurora. *Learns:* a pattern box as a CLIP window (the analyser renders full-frame;
the box keeps only its bottom quarter).
- `pat aurora-2d` full opacity 80
- `pat sound-spectromatrix-agc` box `0 24 0 8` `add 100`, `R 100 0:000000 120:2b7fff 220:ff4fd8 255:ffffff`

**25. Now Playing** — sound plasma ramped to a single palette, a music-note icon and
the track title from slot 0 (an HA `media_player` automation writes it).
- `pat sound-spectromatrix-agc` full, `R 80 0:000000 140:5a1a8c 230:ff7ad9 255:ffffff`
- `color 0 22 0 10 normal 70` black band
- `sprite 1 23 0 0` → `@spr/music-note` NEW
- `text 10 24 0 0` `T slot 0`, `F tiny ffffff l left 12`

**26. Pulse to the Beat** — make ANY composition music-reactive: a beat envelope
pattern on TOP in `mask` dims the whole stack between beats. Shown over the
Heartbeat composition. *Learns:* `mask` as global modulation. Needs a tiny new
library pattern `beat-envelope` (uniform grey = smoothed `energyAverage` with a
floor control so it never goes fully black) — or G11 makes it a layer parameter.
- `color` `K 140208`; `sprite 2 1 20 20 add` → `@spr/pulse-ring`; `sprite 7 6 9 8` → `@spr/heart`
- `pat beat-envelope` NEW full `mask 100`

### I. Strip-friendly scenes (need G1; work today only on a wound strip)

What survives on a W×1 row: colour washes, pattern + ramp, pattern-masked-by-pattern,
`add` particles, boxes as **segments** (`x`, `w`), `flipx` as a reversed segment,
1-row sprites (`tile` = repeating pattern, `fill` = stretched bands, frames =
chases). What does not: text and clocks (the smallest font is 6 rows), anything 2D.

**27. Strip Zones** — WLED-style segments without segment code on a 150 px strip:
fire on the left half, ocean on the right half running AWAY from the seam, a soft
warm wash where they meet. *Learns:* boxes are segments; `flipx` reverses a segment.
- `pat fire-red` box `0 0 75 0` (h 0 = the one row)
- `pat ocean` box `75 0 75 0`, flags 3 (flipx)
- `color 70 0 10 0 normal 40` `K ff9a3c` (seam glow)
- (a third zone, or a mirrored copy of the fire, needs a third engine — or G6)

**28. Bulb String** — classic C9 bulbs: a 1-row 8-px bulb-string sprite (`R . G . B . Y .`)
tiled the length of the strip, two frames trading which half twinkles. Replaces
`static-christmas-lights-4-colors` (and works on a matrix TODAY with the 16×6 2D
variant tiled across the panel).
- `sprite 0 0 0 0` won't tile → `sprite 0 0 300 1 normal 100 none tile 1` → `@spr/bulb-string` NEW (G3 would let `w=0` mean "whole strip")

### J. Blend-mode teaching scenes

**29. Light Boxes (Add)** — three overlapping colour boxes, red/green/blue, in `add`:
the overlaps show yellow, cyan, magenta and a white centre. Engine-free. *Learns:*
what `add` does, at a glance. Same scene with the top two in `lighten` shows no
white — duplicate it as "Light Boxes (Lighten)" for the comparison.
```
S - Light Boxes
L color 3 3 16 16 normal 100 none fill 1
K ff0000
L color 13 3 16 16 add 100 none fill 1
K 00ff00
L color 8 13 16 16 add 100 none fill 1
K 0000ff
```

**30. Vignette & Tint (Multiply)** — a kaleidoscope with a warm `multiply` tint and a
greyscale vignette sprite stretched over it in `multiply` to darken the corners.
*Learns:* multiply = white leaves it, black kills it, grey shades it; a greyscale
sprite is a reusable lighting mask for any scene.
- `pat kaleidoscope-2d` full
- `color` full `multiply 100`, `K ffc890`
- `sprite 0 0 32 32 multiply 100 none fill 1` → `@spr/vignette` NEW

**31 (optional). Hard vs Soft Key** — one glow pattern twice over a blue wash: left
half `key black`, right half `key luma`. Left keeps the dark halos (hard edge),
right fades them into the wash. *Learns:* the Transparent picker. Costs both
pattern layers on one idea, so ship only if the slot budget allows.
- `color` `K 0a2a6a`; `pat ripples-2d` box `0 0 16 32 normal 100 black`; `pat ripples-2d` box `16 0 16 32 normal 100 luma`

Also keep the five shipped scenes: **Clock over Aurora, Heartbeat, Loading, Spark
Field, Sunrise**.

---

## 2. Sprites worth shipping (text pixel art)

Existing 11: arrow-up, check, droplet, eye-blink, heart, plus, pulse-ring, spark,
spinner, sun-spin, walk-dot. Proposed below; **T1** = needed by a §1/§3 scene,
**T2** = good on its own. Every character sprite must read on DARK bases: no
near-black bodies (a bat is purple-grey, not black). Original art only — no
recognisable commercial characters (see `nyan-lights` in §3).

### Characters & faces
| slug | size · frames · fps | palette | for |
|---|---|---|---|
| `smiley` T1 | 16×16 · 1 | white face `ffffff`, features TRANSPARENT | Rainbow Smiley (§3): the base shows through eyes/mouth |
| `face-moods` T1 | 16×16 · 8 · 2 | yellow `ffcc33`, rim `b37400`, dark `3a2200`, cheek `ff7a7a`, white | expressions cycle: smile, smile, blink, glance L, glance R, surprised, wink, smile; replaces emoji-animation-2 |
| `cat` T2 | 16×12 · 6 · 3 | ginger `f4a259`, stripe `c46a1f`, belly `ffe1c0`, eye `7cff7c`, nose `ff7a9a` | sitting mascot: tail swish ×2, blink, ear twitch |
| `cat-runner` T1 | 16×10 · 4 · 12 | same cat palette | run cycle for Rainbow Runner (§3) |
| `ghost` T1 | 12×14 · 2 · 3 | `e8f0ff`, shade `a9b8d6`, eyes `1b1b2e` | wavy hem alternates |
| `bat` T1 | 16×8 · 4 · 8 | body `4b3a5a`, wing edge `7a5c94`, eyes `ff3b3b` | wings up/level/down/level |
| `fish` T1 | 12×7 · 2 · 4 | `ff8a3d`, fin `ffbf80`, band `ffffff`, eye `101010` | tail wag |
| `pixel-alien` T2 | 11×8 · 2 · 2 | `6cff6c`, eye `ffffff` | arcade-style but ORIGINAL silhouette |

### Icons & info
| slug | size · frames · fps | palette | for |
|---|---|---|---|
| `cloud` T2 | 16×9 · 1 | `ffffff`, `c8d2e0` | weather |
| `cloud-sun` T1 | 16×12 · 1 | sun `ffd166`/`fff3c4` + cloud | Weather Card |
| `rain-cloud` T2 | 16×14 · 3 · 6 | cloud + drops `5fa8ff` | drops fall within the frame |
| `storm-cloud` T2 | 16×14 · 4 · 4 | dark cloud `8a95a8` + bolt `ffe94d`/`ffffff` | bolt flashes on 1 of 4 frames |
| `snow-cloud` T2 | 16×14 · 3 · 4 | cloud + flakes `ffffff` | |
| `moon` T1 | 9×9 · 1 | `f2efd0`, crater `c9c4a0` | crescent; night scenes |
| `envelope` T1 | 9×7 · 1 | `f5f5f5`, fold `9aa4b2` | Message Board / notifications |
| `music-note` T1 | 8×10 · 2 · 2 | `ff7ad9`, `ffffff` | Now Playing; a 1-px bob |
| `bell` T2 | 9×9 · 4 · 8 | gold `ffd166`, `b8860b` | ringing swing; doorbell scene |

### Holiday motifs
| slug | size · frames · fps | palette | for |
|---|---|---|---|
| `jack-o-lantern` T1 | 32×32 · 1 | surround `0a0508` OPAQUE, shell `ff7a1a`, rib `c24e00`, hi `ffb066`, stem `3f7d20`; carve TRANSPARENT | Jack-o'-Lantern; ~2.1 KB |
| `christmas-tree` T1 | 24×30 · 3 · 2 | `0f5e2a`, `1f8f3f`, trunk `6b3e1e`, star `ffd84d`/`fff3b0`, bulbs red/blue/gold/white rotating per frame | Silent Night, Lighthouse |
| `candle` T1 | 4×12 · 4 · 8 | wax `f4ecd8`/`d8cdb0`, flame `fff3c4`/`ffd166`/`ff8a00`; col 4 transparent (the gap) | Candle Row; tileable |
| `firework-burst` T1 | 15×15 · 6 · 10 | `fff3c4`, `ffd166`, `ff5a5a`, `8a2d0b` | seed, shell, ring, sparkle, fade, empty |
| `cake` T1 | 16×16 · 3 · 6 | sponge `f6d6a8`, icing `ff8fc7`, plate `c8d2e0`, candles + flame | Happy Birthday |
| `bulb-string` T1 | 16×6 · 2 · 1 (+ an 8×1 strip variant) | wire `2f4f2f`, bulbs `ff2a2a` `2aff5a` `2a6bff` `ffcc2a` | Bulb String; tile |
| `cauldron` T1 | 16×10 · 2 · 3 | iron `3a3f4a`/`5c6370`, brew `39ff14` | Witch's Cauldron |

### Scenery and LOOP TILES (the engine-free motion trick)
A loop tile is a small tile whose frames shift its content by one step per
frame and wrap seamlessly in both axes; `fit tile` over a box turns it into a
full-panel moving texture with no engine.

| slug | size · frames · fps | palette | for |
|---|---|---|---|
| `snowfall-tile` T1 | 16×16 · 16 · 8 | `ffffff`, `b8c8e0` | flakes fall 1 px/frame, a few drift ±1 x; ~4.1 KB |
| `rain-tile` T1 | 8×16 · 8 · 24 | `7fb2ff`, `3d6fb0` | 2-px streaks, 2 px/frame |
| `bubbles-tile` T1 | 6×16 · 16 · 10 | `9ff7ff`, `ffffff` | rising rings |
| `seaweed` T1 | 8×12 · 2 · 2 | `1f8f3f`, `3fbf5f`, `0f5e2a` | sway; tiles along the floor |
| `campfire` T1 | 16×14 · 6 · 10 | logs `5a3416`/`3a200c`, flame `ff3d00` `ff8a00` `ffd166` `fff3c4` | Campfire |
| `rocket` T1 | 9×16 · 3 · 12 | hull `e8e8f0`, window `4ac8ff`, fins `e23d3d`, exhaust | Space Cadet |
| `planet` T1 | 14×14 · 1 | bands `e8c79a` `b8895a`, ring `d8e0ff` | Space Cadet |

### Utility (compositing tools as sprites)
| slug | size · frames | palette | for |
|---|---|---|---|
| `heart-mask` T1 | 16×14 · 4 · 6 | white `ffffff`, AA edge `a0a0a0` | a beating WHITE heart for the multiply-fill recipe (Rainbow Heart, §3) |
| `vignette` T1 | 32×32 · 1 | 8 greys `ffffff`→`202020` radial | `multiply`-stretched lighting mask (scene 30) |
| `pride-stripes` T1 | 1×6 · 1 | six flag colours | `fill`-stretched to any box = a flag in 9 bytes of texels (replaces rainbow-flag) |

Also **upgrade `eye-blink`**: add glance-left / glance-right frames (8 frames at 3
fps) so "Watching You" (§3) recovers the original's darting irises.

That is 34 slugs; the T1 set (~27) is what the scenes need. If the cap is 25,
drop the T2 weather variants (keep `cloud-sun`), `bell`, `cat`, `pixel-alien`.

---

## 3. Existing patterns → scene or sprite

### Convert (the pattern is a picture, a caption, or a composite)
| pattern | becomes | layer plan | note |
|---|---|---|---|
| `rainbow-smiley` | scene **Rainbow Smiley** + sprite `smiley` | `pat bulk-rainbow` full; `sprite 0 0 32 32 normal fill` → `smiley` (16×16 stretched 2×) | exactly the original's effect (white face, rainbow through the features); delete the pattern |
| `scrolling-text-marquee-2d` | scene **Marquee** | any base (`aurora-2d`, 70 %); `color 0 10 0 12 normal 60` band; `text 0 12 0 0 T slot 0 F regular ffd9b0 l left 18` | the pattern hand-rolls a blocky glyph table the engine now has real fonts for; delete |
| `emoji-animation-2` | sprite `face-moods` + scene **Googly Face** | `color` `K 101828`; `sprite 0 0 32 32 normal contain` → `face-moods` | random glance timing becomes a fixed 8-frame cycle; acceptable |
| `blinky-eyes-2d` | scene **Watching You** | `color` `K 000000`; `sprite 3 12 0 0` → `eye-blink` (upgraded); `sprite 18 12 0 0 … flags 3` → `eye-blink` | the mirrored copy is one sprite; random darts become frames |
| `scary-pumpkin` | scene **Jack-o'-Lantern** (#19) | as #19 | fire through the carve reads better than the port's swirl tunnel |
| `orv-christmas-tree` | sprite `christmas-tree` + scene **Silent Night** (#20) | as #20 | the port is a static picture with particles; the sprite + snow tile is cheaper and nicer |
| `heart` (beating rainbow heart) | sprite `heart-mask` + scene **Rainbow Heart** | `sprite 0 0 32 32 normal fill` → `heart-mask`; `pat rainbow` full `P y` `multiply 100` | multiply turns the white beating heart into a vertical rainbow; black outside. Delete the pattern once the scene ships |
| `nyan-lights` | scene **Rainbow Runner** + sprite `cat-runner` | `pat starfield-2d` full; `pat rainbow` box `0 11 14 6` `P y` (clip = trail); `sprite 12 9 0 0` → `cat-runner` | the original is a recognisable commercial character (pop-tart cat) — replace with an ORIGINAL cat; IP risk otherwise |
| `static-christmas-lights-4-colors` | sprite `bulb-string` + scene **Bulb String** (#28) | `sprite 0 0 64 64 tile` (matrix today) | strip use waits on G1; keep the pattern only until then |
| `rainbow-flag` | sprite `pride-stripes` + scene **Pride** | `sprite 0 0 32 32 normal fill` → `pride-stripes`; optional `pat wavy-bands` grey ramp `multiply 60` for cloth shading | the pattern is a static 6-band fill — lame on its own |
| `bulk-sprite-scroll-2d` | keep ONLY as a `blit()` API teaching example | — | users wanting a picture should be pointed at Sprites, not this |

### Keep as patterns, but add a companion scene
* `rgbclock-2d` / `2d-clock-with-hand-color-pickers` (dedupe to one analog clock) →
  scene **Analog + Date**: the clock full, `color 0 26 0 6 normal 60` band,
  `text 0 26 0 0 T clock MM-DD F tiny ffffff c none 0`.
* `us-flag-2d` — the cloth wave is per-pixel geometry a sprite cannot do; it is the
  base of scene 22.
* `blue-holiday-candle-2d` — procedural sway beats 4 frames; Candle Row covers the
  "many candles" case.
* `crawling-spider-2d` / `dire-spider-2d` / `butterfly-2d` — these are SPRITE
  characters on a PATH. Today keep (dedupe the spider pair); after G2 they become a
  walk-cycle sprite + a motion line, which is far cheaper than per-pixel capsule SDFs.
* `eye-of-sauron` / `…-with-movement` — keep one procedural flame eye; the name is a
  trademarked-franchise reference worth softening (e.g. "Fire Eye").

### Better as a ramped BASE LAYER than alone
These are luma-rich but either garish, single-hue, or one of a colour-variant family
that a ramp collapses. In the scene world, ship ONE of each family and let the ramp
(and scene examples) carry the colour.

| pattern | alone today | as a base |
|---|---|---|
| `interference-2d` | hue-cycling rings, busy | the best ramp source in the library: dark seams, smooth crests (scenes 2, 5, 16) |
| `ripples-2d` | faint cyan rings | sparse bright rings → `add` or `mask` source (5, 8, 31) |
| `metaballs-of-fire-2d` | one orange look | any lava lamp by ramp (6) |
| `doom-fire-2d`, `perlin-fire`, `palette-fire-2d`, `fire-red`, `fire-blue` | five fires, two differing only by colour | keep the best fire or two; colour by ramp (7, 9, 19). `fire-red`/`fire-blue` → merge |
| `digital-rain-2d`, `matrix-green-waterfall-2d`, `matrix-rain` | three green rains | one rain + ramp ("gold rain", "pink rain") |
| `voronoi-2d` (orbs style) | primary-colour cells | a stained-glass / cell `mask` (9) |
| `radar-2d` | green radar | a sweep `mask` for reveals (10) |
| `starfield-2d` | dim white specks | THE night-sky base (1, 17, 18, 20); ramp sets the sky colour |
| `fireflies`, `colourful-fireflies`, `slowflies`, `xmasflies` | four firefly variants | one firefly + ramp + `add` (11, 17) |
| `twinkle`, `twinkle-2`, `color-twinkles`, `halloween-color-twinkles`, `glitter` | five sparkles, mostly colour variants | one sparkle source; `add` + ramp (16, 23) |
| `wavy-bands` / `halloween-wavy-bands` | colour variants | one + ramp; also a grey cloth-shading `multiply` source |
| `reaction-diffusion-2d` | pink only | luma-rich; any palette by ramp |
| `breathing-gradient` | 1D OKLab breathe | `P y` sky backdrop (4, 11) |
| `1-white-fade`, `2-purple-fade`, `2-colors`, `christmas-rg-fade`, `color-fade-pulse`, `twocolorhsvmix`, `color-blend`, `crossfading`, `slow-color-shift` | a family of "fade between N colours" strip patterns | one greyscale breathe/sweep source + a ramp per colour pair. On STRIPS this needs G1; until then the honest dedupe is one pattern with colour-picker controls |

Rule of thumb for the rewrite agents: a pattern whose main use is as a base should
render a luma-rich, low-saturation (ideally a "mono" toggle) output and leave colour
to the ramp; constant-brightness rainbows (`bulk-rainbow`) ramp badly.

Not a scene matter, but spotted on a contact sheet: `swirlpool-2d` frames read as
hooked-cross shapes at 16×16 — worth a look in the visual audit.

---

## 4. Gaps — what these examples want and the scene system cannot express

Ranked by how many examples above they unblock.

* **G1. Scenes on strips (and any non-grid map).** `skip()` + `detect_grid`
  (`w,h ≥ 2`) mean a strip composites to black, and the console hides the tab.
  Treating a 1D map as a W×1 grid in index order would make boxes = segments,
  `flipx` = reversed segment, tile/fill 1-row sprites, ramps, masks and `add` all
  work — a WLED-segments equivalent for free, and it lets the 9-pattern "two-colour
  fade" family collapse into ramps. Text simply clips away. Biggest single win for
  the 60–300 px strip audience.
* **G2. Layer motion ("scripted layer positioning").** No way to move a box: a
  sprite cannot walk across scenery, a rocket cannot fly, a spider cannot crawl. A
  minimal `M <vx> <vy> <wrap|bounce|once>` line (px/s, applied by the compositor
  clock like scroll) covers most of it; keyframed paths or a scripted layer
  (a tiny pattern that outputs `x, y, frame, opacity` per layer) cover the rest.
  The cheap half of this — **scroll on a tiled sprite** (reuse `F`'s scroll/speed
  semantics) — gives parallax and snow/rain without loop-tile frames.
* **G3. Relative / anchored boxes.** Boxes are absolute pixels; library scenes are
  authored in a 32×32 corner and look lost on 64×64 and tiny on 128×128. Percent
  units or an anchor (`c`, `b`, `tr`…) plus "`w=0` on a tiled sprite = whole
  layout" would let one scene file fit every panel.
* **G4. Big text.** The largest face is 8 rows: on 64×64 a clock is a sliver, on
  128×128 unreadable. A scale factor on `F` (×2–×4) or a large-digit / 7-segment
  face. Also a `°` glyph (temperatures) — fonts are ASCII-only.
* **G5. Stencil semantics for text and sprites.** `mask` on a text layer is a no-op
  (forced black key), contrary to proposal §5.5's "rainbow-filled letters"
  promise — the doc should be corrected and the feature either built (in `mask`,
  keyed-out pixels go BLACK instead of passing through, or a new `stencil` blend)
  or the multiply recipe (scene 15) documented as the way.
* **G6. Clone layer.** A layer whose source is another layer's frame (own box,
  flips, ramp, blend; no engine). Mirrors, kaleidoscopes, glow halos, a mirrored
  Strip Zones fire, Hard vs Soft Key — all without spending the 2-engine budget.
* **G7. Data-driven layers.** Weather Card wants the glyph chosen by data
  (a "sprite slot", or slot text → sprite-name lookup); per-layer visibility
  settable via API/MQTT/HA (show the doorbell layer on ring); number formatting
  for slots.
* **G8. More text sources.** Countdown / days-until (`T until 2026-12-25`),
  weekday names, AM/PM, a blinking colon.
* **G9. Time-varying layer parameters.** Opacity keyframes or LFOs (neon flicker,
  fade a layer in and out, crossfade two sprites), time-of-day schedules (night
  clock dims after 22:00).
* **G10. Per-layer speed / phase / seed for pattern layers.** Two copies of one
  pattern are either identical or need the pattern to expose a control; a scene
  can't say "run this 0.5×" or "offset by 3 s".
* **G11. Sensor-bound layer parameters.** Opacity ← `energyAverage` would make any
  layer music-reactive without spending a pattern layer (Pulse to the Beat). And
  no board's mic is wired (Athom mic unused, Seengreat #142), so every music scene
  depends on an external `/api/sensors` feed.
* **G12. Native generator layers.** Engine-free `twinkle` / `snow` / `rain` /
  `starfield` layer types with a colour and a density would relieve the 2-pattern
  cap that most compositions here bump into (Aquarium and Campfire both spend
  their second engine on a sparkle).
* **G13. Sprite colour cycling / ramp on sprites.** Palette rotation over frames
  (classic water/fire colour-cycling) would make tiny animated sprites far
  cheaper than extra frames; a per-layer palette swap would let one `fish` or
  `bat` ship in several colours.
* **G14. Text legibility.** An outline/shadow option on `F`; today it is 2–4
  stacked offset text layers, with the non-black-shadow trap.
* **G15. Scene-store budget.** 3840 B for ALL of a device's scenes; ten library
  scenes fill it. Either raise it on the 16 MB boards or teach `+ Add` to say
  how much of the budget a library scene will take.
