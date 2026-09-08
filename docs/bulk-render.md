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
| coordinate | `fillRect` `fillCircle` `splat` `drawLine` `fillCanvas` `paintCanvas` `fillGradient`(axis 1–3) | any map, including sparse/irregular; a predicate over each pixel's mapped (x, y) exactly as `render2D` sees it |
| grid | `blit`, `gridWidth`/`gridHeight` | a W×H grid that COVERS the frame (`grid.len() >= pixelCount`); cells past the end of the frame — the tail of the last row on an over-provisioned `ceil(√n)` map — clip. A silent no-op with no grid or one too small, and `gridWidth()` returns 0 in exactly those cases so a pattern can branch |

**What a canvas cannot reach.** `fillCanvas` decouples the simulation
resolution from the fixture's, but the canvas recipe cannot cover a big panel
*at its native resolution*: three parallel `array(4096)` H/S/V channels are
12,288 elements against the 10,236-element `DEFAULT_ARRAY_BUDGET` (and ~96 KB
of `Value` on the S3, which #275/#258 has already OOMed). Both canvas
conversions below simulate on a 16x16 canvas and let the sampler scale it.
`fillCanvas` is **HSV only** (`texel_at(.., hsv: true)` in `bulk.rs`). A
palette pattern whose colour is `paint()` uses **`paintCanvas(vArr, w, h [,
bArr])`** instead: the installed palette sampled at `vArr[i]`, times
`bArr[i]` (or 1), through the same geometry. It resolves the palette with
the same `sample_palette` and the same `paint()` position wrap the
interpreter uses, so a cell is **byte-identical to `paint(v, b)` +
`setPixel(i)`** — an RGB→HSV→RGB round trip through `fillCanvas` is not (`s
= d / max` and `h6 / 6` each truncate in 16.16). It also needs **one array
where `fillCanvas` needs three**, which is the difference between a
palette pattern being representable on a 64×64 panel and not. It leaves the
brush alone; a `bArr` of `v * v` (the usual `paint(v, v * v)` shape) is a
second array, so budget two.

**Producing a canvas.** Two ops fill one natively, and both are ordinary
array builtins — they work outside `renderFrame` too:

| op | what |
|---|---|
| `fillNoise2D(dst, w, h, sx, sy, ox, oy, seed)` | `dst[r·w + c] = simplex2(c·sx + ox, r·sy + oy, seed)`; returns dst |
| `fillNoise3D(dst, w, h, sx, sy, ox, oy, z, seed)` | the same with `simplex3` at a fixed z |

Both produce **exactly** what the equivalent interpreted loop produces,
argument arithmetic included (`c * sx + ox` as one `Fx` multiply then one
add, not an incremental accumulation) — they remove the interpreter around
the noise, never change it. `h = 1` fills a single row whose y is just
`oy`, which is how a pattern samples a lattice row at a time without
allocating a full-panel canvas.

A **lattice is not the map's own normalization**: a grid map puts column
`c` at `round(c · 65535 / (w − 1))` and `c · sx + ox` reproduces that
exactly only where `65535 / (w − 1)` divides evenly (it does at 16 and 18
wide, not at 64). A pattern that switches a per-pixel noise term to a
lattice fill is therefore sampling the same field on evenly spaced
coordinates instead of rounded ones — see the `aurora-2d.js` section for
what that is worth in output bytes.

**Exact cell coordinates.** A pattern that walks the grid itself has to
reproduce `MapData::coord` — `round(c * 65535 / (w - 1))` in 16.16 — and NOT
the obvious `c / (w - 1)`. The two agree only where `65535 / (w - 1)` divides
evenly (w = 16 does, and there only the far edge differs: a map normalizes it
to 65535/65536, never 1); they disagree on 11 of 64 columns at 64 wide and on
8 of 17 at 17 wide, which is a visible reconstruction error, not a rounding
LSB. `normAxis()` in `library/aurora-2d.js` is the reference form.

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

**The canvas block expand.** `fillCanvas` and `paintCanvas` have a second
fast path of their own, for the case the whole canvas idiom exists to serve:
a panel that is an exact integer multiple of the canvas. A 64×64 panel
showing a 16×16 canvas is a 4× block expand, so instead of the map lookup
and two `cell_index` divides per pixel it walks the grid in blocks — a
row-base add per pixel, and the texel resolved once per canvas cell per grid
row (1024 times instead of 4096). Measured on the `fillCanvas`-only
microbench at 4096 px: **8.1 → 1.4 ns/px, 33.0 → 5.9 µs/frame (5.6×)**; 3.1×
at 1024 px on a 32×32 grid and 1.7× at 256 px where the expand is 1×.

The block factor is **verified per call, never assumed**: `n % k == 0` is
necessary and not sufficient, because the block boundaries fall where the
map's own normalization rounding puts them and for some pairs (51 grid
columns onto a 51-cell canvas, 3569 onto 43) one lands a cell early. The
check walks one row and one column — 64 + 64 coordinate reads against the
4096 the scan costs — comparing `cell_index(coord(i), k)` against `i / m` at
every position, so the path is bit-identical to the scan by construction and
falls back to it whenever it is not. It covers both map kinds (a `--map-grid`
coordinate map and the zero-heap procedural grid) and both wirings; a
transposed grid (rows running along y) and any active transform keep the
scan.

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

