# Clean-room port verification sweep — aggregate findings

Output-only verification of all 293 paired patterns (original `.epe` vs
clean-room port), judged purely from rendered output by independent Opus
judges following `JUDGE.md`. Per-pair detail lives in `results/<slug>.json`
(each carries measured observations, per-dial comparisons, and actionable
`feedback[]` for a fix pass). This file synthesizes the SYSTEMIC defect
classes that recur across pairs, so the fix pass can work family-by-family
instead of pair-by-pair.

## Headline numbers

| verdict | count | share |
|---|---|---|
| match | 24 | 8% |
| close | 119 | 41% |
| divergent | 123 | 42% |
| broken | 17 | 6% |
| orig-unrenderable | 10 | 3% |

Mean score 5.42/10 (excluding orig-unrenderable). Score histogram peaks at
4-7: most ports get the pattern's *identity* right and fail on constants,
lifecycle, or control mapping.

Perfect/near-perfect ports (byte-identical or LSB-level): `rainbow`,
`rainbow-melt`, `rainbow-pinwheel`, `marching-rainbow`,
`three-red-pixels-mathy`, `wichmannhill-prng` (same PRNG bit-for-bit),
`policelights`, `rgb-test-pattern`, `static-christmas-lights-4-colors`,
`the-grinch`, `snake`, `rainbow-fonts`, `neutronorbit`, plus 11 more.

## Systemic defect classes (fix these as families)

### 1. PB time-base constants (the single biggest family)
Originals overwhelmingly derive periods from Pixel Blaze `time(k)` =
k×65.536 s sawtooths; ports rounded, re-derived, or replaced the constant.
Signature: measured original periods land exactly on k×65.536 (6.5536,
13.107, 19.66, 26.2, 32.77, 65.536 s...) while the port's land elsewhere.
Fix = restore the single constant; the geometry usually already matches
under a pure time rescale (judges repeatedly proved orig(t) == port(k·t)
to within a few LSB).
- Exact rescale proven: `sierpinski-rainbow-2d` (2.00×), `spin-cycle`
  (0.065 vs 0.1), `opposites` (0.9008), `marching-rainbow-buffered`
  (0.07 vs 0.1), `rainbow-fonts-2` (0.3 vs 0.2), `rgbw-mapping-tester`
  (arbitrary 8.72 s vs 6.5536), `rgbw-mapping-tester-hsv-version`
  (0.08 vs 0.1 — note the two siblings err in OPPOSITE directions),
  `xorcery-2d-3d` (TWO timers wrong by DIFFERENT factors: 0.06/0.3 vs
  0.1/1.0 — no single rescale fixes it), `utility-perceptual-hue`
  (65.536×0.05 vs exact 4.000 s real time), `stairmaster-2d` (1.33×),
  `tunnel-of-squares-2d` (colour timer 1.25×), `matrix-green-waterfall-2d`
  (envelope exactly 3×), `matrix-2d-pulse-edit` (exactly 1.5× + double
  phrase length).
- Rate quantization: `solid-rainbow`'s original snaps speed to
  floor(65·v²) multiples of 1/65.536 (frozen below v≈0.12); the port is
  continuous. Check other speed dials for the same PB-unit stepping.
- Order-of-magnitude slow (noise-field subfamily): `metaballs-of-fire-2d`
  (~16-25×), `perlin-fire-wind` (~15-20×), `perlin-simplex-noise-1d`
  (24.75×), `perlin-simplex-noise-2d` (3×), `nano-orbital` (~60× — smells
  like a /60 unit slip), `voronoi-2d` (~15×), `spiral-twirls-star-2d`
  rotation (10×), `oasis` (10-35×).

### 2. Frame-stepped vs time-based mismatches (both directions)
Some originals advance per RENDERED FRAME, some per second; ports often
guessed wrong. Either direction is a real bug (they agree at exactly one
fps).
- Original frame-stepped, port time-based: `meteor-shower` (1 px/frame),
  `scrolling-text-marquee-2d` (1 font cell/frame), `twinkle`,
  `twinkly-stars`, `thunderstorm` lightning (21-frame storm tick),
  `saberdeploy-tutorial`, `sound-spectromatrix-agc` transient clock.
- Port frame-coupled where original is not: `rocket-by-tony-hampton`
  exhaust, `sound-rays` trail width, `twinkle-2` ignition (per-frame
  probability), `sparkfire` (both coupled, port 3× more headroom).
