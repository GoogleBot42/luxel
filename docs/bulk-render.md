# `renderFrame` — whole-frame rendering and the bulk ops

`renderFrame()` is a fourth render entry beside `render`/`render2D`/`render3D`.
It runs **once per frame**, not once per pixel, and comes with sixteen native
builtins (ids 166–181) that touch many pixels per call. This page is the
design in one place plus the measured evaluation of it.

Branch `agent/luxel/bulk-render`, merge base `c7e0266`; measured at `89ed146`
(nothing under `crates/` or `firmware/` has moved since). All numbers below
are host measurements on this box unless a section says otherwise. Shipped
examples of each shape live in `library/bulk-*.js`; the language reference is
docs/lang.md.

## Why

The per-pixel entry into `render*` is a fixed tax that no interpreter tuning
can reach. On the Seengreat 64×64 panel an **empty** `render(index) {}` costs
5.4–7.6 ms/frame at 4096 px — **317–440 Xtensa cycles per pixel** before the
pattern's own body runs at all (docs/boards.md, "Second light"). That is 54 %
of a one-`rgb()` frame and 32 % of the default rainbow. #314's −18 % dispatch
win left it exactly where it was; only a whole-frame entry removes it.

A survey of the 299-pattern library (`luxel bench --profile` at 256 and
1024 px, fitting per-pixel against fixed cost) says how much of the library
that is worth:

| bucket | patterns | shape | bulk op |
|---|---:|---|---|
| persistent-buffer readout | 87 (29 %) | `hsv(hues[index], 1, vals[index])`, 5–30 insns/px | `fillHSV` / `fillRGB` |
| entity loops | 29 | 3–24 sprites, 112–658 insns/px — the most expensive bucket | `clear` + `splat` / `drawLine` |
| index-range fills | ~20 | block/stripe tests on `index` | `fillRange` |
| 2D canvas readout | 28 | `canvas[floor(y*H)*W + floor(x*W)]` | `fillCanvas` |
| constant fills | 9 | one `hsv()` for the whole strip | `fill` |
| **dense procedural** | **~110** | per-pixel noise/trig — every pixel is a different computation | **none; do not rewrite these** |

Rule of thumb for the saving on device:
`≈ 400 + insns_per_px × ~110 cycles/px`, minus 20–100 cycles/px for the
native op.

## The model

**Entry.** `renderFrame` is a fourth well-known name in `render_targets`
(`RenderKind::Frame`, `RunStage::Frame`). Present, it wins over
`render2D`/`render3D`/`render` regardless of map dimensions; late binding
(`export var renderFrame` holding a `Fun`) works like the others.
`beforeRender(delta)` is unchanged, and `post_chain()` + `finish_frame()`
still run after the call, so `setFrameRate`, `timeScale`, `setBlur`,
`setGlow` and the output palette all behave as before.