> **Correction (2026-09-07, Gitea #378, #387).** `out_fps` in this table is a
> **compose** rate, not a display rate. It counted every `write_frame` call,
> including the ones the panel refused because the previous buffer swap had
> not landed — and a refused call did not retry until the render loop came
> round ~8 ms later, so the compose kept missing the rescan boundary. The
> panel rescans at 115 Hz and was actually showing roughly **60** frames a
> second on the 125-fps rows. `rainbow` at 52 is real (it is VM-bound, well
> under the ceiling); every row above ~60 is not. With vsync pacing the same
> rows land at ~112 displayed, and `out_fps` now counts displayed frames on
> every board. Re-measure before quoting these as throughput; the VM columns
> are unaffected.

| pattern | fps | out_fps | vm µs | vm µs/px |
|---|---:|---:|---:|---:|
| `rainbow` (per-pixel control) | 52 | 52 | 19,417 | 4.740 |
| `bulk-rainbow` | 125 | 125 | **2,772** | **0.677** |
| `bulk-comet-trails` | 125 | 126 | **431** | **0.105** |
| `bulk-bouncing-balls-2d` | 125 | 125 | **5,477** | **1.337** |
| `bulk-sprite-scroll-2d` | 125 | 124 | **5,125** | **1.251** |
| `bulk-canvas-ripples-2d` | 100 | 100 | **9,900** | **2.417** |

`rainbow` → `bulk-rainbow` is the only genuine like-for-like pair among the
shipped library patterns: **7.0× of VM time**, 52 → 125 fps composed (see the
correction above: the panel showed ~60 of those, and ~112 once vsync-paced). The
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
| `aurora-2d.js` (converted in place) | per-column band, per-pixel shimmer, `paint` + `setPixel` (`fillRect` above Cell Size 1) | 6.38 → **7.29** Mpx/s (**1.14x**); Cell Size 2/3/4 **1.75x / 3.48x / 5.94x** | 35.0 → 36.7 | **byte-identical** on eight rigs |
| `novas.js` (converted in place) | two pulse generators, three RGB channel buffers, one `fillRGB` | budget-refused at 4096 px **before and after**; 1024 px **1.05x** | 249.4 → **235.6** (1024 px) | **byte-identical** on six rigs |
| `fireblobs.js` (converted in place) | two additive blob layers, three RGB channel buffers, one `fillRGB` | budget-refused at 4096 px **before and after**; 1024 px **1.20x** | 177.2 → **145.2** (1024 px) | **byte-identical** on six rigs |
| `heatshivers.js` (converted in place) | two pulse generators + afterglow, two RGB channel buffers and a scalar blue, one `fillRGB` | budget-refused at 4096 px **before and after**; 1024 px **1.18x** | 48.4 → **43.4** (1024 px) | **byte-identical** on six rigs |
| `fireworks-finale.js` (converted in place) | 3 per-pixel RGB buffers, one `fillRGB` | 99.3 → **12.3** µs/frame at 2025 px (45x45) — see the note on 4096 px below | — | **byte-identical**, 7 rigs |
| `rocket-by-tony-hampton.js` (converted in place) | 3 per-pixel RGB buffers, one `fillRGB` | 88.3 → **21.8** µs/frame at 2025 px (45x45) | — | **byte-identical**, 7 rigs |
| `coolaura.js` (converted in place) | 2 per-pixel buffers (down from 4), one `fillRGB` | 467.6 → **367.6** µs/frame at 2025 px (45x45) | — | **byte-identical** wherever the original loads; the original cannot load above 2559 px |
| `christmaspewpew.js` (converted in place) | 2 trail buffers + a constant underglow array, one `fillRGB` | 149.8 → **84.4** µs/frame at 3364 px (58x58) | — | **byte-identical**, 8 rigs |
| `neutronorbit.js` (converted in place) | 3 comet trails, native release + windowed hold, `clear` + a `setPixel` loop over the lit pixels | budget-refused at 4096 px **before and after**; 1024 px **2.12x** | 107.5 → **56.8** (1024 px) | **byte-identical** on six rigs |
| `4th.js` (converted in place) | 4 per-pixel buffers, per-pixel `random()` so no bulk fill is possible; hoisted bunting bands + a `setPixel` loop | budget-refused at 4096 px **before and after**; 1024 px **1.11x** | 56.2 → **49.3** (1024 px) | **byte-identical** on six rigs, and across 7 `StripeWidth` values x 4 pixel counts |
| `bouncing-balls-rgb.js` (converted in place) | 3 accumulation buffers, the direction fold moved to the deposit, one `fillRGB` | budget-refused at 4096 px before and after; 3364 px (58x58) 244.0 → **15.5** µs/frame (**15.8x**) | — | **byte-identical** on 8 rigs x all 4 direction modes |
| `pew-pew-pew.js` (converted in place) | 3 trail buffers + 2 constant ambient arrays, mirror moved to the paint, one `fillRGB` | budget-refused at 4096 px before and after; 2000 px 127.4 → **24.7** µs/frame (**5.2x**) | — | **byte-identical** on 8 rigs x all 4 toggle combinations |
| `swirlpool-2d.js` (converted in place) | 16x16 brightness/hue canvases, one `fillCanvas` | 183.1 → **41.8** µs/frame (**4.38x**) | 16.1 → **1.0** | **byte-identical** on the three coordinate maps and a 60 px strip; on the 300/512 px mapless strips the only delta is the `-0.01` floor fudge (below) |
| `2d-fireworks-fade.js` | **not converted** — bilinear canvas, additive RGB | — | — | a nearest-sampling rewrite is 3.27x at 4096 px and **not** equivalent: 1.3–7 % of bytes differ, maxdiff 255 |

### The `fillRGB` readout batch (2026-09-07)

Four patterns from the "already fillHSV/fillCanvas-shaped" bucket #373 section 5
listed. All four kept three parallel per-pixel channel arrays and read them back
with one `rgb()` per pixel, which is exactly `fillRGB`'s shape:

```
render(index) { rgb(rBuf[index], gBuf[index], bBuf[index]) }
    ->  renderFrame() { fillRGB(rBuf, gBuf, bBuf) }
```

The readout's own `saturate()` / `clamp(v, 0, 1)` / `min(v, 1)` guards drop out:
`fillRGB` goes through `engine::quantize`, which clamps to 0..1 exactly the way
`rgb()` does, so the conversion is byte-exact by construction and not merely by
measurement. All four stay in **index space**, so a mapless strip is the native
rig and none of them acquires a `ceil(√n)` grid it never asked for; on a 2D map
each pixel still reads its own buffer slot, so the fill is the same readout at
the same resolution — there is no canvas and no resampling anywhere here.

| pattern | rig | before µs/frame | after µs/frame | ratio |
|---|---|---:|---:|---:|
| `fireworks-finale.js` | 1024 px, 32x32 | 52.6 | 8.3 | **6.3x** |
| `fireworks-finale.js` | 2025 px, 45x45 | 99.3 | 12.3 | **8.1x** |
| `rocket-by-tony-hampton.js` | 1024 px, 32x32 | 49.8 | 11.7 | **4.3x** |
| `rocket-by-tony-hampton.js` | 2025 px, 45x45 | 88.3 | 21.8 | **4.0x** |
| `coolaura.js` | 1024 px, 32x32 | 241.1 | 195.9 | 1.23x |
| `coolaura.js` | 2025 px, 45x45 | 467.6 | 367.6 | 1.27x |
| `christmaspewpew.js` | 1024 px, 32x32 | 45.7 | 26.3 | **1.7x** |
| `christmaspewpew.js` | 3364 px, 58x58 | 149.8 | 84.4 | **1.8x** |

`coolaura` is the low ratio because almost all of its frame is a `beforeRender`
that already walks every pixel for every live pulse — the per-pixel entry it
sheds is a small share of that. `christmaspewpew` sits in between because its
red channel is the trail plus a constant underglow and the fill has to pay two
extra native passes for it (below).

**Two of the four needed more than a one-line swap.**

* `coolaura` allocated **four** `array(pixelCount)` buffers — an intensity
  accumulator plus R/G/B — and red was identically 0. Red is now a scalar
  `fillRGB` broadcasts, the intensity accumulator *is* the green buffer until
  step 5 rewrites it in place, and the per-channel square that `render` applied
  moved into that same step. Two buffers instead of four. That is not
  housekeeping: four `array(4096)` channels are 16,384 elements against the
  10,236-element `DEFAULT_ARRAY_BUDGET`, so **the pre-conversion pattern could
  not load on a 64x64 panel at all** and rendered black. It now runs there, at
  799 µs/frame. Its per-frame clear also stopped being a `pixelCount` bytecode
  loop and became `feedback(gbuf, 0)`.
* `christmaspewpew` read `min(1, trailR[index] + AMBIENT_R)`. **There is no
  array-plus-scalar builtin** — `arrayAdd`/`arraySub` take two arrays,
  `arrayScale` multiplies, and #373's proposed `arrayAffine(dst, src, k, c)` is
  exactly the missing arm. The underglow is therefore a constant
  `array(pixelCount)` that is added before the fill and subtracted straight
  after, which is exact (a fixed-point add and its inverse round-trip with no
  rounding) and costs two native passes. It charges no extra elements, because
  the blue trail buffer it replaces was never written by anything.

**The array budget, not the VM, is the ceiling for this bucket.** Six of the
seven patterns in #373's list of 21 that were sampled here cannot load at
4096 px *before or after* conversion: three `array(pixelCount)` channels are
12,288 elements against the 10,236 budget, so `array()` fails during init and
the pattern renders black on a 64x64 panel. That is the same wall
docs/bulk-render.md already records for `rainbow-comet.js`, and it is why the
ratios above are quoted at 45x45 and 58x58 rather than 64x64. Gitea #405 carries
the conversions; the budget itself is tracked separately.

**Visual equivalence**, 60 frames at a fixed 30 fps delta and seed
(`luxel run --out`), byte for byte against the pre-conversion file on 16x16,
32x32, 45x45, 58x58 and 64x64 grids and 60/300/512/1000 px mapless strips:
**maxdiff 0 on every rig where the original loads**, for all four patterns. The
one non-zero cell in the sweep is `coolaura` at 58x58 and 64x64, where the
pre-conversion file renders black because of the budget and the converted one
renders the pattern.
| `bouncy-boxes.js` (converted in place) | 16x16 box sim + glitch garnish, one `fillCanvas`, `has2DMap()` keeps the mapless-strip black | 13.82 → **23.68** Mpx/s (**1.71x**) | 26.6 → **14.6** | **byte-identical** on all five rigs |
| `ice-floes-2d.js` (converted in place) | 16x16 Voronoi canvas, one `fillCanvas` | 13.02 → **21.47** Mpx/s (**1.65x**) | 26.6 → **13.6** | byte-identical on 16x16 / 32x32 / 64x64 / 60 px; the `-0.01` floor fudge elsewhere (see below) |
| `nyan-lights.js` (converted in place) | sprite/rainbow composite into a 16x16 canvas, rebuilt only on a flip or a dial move, one `fillCanvas` | 12.88 → **108.36** Mpx/s (**8.40x**) | 35.4 → **0.5** | byte-identical on 16x16 / 32x32 / 64x64 / 60 px; the `-0.01` floor fudge elsewhere (see below) |
| `color-bands-buffered.js` (converted in place) | 3 per-pixel H/S/V buffers, one `fillHSV` | budget-refused at 4096 px **before and after**; 1024 px **1.10x**, 3000 px **1.12x** | 71.0 → **66.0** (1024 px) | **byte-identical** on all five rigs |
| `music-sequencer-for-v3-only.js` (converted in place) | 3 per-pixel H/S/V buffers, one `fillHSV`, hue offset added and taken back out | budget-refused at 4096 px **before and after**; 1024 px **7.67x**, 3000 px **5.64x** | 10.3 → **0.3** (1024 px) | **byte-identical** on all five rigs, undriven and with `Theme Hue` driven |
| `rainbow-comet.js` (converted in place) | per-pixel state evolution, `clear()` + a `setPixel` loop that skips dead pixels, `feedback` for the fade | budget-refused at 4096 px **before and after**; 3000 px **1.09x**, 300 px **1.33x**, 60 px **1.41x** | 25.2 → **23.8** (3000 px) | **byte-identical**, 400 frames, five rigs, three control settings |

### Two readouts that were not indexed by pixel (2026-09-07)

`bouncing-balls-rgb.js` and `pew-pew-pew.js` are the same `fillRGB` shape as
the batch above with one extra twist each: their per-pixel `render` did not
read `buf[index]`, it read `buf[f(index)]`. **A whole-frame fill is indexed by
pixel and cannot remap**, so in both cases the remap moved to the write side —
which is also where it belongs, since it then runs once per *entity* instead
of once per pixel.

| pattern | rig | before µs/frame | after µs/frame | ratio |
|---|---|---:|---:|---:|
| `bouncing-balls-rgb.js` | 1024 px, 32x32 | 77.4 | 5.9 | **13.1x** |
| `bouncing-balls-rgb.js` | 3364 px, 58x58 | 244.0 | 15.5 | **15.8x** |
| `pew-pew-pew.js` | 512 px strip | 32.5 | 8.7 | **3.7x** |
| `pew-pew-pew.js` | 1024 px, 32x32 | 62.0 | 13.7 | **4.5x** |
| `pew-pew-pew.js` | 2000 px strip | 127.4 | 24.7 | **5.2x** |

* **`bouncing-balls-rgb`** has a four-way Direction control — head, tail, both
  ends into the middle, middle out to both ends — implemented as an index
  remap in `render`, with the accumulator holding only the "usable" half in
  the folded modes. The fold now happens where a ball is deposited: a ball
  lands directly on the strip pixel (or the two mirrored pixels) the remap
  used to make it appear at, so the accumulator is already in strip order.
  That is `NUM` deposits a frame instead of `pixelCount` remaps, and it is
  what makes this the batch's largest ratio. Its per-frame clear also stopped
  being a bytecode loop and became three `feedback(buf, 0)` calls. The mirror
  arm of "both ends into the middle" needs one guard — on an odd pixel count
  the centre pixel is its own reflection and must not be deposited twice.
* **`pew-pew-pew`** has a Mirror toggle (same problem, same fix: the volley is
  fired the other way down the strip instead of being reflected on the way
  out) and a **warm ambient underlay added per pixel**,
  `rgb(bufR[p] + 0.05, bufG[p] + 0.01, …)`. There is no array-plus-scalar
  builtin, so the two constants live in constant `array(pixelCount)`s that are
  added before the fill and subtracted straight after — exact, because a
  fixed-point add and its inverse round-trip with no rounding.

  **The interpreted alternative is a regression, measured:** doing the same
  two adds and their undo as bytecode loops over `pixelCount` runs at
  **0.75x** at 1024 px and 0.84x at 1000 px — slower than the per-pixel
  `render` it replaced. A `pixelCount` bytecode loop costs about 50 ns/px
  here, more than the per-pixel render entry it is trying to avoid. So for
  this shape the constant arrays are not a convenience, they are the only
  version that wins.

  They cost element budget, and this is the one conversion in these batches
  that **lowers a ceiling**: five `array(pixelCount)` instead of three takes
  the pattern's maximum strip from ~3,399 px to ~2,019 px. Both are far past
  every rig in the tree (`check-library` tops out at 512 px, the Athom runs
  60) and the pattern was already budget-refused on a 4096-px panel, so the
  affected range is empty in practice — but #373's proposed
  `arrayAffine(dst, src, k, c)` would remove both arrays and the two passes
  together, and this is the concrete case for it.

Mirror and Direction are the reason the equivalence sweep here is bigger than
usual: **byte-identical on 8 rigs for all four `bouncing-balls-rgb` direction
modes and all four `pew-pew-pew` toggle combinations**, not just at the
defaults. Moving a remap to the write side is only equivalent if the *inverse*
is exactly right, and a two-mode spot check would not have caught the odd-count
centre pixel. What is not identical is the transient when one of those controls
is flipped *mid-run*: the old code reflected the trail already in the air
instantly, the new code turns the new paint around and the trail follows within
a few frames (about 5 at the default decay). Steady state is the same picture.

### `2d-fireworks-fade.js` — does not convert, and the reusable piece that came out of trying (2026-09-07)

`2d-fireworks-fade.js` deposits its shells and embers into a `CW x CH` virtual
canvas and reads it back with `canvasGet` in `render2D`. That is the classic
`fillCanvas` shape, and it is the one pattern in #405's section-5 bucket that
**cannot take a bulk fill at all**, for two independent reasons:

1. **The canvas is RGB and additive.** Sparks accumulate `col[0..2]` per cell.
   `fillCanvas` and `blit` are HSV-only (`texel_at(.., hsv: true)`), and
   resolving an additive RGB canvas through `rgb2hsv` is not exact in 16.16 —
   the same wall `aurora-2d.js` hit with `paint()`. #373's `paintCanvas` is
   the missing op.
2. **`canvasGet` is bilinear; `fillCanvas` is nearest.** `canvasGet` runs
   `sample_axis` on both axes and lerps four texels (docs/lang.md: "**bilinear**
   sample"); `fillCanvas` takes `cell_index` — one texel, no blend. This
   pattern's whole design is the smooth upscale ("smoothly upscaled on a 64x64
   panel", its own header), so nearest sampling is not a rounding difference,
   it is a different picture. **There is no bilinear bulk fill of any kind.**

Both were measured, not assumed. A rewrite that paints the canvas as
rectangles (below) is **3.27x at 4096 px / 64x64** and 1.11x at 1024 px, and it
is *not* equivalent: 1.3–7 % of bytes differ on every rig with maxdiff up to
255. Left unconverted; the win is real but it is not this pattern's look. A
bilinear mode on `fillCanvas`, or a `paintCanvas` that takes one, would make it
convertible — that is the concrete ask on #373.

**The reusable piece: a nearest canvas fill with an RGB (or palette) brush.**
A `renderFrame` can reproduce `fillCanvas`'s nearest sampling exactly using one
`fillRect` per *run* of identical cells along a canvas row, which is what a
non-HSV canvas pattern needs until `paintCanvas` exists. On a mostly-dark
canvas that is a handful of rects per row instead of `CW` of them. Two details
make it exact rather than nearly-exact:

* **The cell edge table cannot be `k / CW`.** A 16.16 divide truncates, and the
  truncated boundary can still resolve to cell `k - 1`: `1/17` is 3855 raw, and
  `cell_index(3855, 17)` is **0**, not 1 — and a 300-px strip's 18x17 default
  grid hands `render2D` exactly 3855. A rect starting there steals the previous
  cell's last pixel. Nudge by one 16.16 unit when the divide undershot:

  ```
  const RAW1 = 1 / 256 / 256        // 1/65536 is not a writable literal:
                                    // literals are 16.15, so 1/65536 rounds to 0
  t = k / n
  if (floor(t * n) < k) t = t + RAW1
  ```
* **Make the rects disjoint, not merely adjacent.** `fillRect` bounds are
  inclusive, so `[b[k], b[k+1]]` shares its far edge with the next cell and the
  result depends on paint order — which stops being sound the moment a run is
  skipped or merged. `[b[k], b[k+1] - RAW1]` is exactly cell `k` and order
  stops mattering. Store `b[n] = 1 + RAW1` so the last cell's high edge is 1.

Verified: this construction is **byte-identical to a `floor(x*CW)` nearest
readout on seven rigs** — 16x16, 32x32 and 45x45 grids and 60/300/512/1000 px
strips, the 300 px case being the one the naive edge table gets wrong.

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

### `aurora-2d.js` (2026-09-07)

The dense-procedural counter-example, converted anyway because one of its two
noise fields is not actually per pixel. `simplex2(x * 1.8, z, 5)` depends only
on the column, and the old `render2D` evaluated it 4096 times a frame on the
64x64 panel for 64 distinct answers; `renderFrame` walks the grid itself, so
the band is a 64-entry table rebuilt in `beforeRender` and the shimmer stays
one `simplex3` per pixel. Builtin calls per frame: **simplex2 4096 -> 64**,
everything else unchanged.

**No canvas, and no `fillCanvas`.** Two facts pushed this one off the
`fillCanvas` shape the other two conversions use:

* **There is no palette-space bulk fill.** `fillCanvas` samples H/S/V, and the
  pattern's colour is `paint(v, v * v)` -- a palette index and a brightness.
  Resolving that per cell means a bytecode palette lookup plus `rgb2hsv`, and
  the RGB -> HSV -> RGB round trip is not exact in 16.16 (`s = d / max` and
  `h6 / 6` each truncate), so it moves the odd 8-bit channel by one. Keeping
  `paint()` as the brush and putting it down with `setPixel(index)` keeps the
  real palette and is byte-exact.
* **A full-resolution HSV canvas does not fit.** Three parallel `array(4096)`
  channels are 12,288 elements against the 10,236-element budget
  (`DEFAULT_ARRAY_BUDGET`), and ~96 KB of Value on the S3 that #275/#258 has
  already OOMed once. The converted pattern allocates five 128-entry column
  tables -- 640 elements -- and no canvas at all.

| rig | before ns/px | after ns/px | ratio |
|---|---:|---:|---:|
| 4096 px, `--map-grid 64x64` | 156.85 | 137.23 | **1.14x** |
| 256 px, `--map-grid 16x16` | 165.91 | 151.83 | 1.09x |
| 300 px strip (18x17 default grid) | 165.33 | 150.27 | 1.10x |
| 4096 px, Cell Size 2 / 3 / 4 | 158.7 / 157.1 / 156.5 | 90.8 / 45.2 / 26.3 | **1.75x / 3.48x / 5.94x** |

Interpreted instructions go *up* slightly at Cell Size 1 (35.0 -> 36.7
insns/px at 4096 px, 35.0 -> 38.9 at 256 px) while wall time goes down: what
the rewrite removes is 63/64 of an expensive native builtin plus the per-pixel
entry, and what it adds is cheap loop and array bookkeeping. That is the
honest shape of a dense-procedural conversion -- 1.1x, not the 3-30x a readout
or an entity loop gets, and it comes entirely from the one term that was not
per pixel.

**Visual equivalence**, 60 frames at a fixed 30 fps delta and seed
(`luxel run --out`), compared byte for byte against the pre-conversion file:

| rig | maxdiff | bytes differing |
|---|---:|---:|
| 256 px, 16x16 map | **0** | **0** / 46,080 |
| 1024 px, 32x32 map | **0** | **0** / 184,320 |
| 4096 px, 64x64 map | **0** | **0** / 737,280 |
| 100 px, 10x10 map | **0** | **0** |
| 289 px, 17x17 map | **0** | **0** |
| 60 / 300 / 512 px strips (default grids) | **0** | **0** |

Byte-identical everywhere, which took reproducing the engine's own grid
normalization rather than the obvious `c / (w - 1)`: `MapData::coord` returns
raw `(c * 65535 + (n - 1) / 2) / (n - 1)`, and the naive form disagrees on 11
of 64 columns at 64 wide and 8 of 17 at 17 wide. `normAxis()` in the pattern
does the exact integer arithmetic (`i * EPS` is `i` *raw* units, so the
numerator is `i * 65535 + floor(d/2)` before an integer-divisor divide, which
the `Fx` `Div` integer-divisor fast path evaluates exactly). Driving the one
control at its declared `default=` is byte-identical to leaving it undriven.

**Where the frame goes now** (4096 px): 4096 `simplex3` calls, 4096 `paint`
calls, 4096 `setPixel` calls and ~36.7 interpreted instructions per pixel.
Gitea #373 carries the two ops that would change that -- a canvas noise fill
and a palette-space canvas fill -- with the estimate this profile supports.

### The 1-D channel-buffer bucket — `novas`, `fireblobs`, `heatshivers` (2026-09-07)

Gitea #405 / #373 §5: patterns whose `beforeRender` already fills parallel
per-pixel channel buffers and whose `render` is one `rgb()`/`hsv()` read-out of
them. These three are the pure RGB form, and the conversion is the same three
moves every time:

1. **The read-out's arithmetic folds back into the buffers, in place.** All
   three squared for gamma in `render` (`rgb(r * r, g * g, b * b)`), and `novas`
   clamped first. Those buffers are fully rewritten every frame — zeroed at the
   top, accumulated, then read — so squaring them at the *end* of `beforeRender`
   applies exactly once and needs **no new arrays**. That matters more here than
   anywhere else: these patterns are already four or five `pixelCount` buffers
   against a 10,236-element budget, so a conversion that added a fourth channel
   trio would have cost more ceiling than it bought speed.
2. **`renderFrame()` is one `fillRGB`.** `fillRGB` is an *index-space* op — it
   takes an array or a scalar per channel and addresses pixel `i` — so it is
   map-independent: on a bare strip it is exactly the old `render(index)` loop,
   and on a matrix these patterns still run along the pixel index rather than
   acquiring a geometry they never had. `heatshivers` uses the scalar form for
   its always-black blue channel: `fillRGB(chR, chG, 0)`.
3. **The zero-fills go native.** Each frame these patterns cleared three or four
   `pixelCount` buffers with an interpreted `for` loop. `feedback(a, 0)` is the
   same thing as one VM call, and it is where most of the measured win actually
   comes from — the `renderFrame` entry alone is worth ~1.02x on this host.
   Two per-pixel passes also gained a `v > 0` / `iv <= 0` skip, exact because
   every term reaching these buffers is non-negative (a zero channel squares to
   itself, and a zero intensity contributes nothing to the composite).

| rig | `novas` | `fireblobs` | `heatshivers` |
|---|---:|---:|---:|
| 1024 px, `--map-grid 32x32`, µs/frame | 580.1 → **554.2** (1.05x) | 399.9 → **334.0** (1.20x) | 110.9 → **93.9** (1.18x) |
| 256 px, `--map-grid 16x16`, µs/frame | 147.3 → **139.7** (1.05x) | 102.1 → **87.0** (1.17x) | 28.2 → **24.4** (1.16x) |
| insns/px at 1024 px | 249.4 → **235.6** | 177.2 → **145.2** | 48.4 → **43.4** |

`novas` gains least because its frame is dominated by the per-bloom half-sine
painting loops and the tint/max-merge pass, neither of which any existing bulk
op covers — the read-out was never the expensive part. Host numbers understate
the device win, where the per-pixel `render` entry alone is 317–440 Xtensa
cycles.

**Equivalence**: 60 frames at a fixed 30 fps delta and seed, PPMs compared byte
for byte against the pre-conversion files on **six** rigs — 16x16, 32x32 and
64x64 coordinate maps and 60 / 300 / 512 px mapless strips — **maxdiff 0**
everywhere, undriven and with every control driven off its default
(`novas`: five controls; `fireblobs`: four; `heatshivers` has none).

**These three do not run at 4096 px, before or after.** `novas` and `fireblobs`
allocate four `pixelCount` arrays and `heatshivers` five, against
`DEFAULT_ARRAY_BUDGET` = 10,236 elements: the ceilings are 2,559 / 2,559 /
2,047 pixels and the conversion moves none of them, because it adds no arrays.
At 4096 px both sides fail the budget identically. One caveat worth recording:
`fireblobs` allocates lazily inside `beforeRender` (`ensureBuffers()`), so at
4096 px the budget error lands *mid-allocation* and the pattern runs on a
partly-allocated buffer set; before and after then diverge on 66 of 737,295
bytes with the controls driven. That is two different walks through an already
broken state, not a rendering difference — at 2,048 and 2,400 px, the largest
sizes that actually allocate, the same driven run is byte-identical.
### The canvas readouts — `bouncy-boxes`, `ice-floes-2d`, `nyan-lights` (2026-09-07)

Three patterns from #373's "already sitting on the `fillHSV`/`fillCanvas`
shape" bucket (#405). All three were already 16x16 simulations whose
`render2D` did nothing but resolve a colour out of the canvas, so the whole
conversion is deleting the readout and issuing `fillCanvas(hC, sC, vC, 16, 16)`
once per frame. `nyan-lights` also moves its *composite* off the per-pixel
path: the sprite/rainbow/background decision was re-run at every LED for 256
distinct answers, and now runs into the canvas only when the flip flag or one
of the three rainbow dials actually changes — roughly 10 repaints a second
instead of 4096 per frame.

| rig | bouncy-boxes | ice-floes-2d | nyan-lights |
|---|---:|---:|---:|
| 4096 px, `--map-grid 64x64` | 296.3 → **173.0** µs/frame (**1.71x**) | 314.6 → **190.8** (**1.65x**) | 317.9 → **37.8** (**8.40x**) |
| 256 px, `--map-grid 16x16` | 151.5 → 141.9 (1.07x) | 175.7 → 165.3 (1.06x) | 22.4 → **6.6** (3.41x) |
| 300 px strip (18x17 default grid) | 145.5 → 138.5 (1.05x) | 166.5 → 159.4 (1.04x) | 25.2 → **8.4** (2.99x) |
| insns/px @ 4096 px | 26.6 → **14.6** | 26.6 → **13.6** | 35.4 → **0.5** |

The 256 px rows are the same lesson `raindrops-2d` taught: when the display is
already the size of the simulation, a readout conversion buys almost nothing —
the `beforeRender` pass that was always there is the whole frame. What the
conversion removes is the *scaling* term, and each of the three now reports the
**same instruction count at 256 and at 4096 px** (17,954,882 / 16,704,000 /
624,843 over 300 frames), so the panel size no longer costs interpreted work at
all.

**Visual equivalence**, 60 frames at a fixed 30 fps delta and seed
(`luxel run --out`), byte for byte against the pre-conversion file, undriven and
again with every control driven at its declared `default=`:

| rig | bouncy-boxes | ice-floes-2d | nyan-lights |
|---|---:|---:|---:|
| 256 px, 16x16 map | **0** | **0** | **0** |
| 1024 px, 32x32 map | **0** | **0** | **0** |
| 4096 px, 64x64 map | **0** | **0** | **0** |
| 60 px strip (8x8 default grid) | **0** | **0** | **0** |
| 300 px strip (18x17 default grid) | **0** | maxdiff 132, 44.6 % | maxdiff 255, 17.7 % |

**Every non-zero byte is the `-0.01` floor fudge, not the conversion.** The old
readout wrote `floor(y * 15.99) * 16 + floor(x * 15.99)` — the `15.99` being the
usual dodge for `x == 1` indexing one past the end — while `fillCanvas` samples
with `cell_index`, i.e. `clamp(floor(x * 16), 0, 15)`. The two agree wherever no
mapped coordinate lands exactly on a cell boundary, which is why 16x16, 32x32,
64x64 and the 8x8 default grid are byte-identical; they disagree by a whole cell
wherever one does. The 18x17 default grid of a 300 px strip is the case: its 17
rows normalize to exactly `r / 16`, so `15.99 * r / 16 < r` and the old form
read row `r - 1` for every row. That is not an inference — the **original** with
only its index arithmetic changed to `min(floor(c * 16), 15)` is byte-identical
to each converted pattern on 289, 300, 512 and 1000 px, where the shipped form
differs by up to 207 per channel.

`bouncy-boxes` is byte-identical everywhere because it exports `render` as well
as `render2D`, which suppresses the default grid: on a mapless strip it was
black and still is. Keeping it that way needed a guard, because `renderFrame`
wins over `render` unconditionally and `fillCanvas` on a mapless fixture is not
a no-op — the 1-D fallback coordinate hands every pixel `y = 0.5`, which would
paint one row of the canvas along the whole strip. `has2DMap()` is the branch:

```js
export function renderFrame() {
  if (has2DMap()) fillCanvas(hc, sc, vc, W, H)
  else clear()          // no map: the shipped 1-D fallback was black
}
```

`ice-floes-2d` and `nyan-lights` export no `render`, so they get the default
`ceil(√n)` grid exactly as they did under `render2D` and need no guard.

### The `fillHSV` readouts, and the two hardest cases (2026-09-07)

Batch 2 of #405's #373-section-5 bucket, and the honest half of it: two
one-line conversions, one that took real work, and one that does not convert
at all.

**`color-bands-buffered.js`** is the bucket's purest case: it is a *technique
demo* whose whole point is that `beforeRender` fills `hueB`/`satB`/`briB` and
`render` only reads them back, keeping the per-pixel callback cheap for
timing-sensitive protocols. That readout is `fillHSV(hueB, satB, briB)` — one
line, and byte-exact by construction. **`music-sequencer-for-v3-only.js`
("Main Stage")** is the same shape at the end of a 700-line music-choreography
framework: sixteen mini-patterns write the three shared scratch buffers and the
three renderers only read them.

| rig | color-bands-buffered | music-sequencer (Main Stage) |
|---|---:|---:|
| 1024 px, `--map-grid 32x32` | 164.3 → **148.7** µs/frame (1.10x) | 40.4 → **5.3** (**7.67x**) |
| 3000 px strip | 460.3 → **410.1** (1.12x) | 76.6 → **13.6** (**5.64x**) |
| 300 px strip | 46.3 → **41.1** (1.13x) | 8.3 → **2.1** (**4.02x**) |
| insns/px, 1024 px | 71.0 → **66.0** | 10.3 → **0.3** |

The gap between the two ratios is the whole lesson of a read-out conversion:
`color-bands-buffered` spends its frame in the `beforeRender` loop that was
always there, so removing the per-pixel entry moves 10 %; Main Stage's
mini-patterns touch only a few pixels a frame, so the entry *was* the frame and
removing it moves 7.7x.

**`Theme Hue` is the one thing `fillHSV` cannot express.** Main Stage's readout
was `hsv(hueA[index] + hueOffset, …)` — an array plus a scalar, and `arrayAdd`
takes two arrays (#373's proposed `arrayAffine` is exactly this arm). A fourth
`array(N + 1)` for an offset copy is not affordable here (see below), so the
offset is added into `hueA` before the fill and subtracted straight after.
Fixed-point add and subtract are exact and mutually inverse, so the buffer the
mini-patterns see next frame is bit-for-bit the one they left; the shipped
default (`hueOffset == 0`) takes an early branch and pays nothing at all.
Verified both ways — byte-identical on all five rigs undriven *and* with
`sliderThemeHue=90`.

**Both are refused by the array budget above ~3,400 px, before and after.**
Three `array(pixelCount)` channels are 12,288 elements at 4096 px against the
10,236-element `DEFAULT_ARRAY_BUDGET`, so `array()` fails during init and the
pattern renders black on a 64x64 panel. Measured ceilings:
`color-bands-buffered` 3,408 px, `music-sequencer-for-v3-only` 3,256 px (it
allocates `N + 1`), `rainbow-comet` 3,408 px. That is the same wall the
`fillRGB` batch above hit, and it is why the ratios here are quoted at 1024 and
3000 px. **Converting this bucket does not change the ceiling** — the buffers
are the pattern's state, not the readout — so for these patterns the win is on
strips and small matrices, not on the panel.

#### `rainbow-comet.js` converts, `meteor-shower.js` does not (2026-09-07)

Both were on #373's list of 21, both were tried and measured, and they came out
on opposite sides of the same rule.

**`rainbow-comet.js` — 1.09x at 3000 px, 1.33x at 300 px, 1.41x at 60 px,
byte-identical over 400 frames on all five rigs at three control settings
(undriven, both dials at 1, both at 0).** It is *not* a `fillHSV`: the value
channel is `bri[i] * bri[i]` rather than `bri`, and the per-pixel body does not
read state, it **evolves** it — hue smear, saturation cure, brightness decay.
A fourth `array(pixelCount)` for the squared channel would drop the pattern's
ceiling from 3,408 px to ~2,556 (#420), and squaring `bri` in place is not
bit-exact, because the stored value is re-multiplied by `decay` every frame and
`(b · decay)²` drifts from `b² · decay²` over a tail's ~37 frames of 16.16
rounding.

What made it convert anyway is the two things the loop *can* hand to the engine:

```js
export function renderFrame() {
  clear()                              // renderFrame does NOT clear between frames
  for (i = 0; i < pixelCount; i++) {
    b = bri[i]
    if (b == 0) continue               // black already, and hue/sat are
                                       // overwritten wholesale on the next stamp
    hsv(hue[i], sat[i], b * b)
    setPixel(i)
    hue[i] -= 0.004
    sat[i] = min(sat[i] * 1.06, 1)
  }
  feedback(bri, decay)                 // the whole fade, natively, after the pass
}
```

`feedback(bri, decay)` is exact because every element is read before the array
is scaled once, and the dead-pixel skip is exact because a dark pixel's hue and
saturation are overwritten wholesale when the head next stamps it. The skip is
most of a strip between passes of the head, and it is what turns the trade
positive: a naive port with neither measured **0.81x**. `clear()` is
load-bearing, not decoration — `renderFrame` starts on last frame's output, so
without it a skipped pixel would keep its old colour forever, and a 60-frame
equivalence sweep does **not** catch that (no pixel decays to exactly 0 inside
two seconds). The sweep was extended to 400 frames for exactly this reason.

**`meteor-shower.js` — 0.83x at 3000 px, 0.84x at 300 px, 0.83x at 60 px.
Reverted.** Its trail is a ring buffer read through a rotation,
`hBuf[(index + head) % pixelCount]`, and `fillHSV` indexes its channel arrays
by the pixel index with **no offset**; there is no `arrayRotate`, and
materializing unrotated copies would cost three more `array(pixelCount)`
buffers the budget cannot pay for. The rotation *is* two contiguous runs rather
than a modulo — pixels `[0, n - head)` read cells `[head, n)` and the rest read
`[0, head)` — and hoisting the `%` and the `REVERSE` test out of the body took
a first attempt from 0.70x to 0.83x, byte-identical including with `Reverse`
driven. It is still a loss, and there is nothing left to hoist: the body is
three array reads, and unlike the comet it has no dead pixels to skip (a cell's
value is reset to 1 when it falls below 0.02, so the buffer never holds zeros).
An offset (or stride) argument on `fillHSV`/`fillRGB`, or an `arrayRotate`,
converts it in one line — a #373-class ask, not a pattern change.

The rule these two produce: **a `setPixel` loop wins only when the per-pixel
body is native work, or when enough of the per-pixel work can be lifted out of
it.** `aurora-2d.js` wins because its body is `simplex3` + `paint`;
`rainbow-comet` wins because `feedback` takes the fade and the skip takes the
dark pixels; `meteor-shower` loses because after every hoist its body is still
three interpreted array reads, which is exactly what the interpreter is slow at
and exactly what a bulk op would have absorbed.

### `neutronorbit` and `4th` — when the read-out cannot be a bulk fill (2026-09-07)

Two more #405 conversions where `fillRGB` is the *wrong* answer and
`renderFrame` is still the right one. Both keep `clear()` + a `setPixel` loop,
and both are byte-identical over 60 frames at a fixed delta and seed on six
rigs (16x16 / 32x32 / 64x64 maps, 60 / 300 / 512 px mapless strips), undriven
and driven.

**`neutronorbit` — 2.12x, and the array ceiling is why it is a loop.** The
frame is three comet trails (peak-hold with exponential release), a nucleus,
and a per-channel max of the three tinted layers, squared. That read-out is
*destructive*: the trails are persistent state, so filling three channel
buffers would mean three more `pixelCount` arrays on top of the three it
already has — which measurably drops the pattern's pixel ceiling from ~3,411
to ~1,705 against the 10,236-element budget. Both shapes were written and
benched side by side and they are **the same speed** (`fillRGB` over three
prebuilt buffers 2.00x, `clear` + `setPixel` over the lit pixels 1.98x at
1024 px), because both skip the dark pixels and this pattern's frame is rarely
more than a fifth lit. The loop wins on the tie-break: no new arrays.

Almost all of the 2.12x is in `beforeRender`, not the read-out:

* The trail release was `trail[i] = max(trail[i] * decay, hump(p, c))` over
  every pixel for each of three comets — three interpreted passes with three
  function calls per pixel. It is now `feedback(trail, decay)` (one native
  call) plus a hold over only the window `hump()` can reach, which is `cwidth`
  of the strip: **10 % by default**. Outside the window `hump()` is 0 and
  `max(v, 0)` is `v`, so the native release is already the whole answer; the
  window bounds are widened one pixel each way and `hump()`'s own
  `d >= cwidth / 2` guard keeps the edges exact.
* The nucleus is a triangle with no trail, so it also only touches its own
  window. It merges by `max` and every value involved is non-negative, and
  squaring is monotone there — `max(v, n)²  == max(v², n²)` — so the merge can
  happen after the gamma square rather than before it.

107.5 → **56.8** insns/px at 1024 px; µs/frame 460.0 → **216.9** at 1024 px,
112.6 → **55.1** at 256 px, 919.8 → **444.5** at 2048 px.

**`4th` — 1.11x, and it can never be a bulk fill at all.** Its crackle draws a
fresh `random(1)` for *every* pixel, in index order, as an ignition
probability. The RNG is one shared stream, so skipping a draw or reordering
the draws changes every later frame — the loop has to stay per-pixel and pay
one `rgb` + one `setPixel` per pixel. That is still cheaper than the
per-pixel `render` entry it replaces, and on the panel much more so (317–440
Xtensa cycles).

What the frame entry bought beyond that is the bunting: the band colour is
`floor((index + bunting) / stripeWidth) % 3`, which changes once every
`stripeWidth` pixels, not once per pixel. It is now a walking edge — one
divide-floor-modulo per *band* — with the three block colours picked at the
edge. The hoist is the risky part of this conversion, so it was checked across
7 `StripeWidth` values (1, 2, 3, 5, 7, 11, 24) x 4 pixel counts with all five
other controls driven: maxdiff 0 in all 28.

56.2 → **49.3** insns/px at 1024 px; µs/frame 246.1 → **221.2** at 1024 px,
62.8 → **58.5** at 256 px, 495.6 → **445.8** at 2048 px.

Neither pattern runs at 4096 px before or after — `neutronorbit` is three
`pixelCount` arrays (ceiling ~3,411) and `4th` four (ceiling ~2,559), and
neither conversion adds one.

### `swirlpool-2d.js` — the canvas shape, and the `15.99` fudge (2026-09-07)

The last of #405 batch 3, and the batch's biggest win: a 16x16 brightness/hue
canvas pair that `render2D` was resolving once per LED. On the 64x64 panel that
is **4096 VM entries a frame to answer 256 distinct questions**. One
`fillCanvas(hues, 1, vC, 16, 16)` replaces the lot.

The only structural change the conversion forces is where the gamma square
happens. `render2D` did `hsv(hues[idx], 1, b * b)`, but `bright` is persistent
state that decays by 0.94 every frame, so it cannot be squared in place. It is
squared per *cell* into a third `array(16 * 16)` — 256 elements, next to
nothing against the budget, and the reason this conversion (unlike the rest of
the batch) runs perfectly well at 4096 px.

| rig | before ns/px | after ns/px | before µs/frame | after µs/frame | ratio |
|---|---:|---:|---:|---:|---:|
| 4096 px, `--map-grid 64x64` | 44.69 | 10.21 | 183.1 | **41.8** | **4.38x** |
| 1024 px, `--map-grid 32x32` | 46.51 | 15.70 | 47.6 | **16.1** | **2.96x** |
| 256 px, `--map-grid 16x16` | 50.91 | 39.22 | 13.0 | **10.0** | 1.30x |
| 300 px strip (default 18x18 grid) | 51.99 | 41.37 | 15.6 | **12.4** | 1.26x |

16.1 → **1.0** insns/px at 4096 px — the frame is now 3,988 interpreted
instructions no matter how big the fixture is. The 256 px row is the same
lesson `raindrops-2d` taught: when the display is already the size of the
simulation, a conversion buys almost nothing.

**Equivalence, and the one honest delta.** Byte-identical over 60 frames at a
fixed delta and seed on the 16x16, 32x32 and 64x64 coordinate maps and the
60 px strip, undriven and with all four controls driven. On the **300 px and
512 px mapless strips** it is not: 5–11 pixels of 300 differ per frame, up to a
full 255 on a channel. Those strips get the engine's over-provisioned default
`ceil(√n)` grid (18x18 and 23x23), whose cell edges do not line up with the
16-wide canvas, and there the pattern's `floor(x * 15.99)` and `fillCanvas`'s
true nearest `floor(x * 16)` land in different cells — a bright dot on one side
of a canvas boundary and black on the other, hence the 255.

That is not an inference. The pre-conversion file with **only** the sampler
changed to exact nearest (`floor(x * W)`, clamped) is **byte-identical to the
converted pattern on all six rigs**, so the whole difference is the `-0.01`
fudge and nothing else. The fudge was always the approximation — it exists to
keep `x = 1` from indexing off the end — and `fillCanvas` does the clamp
properly, so the converted pattern is the more correct of the two.

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
