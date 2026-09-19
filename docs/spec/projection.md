# Projection specification

Status: v1, tracks `crates/luxel-core/src/projection.rs` and
`Engine`'s frame loop. Implemented once in `luxel-core`, so the firmware, the
mirror (`luxel serve`) and the playground's wasm build agree by construction.

A **Layout** is the rig: how many pixels there are and where they sit. Its
`dims` is 1 (a strip — no map), 2 (a matrix, or a map that plots `x, y`) or 3
(a lattice, or a map that plots `x, y, z`).

A **pattern** has its own dimensionality: 1 for `render(index[, x])`, 2 for
`render2D`, 3 for `render3D`. A `renderFrame` pattern follows the 2D row when
it actually draws in grid space — i.e. it names one of the coordinate/grid-space
bulk builtins (`gridWidth`, `fillRect`, `splat`, `blit`, …), the same signal
the engine already uses to decide whether to fabricate a default grid.
A `renderFrame` that paints only in index space (`fillHSV`, `fade`,
`setPixel`) is a strip pattern and stays one.

When they differ the engine has to decide what coordinates — and how many
render calls — the pattern gets. That decision is the **projection**.

Projection keys off **dims alone**. "Custom map" is a coordinate *source*, not
a dimensionality: a slice or an axis is a coordinate substitution and does not
care whether the pixels sit on a regular grid. Regularity matters for exactly
two things — what by-index means (row-major on a grid, wiring order otherwise)
and how cheaply an along-axis projection can be run (below).

## 1. The table

Options are listed in display order; the **first is the default**, and every
default reproduces the engine's behaviour from before projections existed.

| Layout dims | 1D patterns | 2D patterns (incl. `renderFrame`) | 3D patterns |
|---|---|---|---|
| 1D (strip) | *native* | Middle row `x` · Middle column `y` | Line along `x` · `y` · `z` |
| 2D (matrix or 2D map) | By index `index` · Along x `x` · Along y `y` | *native* | Slice xy `xy` · xz `xz` · yz `yz` |
| 3D (lattice or 3D map) | By index `index` · Along `x` · `y` · `z` | Repeat along z `z` · y `y` · x `x` | *native* |

What each token names:

- **1D pattern, `x`/`y`/`z`** — the Layout axis the strip is laid along. The
  pattern renders ONE strip; it is replicated across the other axes.
