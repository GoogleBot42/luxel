# `renderFrame` — whole-frame rendering and the bulk ops

`renderFrame()` is a fourth render entry beside `render`/`render2D`/`render3D`.
It runs **once per frame**, not once per pixel, and comes with sixteen native
builtins (ids 166–181) that touch many pixels per call. This page is the
design in one place plus the measured evaluation of it.

Branch `agent/luxel/bulk-render`, merge base `c7e0266`. All throughput and
size numbers below were re-measured after the grid-coverage fix and the
image-size pass (`59fbd7e`); the ratio tables are one sweep each, so they are
internally comparable — do not compare them against numbers taken on another
day, since the absolute ns/px on this box moves by well over the ±3 % noise
band between sessions. All numbers are host measurements unless a section says
otherwise. Shipped examples of each shape live in `library/bulk-*.js`; the
language reference is docs/lang.md.

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
| grid | `blit`, `gridWidth`/`gridHeight` | a W×H grid that COVERS the frame (`grid.len() >= pixelCount`); cells past the end of the frame — the tail of the last row on an over-provisioned `ceil(√n)` map — clip. A silent no-op with no grid or one too small, and `gridWidth()` returns 0 in exactly those cases so a pattern can branch |

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
| k-empty | `render(index){}` vs `renderFrame(){}` | 9.68 | 0.004 | 39.6 | 0.02 | **2201×** |
| a-fill | constant `hsv` vs `fill()` | 15.75 | 0.21 | 64.5 | 0.86 | 75.1× |
| b-rainbow | per-pixel hue vs `fillGradient` | 20.74 | 4.76 | 84.9 | 19.5 | 4.4× |
| c-readout | `hsv(hues[i],1,vals[i])` vs `fillHSV(hues,1,vals)` | 19.15 | 4.16 | 78.4 | 17.0 | 4.6× |
| d-comet | array trail vs `fade`+`setPixel` | 17.27 | 0.67 | 70.7 | 2.8 | 25.7× |
| e-blocks | 3 block tests vs 3× `fillRange` | 36.85 | 0.28 | 150.9 | 1.1 | 133× |
| f-balls | 6-ball `hypot` loop vs `clear`+6 `splat` | 441.56 | 14.06 | 1808.6 | 57.6 | 31.4× |
| g-canvas | 32×32 canvas readout vs `fillCanvas` | 54.49 | 11.34 | 223.2 | 46.4 | 4.8× |
| h-sprite | 8×8 keyed sprite vs `blit(…, 3)` | 69.51 | 0.31 | 284.7 | 1.3 | 225× |
| i-lines | 8 segment distances vs 8 `drawLine` | 896.74 | 51.61 | 3673.0 | 211.4 | 17.4× |
| **j-perlin** | **dense procedural control** | 138.84 | 184.36 | 568.7 | 755.1 | **0.75×** |

### 300 px strip, no map (a typical WS2812 install)

The five 2D pairs get the `ceil(√300)` = **18×17 procedural default grid** on
both sides; the six index-space pairs are true mapless strips.

| pair | px ns/px | bulk ns/px | px µs/frame | bulk µs/frame | ratio |
|---|---:|---:|---:|---:|---:|
| k-empty | 9.79 | 0.05 | 2.9 | 0.015 | 196× |
| a-fill | 15.64 | 0.32 | 4.7 | 0.10 | 48.5× |
| b-rainbow | 22.12 | 4.98 | 6.6 | 1.5 | 4.4× |
| c-readout | 21.19 | 4.28 | 6.4 | 1.3 | 5.0× |
| d-comet | 17.37 | 0.95 | 5.2 | 0.28 | 18.3× |
| e-blocks | 37.35 | 0.80 | 11.2 | 0.24 | 46.9× |
| f-balls | 451.53 | 49.92 | 135.5 | 15.0 | 9.1× |
| g-canvas | 60.95 | 19.65 | 18.3 | 5.9 | 3.1× |
| h-sprite | 84.85 | 1.34 | 25.5 | 0.40 | 63.5× † |
| i-lines | 911.20 | 114.77 | 273.4 | 34.4 | 7.9× |
| **j-perlin** | 133.19 | 184.39 | 40.0 | 55.3 | **0.72×** |

