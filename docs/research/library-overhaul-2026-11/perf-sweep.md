# Library performance sweep — host engine, 308 patterns (2026-11-01)

Raw data beside this file: `sweep-data.json` (merged), `timings.json` + `timings2.json`
(two full passes), `static.json` (per-pixel census), `check.jsonl` (`luxel check` at 64x64,
128x128, 300 px). Scripts: `measure.mjs`, `static.mjs`, `check.sh`, `build.py`, `table.py`.
Per-pattern diagnosis rows come from `diag/group-*.md`. Scratch benches that back some
claims are in `diag/scratch-*/`.

**Method.** The engine wasm (`luxel_wasm.wasm`, interpreter only, no JIT) was driven through
`tools/verify/enginehost.mjs` at a 30 fps delta with synthetic music sensors and a pinned
wall clock. Each measurement is the median ms per `frame()` over 60 frames after 20 warm-up
frames. Rigs: 64x64 `setMapGrid` (4,096 px), 128x128 (16,384 px), and a 300 px mapless
strip. The array ledger was raised to 4 M elements so that a budget-failing pattern still
renders and is timed (as on an S3 PSRAM arena). A separate short run on the default ledger
checked the 64x64 init errors. Two full passes were made; the table uses the lower median of
the two. The median pass-to-pass ratio was 1.018, and the worst was 1.44 (load average
spiked to 21 during pass 1). The `frame()` time includes a pixelCount×3 byte copy, about
0.01 ms at 64x64.

**Caveats.** Host ms ranks the interpreted work, not the device. The wasm host has an FPU,
while Xtensa has no FPU, uses 16.16 math, and pays 150–2,300 cycles per noise, trig, sqrt or
pow call. The "est cyc/px" column is a static census of the render entry: user functions
are inlined, loop trips are resolved from constants, and calls are priced in S3 cycles from
docs/boards.md "Builtins on metal". It is an upper bound for multi-mode patterns (every
branch is summed) and ignores early-outs. For `renderBulk` patterns the census is
per-frame/4096, and unresolved loops count 8 trips. The diagnosis agents corrected the
census where the source says otherwise; their rows say so. No device was touched.

**Headline.**

- **No runtime errors** in any pattern on any rig.
- **25 patterns fail the 64x64 budget**. All are 1-D strip patterns with three or more
  `array(pixelCount)` buffers (§3). 78 fail at 128x128.
- **Three patterns render black** inside the 60-frame window:
  - `sparks-center` is sparse: 81 of 600 frames are lit.
  - `matrix-green-waterfall-1d` at 4,096 px needs more than 80 frames before the drops
    arrive.
  - `sound-rays-frequency-bpm-reactive-1` is **black whenever sensor data is present**: 0 of
    600 frames lit with either a flat or a synthetic spectrum, against 595 of 600 lit with no
    sensors. This is a functional bug worth a ticket, not a perf issue.
- **The dominant cost families in the top 50:**
  1. **N entities tested at every pixel.** Cars, legs, balls, sprites, stars, SDF parts:
     crosstown, both spiders, crosshair, 80s-kid-show, dbz, bouncing-balls-rgb-2d,
     time-flies, metaballs, orv tree, emoji. This is the `renderBulk` + `drawLine`/`splat`/
     `fillRect` case, and the measured wins are 20–40x.
  2. **Per-frame math left inside render.** glittering-jewels, radar, crosshair, the clock
     patterns, carrie/blue-holiday star `pow`s.
  3. **Per-pixel multi-octave noise.** Perlin-fire family, coronal pair, eye-of-sauron pair,
     coral-plasma, kaleidoscope, sun-rays, scary-pumpkin. This needs `fillNoise3D` rows or a
     coarse canvas with `fillCanvas`/`paintCanvas`. It is host-cheap but 70–80 % of the
     device's per-pixel time.
  4. **1-D colour functions evaluated per pixel instead of as a LUT.** ryb-colors (LUT
     measured 16x), utility-palettes (5.5x), breathing-gradient (27x).
  5. **Per-pixel interpreted loops in beforeRender.** synchronized-random-numbers (14x
     measured), novas, bubble-column, portal.


## 1. Full table, sorted by 64x64 ms/frame

ms = host wasm median ms/frame. ns/px @64 = 64x64 ms / 4096. chk = `luxel check` against the 10,236-element array budget (128x128 failures are informational: the S3 panels have the PSRAM arena). est cyc/px = static device estimate (see Caveats); noise/trig/sqrt/pow/color/arr = static per-pixel call counts in the render entry (render2D on a grid; renderBulk counts are per-frame/4096).