**Frame persistence.** The engine's `pixels` Vec is **not** cleared between
frames — `renderFrame` starts on last frame's final output. That is what
makes `fade(k)` + a few `setPixel`s a trail with **no pattern-side array**
(a 4096-element `Value` array is 32 KB and has OOMed the S3 panel: #275/#258).
A pattern that wants a fresh canvas calls `clear()`. Post-chain stages
compound frame over frame under persistence unless the pattern clears.

**Buffer plumbing.** `Engine::frame_buffer_out` `mem::take`s `pixels` into
`Vm::frame` and `frame_buffer_in` takes it back — a move, never a copy (12 KB
at 4096 px, every frame), paired on every exit path including pattern error
and debug pause. Outside `renderFrame` the Vec is empty, which makes every
bulk builtin a silent no-op; that is the entire guard, there is no mode flag.

**The brush.** `hsv()`/`rgb()`/`paint()`/`oklch()` keep writing `Vm::pixel`;
under `renderFrame` that slot is the current colour and the shape ops read it.
Every existing colour builtin therefore works here with no new plumbing. The
engine resets the brush to black at the top of each frame; within a frame it
is sticky.

**Three spaces.**

| space | ops | needs |
|---|---|---|
| index | `clear` `fill` `fade` `setPixel` `fillRange` `fillHSV` `fillRGB` `fillGradient`(axis 0) | nothing — works on a bare strip |
| coordinate | `fillRect` `fillCircle` `splat` `drawLine` `fillCanvas` `fillGradient`(axis 1–3) | any map, including sparse/irregular; a predicate over each pixel's mapped (x, y) exactly as `render2D` sees it |
| grid | `blit`, `gridWidth`/`gridHeight` | a real W×H grid; a silent no-op without one, and `gridWidth()` returns 0 so a pattern can branch |

**Fast path.** With a grid map installed and no transform active, the
coordinate ops walk only the cells in the shape's bounding box instead of
every pixel. It is bit-identical to the generic scan *by construction* — the
fast path narrows the candidate set and then evaluates the same predicate on
the same coordinate, so it can only skip pixels the predicate would have
rejected. A `FORCE_SCAN` test hook asserts the two paths equal over
randomised shape parameters; a transform routes to the scan.

**Modes.** `0` replace, `1` add (saturating), `2` max (lighten), `3` keyed
(black source transparent — `blit` only).

**Default map.** A pattern with no `render`/`render2D`/`render3D` but a
`renderFrame` that *calls a coordinate- or grid-space builtin* gets the same
`ceil(√n)` default grid a 2D-only pattern gets. `renderFrame` + `fillHSV` is
a strip pattern and is handed no geometry it never asked for.

## Results — host throughput

Eleven pairs written to be visually equivalent; the per-pixel side is the
control. `tools/pairbench.mjs`, 2000 frames, **best of 5** interleaved runs
(host `luxel bench` has 6–19 % run-to-run spread — .claude/rules/vm-bytecode.md).
The pairs are benchmark fixtures, not library patterns: each is written twice
against the same `beforeRender`, with the animation driven off accumulated
`delta` rather than `time()` so a frame dump is reproducible.

### 4096 px, `--map-grid 64x64` (coordinate map, grid fast path via `detect_grid`)

| pair | what | px ns/px | bulk ns/px | px µs/frame | bulk µs/frame | ratio |
|---|---|---:|---:|---:|---:|---:|
| k-empty | `render(index){}` vs `renderFrame(){}` | 13.35 | 0.01 | 54.7 | 0.03 | **1886×** |
| a-fill | constant `hsv` vs `fill()` | 26.87 | 0.29 | 110.1 | 1.2 | 92.9× |
| b-rainbow | per-pixel hue vs `fillGradient` | 36.38 | 6.30 | 149.0 | 25.8 | 5.8× |
| c-readout | `hsv(hues[i],1,vals[i])` vs `fillHSV(hues,1,vals)` | 33.56 | 4.76 | 137.5 | 19.5 | 7.1× |
| d-comet | array trail vs `fade`+`setPixel` | 29.74 | 1.05 | 121.8 | 4.3 | 28.4× |
| e-blocks | 3 block tests vs 3× `fillRange` | 62.01 | 0.40 | 254.0 | 1.6 | 155× |
| f-balls | 6-ball `hypot` loop vs `clear`+6 `splat` | 615.22 | 17.15 | 2519.9 | 70.2 | 35.9× |
| g-canvas | 32×32 canvas readout vs `fillCanvas` | 100.76 | 15.62 | 412.7 | 64.0 | 6.5× |
| h-sprite | 8×8 keyed sprite vs `blit(…, 3)` | 125.50 | 0.16 | 514.0 | 0.66 | 784× |
| i-lines | 8 segment distances vs 8 `drawLine` | 909.74 | 46.80 | 3726.3 | 191.7 | 19.4× |
| **j-perlin** | **dense procedural control** | 139.28 | 185.16 | 570.5 | 758.4 | **0.75×** |

### 300 px strip, no map (a typical WS2812 install)

The five 2D pairs get the `ceil(√300)` = **18×17 procedural default grid** on
both sides; the six index-space pairs are true mapless strips.

| pair | px ns/px | bulk ns/px | px µs/frame | bulk µs/frame | ratio |
|---|---:|---:|---:|---:|---:|
| k-empty | 8.90 | 0.05 | 2.7 | 0.015 | 179× |
| a-fill | 15.95 | 0.30 | 4.8 | 0.09 | 52.5× |
| b-rainbow | 21.47 | 4.74 | 6.4 | 1.4 | 4.5× |
| c-readout | 20.67 | 4.27 | 6.2 | 1.3 | 4.8× |
| d-comet | 18.35 | 0.96 | 5.5 | 0.29 | 19.1× |
| e-blocks | 37.88 | 0.78 | 11.4 | 0.23 | 48.7× |
| f-balls | 453.23 | 51.33 | 136.0 | 15.4 | 8.8× |
| g-canvas | 66.23 | 21.27 | 19.9 | 6.4 | 3.1× |
| h-sprite | 86.50 | 0.46 | 26.0 | 0.14 | 187× † |
| i-lines | 921.13 | 116.05 | 276.3 | 34.8 | 7.9× |
| **j-perlin** | 132.98 | 180.18 | 39.9 | 54.1 | **0.74×** |

† not a real number — `blit` no-ops on this rig, see the `blit` note below.

### 4096 px, no `--map-grid` (procedural 64×64 default grid)

`luxel bench` has no flag for `Engine::set_grid_map`, but a 2D pattern at
4096 px with no installed map gets exactly a procedural 64×64 default grid —
so this column is the procedural-map counterpart of the first table, same
geometry, coordinates computed instead of read from a 48 KB array.

| pair | px ns/px | bulk ns/px | ratio | vs coordinate map |
|---|---:|---:|---:|---|
| f-balls | 443.29 | 14.56 | 30.4× | px 1.39× faster, bulk 1.18× faster |
| g-canvas | 55.10 | 17.15 | 3.2× | px 1.83× faster, bulk 1.10× slower |
| h-sprite | 75.04 | 0.11 | 707× | `blit` active here (64×64 = 4096) |
| i-lines | 906.26 | 59.06 | 15.3× | px flat, bulk 1.26× *slower* |
| j-perlin | 134.37 | 179.94 | 0.75× | unchanged both sides |

The procedural map is cheaper for the per-pixel side (no array load per
coordinate) and roughly neutral for the bulk side, so the *ratios* shrink
slightly — the wins below are the conservative ones.

## Results — interpreted instructions per pixel

`luxel bench --profile --json` (the `profile` cargo feature), 4096 px,
64×64. This decomposes the win: what is removed is both the entry **and**
the body.

| pair | px insns/px | bulk insns/px |
|---|---:|---:|
| k-empty | 1.00 | 0.0002 |
| a-fill | 3.00 | 0.0012 |
| b-rainbow | 6.00 | 0.0029 |
| c-readout | 5.01 | 0.0066 |
| d-comet | 5.00 | 0.0046 |
| e-blocks | 13.00 | 0.0098 |
| f-balls | 116.13 | 0.074 |
| g-canvas | 19.06 | 0.059 |
| h-sprite | 31.48 | 0.0088 |
| i-lines | 337.89 | 0.191 |
| **j-perlin** | **27.00** | **52.00** |

Every beneficiary drops to ~0 interpreted instructions per pixel: the whole
per-pixel stream, entry and body, moves into native code, and what remains is
the handful of instructions `beforeRender` and the bulk call itself execute
once per frame. The dense-procedural control is the mirror image — rewriting
it as `renderFrame` *doubles* its interpreted work (27 → 52 insns/px), because
the loop bookkeeping and the index→(x, y) arithmetic that the engine did
natively are now bytecode.

## Results — visual equivalence

Both sides of every pair rendered 60 frames at a fixed 30 fps delta and seed
(`luxel run --out`), compared byte for byte. `maxdiff` is the largest
per-channel difference over 737,280 (4096 px) / 54,000 (300 px) bytes.

| pair | 4096 grid | 300 strip | 4096 procedural | why not identical |
|---|---:|---:|---:|---|
| a-fill | **0** | **0** | — | |
| b-rainbow | **0** | **0** | — | per-pixel side written as `index/(pixelCount-1)` to match `fillGradient`'s `t = i/(n-1)`; the library's `rainbow.js` uses `index/pixelCount` and would differ at the last pixel |
| c-readout | **0** | **0** | — | |
| e-blocks | **0** | **0** | — | per-pixel side uses the same integer boundaries the bulk side passes to `fillRange`; a `floor(index*3/pixelCount)` block test differs by one pixel at each seam |
| g-canvas | **0** | **0** | **0** | |
| h-sprite | **0** | 255 ‡ | **0** | |
| k-empty | **0** | **0** | — | |
| d-comet | 7 | 7 | — | `fade` decays the **quantized RGB888** frame; the per-pixel version decays a 16.16 value and quantizes once. Visually the same trail, up to 7/255 apart mid-decay |
| f-balls | 1 | 1 | 1 | `splat` adds each ball as quantized bytes (saturating); the per-pixel version sums in 16.16 and quantizes once. Differs only where balls overlap (4.8 % of bytes, ≤1 LSB) |
| i-lines | 2 | 2 | 2 | same additive-quantization argument as `f-balls` (13.4 % of bytes, ≤2 LSB) |
| j-perlin | 1 | 1 | 1 | the bulk rewrite recomputes x/y as `mod(i,W)/(W-1)` in fixed point; `MapData::coord`'s `norm()` rounds, so the two differ by one 16.16 LSB on some cells |

‡ **`blit` silently does nothing when the grid has more cells than the strip
has pixels.** `blit_into` requires `g.len() == frame.len()`, and the
`ceil(√n)` default grid is over-provisioned whenever `n` is not a rectangle:
at 300 px the grid is 18×17 = 306 cells, so `blit` no-ops while `gridWidth()`
still reports 18 and `gridHeight()` 17. The coordinate-space ops degrade
gracefully here (they fall back to the generic scan and stay correct — `f`,
`g`, `i` all match on this rig); only the grid-space op hard-fails. Worth a
ticket: either clip `blit` to the frame length instead of bailing, or make
`gridWidth`/`gridHeight` report 0 when the grid does not cover the frame, so
a pattern can branch on the same condition the op tests.

## Results — firmware

Devshell builds, same `creds.env` both sides, merge base `c7e0266` built from
a throwaway worktree. `espflash save-image` app image, `tools/image-check.sh`
for the margin.

| board | base `c7e0266` | branch | Δ | branch margin | gate |
|---|---:|---:|---:|---:|---|
| `board-pixelblaze-v3` | 1,006,640 | 1,021,520 | **+14,880** | 27,056 B / **2.58 %** | **FAIL** (base 3.99 %, ok) |
| `board-athom-music` | 1,006,752 | 1,021,616 | **+14,864** | 26,960 B / **2.57 %** | **FAIL** (base 3.98 %, warn) |
| `board-seengreat-hub75` | 941,600 | 956,192 | +14,592 | 92,384 B / 8.81 % | ok (base 10.20 %) |
| `board-c6-devkit` | 1,021,136 | 1,034,464 | +13,328 | 14,112 B / **1.34 %** | **FAIL** (base 2.61 %, already failing — #291) |

Nothing in `firmware/` changed; the whole +14.6–14.9 KB is `luxel-core`
(`bulk.rs` plus the sixteen `call_builtin` arms and the `RunStage::Frame`
path). The app still **fits** the 1 MiB slot everywhere, but
`image-check.sh`'s **3 % floor is breached on three of the four boards**, and
`board-pixelblaze-v3` is the board `tools/ci.sh` builds — so **CI goes red on
this branch as it stands.** The classic ESP32 boards had ~42 KB of margin
(3.99 %) and this spends 35 % of it in one feature. Either the diet in
docs/size-report.md comes first, or the bulk module needs a size pass (the
sixteen ops are ~930 B each as written), or the gate's default has to change
— which docs/boards.md says is a decision to record there, not a flag flip.

**Stack** (`tools/stack-check.sh`, `board-pixelblaze-v3`): clean. `.stack`
25,484 → **25,420 B** (−64 B, well over the 24 KB floor); no function exceeds
the 12,288 B budget; the largest frame is unchanged at 9,744 B (picoserve's
request future) and no `bulk`/`Vm` frame appears in the top fifteen. Function
count 1,319 → 1,338.

## How to judge this on device

Host numbers **understate the device win by roughly 3×**: an x86 per-pixel VM
entry is a few nanoseconds, the Xtensa one is 317–440 cycles. Read the ratios
above as a floor.

- **`tools/opbench.mjs` cannot see this change at all.** Its K-sweep loop
  fits the slope of a bytecode loop and cancels the per-pixel entry by
  construction — which is precisely what `renderFrame` removes. It will read
  flat. Run it only to confirm dispatch has not regressed.
- **`tools/patbench.mjs <pattern>` is the measurement**: push the per-pixel
  pattern, record median `vm_us/px`, push the `renderFrame` rewrite of the
  same pattern, record again. Use a **stateless** probe — `snake-2d` swings
  74 % between builds a kilobyte apart and is useless as an A/B; the pairs
  above are all deterministic given a fixed delta.
- **The I-cache caveat applies with force.** `Vm::call_builtin` grows by
  sixteen arms here, and #318/#325 showed that `Vm::run` and `call_builtin`
  competing for the flash instruction cache is worth *tens of percent*,
  non-monotonically in size. A pattern that does **not** use bulk ops can get
  slower on this branch purely from layout. Re-run `patbench.mjs` on
  `perlin-fire-wind-tunnel` (repeats to ±0.3 %) against master before
  concluding the branch is free for existing patterns.
- **`/api/status`'s `frame`/`vm`/`pipe`/`out` split** is where the win should
  land: `vm` collapses, `pipe`/`out` are untouched. On the HUB75 panel `out`
  is a flat 5.3–6.4 ms whatever runs, so 4096-px bulk patterns will hit that
  ~155 fps ceiling rather than a VM one.

## Scope — an honest statement

Bulk ops help patterns whose per-pixel body is a *lookup or a shape test*,
and they help enormously: 4.5–7× for buffer and canvas readouts, 8–36× for
entity loops, 19–186× for fills and trails, and the fixed entry cost goes to
zero. That is ~145 of 299 library patterns.

They do **not** help the ~110 dense-procedural patterns, and rewriting one is
an active regression (0.74–0.75× here, interpreted work 27 → 52 insns/px).
`renderFrame` is an additional entry point, not a replacement: the right rule
is *stay on `render` unless a bulk op replaces the body*.

The costs are real and should be weighed against that: **+14.6–14.9 KB of
flash on every board**, which breaks the OTA-margin gate on the three
classic-ESP32/C6 boards, and an unmeasured I-cache-layout risk to every
existing pattern that this evaluation could not test without hardware.
