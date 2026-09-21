# Projection specification

Status: v1, tracks `crates/luxel-core/src/projection.rs` and
`Engine`'s frame loop. Implemented once in `luxel-core`, so the firmware, the
mirror (`luxel serve`) and the playground's wasm build agree by construction.

A **Layout** is the rig: how many pixels there are and where they sit. Its
`dims` is 1 (a strip — no map), 2 (a matrix, or a map that plots `x, y`) or 3
(a lattice, or a map that plots `x, y, z`).

A **pattern** has its own dimensionality: **0** for a pattern that declares no
geometry at all, 1 for `render(index[, x])`, 2 for
`render2D`, 3 for `render3D`. A `renderFrame` pattern follows the 2D row when
it actually draws in grid space — i.e. it names one of the coordinate/grid-space
bulk builtins (`gridWidth`, `fillRect`, `splat`, `blit`, …), the same signal
the engine already uses to decide whether to fabricate a default grid.
**Dimensionality 0 is "any", and it is native on every Layout.** A pattern
whose only render entry is `renderFrame` painting in index space (`fillHSV`,
`fade`, `setPixel`) names no geometry: it is a field over `pixelCount` that
looks the same on a strip, a panel or a cloud — `library/fairies.js` is the
example. It is **not** a 1D pattern. A 1D pattern is a strip drawn on this
Layout and can be replicated along an axis; a whole-frame pattern owns the
buffer and never is. Collapsing that 0 to 1 gave such a pattern a
`1D · by index` caption, a Projection row and along-x/along-y options that
changed nothing at all (Jeremy, 2026-09-20). So dims 0 offers no options, has
no projection in force, is never flagged incompatible, and the Layout draws it
in its own shape.

`luxel_core::projection::dims()` still normalizes 0 to 1, because 0 and 1
render in the same space and share the `proj1d` storage slot — but
`projection_options`, `compatible` and `projection_label` test the raw 0 first.
Same split in the TypeScript mirror (`normDims` vs
`projectionOptions`/`projectionCompatible`), and `gallery.json`'s advisory
`kind` carries it as `"any"` rather than `"strip"`.

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

| Layout dims | dims 0 (any) | 1D patterns | 2D patterns (incl. grid-space `renderFrame`) | 3D patterns |
|---|---|---|---|---|
| 1D (strip) | *native* | *native* | — | — |
| 2D (matrix or 2D map) | *native* | By index `index` · Along x `x` · Along y `y` | *native* | — |
| 3D (lattice or 3D map) | *native* | By index `index` · Along `x` · `y` · `z` | Repeat along z `z` · y `y` · x `x` | *native* |