| # | pattern | entry | 64x64 ms | 128x128 ms | 300 px ms | ns/px @64 | chk 64 | chk 128 | est cyc/px | noise | trig | sqrt | pow | color | arr | notes |
|--:|---|---|--:|--:|--:|--:|:-:|:-:|--:|--:|--:|--:|--:|--:|--:|---|
| 1 | `crosstown-traffic-2d` | render2D | 10.636 | 41.948 | 0.776 | 2597 | ok | ok | 24013 | 0 | 0 | 48 | 0 | 25 | 216 |  |
| 2 | `synchronized-random-numbers` | render | 8.447 | 30.729 | 0.626 | 2062 | ok | fail | 538 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 3 | `dire-spider-2d` | render2D | 7.357 | 29.301 | 0.537 | 1796 | ok | ok | 6308 | 1 | 2 | 3 | 0 | 1 | 56 |  |
| 4 | `crawling-spider-2d` | render2D | 5.336 | 21.298 | 0.394 | 1303 | ok | ok | 3255 | 0 | 0 | 2 | 0 | 2 | 56 |  |
| 5 | `glittering-jewels` | render | 4.830 | 19.307 | 0.355 | 1179 | ok | ok | 9360 | 0 | 12 | 0 | 6 | 1 | 36 |  |
| 6 | `crosshair-pulse-2d` | render2D | 4.618 | 17.964 | 0.330 | 1127 | ok | ok | 4002 | 0 | 0 | 4 | 0 | 1 | 44 |  |
| 7 | `ryb-colors` | render2D | 4.491 | 17.934 | 0.320 | 1096 | ok | ok | 3599 | 0 | 1 | 0 | 0 | 2 | 0 |  |
| 8 | `butterfly-2d` | render2D | 4.166 | 16.599 | 0.306 | 1017 | ok | ok | 4664 | 0 | 2 | 6 | 2 | 1 | 0 |  |
| 9 | `rgbclock-2d` | render2D | 3.204 | 12.793 | 0.235 | 782 | ok | ok | 4286 | 0 | 1 | 1 | 4 | 1 | 0 |  |
| 10 | `kaleidoscope-2d` | render2D | 2.907 | 11.586 | 0.215 | 710 | ok | ok | 3315 | 2 | 3 | 1 | 0 | 1 | 0 |  |
| 11 | `coronal-ejection-2d` | render2D | 2.828 | 11.071 | 0.206 | 690 | ok | ok | 4690 | 1 | 1 | 1 | 0 | 1 | 16 |  |
| 12 | `80s-kid-show` | render2D | 2.706 | 10.827 | 0.202 | 661 | ok | ok | 15401 | 0 | 0 | 30 | 0 | 1 | 54 |  |
| 13 | `coronal-mass-ejection` | render2D | 2.702 | 10.744 | 0.199 | 660 | ok | ok | 4665 | 1 | 1 | 1 | 0 | 1 | 16 |  |
| 14 | `millipede-1d-2d-controls` | render2D | 2.512 | 9.979 | 0.146 | 613 | ok | ok | 5341 | 0 | 8 | 1 | 2 | 3 | 0 |  |
| 15 | `all-lasers-fire` | render2D | 2.503 | 10.076 | 0.208 | 611 | ok | ok | 4001 | 0 | 3 | 4 | 0 | 1 | 0 |  |
| 16 | `radar-2d` | render2D | 2.500 | 9.908 | 0.184 | 610 | ok | ok | 8918 | 0 | 6 | 6 | 6 | 1 | 20 |  |
| 17 | `dbzbattlefinal` | render2D | 2.408 | 9.570 | 0.177 | 588 | ok | ok | 6552 | 0 | 0 | 12 | 0 | 8 | 34 |  |
| 18 | `perlin-fire-wind` | render2D | 2.336 | 9.340 | 0.201 | 570 | ok | ok | 5965 | 2 | 0 | 0 | 3 | 1 | 0 |  |
| 19 | `voronoi-2d` | render2D | 2.329 | 9.383 | 0.171 | 569 | ok | ok | 3481 | 0 | 5 | 5 | 0 | 4 | 10 |  |
| 20 | `beat-bounce` | render | 2.263 | 9.043 | 0.166 | 552 | ok | ok | 992 | 0 | 0 | 0 | 0 | 1 | 3 |  |
| 21 | `blue-holiday-candle-2d` | render2D | 2.254 | 8.860 | 0.167 | 550 | ok | ok | 2583 | 0 | 4 | 1 | 0 | 3 | 4 |  |
| 22 | `eye-of-sauron-with-movement` | render2D | 2.238 | 8.823 | 0.165 | 546 | ok | ok | 6379 | 2 | 1 | 2 | 0 | 1 | 0 |  |
| 23 | `breathing-gradient` | render | 2.232 | 8.968 | 0.163 | 545 | ok | ok | 751 | 0 | 0 | 0 | 0 | 1 | 9 |  |
| 24 | `eye-of-sauron` | render2D | 2.224 | 8.883 | 0.164 | 543 | ok | ok | 6083 | 2 | 1 | 2 | 0 | 1 | 0 |  |
| 25 | `mandelbrot-2d` | render2D | 2.209 | 8.823 | 0.161 | 539 | ok | ok | 739 | 0 | 0 | 0 | 0 | 2 | 0 | loop trip unresolved |
| 26 | `bubble-column` | render | 2.197 | 8.754 | 0.158 | 536 | ok | fail | 1448 | 1 | 0 | 0 | 0 | 2 | 1 |  |
| 27 | `blue-holiday-star-2d` | render2D | 2.189 | 8.644 | 0.159 | 535 | ok | ok | 5514 | 0 | 0 | 0 | 8 | 1 | 0 |  |
| 28 | `perlin-fire` | render2D | 2.139 | 8.564 | 0.191 | 522 | ok | ok | 5394 | 2 | 0 | 0 | 2 | 1 | 0 |  |
| 29 | `distance-function-kaleidoscope-2` | render2D | 2.119 | 8.429 | 0.157 | 517 | ok | fail | 4302 | 1 | 3 | 1 | 0 | 1 | 8 |  |
| 30 | `carrie-s-holiday-star-2d` | render2D | 2.076 | 8.254 | 0.152 | 507 | ok | ok | 4058 | 0 | 0 | 0 | 6 | 1 | 0 |  |
| 31 | `sunrise-alarm-clock` | render | 2.051 | 8.238 | 0.153 | 501 | ok | ok | 4183 | 1 | 1 | 0 | 4 | 2 | 12 |  |
| 32 | `coral-plasma` | render2D | 2.014 | 8.060 | 0.147 | 492 | ok | ok | 3163 | 1 | 0 | 0 | 0 | 1 | 0 |  |
| 33 | `novas` | renderBulk | 1.920 | 7.778 | 0.145 | 469 | **FAIL** | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 34 | `sun-rays-through-trees` | render2D | 1.896 | 7.571 | 0.140 | 463 | ok | ok | 4138 | 1 | 3 | 1 | 0 | 1 | 0 |  |
| 35 | `2d-clock-with-hand-color-pickers` | render2D | 1.875 | 7.495 | 0.139 | 458 | ok | ok | 3656 | 0 | 1 | 1 | 3 | 4 | 0 |  |
| 36 | `1d-aurora-borealis` | render | 1.864 | 7.473 | 0.137 | 455 | ok | ok | 4846 | 0 | 0 | 10 | 0 | 1 | 45 |  |
| 37 | `orv-christmas-tree` | render2D | 1.854 | 7.374 | 0.146 | 453 | ok | ok | 3196 | 0 | 0 | 0 | 0 | 8 | 25 | loop trip unresolved |
| 38 | `2d-sinc-theta-theta` | render2D | 1.823 | 7.263 | 0.133 | 445 | ok | ok | 3652 | 0 | 3 | 3 | 3 | 1 | 0 |  |
| 39 | `utility-palettes` | render | 1.821 | 7.309 | 0.133 | 445 | ok | ok | 477 | 0 | 0 | 0 | 0 | 0 | 1 |  |
| 40 | `wichmannhill-prng` | render | 1.731 | 6.914 | 0.127 | 423 | ok | fail | 2867 | 0 | 0 | 0 | 0 | 1 | 4 |  |
| 41 | `ripples-2d` | render2D | 1.730 | 6.933 | 0.127 | 422 | ok | ok | 1758 | 0 | 0 | 3 | 0 | 1 | 12 |  |
| 42 | `time-flies-2d` | render2D | 1.725 | 6.746 | 0.008 | 421 | ok | ok | 3925 | 0 | 0 | 6 | 0 | 7 | 18 |  |
| 43 | `bouncing-balls-rgb-2d` | render2D | 1.694 | 6.751 | 0.148 | 414 | ok | ok | 3469 | 0 | 0 | 6 | 0 | 1 | 30 |  |
| 44 | `portal` | render | 1.691 | 6.846 | 0.125 | 413 | ok | fail | 7418 | 0 | 0 | 0 | 0 | 1 | 160 |  |
| 45 | `metaballs-of-fire-2d` | render2D | 1.684 | 6.732 | 0.124 | 411 | ok | ok | 2346 | 0 | 1 | 5 | 0 | 2 | 10 |  |
| 46 | `geometry-morphing-demo-2d` | render2D | 1.645 | 6.574 | 0.121 | 402 | ok | ok | 5949 | 0 | 0 | 12 | 0 | 2 | 0 |  |
| 47 | `oasis` | render | 1.613 | 6.479 | 0.119 | 394 | ok | ok | 2194 | 0 | 0 | 0 | 0 | 1 | 16 |  |
| 48 | `spiral-twirls-star-2d` | render2D | 1.577 | 6.468 | 0.143 | 385 | ok | ok | 1654 | 0 | 1 | 2 | 0 | 2 | 0 |  |
| 49 | `emoji-animation-2` | render2D | 1.569 | 6.270 | 0.118 | 383 | ok | ok | 2570 | 0 | 0 | 2 | 1 | 3 | 13 |  |
| 50 | `scary-pumpkin` | render2D | 1.516 | 6.122 | 0.106 | 370 | ok | ok | 4894 | 1 | 3 | 3 | 0 | 3 | 0 |  |
| 51 | `sound-spectroblots-pow-fade` | render2D | 1.511 | 6.013 | 0.116 | 369 | ok | fail | 3118 | 1 | 0 | 0 | 0 | 1 | 3 |  |
| 52 | `frogger-2d` | render2D | 1.496 | 6.012 | 0.107 | 365 | ok | ok | 3665 | 0 | 1 | 0 | 0 | 3 | 20 |  |
| 53 | `zoom-kaleidoscope` | render2D | 1.405 | 5.585 | 0.104 | 343 | ok | fail | 3754 | 1 | 1 | 0 | 0 | 1 | 8 |  |
| 54 | `rainstorm` | render2D | 1.382 | 5.527 | 0.101 | 337 | ok | ok | 1377 | 0 | 1 | 2 | 0 | 1 | 0 |  |
| 55 | `2d-bouncing-additive-primaries` | render2D | 1.352 | 5.484 | 0.102 | 330 | ok | ok | 1942 | 0 | 0 | 3 | 0 | 1 | 15 |  |
| 56 | `sound-spectrokalidamandala` | render2D | 1.349 | 5.318 | 0.101 | 329 | ok | fail | 1592 | 0 | 1 | 1 | 0 | 1 | 7 |  |
| 57 | `angle-and-radius-from-coordinates` | render2D | 1.339 | 5.352 | 0.100 | 327 | ok | ok | 1404 | 0 | 1 | 1 | 0 | 1 | 0 |  |
| 58 | `scrolls` | render | 1.303 | 5.122 | 0.097 | 318 | ok | fail | 1036 | 0 | 0 | 0 | 0 | 1 | 16 | loop trip unresolved |
| 59 | `perlin-fire-wind-tunnel` | render2D | 1.294 | 5.181 | 0.110 | 316 | ok | ok | 9156 | 4 | 3 | 1 | 0 | 1 | 0 |  |
| 60 | `tixy` | render2D | 1.289 | 5.123 | 0.091 | 315 | ok | ok | 7587 | 0 | 20 | 5 | 0 | 2 | 0 |  |
| 61 | `fireblobs` | renderBulk | 1.282 | 5.103 | 0.097 | 313 | **FAIL** | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 62 | `lightning-clouds` | render2D | 1.261 | 5.013 | 0.093 | 308 | ok | ok | 2670 | 2 | 0 | 1 | 0 | 1 | 0 |  |
| 63 | `post-process-chain` | render | 1.217 | 4.872 | 0.086 | 297 | ok | ok | 1119 | 0 | 0 | 0 | 0 | 1 | 14 |  |
| 64 | `flash-posterize-music-sequencer-framework` | render2D | 1.175 | 4.705 | 0.089 | 287 | **FAIL** | fail | 495 | 0 | 0 | 0 | 0 | 0 | 0 |  |
| 65 | `dna-helix-2d` | render2D | 1.149 | 4.541 | 0.085 | 281 | ok | ok | 1545 | 0 | 3 | 0 | 0 | 3 | 0 |  |
| 66 | `interference-2d` | render2D | 1.144 | 4.579 | 0.084 | 279 | ok | ok | 2030 | 0 | 3 | 3 | 0 | 1 | 6 |  |
| 67 | `sound-spectromatrix-render2d` | render2D | 1.140 | 4.557 | 0.116 | 278 | ok | ok | 1519 | 0 | 2 | 0 | 0 | 1 | 7 |  |
| 68 | `stargen-polar-2d` | render2D | 1.121 | 4.487 | 0.083 | 274 | ok | fail | 14244 | 0 | 21 | 2 | 1 | 13 | 1 |  |
| 69 | `xorcery-2d-3d` | render2D | 1.105 | 4.415 | 0.083 | 270 | ok | ok | 1363 | 0 | 2 | 0 | 0 | 1 | 0 |  |
| 70 | `spotlights-rotation-3d` | render2D | 1.088 | 4.328 | 0.081 | 266 | ok | ok | 1256 | 0 | 0 | 1 | 0 | 1 | 0 |  |
| 71 | `fire-blue` | render | 1.063 | 4.434 | 0.079 | 259 | ok | fail | 519 | 0 | 0 | 0 | 0 | 1 | 4 |  |
| 72 | `bouncer3d` | render2D | 1.053 | 4.170 | 0.078 | 257 | ok | ok | 2384 | 0 | 0 | 0 | 0 | 6 | 15 |  |
| 73 | `twinkling-classic-xmas-strands` | render | 1.029 | 4.117 | 0.075 | 251 | ok | fail | 1280 | 0 | 1 | 1 | 0 | 1 | 9 |  |
| 74 | `us-flag-2d` | render2D | 1.018 | 4.060 | 0.076 | 248 | ok | ok | 1736 | 0 | 2 | 0 | 0 | 4 | 0 |  |
| 75 | `multimap-simpledemo` | render2D | 1.016 | 4.058 | 0.087 | 248 | ok | ok | 689 | 0 | 0 | 0 | 0 | 1 | 4 |  |
| 76 | `soap-2d` | render2D | 1.007 | 3.925 | 0.076 | 246 | ok | ok | 2304 | 3 | 0 | 0 | 0 | 1 | 0 |  |
| 77 | `animated-asterisks-2d` | render2D | 0.998 | 3.974 | 0.076 | 244 | ok | ok | 3345 | 0 | 0 | 8 | 0 | 5 | 12 |  |
| 78 | `sound-spectro-kalidastrip` | render | 0.998 | 4.006 | 0.076 | 244 | ok | fail | 1055 | 0 | 0 | 0 | 0 | 1 | 8 |  |
| 79 | `bouncing-rgb-balls-2d` | render2D | 0.965 | 3.907 | 0.082 | 236 | ok | ok | 1729 | 0 | 0 | 3 | 0 | 1 | 12 |  |
| 80 | `halloween-wavy-bands` | render2D | 0.964 | 3.823 | 0.082 | 235 | ok | ok | 2183 | 1 | 1 | 0 | 1 | 1 | 1 |  |
| 81 | `real-world-lights` | render | 0.955 | 3.825 | 0.070 | 233 | ok | ok | 452 | 0 | 0 | 0 | 0 | 0 | 1 |  |
| 82 | `aurorashivers` | render | 0.939 | 3.741 | 0.072 | 229 | **FAIL** | fail | 439 | 0 | 0 | 0 | 0 | 1 | 3 |  |
| 83 | `radiant-pulse-3` | render2D | 0.931 | 3.710 | 0.070 | 227 | ok | ok | 1407 | 0 | 2 | 1 | 0 | 1 | 0 |  |
| 84 | `tunnel-of-squares-2d` | render2D | 0.929 | 3.675 | 0.071 | 227 | ok | ok | 1294 | 0 | 2 | 0 | 1 | 1 | 0 |  |
| 85 | `m5stack-hex-panels` | render3D | 0.918 | 3.661 | 0.069 | 224 | ok | ok | 3028 | 0 | 3 | 1 | 0 | 1 | 0 |  |
| 86 | `wavy-bands` | render2D | 0.904 | 3.606 | 0.074 | 221 | ok | ok | 2128 | 1 | 1 | 0 | 1 | 1 | 0 |  |
| 87 | `continuous-cellular-automata` | render2D | 0.896 | 3.581 | 0.066 | 219 | ok | ok | 1299 | 0 | 1 | 0 | 0 | 1 | 2 |  |
| 88 | `crossfading` | render | 0.894 | 3.585 | 0.066 | 218 | ok | ok | 459 | 0 | 0 | 0 | 0 | 1 | 2 |  |
| 89 | `glowflow-3d-coord-transform-api-port` | render2D | 0.880 | 3.521 | 0.066 | 215 | ok | fail | 736 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 90 | `us-flag` | render | 0.879 | 3.575 | 0.066 | 215 | ok | ok | 1697 | 0 | 3 | 0 | 0 | 3 | 0 |  |
| 91 | `3d-rotation-spotlights` | render3D | 0.873 | 3.526 | 0.065 | 213 | ok | ok | 1211 | 0 | 0 | 1 | 0 | 1 | 0 |  |
| 92 | `the-grinch` | render2D | 0.872 | 3.458 | 0.050 | 213 | ok | ok | 1394 | 0 | 0 | 0 | 0 | 1 | 4 |  |
| 93 | `blinky-eyes-2d` | render2D | 0.869 | 3.481 | 0.065 | 212 | ok | ok | 1637 | 0 | 0 | 2 | 0 | 4 | 0 |  |
| 94 | `firework-nova` | render2D | 0.865 | 3.461 | 0.063 | 211 | ok | ok | 1097 | 0 | 0 | 1 | 0 | 2 | 0 |  |
| 95 | `custom-sequences` | render | 0.857 | 3.432 | 0.063 | 209 | ok | fail | 3603 | 0 | 0 | 0 | 2 | 1 | 11 |  |
| 96 | `2d-spiral-twirls` | render2D | 0.855 | 3.388 | 0.064 | 209 | ok | ok | 1050 | 0 | 1 | 1 | 0 | 1 | 0 |  |
| 97 | `4th` | renderBulk | 0.854 | 3.406 | 0.064 | 208 | **FAIL** | fail | 849 | 0 | 0 | 0 | 0 | 2 | 4 | bulk: census = per-frame/4096 |
| 98 | `neutronorbit` | renderBulk | 0.841 | 3.385 | 0.066 | 205 | **FAIL** | fail | 200 | 0 | 0 | 0 | 0 | 1.002 | 3.006 | loop trip unresolved; bulk: census = per-frame/4096 |
| 99 | `amoeba` | render | 0.836 | 3.306 | 0.064 | 204 | ok | fail | 509 | 0 | 0 | 0 | 0 | 1 | 2 |  |
| 100 | `sparkfire` | render | 0.832 | 3.320 | 0.062 | 203 | ok | fail | 506 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 101 | `sound-lavablob` | render2D | 0.827 | 3.307 | 0.060 | 202 | ok | ok | 1416 | 0 | 5 | 0 | 0 | 1 | 1 |  |
| 102 | `easing-library-v1-0` | render2D | 0.822 | 3.325 | 0.018 | 201 | ok | ok | 1336 | 0 | 0 | 1 | 0 | 4 | 1 |  |
| 103 | `perlin-simplex-noise-2d` | render2D | 0.817 | 3.130 | 0.098 | 199 | ok | ok | 1415 | 0 | 0 | 0 | 0 | 2 | 1 |  |
| 104 | `light-organ-sensor-board` | render | 0.814 | 3.254 | 0.063 | 199 | ok | ok | 1475 | 0 | 0 | 0 | 0 | 1 | 27 | loop trip unresolved |
| 105 | `multisegment-demo` | render | 0.811 | 3.259 | 0.061 | 198 | ok | ok | 7698 | 0 | 8 | 0 | 0 | 28 | 23 | loop trip unresolved |
| 106 | `color-bands-buffered` | renderBulk | 0.809 | 3.233 | 0.059 | 197 | **FAIL** | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 107 | `2d-fireworks-fade` | render2D | 0.805 | 3.256 | 0.061 | 197 | ok | ok | 619 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 108 | `traffic` | render2D | 0.797 | 3.193 | 0.059 | 195 | ok | ok | 1375 | 0 | 1 | 1 | 0 | 1 | 0 |  |
| 109 | `fire-red` | render | 0.792 | 3.167 | 0.058 | 193 | ok | fail | 721 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 110 | `palette-fire-2d` | render2D | 0.792 | 3.144 | 0.059 | 193 | ok | ok | 2932 | 1 | 0 | 0 | 0 | 1 | 0 |  |
| 111 | `spiral-2d` | render2D | 0.789 | 3.152 | 0.059 | 193 | ok | ok | 1171 | 0 | 2 | 1 | 0 | 1 | 0 |  |
| 112 | `sunset` | render | 0.786 | 3.159 | 0.058 | 192 | ok | ok | 1783 | 0 | 3 | 0 | 0 | 1 | 0 |  |
| 113 | `digital-rain-2d` | render2D | 0.783 | 3.112 | 0.059 | 191 | ok | ok | 1000 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 114 | `polar-mapping-helper-2d-3d` | render2D | 0.778 | 3.069 | 0.064 | 190 | ok | ok | 938 | 0 | 0 | 0 | 0 | 7 | 1 |  |
| 115 | `aurora-2d` | renderBulk | 0.775 | 3.075 | 0.063 | 189 | ok | ok | 10 | 0 | 0 | 0 | 0 | 0.016 | 0.094 | loop trip unresolved; bulk: census = per-frame/4096 |
| 116 | `spinwheel-2d` | render2D | 0.771 | 3.082 | 0.058 | 188 | ok | ok | 1182 | 0 | 1 | 1 | 0 | 1 | 0 |  |
| 117 | `cube-fire-3d` | render2D | 0.767 | 3.061 | 0.057 | 187 | ok | ok | 1482 | 0 | 6 | 0 | 0 | 1 | 0 |  |
| 118 | `matrix-2-tone-pulse` | render | 0.748 | 2.991 | 0.055 | 183 | ok | ok | 2058 | 0 | 2 | 0 | 0 | 1 | 0 |  |
| 119 | `lightning-strike` | render | 0.741 | 2.478 | 0.055 | 181 | **FAIL** | fail | 575 | 0 | 0 | 0 | 0 | 2 | 2 |  |
| 120 | `line-dancer-2d` | render2D | 0.735 | 2.917 | 0.056 | 179 | ok | ok | 2403 | 0 | 7 | 2 | 0 | 1 | 0 |  |
| 121 | `color-bands` | render | 0.730 | 2.900 | 0.053 | 178 | ok | ok | 1520 | 0 | 4 | 0 | 0 | 1 | 0 |  |
| 122 | `complements-3d` | render2D | 0.728 | 2.853 | 0.053 | 178 | ok | ok | 784 | 0 | 0 | 0 | 0 | 1 | 6 |  |
| 123 | `ocean` | render | 0.719 | 2.871 | 0.053 | 176 | ok | ok | 1035 | 0 | 3 | 0 | 0 | 1 | 0 |  |
| 124 | `xmasflies` | render | 0.710 | 2.868 | 0.052 | 173 | **FAIL** | fail | 546 | 0 | 0 | 0 | 0 | 1 | 4 |  |
| 125 | `heart` | render2D | 0.688 | 2.741 | 0.052 | 168 | ok | ok | 1303 | 0 | 0 | 1 | 0 | 2 | 0 |  |
| 126 | `wanderedges` | render | 0.682 | 2.757 | 0.052 | 167 | ok | fail | 379 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 127 | `shimmer-crossfade-2d` | render2D | 0.681 | 2.708 | 0.051 | 166 | ok | ok | 637 | 0 | 0 | 0 | 0 | 0 | 1 |  |
| 128 | `spin-cycle` | render | 0.675 | 2.714 | 0.051 | 165 | ok | ok | 1513 | 0 | 4 | 0 | 0 | 1 | 0 |  |
| 129 | `ember-diffusion` | render | 0.668 | 2.672 | 0.049 | 163 | ok | ok | 648 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 130 | `halloween-color-twinkles` | render | 0.668 | 2.679 | 0.049 | 163 | ok | ok | 1413 | 0 | 4 | 0 | 0 | 1 | 0 |  |
| 131 | `rock-sparks` | render2D | 0.666 | 2.670 | 0.049 | 163 | ok | ok | 6282 | 0 | 15 | 0 | 0 | 4 | 18 |  |
| 132 | `static-random-colors` | render | 0.655 | 2.625 | 0.049 | 160 | ok | ok | 835 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 133 | `green-ripple-reflections` | render | 0.645 | 2.581 | 0.048 | 158 | ok | ok | 1374 | 0 | 3 | 0 | 0 | 1 | 0 |  |
| 134 | `slowflies` | render | 0.643 | 2.563 | 0.049 | 157 | ok | fail | 484 | 0 | 0 | 0 | 0 | 1 | 2 |  |
| 135 | `doom-fire` | render2D | 0.642 | 2.450 | 0.075 | 157 | ok | ok | 865 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 136 | `matrix-2d-pulse-edit` | render | 0.640 | 2.563 | 0.047 | 156 | ok | ok | 1819 | 0 | 2 | 0 | 0 | 1 | 0 |  |
| 137 | `glitch-bands` | render | 0.636 | 2.543 | 0.047 | 155 | ok | ok | 1819 | 0 | 0 | 0 | 2 | 1 | 0 |  |
| 138 | `coolaura` | renderBulk | 0.633 | 2.514 | 0.047 | 155 | ok | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 139 | `infinity-flower-2d` | render2D | 0.631 | 2.514 | 0.044 | 154 | ok | ok | 3797 | 0 | 2 | 1 | 0 | 8 | 10 |  |
| 140 | `tree-setup-pattern` | render2D | 0.617 | 2.444 | 0.022 | 151 | ok | ok | 1049 | 0 | 1 | 0 | 0 | 1 | 0 |  |
| 141 | `slime-mold-palette` | render2D | 0.615 | 1.582 | 0.312 | 150 | ok | ok | 699 | 0 | 0 | 0 | 0 | 2 | 1 |  |
| 142 | `newfire` | render | 0.614 | 2.445 | 0.045 | 150 | ok | fail | 720 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 143 | `spinning-plasma-2d` | render2D | 0.611 | 2.443 | 0.046 | 149 | ok | ok | 1345 | 1 | 0 | 0 | 0 | 1 | 0 |  |
| 144 | `opposites` | render | 0.601 | 2.397 | 0.046 | 147 | ok | ok | 1210 | 0 | 3 | 0 | 0 | 1 | 0 |  |
| 145 | `twocolorhsvmix` | render | 0.597 | 2.385 | 0.044 | 146 | ok | ok | 1597 | 0 | 2 | 0 | 1 | 1 | 0 |  |
| 146 | `perlin-simplex-noise-1d` | render | 0.591 | 2.369 | 0.043 | 144 | ok | fail | 1273 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 147 | `curl-flow-2d` | render2D | 0.586 | 2.320 | 0.049 | 143 | ok | ok | 656 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 148 | `snake-2d` | render2D | 0.585 | 2.342 | 0.031 | 143 | ok | ok | 1778 | 0 | 0 | 0 | 0 | 7 | 2 |  |
| 149 | `color-fade-pulse` | render | 0.579 | 2.316 | 0.043 | 141 | ok | ok | 830 | 0 | 1 | 0 | 0 | 1 | 0 |  |
| 150 | `color-twinkles` | render | 0.573 | 2.286 | 0.043 | 140 | ok | ok | 1061 | 0 | 4 | 0 | 0 | 1 | 0 |  |
| 151 | `block-reflections` | render | 0.571 | 2.276 | 0.043 | 139 | ok | ok | 929 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 152 | `audio-volume-meter` | render | 0.568 | 2.275 | 0.046 | 139 | **FAIL** | fail | 516 | 0 | 0 | 0 | 0 | 1 | 3 |  |
| 153 | `sinpulse-3d` | render2D | 0.564 | 2.248 | 0.041 | 138 | ok | ok | 1290 | 0 | 3 | 0 | 0 | 1 | 0 |  |
| 154 | `upward-waves-3d-using-accelerometer` | render3D | 0.563 | 2.237 | 0.043 | 137 | ok | ok | 829 | 0 | 0 | 1 | 0 | 1 | 0 |  |
| 155 | `stairmaster-2d` | render2D | 0.550 | 2.189 | 0.042 | 134 | ok | ok | 978 | 0 | 0 | 1 | 0 | 1 | 0 |  |
| 156 | `bustle` | render | 0.550 | 2.170 | 0.043 | 134 | **FAIL** | fail | 499 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 157 | `music-sequencer-for-v2` | render2D | 0.548 | 2.204 | 0.042 | 134 | **FAIL** | fail | 435 | 0 | 0 | 0 | 0 | 0 | 0 |  |
| 158 | `bessel-chaos` | render | 0.548 | 2.175 | 0.042 | 134 | ok | ok | 1052 | 0 | 2 | 0 | 0 | 1 | 0 |  |
| 159 | `sinus` | render2D | 0.547 | 2.156 | 0.041 | 134 | ok | ok | 761 | 0 | 1 | 0 | 0 | 1 | 0 |  |
| 160 | `doom-fire-2d` | render2D | 0.534 | 2.044 | 0.063 | 130 | ok | ok | 651 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 161 | `reaction-diffusion-2d` | render2D | 0.529 | 1.740 | 0.151 | 129 | ok | ok | 626 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 162 | `heatshivers` | renderBulk | 0.524 | 2.090 | 0.040 | 128 | **FAIL** | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 163 | `blink-fade` | render | 0.502 | 1.953 | 0.036 | 123 | ok | fail | 461 | 0 | 0 | 0 | 0 | 1 | 2 |  |
| 164 | `sound-music-spectrum-visualizer` | render | 0.501 | 2.010 | 0.037 | 122 | ok | fail | 695 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 165 | `tetrix-2d` | render2D | 0.490 | 1.929 | 0.038 | 120 | ok | ok | 1114 | 0 | 0 | 0 | 0 | 4 | 7 |  |
| 166 | `breakout-2d` | render2D | 0.480 | 1.917 | 0.036 | 117 | ok | ok | 1376 | 0 | 0 | 1 | 0 | 5 | 1 |  |
| 167 | `matrix-2d-honeycomb` | render2D | 0.478 | 1.899 | 0.043 | 117 | ok | ok | 1140 | 0 | 2 | 0 | 0 | 1 | 0 |  |
| 168 | `light-organ-2-0` | render | 0.473 | 1.870 | 0.040 | 116 | ok | ok | 2324 | 0 | 1 | 0 | 0 | 3 | 7 |  |
| 169 | `2d-canvas-example` | render2D | 0.472 | 1.882 | 0.035 | 115 | ok | ok | 571 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 170 | `sound-spectrum-analyser` | render2D | 0.467 | 1.843 | 0.039 | 114 | ok | ok | 851 | 0 | 0 | 0 | 0 | 3 | 2 |  |
| 171 | `sound-blinkfade` | render | 0.467 | 1.855 | 0.035 | 114 | ok | fail | 486 | 0 | 0 | 0 | 0 | 1 | 2 |  |
| 172 | `fast-pulse-3d` | render2D | 0.466 | 1.852 | 0.022 | 114 | ok | ok | 1117 | 0 | 0 | 0 | 1 | 1 | 0 |  |
| 173 | `marching-rainbow` | render | 0.461 | 1.841 | 0.035 | 113 | ok | ok | 1263 | 0 | 5 | 0 | 0 | 1 | 0 |  |
| 174 | `2d-wandering-fireball` | render2D | 0.457 | 1.821 | 0.034 | 112 | ok | ok | 946 | 0 | 0 | 0 | 0 | 2 | 0 |  |
| 175 | `midpointdisplacement1d` | render | 0.456 | 1.820 | 0.034 | 111 | ok | ok | 805 | 0 | 0 | 0 | 0 | 1 | 3 |  |
| 176 | `angry-xmass-3d` | render3D | 0.444 | 1.781 | 0.033 | 108 | ok | ok | 661 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 177 | `quiet-blinkfade` | render | 0.444 | 1.777 | 0.033 | 108 | ok | fail | 515 | 0 | 0 | 0 | 0 | 2 | 1 |  |
| 178 | `music-sequencer-for-v3-only` | renderBulk | 0.443 | 1.765 | 0.034 | 108 | **FAIL** | fail | 110 | 0 | 0 | 0 | 0 | 0 | 2 | loop trip unresolved; bulk: census = per-frame/4096 |
| 179 | `slow-color-shift` | render | 0.442 | 1.757 | 0.033 | 108 | ok | ok | 945 | 0 | 2 | 0 | 0 | 1 | 0 |  |
| 180 | `marching-rainbow-buffered` | renderBulk | 0.435 | 1.737 | 0.033 | 106 | ok | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 181 | `color-twinkle-bounce` | render | 0.431 | 1.723 | 0.032 | 105 | ok | ok | 838 | 0 | 2 | 0 | 0 | 1 | 0 |  |
| 182 | `rainbow-rocket-sparks` | render | 0.422 | 1.692 | 0.031 | 103 | ok | ok | 623 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 183 | `glitter` | render | 0.410 | 1.636 | 0.030 | 100 | ok | ok | 777 | 0 | 0 | 0 | 0 | 2 | 0 |  |
| 184 | `christmasstretch` | render | 0.408 | 1.633 | 0.031 | 100 | ok | ok | 1076 | 0 | 0 | 0 | 0 | 3 | 0 |  |
| 185 | `millipede` | render | 0.406 | 1.621 | 0.031 | 99 | ok | ok | 932 | 0 | 2 | 0 | 0 | 1 | 0 |  |
| 186 | `color-blend` | render | 0.404 | 1.618 | 0.030 | 99 | ok | ok | 894 | 0 | 3 | 0 | 0 | 1 | 0 |  |
| 187 | `color-pick-fade` | render | 0.398 | 1.591 | 0.029 | 97 | ok | ok | 583 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 188 | `rainbow-melt` | render | 0.394 | 1.574 | 0.030 | 96 | ok | ok | 1103 | 0 | 3 | 0 | 0 | 1 | 0 |  |
| 189 | `scanner` | render | 0.393 | 1.570 | 0.030 | 96 | ok | fail | 538 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 190 | `white-rainbows` | render | 0.392 | 1.558 | 0.031 | 96 | ok | fail | 653 | 0 | 0 | 0 | 0 | 1 | 4 |  |
| 191 | `autumn-colors` | render | 0.388 | 1.538 | 0.029 | 95 | ok | fail | 461 | 0 | 0 | 0 | 0 | 1 | 2 |  |
| 192 | `stacker` | render | 0.385 | 1.549 | 0.033 | 94 | ok | ok | 1193 | 0 | 0 | 0 | 0 | 4 | 0 |  |
| 193 | `sierpinski-rainbow-2d` | render2D | 0.383 | 1.518 | 0.033 | 94 | ok | ok | 1019 | 0 | 3 | 0 | 0 | 1 | 0 |  |
| 194 | `example-time-and-animation` | render | 0.379 | 1.539 | 0.029 | 93 | ok | ok | 563 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 195 | `sound-starburst-2` | render | 0.370 | 1.460 | 0.029 | 90 | **FAIL** | fail | 880 | 0 | 0 | 0 | 0 | 1 | 8 |  |
| 196 | `firework-rocket-sparks` | render | 0.368 | 1.462 | 0.027 | 90 | ok | ok | 976 | 0 | 2 | 0 | 0 | 2 | 0 |  |
| 197 | `rainbow-fonts` | render | 0.366 | 1.455 | 0.028 | 89 | ok | ok | 1039 | 0 | 2 | 0 | 0 | 1 | 0 |  |
| 198 | `christmas-rg-fade` | render | 0.365 | 1.464 | 0.028 | 89 | ok | fail | 461 | 0 | 0 | 0 | 0 | 1 | 2 |  |
| 199 | `fast-pulse` | render | 0.360 | 1.443 | 0.027 | 88 | ok | ok | 704 | 0 | 1 | 0 | 0 | 1 | 0 |  |
| 200 | `snake` | render | 0.360 | 1.437 | 0.027 | 88 | ok | ok | 862 | 0 | 0 | 0 | 0 | 3 | 0 |  |
| 201 | `thunderstorm` | render | 0.359 | 1.436 | 0.027 | 88 | ok | ok | 548 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 202 | `boids-2d` | render2D | 0.359 | 1.419 | 0.032 | 88 | ok | ok | 601 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 203 | `christmas-lights` | render | 0.358 | 1.439 | 0.027 | 87 | ok | ok | 841 | 0 | 0 | 0 | 0 | 2 | 3 |  |
| 204 | `kitt-w-color-picker` | render | 0.357 | 1.429 | 0.028 | 87 | ok | fail | 431 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 205 | `spirograph-2d` | render2D | 0.353 | 1.425 | 0.027 | 86 | ok | ok | 601 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 206 | `pixelclock` | render | 0.348 | 1.391 | 0.026 | 85 | ok | ok | 998 | 0 | 0 | 0 | 0 | 4 | 0 |  |
| 207 | `typing-heatmap-2d` | render2D | 0.343 | 1.364 | 0.028 | 84 | ok | ok | 610 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 208 | `bouncing-balls-2d` | render2D | 0.341 | 1.275 | 0.048 | 83 | ok | ok | 719 | 0 | 0 | 0 | 0 | 3 | 1 |  |
| 209 | `scrolling-text-marquee-2d` | render2D | 0.340 | 1.356 | 0.033 | 83 | ok | ok | 708 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 210 | `sunrise-2d` | render2D | 0.339 | 1.351 | 0.025 | 83 | ok | ok | 758 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 211 | `falling-sand-2d` | render2D | 0.336 | 1.322 | 0.037 | 82 | ok | ok | 601 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 212 | `gradient-blue-purple-pink` | render | 0.333 | 1.331 | 0.025 | 81 | ok | ok | 583 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 213 | `rgb-xyz-3d-sweep` | render3D | 0.333 | 1.382 | 0.025 | 81 | ok | ok | 1005 | 0 | 1 | 0 | 0 | 2 | 0 |  |
| 214 | `matrix-rain` | render2D | 0.332 | 1.300 | 0.026 | 81 | ok | ok | 740 | 0 | 0 | 0 | 0 | 2 | 2 |  |
| 215 | `single-color-picker-wide-or-spot` | render | 0.330 | 1.320 | 0.025 | 81 | ok | ok | 1069 | 0 | 0 | 0 | 1 | 1 | 0 |  |
| 216 | `cylon` | render | 0.328 | 1.322 | 0.025 | 80 | ok | fail | 431 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 217 | `cyclic-cellular-automata-2d` | render2D | 0.327 | 1.287 | 0.025 | 80 | ok | ok | 854 | 0 | 1 | 0 | 0 | 1 | 1 |  |
| 218 | `spring-colors` | render | 0.327 | 1.308 | 0.024 | 80 | ok | fail | 461 | 0 | 0 | 0 | 0 | 1 | 2 |  |
| 219 | `kitt` | render | 0.326 | 1.302 | 0.025 | 79 | ok | fail | 379 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 220 | `example-modes-and-waveforms` | render | 0.324 | 1.298 | 0.024 | 79 | ok | ok | 563 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 221 | `sound-rays` | render | 0.321 | 1.296 | 0.024 | 78 | ok | fail | 725 | 0 | 0 | 0 | 0 | 1 | 2 |  |
| 222 | `lissajous-curve-tracer` | render2D | 0.320 | 1.106 | 0.067 | 78 | ok | ok | 631 | 0 | 0 | 0 | 0 | 1 | 2 |  |
| 223 | `matrix-green-waterfall-2d` | render2D | 0.318 | 1.274 | 0.024 | 78 | ok | ok | 799 | 0 | 1 | 0 | 0 | 1 | 0 |  |
| 224 | `unstable-orbits-2d` | render2D | 0.317 | 1.239 | 0.027 | 77 | ok | ok | 631 | 0 | 0 | 0 | 0 | 1 | 2 |  |
| 225 | `regenbogendrogen` | render | 0.317 | 1.251 | 0.024 | 77 | ok | ok | 825 | 0 | 2 | 0 | 0 | 1 | 0 |  |
| 226 | `holiday-diagonal-stripes` | render2D | 0.316 | 1.219 | 0.024 | 77 | ok | ok | 947 | 0 | 1 | 0 | 0 | 3 | 0 |  |
| 227 | `rainbow-comet` | renderBulk | 0.316 | 1.278 | 0.024 | 77 | **FAIL** | fail | 341 | 0 | 0 | 0 | 0 | 1 | 6 | bulk: census = per-frame/4096 |
| 228 | `sound-rays-frequency-bpm-reactive-1` | render | 0.314 | 1.242 | 0.024 | 77 | ok | fail | 725 | 0 | 0 | 0 | 0 | 1 | 2 | black @64 in window |
| 229 | `accelerometer-level-example` | render2D | 0.311 | 1.237 | 0.025 | 76 | ok | ok | 571 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 230 | `sparks-center` | render | 0.310 | 1.241 | 0.024 | 76 | ok | fail | 456 | 0 | 0 | 0 | 0 | 1 | 1 | black @64 in window |
| 231 | `starfield-2d` | render2D | 0.306 | 1.212 | 0.027 | 75 | ok | ok | 601 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 232 | `firework-dust` | render | 0.304 | 1.205 | 0.022 | 74 | ok | fail | 601 | 0 | 0 | 0 | 0 | 1 | 4 |  |
| 233 | `golden-tix` | render | 0.296 | 1.179 | 0.022 | 72 | ok | ok | 844 | 0 | 1 | 0 | 0 | 2 | 0 |  |
| 234 | `wanderers` | render2D | 0.292 | 1.124 | 0.033 | 71 | ok | ok | 660 | 0 | 0 | 0 | 0 | 2 | 1 |  |
| 235 | `performance-test-framework` | render | 0.289 | 1.148 | 0.025 | 71 | ok | ok | 615 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 236 | `2-colors` | render | 0.285 | 1.139 | 0.021 | 70 | ok | ok | 730 | 0 | 0 | 0 | 0 | 1 | 3 |  |
| 237 | `pendulum-wave` | render | 0.283 | 1.136 | 0.021 | 69 | ok | ok | 673 | 0 | 1 | 0 | 0 | 1 | 0 |  |
| 238 | `example-color-hues` | render | 0.282 | 1.131 | 0.021 | 69 | ok | ok | 563 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 239 | `sunrise` | render | 0.281 | 1.138 | 0.021 | 69 | ok | ok | 805 | 0 | 0 | 0 | 0 | 3 | 0 |  |
| 240 | `mapping-helper-single-and-10x` | render | 0.281 | 1.126 | 0.021 | 69 | ok | ok | 987 | 0 | 0 | 0 | 0 | 4 | 3 |  |
| 241 | `comets` | render | 0.279 | 1.108 | 0.021 | 68 | ok | fail | 538 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 242 | `rainbow-smiley` | render2D | 0.274 | 1.092 | 0.010 | 67 | ok | ok | 767 | 0 | 0 | 0 | 0 | 2 | 1 |  |
| 243 | `edgeburst` | render | 0.270 | 1.081 | 0.020 | 66 | ok | ok | 583 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 244 | `chasing-rainbows-hsluv` | render | 0.270 | 1.084 | 0.020 | 66 | **FAIL** | fail | 3579 | 0 | 1 | 0 | 2 | 6 | 15 |  |
| 245 | `colourful-fireflies` | render | 0.260 | 1.034 | 0.019 | 63 | ok | fail | 461 | 0 | 0 | 0 | 0 | 1 | 2 |  |
| 246 | `rainbow-pinwheel` | render | 0.256 | 1.022 | 0.019 | 62 | ok | ok | 654 | 0 | 1 | 0 | 0 | 1 | 0 |  |
| 247 | `fairies` | renderBulk | 0.255 | 1.020 | 0.019 | 62 | **FAIL** | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 248 | `christmas-candy-cane` | render | 0.252 | 1.001 | 0.019 | 62 | ok | ok | 776 | 0 | 0 | 0 | 0 | 2 | 0 |  |
| 249 | `twinkle` | render | 0.251 | 0.975 | 0.023 | 61 | ok | fail | 431 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 250 | `christmaslights` | render | 0.249 | 0.997 | 0.019 | 61 | ok | ok | 892 | 0 | 0 | 0 | 0 | 3 | 1 |  |
| 251 | `matrix-green-waterfall-1d` | render | 0.248 | 0.986 | 0.021 | 61 | ok | fail | 542 | 0 | 0 | 0 | 0 | 2 | 1 | black @64 in window |
| 252 | `bouncing-balls-hsv` | render | 0.248 | 1.004 | 0.020 | 61 | ok | fail | 461 | 0 | 0 | 0 | 0 | 1 | 2 |  |
| 253 | `twinkly-stars` | render | 0.245 | 0.985 | 0.018 | 60 | ok | fail | 860 | 0 | 0 | 0 | 0 | 3 | 3 |  |
| 254 | `marquee-chase` | render | 0.236 | 0.945 | 0.018 | 58 | ok | ok | 558 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 255 | `bouncy-boxes` | renderBulk | 0.230 | 0.256 | 0.217 | 56 | ok | ok | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 256 | `tv-simulator` | render | 0.229 | 0.917 | 0.017 | 56 | ok | ok | 644 | 0 | 0 | 0 | 0 | 2 | 0 |  |
| 257 | `rainbow-flag` | render | 0.228 | 0.911 | 0.017 | 56 | ok | ok | 695 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 258 | `chevron-2d` | render2D | 0.227 | 0.898 | 0.018 | 55 | ok | ok | 571 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 259 | `ice-floes-2d` | renderBulk | 0.227 | 0.250 | 0.218 | 55 | ok | ok | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 260 | `3-color-rotation` | render | 0.225 | 0.915 | 0.017 | 55 | ok | ok | 670 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 261 | `fast-palette-blending` | render | 0.224 | 0.866 | 0.017 | 55 | ok | ok | 572 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 262 | `fireflies` | render | 0.218 | 0.870 | 0.018 | 53 | ok | fail | 456 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 263 | `fractal-flower` | renderBulk | 0.214 | 0.238 | 0.208 | 52 | ok | ok | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 264 | `drip` | render | 0.209 | 0.839 | 0.016 | 51 | ok | fail | 431 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 265 | `lightbulb-crank-hue-to-complete` | render | 0.202 | 0.808 | 0.015 | 49 | ok | ok | 739 | 0 | 0 | 0 | 0 | 2 | 4 |  |
| 266 | `three-red-pixels-mathy` | render | 0.195 | 0.782 | 0.015 | 48 | ok | ok | 619 | 0 | 0 | 0 | 0 | 2 | 0 |  |
| 267 | `cellular-automata-1d` | render | 0.195 | 0.821 | 0.015 | 48 | **FAIL** | fail | 538 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 268 | `three-red-pixels-array` | render | 0.192 | 0.764 | 0.015 | 47 | ok | fail | 431 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 269 | `meteor-shower` | render | 0.190 | 0.761 | 0.015 | 46 | **FAIL** | fail | 598 | 0 | 0 | 0 | 0 | 1 | 3 |  |
| 270 | `policelights` | render | 0.185 | 0.742 | 0.014 | 45 | ok | ok | 665 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 271 | `b-lightning-flashes` | render | 0.180 | 0.709 | 0.014 | 44 | ok | ok | 617 | 0 | 0 | 0 | 0 | 2 | 0 |  |
| 272 | `static-christmas-lights-4-colors` | render | 0.176 | 0.723 | 0.013 | 43 | ok | ok | 695 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 273 | `saberdeploy-tutorial` | render | 0.172 | 0.685 | 0.013 | 42 | ok | ok | 512 | 0 | 0 | 0 | 0 | 2 | 0 |  |
| 274 | `sparks` | render | 0.170 | 0.676 | 0.017 | 42 | ok | fail | 431 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 275 | `twinkle-2` | renderBulk | 0.168 | 0.666 | 0.013 | 41 | ok | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 276 | `rgbw-mapping-tester` | render | 0.162 | 0.659 | 0.012 | 40 | ok | ok | 551 | 0 | 0 | 0 | 0 | 4 | 0 |  |
| 277 | `rainbow` | render | 0.161 | 0.644 | 0.012 | 39 | ok | ok | 533 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 278 | `rainbow-v2` | render | 0.156 | 0.624 | 0.012 | 38 | ok | ok | 508 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 279 | `nano-orbital` | render | 0.153 | 0.616 | 0.012 | 37 | ok | fail | 538 | 0 | 0 | 0 | 0 | 1 | 1 |  |
| 280 | `rgbw-mapping-tester-hsv-version` | render | 0.148 | 0.593 | 0.011 | 36 | ok | ok | 948 | 0 | 0 | 0 | 0 | 4 | 0 |  |
| 281 | `christmaspewpew` | renderBulk | 0.143 | 0.571 | 0.012 | 35 | **FAIL** | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 282 | `a-peak-integrator` | render | 0.142 | 0.541 | 0.021 | 35 | ok | ok | 818 | 0 | 0 | 0 | 0 | 3 | 2 |  |
| 283 | `example-smooth-speed-slider` | render | 0.130 | 0.518 | 0.010 | 32 | ok | ok | 508 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 284 | `example-button-w-debounce` | render | 0.115 | 0.467 | 0.009 | 28 | ok | ok | 508 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 285 | `sound-reactive-color-fade` | render2D | 0.115 | 0.470 | 0.007 | 28 | ok | ok | 521 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 286 | `2-purple-fade` | render | 0.097 | 0.400 | 0.008 | 24 | ok | ok | 401 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 287 | `1-white-fade` | render | 0.097 | 0.386 | 0.007 | 24 | ok | ok | 401 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 288 | `sound-spectromatrix-agc` | renderBulk | 0.089 | 0.113 | 0.079 | 22 | ok | ok | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 289 | `naturallightsync` | render | 0.088 | 0.362 | 0.007 | 21 | ok | ok | 349 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 290 | `utilitycolortemp` | render | 0.087 | 0.348 | 0.007 | 21 | ok | ok | 349 | 0 | 0 | 0 | 0 | 1 | 0 |  |
| 291 | `bulk-bouncing-balls-2d` | renderBulk | 0.069 | 0.246 | 0.028 | 17 | ok | ok | 1 | 0 | 0 | 0 | 0 | 0.002 | 0.01 | bulk: census = per-frame/4096 |
| 292 | `rocket-by-tony-hampton` | renderBulk | 0.060 | 0.236 | 0.006 | 15 | **FAIL** | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 293 | `bulk-canvas-ripples-2d` | renderBulk | 0.055 | 0.080 | 0.047 | 13 | ok | ok | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 294 | `bulk-sprite-scroll-2d` | renderBulk | 0.053 | 0.205 | 0.005 | 13 | ok | ok | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 295 | `raindrops-2d` | renderBulk | 0.051 | 0.074 | 0.043 | 13 | ok | ok | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 296 | `pew-pew-pew` | renderBulk | 0.046 | 0.166 | 0.008 | 11 | **FAIL** | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 297 | `flow-field-2d` | renderBulk | 0.039 | 0.063 | 0.030 | 9 | ok | ok | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 298 | `frame-rate-test` | renderBulk | 0.036 | 0.126 | 0.011 | 9 | ok | ok | 1 | 0 | 0 | 0 | 0 | 0.004 | 0.002 | bulk: census = per-frame/4096; setFrameRate |
| 299 | `fireworks-finale` | renderBulk | 0.032 | 0.111 | 0.005 | 8 | **FAIL** | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 300 | `bouncing-balls-rgb` | renderBulk | 0.029 | 0.107 | 0.004 | 7 | **FAIL** | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 301 | `bulk-rainbow` | renderBulk | 0.026 | 0.101 | 0.003 | 6 | ok | ok | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 302 | `swirlpool-2d` | renderBulk | 0.025 | 0.049 | 0.017 | 6 | ok | ok | 0 | 0 | 0 | 0 | 0 | 0 | 0.004 | loop trip unresolved; bulk: census = per-frame/4096 |
| 303 | `chill-confetti` | renderBulk | 0.025 | 0.095 | 0.003 | 6 | ok | fail | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 304 | `nyan-lights` | renderBulk | 0.014 | 0.037 | 0.006 | 3 | ok | ok | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 305 | `snake-2d-v2` | renderBulk | 0.014 | 0.037 | 0.006 | 3 | ok | ok | 0 | 0 | 0 | 0 | 0 | 0 | 0 | bulk: census = per-frame/4096 |
| 306 | `frame-rate-scan` | renderBulk | 0.012 | 0.030 | 0.004 | 3 | ok | ok | 2 | 0 | 0 | 0 | 0 | 0.017 | 0 | loop trip unresolved; bulk: census = per-frame/4096; setFrameRate |
| 307 | `bulk-comet-trails` | renderBulk | 0.009 | 0.028 | 0.001 | 2 | ok | ok | 0 | 0 | 0 | 0 | 0 | 0.001 | 0 | bulk: census = per-frame/4096 |
| 308 | `frame-rate-scan-fast` | renderBulk | 0.007 | 0.020 | 0.006 | 2 | ok | ok | 0 | 0 | 0 | 0 | 0 | 0.001 | 0 | bulk: census = per-frame/4096 |

