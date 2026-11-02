# Library overhaul — master plan (reconciled 2026-11-01)

Inputs: the four audit ranges (308 records), `perf/sweep.md`, `scene-ideas/proposal.md`,
`ideas-online/ideas.md`, `pf-index.md` + the 26 Patternflow specs scored ≥ 3,
`engine-ideas/proposal.md`, `inventory.tsv`. Every conflict between the auditors is resolved
below; where it was a judgment call the alternative is recorded in §B ("alt:").

## Outcome in numbers

| | count |
|---|--:|
| library patterns today | 308 |
| merged into a survivor (file removed) | 144 |
| deleted outright | 51 |
| moved out to `tools/patterns/` or dropped as instruments (to-tools) | 10 |
| converted to scene / sprite (file removed) | 5 + 1 |
| **survivors** (keep 1 · polish 66 · rewrite 30) | **97** |
| of which renamed to a new slug | 22 |
| **new patterns** (§C) | **52** |
| **library after the campaign** | **149 patterns** |
| new example scenes / new sprites | 37 / 29 |
| engine features built now / ticket-only | 4 (in 2 batches) / 11 |
| implementer batches | 40 |

**Bundle finding the brief did not know about.** `web/public/gallery.json` — the full source
text of every `library/*.js` — ships *inside the device asset archive* (`web/tools/device-files.mjs`
keeps it unconditionally; it is 1.19 MB raw). The files this plan removes are 749 KB raw /
331 KB individually gzipped, ~65 % of the library. So the dedupe **pays for the four engine
features many times over** — but only if new and rewritten patterns stay lean. Library rule
for every batch: header comment ≤ 12 lines (no essays — aurora-2d, snake-2d-v2 and
breathing-gradient carry 40–60 line headers today), no dead code paths, and read the
`asset archive:` line `tools/ci.sh` prints before and after; a library batch must not grow it
net of its own deletions.

## Rules every implementer follows (read before §B/§C)

1. **Clean-room firewall (non-negotiable).** Implementers read ONLY: this plan, prose specs
   (`docs/pattern-specs/<slug>.md`, `docs/pattern-specs/patternflow/*.md`), `ideas-online/ideas.md`,
   and existing `library/` code. Never `corpus/*.epe`, never anything under
   `$SCRATCH/libfix/patternflow/` (that is the Patternflow SOURCE tree), never an original.
2. **Better, not identical.** Fidelity to the original port is not a goal and the
   "byte-identical before/after" check in `.claude/rules/library.md` does NOT apply to
   rewrites or merges in this campaign (it still applies to a pure control re-plumb).
   The rest of that file does apply: real-unit `//#` bounds, `default=` == the top-level
   constant, the handler consumes real units (never rescale as if 0..1), mode selectors
   `min=0 max=n-1 step=1`, **no pattern-level brightness or gamma control** (an `hsvPicker`'s
   V is ignored — say so in the control comment), `unitless-ok` only for genuine ratios.