- Pathological: `sound-reactive-color-fade`'s beat detector keeps its
  history in ~12 render frames and produces NOTHING below 24 fps.
- Hybrid: `voronoi-2d` port clamps its per-frame step at ~0.05 s
  (frame-stepped below 20 fps, time-based above).

### 3. Missing lifecycle management (most of the `broken` bucket)
Ports that draw the right thing and never retire/clear/decay it, so the
rig saturates and freezes; or the mirror image, populations that die out.
- Accumulate-forever/saturate: `portal`, `scrolls` (no length clamp AND no
  global reset), `slowflies`, `sparks-center`, `nano-orbital`,
  `wanderers` (trails never erased + unvisited cells painted red),
  `sound-rays-frequency-bpm-reactive-1` (amplitude accumulator with no
  equilibrium), plus the earlier *shivers family, `bouncing-balls-hsv`,
  `bustle`, `amoeba`, `eye-of-sauron`.
- Population dies out: `wanderedges` (absorbed at edge 0, no respawn).
- Missing retirement: `novas` (original hard-kills via a bounded slot
  pool, even emptying entirely; port fades forever, 47% brighter).
- Resource bug: `perlin-fire` port allocates arrays per frame → engine
  'array element budget exceeded' crash at absolute frame 512.

### 4. Control-surface drift (touches well over half of all pairs)
Four recurring sub-patterns; per-pair detail in each verdict's `dials[]`.
- RENAMES (~30+ pairs): saved values and shared `--controls` strings no
  longer bind. Often systematic (Segment→Zone, Speed↔Period meaning
  inversion in `twocolorhsvmix`, DoRotate→Rotate...).
- WRONG QUANTIZATION DIVISOR on mode/step dials: port floor(v×4) vs
  original round(v×3) (`perlin-fire`, `perlin-fire-wind-tunnel`,
  `traffic`), /13 vs /12 (`real-world-lights`), 6 bands vs 5 (`stacker`
  Segments), 6 vs 5 levels (`stargen` matched; `spotlights` Speed 3 steps
  vs 2), floor(v×6) vs floor(v×4) (`perlin-simplex-2d`), boundary-at-0.5
  strictness off-by-one on several two-state dials.
- WRONG CURVES/RANGES: inverted direction (`midpointdisplacement1d`
  Roughness, `thunderstorm` density, `swirlpool-2d` ColorSpeed,
  `snake` Speed label), linear vs hyperbolic (`midpointdisplacement1d`
  Speed), exponential-with-floor vs linear (`stacker` Speed),
  range-compressed so the original's default is unreachable (`scanner`,
  `rainbow-comet`, `utilitycolortemp`, `twocolorhsvmix` envelope = orig^8).
- WRONG UNTOUCHED DEFAULTS (extremely common, cheap to fix): the port's
  untouched render equals a different dial value than the original's
  (judges pinned many exactly by byte-identical dial matching — e.g.
  `zoom-kaleidoscope` ZS 0.5 vs 0.9, `solid-rainbow` 0.5 vs 1.0,
  `midpointdisplacement1d` PaletteWidth 1.0 vs 0.15, `tixy`,
  `white-rainbows`, `stacker` three defaults, `xmasflies` mix weights).
- INVENTED or DROPPED dials, and DEAD dials (`multisegment-demo` 6 of 8
  dead; `spiral-twirls-star-2d` TwistSpeed; `sound-rays-bpm`
  BpmSpeedFactor; `static-christmas-lights` inert BlockSize).

### 5. Coordinate/units errors
- Normalized-vs-pixel: original uses absolute pixels, port strip-fraction
  or vice versa — `matrix-2d-honeycomb` (pixel-locked ~4.6 px cells vs
  canvas-scaled), `scanner`, `snake` (fixed 10 px tail vs 15% of strip —
  the two laws intersect at the default rig!), `sparkfire` (fixed heat
  budget vs per-pixel), `twinkle-2` (per-frame vs per-pixel ignition
  normalization), `time-flies-2d` glow.
- Fixed logical lattice: `swirlpool-2d` and `unstable-orbits-2d` quantize
  to 16 steps regardless of rig; `scrolling-text-marquee-2d` glyphs at
  half scale (16 vs 8 font cells across).
