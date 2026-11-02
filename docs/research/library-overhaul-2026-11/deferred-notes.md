# BF notes (collected from batch reports; the final batch acts on these)

## Superseded lines to prepend (spec outside the batch's ownership)
- 3d-rotation-spotlights.md, spotlights-rotation-3d.md → library/searchlights.js
- lissajous-curve-tracer.md, swirlpool-2d.md → library/spirograph-2d.js
- angle-and-radius-from-coordinates.md → library/radar-2d.js

- sunrise-2d.md → library/coronal-mass-ejection.js; eye-of-sauron-with-movement.md → library/fire-eye.js (L20)

- crawling-spider-2d.md → library/dire-spider-2d.js (L22)

- 2d-bouncing-additive-primaries.md, bouncer3d.md, bouncing-balls-hsv.md, bouncing-balls-rgb.md, bouncing-rgb-balls-2d.md → library/bouncing-balls.js; time-flies-2d.md → library/boids-2d.js (L10)

- colourful-fireflies.md, slowflies.md, xmasflies.md → library/fireflies.js (L16)

- upward-waves-3d-using-accelerometer.md → library/glowflow.js (L09)

- sinus.md → library/dna-helix-2d.js; halloween-wavy-bands.md → library/wavy-bands.js; matrix-2d-honeycomb.md, matrix-2-tone-pulse.md, sunset.md → library/plasma.js (L11); docs/pattern-ideas.md:7 names interference-2d.js

- bustle.md, novas.md, scrolls.md → library/portal.js / crosstown-traffic-2d.js; metaballs-of-fire-2d.md, sound-lavablob.md → library/lava-lamp.js (L30)

- perlin-simplex-noise-1d.md → library/topo-map.js (L12)

- 2d-clock-with-hand-color-pickers.md, pixelclock.md → library/analog-clock.js; sunrise.md → library/sunrise-alarm-clock.js (L19)

- golden-tix.md → library/tixy.js; spiral-twirls-star-2d.md → library/2d-spiral-twirls.js; zoom-kaleidoscope.md → library/kaleidoscope-2d.js (L13)

- blue-holiday-candle-2d.md → library/candle.js; blue-holiday-star-2d.md → library/holiday-star.js; rainbow-flag.md → library/us-flag-2d.js (L21)

- ice-floes-2d.md → library/voronoi-2d.js; neutronorbit.md → library/orbits.js (L29)

- cellular-automata-1d.md → library/cellular-automata.js; cyclic-cellular-automata-2d.md → library/life.js; sierpinski-rainbow-2d.md → library/xorcery-2d-3d.js (L25)

- 4th.md, firework-nova.md, firework-rocket-sparks.md, rainbow-rocket-sparks.md → library/fireworks.js; sparks-center.md → library/sparks.js (L06)

- naturallightsync.md, real-world-lights.md, single-color-picker-wide-or-spot.md → library/lamp.js (L02)

- audio-volume-meter.md, sound-music-spectrum-visualizer.md → library/sound-spectrum-analyser.js; light-organ-sensor-board.md → library/light-organ-2-0.js; sound-rays-frequency-bpm-reactive-1.md → library/sound-rays.js; sound-spectro-kalidastrip.md, sound-spectroblots-pow-fade.md, sound-spectromatrix-agc.md, sound-spectromatrix-render2d.md → library/spectrum-field.js; white-rainbows.md → library/starburst.js (L17)