## 2. The 50 most expensive (64x64): why, and the cheaper design

Gains are cost-model estimates unless a row says *measured* (host scratch benches in `diag/scratch-*`).

| # | pattern | 64x64 ms | why | fix | tag |
|--:|---|--:|---|---|---|
| 1 | `crosstown-traffic-2d` | 10.64 | WHY: render2D tests all 24 cars at every pixel. The `px < lo` / `px > hi` branches call `dist()` whenever the pixel lies beyond either end of a bar, which is most pixels for most cars, because the bars sweep from -1.5 to 2.5. So a black pixel pays about 24 × (4 array reads + 2 compares + ~1 dist) ≈ 8-10k cycles/px. The census's 48 dist/px is an upper bound; roughly 15-24 actually run. | FIX: use `renderBulk`: `clear()`, then for j from 23 down to 0, skip cars that are fully off-panel (`hi<0 \|\| lo>1`) and call `hsv(hueOf[j],1,1); drawLine(lo,lane,hi,lane,lineWidth,0)`, swapping x and y for vertical cars. The capsule's `(1-d/w)` falloff is exactly this pattern's math, and drawing in reverse with replace mode keeps first-hit-wins. Each capsule bounding box covers about 6 % of the panel, so expect ~20-40x on device (docs measured i-lines at 17x for 8 lines). | capsules belong in drawLine |
| 2 | `synchronized-random-numbers` | 8.45 | WHY: beforeRender, not render. Each frame draws one LCG value per pixel through `modMul`, a Russian-peasant loop of ~14 iterations, each making 1–2 `modAdd` user calls (2 `%` each) plus `floor(b/2)`: about 6k cycles/px on device (~110 ms/frame at 64x64). Host bench: 5.7 ms of the frame is beforeRender, 0.2 ms is everything else. The 128x128 FAIL is `array(pixelCount)` = 16384 elements, over the 10,236-element budget. | FIX: build two 128-entry tables once at init, `lo[k]=A·k mod M` and `hi[k]=A·128k mod M`, and step `s = lo[s&127] + hi[floor(s/128)] + C` minus M at most twice. This is exactly the same sequence, with no inner loop (host 5.7 → 0.4 ms, 14x). Also fold the scroll and `index/pixelCount` into `offsets` and read out with one `fillHSV(offsets,1,1)` in `renderBulk`. For 128x128, cap the walk buffer or use a canvas. Expected ~20x on device. | beforeRender per-pixel LCG loop |
| 3 | `dire-spider-2d` | 7.36 | WHY: render2D pays two big costs at every pixel. `perlinRidge(..., 3 octaves)` is about 2,300 cycles. The 8-iteration capsule loop costs about 2,800: 6 array reads, a clamp and about 12 mul/adds per bone. On top come 3 hypot plus an atan2 (about 760) and about 22 clamps. That is roughly 7k cycles/px, about 120 ms/frame on a 64x64 and 480 ms on a 128x128. Most pixels are nowhere near the spider and still run the whole loop. | FIX: Switch to renderBulk. Transform the 16 bone endpoints by heading/crawl yourself in beforeRender (bulk ops ignore the transform). Draw the legs as `drawLine(..., w, 2)` and the body as two `splat`s plus a pedicel `drawLine`. Compute the polar haze (ridge, rays, halo, centred on the spider) on a 32x32 canvas in beforeRender, convert it to HSV and paint it with one `fillCanvas`, then draw the legs over it with mode 1/2. Expect about 5x on a 64x64 and more than 10x on a 128x128. If it stays per-pixel, a minimum fix is an `r*r > 0.45` reject around the legs and body plus 2-octave ridge haze, about 2x. | noise haze plus capsule loop |
| 4 | `crawling-spider-2d` | 5.3356 | WHY: every pixel of the panel runs the render2D 8-iteration point-to-segment loop. Each iteration does 6 array reads, a `clamp` and ~10 mul/adds, so ~2.5k cycles/px before 2 `hypot` and 10 `clamp` for the body. The spider's legs and body cover well under a quarter of the panel, and every pixel outside them still pays the full cost. | FIX: `renderBulk`: `clear()`. In beforeRender, rotate and translate the 16 bone endpoints and 2 lump centres into world coordinates with one per-frame cos/sin, and call `resetTransform()`, because an active transform forces the full scan. Then draw 16 `drawLine(ax,ay,bx,by,w,2)` capsules (hue per bone, femur vs tibia), plus `fillCircle`/`splat` for the abdomen and head and one thick short `drawLine` for the pedicel. Only the bounding-box cells are touched, natively: expect ~10-20x on device. Tune `w` because the falloff is linear 1-d/w, not the current clamp(3·(1-d²/w²)). | per-pixel segment loop → drawLine |
| 5 | `glittering-jewels` | 4.83 | WHY: 1D `render` loops `count` (6) jewels per pixel and recomputes everything per-frame inside it — `sin(p*PI)` bloom, `r*r`/`rh*rh` divides, a second `sin` + `pow(...,10)` glint — ~1 sin+div+pow chain per jewel per pixel, though only `d=abs(x-jpos[i])` depends on the pixel. | FIX: hoist bloom, 1/r², 1/rh², glint `sp` and hue into 5 small per-jewel arrays in `beforeRender` (pixel loop becomes d², 2 mul-saturate, compare; scratch copy measured 1.5x on host, ~3-4x on device where sin/div/pow dominate); further, `renderBulk` with `clear()` + `setPixel` over each jewel's ±rh index window only (most of the strip is black) → ~10x. | per-frame math inside pixel loop |
| 6 | `crosshair-pulse-2d` | 4.62 | WHY: render2D runs up to 4 events per pixel, and about 2 are active at any time. Each active event costs 6 `saturate` + 6 `abs` + 1 `dist` + about 10 array reads of ex/ey/age at every pixel. That is ~1000 cycles per event per pixel, about 2.5k/px in total, yet the row and column bars cover only ~30 % of the panel and the flash only a disc of radius 1/6. | FIX: compute fade, r and flash=saturate(1-age*5) per event in beforeRender. In render2D, skip an event unless `abs(y-ey)<1/12 \|\| abs(x-ex)<1/12 \|\| (flash>0 && dx,dy within 1/6)`; this keeps the output exact and gives ~2-3x. For ~10x, use `renderBulk`: `clear()`, then per event 4 short `drawLine` bars at (ex±r, ey) and (ex, ey±r) in mode 2 plus `splat(ex,ey,1/6,2)`. The renderBulk version is approximate, because the hsv(h,1-v/2,v²) colour mapping is nonlinear. | early-out events by band |
| 7 | `ryb-colors` | 4.49 | WHY: every pixel's colour depends only on its angle, yet each pixel runs `atan2`, 4 `mod`, 3 `hue2chan` + 3 `ease` user calls, and 3 `rybBlend` calls (24 multiplies + 8 adds each): ~3k cycles/px of interpreted math to produce one hue→RGB mapping. | FIX: tabulate the hue→(R,G,B) RYB conversion into three 256-entry arrays in a `rebuild()` called from the slider handlers. Drop the rotate transform for `atan2(y-.5,x-.5)/PI2 - time(.15)`, then do 3 array reads + `rgb`. A host scratch run measured 16x (4.5 → 0.28 ms); expect ~4x on device (atan2 + floor remain). The HSV-mode path can stay as is. | per-pixel hue math, LUT it |
| 8 | `butterfly-2d` | 4.17 | WHY: render2D does everything at every pixel. That is 3 `ell()` calls (2 divides + sqrt + call each, about 1,350 cycles), sqrt + atan2 for the wing polar frame, 2 `lobe()` calls (divide + exp + call), sin, 2 `dist()` + 2 divides for the eyespots, and 2 divides for uu/vv, about 4k cycles/px (about 70 ms/frame on a 64x64). A bounding box won't help, because the open wings span the full panel width and about 80 % of its height. | FIX: Skip work by region. Skip the body ellipses when `u > 0.06` (ba is already 0 there). Test the eyespots as squared distance < 0.021² (no dist, no divides). Hoist `1/open` and `1/(1+0.14(1-open))` to beforeRender and pass the reciprocals of the lobe widths. Skip atan2, exp and sin when `rr² > 0.4²` (outside every wing). Expect about 2x. A 32x32 HSV canvas plus `fillCanvas` (rotation and lift applied per cell) gives about 4x on a 64x64 and 16x on a 128x128, at the cost of 2x2 blocks. | per-pixel SDF stack, no early-outs |
| 9 | `rgbclock-2d` | 3.2042 | WHY: render2D makes 4 `handVal` user calls per pixel, each with `pow(g, sharpEff)` at a fractional, per-frame exponent (4×511 ≈ 2,050 cycles), plus ~6 `triangle`/`clamp` and 60 cycles of call overhead each. On top of that, `atan2`+`sqrt` (~350) recompute a polar position that never changes, because the clock centre is fixed. Roughly 4k cycles/px in all. | FIX: build a 256-entry `powLut[k]=pow(k/255,sharpEff)` in beforeRender (256 pows/frame instead of 4/px) and read it per hand. Inline `handVal` and hoist the `radMode`/`handMode` branches out of the pixel path. On S3, precompute angle and unzoomed radius per pixel at init. The budget-safe form is a 32x32 table, with `zoom` applied as a multiply. Expect ~2-2.5x on device. | fractional pow per hand |
| 10 | `kaleidoscope-2d` | 2.907 | WHY: every pixel pays `hypot`+`atan2`+`cos`+`sin` for the fold and TWO domain-warped `simplex3` (2×586) plus 3 blob reciprocals and `smoothstep`/`hsv` — ~2,500 cycles/px, ~60 % of it the noise and the polar round-trip. | FIX: the source texture is only sampled inside one mirrored fan, so evaluate n1/n2/glo once per frame on a small polar canvas (e.g. 16 angle × 32 radius = 512 cells, vs 4096 px) in `beforeRender`, and per pixel do only atan2/hypot (or a 32x32 angle/radius table) + `canvasGet` + quantise/hsv; ~2-3x at 64x64, ~5x+ at 128x128. | two simplex3 per pixel |
| 11 | `coronal-ejection-2d` | 2.83 | WHY: render2D calls `perlinTurbulence` with `detail`=3 octaves at every pixel, ≈2300 cycles. It also calls `hypot` + `atan2`, which are constant per pixel because the centre is fixed, plus `smoothstep`, 4 `saturate` and 6 divides, for ≈3.5k cycles/px in total. The 4-prominence loop is cheap because the `u>1` test exits early. | FIX: build a polar (r, ang) table once at init as a 32×32 canvas, which fits the array budget, so hypot and atan2 are never computed per pixel. Each frame, evaluate the turbulence field n on a 32×32 canvas in beforeRender (1024 evals instead of 4096/16384) and sample it per pixel with bilinear `canvasGet`, or, if blockiness is acceptable, compose v/hue/sat on the canvas and call one `fillCanvas`. Precompute 1/eWid and 1/(0.35*ce) as reciprocals. Expect ~3-4x at 64x64 and ~10x at 128x128. | coarse polar noise canvas |
| 12 | `80s-kid-show` | 2.71 | WHY: the render2D loop over `numActive` (default 6, not the census's 18) objects. The bbox reject barely rejects: r averages ~0.3 × cutoff 1.6, so the boxes span ~the whole panel. Most pixels therefore run rotate + `shapeDist` (user call, abs/min/max/clamp, one `hypot`) for several objects: ~2–3.5k cycles/px real, against the census's 15k. | FIX: switch to `renderBulk`. `clear()`, then paint objects back-to-front (first-hit order reversed). Circles use `fillCircle`. Square/triangle/hexagon are convex polygons: compute each grid row's x-span from the rotated vertices and `fillRect` one row strip. The hexagram is the union of two triangles. Outline mode uses `drawLine` per edge with w=lineW. Keep render2D only for the non-flat glow toggle. About 6 objects × 64 rows of spans replaces 4096 SDF loops: ~10x+ on device. | per-pixel SDF entity loop |
| 13 | `coronal-mass-ejection` | 2.70 | WHY: `perlinTurbulence(..., 3 octaves)` runs at every pixel (about 2,300 cycles, about 60 % of the pixel's cost). Add per-pixel hypot + atan2 (about 370) for a centre that never moves, divides (`/PI2`, `/(0.35*ce)`) and the 4-slot eruption loop. That is about 3.8k cycles/px, about 65 ms/frame on a 64x64. | FIX: Compute the whole value on a 32x32 canvas in beforeRender: 1,024 turbulence calls instead of 4,096. Store each cell's (a, r) once at init in two 32x32 arrays (2,048 elements, budget-safe) to kill the per-frame hypot/atan2. Paint with one `fillCanvas(hue, sat, v, 32, 32)`. Expect about 4x on a 64x64 and about 16x on a 128x128. Smaller wins if it stays per-pixel: `*(1/PI2)` instead of the divide, and octaves 3 to 2 (-774). | polar turbulence at every pixel |
| 14 | `millipede-1d-2d-controls` | 2.5123 | WHY: about 55 % of the panel (the body arc, `bodyFrac`) runs `drawBody`. Per pixel that is 2 fractional `pow` (`pow(sin,0.55)` and `pow(abs(sin),2.5)` ≈ 1,020 cycles), 3 `sin`, `wave`, ~15 `saturate`, 6 divides, `hsv` and 2 user calls, on top of `atan2`+`hypot`+`sin`+`mod` for every pixel. Yet seg, plate, legs, headMix, taper, halfW, stripe and the hue all depend only on t01 (position along the body), not on the lateral offset. | FIX: tabulate the along-body profile in beforeRender at ~128 t01 samples: plate, legs, halfW, reach, stripe, taper, hue, headMix. Per pixel that leaves only the lat-dependent `saturate`s plus table reads. Replace `/0.12`, `/0.025`, `/bodyFrac`, `/PI2`, `/HALFW` with reciprocal multiplies, and inline the user functions. On S3, a per-pixel polar table removes `atan2`/`hypot` too (the centre is fixed). Expect ~2.5-3x on device. | along-body profile lookup table |
| 15 | `all-lasers-fire` | 2.5029 | WHY: three `laser()` channels per pixel, each `tan` (382) of a pixel-dependent angle, two divides `x/s`,`y/s`, a `hypot` and two more divides — 3 tan + 4 hypot + ~12 div ≈ 3,300 cycles/px; nothing is hoistable because `d*chaos` varies per pixel. | FIX: compute `tan` once (red channel) and get g/b via the tan-addition identity with per-frame `tan(δ)` constants (the channel offsets are frame-constant), use one reciprocal `is=1/s` per channel (x*is, y*is), and cube `0.088/(d*hypot)` via one reciprocal — 1 tan + ~5 div instead of 3 tan + 12 div; ~1.8-2x on device (field is too fine for a low-res canvas). | three tan channels per pixel |
| 16 | `radar-2d` | 2.50 | WHY: render2D pays for `hypot` + `atan2` + `mod` + `pow(…,6)` per pixel, all depending only on the pixel and the fixed centre except for beamA. It also calls 5 `dist()` per pixel for the blip test. Inside a blip it recomputes `atan2(by-0.5,bx-0.5)` + `pow(1-bb,3)`, which are constant per blip per frame. That totals ≈2.5-3k cycles/px; the census's 6 atan2 overcounts because the blip atan2 runs only inside a blip. | FIX: hoist each blip's afterglow `pow(1-bb,3)*1.3` into beforeRender (blipG[i]). Replace the 5 dists with a squared-distance test (`dx*dx+dy*dy<0.0081`) and take sqrt only inside a blip. Read r and the pixel angle from an init-time table (a 32×32 canvas sampled with canvasGet, which is budget-safe) instead of hypot and atan2. Write `pow(x,6)` as x2=x*x; x2*x2*x2. Expect ~2.5-3x on device. | hoist blip glow, LUT angle |
| 17 | `dbzbattlefinal` | 2.41 | WHY: every pixel scans all 8 ring slots (mostly dead, ringAge 1000), then `hypot`s both fighters and checks both projectiles before reaching black. Most pixels are background yet still pay ~2 hypot + ~16 array reads + the render2D floor, ~1.7k cycles/px (the census's 12 hypot assumes every ring is live). | FIX: switch to `renderBulk`: `clear()`, then 2 fighters as `splat(fx,fy,aura,0)` with a `fillCircle` core, projectiles as `fillCircle`, and each live ring as a ~24-segment polygon of `drawLine(..., RING_BAND, 2)` capsules with brush `rgb(b,b,b)` drawn last. Capsule falloff `1-d/w` is exactly the band term. Cost then scales with shape area, not pixel count: ~5–8x on device. If a minimal change is wanted: compact live rings in beforeRender and compare squared distances against aura² before any sqrt (~2x). | sprites tested at every pixel |
| 18 | `perlin-fire-wind` | 2.34 | WHY: Inside the flame column (about 55 % of pixels) `flameAt` runs a 4-octave `perlinTurbulence` (about 3,100 cycles) plus two fractional `pow` (env 0.75 and fuel 0.85, 511 each). Every pixel also pays `lean()`'s `pow(hb,1.4)` (511), the `emberAt` `perlin` (774) and 2 or 3 divides. The pows for lean, fuel and the 1/narrow divide depend only on the row. Roughly 4.5k cycles/px, about 75 ms/frame on a 64x64. | FIX: Build per-row tables in beforeRender (one entry per panel row) for lean, axis, 1/narrow, fuel, the bed term and the ember row envelope; that removes 2 pows and 2 divides per pixel. Replace the env `pow(s,0.75)` with a polynomial such as `s*(1.5-0.5*s)`. Better: render heat on a 32x32 canvas and draw it with one `paintCanvas` (palette-native, like aurora-2d #373), plus embers as a few dozen `splat` particles in renderBulk. Expect about 1.4x from the tables, about 4x on a 64x64 and about 16x on a 128x128 from the canvas. | 4-octave noise plus row pows |
| 19 | `voronoi-2d` | 2.3288 | WHY: render2D's nearest-seed loop runs `count` iterations (5 by default) per pixel. Each one evaluates a `metric` branch, 2 array reads and a `hypot` (193), so ~5×400 ≈ 2k cycles/px, with an `h = bi / count` divide on top. The loop count and costs are correct as the census reported them. | FIX: compare squared distances in the loop and take one `sqrt` of the winner afterwards for metric 0 (5 `hypot` become 1 `sqrt`). Hoist the `metric` branch outside the loop (4 specialised loops) and precompute `invCount` in beforeRender. A canvas+`fillCanvas` would make the cell edges blocky, so keep the per-pixel form. Expect ~2x on device. | squared-distance nearest seed |
| 20 | `beat-bounce` | 2.2625 | WHY: cheap 1D math, but per pixel it calls `mixColors(...,c)` (builtin writing a 3-array then 3 array reads) plus 3 divides by frame constants (`d/width`, `d/0.3`, `d/0.12`) — ~1,000 cycles/px; on a 4096-px grid that is still ~17 ms for one moving blob, and everything beyond 0.3 of `pos` is black. | FIX: precompute `1/width` in `beforeRender`, multiply by constants, inline the two-colour lerp (no array out-param); better, `renderBulk` with `clear()` and `setPixel` only over the ±0.3 window around `pos` (≤60 % of pixels, ≤25 % when the halo is off) → ~3x. | blob drawn over whole strip |
| 21 | `blue-holiday-candle-2d` | 2.25 | WHY: render2D calls 3 `sin` per pixel: the curl `sin(cy*5+…)` and two `lick` sines in fy, which depend only on the row because fy=cy/heightF. It also calls 1 `sqrt`, ~8 `clamp`, `triangle`/`frac`, 6 divides and `rgb`, ≈2.5k cycles/px. Dark pixels additionally loop over the 3 star indices. | FIX: in beforeRender, fill 64-entry per-row tables (gridHeight) for hFrac, lean, curl offset, fy, upness and rOut. That removes 3 sin and most of the clamps per pixel. Divide by constants through reciprocals. Before the sqrt, test whether the pixel is outside the flame's bounding radius (`fx²+fy² > (rOut+0.1)²`) and not on the body, and if so go straight to the background/star path. Expect ~2-2.5x. | per-row LUT the sines |
| 22 | `eye-of-sauron-with-movement` | 2.24 | WHY: two per-pixel `perlinRidge` calls, 3 + 2 octaves = 5 × 774 ≈ 3.9k cycles/px, about 70% of a ~5.6k cycles/px pixel (~95 ms/frame at 64x64 on device). `atan2` + 2 `hypot` + 5 divides make up the rest. The host barely shows it: zeroing the noise only halves host time. | FIX: the noise lives in polar (angle-about-gaze, radius) space, so evaluate both ridge fields once per frame into small polar canvases (e.g. 48 angle × 16 radius, ~0.8k cells). Per pixel, sample them bilinearly with the `atan2`/`hypot` already computed. Hoist `1/slitW`, `1/dilation`, `1/PI2` to beforeRender. About 2x at 64x64 and ~5x at 128x128, since the canvas cost is fixed. Dropping `lick` to 1 octave is a cheap extra 774 cycles/px. | per-pixel ridge noise ×5 octaves |
| 23 | `breathing-gradient` | 2.23 | WHY: Every pixel calls `mixColors` (an OKLab round trip with cube roots, very expensive with no FPU). The census's 751 cycles does not count it. On the host it alone is about 750 ns/px. Yet the colour depends only on `index/(pixelCount-1)`: it is a 1D gradient, so a 64x64 panel recomputes the same smooth ramp 4,096 times a frame. | FIX: Use renderBulk with K=64 to 128 bands: per band, one `mixColors` at the band centre, `rgb(c*breath)`, then `fillRange(i0, i1)`. That is 128 OKLab blends per frame instead of 4,096, with no visible banding on a smooth ramp. Measured on the host (scratch-2/bg-bulk.js): 3.06 to 0.11 ms/frame (about 27x). Expect more than 20x on device. | per-pixel OKLab of 1D ramp |
| 24 | `eye-of-sauron` | 2.224 | WHY: render2D calls `perlinRidge` twice per pixel at 3 and 2 octaves, i.e. 5 perlin evaluations ≈ 3,870 cycles. `atan2` and 2 `hypot` add ~560 more. That is ~5k cycles/px, ~85 ms/frame at 4096 px on device (host noise is cheap, so the host 2.2 ms understates it ~5x relative to the other entries). | FIX: compute the flame `n` (both ridged fields, ring fade, pupil slit and corona multiply) on a 32x32 canvas in beforeRender (64x64 for the 128 panel), then one `paintCanvas(vArr,32,32)` in `renderBulk`. That is 4x fewer noise calls at 64x64 and 16x at 128x128, and keeps `setPerlinWrap` seamless (`fillNoise*` is simplex with no wrap). Alternatively keep per-pixel and drop the `lick` field to 1 octave. Expect ~3-4x on device. | ridged noise per pixel |
| 25 | `mandelbrot-2d` | 2.2093 | WHY: census's unresolved loop is the Julia `while (i < cap)` — default cap 14, and interior (non-escaping) pixels run all 14 iterations of ~6 mul/add + compare each, so ~2,000+ cycles/px in the bright-fringe frames; inherently iterative, no builtin to hoist. | FIX: iterate on a 32x32 canvas in `beforeRender` (1024 points) and output with one `fillCanvas(hArr, 1, vArr, 32, 32)` (native 2x block-expand at 64x64, 4x at 128x128) → ~4x / ~16x, at the cost of fractal edge resolution; alternatively lower the default cap or `setFrameRate(30)`. | iterative escape loop per pixel |
| 26 | `bubble-column` | 2.20 | WHY: The host cost is mostly beforeRender, not render. It runs a pixelCount × NUM_BUBBLES (10) interpreted loop with abs/compare/break every frame (≈40k iterations at 4096 px), although each bubble touches only 6 pixels. render adds one `perlin` (774) + `hsv` per pixel. This is a 1D strip pattern, and `array(pixelCount)` fails the 16384-px budget (check128=FAIL). | FIX: zero `buf` with `arrayScale(buf,0)`, then loop over bubbles and add c⁴ only over each bubble's ±RADIUS window, skipping pixels where `buf[p] > FLUID`. That is exactly the original break semantics and ~60 iterations instead of 40k. Sample the hue wobble perlin into a 256-entry table in beforeRender and read it per pixel (12 periods over the strip, so 256 samples are smooth). Expect ≥20x on the beforeRender cost and ~3x per pixel on device. | per-bubble window deposit |
| 27 | `blue-holiday-star-2d` | 2.19 | WHY: each `minkowski()` is 3 fractional `pow` (511 each) plus a `1/p` divide, and it is called twice per pixel. Add 2 integer `pow`s: ≈3.3k of ~4.4k cycles/px is `pow`. | FIX: algebra, no look change. Star 1 has fixed p=0.5, so `b1 = min(K1/(sqrt|ax|+sqrt|ay|),1)^8` with `K1=sqrt(.11+tw1)` per frame: 2 sqrt + 1 div + `x*x` chain. Star 2 becomes `b2 = min(K2/(|qx|^p+|qy|^p),1)^(4/p)` with `K2=.11^p2` and `4/p2` hoisted to beforeRender: 3 frac pows → 2 + 1, and both per-pixel divides by p are gone. ~1.7x on device. A further step: the rotation is imperceptibly slow (one half-turn in ~8 min), so freezing it makes star 1 separable into 64-entry column/row `sqrt` tables. | fractional pow per pixel |
| 28 | `perlin-fire` | 2.14 | WHY: Same flame model as perlin-fire-wind, without the lean pow. In-column pixels (about 55 %) pay 4-octave `perlinTurbulence` (about 3,100 cycles) + `pow(...,0.75)` + `pow(...,0.85)` + a divide. Every pixel pays `perlin` for embers (774) + 2 divides. About 4k cycles/px, about 68 ms/frame on a 64x64; the fuel pow and narrow divide depend only on the row. | FIX: Same as perlin-fire-wind, applied once to both siblings. Build per-row tables (fuel, 1/narrow, axis, bed and ember row envelopes) in beforeRender, about 1.3x. Better: compute heat on a 32x32 canvas, draw it with one `paintCanvas`, and add embers as `splat` sparks in renderBulk, about 4x on a 64x64 and 16x on a 128x128. | 4-octave noise flame per pixel |
| 29 | `distance-function-kaleidoscope-2` | 2.1194 | WHY: render2D does `hypot`, `atan2`, `cos`, `sin` and a `perlinRidge` per pixel (~1,500 cycles), plus 2 per-pixel smoothing arrays. Two findings the census does not show. First, the noise's y/z arguments are per-frame constants, so the ridge sample is a 1-D function of `sym` alone. Second, the two `array(pixelCount)` buffers (`hMem`/`bMem`) are why check128 FAILs: 16384 elements exceeds the 10,236-element budget at init. | FIX: in beforeRender, tabulate `perlinRidge(s*4+0.5, slow2+1.3, slow3+2.7, …)*28-0.05` over sym ∈ [0,1.5] at 256 entries (256 noise calls/frame instead of 1/px), and read it per pixel. Size the memory as a fixed 64x64 canvas indexed by `floor(x*64)`/`floor(y*64)` instead of pixelCount, which fixes the 128x128 FAIL. Optionally use a 64-entry cos/sin table for the fold. Expect ~1.5-2x on device, plus the 128 panel working. | 1-D noise LUT, budget |
| 30 | `carrie-s-holiday-star-2d` | 2.0764 | WHY: two generalized p-norms per pixel = 6 fractional `pow` (6×511 ≈ 3,070 cycles) plus 2 divides; the sweep census is right that pow is the whole cost. | FIX: each p-norm is r·g(θ) with g(θ)=(\|cosθ\|^p+\|sinθ\|^p)^(1/p), separable in polar form, so build two 256-entry angle LUTs per frame in `beforeRender` (p=0.55 fixed, p2 breathing; 768 pows/frame instead of 24,576) and per pixel do `atan2`+`hypot` (or a 32x32 radius/angle table) + 2 LUT reads offset by the rotation + 2 divides; ~4-5x on device. | six fractional pow per pixel |
| 31 | `sunrise-alarm-clock` | 2.05 | WHY: render calls 4 `pow` with fractional exponents per pixel: the vignette `pow(sin,vigPow)` plus 3 gamma pows, at 511 cycles each ≈ 2k. It adds a `perlin` (774), a `sin` and 6 `mix`, ≈3.5k cycles/px. Everything except the few-pixel sun span is a smooth function of pos alone. This is a 1D strip pattern that only looks expensive at 4096 px; strip300 is 0.15 ms. | FIX: in beforeRender, evaluate the full sky+cloud+vignette+gamma colour at 128 samples along pos (the cloud perlin has 10 periods per strip) into 3 small arrays. render then interpolates from those arrays and adds the sun coverage (pre-gamma'd sun colour × env) only where `cover>0`, or uses a `renderBulk` with `fillGradient`-style reads plus a `fillRange` for the sun. Expect ~6-8x on device. | 128-sample sky table |
| 32 | `coral-plasma` | 2.01 | WHY: one 5-octave `perlinRidge` per pixel = ~3.9k of ~4.7k cycles/px (~80 ms/frame on device; the host hides it). With lacunarity 2, zoom up to 3 and gain 1.3, octaves 4–5 (24–48 cycles/unit, amplitude 2.2–2.9× octave 1) sit at or above the 64-px Nyquist limit. The dominant octaves therefore render as aliasing. | FIX: cut to 3 octaves, retuning gain (needs a look review): ~1.6x. The big win is computing the field on a 32x32 canvas in beforeRender (h/s/v arrays) and painting with one `fillCanvas`: 1024 instead of 4096/16384 noise calls, ~5x at 64x64 and ~15x at 128x128. Nearest sampling gives 2x2 blocks at 64 px, so check the look. | 5-octave ridge noise per pixel |
| 33 | `novas` | 1.92 | WHY: It does not run on a 64x64 at all. Four `array(pixelCount)` (16,384 elements) exceed the 10,236 budget, so init fails with "array element budget exceeded"; that is the check FAIL. Where it does run (S3), all the cost is in beforeRender's interpreted loops. Per generator there is a span loop with `sin()` per pixel; blooms widen to more than 2 strips, so they soon cover all 4,096 px. A per-generator 4,096-px tint/max-merge loop plus a final 4,096-px gamma loop add about 3x4,096 iterations of about 6 array ops each. The blooms also run along the wire index, not the geometry, so on a panel they are horizontal stripes. | FIX: Switch to renderBulk with no pixel arrays. Each frame `clear()`, then for each of the 12 or fewer live blooms set the tinted brush and `splat(x, y, r, 2)` (max = brightest-wins). Add a smaller white `splat` for the hot core, with 2D random positions on a matrix. For strips, keep the 1D index path but band it with `fillRange`. Fixes the budget FAIL. Per-frame work drops to bounding-box-limited native splats: more than 10x on the host and far more on device. | pixelCount arrays exceed budget |
| 34 | `sun-rays-through-trees` | 1.8961 | WHY: render2D calls `perlinTurbulence` at 3 octaves (≈2,320 cycles) per pixel. It also recomputes `atan2`+`hypot` (~370) and a `wave(wave())` rays term (~290), even though the sun never moves, so all three depend only on the pixel. ~4k cycles/px on device; the host figure understates the noise share. | FIX: in beforeRender, sample the turbulence `n` (or directly `v`) into a 32x32 canvas, which is 4x fewer turbulence calls at 64x64 and 16x at 128x128 (the field is low-frequency, so nearest/2x2 is invisible after the smoothstep). Read it per pixel by array index. At init, precompute per pixel `d`, `turn` and the `rays` term. On S3 use a per-pixel table; elsewhere a 32x32 table, rebuilt when a ray-count slider moves. Expect ~3x at 64x64 and ~5x at 128x128 on device. | turbulence per pixel, polar |
| 35 | `2d-clock-with-hand-color-pickers` | 1.8748 | WHY: per pixel `atan2`+`hypot`, then up to three `handOn()` calls each doing `pow(v, sharpness)` with a fractional exponent (sharpBase ≈ 33.4 → 511 cycles) plus `triangle` and user-call overhead — ~1,500 of ~3,600 cycles/px is the three pows. | FIX: `pow(v,s) > 0.5` ⇔ `v > 0.5^(1/s)` for v>0, so compute the threshold once in `beforeRender` and compare `(strength - triangle(a-h))*radial > thr` inline (no pow, no call); optionally a 32x32 angle/radius table for atan2/hypot; ~2-2.5x on device. | pow threshold test per pixel |
| 36 | `1d-aurora-borealis` | 1.86 | WHY: render loops over nWaves (5 by default, up to 10). For each wave covering the pixel it computes `sqrt(d/hw)` plus `triangle(sqrt(age/life))` and a divide, even though the age envelope is constant per wave per frame. That is 2 sqrt + 2 div per covered wave per pixel, ≈2-4k cycles/px at 4096. This is a 1D strip pattern (0.14 ms at 300 px); the cost only shows on a 4096-px strip mapping. | FIX: hoist `a0[i]=opac*triangle(sqrt(age/life))` and `invHw[i]=1/hw` into beforeRender, which halves the sqrts and drops the divides. Better still, since blobs are ≥10 % of the strip wide, composite r/g/b at 256 samples in beforeRender and have render lerp from those tables. Expect ~2x from the hoist alone and ~5x with the table. | hoist envelope, sample strip |
| 37 | `orv-christmas-tree` | 1.85 | WHY: the census's unresolved loop is `starLight`. Every sky pixel (~35% of the panel) loops all `numStars` (default 12) stars with 2 array reads + mul/compare each, ~1.8k cycles. Every other pixel pays a 16x16 canvas read (2 floor + mul), a `% ornSpacing` and a branch ladder: ~1.5k cycles/px average, dominated by the floor + star loop. | FIX: the scene is static except twinkle + stars, so switch to `renderBulk`. Draw: sky `fill`; stars as `splat(...,mode 2)`; tree as one `fillRect` span per grid row (plus the two garland bands and the topper); ground `fillRect`. Lit twinkle cells (v>THRESH) get per-row `fillRect`s clipped to the tree span; ornaments come from a `setPixel` stride loop over `i += ornSpacing` with a `pixelCoord` tree test. ~4–6x on device. Minimal alternative: bucket stars by canvas row in beforeRender and test only rows within `ceil(starCells)` (~1.5x). | per-pixel star loop, static scene |
| 38 | `2d-sinc-theta-theta` | 1.82 | WHY: Already hoisted (#938). What is left is 3× `ripple()` per pixel: hypot 193 + cos 165 + divide 107 + clamp + `pow(v, gammaExp)`. `gammaExp` is a runtime variable, so it may miss the integer-pow fast path and cost 511. Add 3 call overheads, about 2.5k to 3.5k cycles/px, about 45 to 60 ms/frame on a 64x64. Each channel is a pure radial function of its own centre distance. | FIX: Tabulate each channel's radial profile `f(d)` in beforeRender as a d²-indexed LUT (3×256 to 512 entries: cos, /d, clamp and pow done once per entry). Per pixel do `dx*dx+dy*dy` + scale + array read per channel: no sqrt, cos, divide or pow. Expect about 3x on device. Give the LUT finer steps near d²=0, or clamp the hot core. | 3 hypot+cos+pow per pixel |
| 39 | `utility-palettes` | 1.8213 | WHY: the census missed both real costs. Each pixel makes an indirect closure call `modes[mode](t,pos)`, which then runs 1-4 `palGet`s, each a linear `while` stop search (≤8 iterations of `floor`+array read+compare, ≈100 cycles each) plus `frac` and a divide. `grade` does 3. So ~1.5-2.5k cycles/px for what is a 1-D function of `t`. | FIX: rebuild 256-entry h/s/v LUTs keyed on `t` only when `mode` changes (~once/s), plus a `p`-keyed brightness LUT for `fireMode`/`mintPinkMode`. Render becomes `floor` + 3 array reads + `hsv`. A scratch LUT variant measured 5.5x on host at 64x64 (1.47 → 0.27 ms/frame); expect ~3-4x on device. | palette stop search per pixel |
| 40 | `wichmannhill-prng` | 1.7313 | WHY: on every redraw frame (default 60 Hz = every frame) `render` makes TWO `wichmannHill()` user calls per pixel, each 3 `%`, 3 `floor(x/c)`, 3 `seed/modulus` divides, `frac`, `min`/`max` and counter bookkeeping — ~18 div + 6 floor ≈ 2,900 cycles/px; check128 FAIL is the two `array(pixelCount)` (32,768 elements at 16384 px) blowing the array budget. | FIX: one draw per pixel (derive value from `frac(v*K)` of the same draw), the three `/30269`-style divides as multiplies by precomputed reciprocals, and on held frames skip `render` entirely: `renderBulk` + `fillHSV(hArr, 1, vArr)` (or redraw a 32x32 canvas and `fillCanvas`, which also fixes the 128x128 budget failure); ~2x at 60 Hz, near-free on held frames. | PRNG draws per pixel |
| 41 | `ripples-2d` | 1.73 | WHY: render2D computes 3 `dist()` per pixel (≈700 cycles), plus 3 saturate/abs, `min`, `hsv` and the 410-cycle floor, ≈1.5k cycles/px. A ring is nonzero only in a band of width 2/9 around radius ph*0.7, yet every pixel pays the sqrt for every drop. | FIX: in beforeRender, precompute per drop the band limits on squared distance, (R-1/9)² and (R+1/9)², plus the (1-ph) weight. In render2D, compute dx²+dy² and take sqrt only when it falls inside the band; per-row dy² can come from a small table. This skips ~60 % of the sqrts. The gain is modest, ~1.5-2x, because the pattern is already near the engine floor. | squared-distance band test |
| 42 | `time-flies-2d` | 1.72 | WHY: render2D tests all 6 flies at every pixel (2 array reads, 2 abs, add, compare). The Manhattan pre-test rejects almost all of them, so `hypot` is rare: real ~1.4k cycles/px (the census's 3.9k counts 6 hypot). Nearly all of it is the render2D floor plus the loop, painting a black frame with 6 tiny dots. | FIX: switch to `renderBulk`: `clear()`, then flies in reverse order (so the first fly wins), each as 2–3 concentric `fillCircle`s stepping down the cubic falloff, or one `splat(fx,fy,flyR,0)` if a linear falloff is acceptable. Six sub-0.07-radius shapes per frame replace 4096 loop bodies: ~20x+ on device. The 1D path is already a no-op. | sprite loop over black frame |
| 43 | `bouncing-balls-rgb-2d` | 1.69 | WHY: render2D loops over 6 balls per pixel. Each iteration pays loop, array and abs overhead (about 120 cycles × 6), and inside the 0.36×0.36 box (about 13 % of the panel per ball) hypot + divide + 3 array reads. About 1.6k cycles/px, about 27 ms/frame on a 64x64. The census's 3,469 counts every hypot as taken. Classic N-sprites-tested-at-every-pixel. | FIX: renderBulk. `clear()`, then per ball `rgb(ballR, ballG, ballB)` + `splat(bx, by, RADIUS, 1)` (mode 1 adds overlaps toward white, as now). splat's linear falloff vs the `1-q³` core can be approximated with a second, smaller splat. Measured on the host (scratch-2/bb-bulk.js): 1.87 to 0.065 ms/frame (about 29x). Expect more than 10x on device, since only about 6×530 bounding-box cells are touched natively. | N sprites tested per pixel |
| 44 | `portal` | 1.6912 | WHY: two costs dominate, and check128 FAILs for a third reason. (1) `render` → `gradient()` linearly scans 11 stops (up to ~44 array reads with `i*4` index math), then 3 `mix`, a divide and 3 squares per pixel, for a gradient that is STATIC. (2) beforeRender hides a pixelCount loop the census missed: each live pulse writes `sin(PI*s)` over up to 1.2×pixelCount, so ~2 pulses × ~0.6-1.2×N `sin` iterations ≈ 10 ms/frame on device at 4096 px. (3) The `array(pixelCount)` intensity buffer is 16384 elements at 128x128, which exceeds the 10,236 budget. | FIX: bake the squared-channel gradient into a 256-entry r/g/b LUT at init and read it per pixel. Drop `intensity`: in beforeRender keep ≤4 per-pulse params (c, halfW, 1/(2·halfW), env), and in `render` sum the ≤2 live humps directly (range test, then `sin` or a 64-entry half-sine table). That removes the pixelCount loop and fixes the 128 FAIL. Expect ~3x on device. | static gradient rescanned per pixel |
| 45 | `metaballs-of-fire-2d` | 1.6837 | WHY: `render2D` loops `numPoints` (5 at default) with `dist()` (234, a sqrt each) plus array reads and `min` per point — ~1,750 of ~2,350 cycles/px — to build field = Π min(1, spread·dᵢ). | FIX: square the field — field² = Π min(1, spread²·dᵢ²) needs only dx²+dy² per point (no sqrt), test `field2 >= CUTOFF²` for the black background, and take one `sqrt` only for lit pixels; precompute spread² in `beforeRender`; ~3x on device (or 32x32 canvas + `fillCanvas` for ~4x more). | sqrt dist per point per pixel |
| 46 | `geometry-morphing-demo-2d` | 1.65 | WHY: The census is wrong here. It inlined all 6 SDFs (12 sqrt), but `shapeSDF` dispatches at runtime to one SDF (two during the morph half of the time), so the real per-pixel work is 1-2 `sqrt`, 2 user-call overheads per SDF, ~10 abs/min/max/clamp and 2-3 divides, ≈1.2-1.9k cycles/px. That is near the floor already, and on the host it is cheap per pixel too. | FIX: choose the SDF in beforeRender (a function variable, or the per-shape branch moved outside), replace `d/lineWidth` and `abs(d)/(size*2)` with precomputed reciprocals, and early-out to black past the circumradius (size*1.2+lineWidth)² using a squared radius before any SDF work. Expect only ~1.3-1.5x. A 32×32 canvas + `fillCanvas` would be ~4-16x but makes the contour edges blocky, which is a poor trade for an SDF demo. | near floor, small wins |
| 47 | `oasis` | 1.61 | WHY: render() runs a 4-train loop per pixel, and each train does `mod(index, wlPx)`, an add, `*wEff`, a **divide by REF**, `frac`, `floor` and a LUT read: 4 mods + 4 divides + 4 floors ≈ 1.4k cycles/px on device. It is a cheap 1D look paying general-divide prices. | FIX: per frame, precompute `k[i]=wEff[i]/REF` and `ph[i]=phOffPx[i]*k[i]`, and unroll the constant 4-train loop. Per pixel this leaves `t=frac(index*k[i]+ph[i])` with no `mod` and no divide. `index*k` ≤ ~2.4k at 16384 px, so it stays in 16.16 range and the #197 overflow does not return. Also replace `sum/NUM` with `*0.25`. ~2x on device. | per-pixel mod+divide wave sum |
| 48 | `spiral-twirls-star-2d` | 1.58 | WHY: Already fairly cheap on device: about 1.2k cycles/px, about 21 ms/frame on a 64x64. Every pixel runs the hexagram `starSD` (2 folds, clamp, hypot 193, call 60). Pixels inside the star (about 30 %) also pay hypot + atan2 + `/PI2` divide + floor + hsv. The host number is inflated by the call and fold arithmetic, not by an expensive builtin. | FIX: Reject early with `x*x+y*y > (starR*1.16+edge)²` (outside the hexagram's circumradius, about 70 % of pixels) and go straight to black, skipping starSD. Accept inside the inscribed radius without starSD. Reuse r² for the later `hypot` and `*(1/PI2)` instead of the divide. Expect about 1.5 to 2x. A low-priority target. | star SDF everywhere; already cheap |
| 49 | `emoji-animation-2` | 1.5694 | WHY: every pixel pays the iris `hypot` (whenever not blinking) and the ellipse `sqrt`+`pow(…,6)` before any bounds test, although the eyes cover ~20 % of the panel. Background pixels then run `bgColor`, a user call with a long compare chain, 3 array writes and 3 reads, plus 2 `floor`/`clamp` pairs. ~2.5k cycles/px. | FIX: rebuild a 16x16 h/s/v background canvas only on `bgFrame` flips. In `renderBulk`, draw it with `fillCanvas(bgH,bgS,bgV,16,16)` (native 4x block-expand at 64x64), then draw the eyes over only their bounding boxes: an interpreted `setPixel(gridIndex(c,r))` loop for the rim and `fillCircle`/`splat` for the iris. Minimal in-render form: test the ellipse bbox (|px|>ax or |py|>ay) first, and read the background from the 16x16 tables. Expect ~3x on device. | per-pixel eye tests, bitmap |
| 50 | `scary-pumpkin` | 1.516 | WHY: body pixels (~70 % of the square) pay `perlinFbm` with 3 octaves (3×774 ≈ 2,300 cycles) after a tunnel warp of `atan2`+divide+`cos`+`sin`, and every inside pixel runs `inSmile`'s 2 `hypot`s — ~3,500 cycles/px, two-thirds of it the fbm. | FIX: evaluate the warped fire texture on a 32x32 canvas in `beforeRender` (1024 fbm calls vs ~2,900) and sample it per pixel with `canvasGet` (face mask stays crisp per pixel), or drop to 2 octaves of `simplex3` (2×586); make `inSmile` squared-distance compares and the outer `r>pumpkinR` test r²; ~3x at 64x64, ~8x at 128x128. | fbm noise under warp |

## 3. Patterns that fail the 64x64 array budget (10,236 elements) — and how to fit them

**25 patterns fail `luxel check --grid 64x64`; every one of them is a 1-D strip pattern
holding three or more `array(pixelCount)` buffers** (3 × 4,096 = 12,288 > 10,236). All 25
pass at 300 px. None fails for any other reason, and none raised a runtime error on the host
with the arena-sized ledger. (128x128: 78 fail, i.e. these 25 plus 53 that hold a single
`array(pixelCount)` — 16,384 alone exceeds the budget. Informational: the S3 panel boards
lift it with the PSRAM arena, every classic board keeps it.)

**The platform-level fix first.** On a matrix these strip patterns run "by index" over all
4,096 pixels by default. With the 1-D-along-x projection (docs/spec/projection.md)
`pixelCount` is 64 and every one of them fits with room to spare — and looks better (a
strip effect along 4,096 serpentine pixels reads as noise on a panel). Worth proposing: a
per-pattern projection hint (e.g. `//# projection=x`) so the library can ship 1-D patterns
that default to it on a matrix. That alone clears this whole section.

Pattern-side recipes, in order of preference:

- **R1 — the frame is the buffer.** `renderBulk` + `fade(k)` + `splat(x, 0.5, r, 1)` /
  `setPixel` / `fillRange` deposits: the persistent frame replaces the RGB (or H/S/V) trail
  buffers outright. Costs zero array elements. Limitation: one decay rate for all channels
  (per-channel decay — e.g. red lingering longer than blue — needs a buffer).
- **R2 — drop a channel.** A saturation buffer that is almost always 1, or a hue that is a
  function of position/time, becomes a scalar argument to `fillHSV` or is recomputed.
- **R3 — cap the buffer.** `L = min(pixelCount, 1024)`; simulate on `L` cells and read
  out with `floor(index * L / pixelCount)` (a strip effect does not need 4,096 cells of
  state). Three `array(1024)` = 3,072 elements.
- **R4 — tables become functions or small LUTs.** Precomputed per-pixel tables that are a
  function of `index / pixelCount` become a 256-entry LUT or an inline expression.

| pattern | 64x64 ms | arrays at pixelCount | fit |
|---|--:|---|---|
| `novas` | 1.92 | tmp + rBuf/gBuf/bBuf | R1: two pulse generators deposit into the frame with additive `splat` + `fade`; drop `tmp` and the trio (or R3) |
| `fireblobs` | 1.28 | 3 RGB channel buffers | R1: additive `splat` blobs into a faded frame |
| `flash-posterize-music-sequencer-framework` | 1.17 | shHue/shSat/shVal (N+1) | R3: cap the shared scratch at 1024 and sample by index scale; shSat is near-constant (R2) |
| `aurorashivers` | 0.94 | pulseA, pulseB, glow | R1 for pulses (additive deposits + `fade`); keep one `glow` buffer → 4,096 fits |
| `4th` | 0.85 | trail + sparkR/G/B | R1 for the comet trail; sparks as additive `splat` into the faded frame; per-pixel `random()` twinkle stays in the loop |
| `neutronorbit` | 0.84 | trailA/B/C | R1: three comets drawn with `splat`/`setPixel` into a `fade`d frame — the three trails differ only in hue |
| `color-bands-buffered` | 0.81 | hueB, satB, briB | R2 (sat scalar) → 2 × 4,096 = 8,192 fits; or R3. It is a buffered-idiom demo; consider deleting (coolness) |
| `lightning-strike` | 0.74 | chan, hot, glow | R2: `hot` and `glow` are decays of `chan` → keep `chan` + one combined intensity buffer; or R1 with `fade` for the afterglow |
| `xmasflies` | 0.71 | energy0..3 (4) | R1: each fly is a moving `splat` in its colour on a `fade`d frame; no per-pixel energy needed |
| `audio-volume-meter` | 0.57 | sec5, frac5, sec3, frac3, prevV | R4: section/fraction are pure functions of index — compute inline; keep `prevV` only |
| `bustle` | 0.55 | buf0..buf3 | R1 (one fade rate) or R3 |
| `music-sequencer-for-v2` | 0.55 | hueA/satA/valA (N+1) | R3 as for the posterize framework (the two sequencers share the design) |
| `heatshivers` | 0.52 | pulseA, pulseB, heat, chR, chG | R2: `chR`/`chG` are derived from `heat` at readout — fold into `fillRGB` args computed per frame into one buffer; R3 caps the rest |
| `music-sequencer-for-v3-only` | 0.44 | hueA/satA/valA (N+1) | R3 (it already has `renderBulk` + `fillHSV`, so a capped scratch + a scaled readout is the only change) |
| `sound-starburst-2` | 0.37 | pix, fade, sat, age | `age` is written but unused (delete); `fade` coefficients → per-ray scalar; leaves pix + sat = 8,192 |
| `rainbow-comet` | 0.32 | bri, hue, sat | R1: hue is position/time-derived (R2), comet = `setPixel` head + `fade` |
| `chasing-rainbows-hsluv` | 0.27 | rT, gT, bT (HSLuv LUT per pixel) | R4: a 256-entry RGB LUT indexed by `floor(f*256)` (768 elements) — the table is a function of `f` only |
| `fairies` | 0.25 | briB, satB (+ 3 × total sparks) | R1: sparks as `setPixel`/`splat` into a `fade`d frame; ceiling today 2,771 px |
| `cellular-automata-1d` | 0.19 | cellsA, cellsB, hueAge | R3 (cap the automaton at 1024 cells, each cell spans k px), or better: on a matrix make it the 2-D waterfall (history rows) via `blit`/scroll — the natural 2-D form |
| `meteor-shower` | 0.19 | hBuf, sBuf, vBuf | R1: meteors are moving heads with decaying tails → `setPixel` + `fade` |
| `christmaspewpew` | 0.14 | trailR, trailG, ambR (constant) | R2: `ambR` is a constant underglow → scalar/`fillRange`; R1 for trails |
| `rocket-by-tony-hampton` | 0.06 | rBuf/gBuf/bBuf | R1 |
| `pew-pew-pew` | 0.05 | bufR/G/B + ambR/ambG (constant) | R2 (ambient arrays → a `fillGradient`/`fill` underlay) + R1 |
| `fireworks-finale` | 0.03 | rBuf/gBuf/bBuf | R1 (shells and sparks as additive `splat` into a `fade`d frame) |
| `bouncing-balls-rgb` | 0.03 | rBuf/gBuf/bBuf | R1 |

Note `bulk-render.md` "Converting a pattern" already records most of these as
"budget-refused at 4096 px before and after" — the 2026-09 conversions kept the trio and
only moved the readout to `fillRGB`. R1 is the step they did not take: it changes the
decay arithmetic (one shared `fade` rate, frame-quantized), so it is not byte-exact — which
the overhaul's "better, not identical" principle now permits.

## 4. Device-heavy per pixel although host-cheap (outside the host top 50)

The second ranking — "est cyc/px" in the table — prices the static per-pixel census with the
S3 cycle costs from docs/boards.md "Builtins on metal" (perlin 774, simplex3 586, simplex2
314, fBm/ridge/turbulence 774 × octaves, sqrt 172, hypot 193, dist 234, sin/cos 165, wave
146, atan2 179, pow(frac) 511, log 404, x/y 107, hsv 111, array read ~30) plus the engine
floor (~410 cycles render2D, ~290 render). **It is an upper bound for multi-mode patterns**
(the census sums every branch; a 12-mode dispatcher runs one) and ignores early-outs. At
16,384 px every 1,000 cycles/px is ~68 ms of `vm_us` per frame on the S3 — a 15 fps budget
is ~1,000 cycles/px total, engine floor included.

Host and device disagree here because the wasm host has an FPU and cheap transcendental
math, while on Xtensa the interpreted bookkeeping is only part of the cost and each
noise/trig/sqrt call is 150–2,300 cycles. These rank 51–308 on the host but sit in the top
60 by estimated device cost:

| pattern | host rank / 64x64 ms | est cyc/px | what is per pixel | cheaper design |
|---|--:|--:|---|---|
| `perlin-fire-wind-tunnel` | 59 / 1.29 | ~9,200 (one mode: ~3–4k) | `hypot` + `sin` + `cos` + `sin` + `perlin`/`perlinFbm(3 oct)`/`perlinRidge(3)`/`perlinTurbulence(3)` | measured 172 ms at 16,384 px on metal. The tunnel transform (`density/hypot`, rotation by angle) is static per pixel when the centre does not move → a 32×32 polar-UV table; the noise then on a coarse lattice via `fillNoise3D` rows (simplex instead of 3-octave perlin) and `fillCanvas` — ~4–6x |
| `tixy` | 60 / 1.29 | ~7,600 (formula-dependent) | evaluates the tixy formula (`evalFormula`, up to ~20 trig) **at every pixel although the output is a 16×16 grid** | evaluate the 256 cells once per frame into a 16×16 V/H canvas and `fillCanvas` — 16x fewer formula evaluations at 4096 px, 64x at 16,384 |
| `stargen-polar-2d` | 68 / 1.12 | ~14,000 summed / ~1,200–2,000 per mode | 12-mode dispatcher on polar coords; trig per pixel in most modes; per-pixel `random(1)` crossfade | per-mode: hoist time terms; the crossfade's per-pixel `random` + double dispatch doubles the cost during fades — fade via a per-frame mask instead |
| `multisegment-demo` | 105 / 0.81 | ~7,700 summed | 15-effect zone dispatcher; `while` zone search per pixel, `time()` calls inside render | union overcount — real cost is one effect; hoist the `time(...)`/`triangle(time(...))` terms per zone into beforeRender arrays; zone lookup is already incremental |
| `rock-sparks` | 131 / 0.67 | ~6,300 summed | 3-mode dispatcher, per-pixel `cos` of a per-frame ease and per-pixel `random(1)` | hoist `eased` (constant per frame) — `(1 - cos(prog*PI))/2` is computed 4,096× per frame |
| `infinity-flower-2d` | 139 / 0.63 | ~3,800 | `hypot` + `atan2` + `sin` per pixel and a petal loop with `mod` | static centre → polar table (32×32 r/θ via `canvasGet`, or 2 × array(1024) on a coarse lattice); petal test by `floor(θ·petals)` instead of a loop |
| `zoom-kaleidoscope` | 53 / 1.40 | ~3,750 | `atan2` + `perlinRidge(3 oct)` per pixel + two per-pixel persistence arrays | the ridge noise depends only on `f` (1-D) → a 256-entry LUT per frame; `atan2` → static table; persistence arrays are the 128x128 budget failure |
| `frogger-2d` | 52 / 1.50 | ~3,700 | lane loop with per-lane car tests per pixel | `renderBulk`: lanes as `fillRect` bands, cars as `fillRect`, frog as `fillRect` — a sprite scene, ~10x |
| `animated-asterisks-2d` | 77 / 1.00 | ~3,300 | loop over N line segments per pixel with `hypot` end caps | `renderBulk` + `clear()` + one `drawLine(…, w, 2)` per spoke — exactly the capsule op |
| `soap-2d` | 76 / 1.01 | ~2,300 | 3 × `simplex3` per pixel (2 domain-warp fields + 1 sample) | the warp fields are low-frequency → `fillNoise3D` on a 16×16 canvas (2 fields), sample warped noise on a 32×32 canvas, `fillCanvas` — ~6–10x |
| `palette-fire-2d` | 110 / 0.79 | ~2,900 | `perlinFbm(3 oct)` + `time()` per pixel | hoist `time(.03)*8`; noise as `fillNoise3D` rows (simplex, 2 octaves by two fills + `arrayAdd`) on a 32-wide lattice; `paintCanvas` — ~5x |
| `lightning-clouds` | 62 / 1.26 | ~2,700 | 2 × `perlin` (two octaves, hand-rolled) + `dist` per pixel | two `fillNoise3D` canvases (32×32) + `arrayAdd`; flash glow as one additive `splat` over the cloud canvas readout |
| `line-dancer-2d` | 120 / 0.73 | ~2,400 | 2 × `hypot`, `atan2`, 4 × `sin`/`cos`, `wave` | static polar table for `r`, `atan2`; `cos(ang)`/`sin(ang)` share an angle → one rotation; ~2x |
| `wavy-bands`, `halloween-wavy-bands` | 86, 80 / 0.90, 0.96 | ~2,100 | `perlin` + `sin` + `pow(v, 1.4)` per pixel | `pow(v,1.4)` → `v*v` (integer exponent is 119 vs 511 cycles) or a LUT; perlin warp on a 16×16 canvas via `canvasGet`; dedupe the two (same pattern, palette differs) |
| `matrix-2-tone-pulse`, `matrix-2d-pulse-edit`, `sunset` | 118, 136, 112 | ~1,800–2,000 | `sin(u…)` + `cos(v…)` per pixel where u/v are column/row terms | separable: `sin` depends only on the column, `cos` only on the row → two 64-entry tables per frame (4096 → 128 trig calls) |
| `interference-2d` | 66 / 1.14 | ~2,000 | 3 × `dist` + 3 × `wave` + `oklch` per pixel | `oklch` per pixel is the hidden cost — palette LUT of 256 OKLCH colours via `setPalette` + `paint`; source distances via squared distance into a `wave` LUT is not possible (needs sqrt) — keep `dist`, ~1.5x |
| `glitch-bands` | 137 / 0.64 | ~1,800 | `pow(…, 2)`, `pow(…, 4)` per pixel | `x*x`, `(x*x)*(x*x)` — integer powers already have the fast path, but the multiply is 10 cycles vs 119 |
| `chasing-rainbows-hsluv`, `custom-sequences` | 244, 95 | ~3,600 summed | multi-mode; `pow(frac, 1/e)` per pixel, tables per pixel | `pow` with a per-frame exponent → 64-entry LUT of the ease curve per frame |
| `m5stack-hex-panels` | 85 / 0.92 | ~3,000 summed | `atan2` + `hypot` per pixel in every mode (computed before the mode switch) | static map → precompute angle/radius once (3D fixture is fixed); move into the modes that use them |
| `bouncer3d` | 72 / 1.05 | ~2,400 | ball loop with box rejection per pixel | `renderBulk` + `splat` per ball (Manhattan falloff is cosmetic) |

**Cross-cutting findings from this ranking**

- **Static geometry recomputed every frame** is the single most common device cost: `hypot`
  + `atan2` of a fixed centre (polar patterns, kaleidoscopes, tunnels, clocks, radar) costs
  ~370 cycles/px every frame for a value that never changes. A coarse polar table
  (32×32 = 1,024 elements, budget-safe) sampled with `canvasGet`, or an engine-provided
  polar map / `pixelPolar(i)` builtin, would remove it library-wide — a candidate engine
  feature (goal 8).
- **Separable trig** (`sin(f(x)) + cos(g(y))`) — plasma-style patterns call trig per pixel
  for terms that depend on one axis only: per-row/per-column tables cut 4,096 calls to 128.
- **Fractional `pow`** (511 cycles) for gamma/softness — `v*v`, `v*v*v` or a LUT.
- **Per-pixel `oklch`/`oklab`** — use `setPalette` with OKLCH stops + `paint`, or
  `paintCanvas`.
- **Per-pixel `random()` crossfades** (stargen, rock-sparks): a dithered transition doubles
  the dispatch; a `mix` of two modes, or a hard switch, is cheaper.