**A Layout only ever shows a pattern of its own dimensionality or lower**
(Gitea #538). A strip does not project a 2D or 3D pattern and a plane does not
project a 3D one — it looks terrible, so the choice is not offered: those three
cells are empty everywhere, in the engine, in `/api/layout`, in the mirror and
in the UI's tables. There is no explanatory copy anywhere; the options simply
are not there. What happens when one is activated anyway is §1a.

What each remaining token names:

- **1D pattern, `x`/`y`/`z`** — the Layout axis the strip is laid along. The
  pattern renders ONE strip; it is replicated across the other axes.
- **2D pattern on a lattice, `x`/`y`/`z`** — the Layout axis the image is
  repeated (extruded) along; the image occupies the complementary plane.

`index` means the pixel's index in the Layout's own order — row-major on a
grid, wiring order on a custom map. It is the engine's historical fallback and
changes nothing.

## 1a. Incompatible patterns

A host must not *offer* a pattern the Layout cannot show, but it can still be
*handed* one: a playlist saved before #538, a shared link, a Home Assistant
call, `POST /api/code` from a script. **The engine renders it rather than going
dark.** There is no projection in force (`effective_projection()` is `None`),
so the pattern simply gets the engine's plain fallback coordinates:

- a 2D pattern on a strip sees `x = index / pixelCount`, `y = 0.5` — which is
  exactly what the old "middle row" produced;
- a 3D pattern on a strip sees the same, with `z = 0.5` — the old "line along x";
- a 3D pattern on a plane sees the Layout's `x, y` and `z = 0.5` — the old
  "slice xy";
- a grid-space `renderFrame` on a strip still gets a **w×1** grid, so
  `gridWidth`/`gridHeight` and the grid-space bulk builtins describe the strip
  instead of nothing. It owns the buffer, so there is no per-pixel fallback to
  give it.
- a 2D-only pattern that reaches an engine with no Layout installed keeps the
  fabricated ceil(√n) grid it has always had.

In other words the removed cells' *first* options survive as the fallback, and
only the removed *choices* are gone. Upgrading a device therefore changes the
picture only for someone who had explicitly picked middle-column, line-along-y
or a non-xy slice.

So that a UI can hide or flag such a pattern instead of showing it as normal,
the engine reports the fact:

- `luxel_core::projection::compatible(pattern_dims, layout_dims)` — the rule,
  one line;
- `Engine::effective_geometry().compatible` and the wasm
  `lx_effective_geometry`'s `"compatible"`;
- **`GET /api/status` → `geom.compatible`** (docs/api.md), on the firmware and
  the mirror alike. It reads the LAYOUT's dimensionality, not `geom.dims`: a
  `"source":"default"` geometry is the engine papering a fabricated grid over a
  bare strip, and that is precisely the case to flag.

## 2. Wire form

Three fields, one per PATTERN dimensionality, each carrying one of seven
tokens:

```
proj1d = index | x | y | z      ← in force on a 2D or 3D Layout
proj2d = x | y | z              ← in force on a 3D Layout only
proj3d = —                      ← never in force (#538)
```

A host stores all three and sends only the ones meaningful for its Layout
kind. A value that is not one this (pattern dims, Layout dims) pair offers
falls back to that pair's first option, so one triple survives Layout and
pattern changes without losing the user's other choices.

Since #538 `proj3d` can never be in force (a 3D pattern is native to a 3D
Layout and is not shown on a smaller one) and `proj2d` only on a 3D Layout.
Both nevertheless stay in the grammar and in the stored triple, and
`POST /api/layout` still **accepts** a `proj2d`/`proj3d` line whatever the
kind — it is simply not in force. That is deliberate: `Layout::to_wire`
persists all three lines, so a parser that rejected them would drop every
device upgraded past #538 back to its board default on the first boot. A
value with nowhere to apply is ignored, not an error.

Likewise the three plane modes' FFI codes (`xy`/`xz`/`yz`) stay **reserved**
rather than being reused: a `proj3d xy` written by an older build must keep
parsing.

The FFI codes (`ProjectionMode` discriminants, stable):
`index`=0, `x`=1, `y`=2, `z`=3, `xy`=4, `xz`=5, `yz`=6.

Defaults: `proj1d=index proj2d=z proj3d=xy`.

Where they live:

- **Device default** — `/api/layout`'s `proj1d/2d/3d` lines (ticket A4, Gitea
  #465), persisted, applied to the running engine on the next frame.
- **Running-pattern override** — `POST /api/layout` `proj <mode|default>`
  (Gitea #598), on the firmware and the mirror. It installs the mode in the
  slot for the RUNNING pattern's dimensionality and is **not persisted**: the
  next activation, playlist item or `/api/code` push starts from the defaults
  again. That is what a console's editor override is — a property of the
  working copy, not of the rig — so the editor posts it on every pick and
  re-posts it after every live code push, because a push rebuilds the
  device's engine from the defaults. Before #598 an editor override reached
  the local preview engine only, which is what #538's "the per pattern
  override isn't applied live" was.

- **Per-item override** — saved beside a playlist item's or a scene layer's
  control values. A projection is "how this pattern is presented on this
  Layout", so it belongs with the values, never in the pattern source. The
  playlist carries it as a `P <mode>` line following the item's `I` (docs/api.md
  "Playlist"), in the firmware, the mirror and the web client since Gitea
  #470; the scene-layer half is #481. The token goes into the slot matching the
  ITEM'S PATTERN's dims, so `Projection::set(preferred_dims(), mode)` is the
  whole application, and it is installed after the host's own defaults.
  Order against a map install does not matter: `Engine` writes
  `self.projection` only in `set_projection`, and `set_map`/`set_grid_map`
  re-derive the plan FROM the stored triple — so the mirror's
  map-then-projection and the firmware's projection-then-map (its map is
  re-applied after the message queue drains) reach the same engine.