† `blit` really runs on this rig now — it used to no-op on any strip whose
pixel count is not a rectangle, which is what the note below is about.

### 4096 px, no `--map-grid` (procedural 64×64 default grid)

`luxel bench` has no flag for `Engine::set_grid_map`, but a 2D pattern at
4096 px with no installed map gets exactly a procedural 64×64 default grid —
so this column is the procedural-map counterpart of the first table, same
geometry, coordinates computed instead of read from a 48 KB array.

| pair | px ns/px | bulk ns/px | ratio | vs coordinate map |
|---|---:|---:|---:|---|
| f-balls | 436.42 | 15.56 | 28.0× | px flat, bulk 1.11× slower |
| g-canvas | 56.54 | 17.15 | 3.3× | px flat, bulk 1.51× slower |
| h-sprite | 74.55 | 0.29 | 259× | px 1.07× slower, bulk flat |
| i-lines | 903.12 | 61.00 | 14.8× | px flat, bulk 1.18× slower |
| j-perlin | 135.86 | 179.68 | 0.76× | unchanged both sides |

The per-pixel side does not care which map it reads (measured directly:
`luxel bench` with and without `--map-grid 64x64` is inside 1 % on
`f-balls-px`). The bulk side does: a procedural map computes each
coordinate with a divide per axis where a coordinate map loads it, so
every coordinate-space op is a little slower on it — the *ratios* here are
the conservative ones.

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
| h-sprite | **0** | **0** ‡ | **0** | |
| k-empty | **0** | **0** | — | |
| d-comet | 7 | 7 | — | `fade` decays the **quantized RGB888** frame; the per-pixel version decays a 16.16 value and quantizes once. Visually the same trail, up to 7/255 apart mid-decay |
| f-balls | 1 | 1 | 1 | `splat` adds each ball as quantized bytes (saturating); the per-pixel version sums in 16.16 and quantizes once. Differs only where balls overlap (4.8 % of bytes, ≤1 LSB) |
| i-lines | 2 | 2 | 2 | same additive-quantization argument as `f-balls` (13.4 % of bytes, ≤2 LSB) |
| j-perlin | 1 | 1 | 1 | the bulk rewrite recomputes x/y as `mod(i,W)/(W-1)` in fixed point; `MapData::coord`'s `norm()` rounds, so the two differ by one 16.16 LSB on some cells |

‡ **Fixed.** The first cut of `blit_into` required `g.len() ==
frame.len()`, and the `ceil(√n)` default grid is over-provisioned whenever
`n` is not a rectangle (300 px → 18×17 = 306 cells, 60 px → 8×8 = 64), so
`blit` silently no-oped on every non-rectangular strip while `gridWidth()`
still reported the grid. The rule now is **cover, not match**: a grid-space
op runs whenever `grid.len() >= frame.len()`, and cells whose index is past
the end of the frame — the tail of the last row — clip like any other
off-grid cell. `gridWidth()`/`gridHeight()` return 0 exactly when the grid
cannot cover the frame or there is no grid, so the documented "`gridWidth()`
== 0 → no grid-space ops" predicate is now true rather than merely
plausible. The coordinate-space ops were never affected (they fall back to
the generic scan and stay correct — `f`, `g`, `i` match on this rig
throughout); the coordinate fast path keeps its stricter `g.len() == n`
precondition, since falling back to the scan there is correct by
construction. With the fix `h-sprite` is byte-identical on the 300 px strip
too, and `library/bulk-sprite-scroll-2d.js` shows its sprite on the 60/300/
512 px rigs instead of degrading to its background wash.

## Results — firmware

Devshell builds, same `creds.env` throughout, merge base `c7e0266` built from
a throwaway worktree. `espflash save-image` app image, `tools/image-check.sh`
for the margin. "first cut" is the branch at `c51e1db`, before the size pass.