- Spatial frequency scale: `sunset` (phase step 6-10× too big → aliases),
  `regenbogendrogen` (exactly 2× — proven pixel-exact via cross-rig
  alignment), `zoom-kaleidoscope` (2×), `sound-spectrum-analyser`
  (columns compressed 1.5×), `sun-rays-through-trees` (2-3 beams vs 20+).
- Walk in wrong space: `wanderers` walks (x,y) where the original walks
  the flat index ±1; `upward-waves-3d` wave on the wrong lattice axis.
- Half-pixel sampling conventions: i/n vs i/(n-1) ramps (`solid-rainbow`),
  pixel-centre vs pixel-edge (`rainbow-fonts`), odd-vs-even stack parity
  (`stacker`), `>= 0.5` vs `> 0.5` midplane strictness
  (`rgb-xyz-3d-octants` — a one-character fix).

### 6. Colour constant families
- EXACT-THIRDS vs DECIMALS: originals often use hue 0.30/0.60/0.33/0.66
  (NOT 1/3, 2/3); ports substituted exact thirds — `multimap-simpledemo`,
  `rgb-xyz-3d-sweep`, `the-grinch`, `time-flies-2d` (k/6 vs k/7),
  `rgbw-mapping-tester-hsv-version` (inverted case: orig decimals).
  A ≤10/255 delta each, but it's the difference between match and close.
- CHRISTMAS TEMPLATE (confirmed ~6×): signature green **51,255,0**
  (hue 0.30), neutral grey fade targets (~114,114,114 family), blue with
  a small green lift **0,10,255**, deep-orange 255,45,0 and red-pink
  255,0,77 (not amber/magenta). `xmasflies`, `orv-christmas-tree`,
  `twinkling-classic-xmas-strands`, `twinkly-stars`, `the-grinch`,
  `spring-colors` all show pieces of it.
- Hue offsets/rotations: `opposites` (−100°), `tree-setup-pattern`
  (exact half-turn in the angle→hue origin — functionally serious for a
  calibration pattern), `quiet-blinkfade` (+0.05), `traffic` (−45°),
  `sound-lavablob` (invented global rotation to delete),
  `sound-spectrum-analyser`/`sound-music-spectrum-visualizer` (hue term
  negated and/or halved).
- Saturation/value: desaturation (`rainbow-comet` head 0.35 vs 0.88,
  `perlin-fire-wind-tunnel` curve ~2× too aggressive) and over-saturation
  (`twinkly-stars` 0.30 vs 0.20), white-clipping
  (`newfire`, `sound-spectro-kalidastrip` 16% white pixels,
  `spotlights` never reaching 255, `white-rainbows` 255 vs 234 head),
  inverted brightness/saturation ladder (`sound-starburst-2`),
  official-hex vs hsv(h,1,1) approximations (`rainbow-flag`).
- Unit slips: `rainbow-rocket-sparks` trail hue ÷6 (0..1 fed into a 0..6
  hue space).

### 7. Direction/sign flips (~14 confirmed)
Single-sign fixes: `pride-progress` (centre-out vs edges-in),
`spinwheel-2d` (rotation), `wavy-bands` (vertical travel),
`millipede-1d-2d-controls` (rotation, magnitude exact),
`ryb-colors` (wheel selector swapped end-for-end — the looks themselves
byte-identical), `sunrise`-family, `sound-rays` hue gradient,
`sunset`/`sound-spectrum` hue direction, `saberdeploy` toggle polarity,
`policelights` initial toggle (cosmetic), `tree-setup-pattern` (both axes
via the half-turn), `rainbow-v2` Direction semantics.

### 8. Structural/waveform subtleties
- Continuous term quantized: `millipede` speed modulation square-wave vs
  sinusoid; `traffic` LineWidth scaling the whole kernel vs the glow.
- abs() of a signed waveform: `radiant-pulse-3` ray-doubling signature.
- Missing halo/plateau/floor: `time-flies-2d` glow ring, `twinkle`
  pop-and-hold envelope, `sound-spectrum-analyser` 3-row floor band,
  `sparks` distance drag/falloff (strip-fraction law, dies at ~90%).
- Missing secondary cycles: breathing/super-cycles absent in ports
  (`spinwheel-2d` 13.1 s, `swirlpool-2d` 62 s refill, `sound-rays`
  multi-minute build-up, `stargen` carousel 2.5×).

## Engine gaps discovered by the sweep (tracked in Gitea #84, #99)