## 3. What the pattern sees

Under **by index**, and under every native pair, nothing changes: one render
call per Layout pixel, the Layout's own coordinates, `pixelCount` = the
Layout's pixel count.

Under a **coordinate substitution** (the 2D-pattern-on-a-lattice row, the only
one left) the call count and `pixelCount` are unchanged; only which Layout
coordinate feeds which pattern argument changes. An argument the projection
does not feed reads 0.5, exactly like an absent map axis does today.
Transforms (`translate`/`rotate`/`scale`) still apply in PATTERN space, i.e.
after the substitution. An *incompatible* pattern (§1a) takes the same path
with no substitution at all: every axis the Layout does not have reads 0.5.

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

A grid-space `renderFrame` pattern on a 1D Layout gets a **w×1** grid so
`gridWidth`/`gridHeight` and the grid-space bulk builtins still describe the
strip. That pairing is incompatible (§1a) rather than a choice, so there is no
1×h transpose any more. A whole-frame pattern is **never** strip-rendered — it
owns the buffer, so there is no per-pixel strip to replicate. That is exactly
why an index-space `renderFrame` is dims **0** and not 1: there is no
projection for a host to offer it, so it must not be offered one.

A `renderFrame` pattern that genuinely IS positional — one where the index is
a place along a strip, e.g. `library/bulk-comet-trails.js` or
`library/rainbow-comet.js` — is classed dims 0 too, and so is shown by index
on a panel rather than replicated along an axis. Serving those means rendering
a w-long frame and replicating it, which needs the projection installed BEFORE
program init (`array(pixelCount)` is sized there); that is Gitea #628,
deliberately out of scope here.

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
pub const fn compatible(pattern_dims: u8, layout_dims: u8) -> bool;   // #538

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
row is hidden — or that it is INCOMPATIBLE (§1a), which `compatible` tells
apart and the surface hides or flags. A one-entry list is a note rather than
a control.

wasm (`crates/luxel-wasm`, wrapped in `web/src/lib/luxel.ts`):

| export | meaning |
|---|---|
| `lx_set_projection(h, one, two, three)` | install the triple, by FFI code |
| `lx_projection(h)` | `proj1d \| proj2d << 8 \| proj3d << 16` |
| `lx_projection_options(pattern_dims, layout_dims)` | JSON `[{mode,code,label}]`, returns the count |
| `lx_effective_geometry(h)` | JSON `{pixelCount,patternDims,layoutDims,w,h,mode,label,compatible}` |
| `lx_layout_dims(h)` | the Layout's dims |
| `lx_set_strip_layout(h)` | install the 1D Layout |

`luxel run|bench --proj MODE` applies a mode to the slot matching the
pattern's own dims (`docs/tools.md`).

Tests: `crates/luxel-core/tests/projection.rs` (one per cell of the table),
`crates/luxel-core/src/projection.rs` unit tests (options, labels, string and
code round-trips, and that no Layout offers a bigger pattern),
`crates/luxel-core/src/caps.rs` (`geom.compatible`), `tools/wasm-smoke.mjs`
(the FFI and the replicate), `tools/serve-e2e.mjs` (`geom.compatible` over
the wire), `web/tests/geometry.test.mjs` (the TS tables, cell by cell
against wasm).

Design: `docs/design/webui-v2/proposal.md` §5.4d. Tickets: Gitea #473, and
#538 for the "a Layout never shows a bigger pattern" rule.
