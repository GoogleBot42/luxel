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

Re-confirmed after the rebase onto `974b3b3` (#328), same sweep parameters, on
pairs `a`/`c`/`f`/`i`/`j` at 4096 px 64×64: 15.93/0.22 (71.4×), 19.29/4.40
(4.39×), 401.21/14.04 (28.6×), 858.24/51.54 (16.7×), 128.43/174.24 (0.74×).
Every **bulk** ns/px is inside 6 % of the table below; what moved is the
per-pixel control side (`f-balls` −9 %, `i-lines` −4 %, `j-perlin` −8 %),
which is #328's x86 code placement plus this box's 6–19 % spread and is why
the ratios shift by up to a tenth. The rows are left as the one internally
comparable sweep they were taken as.

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

Devshell builds, same `creds.env` throughout, the base built from a throwaway
worktree at the commit this branch sits on. `espflash save-image` app image,
`tools/image-check.sh` for the margin.

Re-measured 2026-09-06 after the rebase onto **`974b3b3`** — the master that
carries #328's code placement (`Vm::run`, `Vm::call_builtin` and
`Vm::builtin_hot` moved into `.rwtext`, the other ninety builtin arms split
out into a `#[cold]` `Vm::builtin_cold`). The sixteen bulk arms are tier 3, in
`builtin_cold`, and every `bulk.rs` symbol links into flash `.text`, so this
branch adds **zero bytes to `.rwtext`**: 42,644 B on the classic ESP32 and
24,360 B on the S3, byte-identical to the base on both boards. The whole delta
is flash. Per-symbol on `board-pixelblaze-v3` (`nm --print-size`), the three
IRAM-resident functions are byte-identical to the base — `Vm::run` 12,700 B,
`Vm::call_builtin` 1,562 B, `Vm::builtin_hot` 3,568 B — and the entire arm
cost lands in `Vm::builtin_cold`, 14,801 → 15,093 B (**+292 B**, ~18 B per
arm). The other ~8 KB is `bulk.rs` itself, in `.text`.

| board | base `974b3b3` | this branch | Δ vs base | margin now | gate |
|---|---:|---:|---:|---:|---|
| `board-pixelblaze-v3` | 998,144 | **1,006,416** | +8,272 | 42,160 B / 4.02 % | ok (base 4.80 %) |
| `board-athom-music` | 998,048 | **1,006,304** | +8,256 | 42,272 B / 4.03 % | ok (base 4.81 %) |
| `board-seengreat-hub75` | 936,416 | **945,168** | +8,752 | 103,408 B / 9.86 % | ok (base 10.69 %) |
| `board-c6-devkit` | 1,015,696 | **1,024,544** | +8,848 | 24,032 B / **2.29 %** | **FAIL** (base 3.13 %) |

`tools/ci.sh` gates on `board-pixelblaze-v3`, which clears `image-check.sh`'s
3 % floor with a point to spare. **The C6 changed character in the rebase**:
#328 took enough out of that image to put it back *over* the floor
(2.61 % → 3.13 %), so the branch no longer lands on a board that was already
failing — it is now what puts `board-c6-devkit` under, at 2.29 %. That board
is not in CI and its margin is tracked by #291, but the honest statement is
"this branch costs the C6 its margin", not "the C6 was already under".