## Stale references to clean
- tools/verify/results/*.json for deleted stems; tools/verify/pairs.json, decisions.json, gen-pairs.mjs name deleted slugs (B00, L14)
- tools/oracle/alias-probes.mjs comment describes slime-mold-palette's old runtime render2D switch (dropped in L26 rewrite)
- docs/boards.md quotes deleted files as history (leave)

## Engine tickets to file
- fade(k) floors 8-bit channels → long trails cap; want dithered/16-bit accumulating fade (L14)
- drawLine walks the whole bounding box with a per-pixel divide; a row-span rasterizer (L14)
- fillGradient in coordinate space is ~6x slower than a 1-row fillCanvas (0.21 vs 0.037 ms at 128x128) (L24)
- fillTriangle(x0,y0,x1,y1,x2,y2,mode) builtin — Glenz/filled-face mode for wireframe, asteroids, gears (L31)
- arrayReplace arg-count limit ("too many arguments" at 24) undocumented in docs/lang.md (L31)
- E3 scenes on strips (product rule; ask Jeremy); director builtins; clip layers (if B's PR2 lands, drop); palette system; kaleido folds; drawSprite; fBm fillNoise; #373/#453 evidence; text scale/num; per-layer speed; projection hint; scene gaps G7–G15; beat builtins; dither + row-glitch post stages (PLAN §D.2)
- Live-pinned by design: crates/luxel-cli/src/serve.rs include_str!s library/rainbow.js (rainbow keeps its slug)

## E1/E2 upgrade call sites (after BE1 merges)
- bouncing-balls.js strip(): fillRange tail + ball1D loop → fillRangeAA / splatIndex(…,1) (L10)
- fireflies.js renderBulk strip branch: ghost setPixel, core fillRange, core setPixel → splatIndex (L16)
- caustics.js: paintCanvas → filter 1; strip() paint+fillRange per cell → gradientRange (L09)
- raindrops-2d.js: paintCanvas+setBlur(0.3) → paintCanvas(…,1) drop blur; tail() setPixel → splatIndex; strip() pool loop → gradientRange (L09)
- rain-on-glass.js: fillCanvas+setBlur(0.25) → fillCanvas(…,1) (L09)
- glowflow.js: strip() per-pixel loop → gradientRange; fizz setPixel → splatIndex (L09)
- plasma.js: fillCanvas → filter 1, drop setBlur line (L11)
- soap-2d.js: fillCanvas + paintCanvas → filter 1, drop setBlur (L11)
- dna-helix-2d.js rope(): optional fillRangeAA/gradientRange (L11)
- attractor.js, physarum.js: fillCanvas → filter 1, drop setBlur stand-in (L15); docs/pattern-ideas.md:6 names flow-field-2d.js
- crosstown-traffic-2d.js strip: fillRange per car → gradientRange mode 2 / fillRangeAA (L30)
- portal.js strip: field array + paint loop → splatIndex/gradientRange; panel paintCanvas → filter 1 (L30)
- lava-lamp.js: paintCanvas → filter 1, drop setBlur(0.5); strip field+paint loop → splatIndex mode 2 (L30)
- docs/lang.md: note that `>>` shifts the raw 16.16 word (1 >> 1 = 0.5) — check it is documented (L30)
- topo-map.js, coral-plasma.js, chladni.js: paintCanvas filter 1 ONLY if the filter blends after the palette lookup (palette positions wrap 0↔1 on the lines) (L12)
- heatshivers.js renderStrip(): hc array + paint/setPixel + per-pulse fillRange → fade() + splatIndex per pulse (L07)
- perlin-fire.js renderStrip() and doom-fire.js strip loop: smoothed paint/setPixel → gradientRange/fillRangeAA (L07)
- perlin-fire.js renderPanel(): paintCanvas → filter 1, drop setBlur(0.3) in panelStep() (L07)
- docs/lang.md: on a MAPLESS strip gridWidth() returns the pixel count, not 0 — document; check has2DMap() first (L07)
- web/src/lib/geometry.ts ~754 comment names heatshivers as six-buffer; kinds.rs:114 comment mentions fire-blue.js (L07)
- day-ribbon.js: setPixel repaint loop → gradientRange per keyframe span; marker glow → splatIndex (L19)
- sunrise-alarm-clock.js: per-pixel render over 49-sample tables → renderBulk + gradientRange; sun → fillRangeAA/splatIndex (L19)
- analog-clock.js strip blob(): setPixel loop → fillRangeAA/splatIndex (L19)
- aurora-2d.js: paintCanvas → filter 1, drop setBlur(0.3); strip deposit() span loops + fillRGB → splatIndex mode 1 per light, shiver afterglow → fade() (L08)
- docs/bulk-render.md + docs/lang.md cite aurora-2d's OLD internals (normAxis, Cell Size, fillNoise3D rows) — mark historical or re-point (L08)
- oil-slick.js: paintCanvas → filter 1 (L32); digital-rain-2d.js strip drops → splatIndex (optional)
- kaleidoscope-2d.js, loop-ring.js: paintCanvas → filter 1; NOT 2d-spiral-twirls/shadebobs (wrapping values) (L13)
- chomp-chase.js strip run(): fillRange actors → fillRangeAA; pop ring/power pellet setPixel → splatIndex (L23)
- us-flag-2d.js strip(): setPixel loop → fillRangeAA/gradientRange per stripe (L21)
- light-thief.js drawStrip()/span()/stripPx(): splatIndex per bulb/thief/ring (L21)
- candle.js tealights(): gradientRange per candle (L21)
- fractal-flower.js fillCanvas → filter 1; tunnel/radiant-pulse-3/line-dancer >4096 px path → 64x64 canvas + fillCanvas(…,1) (L28)
- mandelbrot-2d.js, voronoi-2d.js renderBulk, lotus-bloom.js mandala(): filter 1, drop setBlur (L29)
- orbits.js seg(), pendulum-wave.js strip(): fillRange → splatIndex; tiny-lane fillRange → gradientRange (L29)
- docs/lang.md: note that add-mode splat on a persistent fade() frame accumulates to white (L29)
- scenes referencing @pat/voronoi-2d want Style orbs → set C override explicitly (default is stained glass) (L29)
- reaction-diffusion-2d.js paintCanvas → filter 1; R loop → arrayMul (#373/#453) (L25)
- cellular-automata.js panel paintCanvas → filter 1 (continuous mode); history shift loop wants a native canvas scroll/copy op (new ticket idea) (L25)

## From engine agent A (PRs #979 0a57c678, #981 230bf51a, #982 936083a8 — all merged)
- headroom after the three: 10,651 B (was 13,629); luxel.wasm 497,732 B. gallery.json shrink from the dedupe should recover far more — read ci.sh's line on the PR.
- TICKETS: tools/stack-check.sh red on plain master (24,308 B vs 24,576 floor) — not ours; measure setRenderSize / index ops / setTrail on the S3 (patbench + opbench + builtinbench); fast path for exact-multiple bilinear expand (host 0.15 ms at 16384 px); decide hue-short-way / palette-space bilinear (E2 as implemented blends RGB after lookup); #843 can build on ProjPlan::Resized; #628 workaround = setRenderSize(n, 1).
- Merged worktrees to remove at the end: .claude/worktrees/engine-rendersize, engine-rendersize-base, engine-index-aa, engine-settrail (git worktree remove + branch delete).
- alloc_peak.rs LIVE_SLACK ceiling may be loose now that 2d-fireworks-fade is gone from the live library — re-run (L06)
- sunrise-alarm-clock.js: its kelvin helper should match lamp.js kelvinRGB() (clamp ≤ 15000 K; 40000 overflows 16.16) — check in BF (L02)
- ticket: fillRect has no blend mode (L02); tooling: enginehost.mjs lacks an event-injection helper (uses lx_push_event directly) (L02)
- docs/lang.md trap: `density * pixelCount / 1000` overflows 16.16 at 4096 px — divide first (L18)
- ticket: multiply blend mode for splat/paintCanvas (L18)
- docs/tools.md lines 33 and 219 prose names library/snake.js (render-bench now uses tools/bench-patterns/) (L05); web/src/lib/geometry.ts:754 comment names pew-pew-pew
- ticket: paintCanvas live-palette lookup is a linear stop search per texel — indexed/binary palette lookup (L05)
- docs/lang.md trap: integer literals above 32767 overflow 16.16 (60000/interval went negative) (L17); docs/bulk-render.md cites sound-spectromatrix-agc as a worked example

## From U1 (engine tickets)
- BUG: bilinear fillCanvas/paintCanvas is not identity at 1:1 (half-cell drift: mapped coords run 0..1 inclusive, sample point drifts up to half a cell) — fix pixel-centre convention; every library call site uses filter 1 only when canvas < grid as a workaround
- splatIndex measures from index i, gradientRange from pixel centre i+0.5 — document or align
- setRenderSize in beforeRender: gridWidth() reports the old grid until the next frame while pixelCount changes immediately
- fillCanvas/paintCanvas on a mapless strip maps pixels to cells unevenly
- bilinear expand has no integer-factor fast path (+0.5–0.75 ms host per 16384-px frame; topo/coral/chladni/cellular 3–4.7x at 128) — measure on the S3 with patbench at 128x128 before release
- native canvas scroll/copy op (cellular-automata)
- ticket: paintCanvas per-pixel palette lookup is a linear stop scan (sample_palette in vm.rs) — cook a 256-entry table (L01, L05)
- ticket: classic boards' JIT_INTERNAL_MAX (8 KB) now excludes a larger share of the library (mean native image 9.6 KB, worst frogger 27 KB) — count via the Athom hw-bench jit.state column and decide whether to raise the internal cap or split hot paths