| board | base `c7e0266` | first cut | after the size pass | Δ vs base | margin now | gate |
|---|---:|---:|---:|---:|---:|---|
| `board-pixelblaze-v3` | 1,006,640 | 1,021,520 | **1,014,976** | +8,336 | 33,600 B / 3.20 % | ok (base 3.99 %) |
| `board-athom-music` | 1,006,752 | 1,021,616 | **1,015,040** | +8,288 | 33,536 B / 3.19 % | ok (base 3.98 %) |
| `board-seengreat-hub75` | 941,600 | 956,192 | **949,744** | +8,144 | 98,832 B / 9.42 % | ok (base 10.20 %) |
| `board-c6-devkit` | 1,021,136 | 1,034,464 | **1,029,840** | +8,704 | 18,736 B / **1.78 %** | **FAIL** (base 2.61 % — already under the floor before this branch, #291) |

Nothing in `firmware/` changed; the whole delta is `luxel-core` (`bulk.rs`
plus the sixteen `call_builtin` arms and the `RunStage::Frame` path). The
first cut was **+14.6–14.9 KB**, which put `board-pixelblaze-v3` — the board
`tools/ci.sh` builds — at a 2.58 % margin, under `image-check.sh`'s 3 % floor,
so CI went red. `nm --size-sort` said where it was: 11,066 B in `bulk.rs`,
1,927 B in the firmware's one inlined copy of `Engine::from_program_budgeted`
(`uses_coordinate_bulk_op` alone carried eight inlined copies of
`lookup_builtin`'s table scan), 550 B in `Engine::frame`, 510 B in
`Vm::call_builtin`, ~550 B of rodata.

The size pass (`59fbd7e`) took **6,544 B** back on `board-pixelblaze-v3`
without changing a single output byte — the whole equivalence table below is
unchanged. What it bought, in order:

| change | board-pixelblaze-v3 |
|---|---:|
| `paint_shape` over `&mut dyn FnMut` (one copy, not four), shared `texel_hsv`/`texel_rgb`/`texel_at`, out-of-line `put`, static error strings | −3,520 |
| out-of-line `set_default_grid_map` / `uses_coordinate_bulk_op` / `calls_any_builtin` | −896 |
| out-of-line `GridView::span`, one `fillGradient` loop, `blit` row base walked not multiplied | −288 |
| one shared `srcs`, `splat` as a zero-length `drawLine`, `blit` + `fillCanvas` share one `paste` | −448 |
| `map_coord` out of line (`MapData::coord` was inlined twice into `span`), tighter builtin-id lookup | −688 |
| `lookup_builtin` `#[inline(never)]`, `span` out of line, one `fillGradient` loop | −240 |
| `blit`/`paste` clip arithmetic narrowed to i32 (every input is an Fx, so it is in −32768..=32767) | −480 |
| the two perf clawbacks below | +128 |
| `texel_at` by array, `clear`/`fill` share one write loop, `grid_dim` out of line | −112 |

The one thing it costs is host throughput on the two coordinate-space shape
pairs, measured by running the pre-pass and post-pass binaries back to back
(the per-pixel sides land within 0.5 %, so this is the bulk side alone):
`a-fill`, `c-readout`, `d-comet` and `g-canvas` are inside the ±3 % noise
band, while **`f-balls` is +13 % and `i-lines` +10 % per bulk pixel** — the
indirect call in `paint_shape` plus the out-of-line coordinate read. Their
ratios move 35.0× → 31.4× and 19.3× → 17.4×. Two clawbacks kept that from
being twice as large and cost 128 B: `splat` short-circuits the segment
projection it provably does not need, and `paint_shape`'s grid loop computes
each cell index once instead of twice.

**Stack** (`tools/stack-check.sh`, `board-pixelblaze-v3`): clean. No function
exceeds the 12,288 B budget; the largest frame is unchanged at 9,744 B
(picoserve's request future) and no `bulk`/`Vm` frame appears in the top
fifteen.

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
and they help enormously: 3–5× for buffer and canvas readouts, 9–31× for
entity loops, 19–225× for fills, trails and sprites, and the fixed entry cost
goes to zero. That is ~145 of 299 library patterns.

They do **not** help the ~110 dense-procedural patterns, and rewriting one is
an active regression (0.72–0.76× here, interpreted work 27 → 52 insns/px).
`renderFrame` is an additional entry point, not a replacement: the right rule
is *stay on `render` unless a bulk op replaces the body*.

The costs are real and should be weighed against that: **+8.1–8.7 KB of flash
on every board** after the size pass (down from +14.6–14.9 KB), which leaves
every board's OTA margin where the gate wants it except `board-c6-devkit`,
which was already under the floor before this branch (#291); and an unmeasured
I-cache-layout risk to every existing pattern that this evaluation could not
test without hardware.