3. **Dimensions.** Every survivor/new pattern is tagged strip-native, panel-native or both.
   *Both* patterns use the documented `bouncy-boxes` idiom (docs/bulk-render.md, "the
   `fillHSV` readouts" section): export a `render(index)` beside `renderBulk` — that suppresses
   the `ceil(√n)` default grid a `renderBulk` naming any coordinate/grid op would otherwise get
   on a mapless strip (`Engine::from_program_budgeted`: the default grid is installed only when
   `render_tgt[0]` is absent) — and branch at the top of `renderBulk` on `has2DMap()`: the
   strip branch calls index-space ops only (a coordinate op on a mapless fixture paints along
   the 1-D fallback `y = 0.5`). Verify on a 300 px mapless strip AND a 64×64 grid, and check
   the playground's dimensionality hint (`Engine.preferredDims()`) still offers the panel form.
   Strip code uses the E1 index ops (`splatIndex`, `fillRangeAA`, `gradientRange`) instead of
   `array(pixelCount)` buffers.
4. **Canvas policy.** No hard-coded 16×16 anywhere (40 patterns do it today). A simulation or
   field canvas is `W = min(gridWidth(), CAP)`, `H = min(gridHeight(), CAP)` with CAP 32–64 as
   the batch says (≤ 2,304 cells per array keeps four arrays inside the 10,236-element
   budget), and `gridWidth() == 0` (sparse/irregular map) falls back to 32×32. Upsample with
   `fillCanvas`/`paintCanvas(…, filter)` — filter 1 (bilinear, E2) for soft fields, 0 for
   pixel art. Use `gridIndex`, never row arithmetic.
5. **Orientation.** `y = 0` is the TOP of a panel. Fire, sand, liquid, grass, rising bubbles
   and skylines root at `y = 1`. (palette-fire-2d and glowflow's 2D shim are upside down today.)
6. **No black screens.** An idle viewer must never stare at a dark panel for more than ~1 s
   by default: overlap cycles, raise floors, raise spawn rates (≥ 30 dark patterns are listed
   in §B's bug ledger).
7. **Perf targets** (host wasm, the `perf/measure.mjs` harness, 64×64 unless stated): ambient
   per-pixel patterns ≤ 0.8 ms and est ≤ 1,200 cycles/px; entity patterns (`renderBulk` +
   `splat`/`drawLine`/`fillRect`) ≤ 0.3 ms with 128×128 within 1.5× of 64×64; strip patterns
   ≤ 0.1 ms at 300 px with zero `array(pixelCount)` (one capped ≤ 1,024-cell array if a sim
   needs state). Every pattern must pass `luxel check --grid 64x64`; at 128×128 only the
   PSRAM boards are guaranteed, but a pattern that fails there must say why in its header.
   Per-survivor targets in §B override these.
8. **Verification per batch:** `tools/check-library.sh` on all five rigs; a contact sheet
   (`tools/verify/sheet.mjs`) at 32×32, 64×64 and a 150 px strip for every touched pattern,
   eyeballed for darkness/orientation/garishness; host perf before/after; `nix develop
   --command tools/ci.sh`; the asset-archive line. Engine batches (BE1, BE2) additionally
   follow CLAUDE.md's on-device measurement rules (devlock first) and verify-webui for UI.
9. **Specs.** A deleted pattern's `docs/pattern-specs/<slug>.md` is deleted with it; a merged
   file's spec stays (it documents an idea the survivor carries); a renamed survivor's spec is
   left at the old slug (BF repoints nothing — specs are historical prose).
10. **Shared files.** `UPDATES.md`, `tools/verify/*.json`, `docs/lang.md`, `docs/tools.md`
    and test files are each owned by exactly one batch (§E). Library batches touch only the
    files §E lists; anything else they notice goes in their report for BF to ticket.

## A. Disposition table (all 308 files)

Sorted by final action, then file. `polish → x` / `rewrite → x` = the survivor is renamed to `library/x.js` (old file deleted in the same batch). Counts: merge-into 144 · polish 66 · delete 51 · rewrite 30 · to-tools 10 (8 moved to `tools/patterns/`, performance-test-framework and rgbw-mapping-tester-hsv-version dropped) · to-scene 5 · to-sprite 1 · keep 1.

| # | file | current name | final action | batch | reason |
|--:|---|---|---|---|---|
| 1 | `color-twinkle-bounce.js` | color twinkle bounce | keep | L01 | bold candy beads; good as is |
| 2 | `2d-spiral-twirls.js` | 2D Spiral Twirls | polish | L13 | survivor Spiral Twirls (spark-field scene uses this slug) |
| 3 | `80s-kid-show.js` | 80s kid show | polish | L27 | fun retro shapes; renderBulk shape ops, Memphis palette, real units |
| 4 | `aurora-2d.js` | Aurora 2D | polish | L08 | survivor Aurora (both dims); clock-over-aurora scene keeps the slug |
| 5 | `bessel-chaos.js` | Bessel Chaos | polish | L12 | genuinely unusual chirp; fix the live 1..15 rescale bug, add radial 2D |
| 6 | `blinky-eyes-2d.js` | Blinky Eyes 2D | polish | L22 | charming; fill the panel, sclera, pupil glint, look up/down |
| 7 | `bouncy-boxes.js` | Bouncy Boxes | polish | L27 | tidy box physics; fillRect at native res |
| 8 | `butterfly-2d.js` | Butterfly 2D | polish | L22 | best-reading figure; bbox early-out, wing variety, several butterflies |
| 9 | `christmas-candy-cane.js` | Christmas Candy Cane | polish | L04 | survivor Candy Stripes (+2D diagonal) |
| 10 | `color-pick-fade.js` | Color Pick Fade | polish | L05 | survivor Pulse Train: hsvPicker, no 0.5 cap |
| 11 | `coronal-mass-ejection.js` | Coronal Mass Ejection | polish | L20 | survivor (c4) with polar noise canvas |
| 12 | `custom-sequences.js` | Custom Sequences | polish | L04 | survivor Color Sequence (eave/holiday colour runs) |
| 13 | `digital-rain-2d.js` | Digital Rain 2D | polish | L32 | survivor Digital Rain (both dims) |
| 14 | `dire-spider-2d.js` | Dire Spider 2D | polish | L22 | survivor Spider; drawLine legs + polar haze canvas |
| 15 | `dna-helix-2d.js` | DNA Helix 2D | polish | L11 | survivor DNA Helix with ribbon count (absorbs sinus) |
| 16 | `edgeburst.js` | Edgeburst | polish | L05 | cheap strip classic; no dead time, 2D ring |
| 17 | `fairies.js` | _Fairies | polish | L03 | Jeremy's live _Fairies; drop buffers (fade + births), keep look |
| 18 | `falling-sand-2d.js` | Falling Sand 2D | polish | L26 | satisfying; native res, spouts |
| 19 | `fast-pulse-3d.js` | fast pulse 3d | polish | L05 | survivor Fast Pulse |
| 20 | `fireflies.js` | Fireflies | polish | L16 | survivor Fireflies (both dims) |
| 21 | `fractal-flower.js` | fractal flower | polish | L28 | grid-sized canvas, real units, wider shapes |
| 22 | `frogger-2d.js` | Frogger 2D | polish | L24 | great game redesign; brighter, renderBulk |
| 23 | `geometry-morphing-demo-2d.js` | Geometry Morphing Demo 2D | polish | L27 | vivid morphs; renamed Geometry Morph, timing controls |
| 24 | `glittering-jewels.js` | Glittering Jewels | polish | L03 | elegant strip jewels; splatIndex instead of 20x pow per px |
| 25 | `heart.js` | heart | polish | L21 | clean scalable heart; fill modes, lub-dub |
| 26 | `kaleidoscope-2d.js` | Kaleidoscope 2D | polish | L13 | survivor Kaleidoscope (c4) |
| 27 | `light-organ-2-0.js` | Light Organ - 2.0 | polish | L17 | survivor Light Organ |
| 28 | `line-dancer-2d.js` | Line Dancer 2D | polish | L28 | vivid ribbons; no brown melt trough |
| 29 | `meteor-shower.js` | Meteor Shower | polish | L05 | survivor Meteor Shower (best strip comet), buffer-free |
| 30 | `ocean.js` | Ocean | polish | L08 | survivor Ocean (strip Pacifica + shoreline, 2D seascape) |
| 31 | `opposites.js` | opposites | polish | L01 | organic lens blobs; smooth fold, 2D form |
| 32 | `radar-2d.js` | Radar 2D | polish | L14 | phosphor radar on persistent frame; brighter afterglow |
| 33 | `radiant-pulse-3.js` | radiant pulse 3 | polish | L28 | renamed Radiant Bloom, cached polar, shorter cycle |
| 34 | `rainbow-melt.js` | Rainbow Melt | polish | L01 | survivor Rainbow Melt (absorbs marching rainbows), 2D form |
| 35 | `raindrops-2d.js` | Raindrops 2D | polish | L09 | survivor Raindrops (water sim, bright rings, tap events, strip drips) |
| 36 | `snake-2d.js` | Infinite Snake | polish | L23 | survivor Infinite Snake (v2 fillCanvas engine, board size) |
| 37 | `soap-2d.js` | Soap 2D | polish | L11 | survivor Soap (domain warp on a coarse canvas) |
| 38 | `sound-rays.js` | sound - rays | polish | L17 | survivor Sound Rays (peak AGC fix) |
| 39 | `sound-spectrum-analyser.js` | Sound - Spectrum Analyser | polish | L17 | survivor Spectrum Analyzer (bars, VU, strip) |
| 40 | `sparks.js` | sparks | polish | L06 | survivor Sparks |
| 41 | `stacker.js` | Stacker | polish | L24 | playful strip game; real units, watchable speed |
| 42 | `sunrise-alarm-clock.js` | Sunrise Alarm Clock | polish | L19 | survivor Sunrise Alarm (no gamma slider) |
| 43 | `tetrix-2d.js` | Tetrix 2D | polish | L24 | grid-sized falling blocks with line clears |
| 44 | `tixy.js` | tixy | polish | L13 | grid-derived tiny-formula showcase |
| 45 | `tunnel-of-squares-2d.js` | Tunnel of Squares 2D | polish | L28 | survivor Tunnel (+ textured tunnel mode) |
| 46 | `tv-simulator.js` | TV Simulator | polish | L02 | presence-simulation utility with a real use |
| 47 | `twinkle.js` | Twinkle | polish | L03 | survivor Twinkle (the sparkle family) |
| 48 | `twinkling-classic-xmas-strands.js` | Twinkling Classic Xmas Strands | polish | L04 | survivor Classic Christmas Strands |
| 49 | `typing-heatmap-2d.js` | Typing Heatmap 2D | polish | L02 | interactive event heatmap, renamed Touch Glow |
| 50 | `us-flag-2d.js` | US Flag 2D | polish | L21 | survivor Waving Flag (flag selector, strip path) |
| 51 | `wanderedges.js` | wanderedges | polish | L03 | distinctive contour loops; controls, 2D rings |
| 52 | `wavy-bands.js` | Wavy Bands | polish | L11 | survivor Wavy Bands (palette mode, no pow) |
| 53 | `xorcery-2d-3d.js` | xorcery 2D/3D | polish | L25 | survivor Xorcery (+AND/Sierpinski, PF tapestry cells) |
| 54 | `bulk-bouncing-balls-2d.js` | Bulk Bouncing Balls 2D | polish → bouncing-balls | L10 | cheapest correct engine; base of Bouncing Balls |
| 55 | `bouncing-balls-2d.js` | Bouncing Balls 2D | polish → bouncing-columns | L10 | distinct collapsing-columns look; grid-sized, renamed Bouncing Columns |
| 56 | `continuous-cellular-automata.js` | Continuous Cellular Automata | polish → cellular-automata | L25 | base of Cellular Automata (grid-sized waterfall) |
| 57 | `dbzbattlefinal.js` | DBZBattleFinal | polish → energy-duel | L24 | fun story; trademark name dropped, renderBulk beams |
| 58 | `eye-of-sauron.js` | Eye of Sauron | polish → fire-eye | L20 | instantly reads; trademark name softened to Fire Eye, gaze control |
| 59 | `glowflow-3d-coord-transform-api-port.js` | GlowFlow (3D coord transform API port) | polish → glowflow | L09 | sensor liquid showcase; fix upside-down 2D shim |
| 60 | `carrie-s-holiday-star-2d.js` | Carrie's Holiday Star 2D | polish → holiday-star | L21 | survivor Holiday Star: bigger, picker, cheap ray falloff |
| 61 | `2d-sinc-theta-theta.js` | 2D sinc(theta)/theta | polish → interference | L11 | flagship 3-source rings; base of Interference (absorbs interference-2d) |
| 62 | `the-grinch.js` | Grinch's Heist | polish → light-thief | L21 | charming strip story; trademark character renamed Light Thief |
| 63 | `millipede-1d-2d-controls.js` | millipede 1d/2d controls | polish → millipede | L22 | charming creature; takes the millipede slug |
| 64 | `sinpulse-3d.js` | sinpulse 3D | polish → plasma | L11 | base of Plasma (1D/2D/3D) |
| 65 | `rocket-by-tony-hampton.js` | Rocket by Tony Hampton | polish → rocket-launch | L06 | real launch sim; author name off the slug |
| 66 | `sound-spectrokalidamandala.js` | sound - spectrokalidamandala | polish → spectrum-field | L17 | base of Spectrum Field |
| 67 | `sound-starburst-2.js` | sound - Starburst 2 | polish → starburst | L17 | base of Starburst (crossing heads, sound optional) |
| 68 | `boids-2d.js` | Boids 2D | rewrite | L10 | real flock at native res: renderBulk drawLine streaks + fade |
| 69 | `breakout-2d.js` | Breakout 2D | rewrite | L23 | instantly readable; grid-sized + PF breakout-arcade juice (power-ups, combos, AI) |
| 70 | `coral-plasma.js` | Coral Plasma | rewrite | L12 | TV static at LED scale; becomes Neon Veins (slug kept: sunrise.scene) |
| 71 | `crosstown-traffic-2d.js` | Crosstown Traffic 2D | rewrite | L30 | street grid: drawLine cars both ways (was 10.6 ms/64x64, worst in library) |
| 72 | `curl-flow-2d.js` | Curl Flow 2D | rewrite | L15 | flagship flow field at native res (absorbs flow-field-2d) |
| 73 | `doom-fire.js` | Doom Fire | rewrite | L07 | survivor Doom Fire, grid-sized, palette, tall flames |
| 74 | `heatshivers.js` | heatshivers | rewrite | L07 | survivor Embers: renderBulk fade + splatIndex, no 5 buffers |
| 75 | `mandelbrot-2d.js` | Mandelbrot 2D | rewrite | L29 | harsh bands; Julia Morph with smooth escape colouring |
| 76 | `marquee-chase.js` | Marquee Chase | rewrite | L04 | real incandescent theater marquee; live sliderBlocks rescale bug |
| 77 | `pendulum-wave.js` | Pendulum Wave | rewrite | L29 | great concept, aliased render; one pendulum per column |
| 78 | `perlin-fire.js` | Perlin fire | rewrite | L07 | survivor Fire (both dims): coarse noise canvas + Fire2012 strip |
| 79 | `policelights.js` | policeLights | rewrite | L02 | crude flicker; real light-bar sequences, sane flash rates |
| 80 | `portal.js` | portal | rewrite | L30 | survivor Portal: pulse pool as splats, palette presets |
| 81 | `rainbow.js` | Rainbow | rewrite | L01 | survivor Rainbow (fillGradient, shapes, OKLCH) |
| 82 | `reaction-diffusion-2d.js` | Reaction Diffusion 2D | rewrite | L25 | 16x16 static noise; real Gray-Scott at 48-64 cells |
| 83 | `scanner.js` | Scanner | rewrite | L05 | survivor Scanner (absorbs kitt/cylon); renderBulk + splatIndex |
| 84 | `slime-mold-palette.js` | Slime mold palette | rewrite | L26 | growth over in 2 s at 16x16; Crystal Growth at native res |
| 85 | `spirograph-2d.js` | Spirograph 2D | rewrite | L14 | survivor Spirograph: curve tracer modes at native res |
| 86 | `starfield-2d.js` | Starfield 2D | rewrite | L18 | dead-pixel specks; warp/drift/trails night-sky base |
| 87 | `sun-rays-through-trees.js` | Sun rays through trees | rewrite | L20 | mostly black; cheap angular-bin god rays |
| 88 | `thunderstorm.js` | Thunderstorm | rewrite | L18 | survivor Thunderstorm (lightning-strike engine + rain + clouds) |
| 89 | `voronoi-2d.js` | Voronoi 2D | rewrite | L29 | survivor Voronoi: orbs / stained glass / ice |
| 90 | `rgbclock-2d.js` | RGBclock 2D | rewrite → analog-clock | L19 | unreadable face; Analog Clock with drawLine hands |
| 91 | `breathing-gradient.js` | Breathing Gradient | rewrite → color-wash | L01 | base of Color Wash; fillGradient/OKLab LUT instead of per-pixel mixColors |
| 92 | `2d-fireworks-fade.js` | 2D Fireworks Fade | rewrite → fireworks | L06 | best 2D fireworks; base of the one Fireworks (renderBulk splats, native res) |
| 93 | `utilitycolortemp.js` | UtilityColorTemp | rewrite → lamp | L02 | base of Lamp (Kelvin white) |
| 94 | `infinity-flower-2d.js` | Infinity Flower 2D | rewrite → lotus-bloom | L29 | broken dashes; rebuilt as Lotus Bloom (+PF lotus-bloom) |
| 95 | `saberdeploy-tutorial.js` | SaberDeploy Tutorial | rewrite → saber | L02 | flat red fill; a real ignite/hum/clash Saber prop |
| 96 | `all-lasers-fire.js` | All Lasers Fire | rewrite → searchlights | L14 | dark 80% of the time; its chromatic laser fan becomes the Laser mode of Searchlights |
| 97 | `perlin-simplex-noise-2d.js` | Perlin/Simplex Noise 2D | rewrite → topo-map | L12 | BROKEN (confetti); rebuilt as Topo Map |
| 98 | `spiral-2d.js` | Spiral 2D | merge-into:2d-spiral-twirls | L13 | arm count/glow; live sliderArms rescale bug must not survive |
| 99 | `spiral-twirls-star-2d.js` | spiral twirls star 2D | merge-into:2d-spiral-twirls | L13 | star mask option |
| 100 | `2d-clock-with-hand-color-pickers.js` | 2d Clock with Hand Color Pickers | merge-into:analog-clock | L19 | pow-threshold wedges; hand pickers become Analog Clock colours |
| 101 | `pixelclock.js` | pixelClock | merge-into:analog-clock | L19 | ring-clock strip path of Analog Clock |
| 102 | `1d-aurora-borealis.js` | 1D Aurora Borealis | merge-into:aurora-2d | L08 | strip path of the one Aurora; its dome/age blobs become splatIndex curtains |
| 103 | `aurorashivers.js` | aurorashivers | merge-into:aurora-2d | L08 | afterglow streaks = Aurora Shiver style |
| 104 | `coolaura.js` | coolaura | merge-into:aurora-2d | L08 | aqua-swell style of Aurora |
| 105 | `time-flies-2d.js` | Time Flies 2D | merge-into:boids-2d | L10 | jitter-flies swarm mode |
| 106 | `2d-bouncing-additive-primaries.js` | 2D Bouncing Additive Primaries | merge-into:bouncing-balls | L10 | additive-primaries palette of Bouncing Balls |
| 107 | `bouncer3d.js` | Bouncer3D | merge-into:bouncing-balls | L10 | only distinct value was a render3D path (dropped: projection slices a 2D pattern); confetti look |
| 108 | `bouncing-balls-hsv.js` | bouncing balls - hsv | merge-into:bouncing-balls | L10 | same as bouncing-balls-rgb without controls |
| 109 | `bouncing-balls-rgb.js` | bouncing balls - rgb | merge-into:bouncing-balls | L10 | strip gravity path of Bouncing Balls |
| 110 | `bouncing-balls-rgb-2d.js` | Bouncing Balls RGB 2D | merge-into:bouncing-balls | L10 | gravity motion mode |
| 111 | `bouncing-rgb-balls-2d.js` | Bouncing RGB Balls - 2D | merge-into:bouncing-balls | L10 | Lissajous-drift motion + breathing size |
| 112 | `blue-holiday-candle-2d.js` | Blue Holiday Candle 2D | merge-into:candle | L21 | reads as an eye, not a flame; Candle (teardrop SDF) with icy-blue preset |
| 113 | `cellular-automata-1d.js` | Cellular Automata 1D | merge-into:cellular-automata | L25 | elementary rule = Elementary mode of Cellular Automata |
| 114 | `holiday-diagonal-stripes.js` | Holiday_Diagonal_Stripes | merge-into:christmas-candy-cane | L04 | 2D diagonal mode of Candy Stripes |
| 115 | `color-fade-pulse.js` | color fade pulse | merge-into:color-pick-fade | L05 | rainbow/wash colour mode of Pulse Train |
| 116 | `1-white-fade.js` | 1 White Fade | merge-into:color-wash | L01 | a solid white breathe = Color Wash with one white picker |
| 117 | `2-purple-fade.js` | 2 Purple Fade | merge-into:color-wash | L01 | violet breathe = Color Wash preset |
| 118 | `color-blend.js` | Color Blend | merge-into:color-wash | L01 | three travelling colour waves = Color Wash Waves motion |
| 119 | `complements-3d.js` | Complements 3D | merge-into:color-wash | L01 | complement/triad pair mode of Color Wash |
| 120 | `gradient-blue-purple-pink.js` | Gradient blue  purple pink | merge-into:color-wash | L01 | default flow preset of Color Wash |
| 121 | `twocolorhsvmix.js` | TwoColorHSVMix | merge-into:color-wash | L01 | two-colour scrolling blend |
| 122 | `coronal-ejection-2d.js` | Coronal Ejection 2D | merge-into:coronal-mass-ejection | L20 | arms/twist/mirror options of the one sun |
| 123 | `sunrise-2d.js` | Sunrise 2D | merge-into:coronal-mass-ejection | L20 | rise-on-start intro |
| 124 | `bustle.js` | bustle | merge-into:crosstown-traffic-2d | L30 | two-way strip traffic = strip path of Crosstown Traffic |
| 125 | `flow-field-2d.js` | Flow Field 2D | merge-into:curl-flow-2d | L15 | heading field mode of Curl Flow |
| 126 | `2-colors.js` | 2 Colors | merge-into:custom-sequences | L04 | static two-colour blocks = one Color Sequence setting |
| 127 | `3-color-rotation.js` | 3 color rotation | merge-into:custom-sequences | L04 | bare 3-colour segment step = Color Sequence chase |
| 128 | `christmas-lights.js` | Christmas Lights | merge-into:custom-sequences | L04 | block lights = Color Sequence with 4 pickers |
| 129 | `christmaslights.js` | ChristmasLights | merge-into:custom-sequences | L04 | same as christmas-lights with bigger blocks |
| 130 | `christmasstretch.js` | ChristmasStretch | merge-into:custom-sequences | L04 | stretch-slide transition of Color Sequence |
| 131 | `matrix-green-waterfall-1d.js` | Matrix Green Waterfall 1D | merge-into:digital-rain-2d | L32 | strip drop model of Digital Rain |
| 132 | `matrix-green-waterfall-2d.js` | Matrix Green Waterfall 2D | merge-into:digital-rain-2d | L32 | cheap stateless background mode |
| 133 | `matrix-rain.js` | matrix rain | merge-into:digital-rain-2d | L32 | hue-spread control only |
| 134 | `crawling-spider-2d.js` | Crawling Spider 2D | merge-into:dire-spider-2d | L22 | Dire Spider at Glow 0 |
| 135 | `sinus.js` | sinus | merge-into:dna-helix-2d | L11 | ribbon count 1 |
| 136 | `doom-fire-2d.js` | Doom Fire 2D | merge-into:doom-fire | L07 | same sim; its sway/reach logic goes into Doom Fire |
| 137 | `fast-pulse.js` | fast pulse | merge-into:fast-pulse-3d | L05 | identical to the survivor's 1D path |
| 138 | `eye-of-sauron-with-movement.js` | Eye of Sauron with movement | merge-into:fire-eye | L20 | saccades = Gaze control |
| 139 | `colourful-fireflies.js` | colourful fireflies | merge-into:fireflies | L16 | per-fly rainbow colour mode |
| 140 | `slowflies.js` | slowflies | merge-into:fireflies | L16 | drift-with-ghost-trail style |
| 141 | `xmasflies.js` | XmasFlies | merge-into:fireflies | L16 | holiday drift colour mode |
| 142 | `4th.js` | 4th | merge-into:fireworks | L06 | strip holiday fireworks + bunting = Fireworks red/white/blue preset + bunting underlay |
| 143 | `firework-nova.js` | firework nova | merge-into:fireworks | L06 | ring shell type of Fireworks |
| 144 | `firework-rocket-sparks.js` | firework rocket sparks | merge-into:fireworks | L06 | sparkly rocket trail = Fireworks rocket stage |
| 145 | `fireworks-finale.js` | Fireworks Finale | merge-into:fireworks | L06 | strip path of Fireworks |
| 146 | `upward-waves-3d-using-accelerometer.js` | Upward waves 3D using accelerometer | merge-into:glowflow | L09 | tilt waves mode of GlowFlow |
| 147 | `ember-diffusion.js` | Ember Diffusion | merge-into:heatshivers | L07 | dim embers = Embers cooling style |
| 148 | `fireblobs.js` | fireblobs | merge-into:heatshivers | L07 | no-drift setting of Embers |
| 149 | `blue-holiday-star-2d.js` | Blue Holiday Star 2D | merge-into:holiday-star | L21 | sub-pixel dot; same Minkowski star as Carrie |
| 150 | `interference-2d.js` | Interference 2D | merge-into:interference | L11 | single-hue/palette ring mode; live freq rescale bug must not survive |
| 151 | `zoom-kaleidoscope.js` | zoom kaleidoscope | merge-into:kaleidoscope-2d | L13 | ridged exponential-zoom source mode |
| 152 | `naturallightsync.js` | NaturalLightSync | merge-into:lamp | L02 | Follow-the-day mode of Lamp |
| 153 | `real-world-lights.js` | Real World Lights | merge-into:lamp | L02 | lamp presets of Lamp (candle -> Candle pattern) |
| 154 | `single-color-picker-wide-or-spot.js` | Single Color Picker - wide or spot | merge-into:lamp | L02 | Solid colour + spot mode of Lamp |
| 155 | `metaballs-of-fire-2d.js` | Metaballs of Fire 2D | merge-into:lava-lamp | L30 | fire palette preset of Lava Lamp |
| 156 | `sound-lavablob.js` | SOUND - lavablob | merge-into:lava-lamp | L30 | bass-swell option of Lava Lamp |
| 157 | `cyclic-cellular-automata-2d.js` | Cyclic Cellular Automata 2D | merge-into:life | L25 | Cyclic rule of Life (spiral waves) |
| 158 | `light-organ-sensor-board.js` | Light Organ -- sensor board | merge-into:light-organ-2-0 | L17 | fixed band hues colour mode |
| 159 | `three-red-pixels-mathy.js` | Marching Dots | merge-into:marquee-chase | L04 | dot count/width/background controls |
| 160 | `bulk-comet-trails.js` | Bulk Comet Trails | merge-into:meteor-shower | L05 | reference comet engine folded into Meteor Shower |
| 161 | `christmaspewpew.js` | ChristmasPewPew | merge-into:meteor-shower | L05 | holiday palette of the Laser Bolts style |
| 162 | `comets.js` | Comets | merge-into:meteor-shower | L05 | multi-comet crossing style of Meteor Shower |
| 163 | `pew-pew-pew.js` | Pew-Pew-Pew! | merge-into:meteor-shower | L05 | Laser Bolts style (blue-channel quirk dropped) |
| 164 | `rainbow-comet.js` | Rainbow Comet | merge-into:meteor-shower | L05 | bounce motion option |
| 165 | `snake.js` | snake | merge-into:meteor-shower | L05 | comet colour modes; name clashes with the game |
| 166 | `crosshair-pulse-2d.js` | Crosshair Pulse 2D | merge-into:notification-ripple | L02 | event cross burst = a Notification Ripple effect |
| 167 | `green-ripple-reflections.js` | green ripple reflections | merge-into:ocean | L08 | Moonlit palette of Ocean |
| 168 | `oasis.js` | Oasis | merge-into:ocean | L08 | bidirectional wave trains of Ocean strip |
| 169 | `neutronorbit.js` | neutronorbit | merge-into:orbits | L29 | strip path of Orbits (comets on an atom) |
| 170 | `fire-blue.js` | fire - blue | merge-into:perlin-fire | L07 | blue palette of Fire (strip path) |
| 171 | `fire-red.js` | fire - red | merge-into:perlin-fire | L07 | Fire2012 sim = strip path of Fire |
| 172 | `newfire.js` | Newfire | merge-into:perlin-fire | L07 | advected licks style of the strip Fire |
| 173 | `palette-fire-2d.js` | Palette Fire 2D | merge-into:perlin-fire | L07 | UPSIDE DOWN (base at top); its palette+fbm becomes Fire core |
| 174 | `perlin-fire-wind.js` | perlin fire wind | merge-into:perlin-fire | L07 | Wind dial |
| 175 | `sparkfire.js` | sparkfire | merge-into:perlin-fire | L07 | spark-ignition mode of strip Fire |
| 176 | `matrix-2-tone-pulse.js` | Matrix 2 tone pulse | merge-into:plasma | L11 | two-tone complementary style of Plasma |
| 177 | `matrix-2d-honeycomb.js` | matrix 2D honeycomb | merge-into:plasma | L11 | neon contour style of Plasma |
| 178 | `sunset.js` | Sunset | merge-into:plasma | L11 | broken 12-wide decode; ambient-wedge palette of Plasma |
| 179 | `novas.js` | novas | merge-into:portal | L30 | Embers palette of Portal; over array budget |
| 180 | `scrolls.js` | scrolls | merge-into:portal | L30 | Bioluminescent palette of Portal |
| 181 | `angle-and-radius-from-coordinates.js` | angle and radius from coordinates | merge-into:radar-2d | L14 | teaching aid; hue-by-radius option on Radar |
| 182 | `bulk-rainbow.js` | Bulk Rainbow | merge-into:rainbow | L01 | fillGradient rainbow is the new Rainbow implementation |
| 183 | `chasing-rainbows-hsluv.js` | Chasing Rainbows & HSLuv | merge-into:rainbow | L01 | colour-space tool; OKLCH perceptual mode on Rainbow |
| 184 | `chevron-2d.js` | Chevron 2D | merge-into:rainbow | L01 | chevron shape mode of Rainbow |
| 185 | `rainbow-fonts.js` | rainbow fonts | merge-into:rainbow | L01 | mirror/warp option of Rainbow |
| 186 | `rainbow-pinwheel.js` | rainbow pinwheel | merge-into:rainbow | L01 | fold option + conic shape of Rainbow |
| 187 | `rainbow-v2.js` | Rainbow v2 | merge-into:rainbow | L01 | identical look; carries a pattern brightness slider |
| 188 | `regenbogendrogen.js` | regenbogendrogen | merge-into:rainbow | L01 | same as rainbow-fonts |
| 189 | `ryb-colors.js` | RYB colors | merge-into:rainbow | L01 | RYB wheel as a baked palette mode of Rainbow |
| 190 | `marching-rainbow.js` | marching rainbow | merge-into:rainbow-melt | L01 | March motion of Rainbow Melt |
| 191 | `marching-rainbow-buffered.js` | marching rainbow (buffered) | merge-into:rainbow-melt | L01 | buffering demo of marching-rainbow |
| 192 | `bulk-canvas-ripples-2d.js` | Bulk Canvas Ripples 2D | merge-into:raindrops-2d | L09 | its bulk wave sim is the Raindrops engine model |
| 193 | `drip.js` | Drip | merge-into:raindrops-2d | L09 | strip path of Raindrops (drips + splash) |
| 194 | `ripples-2d.js` | Ripples 2D | merge-into:raindrops-2d | L09 | visual target + tap-to-splash + stylized rings mode |
| 195 | `rainbow-rocket-sparks.js` | Rainbow rocket sparks | merge-into:rocket-launch | L06 | strip-fixed exhaust colours option |
| 196 | `beat-bounce.js` | Beat Bounce | merge-into:scanner | L05 | single dot sweep = Scanner BPM-lock option |
| 197 | `cylon.js` | Cylon | merge-into:scanner | L05 | single-eye Scanner |
| 198 | `kitt.js` | KITT | merge-into:scanner | L05 | long-tail Scanner preset (trademark name dropped) |
| 199 | `kitt-w-color-picker.js` | KITT (w/ color picker) | merge-into:scanner | L05 | ramped gap fill + picker go into Scanner |
| 200 | `3d-rotation-spotlights.js` | 3D Rotation / Spotlights | merge-into:searchlights | L14 | 3D double-cone tumble = Searchlights cloud mode |
| 201 | `spotlights-rotation-3d.js` | spotlights / rotation 3D | merge-into:searchlights | L14 | second port of the same cones |
| 202 | `snake-2d-v2.js` | Infinite Snake v2 | merge-into:snake-2d | L23 | its engine replaces snake-2d content |
| 203 | `spinning-plasma-2d.js` | Spinning Plasma 2D | merge-into:soap-2d | L11 | rotation control |
| 204 | `sound-rays-frequency-bpm-reactive-1.js` | sound - rays Frequency-BPM Reactive 1 | merge-into:sound-rays | L17 | BLACK with sensors; tempo-follow speed mode |
| 205 | `audio-volume-meter.js` | Audio Volume Meter | merge-into:sound-spectrum-analyser | L17 | VU level = the Level mode of Spectrum Analyzer |
| 206 | `sound-music-spectrum-visualizer.js` | Sound & Music Spectrum Visualizer | merge-into:sound-spectrum-analyser | L17 | strip segment path |
| 207 | `sparks-center.js` | sparks center | merge-into:sparks | L06 | centre origin mode |
| 208 | `sound-spectro-kalidastrip.js` | sound - spectro kalidastrip | merge-into:spectrum-field | L17 | linear-fold strip mode |
| 209 | `sound-spectroblots-pow-fade.js` | sound - spectroblots - pow fade | merge-into:spectrum-field | L17 | noise-blob field mode |
| 210 | `sound-spectromatrix-agc.js` | sound - spectromatrix agc | merge-into:spectrum-field | L17 | plasma field mode (its canvas approach is the model) |
| 211 | `sound-spectromatrix-render2d.js` | sound - spectromatrix render2D | merge-into:spectrum-field | L17 | near-dup of agc with a decay bug; nothing extra |
| 212 | `lissajous-curve-tracer.js` | Lissajous curve tracer | merge-into:spirograph-2d | L14 | Lissajous mode of Spirograph |
| 213 | `swirlpool-2d.js` | Swirlpool 2D | merge-into:spirograph-2d | L14 | epicycle arms mode (no hooked-cross 16x16 look) |
| 214 | `unstable-orbits-2d.js` | Unstable Orbits 2D | merge-into:spirograph-2d | L14 | swarm mode |
| 215 | `white-rainbows.js` | White Rainbows | merge-into:starburst | L17 | non-sound default of Starburst |
| 216 | `sunrise.js` | Sunrise | merge-into:sunrise-alarm-clock | L19 | Sunrise-now trigger |
| 217 | `b-lightning-flashes.js` | b_lightning_flashes | merge-into:thunderstorm | L18 | dim static flashes; Thunderstorm strip strikes replace them |
| 218 | `lightning-clouds.js` | Lightning clouds | merge-into:thunderstorm | L18 | 2D cloud path of Thunderstorm |
| 219 | `lightning-strike.js` | Lightning Strike | merge-into:thunderstorm | L18 | its state machine is the Thunderstorm engine |
| 220 | `rainstorm.js` | Rainstorm | merge-into:thunderstorm | L18 | rain streak layer of Thunderstorm 2D |
| 221 | `golden-tix.js` | Golden Tix | merge-into:tixy | L13 | 1D tixy path |
| 222 | `perlin-simplex-noise-1d.js` | Perlin/Simplex Noise 1D | merge-into:topo-map | L12 | 1D cut of Topo Map |
| 223 | `autumn-colors.js` | Autumn Colors | merge-into:twinkle | L03 | autumn palette preset of Twinkle |
| 224 | `blink-fade.js` | Blink Fade | merge-into:twinkle | L03 | pop-and-fade rainbow = Twinkle rainbow palette |
| 225 | `chill-confetti.js` | chill confetti | merge-into:twinkle | L03 | pop-and-fade drift palette of Twinkle |
| 226 | `christmas-rg-fade.js` | Christmas RG Fade | merge-into:twinkle | L03 | holiday red/green palette of Twinkle |
| 227 | `color-twinkles.js` | Color Twinkles | merge-into:twinkle | L03 | sine streaks, not twinkles; Twinkle does it properly |
| 228 | `firework-dust.js` | firework dust | merge-into:twinkle | L03 | random-hue confetti palette of Twinkle |
| 229 | `glitter.js` | Glitter | merge-into:twinkle | L03 | backdrop + glints option of Twinkle |
| 230 | `halloween-color-twinkles.js` | Halloween color twinkles | merge-into:twinkle | L03 | Halloween palette of Twinkle |
| 231 | `quiet-blinkfade.js` | quiet blinkfade | merge-into:twinkle | L03 | quiet low-density setting |
| 232 | `sound-blinkfade.js` | sound - blinkfade | merge-into:twinkle | L03 | sound-reactive toggle |
| 233 | `spring-colors.js` | Spring Colors | merge-into:twinkle | L03 | weighted-accent spring palette |
| 234 | `twinkle-2.js` | twinkle (2) | merge-into:twinkle | L03 | hue-range colour mode |
| 235 | `twinkly-stars.js` | twinkly stars | merge-into:twinkle | L03 | lit-background option |
| 236 | `static-christmas-lights-4-colors.js` | Static Christmas Lights - 4 Colors | merge-into:twinkling-classic-xmas-strands | L04 | twinkle-off RGBY setting |
| 237 | `rainbow-flag.js` | Rainbow Flag | merge-into:us-flag-2d | L21 | Pride flag selection of Waving Flag |
| 238 | `us-flag.js` | US Flag | merge-into:us-flag-2d | L21 | strip layout path |
| 239 | `ice-floes-2d.js` | Ice Floes 2D | merge-into:voronoi-2d | L29 | Ice style of Voronoi |
| 240 | `halloween-wavy-bands.js` | Halloween Wavy Bands | merge-into:wavy-bands | L11 | Halloween palette of Wavy Bands |
| 241 | `sierpinski-rainbow-2d.js` | Sierpinski Rainbow 2D | merge-into:xorcery-2d-3d | L25 | AND mode + infinite zoom of Xorcery |
| 242 | `2d-canvas-example.js` | 2D canvas example | delete | B00 | teaching scaffolding (canvasSet tutorial); idiom lives in docs/lang.md |
| 243 | `2d-wandering-fireball.js` | 2D Wandering Fireball | delete | B00 | one fuzzy blob with a plus artifact; Lava Lamp/Bouncing Balls cover it |
| 244 | `a-peak-integrator.js` | A Peak Integrator | delete | B00 | debug beat meter, hard-coded 144 px |
| 245 | `accelerometer-level-example.js` | Accelerometer level example | delete | B00 | sensor tutorial (static bar without a sensor) |
| 246 | `amoeba.js` | amoeba | delete | B00 | dim blobs with dropout dimples; not library-grade, Lava Lamp strip covers blobs |
| 247 | `angry-xmass-3d.js` | Angry Xmass 3D | delete | B00 | per-frame random strobe, photosensitivity hazard |
| 248 | `animated-asterisks-2d.js` | Animated Asterisks 2D | delete | B00 | aliases to a frozen icon; spokes covered by Searchlights/Spiral Twirls |
| 249 | `block-reflections.js` | block reflections | delete | B00 | garish mirror chevrons; nothing distinct to keep |
| 250 | `bubble-column.js` | Bubble Column | delete | B00 | dark strip with 1-px blips; weak |
| 251 | `color-bands.js` | color bands | delete | B00 | static-looking barcode moire; not library-grade |
| 252 | `color-bands-buffered.js` | color bands (buffered) | delete | B00 | buffering demo of a weak barcode look |
| 253 | `crossfading.js` | Crossfading | delete | B00 | playlist crossfade framework; device does this |
| 254 | `cube-fire-3d.js` | cube fire 3D | delete | B00 | white-flood blob; Plasma covers 3D volumes |
| 255 | `distance-function-kaleidoscope-2.js` | distance function kaleidoscope 2 | delete | B00 | black 60% of the time; over array budget |
| 256 | `easing-library-v1-0.js` | Easing Library v1.0 | delete | B00 | math graph; easings are builtins |
| 257 | `example-button-w-debounce.js` | Example - Button w/ debounce | delete | B00 | tutorial; debounce snippet to docs/lang.md |
| 258 | `example-color-hues.js` | Example: color hues | delete | B00 | tutorial strobe |
| 259 | `example-modes-and-waveforms.js` | Example: modes and waveforms | delete | B00 | tutorial test card |
| 260 | `example-smooth-speed-slider.js` | Example: Smooth Speed Slider | delete | B00 | plain rainbow; accumulate-phase tip to docs |
| 261 | `example-time-and-animation.js` | Example: time and animation | delete | B00 | tutorial test reel |
| 262 | `fast-palette-blending.js` | Fast Palette Blending | delete | B00 | palette-manager demo; setPalette exists |
| 263 | `flash-posterize-music-sequencer-framework.js` | Flash Posterize + Music Sequencer framework | delete | B00 | 700-line framework, one weak effect |
| 264 | `glitch-bands.js` | glitch bands | delete | B00 | garish lattice; glitch look lives in Oil Slick |
| 265 | `lightbulb-crank-hue-to-complete.js` | Lightbulb - Crank Hue to Complete | delete | B00 | single-installation exhibit code |
| 266 | `m5stack-hex-panels.js` | M5Stack Hex panels | delete | B00 | hardware-specific demo reel of weaker duplicates |
| 267 | `matrix-2d-pulse-edit.js` | matrix 2D pulse edit | delete | B00 | buggy derivative, nearly black |
| 268 | `midpointdisplacement1d.js` | MidpointDisplacement1D | delete | B00 | hue-scrolled terrain barely legible; Night City/Topo cover skylines |
| 269 | `millipede.js` | millipede | delete | L22 | abstract rainbow crawl; slug reused by the creature |
| 270 | `multimap-simpledemo.js` | multimap simpledemo | delete | B00 | framework demo; scenes do regions |
| 271 | `multisegment-demo.js` | Multisegment Demo | delete | B00 | zone framework; strip scenes (E3) do segments |
| 272 | `music-sequencer-for-v2.js` | Opening Act | delete | B00 | subset of v3 framework, 1D only |
| 273 | `music-sequencer-for-v3-only.js` | Main Stage | delete | B00 | 730-line framework unreadable on panels; beat sequencing is an engine ticket |
| 274 | `nano-orbital.js` | Nano Orbital | delete | B00 | install-specific test pattern |
| 275 | `nyan-lights.js` | Nyan Lights | delete | B00 | copyrighted character; original cat runner scene later (SC-D) |
| 276 | `perlin-fire-wind-tunnel.js` | perlin fire wind tunnel | delete | B00 | coffee-swirl, not fire; 172 ms on metal |
| 277 | `post-process-chain.js` | Post-Process Chain | delete | B00 | feature demo; snippet to docs/lang.md |
| 278 | `rock-sparks.js` | Rock sparks | delete | B00 | drives a GPIO relay every frame; searchlight idea salvaged in Searchlights |
| 279 | `shimmer-crossfade-2d.js` | Shimmer Crossfade 2D | delete | B00 | framework of three filler looks |
| 280 | `slow-color-shift.js` | slow color shift | delete | B00 | generic sine islands |
| 281 | `sound-reactive-color-fade.js` | Sound Reactive Color Fade | delete | B00 | whole-panel strobe |
| 282 | `spin-cycle.js` | spin cycle | delete | B00 | restless one-formula chaser |
| 283 | `spinwheel-2d.js` | Spinwheel 2D | delete | B00 | collapses to dots |
| 284 | `stairmaster-2d.js` | Stairmaster 2D | delete | B00 | washed-out ramps that alias to still |
| 285 | `stargen-polar-2d.js` | StarGen polar 2D | delete | B00 | garbage on cartesian panels; figures covered by Heart/Holiday Star/Spiral |
| 286 | `static-random-colors.js` | static random colors | delete | B00 | static noise test card |
| 287 | `synchronized-random-numbers.js` | Synchronized Random Numbers | delete | B00 | PRNG demo; 8.4 ms/64x64 for a rainbow |
| 288 | `three-red-pixels-array.js` | Three Red Pixels (array) | delete | B00 | tutorial |
| 289 | `traffic.js` | Traffic | delete | B00 | dim aliased diamonds, misleading name; Plasma/Interference cover moire |
| 290 | `utility-palettes.js` | Utility: Palettes | delete | B00 | palette library; setPalette/paint are native |
| 291 | `wanderers.js` | wanderers | delete | B00 | static-looking confetti |
| 292 | `wichmannhill-prng.js` | Wichmann–Hill PRNG | delete | B00 | PRNG library |
| 293 | `bulk-sprite-scroll-2d.js` | Bulk Sprite Scroll 2D | to-scene | SC-D | a sprite over a wash: Sprite Parade scene (keyframed motion, E4) |
| 294 | `orv-christmas-tree.js` | Orv - Christmas Tree | to-scene | SC-C | static picture + twinkle: christmas-tree sprite + Silent Night scene |
| 295 | `rainbow-smiley.js` | Rainbow Smiley | to-scene | SC-B | static picture over a fill: smiley sprite + Rainbow Smiley scene |
| 296 | `scary-pumpkin.js` | Scary Pumpkin | to-scene | SC-C | Jack-o'-Lantern scene: carved sprite over Fire |
| 297 | `scrolling-text-marquee-2d.js` | scrolling text marquee 2D | to-scene | SC-A | 6-glyph font; Scrolling Message text-layer scene |
| 298 | `emoji-animation-2.js` | Emoji Animation #2 | to-sprite | SC-B | static expressions = face-moods sprite + Googly Face scene |
| 299 | `frame-rate-scan.js` | Frame Rate Scan | to-tools | B00 | docs/boards.md frame-rate procedure -> tools/patterns/ |
| 300 | `frame-rate-scan-fast.js` | Frame Rate Scan (fast) | to-tools | B00 | measurement fixture -> tools/patterns/ |
| 301 | `frame-rate-test.js` | Frame Rate Test | to-tools | B00 | test card -> tools/patterns/ |
| 302 | `mapping-helper-single-and-10x.js` | Mapping Helper Single and 10x | to-tools | B00 | mapping instrument -> tools/patterns/ |
| 303 | `performance-test-framework.js` | Performance test framework | to-tools | B00 | dropped: opbench/patbench are the instruments |
| 304 | `polar-mapping-helper-2d-3d.js` | Polar mapping helper 2D / 3D | to-tools | B00 | mapping instrument -> tools/patterns/ |
| 305 | `rgb-xyz-3d-sweep.js` | RGB-XYZ 3D Sweep | to-tools | B00 | axis sweep instrument -> tools/patterns/ |
| 306 | `rgbw-mapping-tester.js` | RGBW Mapping Tester | to-tools | B00 | colour-order test card -> tools/patterns/ |
| 307 | `rgbw-mapping-tester-hsv-version.js` | RGBW Mapping Tester - HSV Version | to-tools | B00 | dropped: duplicate of rgbw-mapping-tester |
| 308 | `tree-setup-pattern.js` | tree setup pattern | to-tools | B00 | map orientation instrument -> tools/patterns/ |

## B. Survivor specs

Format per survivor: **slug** "Display name" — dims — *absorbs* — controls (`//#`
real units; `a..b def d`; modes are `min=0 max=n-1 step=1`) — implementation — perf target
(from today's host 64×64 ms / est cycles/px) — fixes. "E1"/"E2" = the engine ops of §D.
Absorbed files are deleted in the survivor's batch. Look at an absorbed file's code and spec
for the behaviour named; do not carry over its bugs or bare 0..1 dials.

### B.0 Bug ledger (each item is owned by the survivor named; the batch must close it)

**Live control-scaling bugs (declared real units, handler still rescales as 0..1):**
1. `bessel-chaos` Transition: `//# 1..15` but handler does `1 + v*14` → up to 211. → L12 fixes in place.
2. `interference-2d` Frequency: `//# min=2 max=12 default=6` but `freq = 2 + v*10` → 62 at the default. → L11 `interference` must take rings/panel directly.
3. `marquee-chase` Blocks: `//# 1..10` but `1 + floor(x*9.99)` → default 5 gives 50 blocks. → L04 rewrite.
4. `spiral-2d` Arms: `//# 1..6` but `1 + floor(v*5.99)` → default 3 gives 18 arms. → L13 `2d-spiral-twirls` Arms control.

**Upside-down:** `palette-fire-2d` (hot band at the top: `heat + 0.9 − y*1.4` treats y=0 as the
base) → L07 Fire roots at y=1. `glowflow-3d-coord-transform-api-port` 2D shim (liquid strata in
the TOP half) → L09 glowflow fills from the bottom. Rule 5 of the header applies library-wide.

**Functional:** `sound-rays-frequency-bpm-reactive-1` is black whenever sensor data is present
(0/600 frames lit) → L17 `sound-rays` survivor must be lit under flat AND synthetic spectra.
`sound-spectromatrix-render2d` decays its 16×16 persist slots once per *pixel* → deleted (L17).
`rock-sparks` digitalWrites a hard-coded relay pin every frame → deleted (B00).
`stargen-polar-2d` renders garbage on cartesian maps → deleted (B00).
`perlin-simplex-noise-2d` renders confetti → L12 topo-map rewrite.
`sunrise` ships its 100× TEST speed-up as default and a hard-coded GPIO → folded, L19.

**Pattern-level brightness/gamma to remove:** `rainbow-v2` Brightness, `ryb-colors`
Brightness (L01), `sunrise-alarm-clock` Gamma (L19), `sunset` MASTER constant (L11),
`single-color-picker-wide-or-spot` picker V acting as brightness (L02), the 0.5 caps in
`color-pick-fade`/`color-fade-pulse` (L05), `green-ripple-reflections` (L08),
`quiet-blinkfade` (L03).

**Dark / mostly-black today (owner must fix):** breathing-gradient near-black third (L01);
glitter, quiet-blinkfade, chill-confetti on panels, color-twinkles (L03); color-pick-fade,
color-fade-pulse, edgeburst half black (L05); sparks-center 86 % black, firework-nova &
fireworks-finale half black (L06); doom-fire/doom-fire-2d bottom third only, ember-diffusion,
palette-fire (L07); aurora-2d top/bottom thirds, green-ripple-reflections (L08); raindrops-2d
REST 9 %, drip (L09); boids sparse, bouncing-balls-2d settles to black (L10); sinpulse-3d
black frames (L11); all-lasers-fire 80 % black, spotlights 2D slice, radar afterglow,
spirograph trail too short (L14); fireflies (L16); sound-rays-bpm (L17); lightning-strike
13/160 lit, lightning-clouds, starfield-2d specks (L18); sunrise-2d, coronal-ejection-2d dim,
sun-rays mostly black (L20); carrie star 85 % black (L21); blinky-eyes 2/3 black, spider
off-screen gaps (L22); frogger dim, dbz dim (L24); xorcery near-black frames (L25);
mandelbrot black/solid frames, infinity-flower mostly black (L29); crosstown starts black,
metaballs dim (L30); line-dancer brown melt trough (L28); typing-heatmap (L02).

**64×64 array-budget failures (all 25 are `≥3 × array(pixelCount)` strip patterns) — owner:**
novas, bustle (L30); fireblobs, heatshivers (L07); aurorashivers (L08); 4th, fireworks-finale,
rocket-by-tony-hampton (L06); neutronorbit (L29 orbits); lightning-strike (L18); xmasflies
(L16); audio-volume-meter, sound-starburst-2 (L17); rainbow-comet, meteor-shower,
christmaspewpew, pew-pew-pew (L05); chasing-rainbows-hsluv (L01); fairies (L03);
cellular-automata-1d (L25); bouncing-balls-rgb (L10); color-bands-buffered,
flash-posterize, both music sequencers (deleted, B00). Every survivor must pass
`luxel check --grid 64x64`.

**Hard-coded 16×16 / 15.99 indexing (rule 4):** bouncing-balls-2d, bulk-canvas-ripples-2d,
continuous-cellular-automata, cyclic-cellular-automata-2d, digital-rain-2d, doom-fire(-2d),
falling-sand-2d, flow-field-2d, fractal-flower, lissajous-curve-tracer, matrix-rain (32×8),
perlin-simplex-noise-2d, raindrops-2d, reaction-diffusion-2d, slime-mold-palette, snake-2d(-v2),
spirograph-2d, starfield-2d, swirlpool-2d, tetrix-2d, tixy, unstable-orbits-2d, breakout-2d,
boids-2d, curl-flow-2d, frogger-2d (board is fine; render via fillCanvas), sunrise-2d, sound-spectrum-analyser.

---

### L01 Rainbows & washes

**rainbow** "Rainbow" — both — *absorbs bulk-rainbow (implementation base), rainbow-v2,
rainbow-fonts, regenbogendrogen, rainbow-pinwheel, chevron-2d, chasing-rainbows-hsluv, ryb-colors.*
Controls: Cycle s 1..120 def 10 (accumulated phase, never `time()*speed` — the
smooth-speed-slider lesson); Repeats 0.25..8 step 0.25 def 1; Reverse toggle; Shape 0..4
(linear | chevron | radial | conic/pinwheel | mirror-fold); Angle deg 0..359 def 0 (2D linear);
Warp % 0..100 def 0 (the double sine-fold of rainbow-fonts/regenbogendrogen); Saturation %
0..100 def 100; Wheel 0..2 (HSV | OKLCH even brightness | RYB painter's wheel — ryb-colors'
conversion baked ONCE into a 64-stop `setPalette`, then `paint`). Implementation: strip and
2D-linear = one `fillGradient` (2D angle via `rotate()` + axis 1); chevron/mirror on strips =
two `gradientRange` (E1); radial/conic on panels = a precomputed radius/angle hue canvas
(min(grid,64)², built once) + per-frame `arrayAdd` phase + `fillCanvas(…,1)`. No brightness
control. Perf: linear ≤ 0.05 ms; radial/conic ≤ 0.3 ms (ryb-colors was 4.49, hsluv 3×array).

**rainbow-melt** "Rainbow Melt" — strip-native + 2D — *absorbs marching-rainbow,
marching-rainbow-buffered.* Add Motion 0..1 (melt | march: difference of two waves,
smoothstep instead of the hard clip, one fewer hue-nesting level, OKLCH so bands do not dip
in blue); Axis 0..2 for 2D (x | y | radius) so panels do not get row stripes. Perf ≤ 0.3 ms.

**color-wash** "Color Wash" (rewrite of breathing-gradient; renamed) — both — *absorbs
1-white-fade, 2-purple-fade, complements-3d, gradient-blue-purple-pink (its default preset),
twocolorhsvmix, color-blend.* Controls: Color A/B/C hsvPickers (V ignored); Stops 1..3 def 2;
Scheme 0..3 (pickers | complement of A | triad of A | blue-purple-pink); Motion 0..3 (breathe
| flow | breathe+flow | waves = color-blend's three travelling waves); Breath s 2..60 def 8;
Floor % 5..80 def 25 (it must never go black — the old 0.15 floor under gamma spent a third of
the cycle dark); Flow s/cycle 2..120 def 20; Repeats 0.5..6 step 0.5 def 1; Axis 0..2 (index |
x | y); Wave speed λ/s −2..2 def 0.3 (waves motion). Implementation: cook an OKLab palette
(`mixColors` at 16 sub-stops, only when a picker/scheme changes) into `setPalette`; strips draw
the 16 sub-stops as `gradientRange` spans (E1, OKLab-faithful, zero per-pixel work); panels
`paintCanvas` a 32×1 / 1×32 position canvas with filter 1 (E2); waves = a 64×1 (strip) or
32×32 (panel) canvas recomputed per frame. Perf ≤ 0.05 ms (breathing-gradient was 2.23 ms —
770 ms/frame on the 128×128, #935). Scenes use it with `P y` as a sky backdrop.

**opposites** "Opposites" — strip-native + 2D — polish: smooth the hue fold (smoothstep over
3 % instead of the hard fold that fringes the rims); 2D form: second swell pair along y,
product of the two lenses. Perf ≤ 0.6 ms.

**color-twinkle-bounce** — keep. Verify only (sheet + check-library).

### L02 Lamp, props & signals

**lamp** "Lamp" (rewrite of utilitycolortemp; renamed) — any (index space) — *absorbs
single-color-picker-wide-or-spot, naturallightsync, real-world-lights.* Mode 0..3 (Color:
hsvPicker hue/sat, V ignored | White: Kelvin 1500..10000 step 100 def 2700 | Follow the day:
Sunrise h 4..9 step 0.25 def 6.5, Sunset h 16..22 def 20, Day K 4000..6500 def 5000, Night K
1800..3000 def 2200, driven by `clockHour/clockMinute` | Preset 0..5: candle-warm 1900 K,
incandescent 2700, halogen 3200, fluorescent 4100, daylight 5600, overcast 6500). Spot toggle
+ Spot position % 0..100 def 50 + Spot width % 5..100 def 30 + Softness % 0..100 def 50
(single-color-picker's spot: E1 `splatIndex`/`fillRangeAA` on strips, `splat` on panels).
Kelvin→RGB: one shared fit function (copy the same function into sunrise-alarm-clock in L19;
there is no include mechanism). real-world-lights' candle flicker is NOT here (→ Candle).
Idle frame = one `fill()`. Drop the hidden demo sweep in the bottom 1/15 of the old dial.

**tv-simulator** "TV Simulator" — any — polish: renderBulk `fillGradient` for the spill falloff;
Scene s 2..60 def 8; Warm/cool bias % −100..100 def 0.

**policelights** "Police Lights" — strip-native + 2D halves — rewrite: left red / right blue
(two pickers); Sequence 0..3 (alternate | quad flash | rotator sweep | takedown white);
Split 0..1 (halves | alternating blocks) + Block px 4..32; flash rate hard-capped at 4 Hz per
colour and no full-panel red/blue alternation above 3 Hz (photosensitivity). fillRange only.

**saber** "Saber" (rewrite of saberdeploy-tutorial; renamed — avoids a trademark) —
strip-native (2D: blade up a centre column) — Ignite s 0.2..3 def 0.8 (easeOutCubic base→tip),
Retract trigger + Auto cycle toggle, Blade picker, Core % 0..100 def 60 (white core width),
Hum % 0..100 def 30 (low-frequency simplex flicker), Clash on any injected event (`readEvent`:
white `splatIndex` flash at a random point), brighter tip. No tutorial framing.

**typing-heatmap-2d** "Touch Glow" — panel — polish: deposit heat 3×, Cooling half-life s
0.5..10 def 3, thermal `setPalette` black→navy→magenta→red→yellow→white so hits reach white
(it reads as dim blue noise today), Layout 0..1 (free | keyboard rows), Demo keys/s 0..20 def 4
when no events arrive; render the heat canvas with `paintCanvas(…,1)`.

(notification-ripple is new content, §C; it absorbs crosshair-pulse-2d.)

### L03 Twinkle & sparkle

**twinkle** "Twinkle" — any (index space; scatters naturally on panels) — *absorbs twinkle-2,
twinkly-stars, chill-confetti, blink-fade, quiet-blinkfade, sound-blinkfade, autumn-colors,
spring-colors, christmas-rg-fade, halloween-color-twinkles, color-twinkles, firework-dust,
glitter.* Palette 0..9 (frost icy-white — today's default | picked (Color hsvPicker) | hue
range (Low/High hue deg 0..360, twinkle-2) | rainbow (blink-fade) | drift (chill-confetti's
cyan→violet drift) | autumn | spring pastel (spring-colors' weighted accent pick) | Halloween
orange/violet | holiday red/green | classic RGBY); Density births/s per 100 px 0.5..60 def 6
(scales with pixelCount — panels are empty today); Attack s 0.02..1 def 0.08; Fade s 0.1..8
def 1.5 (blink-fade faded in 0.1 s = flicker); Peak % 20..100 def 100; Background 0..1 (black |
lit field = twinkly-stars, Field hsvPicker); Backdrop % 0..30 def 0 (glitter's tinted ground);
Sound toggle (energyAverage drives birth rate — sound-blinkfade). Implementation: one live
pool ≤ 512 entries (index, age, hue); `clear()` (or `fill` background) then stamp every live
entry at its envelope from a 32-entry attack/decay LUT (replaces `exp()` per pixel) with
`setPixel`. O(pool), zero `array(pixelCount)`. Perf ≤ 0.1 ms at 64×64 and 300 px.
alt: chill-confetti as survivor (range-00's pick) — twinkle has the better envelope and name.

**fairies** "_Fairies" (name kept — Jeremy runs it) — any — polish: drop briB/satB and the
pixelCount/2 spark loop; `fade(k)` + a constant birth rate of `setPixel(random)` gives the same
exponential twinkles at O(births); add Density births/s 1..200 and Hue spread deg 0..180 def 0.
Must stay index-space only (dimensionless). Today fails 64×64 above 2,771 px.

**glittering-jewels** "Glittering Jewels" — strip-native + 2D gems — polish: renderBulk
`clear()` + per jewel a halo `splatIndex` + a core `splatIndex` (mode 2) + a short white glint
splat; Blooms/s 0.2..10 def 2, Width % of strip 1..30 def 6, Glint % 0..100 def 40, Color 0..1
(hsvPicker | gem palette); panels: round `splat` gems. Perf ≤ 0.1 ms (was 4.83 ms — 20 jewels
× pow(…,10) per pixel; 5th worst in the library).

**wanderedges** "Wander Edges" — strip-native + 2D — polish: Rim hsvPicker, Lifetime s 1..20
def 6, Spawn /s 0.2..10 def 1.5, Footprint % 2..40 def 12; renderBulk with one capped array;
2D: pulses as radial fields summed on a 32×32 canvas through the mid-peaked palette
(contour rings) with `paintCanvas(…,1)`.

### L04 Holiday strings & marquee

**custom-sequences** "Color Sequence" — strip-native (panels via projection) — *absorbs
christmas-lights, christmaslights, christmasstretch, 2-colors, 3-color-rotation.* Colors 1..8
def 4; Color 1..4 hsvPickers; Preset 0..5 (custom | red/green | red/white/green | RGBY classic |
Halloween | patriotic) — colours 5–8 come from the preset; Run px 1..60 def 5; Gap px 0..10
def 1; Transition 0..3 (static | step | crossfade | stretch-slide = christmasstretch, without
its wrap hitch); Hold s 0.2..30 def 2; Chase px/s −60..60 def 3 (signed; default visibly
moving — it is dead-still today); Bulb softness % 0..100 def 40 (per-bulb falloff so blocks
read as bulbs); Blink s 0..10 def 0. Replaces all 20 bare dials (fifth-power responses).
`fillRange` per run, `fillRangeAA` (E1) for sub-pixel chase.

**twinkling-classic-xmas-strands** "Classic Christmas Strands" — strip-native — *absorbs
static-christmas-lights-4-colors (Twinkle 0 + RGBY palette).* Twinkle % 0..100; Auto-fade a
real toggle; Palette 0..2; Bulb px 1..8 def 1; per-pixel `random()` blanks → hashed per-pixel
schedule; renderBulk.

**christmas-candy-cane** "Candy Stripes" — both — *absorbs holiday-diagonal-stripes.* Two
pickers default red/white matched in OKLCH lightness (the red is maroon today), Palette 0..1
(two colours | red/white/green); 1-px smoothstep edges; Stripes, Speed, Width as today; 2D:
diagonal stripes at Angle deg 0..179 def 45 via `rotate()` + `fillGradient`-free stripe
function (no jaggy staircase).

**marquee-chase** "Marquee" — strip-native — rewrite (fixes bug 3): discrete bulbs every
Spacing px 2..8 def 3, theater-chase Step s 0.05..1 def 0.15, filament warm-up/cool-down
Filament ms 20..300 def 90, Tint 0..1 (incandescent Kelvin 2000..3500 def 2400 | hsvPicker);
*absorbs three-red-pixels-mathy:* Bulb px 1..4, Background hsvPicker (V honoured as a dim
underglow, default black). Drop the "UI controls tour" framing.

### L05 Scanner, meteors & pulses

**scanner** "Scanner" — strip-native (2D: vertical bar sweep) — rewrite; *absorbs kitt,
cylon, kitt-w-color-picker (ramped gap fill + picker), beat-bounce (BPM lock).* Color hsvPicker
def red; Rainbow toggle; Eyes 0..2 (single | mirrored pair = scanner.js | bounce-in-halves);
Sweep s 0.3..10 def 2; Tail s 0.05..3 def 0.6; Width px 1..20 def 3; Ease toggle (sine at the
ends); BPM lock toggle + BPM 40..200 def 120. renderBulk `fade(k(delta))` + `fillRangeAA` over
the span swept since last frame (no gaps) + `splatIndex` head. Zero arrays. ≤ 0.02 ms @300 px.
alt: kitt.js as survivor (range-01) — scanner.js already has the pair/rainbow modes, and
"KITT" is a TV trademark.

**meteor-shower** "Meteor Shower" — strip-native + 2D diagonal — *absorbs comets,
bulk-comet-trails, rainbow-comet (bounce), pew-pew-pew (laser bolts; its blue-channel quirk is
NOT carried), christmaspewpew (holiday palette), snake (colour modes).* Style 0..3 (meteors:
discrete heads, saturation-growing tail | comets: N crossing both ways | bolts: short hard
heads, per-bolt speed | bounce: triangle head, hue smear); Count 1..20 def 4; Speed strips/s
0.05..3 def 0.4; Tail s 0.1..4 def 1; Gap s 0..5 def 1.2 (meteors read as individuals);
Palette 0..4 (rainbow | warm | neon | holiday | picker); Launch flash toggle def off. Strip:
`fade` + `gradientRange` tails + `splatIndex` heads, head deposits interpolate the span crossed
(comets.js draws dashed trails today). 2D: `drawLine` streaks at Angle deg + `fade`.
Zero arrays (was 3 × pixelCount). ≤ 0.05 ms.

**edgeburst** "Edge Burst" — strip-native + 2D ring — polish: Period s 0.5..10 def 3, Band %
2..50 def 10, launch the next burst as the last collapses (half the frames are black today);
2D: ring from the centre via a precomputed radius canvas + `paintCanvas(…,1)`.

**fast-pulse-3d** "Fast Pulse" — any — *absorbs fast-pulse.* Keep the direction vector
normalized (an edge-on frame goes black), integer-power falloff by multiplication, rename.

**color-pick-fade** "Pulse Train" — strip-native + 2D — *absorbs color-fade-pulse.* hsvPicker
(V ignored) instead of hue/sat sliders; Color 0..2 (picker | scrolling rainbow | saturation
wash); remove the 0.5 brightness cap; Pulses 1..12, Width %, Lap s as today; 2D Axis 0..2
(x | y | radius).

### L06 Fireworks, rockets & sparks

**fireworks** "Fireworks" (rewrite of 2d-fireworks-fade; renamed) — both — *absorbs
fireworks-finale (strip path), firework-rocket-sparks (sparkly rocket trail), firework-nova
(ring shell), 4th (red-white-blue palette + bunting underlay).* Launch /min 5..120 def 30
(shells overlap; no black sky); Shell 0..4 (mix | peony | ring | willow | crackle); Palette 0..3
(mixed | gold | red-white-blue | pastel); Gravity % 0..200 def 100; Trail s 0.2..3 def 1;
Rocket sparks % 0..100 def 50; Bunting toggle (strip, def off: a dim `fillRange` underlay).
2D: renderBulk `fade(k)` + `splat(mode 1)` per spark at native resolution (drops the three
32×32 canvases — "64×64 upscaled blur"). Strip: `fade` + `splatIndex` (drops 3 RGB buffers).
Spark pool ≤ 256. ≤ 0.3 ms @64, ≤ 0.45 @128.

**rocket-launch** "Rocket Launch" (polish of rocket-by-tony-hampton; renamed — author name
off the slug and title) — strip-native (2D: vertical rockets) — *absorbs rainbow-rocket-sparks
(Exhaust 0..1: physical | strip-fixed rainbow).* Flight s 1..10 def 4, Size px 1..10 def 3,
Delay s 0..20 def 3, Boost × 1..5 def 2, Apex burst toggle def on; `fade` + `splatIndex`
(drops 3 buffers); 2D: `drawLine` exhaust + `splat` sparks.

**sparks** "Sparks" — strip-native — *absorbs sparks-center.* Origin 0..2 (start | centre |
both ends) with immediate relaunch (sparks-center sat dormant 86 % of the time); Trail s
0.1..3 def 0.6 (time-based, not per-frame); colour ramp white→orange→red by energy; `fade` +
`setPixel`/`splatIndex`.

### L07 Fire (L)

**perlin-fire** "Fire" — both — rewrite; *absorbs palette-fire-2d (its palette+fbm core —
upside-down bug fixed), perlin-fire-wind (gust/lean model as Wind), fire-red (Fire2012 strip
sim), fire-blue (palette), sparkfire (spark ignition), newfire (advected licks style).*
Shared: Palette 0..3 (classic blackbody | blue gas | green | custom hsvPicker hue);
Flame height % 20..100 def 70. 2D (has2DMap): heat rooted at y=1; two octaves of
`fillNoise3D` on W=min(gridWidth,32) × H=min(gridHeight,40) summed with `arrayAdd`, per-column
height/lean (Wind % −100..100 def 0 with gusts), `paintCanvas(…,1)` through a `setPalette`
black→red→orange→yellow→white; squares replace `pow(…,0.75/0.85)`. Strip: Fire2012 on ≤ 1,024
cells (R3) with Direction 0..3 (start | end | both ends | middle), cooling scaled by pixelCount
(fire-blue floods a 60 px strip today), Sparking % 0..100, Ignition 0..1 (random heat |
spark launches), Style 0..1 (classic | licks). Perf: 2D ≤ 0.5 ms @64 / ≤ 1.2 @128, est ≤ 900
cycles/px (from 2.14 / 5,394 — 38 ms at 16,384 px); strip ≤ 0.15 ms @300.

**doom-fire** "Doom Fire" — panel-native (strip: single-column path) — rewrite; *absorbs
doom-fire-2d (its smoother sway and flame-height-as-reach).* Spread-fire sim on
min(grid,48)² (one heat array ≤ 2,304), `paintCanvas` with the classic 36-step ramp via
`setPalette`; Flame height % 20..100 def 75 (only the bottom third burns today); Steps/s
10..60 def 30; Wind % −100..100 def 0; Dragon toggle. ≤ 0.4 ms.

**heatshivers** "Embers" — strip-native + 2D rising embers — rewrite; *absorbs fireblobs
(Drift 0), ember-diffusion (smear cooling style).* `fade(halfLife)` + per pulse an additive
`splatIndex` with a blackbody brush — zero arrays (was 5 buffers; fireblobs 4). Pulses/s
0.5..20 def 4, Drift % 0..100 def 30, Shiver % 0..100 def 40, Afterglow s 0.2..5 def 1.5,
Style 0..1 (pulses | smear). 2D: embers rise (y decreasing) as `splat`s. Bright enough to read
(ember-diffusion is nearly black). ≤ 0.05 ms @300.

### L08 Aurora & Ocean (L)

**aurora-2d** "Aurora" (slug kept: clock-over-aurora, painted-letters, spectrum-over-aurora
scenes) — both — *absorbs 1d-aurora-borealis (strip path: dome/age blobs as `splatIndex` mode 2
+ `fade`, OKLCH so overlaps glow), aurorashivers (Shiver style: afterglow streaks), coolaura
(Aqua swell style, ground much dimmer).* Style 0..2 (curtains | shiver | aqua swell); Palette
0..2 (green | violet-green | aqua); Speed s 5..120 def 30. 2D: second noise octave varies band
height so curtains fill more than the middle third; violet at curtain TOPS (palette by
height); the byte-exact per-cell rationale is void — use `paintCanvas(…,1)` (E2) at
min(grid,32)² for the 128×128. Header trimmed to ≤ 12 lines. 2D ≤ 0.5 ms; strip ≤ 0.05 ms.

**ocean** "Ocean" — both — *absorbs oasis (bidirectional trains + whitecap threshold),
green-ripple-reflections (Moonlit palette), ideas #53 Shoreline, PF midsummer-sea.* Strip
Mode 0..1 (open water: today's Pacifica + oasis | shoreline: run-up from swell + noise, wet-sand
band that dries back, lacy foam edge, every 7th wave bigger; one wetness array ≤ 1,024).
2D path = side-view seascape (midsummer-sea, laid out from the grid aspect, not portrait):
sky gradient, sun disc + halo at Sun height % 0..100 def 30, reciprocal-depth perspective
wave bands, swaying sun-path glitter as sparse `splat`s, beach strip with surf; per-row values
once per row. Swell s 2..30 def 8, Wave scale, Foam % 0..100 def 40, Palette 0..3 (night |
tropical | stormy | moonlit; 2D: sunset | dusk | night). 2D ≤ 0.6 ms; strip ≤ 0.1 ms.

### L09 Water & sensor liquid

**raindrops-2d** "Raindrops" — both — *absorbs ripples-2d (the visual target: bright crisp
cyan rings on navy; tap-to-splash via `readEvent`; Mode 1 "rings" = its analytic algorithm),
bulk-canvas-ripples-2d (bulk-array wave sim), drip (strip path: staggered drips with a fade
trail and a splash throwing droplets back up).* Sim at min(grid,32)² with `stencil2D` +
`feedback`, `paintCanvas(…,1)` water palette with a ≥ 15 % floor (REST 0.3² = 9 % is murk),
crest highlights. Drops/s 0.2..20 def 2, Damping s 0.5..10 def 3, Mode 0..1 (sim | rings),
Palette 0..2 (pool | moonlit | ink). ≤ 0.15 ms (fixed-size sim).

**glowflow** "GlowFlow" (polish of glowflow-3d-coord-transform-api-port; renamed) — any,
sensor showcase — *absorbs upward-waves-3d-using-accelerometer (Waves mode: bands rising
along the tilt-corrected vertical in the panel plane).* Fix the 2D shim: liquid fills the
BOTTOM. Mode 0..1 (liquid | waves); spark buffer → `fade` + N sparks/frame scaled by energy;
Tilt smoothing s 0.05..2 def 0.3; stop exporting the debug angle vars. Works with no sensor
(gentle default slosh).

### L10 Balls, flocks & pendulums

**bouncing-balls** "Bouncing Balls" (polish of bulk-bouncing-balls-2d; renamed) — both —
*absorbs 2d-bouncing-additive-primaries, bouncing-rgb-balls-2d (Lissajous drift + breathing),
bouncing-balls-rgb-2d (gravity), bouncer3d (nothing kept but the idea), bouncing-balls-rgb
(strip path + direction modes), bouncing-balls-hsv.* Motion 0..2 (billiard | gravity with
restitution spread | Lissajous drift); Palette 0..2 (rainbow | additive R/G/B | picker); Count
1..24 def 6; Radius % of panel 2..30 def 8; Breathing % 0..50 def 0; Trail s 0..2 def 0.3;
Speed panels/s 0.1..3 def 0.6. 2D `clear`/`fade` + `splat(mode 1)`; strip `splatIndex` add,
Direction 0..2 (up | down | from both ends). ≤ 0.05 ms.

**bouncing-columns** "Bouncing Columns" (polish of bouncing-balls-2d; renamed) — panel —
columns = gridWidth(); `fillRect` per column; Gravity panel-heights/s² 0.5..10 def 3; Stagger
s 0..3; relaunch balls individually so it never settles black.

**boids-2d** "Boids" — panel — rewrite; *absorbs time-flies-2d (Swarm 0..1: flock | jitter
flies).* Count 5..80 def 32 at native resolution; `fade(k)` + `drawLine` velocity streak per
boid (mode 1) + optional `setGlow`; hue by heading in OKLCH; Speed, Trail s 0..3 def 0.6,
Separation % 0..100. ≤ 0.2 ms at both 64 and 128.

### L11 Interference, plasma & waves

**interference** "Interference" (polish of 2d-sinc-theta-theta; renamed) — panel (strip: line
cut) — *absorbs interference-2d (fixes bug 2).* Style 0..1 (RGB sinc: three additive
primaries | palette rings: one drifting OKLCH hue, luma-rich — the ramp base scenes use);
Sources 2..5 def 3; Frequency rings/panel 2..12 def 6; Orbit s 5..120 def 30; integer powers by
multiply; guard d < ε (the hot speck); OKLCH through a 256-entry `setPalette` + `paint` (per-pixel
`oklch` was interference-2d's hidden cost). ≤ 0.9 ms (from 1.82), est ≤ 1,800.
alt: interference-2d as survivor — sinc is c4, carries the #929 perf history and Jeremy's
stored copy; keeping both names would be a duplicate.

**plasma** "Plasma" (polish of sinpulse-3d; renamed) — any (1D/2D/3D) — *absorbs
matrix-2d-honeycomb (Neon contours), matrix-2-tone-pulse (Two-tone: complementary pair with
black seams, hue picker), sunset (Ambient wedge: narrow rotating hue window).* Style 0..3; Hue
spread deg 30..360 def 360; drop the `/2` after cubing (black frames). Separable trig: per-column
sin and per-row cos tables in `beforeRender` (4,096 → 128 trig calls). ≤ 0.35 ms.

**soap-2d** "Soap" — panel — *absorbs spinning-plasma-2d (Rotation s 0..120, 0 = none).* Warp
fields on 16×16 via `fillNoise3D`, final field on 32×32, `fillCanvas(…,1)`; Flow s, Warp %
0..100, Scale, Colour cycle s, Palette 0..1 (marble | OKLCH). ≤ 0.25 ms (from 1.0; 3 simplex3/px).

**dna-helix-2d** "DNA Helix" — panel + strip fallback — *absorbs sinus (Ribbons = 1).*
Ribbons 1..6 def 2 (N = layered additive ribbons, OKLCH hue offsets, depth-dimmed back
strands); Turns 0.5..6 def 2; Rotation s; strand pickers; Vertical toggle; slow twist so the
composition changes. ≤ 0.5 ms (from 1.15).

**wavy-bands** "Wavy Bands" — panel — *absorbs halloween-wavy-bands (Palette 0..2: rainbow |
Halloween | Christmas).* `pow(v,1.4)` → `v*v`; one perlin per pixel. ≤ 0.5 ms (from 0.9).

### L12 Noise fields

**coral-plasma** "Neon Veins" (slug kept: sunrise.scene) — panel — rewrite; folds PF
glow-tendrils and PF a-big-hit. Style 0..2 (coral: |simplex| ridges, 2 octaves, one feature
per 6–8 px | tendrils: curl2-warped level set, white cores | neon tubes: zero-crossings of two
simplex fields in two colours). Field on min(grid,48)² via `fillNoise3D` → `paintCanvas(…,1)`;
Scale, Flow s, Width px 1..4, Palette 0..2 (deep sea | neon | coral). Luma-rich (it is a ramp
base). ≤ 0.4 ms @64 / ≤ 0.6 @128 (from 2.0 / 8.1; today it is TV static).

**topo-map** "Topo Map" (rewrite of perlin-simplex-noise-2d; renamed) — both — *absorbs
perlin-simplex-noise-1d (1D = a line through the same field);* folds ideas #27 and PF
contour-blobs. Style 0..2 (hypsometric map | neon contour lines | islands: sea level, black
sea); Contours 3..20 def 8; Flow s 5..300 def 60; Scale; Sea level % −50..50 def 0 (islands);
hypsometric `setPalette` (deep sea, shallows, beach, green, tan, snow); field on min(grid,48)²
via two `fillNoise3D` octaves; contour lines anti-aliased with smoothstep on `frac`;
`paintCanvas(…,1)`. Drop Show Progress / X Offset / Perlin-vs-Simplex dials. ≤ 0.3 ms.

**bessel-chaos** "Bessel Chaos" — strip-native + 2D radial — polish (fixes bug 1): clamp the
chirp at 0.4 cycles/px so a 60 px strip does not alias to noise; hsvPicker / palette; 2D mode
= chirp of radius.

### L13 Kaleidoscopes, spirals & tiles

**kaleidoscope-2d** "Kaleidoscope" — panel — *absorbs zoom-kaleidoscope.* Source 0..1 (blobs |
ridged exponential zoom); Segments 3..12 def 6; Zoom s; Contrast %; OKLCH palette option.
Evaluate the source once per frame on a wedge canvas (radius × angle, 32×16 via `fillNoise3D`
plus blob terms) and fold per pixel by lookup; drop zoom-kaleidoscope's two pixelCount
smoothing arrays (128×128 fail). ≤ 1.2 ms (from 2.9), est ≤ 2,000.

**2d-spiral-twirls** "Spiral Twirls" (slug kept: spark-field scene) — panel — *absorbs
spiral-2d (fixes bug 4: Arms 1..8 def 2), spiral-twirls-star-2d (Mask 0..2: none | star |
polygon, Size % 20..100);* folds PF chromatic-vortex (Chromatic toggle: per-channel radial
offset, Split px 0..4). Twist −3..3 turns, Spin s, Colour drift s 5..120 def 20 (the rainbow
must actually show), Glow %. ≤ 0.6 ms. alt: spiral-2d as survivor (range-03).

**tixy** "tixy" — panel (+ strip path) — *absorbs golden-tix (1D t,i,p path + accelerating
moiré preset; no slide-to-reset hack).* Grid from gridWidth()/gridHeight(), formulas in cell
units; evaluate once per CELL into a canvas + `fillCanvas` (est 7,600 → ≤ 600 cycles/px);
Formula −1..19 def −1 (−1 = auto reel); Sketch s 3..60 def 8; drop per-pixel `random()`
formulas and any that duplicate a standalone pattern.

### L14 Curves, beams & radar

**spirograph-2d** "Spirograph" — panel — rewrite; *absorbs lissajous-curve-tracer,
swirlpool-2d, unstable-orbits-2d;* folds ideas #29 Harmonograph and PF lissajous-weave.
Mode 0..6 (spirograph | Lissajous | rose | harmonograph: damped, restarts when amplitude dies
| weave: N phase-shifted threads | epicycles: N orbiting arms | swarm: dots on a tumbling
Lissajous); Ratio A/B 1..8 (eased morph between integer ratios); Phase deg; Trail s 0.5..30
def 8 (the rosette must complete); Circuit s; Threads 1..12; Arms 2..8. renderBulk `fade(k)` +
`drawLine` between sub-steps (mode 1), OKLCH hue along the path, native resolution.
≤ 0.15 ms at 64 and 128.

**searchlights** "Searchlights" (rewrite of all-lasers-fire; renamed) — panel (+ cloud) —
*absorbs 3d-rotation-spotlights, spotlights-rotation-3d;* rock-sparks' searchlight mode is the
inspiration (that file is deleted in B00). Mode 0..2 (searchlights: 2–5 soft beams sweeping
from the bottom edge — wide `drawLine` + narrow core, additive | laser fan: chromatic beams
from a moving emitter, R/G/B offset lines, volley/rest s | cones: tumbling double cone,
Cone deg 5..60, Tumble s — the 3D idea projected). Beams 1..8 def 3; Sweep s 2..30 def 8;
Haze % 0..30 def 8; beam pickers. ≤ 0.2 ms (from 2.5).

**radar-2d** "Radar" — panel — *absorbs angle-and-radius-from-coordinates (Hue-by-radius
toggle).* Persistent frame as phosphor: `fade(k)` + sub-stepped centre→rim `drawLine` + blips
`splat` when the beam passes + bezel as 32 cached segments; Sweep s 1..20 def 4, Persistence s
0.5..10 def 3, Contacts 0..12 def 5, Colour 0..2 (green | amber | picker). ≤ 0.1 ms (from 2.5).

### L15 Flow & agents

**curl-flow-2d** "Curl Flow" — panel — rewrite; *absorbs flow-field-2d (Field 0..1: curl |
noise heading; Colour 0..1: speed | heading).* Count 20..300 def 150 specks at native
resolution; `fade(k)` + `setPixel(gridIndex)`/small `splat`; one `curl2` per speck; OKLCH;
optional glow. ≤ 0.25 ms at 64 and 128. Message Board scene base.

### L16 Night meadow & fireflies

**fireflies** "Fireflies" — both — *absorbs colourful-fireflies, slowflies (Drift style with
ghost trail), xmasflies (holiday drift palette).* Style 0..2 (blink | drift | holiday drift);
Colour 0..3 (warm | rainbow | holiday RGBG | picker); Count per 100 px 1..20 def 4; Lifetime s
1..10 def 3; Trail s 0..3 def 0.8; flash envelope fast attack / slow decay. Strip `fade` +
`splatIndex`; 2D flies wander x/y as `splat` (add) with glow. alt: range-03 sent xmasflies
to sparks; it is a drifting fly, not a fountain.

### L17 Sound (L)

**sound-spectrum-analyser** "Spectrum Analyzer" — both — *absorbs
sound-music-spectrum-visualizer (strip segments), audio-volume-meter (Level/VU mode).* Mode
0..1 (bands | level); Bands 4..32 def 16 or bars = gridWidth()/Bar px 1..8; Peak hold s 0..3;
Colour 0..2 (scrolling rainbow | gradient by height | picker); peak-normalised AGC with a
per-band tilt EQ (treble shows); smooth only the N band values. 2D `fillRect` per bar + peak
row; strip `fillRange` per segment. Drops the 5 per-pixel tables (64×64 fail).

**spectrum-field** "Spectrum Field" (polish of sound-spectrokalidamandala; renamed) — both —
*absorbs sound-spectro-kalidastrip (linear fold = strip path), sound-spectroblots-pow-fade
(noise blobs: band map on a coarse canvas refreshed every 0.5 s, 8–12 bands),
sound-spectromatrix-agc (plasma; its canvas + fillCanvas model is the implementation model),
sound-spectromatrix-render2d (nothing).* Field 0..3 (rings | plasma | noise blobs | linear
fold); target fill 20 % (white-hot at 33 %), whiteout capped, AGC starts low (no blown-out
first second); trails via `fade` (no pixelCount array); radius/angle precomputed on a
min(grid,32)² canvas, `fillCanvas(…,1)`. ≤ 0.4 ms (from 1.35).

**sound-rays** "Sound Rays" — strip-native — *absorbs sound-rays-frequency-bpm-reactive-1
(Speed 0..1: fixed | follow tempo, BPM factor × 0.25..4).* AGC: peaks, not the mean, reach full
scale (today it rails to a solid rainbow); lit under flat and synthetic spectra (bug ledger);
one capped history array (scrolling needs it) + `fillHSV`.

**starburst** "Starburst" (polish of sound-starburst-2; renamed — sound is optional now) —
strip-native — *absorbs white-rainbows (the non-sound default).* Sound toggle; Leaders 1..8
def 2; Rainbow toggle (a real one); Rainbow repeats 1..6; drop the unused age array; decay
scaled by delta; `fade(k)` by rising/falling + white head + 2–3 px coloured tail via
`gradientRange`. Passes 64×64 (fails today).

**light-organ-2-0** "Light Organ" — strip-native + 2D columns — *absorbs
light-organ-sensor-board (Colour 0..1: rotating slots | fixed band hues bass red / low orange /
high violet).* Sensitivity dB −20..20, Decay ms 50..2000 def 300, Bar px 1..32 def 6; beat
accent = a true white flash (grey dots read as dead pixels); 2D: four vertical columns.

### L18 Sky & weather

**thunderstorm** "Thunderstorm" — both — rewrite on lightning-strike's engine; *absorbs
lightning-strike (leader / return stroke / restrike state machine), lightning-clouds (2D
clouds), b-lightning-flashes, rainstorm (rain streaks).* Strikes/min 1..60 def 20; Rain
drops/s 0..200 def 40; Wind deg −45..45 def 10; Cloud floor % 0..40 def 15 (never fully black);
Bolt hsvPicker def blue-white. Strip: `fade` decay (drop hot/glow buffers — 3 × pixelCount
fail) + `fillRangeAA` channel + rain glints. 2D: clouds = `fillNoise3D` 32×32 +
`paintCanvas(…,1)` lit by the flash; forked bolt = ≤ 24 `drawLine` segments by midpoint
displacement; rain = `drawLine` streak particles + `fade` motion blur. ≤ 0.3 ms.
alt: keep the file name lightning-strike — "Thunderstorm" is the better name and the file
already exists, so the survivor takes it.

**starfield-2d** "Starfield" — panel — rewrite; folds ideas #56 Star Trails. Mode 0..2 (warp:
`drawLine` prev→projected per star, 60–120 stars scaled to panel, size/brightness by depth,
banking centre | drift: calm twinkling night sky — the scene base | trails: long-exposure
arcs round an off-centre pole, occasional meteor, slow reset fade). Warp 0..100 % def 40 (a
real-unit replacement for sliderWarp — SC-B's Space Cadet sets it), Stars per 1000 px 5..60
def 20, Tint deg. `fade`-based. ≤ 0.15 ms at 64 and 128.

### L19 Clocks

**analog-clock** "Analog Clock" (rewrite of rgbclock-2d; renamed) — both — *absorbs
2d-clock-with-hand-color-pickers (hand pickers), pixelclock (strip ring clock: AA sweeping
hands, 12-o'clock and 5-minute ticks, interpolating hour hand, 24 h / reverse toggles).* 2D:
face ring + 12 tick `drawLine`s + three tapered `drawLine` hands (mode 1 in RGB style keeps
rgbclock's additive-overlap idea); Style 0..2 (classic | RGB additive | minimal rings); Smooth
sweep toggle; three hand pickers; size from min(gridWidth, gridHeight). ≤ 0.1 ms (from 3.2).

**sunrise-alarm-clock** "Sunrise Alarm" — strip-native — *absorbs sunrise (Sunrise-now
trigger + Duration min 1..60 def 20; its test speed-up and GPIO branch are dropped).* Remove
the Gamma slider (device output setting); Cloudiness % and Vignette % real units; vignette
precomputed once; cloud noise on 32 strip samples joined by `gradientRange`. est ≤ 600.

### L20 Sun, eye & cosmos (L)

**coronal-mass-ejection** "Coronal Mass Ejection" — panel — *absorbs coronal-ejection-2d
(Arms 1..12 def 1, Twist, Mirror toggle), sunrise-2d (Rise-on-start toggle, flares as
`splat`s).* Default stays solar (blackbody palette); Hue cycle s 0..120 def 0 (range-00 vs
range-01 conflict: solar wins, cycling is opt-in). Turbulence on a 64-angle × 24-radius polar
canvas per frame, per-pixel lookup. ≤ 1.0 ms, est ≤ 1,500 (from 2.7 / 4,665).
alt: coronal-ejection-2d as survivor (range-00) — CME is c4 with the eruption system.

**sun-rays-through-trees** "God Rays" — panel — rewrite: per frame 64 angular bins of 1D
simplex (canopy occlusion), per pixel bin lookup by angle × radial falloff + haze; sun at the
top; warm OKLCH ramp; Shafts 4..32 def 12, Drift s, Haze % 0..60 def 25. Visible shafts across
the whole panel. ≤ 0.6 ms (from 1.9).

**fire-eye** "Fire Eye" (polish of eye-of-sauron; renamed — franchise name dropped) — panel
— *absorbs eye-of-sauron-with-movement (Gaze % 0..100 def 0: fixed stare → dwell-scaled
saccades, keeping this file's crisp slit pupil).* Ridged field on a polar canvas once per
frame; pupil analytic. ≤ 1.0 ms (from 2.2).

### L21 Holiday figures & candle

**holiday-star** "Holiday Star" (polish of carrie-s-holiday-star-2d; renamed) — panel —
*absorbs blue-holiday-star-2d (icy-blue preset).* Size % of panel 20..100 def 70 (85 % of the
panel is black today); hsvPicker + Hue drift toggle; faint background twinkle; constant-exponent
`pow` pair → abs-product falloff. ≤ 0.5 ms (from 2.1).

**us-flag-2d** "Waving Flag" — both — *absorbs us-flag (strip path), rainbow-flag (Pride in the
correct order).* Flag 0..4 (US | Pride | Progress | Trans | custom 2–3 stripe pickers);
gridWidth()/gridHeight() replace the mapPixels min-step scan; AA stripe edges; Ripple %,
Wave s, Sway px, Star twinkle % (strip: stripes scroll gently).

**light-thief** "Light Thief" (polish of the-grinch; renamed — trademark character) —
strip-native + 2D — a green thief sneaks along stealing bulbs and later puts them back; drop
the render2D index-order alias; 2D: bulbs on a drooping catenary, thief as a `splat`; renderBulk
`splatIndex` per bulb.

**heart** "Heart" — panel — polish: Fill 0..2 (rainbow | picker | OKLCH red-pink); Beat 0..1
(single | lub-dub double); rename display.

### L22 Creatures

**dire-spider-2d** "Dire Spider" — panel — *absorbs crawling-spider-2d (= Glow 0).* Haze on a
48×16 polar canvas once per frame + `paintCanvas`; spider = 16 `drawLine` leg capsules +
`fillCircle` body, add mode; off-screen gap ≤ 1 s; Glow % 0..100 def 60. ≤ 0.4 ms (from 7.4,
3rd worst).

**butterfly-2d** "Butterfly" — panel — polish: bounding-box early-out, per-frame lobe
constants hoisted, per-butterfly wing proportions/lobe angles randomized within legible bounds,
Count 1..4 (2+ on ≥ 48 px panels). ≤ 1.0 ms (from 4.2).

**millipede** "Millipede" (polish of millipede-1d-2d-controls, taking the slug of the deleted
abstract millipede.js) — both — segments as `splat`s/`drawLine` capsules along a wandering
curl/Lissajous path (cost scales with the body, not the panel); body hsvPicker/gradient;
strip path = the creature walking the strip. ≤ 0.3 ms (from 2.5).

**blinky-eyes-2d** "Blinky Eyes" — panel — polish: eyes sized by panel height (~70 %), white
sclera option, pupil + specular glint, eyelid sweeps down, looks up/down, random glances.

### L23 Arcade I

**breakout-2d** "Breakout" — panel — rewrite with PF breakout-arcade's juice: multiball / wide
paddle / fire-ball power-ups, combo-scaled flash, intercept-predicting AI that detours for
items, manual takeover from injected events (tap/drag on the preview, `POST /api/events`) —
not a knob. Board from the grid, bricks along the long axis (auto portrait/landscape); `fillRect`
bricks with outline, `splat` add for ball/items/bursts, `fade` trails, OKLCH row ramp; Speed
ticks/s, Paddle cells. ≤ 0.15 ms.

**snake-2d** "Infinite Snake" — panel — content replaced by snake-2d-v2's fillCanvas engine
(then delete v2); header essay removed; Board 0..32 def 0 (0 = auto gridWidth/2 capped 32);
faint border; head glides by `splat` between steps.

### L24 Arcade II

**frogger-2d** "Frogger" — panel — polish: background and objects brighter, frog 2×2 / pulsing
on ≥ 32 px, render via `fillCanvas` of the 16×16 board (256 cells, not per-LED lane tests).
≤ 0.1 ms (from 1.5).

**tetrix-2d** "Tetrix" — panel — polish: columns/rows from the grid (Column px 1..8 def 2);
`fillRect` per pile/block/flash; Drop rows/s; Palette; an all-columns row-clear moment.

**stacker** "Stacker" — strip-native — polish: Block speed px/s 5..120 def 30, Block px 1..20
def 6, Segments 1..6 def 3, Colour 0..2; slower default so stacking is watchable; flash on a
completed segment; `fillRange`.

**energy-duel** "Energy Duel" (polish of dbzbattlefinal; renamed — trademark) — panel —
`splat` add auras, `fade` dash trails, `fillCircle` cores, rings as 16-segment `drawLine`
polygons, glow; finale = two `drawLine` beams meeting and pushing; hues in degrees. ≤ 0.2 ms
(from 2.4).

### L25 Automata & bit fields (L)

**cellular-automata** "Cellular Automata" (polish of continuous-cellular-automata; renamed) —
both — *absorbs cellular-automata-1d.* Mode 0..1 (continuous | elementary); Rule 0..255
(inputNumber, elementary); Curated 0..4 (30 | 90 | 110 | 184 | random); panel = scrolling
history waterfall at min(gridWidth,64) columns via a canvas ring + `fillCanvas` (filter 0
elementary, 1 continuous); dark→bright `setPalette` by generation; strip = current generation
on ≤ 1,024 cells; watchdog reseed kept. ≤ 0.2 ms.

**reaction-diffusion-2d** "Reaction Diffusion" — panel — rewrite: Gray-Scott at min(grid,48)²
(2 fields × 2,304), `stencil2D` Laplacian + `arrayAdd`/`feedback`, the a·b·b term an interpreted
loop over ≤ 2,304 cells until #373/#453 land; 4–8 steps/frame; Preset 0..3 (spots | worms |
mitosis | coral) sets feed/kill; seeded with noise blobs; `paintCanvas(…,1)` luma-rich
palette. ≤ 1.0 ms.

**xorcery-2d-3d** "Xorcery" — any — polish; *absorbs sierpinski-rainbow-2d;* folds PF
retro-digital-tapestry. Op 0..2 (XOR | AND/Sierpinski with a seamless ×2 infinite zoom |
tapestry: coarse cells lit by bit logic of (col, row, stepped tick), smooth sine hue wash,
faint graph-paper lines — one value per cell → canvas + `fillCanvas`); OKLCH palette option;
fewer near-black frames. ≤ 0.5 ms.

### L26 Growth & packing

**slime-mold-palette** "Crystal Growth" (renamed display — Physarum is the real slime mould)
— panel — rewrite: canvas min(grid,64)²; frontier list so a placement is O(frontier); 20–60
placements/frame so growth lasts ~20 s; old image dissolves (no hard wipe); Palette 0..4 OKLCH
gradients; `fillCanvas(…,0)`.

**falling-sand-2d** "Falling Sand" — panel — polish: sim from the grid (cap 64); `setPixel` only
changed cells (frame persists) or `fillCanvas`; Pour grains/s, Spouts 1..3 (they wander), Sand
0..2 (gold | rainbow | picker).

### L27 Graphic shapes

**80s-kid-show** "80s Kid Show" — panel — polish: renderBulk shape ops (`fillRect`,
`fillCircle`, `drawLine` triangles/squiggles) instead of an SDF test over 18 objects per pixel
(est 15,401 cycles/px); fixed Memphis palette (teal, hot pink, yellow, purple) with a 1-px dark
outline; Count 3..18, Size % 5..40, Speed panels/s. ≤ 0.2 ms (from 2.7).

**bouncy-boxes** "Bouncy Boxes" — panel — polish: `fillRect` at native resolution (keep the
16-unit sim space); Boxes 1..8, Size %, Speed; the glitch sparkle becomes a real toggle.

**geometry-morphing-demo-2d** "Geometry Morph" — panel — polish: Hold s, Morph s, Spin s; OKLCH
contour palette option; 2×2 multi-figure mode on ≥ 48 px. ≤ 1.0 ms.

### L28 Tunnels & demo effects

**tunnel-of-squares-2d** "Tunnel" — panel — polish; folds ideas #2 Textured Tunnel. Style 0..1
(rings | textured: per-pixel angle & 1/r depth precomputed once (two arrays ≤ 4,096; on 128×128
a 64×64 canvas + `fillCanvas(…,1)`), scrolling ring/brick/noise texture, fog by depth,
wandering centre); Speed rings/s 0.1..4, Rings per octave 1..7, Twist deg, Sides 3..8 def 4.
≤ 0.6 ms.

**radiant-pulse-3** "Radiant Bloom" — panel — polish: radius/angle cached once; master cycle
shortened so the morph shows within a minute; OKLCH pastel cores. ≤ 0.5 ms.

**line-dancer-2d** "Line Dancer" — panel — polish: floor the zoom trough (no brown melt); Speed
×, Twist, Reflections 1..7; mirror alternate wedges (seamless); polar table. ≤ 0.4 ms.

**fractal-flower** "Fractal Flower" — panel — polish: canvas from the grid (cap 48); nine bare
dials → Depth 2..7, Petals 2..9, Ring radius %, Trail half-life s, Speed ×; wider default angles
(ferns and stars, not a donut).

### L29 Fractals, cells & physics (L)

**mandelbrot-2d** "Julia Morph" — panel — rewrite: min(grid,48)² canvas per frame,
`paintCanvas(…,1)`; smooth escape colouring (normalized count via a log LUT) through a deep
palette, darkening toward the set; c orbits the main cardioid's edge so every frame is rich;
slow zoom; Iterations 8..32 def 20. ≤ 0.8 ms (from 2.2).

**pendulum-wave** "Pendulum Wave" — both — rewrite: panel = one pendulum per column, a `splat`
bob swinging on y with a faint `fade` trail; strip = Pendulums 8..48 def 18 groups (no per-pixel
aliasing at 300 px); Cycle s 20..120 def 60; Palette 0..2.

**voronoi-2d** "Voronoi" — panel — rewrite; *absorbs ice-floes-2d (Ice: F2−F1 cracks, blue-white,
river drift with wrap);* folds ideas #30 Stained Glass. Style 0..2 (orbs | stained glass: jewel
OKLCH panes, dark lead came, sweeping sunbeam, per-pane noise | ice); Cells 4..24 def 10; Speed
(never reseeds on change). Squared distances, sqrt only for the winners; min(grid,48)² canvas +
`fillCanvas` on big panels. ≤ 0.6 ms (from 2.3). Stained-Glass Fire scene mask.

**lotus-bloom** "Lotus Bloom" (rewrite of infinity-flower-2d; renamed) — panel — folds PF
lotus-bloom. Mode 0..1 (golden bloom: tapered `drawLine` petals at golden-angle steps filling
~90 % of the panel, blooming open with easeOutBack, wilting, next species | rose mandala: polar
rose distance bands — outline, inner glow, crawling striped core, spoke aura — on cached r/θ,
smoothstep AA); Petals 3..12, Hold s, Size %. ≤ 0.5 ms.

### L30 Traffic, portal & lava

**crosstown-traffic-2d** "Crosstown Traffic" — both — rewrite; *absorbs bustle (strip path:
two-way pulses via `gradientRange` mode 2, two pickers, density, speed).* 2D: one `drawLine`
per car (24) in add mode so crossings glow, both directions, cars on screen from frame 1;
Width px 1..4, Crossing s 1..20, Hue cycle s. ≤ 0.15 ms (from 10.6 — the worst pattern in the
library).

**portal** "Portal" — both — rewrite; *absorbs novas (Embers palette, white-hot to crimson
without brown mud), scrolls (Bioluminescent palette, peak at the top so cores do not go dark,
no generation wipe).* Pulse pool summed into a field, banded gradient installed once with
`setPalette`, `paint()`: strip = one field array ≤ 1,024 + `render(index)`; 2D = rings expanding
from the centre (`render2D`, radius cached). Pulse interval s, Lifetime s, Spread %, Palette
0..3 (fire portal | embers | bioluminescent | void). ≤ 0.4 ms; passes 64×64 (novas fails).

### L32 Digital rain & glitch

**digital-rain-2d** "Digital Rain" — both — *absorbs matrix-green-waterfall-1d (strip drop
model + its real-unit controls), matrix-green-waterfall-2d (Cheap mode: stateless columns with
hash(col) cycle lengths), matrix-rain (Hue spread deg 0..90).* One column per pixel column
(gridWidth(), up to 128); `fade(k)` tails + `setPixel(gridIndex)` near-white heads; Hue deg def
120, Density %, Speed rows/s, Trail rows. ≤ 0.1 ms at 64 and 128.

### Scene/sprite conversions (owned by the scene batches)

| pattern | becomes | batch |
|---|---|---|
| scrolling-text-marquee-2d | scene Scrolling Message (text slot 0 over a dim Aurora band) | SC-A |
| emoji-animation-2 | sprite face-moods (16×16, 8 frames, 2 fps) + scene Googly Face | SC-B |
| rainbow-smiley | sprite smiley (white face, transparent features) + scene Rainbow Smiley | SC-B |
| scary-pumpkin | sprite jack-o-lantern (32×32, opaque surround, transparent carving) + scene Jack-o'-Lantern | SC-C |
| orv-christmas-tree | sprite christmas-tree (24×30, 3 frames) + scene Silent Night | SC-C |
| bulk-sprite-scroll-2d | scene Sprite Parade (needs E4 keyframes) | SC-D |

## C. New content

### C.1 New patterns (52), ranked

Source key: `ideas #N` = `$SCRATCH/libfix/ideas-online/ideas.md` entry N (read its full
paragraph); `PF x` = `docs/pattern-specs/patternflow/x.md` (prose spec — never the PF
source). Ideas already folded into survivors (not new files): #2 Textured Tunnel → tunnel,
#27 Topo Map + PF contour-blobs → topo-map, #29 Harmonograph + PF lissajous-weave →
spirograph, #30 Stained Glass → voronoi, #53 Shoreline + PF midsummer-sea → ocean,
#56 Star Trails → starfield, PF breakout-arcade → breakout, PF glow-tendrils + PF a-big-hit →
coral-plasma, PF chromatic-vortex → 2d-spiral-twirls, PF retro-digital-tapestry → xorcery,
PF lotus-bloom → lotus-bloom. All 26 PF specs scored ≥ 3 are covered (score-2 specs
wave-cell-grid, tile-waves, warped-wave are folded as modes; crossing-ribbons,
thermal-posterize, vector-field-flow, wave-saw are not built).

| rank | slug · "Name" | dims | form | source | effort | batch |
|--:|---|---|---|---|:-:|---|
| 1 | caustics · "Pool Caustics" | both | pattern | ideas #46 | S | L09 |
| 2 | truchet · "Truchet Weave" | panel | pattern | ideas #15 | S | L13 |
| 3 | quasicrystal · "Quasicrystal" | panel | pattern | ideas #32 + PF nodal-lattice | S | L11 |
| 4 | sorting · "Sorting" | both | pattern | ideas #17 | S | L23 |
| 5 | double-pendulum · "Double Pendulum" | panel | pattern | ideas #33 | S | L10 |
| 6 | lava-lamp · "Lava Lamp" | both | pattern | ideas #51 (+ metaballs-of-fire-2d, sound-lavablob) | M | L30 |
| 7 | life · "Life" | panel | pattern | ideas #19 (+ cyclic-cellular-automata-2d) | M | L25 |
| 8 | candle · "Candle" | both | pattern | ideas #52 (+ blue-holiday-candle-2d) | S | L21 |
| 9 | notification-ripple · "Notification Ripple" | both | pattern | ideas #63 (+ crosshair-pulse-2d) | S | L02 |
| 10 | plucked-string · "Plucked String" | strip | pattern | ideas #36 | S | L29 |
| 11 | night-city · "Night City" | panel | pattern | ideas #70 | S | L16 |
| 12 | mystify · "Mystify" | panel | pattern | ideas #11 + PF normal-map-plexus | S | L14 |
| 13 | meadow · "Meadow" | panel | pattern | ideas #50 + PF firefly-hollow | M | L16 |
| 14 | physarum · "Physarum" | panel | pattern | ideas #24 | M | L15 |
| 15 | attractor · "Strange Attractor" | panel | pattern | ideas #25 | M | L15 |
| 16 | halftone · "Halftone" | panel | pattern | PF pulse-dot-grid + wave-cell-grid + glyph-mosaic + hatched-warp + tri-march | M | L27 |
| 17 | oil-slick · "Oil Slick" | both | pattern | PF oil-slick-glitch + sliding-strata | M | L32 |
| 18 | rotozoomer · "Rotozoomer" | panel | pattern | ideas #1 + PF poincare-sphere + PF sliding-checker | M | L28 |
| 19 | shadebobs · "Shadebobs" | panel | pattern | ideas #4 | S | L13 |
| 20 | chladni · "Chladni Plate" | panel | pattern | ideas #26 | S | L12 |
| 21 | gravity-wells · "Gravity Wells" | panel | pattern | ideas #35 + PF mag-vortex | S | L15 |
| 22 | bokeh · "Bokeh" | panel | pattern | ideas #48 | S | L16 |
| 23 | galaxy · "Spiral Galaxy" | panel | pattern | ideas #57 | M | L20 |
| 24 | marbling · "Marbling" | panel | pattern | ideas #28 | M | L12 |
| 25 | rain-on-glass · "Rain on Glass" | panel | pattern | ideas #47 | M | L09 |
| 26 | word-clock · "Word Clock" | panel | pattern | ideas #66 | S | L19 |
| 27 | fibonacci-clock · "Fibonacci Clock" | panel | pattern | ideas #65 | S | L19 |
| 28 | block-clock · "Block Clock" | panel | pattern | ideas #67 | M | L24 |
| 29 | pong-clock · "Pong Clock" | panel | pattern | ideas #71 | M | L19 |
| 30 | day-ribbon · "Day Ribbon" | strip | pattern | ideas #64 | S | L19 |
| 31 | chomp-chase · "Chomp Chase" | strip | pattern | ideas #72 | S | L23 |
| 32 | sparkler · "Sparkler" | strip | pattern | ideas #45 | S | L06 |
| 33 | snowfall · "Snowfall" | both | pattern | ideas #49 | M | L18 |
| 34 | turmites · "Turmites" | panel | pattern | ideas #18 | S | L25 |
| 35 | dot-globe · "Dot Globe" | panel | pattern | ideas #8 | S | L20 |
| 36 | loop-ring · "Loop Ring" | panel | pattern | ideas #21 | S | L13 |
| 37 | maze · "Maze" | panel | pattern | ideas #16 | M | L26 |
| 38 | seasons-tree · "Seasons Tree" | panel | pattern | ideas #61 | M | L31 |
| 39 | jellyfish · "Jellyfish" | panel | pattern | ideas #54 | M | L22 |
| 40 | hud-ticker · "HUD Ticker" | both | pattern | PF hud-ticker-rows | M | L27 |
| 41 | bit-rain · "Bit Rain" | both | pattern | PF dither-bit-rain + kaleido-bit-rain | M | L32 |
| 42 | moire-gallery · "Moiré Gallery" | panel | pattern | PF origin + tile-waves + glitch-rings + warped-wave | M | L32 |
| 43 | orbits · "Orbits" | both | pattern | ideas #34 (+ neutronorbit) | M | L29 |
| 44 | copper-bars · "Copper Bars" | both | pattern | ideas #3 | S | L01 |
| 45 | twister · "Twister" | panel | pattern | ideas #9 | M | L31 |
| 46 | wireframe · "Wireframe" | panel | pattern | ideas #7 | M | L31 |
| 47 | pipes · "Pipes" | panel | pattern | ideas #12 | M | L31 |
| 48 | asteroids · "Asteroids" | panel | pattern | ideas #74 | M | L23 |
| 49 | circle-packing · "Circle Packing" | panel | pattern | ideas #22 | M | L26 |
| 50 | gears · "Gears" | panel | pattern | PF spinning-crosses | S | L27 |
| 51 | moon-phase · "Moon Phase" | panel | pattern | ideas #62 | S | L18 |
| 52 | newtons-cradle · "Newton's Cradle" | strip | pattern | ideas #37 | S | L05 |

Strip-capable new patterns: 16 of 52 (caustics, sorting, lava-lamp, candle, notification-ripple,
plucked-string, oil-slick, day-ribbon, chomp-chase, sparkler, snowfall, hud-ticker, bit-rain,
orbits, copper-bars, newtons-cradle). With ~55 of the 97 survivors strip-native, *any* or
*both*, strips keep ~70 first-class patterns after the campaign.

**Briefs** (algorithm sketch + what keeps it cheap; full colour in the source entry):

1. **caustics** — sunlit pool floor: a web of sharp bright lines dancing over aqua tiles with
   faint grout. Two `fillNoise3D` layers drifting in different directions on a min(grid,48)²
   canvas; caustic = `(1 − |n|)^k` of each (k by multiplication), product of the two, + faint
   `frac(x·N)` grout; `paintCanvas(…,1)` through an aqua→white palette. Strip: a sunlit shallow
   stream (one noise row, `gradientRange` spans). Controls: Speed, Scale, Sharpness 2..8 step 1,
   Grout %, Palette (pool | lagoon | lava). Aquarium/Tidepool base.
2. **truchet** — quarter-circle tiles joining into endless ribbons; every few seconds a tile
   spins 90° with easeOutBack, re-routing them; colour flows along each ribbon. Cell from
   `hash2(cell)` (+ a per-cell flip timestamp table, ≤ 256 cells); ribbon mask from distance
   to the two arc corners; phase along the arc + t drives OKLCH hue. Mode 0..1 (arcs | 10 PRINT
   slashes). Tile px 4..16, Flip /s, Width %.
3. **quasicrystal** — k plane waves at evenly spaced angles summed into a never-repeating
   rosette (7-fold on 64², 5 on 128²). Style 0..1 (bands: sum through a palette | nodal lines:
   thin glowing curves where the sum ≈ 0, constant width via smoothstep, glow — PF
   nodal-lattice). Precompute cos/sin of the angles; per pixel k `wave()` calls; Symmetry 5..9,
   Scale, Drift s, Palette.
4. **sorting** — a shuffled rainbow sorts itself live: bubble ripples, insertion slides, shell
   hops, heap pulses; compared pair flashes white; hold with a sparkle, then shuffle. Values in
   one array (≤ 256 bars on panels, ≤ 1,024 on strips, scaled readout); each algorithm a
   resumable step machine doing Ops/frame 1..64. Strip: `fillHSV` from the value array; panel:
   `fillRect` bar per column. Algorithm 0..4 (auto | bubble | insertion | shell | heap).
5. **double-pendulum** — three double pendulums from one pivot, starting a hair apart; unison,
   then chaos; tips draw fading ribbons; resync every ~60 s. Semi-implicit Euler, 4–8 substeps,
   clamp ω (16.16). `drawLine` rods, `splat` bobs, `fade` trails. Pendulums 1..5, Trail s,
   Damping %, Resync s.
6. **lava-lamp** — real lava lamp: wax blobs heat at the bottom, rise with a stretched neck,
   cool, sink, merge and pinch apart, in a tapered glass. Temperature drives buoyancy; field
   Σ r²/d² (squared distances, no sqrt) on a min(grid,32)×min(grid,48) canvas, soft threshold,
   vertical stretch ∝ |v|; `paintCanvas(…,1)`; static per-row glass width table. Palette 0..3
   (red/yellow | purple/blue | fire = metaballs-of-fire-2d | custom); Blobs 3..8; Bass swell %
   (sound-lavablob's idea, off without sensors). Strip: the same blobs in a 1D tube via
   `splatIndex` mode 2. Lava Lamp needs no scene.
7. **life** — Conway done beautifully: newborns flash white, age through an OKLCH ramp, the dead
   leave fading ghosts (gliders draw comet tails); stagnation (repeated population hash)
   triggers a reseed rain. Rule 0..4 (Life | HighLife | Day & Night | Brian's Brain | Cyclic =
   cyclic-cellular-automata-2d's spiral waves, resting state black). min(grid,64)² cells;
   neighbour counts with `stencil2D`; ghosts via `feedback`; `fillCanvas(…,0)`. Steps/s 2..30.
8. **candle** — a convincing lamp candle. Panel: teardrop SDF flame (narrow tip, round base,
   blue root, white core, warm halo lighting the "wall") sheared by sway, rare gutter + recovery;
   evaluated only in the flame's bbox; halo `splat`. Strip "tealights": every LED group its own
   candle with correlated low-frequency simplex flicker (`fillRange` per group). Palette 0..2
   (warm | icy holiday blue = blue-holiday-candle-2d | picker); Candles px/group 1..20; Draft %.
9. **notification-ripple** — HA workhorse: calm OKLCH breathe; an injected event (`readEvent`,
   type = channel) fires a colour-coded signal — ring burst, sweep, double pulse, cross burst
   (crosshair-pulse-2d) — then eases back; unread events keep a breathing corner dot until a
   "clear" event. Queue ≤ 8 animations with easeOutBack/easeOutElastic envelopes; `fillCircle`/
   `drawLine` on panels, `fillRangeAA` on strips. Demo events/min 0..10 def 2 so it is alive
   in the gallery.
10. **plucked-string** — the strip is a guitar string: a pluck sends a kink that splits, bounces
    off the fixed ends, decays into shimmering harmonics; warm for up, cool for down. 1D wave
    equation on two arrays ≤ 1,024 cells (scaled readout), `stencil2D(…, n, 1, …)` neighbour
    sum, `fillHSV` display. Pluck /min, Damping s, Tension.
11. **night-city** — skyline silhouettes with lit windows flicking on and off (tungsten, TV
    blue, the odd party colour), a blinking red beacon, an occasional plane, dusk-to-midnight sky
    by `clockHour`. Stateless: heights `hash(col block)`, windows `hash2(cell) ≤ occupancy(hour)`
    with a slow epoch; `fillGradient` sky + `fillRect` windows (only redrawn on change).
12. **mystify** — two polygons whose corners bounce off the walls, each leaving a fan of fading
    echoes; Mode 1 = plexus (PF normal-map-plexus): drifting nodes linked when close, link
    brightness fading with distance, link hue = its angle on an OKLCH wheel. `fade` + ≤ 40
    `drawLine` (mode 1). Shapes 1..3, Vertices 3..6, Echo s, Link distance %.
13. **meadow** — dusk meadow: rolling hill silhouette, 60–120 grass blades (2–3 segment curves,
    tips bend with gust fronts rolling across), warm sky with a few lit clouds, fireflies blinking
    over the grass (PF firefly-hollow). `fillGradient` sky, blade `drawLine`s back-row darker,
    fireflies as `splat` add with fast-attack/slow-decay flashes; layout from the grid aspect.
14. **physarum** — 300–600 agents sense a trail map at three angles, steer, deposit; vein
    networks self-organise, merge, reroute, gold on indigo. 64×64 trail canvas, `canvasGet`
    sensors, deposits, `blur2D` + `feedback` decay, `paintCanvas(…,1)`. Agents 100..800,
    Sensor angle deg, Decay s, Palette.
15. **attractor** — a Clifford/de Jong attractor as luminous dust; parameters drift on slow
    sines so the nebula unfurls. ~2,000 iterations/frame accumulate hits in a 64×64 density
    canvas; slow `feedback` decay; sqrt-ish tone map via a palette LUT; `paintCanvas(…,1)`.
    Points/frame 500..4000, Morph s, Palette.
16. **halftone** — a flowing field sampled once per CELL and drawn as shapes: Shape 0..4 (dots
    whose radius is the value — halftone pulse | squares | triangles marching on a triangular
    lattice — tri-march | glyphs ordered by ink weight: empty, dot, plus, slash, square —
    glyph-mosaic | print tiers: solid / black / stipple / hatch — hatched-warp riso poster).
    Field = `simplex3` at cell centres or a diagonal wave; checkerboard OKLCH pair; Cell px
    4..16. A few hundred shape ops per frame, no per-pixel field. Optional `frequencyData` →
    columns (dot-matrix spectrum).
17. **oil-slick** — thin-film iridescence: a domain-warped `simplex3` field banded into tight
    contours, coloured through an interference `setPalette` (magenta/gold/cyan/green fringes),
    separation easing over time; Glitch % 0..100 slices thin rows sideways with occasional
    stutters (default 0 = calm); Strata mode (PF sliding-strata) shears noise bands in opposite
    directions with dark seams. Canvas min(grid,48)², `paintCanvas(…,1)`; strip: 1D band flow.
18. **rotozoomer** — a checker/emblem texture rotating and zooming at once (rotate+scale
    transforms, one `frac/floor` per pixel, or a 16×16 texture via `canvasGet`). Mode 0..2
    (rotozoom | fisheye: scrolling grid through a spherical lens with a mirrored outer rim — PF
    poincare-sphere, constant-width lines | drawers: checker rows sliding out of phase with
    staggered easeInOutBack, filled squares vs hollow frames — PF sliding-checker).
19. **shadebobs** — invisible brushes trace Lissajous paths adding heat that wraps round the
    palette, so contour bands build and dissolve. 32×32 accumulation canvas, soft disc stamps,
    `arrayScale` decay, `paintCanvas(…,1)` of `frac(value)`. Bobs 1..6, Decay s, Palette.
20. **chladni** — sand on a vibrating plate: grains gather on nodal lines of
    cos(nπx)cos(mπy) − cos(mπx)cos(nπy), (n,m) stepping through a list with crossfades.
    Mode 0..1 (field: 1 − smoothstep(|f|) via per-column/per-row cos tables | grains: 400
    particles jitter ∝ |f| and settle, `setPixel` + `fade`). Hold s, Palette.
21. **gravity-wells** — 2–3 wandering wells; ~150 particles fall in, spiral into accretion
    discs, get flung when wells pass close; colour by speed. Mode 1 = vortex (PF mag-vortex):
    particles spiral in/out of the centre with Keplerian speed and long trails. Softened
    gravity (16.16-safe), `setPixel(gridIndex)`/`drawLine` step + `fade`.
22. **bokeh** — big soft defocused discs drifting in warm amber and cool teal with brighter
    rims, overlapping additively, faint pulsing; Mode hex tips. ~20 discs: `fillCircle` +
    rim `splat`, depth = size/speed/brightness; radius capped at 128².
23. **galaxy** — face-on spiral galaxy: yellow-white core, blue arms laced with dust lanes and
    pink knots; inner stars orbit faster (differential rotation, arms re-seeded slowly). ~500
    stars as `setPixel`/small `splat` add; dust/core glow on a polar canvas (`fillNoise2D` in
    r,θ) once per frame.
24. **marbling** — suminagashi: ink drops fall, each pushing earlier rings outward (closed-form
    area-preserving inverse), a comb drags chevrons, then it slowly clears. Analytic per-pixel
    inverse over ≤ 12 events on a 48×48 canvas; 4-colour OKLCH ring index; `paintCanvas(…,0)`.
25. **rain-on-glass** — night window: bokeh city lights behind glass, droplets bead, slide in
    jerky runs, merge, leave clear trails that re-fog; each drop shows a flipped refraction.
    Background canvas built once; ≤ 30 drops with a stick/slip machine; drop interiors sample
    the background flipped/magnified (`canvasGet`); fog mask `feedback`-decayed.
26. **word-clock** — dim grey letter grid; "IT IS HALF PAST TEN" lights warm white, words fade
    over on change, minute dots. Fixed 11×10 layout in the `tiny` font (fits 64 px; on < 48 px
    fall back to a 3-word "TEN PAST SIX" line); ≤ 20 `drawText` calls.
27. **fibonacci-clock** — five squares 1,1,2,3,5 in a golden rectangle: red = hours, green =
    minutes/5, blue = both, recoloured among valid solutions each refresh; 5 `fillRect` + dark
    borders, redraw once a minute.
28. **block-clock** — each HH:MM digit built live by falling, rotating tetromino-like blocks
    that lock into the digit; a changed digit dissolves and rebuilds. Authored decompositions
    of 0–9; 4 `fillRect` cells per piece; the locked frame persists. (Not called Tetris.)
29. **pong-clock** — two AI paddles rally forever; the score is HH and MM; the hour side misses
    on the hour, the minute side each minute. 3 `fillRect` + 2 `drawNumber` + dashed centre
    line, light glow.
30. **day-ribbon** — the strip is a 24 h timeline: night ends, dawn pinks, noon blue; a bright
    "now" marker at the clock; faint hour ticks; past hours slightly dimmed. `gradientRange`
    bands + `splatIndex` marker. Sunrise/sunset hour controls.
31. **chomp-chase** — a yellow chomper eats a dotted pellet line chased by four coloured
    ghosts; power pellet → ghosts flee blue; dots refill. Original shapes, no game assets.
    1D positions, pellet bitmap ≤ 1,024, `fillRange`/`fillRangeAA` per actor.
32. **sparkler** — a fuse creeps along the strip; the white-hot point throws arcing orange
    sparks both ways (pool ≤ 40, drag, life), smoke glow behind, pop at the end, rewind.
    `fade` + `splatIndex`, burnt section `fillRange`.
33. **snowfall** — three parallax flake layers through a dusky blue night, drifts building on
    the floor (per-column height array with angle-of-repose smoothing, slow melt), gusts blow
    powder off. Strip: gentle snowfall past a window. Flakes `setPixel`/`splat`; drifts one
    `fillRect` per column.
34. **turmites** — Langton's ant and multi-colour cousins cycling a curated list of rule strings
    (snowflakes, clouds, highways); OKLCH colours per run; hundreds of steps/frame on a 64×64
    state array; only changed cells `setPixel(gridIndex)`; restart on highway exit/timeout.
35. **dot-globe** — 200–400 golden-angle sphere dots spinning and tilting; near dots big and
    bright; optional continents from a `simplex3` read cached per dot at init. `splat` max mode.
36. **loop-ring** — a luminous ring whose noisy edge is a perfect loop (noise sampled on a circle
    in time); several concentric offset rings make a breathing iris. Per-pixel angle cached;
    noise only per 64 angle buckets; per-pixel lookup + falloff.
37. **maze** — a glowing mole carves a perfect maze by backtracking (explicit stack), then a
    rainbow flood fills it from the entrance (BFS distance = hue), the shortest path lights white
    and pulses, then it dissolves. ≤ 21×21 cells, a few steps per frame, `fillCanvas` of a 2×-cell canvas.
38. **seasons-tree** — a fractal tree grows branch by branch, sways (more at the tips), blossoms
    pink, greens, turns gold/red, drops leaves, stands bare under snow; repeat. Depth 7 (~127
    segments) expanded once; per frame `drawLine` + leaf `splat`s; season = slow phase into
    OKLCH hue tables.
39. **jellyfish** — 2–3 translucent bells pulsing upward (fast contract, slow relax) trailing
    sine-delayed tentacles in cyan/magenta, plankton sparkling as they pass. `fillCircle`/
    `splat` bells, ≈ 60 tentacle `drawLine`s, `fade` glow.
40. **hud-ticker** — lanes of a sci-fi HUD scrolling in alternating directions: progress bars
    that fill and reset, caution chevrons, blinking data dots, live oscilloscope traces, dim
    dividers; amber + cyan OKLCH pair. Per visible segment a few `fillRect`/`drawLine`; Lane px,
    Segment px, Speed. Strip: one lane of glyph segments. Optional `frequencyData` on one lane.
41. **bit-rain** — 1-bit Bayer-dithered glitch screen: falling chunk streams with a neon accent
    head, an XOR bit lattice substrate (Mask 0..7), a centre ripple; Symmetry 0..2 (none |
    mirror | four-fold kaleido with warped axes and sky-blue axis beams — PF kaleido-bit-rain);
    working beat-timed row tears that ease back. Streams as particles (`fillRect` chunks);
    background dithered once per frame on a small canvas. Strip: dithered falling chunks.
42. **moire-gallery** — a wall of tiles, each sampling a ripple on a deliberately coarse
    macro-pixel grid at a rising frequency so every tile aliases into a different moiré; Layout
    0..3 tile counts from the grid; Mode 1 = glitch rings (one big posterized ring field with
    sine wobble and eased VHS row tears — PF glitch-rings/warped-wave). Per-tile values into a
    canvas, `fillRect` macro-pixels; OKLCH black→grey→hue→white ramp; slow frequency drift.
43. **orbits** — a little star system: glowing sun, planets on ellipses speeding up at
    perihelion, a comet diving in on a slingshot with an anti-sun tail; Mode atom: electrons on
    tilted orbits round a pulsing nucleus. Strip path = neutronorbit's warm comets orbiting
    (bouncing) along the strip via `fade` + `splatIndex`. Leapfrog, softened gravity, ≤ 8 bodies.
44. **copper-bars** — glossy chrome bars swinging on sines, back-to-front, specular core; Kefrens
    mode stacks vertical bars into a writhing staircase; on a strip: metallic droplets passing.
    `fillGradient`/`fillRect` per bar row (panel), `gradientRange` per bar (strip); OKLCH
    lightness ramps.
45. **twister** — the Amiga twister: a square column twisting with a rippling angle, four
    coloured faces shaded by facing; ≤ 4 `fillRect` spans per row.
46. **wireframe** — rotating wireframe cube morphing to octahedron/icosahedron/torus knot;
    front edges bright, back dim; Glenz mode fills faces additively. ≤ 30 `drawLine`/frame +
    optional `fade`.
47. **pipes** — shaded 3D pipes grow cell by cell on a 16×16 occupancy grid with 90° turns and
    ball joints, cylindrical 3-step shading; fade and regrow when full. One `fillRect` per band
    per step; frame persists.
48. **asteroids** — vector rocks tumbling on a wrapped screen, a triangle ship rotating,
    thrusting (flame flicker) and firing at the nearest rock; rocks split; phosphor afterglow.
    ≤ 100 `drawLine` + `fade`.
49. **circle-packing** — bubbles spawn in free spots and inflate until they touch, freeze with a
    rim highlight in an OKLCH palette; when full they pop one by one (easeOutElastic) and regrow.
    ≤ 60 circles, O(n²) growth tests, `fillCircle` only for growing ones.
50. **gears** — a grid of counter-rotating cogs (3–6 spokes as `drawLine` capsules) that tick a
    quarter turn with easeOutBack and pause, neighbours meshing; axis-aligned housing frames;
    warm/cool OKLCH pair with glowing white edges. Cell px 8..32, Spokes 3..6, Tick s.
51. **moon-phase** — tonight's moon from the date (days since a known new moon mod the synodic
    month): cratered disc, terminator ellipse, earthshine, halo, a few stars. 32×32 canvas
    recomputed once a minute, `fillCanvas(…,1)`. Nocturne scene can use it instead of the sprite.
52. **newtons-cradle** — five chrome balls on the strip: the end ball swings out (eased), clacks
    with a white flash, the far ball leaps; energy decays, then re-lift. `fillRange` + OKLCH
    lightness ramps per ball, `splatIndex` flash.

**Backlog, NOT this campaign (BF files one ticket listing them):** voxel flyover (#5, L —
fixed-point projection), volcano (#58), hourglass (#43), galton board (#39), stable-fluids
smoke (#40), Lenia (#20), differential growth (#23), embossed light (#6), please-stand-by (#13),
sine scroller (#10), split-flap clock (#68), pomodoro (#69), jelly blob (#42), slinky (#38),
domino run (#44), falling leaves (#60), cloudscape day cycle (#55), waterfall mist (#59),
Hilbert walk (#31), cloth flag (#41), Invaders attract (#73).

### C.2 Example scenes and sprites that need NO engine change

All authored at the 32×32 design canvas per `library/README.md`; ≤ 2 `pat` layers each.
Layer plans are `$SCRATCH/libfix/scene-ideas/proposal.md` §1 (scene numbers in brackets) with
the `@pat/` references re-pointed to post-dedupe slugs as written here. Sprites are original
art (palettes/sizes in proposal §2); no recognizable commercial character.

**SC-A — info, clocks, text, sound (12 scenes, 5 sprites):**
night-clock [1] `@pat/starfield-2d` (drift mode); date-time [2] `@pat/interference` (palette
rings style); message-board [3] `@pat/curl-flow-2d` + `@spr/envelope`; weather-card [4]
`@pat/color-wash` (`P y`) + `@spr/cloud-sun`, `@spr/rain-tile` (hidden layer);
analog-date (proposal §3) `@pat/analog-clock` + date text; neon-sign [14]; light-boxes [29];
vignette-tint [30] `@pat/kaleidoscope-2d` + `@spr/vignette`; scrolling-message (from
scrolling-text-marquee-2d) `@pat/aurora-2d` at 70 % + band + `T slot 0`; spectrum-over-aurora
[24] `@pat/aurora-2d` + `@pat/spectrum-field` boxed; now-playing [25] `@pat/spectrum-field` +
`@spr/music-note`; painted-letters [15] `@pat/aurora-2d` + `@pat/soap-2d` in multiply (the
documented rainbow-text recipe). Sprites: envelope, cloud-sun, rain-tile, vignette, music-note.

**SC-B — ambient, nature, characters (13 scenes, 14 sprites):**
tidepool [5] `@pat/interference` + `@pat/raindrops-2d` (Mode rings, add); rainbow-ripples [8]
`@pat/soap-2d` masked by `@pat/raindrops-2d`; stained-glass-fire [9] `@pat/perlin-fire` masked
by `@pat/voronoi-2d` (Style orbs); lighthouse [10] `@spr/harbor` (new 32×32 coastline picture)
revealed by `@pat/radar-2d` mask; fireflies-at-dusk [11] `@pat/color-wash` `P y` + `@pat/fireflies`
add; aquarium [16, ideas #75] `@pat/caustics` + `@pat/twinkle` add + `@spr/seaweed` tile +
`@spr/fish` ×2 (flipx, `A` de-sync) + `@spr/bubbles-tile`; campfire [17] `@pat/starfield-2d` +
`@pat/fireflies` (ember ramp) + `@spr/moon` + `@spr/campfire`; space-cadet [18]
`@pat/starfield-2d` (`C sliderWarp` set to the new real-unit value) + `@spr/planet`, `@spr/rocket`,
`@spr/pixel-alien`; nocturne [ideas #77] `@pat/starfield-2d` (trails mode) + `@pat/meteor-shower`
add + `@spr/moon` + `@spr/mountains`; hearth [ideas #76] `@pat/perlin-fire` boxed low +
`@spr/log-pile` + an add colour wash + optional `T slot 0`; witchs-cauldron [7] `@pat/doom-fire`
(green ramp) + `@spr/cauldron`; googly-face (from emoji-animation-2) `@spr/face-moods`;
rainbow-smiley (from rainbow-smiley) `@pat/rainbow` + `@spr/smiley` fill.
Sprites: harbor, seaweed, fish, bubbles-tile, moon, campfire, planet, rocket, pixel-alien,
mountains, log-pile, cauldron, face-moods, smiley.

**SC-C — holidays & parties (7 scenes, 8 sprites):**
jack-o-lantern [19] (from scary-pumpkin) `@pat/perlin-fire` clipped to the sprite box under
`@spr/jack-o-lantern` (opaque surround, transparent carving) + `@spr/bat` ×2; silent-night [20]
(from orv-christmas-tree) `@pat/starfield-2d` + `@spr/christmas-tree` + `@spr/snowfall-tile`
tiled + crawling greeting; new-years-eve [21] `@pat/fireworks` + band + `T clock HH:MM:SS` +
greeting + `@spr/firework-burst`; fourth-of-july [22] `@pat/us-flag-2d` 45 % + `@pat/fireworks`
add + `@spr/firework-burst` ×2; candle-row [23] colour wash + `@pat/twinkle` add +
`@spr/candle-stick` tiled (menorah variant in comments); happy-birthday [13] `@pat/twinkle`
(rainbow palette) + `@spr/cake` + greeting + `T slot 0`; bulb-string [28] `@spr/bulb-string`
tiled across the panel (works on matrices today).
Sprites: jack-o-lantern, bat, christmas-tree, snowfall-tile, firework-burst, candle-stick, cake,
bulb-string.

Existing scenes stay untouched: clock-over-aurora (aurora-2d), heartbeat, loading, spark-field
(2d-spiral-twirls), sunrise (coral-plasma — SC-D adds sun keyframes). Their pattern slugs are
deliberately kept by L08/L13/L12 so no repoint is needed.

### C.3 Scenes blocked on an engine feature

Built in this campaign (SC-D, after BE2):
- **strip-zones** [27] (E3): `@pat/perlin-fire` on segment `0 0 75 0`, `@pat/ocean` on
  `75 0 75 0` flipx, a seam glow colour layer — WLED-style segments on a 150 px strip.
- **strip-bulbs** (E3): the SC-C `bulb-string` sprite's 8×1 variant tiled along a strip, two
  frames trading twinkle.
- **sprite-parade** (E4; from bulk-sprite-scroll-2d): `@pat/color-wash` + `@spr/smiley` walking
  across with `M` keys (bob via a third key).
- **bouncing-logo** (E4, ideas #14): `@spr/luxel-logo` (original) bouncing on a diamond key path
  over `@pat/starfield-2d`, tint change per wall via frame keys.
- **rainbow-runner** (E4; the original-cat replacement for nyan-lights): `@pat/starfield-2d` +
  `@pat/rainbow` clipped to a trail box + `@spr/cat-runner` (original ginger cat run cycle)
  moving with `M` keys.
- **sunrise.scene upgrade** (E4): the sun sprite rises and sets on an eased key track over
  `@pat/coral-plasma`.

Ticket-only (BF files them with the feature they need): pulse-to-the-beat [26] (sensor-bound
opacity G11), weather-window [ideas #79] (conditional visibility G7), new-year-countdown
[ideas #78] (countdown text source G8), neon flicker (opacity LFO G9), invaders attract
[ideas #73] (director / drawSprite), hard-vs-soft-key [31] (clone layer G6), any
"time drawn in fire" (clip layers).

## D. Engine features

### D.0 How the shortlist was chosen

Budget: 13,575 B of asset-archive headroom, spent by luxel.wasm (all of luxel-core) and any
console UI. **But** this campaign removes ~65 % of `gallery.json`, which ships in the same
archive (header, "Bundle finding") — so the binding constraint is no longer bytes, it is
review/effort and on-device risk. Ranking criterion therefore = how many §B/§C items each
feature unblocks or makes clean, × how much of the library's 60–300 px strip audience it
serves, ÷ effort. Two structural facts decided it:

- **Index space has no blending and no sub-pixel.** `setPixel`/`fillRange` replace, floor
  their index, and have no mode; the coordinate ops that do blend (`splat`, `drawLine`) hand a
  pure `renderBulk` strip pattern a fake `ceil(√n)` grid. That is why 25 strip patterns hold
  three `array(pixelCount)` buffers and fail 64×64. ~30 strip survivors in §B want additive,
  anti-aliased deposits.
- **`fillCanvas`/`paintCanvas` are nearest-only.** Every soft-field survivor (fire, water,
  noise, CA, RD, physarum, julia, topo, soap, aurora…) is simulated on a ≤ 48² canvas; nearest
  upscale is blocky on 64² and terrible on 128², and the per-pixel `canvasGet` alternative
  costs the full engine floor.

Then the scene system's three gaps (scene-ideas §4 G1/G2, engine-ideas #2/#4): strips render
black, sprites cannot move, text cannot be pattern-filled. The first two have no workaround;
text fill has a documented one (white text on a black band + the pattern above in
`multiply` — SC-A's painted-letters), so clip layers go to a ticket.

### D.1 Build now (in this order) — 4 features in 2 batches

**E1 — index-space anti-aliased, blended ops** (BE1; S; ~0.6 KB wasm; no UI)
```js
splatIndex(pos, radius, mode)                 // soft 1-D blob centred at fractional pixel pos:
                                              // brush × max(0, 1 − |i − pos| / radius); radius in px (≥ 0.5)
fillRangeAA(p0, p1, mode)                     // the brush on [p0, p1) with fractional end coverage
                                              // (p1 < p0 swaps); exact-integer ends == fillRange
gradientRange(p0, p1, h0, s0, v0, h1, s1, v1, mode)
                                              // HSV lerp along [p0, p1) (hue unwrapped like fillGradient),
                                              // AA ends; p1 < p0 runs the gradient backwards
// mode: 0 replace, 1 add (saturating), 2 max — the existing blend arms
```
Index space: naming them must NOT install the default grid (keep them out of
`uses_coordinate_bulk_op`). Append to `BUILTINS` (append-only), JIT helper entries, hover docs
in `web/src/lib/builtins.ts`, docs in `docs/lang.md` "Index space" + `docs/bulk-render.md`
"Three spaces". Tests: byte-exact vs a reference loop on 60/300/512 px; integer ends equal
`fillRange`; no default grid on a mapless strip. Unlocks: scanner, meteor-shower, fireworks
(strip), embers, sparks, fireflies, glittering-jewels, aurora (strip), thunderstorm (strip),
starburst, rocket-launch, light-thief, crosstown (strip), lamp spot, color-wash strips,
custom-sequences chase, copper-bars, day-ribbon, chomp-chase, sparkler, newtons-cradle,
sunrise-alarm — and clears all 25 64×64 budget failures without a buffer.

**E2 — bilinear filter on the canvas fills** (BE1; S; ~0.5 KB wasm; no UI)
```js
fillCanvas(hArr, sArr, vArr, w, h, filter = 0)   // filter 1 = bilinear (hue interpolated the short way)
paintCanvas(vArr, w, h, bArr = 1, filter = 0)    // bArr may be a scalar; filter 1 = bilinear on v (and b)
```
Integer math matching `canvasGet`'s interpolation, pinned byte-exact across hosts; edge clamp;
filter 0 stays byte-identical to today (all existing callers unchanged); a bilinear arm for the
integer-multiple block-expand fast path. Unlocks: every canvas survivor in §B (fire, doom fire,
raindrops, aurora 2D, soap, neon veins, topo map, spectrum field, typing heat, CA, RD,
julia, tunnel texture, color-wash panels) and new caustics, lava-lamp, physarum, attractor,
shadebobs, oil-slick, rotozoomer, moon-phase. It is the thin first slice of engine-ideas #1
`setRenderSize` (the full version stays a ticket, tied to #843).

BE1 also owns the **teaching snippets** that move out of the deleted examples into
`docs/lang.md` (short, one paragraph + ≤ 10 lines of code each): GPIO button debounce
(example-button-w-debounce), accumulate phase for a glitch-free speed slider
(example-smooth-speed-slider), the canvas idiom → now `fillCanvas` (2d-canvas-example),
sync-safe determinism → the existing seeding section (synchronized-random-numbers,
wichmannhill-prng), coordinate rotation (spinning-plasma-2d's transform), accelerometer +
transform (accelerometer-level-example), the post-process chain (post-process-chain),
easing curves (easing-library-v1-0 — link the builtin list). Write them from this
description; the deleted files stay in git history.

BE1 is an engine change → CLAUDE.md "measure it twice on device": `tools/opbench.mjs` +
`tools/patbench.mjs` (and `tools/builtinbench.mjs` for the new ops), devlock first.

**E3 — scenes on strips** (BE2; S–M; ~0.3 KB wasm + a small UI change)
- `outpipe::detect_grid` (and whatever `compose::skip` consults) treats a 1-D layout of n
  pixels — mapless or 1-D-mapped — as an **n×1 grid in index order**. Boxes become segments
  (`x`, `w`; `y = 0`, `h` 0 or 1), `flipx` reverses a segment, `fit tile` repeats a 1-row
  sprite, ramps/masks/blend modes work unchanged. Text layers clip to nothing (no glyph fits
  one row) — documented, not an error. A 2D map is unaffected.
- `w = 0` on a tiled sprite layer means "the whole layout" (so a strip bulb string does not
  need the strip length typed in; also helps G3 on panels).
- Console: show the Scenes tab on strip layouts; the editor's preview canvas is n×1.
- Spec: `docs/spec/scenes.md` + `library/README.md` "Strips" section.
Unlocks: strip-zones, strip-bulbs (SC-D), every ramp/mask teaching scene on a wound or plain
strip, and later the "two-colour fade family as ramps" idea.

**E4 — keyframed layer motion** (BE2; M core S + read-only UI; ~1.0 KB wasm + ~0.3 KB UI)
```
M <ms> <x|=> <y|=> <opacity|=> <ease> [<frame>]
  ms       key time from scene start; the LAST key's ms is the loop period
  x y      layer box origin in layout px (box size unchanged); `=` keeps the L-line value
  opacity  0..100 or `=`
  ease     linear | step | in | out | inout | sine | back | bounce | elastic  (segment ending here,
           reusing the easing builtins' Rust kernels — no duplicate curves)
  frame    optional sprite frame to hold from this key (overrides the sprite clock)
```
≤ 16 keys per layer (bounds the 3,840 B store); integer px positions in v1; evaluated in
`Compositor::advance` into the per-frame layer style; parse → serialize → JSON round-trip
exact; an unknown binding line is already ignored by older hosts, so old firmware degrades to
a static layer. Console v1: a read-only "N keyframes" badge in the layer inspector and keys
preserved through edits (no keyframe editor — ticket). Unlocks: sprite-parade, bouncing-logo,
rainbow-runner, the sunrise sun, and fish that swim (Aquarium v2 later).

Combined estimate ≈ 2.4 KB wasm + ~0.6 KB UI — under 25 % of today's headroom before the
gallery shrink is even counted.

### D.2 Ticket-only (BF files each on Gitea, with the library items that want it)

| feature | why not now | wanted by |
|---|---|---|
| `setRenderSize(w,h,filter)` full (engine-ideas #1, with #843) | M; E2 delivers the soft-upscale half | every 128×128 ambient pattern; 40 canvas idioms |
| Director `setLayer`/`setLayerFrame`/`ctl` layers (#2b) | M + caps accounting; E4 covers authored motion | Invaders, reactive scenes, snake-with-sprites |
| Clip layers `X clip` (#4) | multiply recipe works today | fire-filled clock, rainbow letters, picture-in-sprite |
| Palette system: `palettePicker` control, named palettes, OKLab LUT (#3) | M + UI + preset data | twinkle/fire/color-wash palette modes become one control |
| `setTrail(decay)` afterglow stage (#5) | renderBulk `fade` already serves the rewrites | per-pixel patterns that want trails |
| Symmetry folds `mirror()`/`kaleido(n)` (#7) | kaleidoscope survivors do it in-pattern | kaleidoscope, spectrum-field, any pattern as a scene option |
| `drawSprite(sprite(name), …)` (#8) | M; pinning/name resolution | breakout, frogger, invaders art in `.sprite` files |
| fBm octaves on `fillNoise2D/3D` (#9) | S but not blocking (2 fills + arrayAdd) | fire, topo, neon veins, caustics, clouds |
| `arrayMul`/`arrayAffine`/`arrayLookup` (#373 §2 / #453, #10) | already filed — add the RD/CA/wave evidence | reaction-diffusion at 64², Life rules, plucked string |
| Text scale on `F` + `T num` source (#11) | S+S, UI | clocks/scoreboards legible on 64²/128² |
| Per-layer speed `V <pct>` (#12) and per-pattern projection hint `//# projection=x` (perf §3) | UI + host metadata | strip patterns default to 1-D-along-x on panels (clears 64×64 budget by construction) |

Plus scene-side gaps G7 (conditional visibility), G8 (countdown/weekday text), G9 (opacity
LFO), G11 (sensor-bound opacity), G6 (clone layer), G13 (sprite palette cycling), G14 (text
outline), G15 (scene-store budget), and engine beat/tempo builtins (beatPhase, bpm, kick
triggers — what the deleted music sequencers really wanted), an ordered-dither output stage
(PF dither specs), and a row-displacement glitch post stage (PF oil-slick/glitch-rings).

## E. Work batches

40 batches, one PR each. **Zero file overlap**: every path below belongs to exactly one batch
(the generator that produced the lists checks it; `library/millipede.js` is deleted and
re-created inside L22 only). "edit-if-present" spec files are touched only if the batch
changes what they describe. Anything a batch needs changed outside its list goes in its
report for BF, never into another batch's file.

### E.0 Ordering, waves and shared rules

- **Wave 0:** B00 and BE1 in parallel (disjoint files: B00 edits existing test/bench files,
  BE1 adds `tests/bulk_index_ops.rs` and engine sources). BE2 starts when BE1 merges (both
  touch luxel-core plumbing).
- **Wave 1 (after B00):** batches with no BE1 dependency — L14, L20, L22, L24, L26, L27, L31.
- **Wave 2 (after B00 + BE1):** all other L batches. Disjoint files, so they can run in
  parallel; merge each the moment its gate is green (master moves every few minutes).
- **Wave 3:** SC-A, SC-B, SC-C once the survivors they reference are merged (deps listed).
- **Wave 4:** SC-D after BE2, SC-B (smiley sprite) and SC-C (bulb-string sprite).
- **Wave 5:** BF last.
- Device-touching steps (BE1/BE2 on-device measurement only) are serialized through
  `tools/devlock.sh`; no library batch needs a device.

**B00 fixture policy (why it must land first).** Tests and tools pin specific library files:
`crates/luxel-core/tests/engine.rs` `include_str!`s blink-fade, ripples-2d, slime-mold-palette,
saberdeploy-tutorial, easing-library-v1-0, us-flag-2d, lissajous-curve-tracer;
`prelude.rs` hashes eight files listed in `tests/data/library-callbacks.txt`; `kinds.rs`
asserts `library().len() > 300` and pins a census; luxel-jit `golden.rs`/`engine_diff.rs` name a
subset; `alloc_peak.rs` fits ceilings over the library; `tools/patbench.mjs`, `jit-diff.mjs`,
`render-bench.mjs`, `wasm-bench.mjs`, `opbench.mjs`, `hw-bench.mjs`, `wasm-smoke.mjs`,
`serve-e2e.mjs`, `event-soak.mjs`, `web/tools/e2e.mjs`, `device-e2e.mjs`,
`mockdiff.map.json` and `web/tests/geometry.test.mjs` name patterns (several only as English
words — "traffic", "portal", "snake", "heart", "scanner" — check each hit). Rule: anything that
pins pixels, hashes, callbacks, a census or perf comparability gets a **frozen verbatim copy**
(`crates/luxel-core/tests/data/lib/` for Rust, `tools/bench-patterns/` for tools) and is
repointed; `kinds.rs` pins its census over the frozen set and keeps a live-library smoke test
with no count; `alloc_peak.rs` ceilings are fitted over the frozen set plus a generous
live-library sanity ceiling. Device benches (`patbench` & co.) keep measuring the frozen copies
so numbers stay comparable with the history in docs/boards.md. After B00, no test may break
because a library file was rewritten or deleted.

### E.1 Batch list

### B00 — Prep: decouple tests from the library, commit PF specs, pure deletes, to-tools moves  ·  effort M  ·  depends on: nothing

Executes: A rows owned by B00 (pure deletes + to-tools); §E.0 rules (fixture policy).

- **delete**: `library/2d-canvas-example.js`, `library/2d-wandering-fireball.js`, `library/a-peak-integrator.js`, `library/accelerometer-level-example.js`, `library/amoeba.js`, `library/angry-xmass-3d.js`, `library/animated-asterisks-2d.js`, `library/block-reflections.js`, `library/bubble-column.js`, `library/color-bands.js`, `library/color-bands-buffered.js`, `library/crossfading.js`, `library/cube-fire-3d.js`, `library/distance-function-kaleidoscope-2.js`, `library/easing-library-v1-0.js`, `library/example-button-w-debounce.js`, `library/example-color-hues.js`, `library/example-modes-and-waveforms.js`, `library/example-smooth-speed-slider.js`, `library/example-time-and-animation.js`, `library/fast-palette-blending.js`, `library/flash-posterize-music-sequencer-framework.js`, `library/glitch-bands.js`, `library/lightbulb-crank-hue-to-complete.js`, `library/m5stack-hex-panels.js`, `library/matrix-2d-pulse-edit.js`, `library/midpointdisplacement1d.js`, `library/multimap-simpledemo.js`, `library/multisegment-demo.js`, `library/music-sequencer-for-v2.js`, `library/music-sequencer-for-v3-only.js`, `library/nano-orbital.js`, `library/nyan-lights.js`, `library/perlin-fire-wind-tunnel.js`, `library/post-process-chain.js`, `library/rock-sparks.js`, `library/shimmer-crossfade-2d.js`, `library/slow-color-shift.js`, `library/sound-reactive-color-fade.js`, `library/spin-cycle.js`, `library/spinwheel-2d.js`, `library/stairmaster-2d.js`, `library/stargen-polar-2d.js`, `library/static-random-colors.js`, `library/synchronized-random-numbers.js`, `library/three-red-pixels-array.js`, `library/traffic.js`, `library/utility-palettes.js`, `library/wanderers.js`, `library/wichmannhill-prng.js`
- **delete-if-present**: `docs/pattern-specs/2d-canvas-example.md`, `docs/pattern-specs/2d-wandering-fireball.md`, `docs/pattern-specs/a-peak-integrator.md`, `docs/pattern-specs/accelerometer-level-example.md`, `docs/pattern-specs/amoeba.md`, `docs/pattern-specs/angry-xmass-3d.md`, `docs/pattern-specs/animated-asterisks-2d.md`, `docs/pattern-specs/block-reflections.md`, `docs/pattern-specs/bubble-column.md`, `docs/pattern-specs/color-bands.md`, `docs/pattern-specs/color-bands-buffered.md`, `docs/pattern-specs/crossfading.md`, `docs/pattern-specs/cube-fire-3d.md`, `docs/pattern-specs/distance-function-kaleidoscope-2.md`, `docs/pattern-specs/easing-library-v1-0.md`, `docs/pattern-specs/example-button-w-debounce.md`, `docs/pattern-specs/example-color-hues.md`, `docs/pattern-specs/example-modes-and-waveforms.md`, `docs/pattern-specs/example-smooth-speed-slider.md`, `docs/pattern-specs/example-time-and-animation.md`, `docs/pattern-specs/fast-palette-blending.md`, `docs/pattern-specs/flash-posterize-music-sequencer-framework.md`, `docs/pattern-specs/frame-rate-scan.md`, `docs/pattern-specs/frame-rate-scan-fast.md`, `docs/pattern-specs/frame-rate-test.md`, `docs/pattern-specs/glitch-bands.md`, `docs/pattern-specs/lightbulb-crank-hue-to-complete.md`, `docs/pattern-specs/m5stack-hex-panels.md`, `docs/pattern-specs/mapping-helper-single-and-10x.md`, `docs/pattern-specs/matrix-2d-pulse-edit.md`, `docs/pattern-specs/midpointdisplacement1d.md`, `docs/pattern-specs/multimap-simpledemo.md`, `docs/pattern-specs/multisegment-demo.md`, `docs/pattern-specs/music-sequencer-for-v2.md`, `docs/pattern-specs/music-sequencer-for-v3-only.md`, `docs/pattern-specs/nano-orbital.md`, `docs/pattern-specs/nyan-lights.md`, `docs/pattern-specs/performance-test-framework.md`, `docs/pattern-specs/perlin-fire-wind-tunnel.md`, `docs/pattern-specs/polar-mapping-helper-2d-3d.md`, `docs/pattern-specs/post-process-chain.md`, `docs/pattern-specs/rgb-xyz-3d-sweep.md`, `docs/pattern-specs/rgbw-mapping-tester.md`, `docs/pattern-specs/rgbw-mapping-tester-hsv-version.md`, `docs/pattern-specs/rock-sparks.md`, `docs/pattern-specs/shimmer-crossfade-2d.md`, `docs/pattern-specs/slow-color-shift.md`, `docs/pattern-specs/sound-reactive-color-fade.md`, `docs/pattern-specs/spin-cycle.md`, `docs/pattern-specs/spinwheel-2d.md`, `docs/pattern-specs/stairmaster-2d.md`, `docs/pattern-specs/stargen-polar-2d.md`, `docs/pattern-specs/static-random-colors.md`, `docs/pattern-specs/synchronized-random-numbers.md`, `docs/pattern-specs/three-red-pixels-array.md`, `docs/pattern-specs/traffic.md`, `docs/pattern-specs/tree-setup-pattern.md`, `docs/pattern-specs/utility-palettes.md`, `docs/pattern-specs/wanderers.md`, `docs/pattern-specs/wichmannhill-prng.md`
- **move/drop**: `library/frame-rate-scan.js`, `library/frame-rate-scan-fast.js`, `library/frame-rate-test.js`, `library/mapping-helper-single-and-10x.js`, `library/performance-test-framework.js`, `library/polar-mapping-helper-2d-3d.js`, `library/rgb-xyz-3d-sweep.js`, `library/rgbw-mapping-tester.js`, `library/rgbw-mapping-tester-hsv-version.js`, `library/tree-setup-pattern.js`
- **create**: tools/patterns/ (frame-rate-scan, frame-rate-scan-fast, frame-rate-test, rgbw-mapping-tester, rgb-xyz-3d-sweep, mapping-helper-single-and-10x, polar-mapping-helper-2d-3d, tree-setup-pattern .js — git mv from library/), crates/luxel-core/tests/data/lib/*.js (frozen copies of every library file a Rust test pins), tools/bench-patterns/*.js (frozen copies of the patterns the bench/diff tools pin: perlin-fire, perlin-fire-wind, perlin-fire-wind-tunnel, snake-2d, rainbow, ripples-2d, bulk-canvas-ripples-2d, 2d-fireworks-fade, novas, rainbow-smiley, fairies, …), docs/pattern-specs/patternflow/*.md (commit the 34 untracked PF idea specs as-is), tools/verify/sheet.mjs (commit the untracked contact-sheet tool as-is)
- **edit**: `crates/luxel-core/tests/engine.rs`, `crates/luxel-core/tests/prelude.rs`, `crates/luxel-core/tests/data/library-callbacks.txt`, `crates/luxel-core/tests/kinds.rs`, crates/luxel-core/tests/semantics.rs (only if it include_str!s a library file), crates/luxel-core/tests/projection.rs (only if it pins fairies output), `crates/luxel-jit/tests/golden.rs`, `crates/luxel-jit/tests/engine_diff.rs`, `crates/luxel-jit/tests/alloc_peak.rs`, `crates/luxel-jit/tests/common/mod.rs`, `tools/patbench.mjs`, `tools/jit-diff.mjs`, `tools/opbench.mjs`, `tools/render-bench.mjs`, `tools/hw-bench.mjs`, `tools/wasm-smoke.mjs`, `tools/serve-e2e.mjs`, `tools/event-soak.mjs`, `web/tools/wasm-bench.mjs`, `web/tools/e2e.mjs`, `web/tools/device-e2e.mjs`, `web/tools/mockdiff.map.json`, `web/tests/geometry.test.mjs`, tools/check-library.sh (only if it names a moved/deleted file), docs/boards.md (frame-rate procedure path -> tools/patterns/), docs/tools.md (index tools/patterns/ and tools/bench-patterns/), .claude/rules/library.md (band() idiom reference -> tools/patterns/frame-rate-test.js)

### BE1 — Engine: index-space AA/blend ops (E1) + bilinear canvas fills (E2)  ·  effort M  ·  depends on: nothing

Executes: §D E1; §D E2.

- **edit**: `crates/luxel-core/src/bulk.rs`, crates/luxel-core/src/vm.rs (BUILTINS append-only), crates/luxel-core/src/** builtin plumbing (compile/arity/kinds/caps tables) EXCEPT compose.rs, scene.rs, outpipe.rs (BE2) — engine.rs only if the coordinate-op classifier must learn the new ops are index-space, crates/luxel-jit/src/** (helper entries for the new ops), web/src/lib/builtins.ts (hover docs), docs/lang.md (new ops + the teaching snippets from deleted examples, §D.E1 list), `docs/bulk-render.md`
- **create**: `crates/luxel-core/tests/bulk_index_ops.rs`

### BE2 — Engine: scenes on strips (E3) + keyframed layer motion (E4)  ·  effort L  ·  depends on: BE1

Executes: §D E3; §D E4.

- **edit**: `crates/luxel-core/src/compose.rs`, `crates/luxel-core/src/scene.rs`, crates/luxel-core/src/outpipe.rs (detect_grid), `firmware/src/scenes.rs`, web/src/** scene editor + Scenes-tab visibility (the pages/components that hide Scenes on non-matrix layouts; the layer inspector keyframe badge) — never web/src/lib/builtins.ts (BE1), `docs/spec/scenes.md`, library/README.md (scene grammar: M lines, strip boxes), web/tools/check-sprite-scene-library.mjs (only if the gate must learn M lines)
- **create**: `crates/luxel-core/tests/scene_strip.rs`, `crates/luxel-core/tests/scene_keys.rs`

### L01 — Rainbows & washes  ·  effort M  ·  depends on: B00, BE1

Executes: §B L01; §C copper-bars.

- **delete**: `library/1-white-fade.js`, `library/2-purple-fade.js`, `library/breathing-gradient.js`, `library/bulk-rainbow.js`, `library/chasing-rainbows-hsluv.js`, `library/chevron-2d.js`, `library/color-blend.js`, `library/complements-3d.js`, `library/gradient-blue-purple-pink.js`, `library/marching-rainbow.js`, `library/marching-rainbow-buffered.js`, `library/rainbow-fonts.js`, `library/rainbow-pinwheel.js`, `library/rainbow-v2.js`, `library/regenbogendrogen.js`, `library/ryb-colors.js`, `library/twocolorhsvmix.js`
- **create**: library/color-wash.js (from breathing-gradient), library/copper-bars.js (new)
- **edit-if-present**: `docs/pattern-specs/breathing-gradient.md`, `docs/pattern-specs/opposites.md`, `docs/pattern-specs/rainbow.md`, `docs/pattern-specs/rainbow-melt.md`
- **verify only**: `library/color-twinkle-bounce.js`
- **edit**: `library/opposites.js`, `library/rainbow.js`, `library/rainbow-melt.js`

### L02 — Lamp, props & signals  ·  effort M  ·  depends on: B00, BE1

Executes: §B L02; §C notification-ripple.

- **delete**: `library/crosshair-pulse-2d.js`, `library/naturallightsync.js`, `library/real-world-lights.js`, `library/saberdeploy-tutorial.js`, `library/single-color-picker-wide-or-spot.js`, `library/utilitycolortemp.js`
- **edit**: `library/policelights.js`, `library/tv-simulator.js`, `library/typing-heatmap-2d.js`
- **edit-if-present**: `docs/pattern-specs/policelights.md`, `docs/pattern-specs/saberdeploy-tutorial.md`, `docs/pattern-specs/tv-simulator.md`, `docs/pattern-specs/typing-heatmap-2d.md`, `docs/pattern-specs/utilitycolortemp.md`
- **create**: library/saber.js (from saberdeploy-tutorial), library/lamp.js (from utilitycolortemp), library/notification-ripple.js (new)

### L03 — Twinkle & sparkle  ·  effort M  ·  depends on: B00, BE1

Executes: §B L03.

- **delete**: `library/autumn-colors.js`, `library/blink-fade.js`, `library/chill-confetti.js`, `library/christmas-rg-fade.js`, `library/color-twinkles.js`, `library/firework-dust.js`, `library/glitter.js`, `library/halloween-color-twinkles.js`, `library/quiet-blinkfade.js`, `library/sound-blinkfade.js`, `library/spring-colors.js`, `library/twinkle-2.js`, `library/twinkly-stars.js`
- **edit**: `library/fairies.js`, `library/glittering-jewels.js`, `library/twinkle.js`, `library/wanderedges.js`
- **edit-if-present**: `docs/pattern-specs/fairies.md`, `docs/pattern-specs/glittering-jewels.md`, `docs/pattern-specs/twinkle.md`, `docs/pattern-specs/wanderedges.md`

### L04 — Holiday strings & marquee  ·  effort M  ·  depends on: B00, BE1

Executes: §B L04.

- **delete**: `library/2-colors.js`, `library/3-color-rotation.js`, `library/christmas-lights.js`, `library/christmaslights.js`, `library/christmasstretch.js`, `library/holiday-diagonal-stripes.js`, `library/static-christmas-lights-4-colors.js`, `library/three-red-pixels-mathy.js`
- **edit**: `library/christmas-candy-cane.js`, `library/custom-sequences.js`, `library/marquee-chase.js`, `library/twinkling-classic-xmas-strands.js`
- **edit-if-present**: `docs/pattern-specs/christmas-candy-cane.md`, `docs/pattern-specs/custom-sequences.md`, `docs/pattern-specs/marquee-chase.md`, `docs/pattern-specs/twinkling-classic-xmas-strands.md`

### L05 — Scanner, meteors & pulses  ·  effort M  ·  depends on: B00, BE1

Executes: §B L05; §C newtons-cradle.

- **delete**: `library/beat-bounce.js`, `library/bulk-comet-trails.js`, `library/christmaspewpew.js`, `library/color-fade-pulse.js`, `library/comets.js`, `library/cylon.js`, `library/fast-pulse.js`, `library/kitt.js`, `library/kitt-w-color-picker.js`, `library/pew-pew-pew.js`, `library/rainbow-comet.js`, `library/snake.js`
- **edit**: `library/color-pick-fade.js`, `library/edgeburst.js`, `library/fast-pulse-3d.js`, `library/meteor-shower.js`, `library/scanner.js`
- **edit-if-present**: `docs/pattern-specs/color-pick-fade.md`, `docs/pattern-specs/edgeburst.md`, `docs/pattern-specs/fast-pulse-3d.md`, `docs/pattern-specs/meteor-shower.md`, `docs/pattern-specs/scanner.md`
- **create**: library/newtons-cradle.js (new)

### L06 — Fireworks, rockets & sparks  ·  effort M  ·  depends on: B00, BE1

Executes: §B L06; §C sparkler.

- **delete**: `library/2d-fireworks-fade.js`, `library/4th.js`, `library/firework-nova.js`, `library/firework-rocket-sparks.js`, `library/fireworks-finale.js`, `library/rainbow-rocket-sparks.js`, `library/rocket-by-tony-hampton.js`, `library/sparks-center.js`
- **create**: library/fireworks.js (from 2d-fireworks-fade), library/rocket-launch.js (from rocket-by-tony-hampton), library/sparkler.js (new)
- **edit-if-present**: `docs/pattern-specs/2d-fireworks-fade.md`, `docs/pattern-specs/rocket-by-tony-hampton.md`, `docs/pattern-specs/sparks.md`
- **edit**: `library/sparks.js`

### L07 — Fire (2D + strip), Doom Fire, Embers  ·  effort L  ·  depends on: B00, BE1

Executes: §B L07.

- **edit**: `library/doom-fire.js`, `library/heatshivers.js`, `library/perlin-fire.js`
- **edit-if-present**: `docs/pattern-specs/doom-fire.md`, `docs/pattern-specs/heatshivers.md`, `docs/pattern-specs/perlin-fire.md`
- **delete**: `library/doom-fire-2d.js`, `library/ember-diffusion.js`, `library/fire-blue.js`, `library/fire-red.js`, `library/fireblobs.js`, `library/newfire.js`, `library/palette-fire-2d.js`, `library/perlin-fire-wind.js`, `library/sparkfire.js`

### L08 — Aurora & Ocean  ·  effort L  ·  depends on: B00, BE1

Executes: §B L08.

- **delete**: `library/1d-aurora-borealis.js`, `library/aurorashivers.js`, `library/coolaura.js`, `library/green-ripple-reflections.js`, `library/oasis.js`
- **edit**: `library/aurora-2d.js`, `library/ocean.js`
- **edit-if-present**: `docs/pattern-specs/aurora-2d.md`, `docs/pattern-specs/ocean.md`

### L09 — Water & sensor liquid  ·  effort M  ·  depends on: B00, BE1

Executes: §B L09; §C caustics, rain-on-glass.

- **delete**: `library/bulk-canvas-ripples-2d.js`, `library/drip.js`, `library/glowflow-3d-coord-transform-api-port.js`, `library/ripples-2d.js`, `library/upward-waves-3d-using-accelerometer.js`
- **create**: library/glowflow.js (from glowflow-3d-coord-transform-api-port), library/caustics.js (new), library/rain-on-glass.js (new)
- **edit-if-present**: `docs/pattern-specs/glowflow-3d-coord-transform-api-port.md`, `docs/pattern-specs/raindrops-2d.md`
- **edit**: `library/raindrops-2d.js`

### L10 — Balls, flocks & pendulums  ·  effort M  ·  depends on: B00, BE1

Executes: §B L10; §C double-pendulum.

- **delete**: `library/2d-bouncing-additive-primaries.js`, `library/bouncer3d.js`, `library/bouncing-balls-2d.js`, `library/bouncing-balls-hsv.js`, `library/bouncing-balls-rgb.js`, `library/bouncing-balls-rgb-2d.js`, `library/bouncing-rgb-balls-2d.js`, `library/bulk-bouncing-balls-2d.js`, `library/time-flies-2d.js`
- **edit**: `library/boids-2d.js`
- **edit-if-present**: `docs/pattern-specs/boids-2d.md`, `docs/pattern-specs/bouncing-balls-2d.md`, `docs/pattern-specs/bulk-bouncing-balls-2d.md`
- **create**: library/bouncing-columns.js (from bouncing-balls-2d), library/bouncing-balls.js (from bulk-bouncing-balls-2d), library/double-pendulum.js (new)

### L11 — Interference, plasma & waves  ·  effort M  ·  depends on: B00, BE1

Executes: §B L11; §C quasicrystal.

- **delete**: `library/2d-sinc-theta-theta.js`, `library/halloween-wavy-bands.js`, `library/interference-2d.js`, `library/matrix-2-tone-pulse.js`, `library/matrix-2d-honeycomb.js`, `library/sinpulse-3d.js`, `library/sinus.js`, `library/spinning-plasma-2d.js`, `library/sunset.js`
- **create**: library/interference.js (from 2d-sinc-theta-theta), library/plasma.js (from sinpulse-3d), library/quasicrystal.js (new)
- **edit-if-present**: `docs/pattern-specs/2d-sinc-theta-theta.md`, `docs/pattern-specs/dna-helix-2d.md`, `docs/pattern-specs/sinpulse-3d.md`, `docs/pattern-specs/soap-2d.md`, `docs/pattern-specs/wavy-bands.md`
- **edit**: `library/dna-helix-2d.js`, `library/soap-2d.js`, `library/wavy-bands.js`

### L12 — Noise fields  ·  effort M  ·  depends on: B00, BE1

Executes: §B L12; §C marbling, chladni.

- **edit**: `library/bessel-chaos.js`, `library/coral-plasma.js`
- **edit-if-present**: `docs/pattern-specs/bessel-chaos.md`, `docs/pattern-specs/coral-plasma.md`, `docs/pattern-specs/perlin-simplex-noise-2d.md`
- **delete**: `library/perlin-simplex-noise-1d.js`, `library/perlin-simplex-noise-2d.js`
- **create**: library/topo-map.js (from perlin-simplex-noise-2d), library/marbling.js (new), library/chladni.js (new)

### L13 — Kaleidoscopes, spirals & tiles  ·  effort M  ·  depends on: B00, BE1

Executes: §B L13; §C truchet, loop-ring, shadebobs.

- **edit**: `library/2d-spiral-twirls.js`, `library/kaleidoscope-2d.js`, `library/tixy.js`
- **edit-if-present**: `docs/pattern-specs/2d-spiral-twirls.md`, `docs/pattern-specs/kaleidoscope-2d.md`, `docs/pattern-specs/tixy.md`
- **delete**: `library/golden-tix.js`, `library/spiral-2d.js`, `library/spiral-twirls-star-2d.js`, `library/zoom-kaleidoscope.js`
- **create**: library/truchet.js (new), library/loop-ring.js (new), library/shadebobs.js (new)

### L14 — Curves, beams & radar  ·  effort M  ·  depends on: B00

Executes: §B L14; §C mystify.

- **delete**: `library/3d-rotation-spotlights.js`, `library/all-lasers-fire.js`, `library/angle-and-radius-from-coordinates.js`, `library/lissajous-curve-tracer.js`, `library/spotlights-rotation-3d.js`, `library/swirlpool-2d.js`, `library/unstable-orbits-2d.js`
- **create**: library/searchlights.js (from all-lasers-fire), library/mystify.js (new)
- **edit-if-present**: `docs/pattern-specs/all-lasers-fire.md`, `docs/pattern-specs/radar-2d.md`, `docs/pattern-specs/spirograph-2d.md`
- **edit**: `library/radar-2d.js`, `library/spirograph-2d.js`

### L15 — Flow & agents  ·  effort M  ·  depends on: B00, BE1

Executes: §B L15; §C gravity-wells, physarum, attractor.

- **edit**: `library/curl-flow-2d.js`
- **edit-if-present**: `docs/pattern-specs/curl-flow-2d.md`
- **delete**: `library/flow-field-2d.js`
- **create**: library/gravity-wells.js (new), library/physarum.js (new), library/attractor.js (new)

### L16 — Night meadow & fireflies  ·  effort M  ·  depends on: B00, BE1

Executes: §B L16; §C meadow, night-city, bokeh.

- **delete**: `library/colourful-fireflies.js`, `library/slowflies.js`, `library/xmasflies.js`
- **edit**: `library/fireflies.js`
- **edit-if-present**: `docs/pattern-specs/fireflies.md`
- **create**: library/meadow.js (new), library/night-city.js (new), library/bokeh.js (new)

### L17 — Sound  ·  effort L  ·  depends on: B00, BE1

Executes: §B L17.

- **delete**: `library/audio-volume-meter.js`, `library/light-organ-sensor-board.js`, `library/sound-music-spectrum-visualizer.js`, `library/sound-rays-frequency-bpm-reactive-1.js`, `library/sound-spectro-kalidastrip.js`, `library/sound-spectroblots-pow-fade.js`, `library/sound-spectrokalidamandala.js`, `library/sound-spectromatrix-agc.js`, `library/sound-spectromatrix-render2d.js`, `library/sound-starburst-2.js`, `library/white-rainbows.js`
- **edit**: `library/light-organ-2-0.js`, `library/sound-rays.js`, `library/sound-spectrum-analyser.js`
- **edit-if-present**: `docs/pattern-specs/light-organ-2-0.md`, `docs/pattern-specs/sound-rays.md`, `docs/pattern-specs/sound-spectrokalidamandala.md`, `docs/pattern-specs/sound-spectrum-analyser.md`, `docs/pattern-specs/sound-starburst-2.md`
- **create**: library/spectrum-field.js (from sound-spectrokalidamandala), library/starburst.js (from sound-starburst-2)

### L18 — Sky & weather  ·  effort M  ·  depends on: B00, BE1

Executes: §B L18; §C moon-phase, snowfall.

- **delete**: `library/b-lightning-flashes.js`, `library/lightning-clouds.js`, `library/lightning-strike.js`, `library/rainstorm.js`
- **edit**: `library/starfield-2d.js`, `library/thunderstorm.js`
- **edit-if-present**: `docs/pattern-specs/starfield-2d.md`, `docs/pattern-specs/thunderstorm.md`
- **create**: library/moon-phase.js (new), library/snowfall.js (new)

### L19 — Clocks  ·  effort M  ·  depends on: B00, BE1

Executes: §B L19; §C word-clock, fibonacci-clock, day-ribbon, pong-clock.

- **delete**: `library/2d-clock-with-hand-color-pickers.js`, `library/pixelclock.js`, `library/rgbclock-2d.js`, `library/sunrise.js`
- **create**: library/analog-clock.js (from rgbclock-2d), library/word-clock.js (new), library/fibonacci-clock.js (new), library/day-ribbon.js (new), library/pong-clock.js (new)
- **edit-if-present**: `docs/pattern-specs/rgbclock-2d.md`, `docs/pattern-specs/sunrise-alarm-clock.md`
- **edit**: `library/sunrise-alarm-clock.js`

### L20 — Sun, eye & cosmos  ·  effort L  ·  depends on: B00

Executes: §B L20; §C galaxy, dot-globe.

- **delete**: `library/coronal-ejection-2d.js`, `library/eye-of-sauron.js`, `library/eye-of-sauron-with-movement.js`, `library/sunrise-2d.js`
- **edit**: `library/coronal-mass-ejection.js`, `library/sun-rays-through-trees.js`
- **edit-if-present**: `docs/pattern-specs/coronal-mass-ejection.md`, `docs/pattern-specs/eye-of-sauron.md`, `docs/pattern-specs/sun-rays-through-trees.md`
- **create**: library/fire-eye.js (from eye-of-sauron), library/galaxy.js (new), library/dot-globe.js (new)

### L21 — Holiday figures & candle  ·  effort M  ·  depends on: B00, BE1

Executes: §B L21; §C candle.

- **delete**: `library/blue-holiday-candle-2d.js`, `library/blue-holiday-star-2d.js`, `library/carrie-s-holiday-star-2d.js`, `library/rainbow-flag.js`, `library/the-grinch.js`, `library/us-flag.js`
- **create**: library/holiday-star.js (from carrie-s-holiday-star-2d), library/light-thief.js (from the-grinch), library/candle.js (new)
- **edit-if-present**: `docs/pattern-specs/carrie-s-holiday-star-2d.md`, `docs/pattern-specs/heart.md`, `docs/pattern-specs/the-grinch.md`, `docs/pattern-specs/us-flag-2d.md`
- **edit**: `library/heart.js`, `library/us-flag-2d.js`

### L22 — Creatures  ·  effort M  ·  depends on: B00

Executes: §B L22; §C jellyfish.

- **edit**: `library/blinky-eyes-2d.js`, `library/butterfly-2d.js`, `library/dire-spider-2d.js`
- **edit-if-present**: `docs/pattern-specs/blinky-eyes-2d.md`, `docs/pattern-specs/butterfly-2d.md`, `docs/pattern-specs/dire-spider-2d.md`, `docs/pattern-specs/millipede-1d-2d-controls.md`
- **delete**: `library/crawling-spider-2d.js`, `library/millipede.js`, `library/millipede-1d-2d-controls.js`
- **delete-if-present**: `docs/pattern-specs/millipede.md`
- **create**: library/millipede.js (from millipede-1d-2d-controls), library/jellyfish.js (new)

### L23 — Arcade I  ·  effort M  ·  depends on: B00, BE1

Executes: §B L23; §C asteroids, chomp-chase, sorting.

- **edit**: `library/breakout-2d.js`, `library/snake-2d.js`
- **edit-if-present**: `docs/pattern-specs/breakout-2d.md`, `docs/pattern-specs/snake-2d.md`
- **delete**: `library/snake-2d-v2.js`
- **create**: library/asteroids.js (new), library/chomp-chase.js (new), library/sorting.js (new)

### L24 — Arcade II & block clock  ·  effort M  ·  depends on: B00

Executes: §B L24; §C block-clock.

- **delete**: `library/dbzbattlefinal.js`
- **create**: library/energy-duel.js (from dbzbattlefinal), library/block-clock.js (new)
- **edit-if-present**: `docs/pattern-specs/dbzbattlefinal.md`, `docs/pattern-specs/frogger-2d.md`, `docs/pattern-specs/stacker.md`, `docs/pattern-specs/tetrix-2d.md`
- **edit**: `library/frogger-2d.js`, `library/stacker.js`, `library/tetrix-2d.js`

### L25 — Automata & bit fields  ·  effort L  ·  depends on: B00, BE1

Executes: §B L25; §C life, turmites.

- **delete**: `library/cellular-automata-1d.js`, `library/continuous-cellular-automata.js`, `library/cyclic-cellular-automata-2d.js`, `library/sierpinski-rainbow-2d.js`
- **create**: library/cellular-automata.js (from continuous-cellular-automata), library/life.js (new), library/turmites.js (new)
- **edit-if-present**: `docs/pattern-specs/continuous-cellular-automata.md`, `docs/pattern-specs/reaction-diffusion-2d.md`, `docs/pattern-specs/xorcery-2d-3d.md`
- **edit**: `library/reaction-diffusion-2d.js`, `library/xorcery-2d-3d.js`

### L26 — Growth & packing  ·  effort M  ·  depends on: B00

Executes: §B L26; §C maze, circle-packing.

- **edit**: `library/falling-sand-2d.js`, `library/slime-mold-palette.js`
- **edit-if-present**: `docs/pattern-specs/falling-sand-2d.md`, `docs/pattern-specs/slime-mold-palette.md`
- **create**: library/maze.js (new), library/circle-packing.js (new)

### L27 — Graphic shapes  ·  effort M  ·  depends on: B00

Executes: §B L27; §C gears, halftone, hud-ticker.

- **edit**: `library/80s-kid-show.js`, `library/bouncy-boxes.js`, `library/geometry-morphing-demo-2d.js`
- **edit-if-present**: `docs/pattern-specs/80s-kid-show.md`, `docs/pattern-specs/bouncy-boxes.md`, `docs/pattern-specs/geometry-morphing-demo-2d.md`
- **create**: library/gears.js (new), library/halftone.js (new), library/hud-ticker.js (new)

### L28 — Tunnels & demo effects  ·  effort M  ·  depends on: B00, BE1

Executes: §B L28; §C rotozoomer.

- **edit**: `library/fractal-flower.js`, `library/line-dancer-2d.js`, `library/radiant-pulse-3.js`, `library/tunnel-of-squares-2d.js`
- **edit-if-present**: `docs/pattern-specs/fractal-flower.md`, `docs/pattern-specs/line-dancer-2d.md`, `docs/pattern-specs/radiant-pulse-3.md`, `docs/pattern-specs/tunnel-of-squares-2d.md`
- **create**: library/rotozoomer.js (new)

### L29 — Fractals, cells & physics  ·  effort L  ·  depends on: B00, BE1

Executes: §B L29; §C plucked-string, orbits.

- **delete**: `library/ice-floes-2d.js`, `library/infinity-flower-2d.js`, `library/neutronorbit.js`
- **create**: library/lotus-bloom.js (from infinity-flower-2d), library/plucked-string.js (new), library/orbits.js (new)
- **edit-if-present**: `docs/pattern-specs/infinity-flower-2d.md`, `docs/pattern-specs/mandelbrot-2d.md`, `docs/pattern-specs/pendulum-wave.md`, `docs/pattern-specs/voronoi-2d.md`
- **edit**: `library/mandelbrot-2d.js`, `library/pendulum-wave.js`, `library/voronoi-2d.js`

### L30 — Traffic, portal & lava  ·  effort M  ·  depends on: B00, BE1

Executes: §B L30; §C lava-lamp.

- **delete**: `library/bustle.js`, `library/metaballs-of-fire-2d.js`, `library/novas.js`, `library/scrolls.js`, `library/sound-lavablob.js`
- **edit**: `library/crosstown-traffic-2d.js`, `library/portal.js`
- **edit-if-present**: `docs/pattern-specs/crosstown-traffic-2d.md`, `docs/pattern-specs/portal.md`
- **create**: library/lava-lamp.js (new)

### L31 — Retro vector & seasons tree  ·  effort M  ·  depends on: B00

Executes: §C wireframe, pipes, twister, seasons-tree.

- **create**: library/wireframe.js (new), library/pipes.js (new), library/twister.js (new), library/seasons-tree.js (new)

### L32 — Digital rain & glitch  ·  effort M  ·  depends on: B00, BE1

Executes: §B L32; §C bit-rain, oil-slick, moire-gallery.

- **edit**: `library/digital-rain-2d.js`
- **edit-if-present**: `docs/pattern-specs/digital-rain-2d.md`
- **delete**: `library/matrix-green-waterfall-1d.js`, `library/matrix-green-waterfall-2d.js`, `library/matrix-rain.js`
- **create**: library/bit-rain.js (new), library/oil-slick.js (new), library/moire-gallery.js (new)

### SC-A — Scenes: info, clocks, text & sound  ·  effort M  ·  depends on: L01, L08, L11, L13, L15, L17, L18, L19

Executes: §C.2 SC-A.

- **delete**: `library/scrolling-text-marquee-2d.js`
- **delete-if-present**: `docs/pattern-specs/scrolling-text-marquee-2d.md`
- **create**: `library/scenes/night-clock.scene`, `library/scenes/date-time.scene`, `library/scenes/message-board.scene`, `library/scenes/weather-card.scene`, `library/scenes/analog-date.scene`, `library/scenes/neon-sign.scene`, `library/scenes/light-boxes.scene`, `library/scenes/vignette-tint.scene`, `library/scenes/scrolling-message.scene`, `library/scenes/spectrum-over-aurora.scene`, `library/scenes/now-playing.scene`, `library/scenes/painted-letters.scene`, `library/sprites/envelope.sprite`, `library/sprites/cloud-sun.sprite`, `library/sprites/rain-tile.sprite`, `library/sprites/vignette.sprite`, `library/sprites/music-note.sprite`

### SC-B — Scenes: ambient, nature & characters  ·  effort L  ·  depends on: L01, L03, L05, L07, L09, L11, L14, L16, L18, L29

Executes: §C.2 SC-B.

- **delete**: `library/emoji-animation-2.js`, `library/rainbow-smiley.js`
- **delete-if-present**: `docs/pattern-specs/emoji-animation-2.md`, `docs/pattern-specs/rainbow-smiley.md`
- **create**: `library/scenes/tidepool.scene`, `library/scenes/rainbow-ripples.scene`, `library/scenes/stained-glass-fire.scene`, `library/scenes/lighthouse.scene`, `library/scenes/fireflies-at-dusk.scene`, `library/scenes/aquarium.scene`, `library/scenes/campfire.scene`, `library/scenes/space-cadet.scene`, `library/scenes/nocturne.scene`, `library/scenes/hearth.scene`, `library/scenes/witchs-cauldron.scene`, `library/scenes/googly-face.scene`, `library/scenes/rainbow-smiley.scene`, `library/sprites/harbor.sprite`, `library/sprites/seaweed.sprite`, `library/sprites/fish.sprite`, `library/sprites/bubbles-tile.sprite`, `library/sprites/moon.sprite`, `library/sprites/campfire.sprite`, `library/sprites/planet.sprite`, `library/sprites/rocket.sprite`, `library/sprites/pixel-alien.sprite`, `library/sprites/mountains.sprite`, `library/sprites/log-pile.sprite`, `library/sprites/cauldron.sprite`, `library/sprites/face-moods.sprite`, `library/sprites/smiley.sprite`

### SC-C — Scenes: holidays & parties  ·  effort M  ·  depends on: L03, L06, L07, L18, L21

Executes: §C.2 SC-C.

- **delete**: `library/orv-christmas-tree.js`, `library/scary-pumpkin.js`
- **delete-if-present**: `docs/pattern-specs/orv-christmas-tree.md`, `docs/pattern-specs/scary-pumpkin.md`
- **create**: `library/scenes/jack-o-lantern.scene`, `library/scenes/silent-night.scene`, `library/scenes/new-years-eve.scene`, `library/scenes/fourth-of-july.scene`, `library/scenes/candle-row.scene`, `library/scenes/happy-birthday.scene`, `library/scenes/bulb-string.scene`, `library/sprites/jack-o-lantern.sprite`, `library/sprites/bat.sprite`, `library/sprites/christmas-tree.sprite`, `library/sprites/snowfall-tile.sprite`, `library/sprites/firework-burst.sprite`, `library/sprites/candle-stick.sprite`, `library/sprites/cake.sprite`, `library/sprites/bulb-string.sprite`

### SC-D — Scenes that need E3/E4  ·  effort M  ·  depends on: BE2, L01, L07, L08, L12, L18, SC-B, SC-C

Executes: §C.3.

- **delete**: `library/bulk-sprite-scroll-2d.js`
- **delete-if-present**: `docs/pattern-specs/bulk-sprite-scroll-2d.md`
- **create**: `library/scenes/strip-zones.scene`, `library/scenes/strip-bulbs.scene`, `library/scenes/sprite-parade.scene`, `library/scenes/bouncing-logo.scene`, `library/scenes/rainbow-runner.scene`, `library/sprites/cat-runner.sprite`, `library/sprites/luxel-logo.sprite`
- **edit**: library/scenes/sunrise.scene (sun keyframes)

### BF — Final: registries, worklog, tickets, full sweep  ·  effort S  ·  depends on: all

Executes: §E.2 BF checklist.

- **edit**: tools/verify/pairs.json (regenerate with gen-pairs.mjs), tools/verify/decisions.json, tools/verify/fixups.json, tools/verify/results/* (prune removed slugs), UPDATES.md (one dated campaign entry), README.md, docs/PLAN.md (library counts/description), docs/pattern-ideas.md, docs/ideas.md (mark shipped ideas), tools/verify/gen-pairs.mjs (only if it hard-codes removed slugs)

### E.2 BF checklist

1. Regenerate `tools/verify/pairs.json` (`tools/verify/gen-pairs.mjs`) so the review UI pairs
   only existing library files; prune removed slugs from `decisions.json`, `fixups.json`
   (`nonVisual` list) and `results/`.
2. Full `tools/check-library.sh` on all five rigs; `luxel check --grid 64x64` on every pattern
   (expect zero budget failures); the `perf/measure.mjs` sweep re-run and its top-20 compared
   with `perf/sweep.md` §1; record the asset-archive line before (first commit of the campaign)
   and after.
3. `UPDATES.md`: one dated entry (counts, renames table, engine features, archive bytes).
4. README.md / docs/PLAN.md library description and counts; docs/pattern-ideas.md and
   docs/ideas.md: mark shipped ideas.
5. Gitea tickets (`tea issues create --repo zuckerberg/luxel`): one per §D.2 row; one for the
   §C.1 backlog; one per C.3 ticket-only scene; one for stale code comments that still name
   removed slugs (crates/luxel-core/src/vm.rs, fmath.rs, firmware/src/jit.rs, appwdt.rs,
   luxel-hub75) — they were deliberately not edited to keep file ownership disjoint; one for
   anything batches reported out of scope.
6. Run the `reflect` skill.

### E.3 Renames (old slug → new slug) — for BF's ledger and for scene authors

2d-fireworks-fade → fireworks · 2d-sinc-theta-theta → interference · all-lasers-fire →
searchlights · breathing-gradient → color-wash · bulk-bouncing-balls-2d → bouncing-balls ·
bouncing-balls-2d → bouncing-columns · carrie-s-holiday-star-2d → holiday-star ·
continuous-cellular-automata → cellular-automata · dbzbattlefinal → energy-duel ·
eye-of-sauron → fire-eye · glowflow-3d-coord-transform-api-port → glowflow ·
infinity-flower-2d → lotus-bloom · millipede-1d-2d-controls → millipede ·
perlin-simplex-noise-2d → topo-map · rgbclock-2d → analog-clock · rocket-by-tony-hampton →
rocket-launch · saberdeploy-tutorial → saber · sinpulse-3d → plasma ·
sound-spectrokalidamandala → spectrum-field · sound-starburst-2 → starburst · the-grinch →
light-thief · utilitycolortemp → lamp. Content-replaced in place (slug kept): snake-2d (from
snake-2d-v2), thunderstorm (from lightning-strike's engine), coral-plasma (Neon Veins).