1. **Time-of-day builtins return constants** regardless of
   `lx_set_wall_clock` — CONFIRMED (`pixelclock` byte-identical across 11
   wall clocks). Seconds-level time DOES advance (`rgbclock-2d` original's
   seconds hand works). Blocks: `pixelclock`, `naturallightsync`,
   `sunrise-alarm-clock` (partial), `utility-scheduled-percent-on-demo`
   (total collapse). Re-judge all four after the fix.
2. **Init-time/top-level randomness returns constant 0** (suspected,
   two independent corroborations): `static-random-colors` renders solid
   hsv(0,1,1); `synchronized-random-numbers`'s original renders zero
   randomness with a drift whose RATE AND SIGN depend on the frame delta.
   Per-frame randomness demonstrably works (seed changes other originals).
3. **Out-of-range array/pixel writes hard-error** where real PB tolerates
   them: `nano-orbital` (needs ≥144 px), `orv-christmas-tree`
   (pixelCount%20), `rainbow-comet` (one-shot error at frame 982),
   `rainbow-smiley` (≥32×32), `tixy` (walks off its formula table after
   ~46 modes → 'call of a non-function value').
4. **32.768 s (2^15 ms) freeze family**: `fire-blue`, `fire-red`,
   `spring-colors` originals freeze byte-identical for exactly 32.8 s on
   repeating cycles with fps-dependent onset — a 16.16 wraparound
   somewhere in the engine.
5. **Silent-null originals** (subtype c): `automap`, `coral-plasma`,
   `fast-palette-blending`, `performance-test-framework`,
   `skypirate-s-centered-spectrum`, `slime-mold-palette`.
6. **Sentinel-tripwire .epes** (corpus-prep, NOT engine): both
   music-sequencers ship author-planted invalid lines — strip and
   re-judge (Gitea #99).
7. **Array element budget** ('arrays are never freed') caps persistence
   patterns at large rigs (`unstable-orbits-2d` original at 96×96) and
   kills the `perlin-fire` port.

### Addendum 2026-08-23 — gaps 1 & 2 root-caused and fixed (Gitea #104, #105)

Gap 1 was TWO stacked bugs, both fixed:
- **Harness**: snap.mjs's main render (and `--probe-controls`) built its
  per-side options without `wallClock`, so `--wall-clock` was recorded in
  meta.json but never applied — every render in the whole sweep ran at
  epoch 0 (`setWallClock(undefined)` → NaN → 0 through the f64→i64 cast).
  This is why output was "byte-identical across 11 wall clocks". The
  hosts now throw on a non-finite clock instead of rendering 1970.
- **Engine**: the wall clock could not reach top-level init at all (init
  runs inside engine construction; `set_wall_clock` only lands before the
  first frame). All hosts now hand the clock to the engine constructor
  (`Engine::new_at` / `lx_set_default_wall_clock`), so top-level
  `clockHour()`-family reads see real time-of-day, like on a PB with RTC.
Cross-clock renders of `pixelclock` now differ on both sides. Wall-clock
verdict observations from the sweep describe epoch-0 renders; only the
four flagged clock patterns need re-judging on that account.

Gap 2 was not init-specific: `random(max)` clamped a NEGATIVE `max` to 0,
and `random(0xffff)` is `random(-1.0)` — 16.16 literals wrap identically
on PB (documented in 04-oracle-findings.md). Oracle probe (fw 3.67,
2026-08-23): PB draws over the whole signed range for negative `max`
(measured ±32760 for both `0xffff` and `-5`). `scale_random` now takes
the max's raw word unsigned, PB-exact; positive `max` is unchanged.
`static-random-colors` renders 59 distinct colors (was solid hue-0);
`synchronized-random-numbers` shows real motion on both sides.

## Recommended fix-pass order

1. Engine gaps 1-4 first — they block ~12 re-judges and taint several
   reference measurements.
2. The time-base family (§1): highest count, usually one-constant fixes,
   verified cheaply by the rescale-fit numbers in each verdict.
3. Lifecycle family (§3): converts most of `broken` to `close`+.
4. Untouched-default corrections (§4): trivially cheap, big visible wins.
5. Colour constants (§6) incl. the Christmas template as one batch.
6. Sign flips (§7): one-character fixes with exact acceptance numbers.
7. The remaining per-pair items in each verdict's `feedback[]`.

Every verdict file carries concrete acceptance numbers (target periods,
px/s, RGB triples, histograms) so fixes can be validated with the same
harness (`tools/verify/snap.mjs`) without re-deriving anything.