- **2D pattern on a strip, `x`/`y`** — the pattern axis the strip walks. `x` is
  the middle row (the pattern's `y` is pinned to 0.5), `y` the middle column.
- **2D pattern on a lattice, `x`/`y`/`z`** — the Layout axis the image is
  repeated (extruded) along; the image occupies the complementary plane.
- **3D pattern on a strip, `x`/`y`/`z`** — the pattern axis the line runs
  along; the other two coordinates are pinned to 0.5.
- **3D pattern on a matrix, `xy`/`xz`/`yz`** — the plane of the cube that is
  sampled; the third coordinate is pinned to 0.5.

`index` means the pixel's index in the Layout's own order — row-major on a
grid, wiring order on a custom map. It is the engine's historical fallback and
changes nothing.

## 2. Wire form

Three fields, one per PATTERN dimensionality, each carrying one of seven
tokens:

```
proj1d = index | x | y | z
proj2d = x | y | z
proj3d = x | y | z | xy | xz | yz
```

A host stores all three and sends only the ones meaningful for its Layout
kind. A value that is not one this (pattern dims, Layout dims) pair offers
falls back to that pair's first option, so one triple survives Layout and
pattern changes without losing the user's other choices — `proj2d=z` on a
strip reads as "middle row", `proj3d=xy` on a strip reads as "line along x".

The FFI codes (`ProjectionMode` discriminants, stable):
`index`=0, `x`=1, `y`=2, `z`=3, `xy`=4, `xz`=5, `yz`=6.

Defaults: `proj1d=index proj2d=z proj3d=xy`.

Where they live:

- **Device default** — `/api/layout` (ticket A4, Gitea #465). Until that lands
  the mirror (`luxel serve`) accepts them as extra tokens on `POST /api/map`
  and reports them from `GET /api/map`; the firmware does not carry them yet.
- **Per-item override** — saved beside a playlist item's or a scene layer's
  control values. A projection is "how this pattern is presented on this
  Layout", so it belongs with the values, never in the pattern source. The
  playlist carries it as a `P <mode>` line following the item's `I` (docs/api.md
  "Playlist"), in the firmware, the mirror and the web client since Gitea
  #470; the scene-layer half is #481. The token goes into the slot matching the
  ITEM'S PATTERN's dims, so `Projection::set(preferred_dims(), mode)` is the
  whole application, and it is installed after the host's own defaults.

## 3. What the pattern sees

Under **by index**, and under every native pair, nothing changes: one render
call per Layout pixel, the Layout's own coordinates, `pixelCount` = the
Layout's pixel count.

Under a **coordinate substitution** (every 2D and 3D pattern row of the table)
the call count and `pixelCount` are unchanged; only which Layout coordinate
feeds which pattern argument changes. An argument the projection does not feed
reads 0.5, exactly like an absent map axis does today. Transforms
(`translate`/`rotate`/`scale`) still apply in PATTERN space, i.e. after the
substitution.

Under an **along-axis projection** of a 1D pattern the engine renders one
strip and replicates it:

- `pixelCount` reads as the strip's length — which is what the pattern's
  author assumed. On a 64×64 matrix, `proj1d=x` means 64 render calls per
  frame instead of 4096, and `pixelCount` is 64.
- The pattern sees **no map** for the duration: `render(index, x)` gets
  `x = index / pixelCount`, the bare-strip convention.
- Every Layout pixel then takes the strip pixel nearest its own coordinate on
  that axis, so a serpentine panel, a rotated map or an irregular cloud all
  replicate correctly — the replicate reads coordinates, never wiring.

The strip's length is the number of cells the Layout has along that axis. It
is known exactly for a procedural W×H grid (`set_grid_map`, the matrix and
panel case) and for a coordinate map that `outpipe::detect_grid` recognised.
Any other Layout — an irregular 2D cloud, any 3D Layout — has no cell count,
so the strip is as long as the Layout and pixels sample it by coordinate: the
same picture, without the saving.

A grid-space `renderFrame` pattern on a 1D Layout gets a **w×1** grid (or 1×h
for middle column) so `gridWidth`/`gridHeight` and the grid-space bulk
builtins still describe the strip. A whole-frame pattern is **never**
strip-rendered — it owns the buffer, so there is no per-pixel strip to
replicate, and a 1D `renderFrame` on a matrix keeps by-index.

One caveat: the pattern's top-level init has already run by the time a host
installs a projection, so a pattern that sizes a buffer with
`array(pixelCount)` at the top level keeps the Layout-sized buffer it
allocated. Hosts should set the projection immediately after building the
engine, before the first frame.

## 4. API

`luxel-core`:

```rust
pub enum ProjectionMode { Index, X, Y, Z, Xy, Xz, Yz }   // #[repr(u8)]
impl ProjectionMode { as_str, as_u8, from_u8, axis }      // + FromStr, Display
pub struct Projection { proj1d, proj2d, proj3d }          // + DEFAULT, new, get, set,
                                                          //   field_name, effective
pub fn projection_options(pattern_dims: u8, layout_dims: u8) -> &'static [ProjectionMode];
pub fn projection_label(mode, pattern_dims, layout_dims) -> &'static str;

impl Engine {
    fn set_projection(&mut self, Projection);
    fn projection(&self) -> Projection;
    fn layout_dims(&self) -> u8;
    fn set_strip_layout(&mut self);                 // install the 1D Layout
    fn effective_projection(&self) -> Option<ProjectionMode>;
    fn effective_geometry(&self) -> EffectiveGeometry;
}
```

UIs build their pickers from `projection_options` + `projection_label` rather
than restating the table: an empty list means the pattern is native and the
row is hidden, a one-entry list is a note rather than a control.

wasm (`crates/luxel-wasm`, wrapped in `web/src/lib/luxel.ts`):

| export | meaning |
|---|---|
| `lx_set_projection(h, one, two, three)` | install the triple, by FFI code |
| `lx_projection(h)` | `proj1d \| proj2d << 8 \| proj3d << 16` |
| `lx_projection_options(pattern_dims, layout_dims)` | JSON `[{mode,code,label}]`, returns the count |
| `lx_effective_geometry(h)` | JSON `{pixelCount,patternDims,layoutDims,w,h,mode,label}` |
| `lx_layout_dims(h)` | the Layout's dims |
| `lx_set_strip_layout(h)` | install the 1D Layout |

`luxel run|bench --proj MODE` applies a mode to the slot matching the
pattern's own dims (`docs/tools.md`).

Tests: `crates/luxel-core/tests/projection.rs` (one per cell of the table),
`crates/luxel-core/src/projection.rs` unit tests (options, labels, string and
code round-trips), `tools/wasm-smoke.mjs` (the FFI and the replicate).

Design: `docs/design/webui-v2/proposal.md` §5.4d. Ticket: Gitea #473.
