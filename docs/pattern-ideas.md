# Pattern ideas: research backlog for example patterns

A survey of what the wider LED ecosystem ships as effects, filtered down
to ideas that (a) are not already in the corpus (288 compile-clean PB
community patterns) or our examples, and (b) would show off Luxel builtins
well. Write originals — take the *idea*, not the code.

Sources surveyed (2026-07-06):

- **WLED** (~180 effects incl. the v16 particle systems):
  <https://kno.wled.ge/features/effects/>
- **FastLED classics** by Mark Kriegsman (Fire2012, Pacifica, TwinkleFox,
  Pride2015, SoftTwinkles): <https://gist.github.com/kriegsman>
- **pixelmatix Aurora** (SmartMatrix art display: flock, flowfield,
  pendulum wave, radar, spiro, munching squares, life):
  <https://github.com/pixelmatix/aurora>
- **QMK RGB Matrix** (keyboard effects, incl. the keypress-reactive
  family): <https://docs.qmk.fm/features/rgb_matrix>
- RGB-keyboard middleware conventions (Razer Chroma / Corsair iCUE /
  SignalRGB / OpenRGB): wave, visor, starlight, splash-on-keypress,
  typing heatmap, temperature/ambient-driven color.

**Status 2026-07-06:** the entire shortlist (1–12) is implemented in
`examples/` + the playground gallery, plus stretch items Tetrix, falling
sand ("Hourglass" reimagined as a pour), TV simulator, Soap, Spirograph,
and — successfully, in 16.16 fixed point — Gray–Scott reaction–diffusion.
Skipped deliberately: Popcorn (mechanically a Drip duplicate) and munching
squares (corpus already has `xorcery 2D/3D`). Builtin wants that fell out
of the batch are recorded in ideas.md (blur2D, bulk array math, event
injection).

## Shortlist — next example batch

Ranked by visual payoff ÷ effort, all feasible with current builtins.

1. **Boids 2D** (Aurora "flock") — real flocking: ~8 agents, parallel
   arrays for pos/vel, `dist`/`dot`/`angleBetween` for the three rules,
   `feedback` trails. Nothing like it in the corpus; the best possible
   showcase for the vector builtins.
2. **Flow field 2D** (Aurora "flowfield") — particles advected along
   `simplex2` directions (sample noise → angle → step), trails via
   `feedback`. Organic, endless, gorgeous.
3. **Typing heatmap 2D** (QMK) — simulated keystrokes deposit heat at
   random cells; heat spreads to row-neighbors (`blur1D` per row) and
   cools. Blue→red→white palette. Reads as "a keyboard being typed on".
4. **Pendulum wave 1D** (Aurora / physics classic) — N pendulums with
   graduated periods drawn as glowing dots; they drift out of phase into
   traveling waves and snap back into unison. Pure `sin`, mesmerizing.
5. **Interference 2D** (demoscene) — two or three moving wave sources,
   `wave(dist·k)` summed; moiré fringes everywhere. Great `oklch` demo
   (smooth fringe gradients).
6. **Crosshair pulse 2D** (QMK Reactive Cross/Nexus) — random "keypress"
   fires a pulse racing outward along that cell's row and column, fading.
   Trigger control doubles as a manual key-hit.
7. **Drip 1D** (WLED) — droplet swells at the top, detaches, accelerates
   under gravity, splashes at the bottom. Tiny state machine; charming.
8. **Ocean 1D** (Pacifica-inspired, original construction) — 3–4 layered
   scrolling waves (different speeds/scales) summed through a deep
   blue-green palette, whitecaps where the sum crests. The corpus has no
   good ocean.
9. **Starfield 2D** (demoscene) — stars fly outward from center with
   perspective (x/z, y/z), respawn at depth; warp-speed slider.
10. **DNA helix 2D** (WLED) — two phase-offset sine strands with rungs,
    depth-dimming the "far" strand. Iconic on matrices.
