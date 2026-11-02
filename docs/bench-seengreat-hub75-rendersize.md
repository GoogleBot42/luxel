# 128x128 `setRenderSize` pass: before/after (2026-11-02)

*A hand-written companion to the generated `docs/bench-report-seengreat-hub75.md`
(that file is rewritten by every `tools/hw-bench.mjs` run). Device: Seengreat unit 2
(192.168.0.142), 2x2 chain = 128x128 = 16384 px, master firmware v0.1.40 on `ota_1`,
JIT on. Method: the same as `tools/patbench.mjs` (push via `/api/code`, 5 s settle,
median of 9 `/api/status` samples 1 s apart). A scratch copy also recorded
`frame_us` and `out_fps`, ran every pattern in one session, and left the pixel count
alone. "Before" is the `origin/master` file and "after" is the edited one.*

Jeremy approved this on 2026-11-02: above 4,096 px, the slowest ambient patterns
render at <= 4,096 cells (64x64 on the chain) and the engine expands them bilinearly.
The edit is a 9-line `bigGrid()` helper called first in `beforeRender`. It is a
no-op at 4,096 px and below: the 32x32 and 64x64 contact sheets are byte-identical
to master.

## Results

| pattern | vm_us before → after | fps / out_fps before → after | 128x128 output vs master | verdict |
|---|---:|---:|---|---|
| Portal | 109,988 → 56,016 | 9 / 9 → 17 / 16 | byte-identical | shipped |
| Topo Map | 132,329 → 76,509 | 8 / 7 → 13 / 12 | byte-identical | shipped |
| Pool Caustics | 106,716 → 76,733 | 9 / 9 → 13 / 12 | byte-identical | shipped |
| Oil Slick | 110,627 → 81,198 | 9 / 9 → 12 / 12 | byte-identical | shipped |
| Kaleidoscope | 108,486 → 77,582 | 9 / 9 → 12 / 12 | byte-identical | shipped |
| Reaction Diffusion | 123,030 → 103,531 | 8 / 8 → 10 / 9 | byte-identical | shipped (48x48 render size) |
| Interference | 108,010 → 71,013 | 9 / 9 → 14 / 13 | soft rings, indistinguishable | shipped |
| Wavy Bands | 94,043 → 66,987 | 11 / 10 → 14 / 14 | indistinguishable | shipped |
| Geometry Morph | 87,607 → 68,746 | 11 / 11 → 14 / 14 | silhouette edge marginally softer (already a bilinear 32x32 SDF) | shipped |
| HUD Ticker | 162,045 → 75,471 | 6 / 6 → 13 / 12 | half the lanes at 2x, soft digits | **left alone** (#1023) |
| Meadow | 135,005 → 90,850 | 8 / 7 → 11 / 10 | grass blades fat and blurred | **left alone** (#1023) |
| Lotus Bloom | 82,044 → 67,308 | 12 / 9 → 14 / 14 | jagged petal rims, seed head blurs | **left alone** (#1023) |
| Quasicrystal | not benched | | wavelength is in pixels, so it zooms 2x; relies on 128-px detail | **left alone** (#1023) |

Interference measured 108,010 and 108,496 µs before (two runs) and 70,517 and
71,013 µs after. Lotus Bloom's "before" spread was 93 ms, because its flower cycle
is stateful. Reaction Diffusion at 64x64 (its 48x48 canvas bilinear onto 64, then
expanded) measured 122,404 µs, so there was no gain. Rendering at the canvas's own
48x48 makes `paintCanvas` 1:1 and is the shipped form.

## Why the byte-identical rows got faster

Six of these patterns already drew a <= 64x64 canvas and let `paintCanvas` upscale
it bilinearly. `paintCanvas` palettises each canvas cell and then runs the same
integer bilinear sampler as the `setRenderSize` expand (`canvas_filtered` in
crates/luxel-core/src/bulk.rs), so moving the upscale into the engine produces the
same bytes. It is still 30–55 ms cheaper per frame, because a full-panel
`paintCanvas` costs more than the expand. Measured with probe patterns, on the same
device and session:

| probe | vm_us | fps / out_fps |
|---|---:|---:|
| empty `renderBulk`, native | 560 | 46 / 46 |
| empty `renderBulk` + `setRenderSize(64, 64, 0)` (nearest) | 13,483 | 40 / 40 |
| empty `renderBulk` + `setRenderSize(64, 64, 1)` (bilinear) | 43,427 | 21 / 21 |
| `paintCanvas` 64x64 → 128x128, bilinear | 59,338 | 16 / 15 |

Two things to take from this. **The bilinear expand alone is ~43 ms on the chain**,
which caps any bilinear `setRenderSize` pattern there at ~21 fps and is now the
largest cost in most of the rows above. The tiled chain takes the per-pixel scan,
not `bilinear_rows` (#1022). **Any pattern that `paintCanvas`/`fillCanvas`es a
canvas coarser than a 16384-px layout gains by rendering at the canvas size
instead**, and the output is identical.

## Control units under a render size

`gridWidth()` reads the SMALL grid once the size is set, so a control denominated
in fixture pixels changes meaning. Pool Caustics' `cellPx` and 8 px tiles, Oil
Slick's "half resolution above 32 px" canvas rule, HUD Ticker's lane height and
Quasicrystal's wavelength would all have doubled. Pool Caustics keeps
`rsK = native / rendered` to stay in fixture pixels, and Oil Slick keeps its canvas
1:1 under a render size. Both stay byte-identical to master.