`.stack` is untouched: 24,868 B on `board-pixelblaze-v3` and 32,380 B on
`board-seengreat-hub75` against a base of 24,932 / 32,452 — the 64–72 B is
DRAM-layout rounding, not a new static — both over `tools/stack-check.sh`'s
24 KB floor, and no function on either board exceeds the 12,288 B frame
budget (largest is still picoserve's ~9.7 KB request future).

Nothing in `firmware/` changed; the whole delta is `luxel-core` (`bulk.rs`
plus the sixteen `builtin_cold` arms and the `RunStage::Frame` path).

### How the delta got from +14.6 KB to +8.3 KB

Measured against the pre-#328 merge base `c7e0266`, where the branch's first
cut (`c51e1db`) was **+14.6–14.9 KB** — enough to put `board-pixelblaze-v3` at
a 2.58 % margin and turn CI red. `nm --size-sort` said where it was: 11,066 B
in `bulk.rs`, 1,927 B in the firmware's one inlined copy of
`Engine::from_program_budgeted` (`uses_coordinate_bulk_op` alone carried eight
inlined copies of `lookup_builtin`'s table scan), 550 B in `Engine::frame`,
510 B in `Vm::call_builtin`, ~550 B of rodata.

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

## Results — on device (2026-09-07)

Gitea #336. Everything above this section is a host measurement; this one is
the two rigs. Three firmware builds went to both boards out of one worktree —
the merge base **`974b3b3`**, the #335 merge **`eedabc8`**, and the master of
the day **`ed2ac3f`** — with a single `web/public/luxel.wasm` compiling every
pattern, so the bytecode on the wire is byte-identical across builds and only
the firmware differs. Both S3 slots carried the same image before any
measurement (#294); `core1.last` was clean after all six pushes and no OTA
wedged. Full tables live in docs/boards.md, "Bulk render (`renderFrame`) on
metal".

**The I-cache risk did not materialise.** `tools/patbench.mjs` on
`perlin-fire-wind-tunnel` (three repeats) and `tools/opbench.mjs`:

| board | `974b3b3` | `eedabc8` (#335) | `ed2ac3f` |
|---|---:|---:|---:|
| Athom @ 256 px, µs/px | 58.039 | **57.973** (−0.11 %) | 58.121 (+0.14 %) |
| Athom, cycles/op | 100.9 | **100.7** | 100.8 |
| Seengreat @ 4096 px, µs/px | 44.360 | **44.382** (+0.05 %) | 44.413 (+0.12 %) |
| Seengreat, cycles/op | 83.4 | **83.4** | 83.4 |

Every delta is inside the probe's own ±0.3 % repeatability on both an Xtensa
board that pins `Vm::run`/`call_builtin`/`builtin_hot` in IRAM and one that
pins only `Vm::run`. The caveat this page carried — "an unmeasured
I-cache-layout risk to every existing pattern" — is discharged.

**The bulk patterns on the 64×64 panel**, `ed2ac3f`, 4096 px:

| pattern | fps | out_fps | vm µs | vm µs/px |
|---|---:|---:|---:|---:|
| `rainbow` (per-pixel control) | 52 | 52 | 19,417 | 4.740 |
| `bulk-rainbow` | 125 | 125 | **2,772** | **0.677** |
| `bulk-comet-trails` | 125 | 126 | **431** | **0.105** |
| `bulk-bouncing-balls-2d` | 125 | 125 | **5,477** | **1.337** |
| `bulk-sprite-scroll-2d` | 125 | 124 | **5,125** | **1.251** |
| `bulk-canvas-ripples-2d` | 100 | 100 | **9,900** | **2.417** |

`rainbow` → `bulk-rainbow` is the only genuine like-for-like pair among the
shipped library patterns: **7.0× of VM time**, 52 → 125 fps on the wire. The
host bench read 4.4× for the same shape (`b-rainbow`), so the device
multiplier here is **1.6×, not the ~3× this page's rule of thumb suggests** —
that estimate came from the per-pixel *entry* cost, and for a fill-shaped
pattern whose body was already trivial the entry is a smaller share of the
frame than it is for the entity loops. Take 1.5–2× as the measured multiplier
for fills and expect more only where the per-pixel body was expensive.

Two device-only facts the host bench cannot show:

- **Four of the five bulk patterns are frame-cap bound, not VM bound.** At
  2.8–5.5 ms of VM they sit on the engine's 125 fps ceiling with the HUB75
  compose at 3.2–3.6 ms. On this panel a bulk rewrite's payoff *stops* at the
  cap; past it you are buying headroom, not frames.
- **`rainbow-comet` will not load at 4096 px at all** — its `array(pixelCount)`
  trail buffer is refused by the pre-flight check ("pattern too large for this
  device — it left only 16 KB of heap free"), while `bulk-comet-trails` draws
  the same shape from six scalars in **431 µs**, the cheapest frame measured on
  the board. The frame-persistence argument in "The model" is not a
  micro-optimisation on this hardware; it is the difference between running and
  not running.

**`blit` keyed mode and `fillCanvas` are correct on metal**, asserted through
`GET /api/pixels` with fixed-position probe patterns rather than by eye. On the
panel a 4×2 keyed sprite over a green fill gives exactly 4 red pixels of 4096
in the right cells; an 8×4 sprite at `gridWidth() - 4` gives exactly 16, the
overhanging columns clipped; a 2×2 `fillCanvas` gives exactly 1024 pixels per
quadrant. On the Athom's 60-px strip — the over-provisioned 8×8 `ceil(√n)`
grid the ‡ note above is about — the same probes land at indices 0/2/8/10, an
8×1 sprite on grid row 7 paints 56–59 and clips 60–63 with no `vmerr`, and
`library/bulk-sprite-scroll-2d.js` shows its face over the wash. **"Cover, not
match" works on a real non-rectangular fixture.**

**No allocation per frame.** 40-minute soak on the Athom (20 min holding
`bulk-comet-trails`, 20 min rotating all five `bulk-*.js`): `heap_free` read
**83,616 B on 38 of the hold's 40 samples** — first and last included, the other
two 192 B lower with a response in flight — and each rotated pattern
returned to its own fixed value every time it came round. AppCpu stack
high-water 11,136 → 11,520 B of 20,480; no `vmerr`, no reboot. The
`mem::take`/give-back of the frame `Vec` costs nothing per frame, as designed.

**Image sizes re-measured on the merged tree** (the table under "Results —
firmware" was taken on the branch): +8,832 B on `board-pixelblaze-v3`, +8,816
on `board-athom-music`, +9,312 on `board-seengreat-hub75`, +9,232 on
`board-c6-devkit`. Margins at `ed2ac3f`: 4.01 % / 4.01 % / 9.64 % / **2.26 %**.
The C6 conclusion stands unchanged — it is the one board this change puts under
`image-check.sh`'s 3 % floor (#291).

## Converted library patterns

`renderFrame` is an entry point, not a migration: the rule in "Scope" below
still holds. These are the library patterns that have actually been moved onto
it, with the numbers each conversion was accepted on. Host `luxel bench`, best
of five interleaved runs; `--profile` for the instruction counts.

| pattern | shape | 4096 px (64x64) throughput | insns/px 4096 px | equivalence |
|---|---|---:|---:|---|
| `snake-2d.js` → `snake-2d-v2.js` (new file, both kept) | 16x16 board repainted on a board change, one `fillCanvas` | — | — | max per-channel diff **0** vs `snake-2d.js`, 240 frames at 256 and 4096 px, coordinate map and procedural grid |
| `raindrops-2d.js` (converted in place) | 16x16 water sim, per-cell shading, one `fillCanvas` | 6.90 → **46.98** Mpx/s (**6.8x**) | 60.6 → **6.4** | see below |

### `raindrops-2d.js` (2026-09-07)

The pool was always a 16x16 simulation and `render2D` only resolved a colour
out of it per LED. The colour resolution moved to the pool's own resolution —
one `shade()` pass over 256 cells into `hC`/`sC`/`vC`, one
`fillCanvas(hC, sC, vC, 16, 16)` per frame — with the static terms (the sea
floor's contribution and the two shimmer wave arguments minus their phase)
baked at init, and the pass skipped outright when nothing that feeds it moved.

| rig | before ns/px | after ns/px | before µs/frame | after µs/frame | ratio |
|---|---:|---:|---:|---:|---:|
| 4096 px, `--map-grid 64x64` | 145.0 | 21.3 | 593.8 | 87.2 | **6.8x** |
| 256 px, `--map-grid 16x16` | 247.9 | 221.3 | 63.5 | 56.6 | 1.12x |

Interpreted instructions: **60.6 → 6.4 insns/px** at 4096 px and 114.0 → 102.2
at 256 px. The two profile runs of the converted pattern report the *same*
7,847,813 instructions over 300 frames at both pixel counts — the per-pixel
tax is gone entirely and what is left is 26,159 interpreted instructions per
frame no matter how big the panel is. The 256 px row is what a conversion buys
when the display is already the size of the simulation: almost nothing.

A pool that has gone exactly flat now skips the recurrence *and* the repaint,
which the per-pixel version could not do: at Raindrops 0.3 / Shimmer 0 /
RippleFade 0.4 the converted pattern runs **2,279 instructions per frame**
against the busy frame's 26,159, and calls `wave()` on 25 of 300 frames.

**Visual equivalence**, 60 frames at a fixed 30 fps delta and seed
(`luxel run --out`), compared byte for byte against the pre-conversion file:

| rig | maxdiff | bytes differing |
|---|---:|---:|
| 256 px, 16x16 map | **1** | 1 / 46,080 (0.002 %) |
| 1024 px, 32x32 map | 10 | 39.1 % |
| 4096 px, 64x64 map | 16 | 41.6 % |
| 60 px strip (8x8 default grid) | 5 | 38.2 % |
| any of the four, with Texture = 0 | **0** | **0** |

Every non-zero delta is the **surface texture**, and inside it only the
shimmer: the sea floor was already indexed per cell, but the shimmer used to
be evaluated at each PIXEL's mapped coordinates, so on a panel larger than the
pool it carried full-panel detail riding over the blocky water. It is now a
per-cell field like everything else. That is not an inference — the original
with *only* the shimmer's coordinates quantized to the pool's cells
(`floor(x * 15.99) / 15`) is **byte-identical to the converted pattern on all
four rigs**, and driving Texture to 0 removes the term and the difference
together.

The one byte that still differs at 16x16 is in **column 15**: a map normalizes
its far edge to 65535/65536, not 1, while a cell's own `cx / (W - 1)` is
exactly 1, so the last column's shimmer arguments differ by one 16.16 LSB.
It moved a single green channel by 1/255 in one frame of sixty.

Dial agreement is unchanged: undriven vs every control driven at its declared
`default=` differs by maxdiff 1 over 0.21 % of bytes **both before and after**
(a pre-existing 1-LSB rounding of `198 / 360` against the literal `0.55`).

**Where the frame goes now** (4096 px, host, best-of-five ablation runs —
each part disabled in turn):

| part | µs/frame | insns/frame | share |
|---|---:|---:|---:|
| `fillCanvas` (+ the frame's fixed overhead) | 32.6 | 179 | 37 % |
| `rippleStep()` — the water recurrence | 28.5 | 14,461 | 33 % |
| `shade()` — the per-cell colour | 26.5 | 11,520 | 30 % |
| whole frame | 87.2 | 26,159 | |

Two thirds of the frame is now interpreted bytecode that no existing bulk op
covers — the 4-neighbour mirrored Laplacian and the element-wise shading map.
Gitea #373 sketches the builtins that would, with these numbers; #374 is the
on-panel look check.

## How to judge this on device

The method, kept for the next change that needs it — the results it produced
are the section above.

Host numbers understate the device win, but by **1.6× on the one fill-shaped
pair that was measured both ways**, not the ~3× the per-pixel entry cost
(317–440 Xtensa cycles vs a few x86 nanoseconds) suggests on its own. Read the
host ratios as a floor, and re-measure rather than scaling them.

- **`tools/opbench.mjs` cannot see this change at all.** Its K-sweep loop
  fits the slope of a bytecode loop and cancels the per-pixel entry by
  construction — which is precisely what `renderFrame` removes. It will read
  flat. Run it only to confirm dispatch has not regressed.
- **`tools/patbench.mjs <pattern>` is the measurement**: push the per-pixel
  pattern, record median `vm_us/px`, push the `renderFrame` rewrite of the
  same pattern, record again. Use a **stateless** probe — `snake-2d` swings
  74 % between builds a kilobyte apart and is useless as an A/B; the pairs
  above are all deterministic given a fixed delta.
- **The I-cache caveat still applies, but #328 defused most of it.** The
  sixteen arms land in `Vm::builtin_cold` — tier 3, `#[cold]`, flash-resident
  — and every `bulk.rs` symbol links into `.text`, so `Vm::run`,
  `Vm::call_builtin` and `Vm::builtin_hot` are byte-for-byte where master puts
  them and `.rwtext` does not grow. On the classic ESP32 all three of those
  execute from IRAM, which is what made placement reproducible in the first
  place (#328). It is *not* zero risk: the S3 pins only `Vm::run`, the RISC-V
  boards pin nothing, and #318/#325 showed layout is worth tens of percent
  non-monotonically. Re-run `patbench.mjs` on `perlin-fire-wind-tunnel`
  (repeats to ±0.3 %) against master before concluding the branch is free for
  patterns that use no bulk op.
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

The costs are real and should be weighed against that: **+8.8–9.3 KB of flash
on every board** as merged (down from +14.6–14.9 KB before the size pass),
which leaves every board's OTA margin where the gate wants it except
`board-c6-devkit`, which this change puts under the floor at 2.26 % (#291).
The I-cache risk that this evaluation could not test without hardware was
measured on 2026-09-07 and is **nil on both rigs** — see "Results — on
device".