11. **Radar sweep 2D** (Aurora) — rotating beam via `angleBetween`,
    persistence via `feedback`, random blips that light when swept.
12. **Chevron 2D** (QMK moving chevron) — `abs(x−0.5)` V-shaped rainbow
    wavefront. Trivial — good as the "first render2D" teaching example.

## Second tier / stretch

- **Tetrix 2D** (WLED) — falling stacking blocks; fun but stateful-fiddly.
- **Hourglass 2D** (WLED PS) — falling-sand cellular rules; medium effort.
- **Reaction–diffusion 2D** (Gray–Scott) — stunning, but ~5 array ops ×
  256 cells × 2 grids per frame; prototype in playground first, may be
  too slow on-device at 16.16 without a dedicated builtin.
- **TV simulator** (WLED) — ambient light-spill imitation; practical
  (people actually run this nightly), low glamour.
- **Soap / distortion waves** (WLED 2D) — noise-warped coordinates
  (`simplex` fed into `translate`); hypnotic.
- **Popcorn 1D** (WLED) — kernels launched with gravity; overlaps Drip.
- **Spirograph 2D** — parametric epicycles with trails; corpus has a
  Lissajous tracer, this is the fancier cousin.
- **Munching squares** — corpus has `xorcery 2D/3D` already; skip unless
  doing a faithful XOR-classic teaching example.

## Ideas that imply engine/builtin work (fold into docs/ideas.md)

- **`blur2D(arr, w, h, radius)`** — heatmap, soap, and reaction-diffusion
  all hand-roll row/column blurs over a `pixelCount` buffer. The 2D
  sibling of `blur1D`. [S/M]
- **External event injection** — the whole QMK "keypress-reactive" genre
  (splash, nexus, heatmap) needs *events*. Trigger controls emulate one
  button today; a small websocket/API event queue (`eventCount()` /
  `nextEvent(out)`) would let HA/MQTT/keyboards drive patterns later.
  Pairs with the M4 integration work.
- **Noise with analytic derivatives** — flow field advection is the
  strongest consumer yet for curl/gradient noise (already deferred in
  ideas.md).
- **Temperature/sensor-driven color** (iCUE convention) — falls out of
  the M5 sensor framework; a "thermometer" example belongs with it.
- **Screen-ambient / music modes** — covered by existing ideas.md items
  (DDP input, I2S FFT); keyboards confirm these are table stakes
  ecosystem-wide.

## Clean-room reimplementations of corpus patterns

The scraped corpus has unknown licensing, so its best patterns are being
reimplemented clean-room: a describer agent reads the original and writes
a functional spec (prose only — no code, no identifier names, no copied
constants), and the implementer writes fresh code from the spec without
ever seeing the source. Each reimplementation notes this provenance in
its header comment.

Done 2026-07-06 (specs + implementations, all verified in the VM):
Edgeburst, Rainbow Melt, Color Twinkles, Thunderstorm, Glittering
Jewels, Doom Fire 2D, Voronoi 2D, Kaleidoscope 2D, Coronal Ejection 2D,
Unstable Orbits 2D. The gallery dedupes by name, so these shadow their
corpus originals in the pattern browser.

Worthwhile future clean-room candidates (not yet spec'd): the sound-
reactive set (blocked on mic hardware anyway), Cellular Automata 1D,
Slime mold palette, Swirlpool 2D, block reflections, fireblobs,
Mandelbrot 2D. Once coverage feels sufficient, the corpus can be dropped
from the gallery entirely and kept only as a private compile-compat test
battery.

## What we already cover (don't duplicate)

Fire (5+ variants incl. Doom Fire), matrix rain, plasma, kaleidoscopes,
metaballs, Mandelbrot, cellular automata (1D + cyclic 2D), lightning,
bouncing balls, fireworks (4+), twinkles/sparkles (10+), voronoi, sunrise,
clocks, KITT/scanners, theater chase, ripples, aurora curtains, spirals,
polar/tunnels, comet trails, flag/holiday themes, snake, Lissajous.
