# Luxel engine + firmware constraints — survey for a compositor & text builtin

Read-only survey of worktree `/home/googlebot/workspace/pixler-webui-v2` at `56f7e80`.
Nothing was built, modified, or run against hardware. All paths absolute.

---

## 1. FRAME PIPELINE

### The render task loop

`firmware/src/main.rs:1180` is the top of `loop {}` in the render task. One iteration:
drain `MSG_QUEUE` — `Msg::{Code, Library, Crossfade}`, where engines are built and dropped
(`main.rs:1181`); apply a dirty pixel map (`main.rs:1477-1489`); sensors and injected
events (`main.rs:1492-1508`); live DDP/E1.31 input, which assembles into the sink's staging
buffer and short-circuits the engine entirely (`main.rs:1511-1530`); **render**
(`main.rs:1531-1594`); per-second timing publish (`main.rs:1634-1658`); 250 ms
`vars`/`readouts` publish (`main.rs:1659-1671`); playlist pre-flight, one item per frame
(`main.rs:1679`); `pipeline::pace(spent)` (`main.rs:1715`).

### `Engine::frame`

`crates/luxel-core/src/engine.rs:1018`. `timeScale` scaling and the `setFrameRate` hold
first (`engine.rs:1022-1054`) — under the cap it returns the *previous* `&self.pixels` and
runs no pattern code. Then `drive(None)` (`engine.rs:1060`), a resumable state machine over
`RunStage::{Before, Pixel(i), Frame}`. `Before` runs `beforeRender(delta)` and
**re-resolves the render entry every frame** (`engine.rs:1173`) so late-bound
`export var render2D` works. The non-debug non-map per-pixel path escapes into the tight
loop `render_pixels` (`engine.rs:1220`), which hoists the entry, argc and whether the
pattern reads coordinates at all. The whole-frame path (`engine.rs:1071-1087`) `mem::take`s
`self.pixels` into `vm.frame` (`frame_buffer_out`, `engine.rs:1286`) — **a move, never a
copy** — and takes it back on every exit path (`frame_buffer_in`, `engine.rs:1293`). Every
completion ends in `post_chain()` + `finish_frame()` (`engine.rs:1185-1189`, `1207-1211`,
`1283`).

### The engine's own post-process chain

`Engine::post_chain` (`engine.rs:1401`), fixed order, all pattern-controlled:
**palette remap** via a cached 256-entry luma→color LUT (`engine.rs:1406-1411`,
`outpipe.rs:151`) → **blur** (`engine.rs:1414-1428`) → **glow** (`engine.rs:1429-1436`) →
**gamma**, 3 table lookups/px (`engine.rs:1437-1445`). Both spatial stages fork on
geometry: `blur_frame_grid`/`glow_frame_grid` (`outpipe.rs:387`/`402`) when `self.grid` is
`Some` **and** `grid.len() == pixels.len()`, else the 1D index-space versions
(`outpipe.rs:177`/`315`). Each stage costs one comparison per frame when off. The grid is a
6-byte `Copy` descriptor (`GridMap { w: u16, h: u16, serpentine: bool }`,
`outpipe.rs:208`), never a neighbour table.

### The *device* post-process chain (a second, separate chain)

`firmware/src/main.rs:769` `apply_outpipe` — the Settings-page chain, which **composes on
top of** the engine's: palette remap → blur → glow → color-order permute → gamma LUT →
power cap, into a scratch `Vec` (`main.rs:823-855`, `outpipe::apply` at `outpipe.rs:466`).
It returns the input frame untouched when every knob is off (`main.rs:794-796`).
`PowerModel::Hub75 { scan: PANEL_ROWS/2 }` on panel boards (`main.rs:783-785`).

### Where the frame goes

`firmware/src/pipeline.rs` owns the sink. Two builds:

- **direct** (`DirectSink`, `pipeline.rs:128`) — `PipeState::run` (`pipeline.rs:103`) does
  the `/api/pixels` preview copy (`shared::set_pixels`, `shared.rs:512`), then
  `apply_outpipe`, then `out.write_frame(wire, b5)`. Returns `(pipe_us, out_us)`.
- **pipelined** (`cfg(pipelined)`, dual-core; `RenderSide` at `pipeline.rs:335`) — the
  render task on core 1 copies the frame into a single travelling `Vec<[u8;3]>` and hands
  it to `output_task` on core 0 (`pipeline.rs:537`), which runs `apply_outpipe` +
  `write_frame`. `pipe_us`/`out_us` are published from core 0
  (`pipeline.rs:625-648`). The frame period becomes `max(frame_us, pipe_us + out_us)`
  (`docs/firmware.md:612-620`).

Output drivers: `firmware/src/output.rs:51` `OutputDriver` trait (`set_protocol`,
`resize`, `write_frame(&[[u8;3]], brightness5) -> bool`, `ready_for_frame`,
`paces_frames`); `SpiStripOutput` (`output.rs:145`) for SK9822/WS281x;
`firmware/src/hub75.rs:88` `Hub75Output` with `DmaFrameBuffer<NROWS, PANEL_COLS, PLANES>`,
`PANEL_COLS = PANEL_ROWS = 64` (`hub75.rs:49-50`), `PLANES = 7` (`hub75.rs:58`),
LCD_CAM clock 30 MHz (`hub75.rs:86`). Compose is `luxel_hub75::pack::<NROWS, COLS,
PLANES>` (`hub75.rs:478`) with a per-pixel `set_pixel` fallback (`hub75.rs:481-496`).

### Where a compositor slots in

**Before the post-process chains, after the per-layer render — i.e. exactly where the
crossfade blend already sits (`main.rs:1573-1586`).** Reasons, all load-bearing:

- The engine's `post_chain` is *per pattern* and pattern-controlled: a layer's own
  `setBlur`/`setGlow`/`setOutputPalette` must apply to that layer's pixels before they are
  composited, or "blur this layer" becomes "blur the composite".
- `apply_outpipe` is the *installation's* look (`main.rs:763-768` states this explicitly)
  and must run once, on the finished composite — it also does the color-order permute and
  the power cap, both of which are wire-level and meaningless per layer.
- The staging buffer already exists and is free on the pipelined board: `sink.stage()`
  (`pipeline.rs:159`/`361`) **is** the travelling hand-off buffer, so `emit_staged` swaps
  rather than copies (`pipeline.rs:406-412`). A compositor writing into `stage` costs the
  pipeline nothing extra — this is exactly how the crossfade avoids a copy
  (`main.rs:1570-1572`).

### Frame buffer type and size

`Vec<[u8; 3]>` everywhere — **3 bytes/px, 8-bit sRGB-ish, post-quantize**
(`Engine::pixels` at `engine.rs:134`, `Vm::frame`, `PipeState::buf`, the travelling
buffer, `shared::PIXELS`). At 4096 px that is **12,288 B** per buffer.
`docs/boards.md:1015-1021` inventories them at 4096 px: engine frame + crossfade blend +
outpipe wire buffer + `/api/pixels` snapshot ≈ 12 KB each, ~48 KB together, on top of the
panel's two ~28 KB DMA framebuffers. On a pipelined build the `/api/pixels` snapshot is
deleted and the travelling buffer serves as the preview (`docs/firmware.md:852-860`), so
the pipeline itself is net-free.

There is no alpha channel and no float frame anywhere. `quantize()` happens at the VM
boundary (`engine.rs:1204`, `engine.rs:1275`).

### The crossfade — the existing multi-engine precedent

**Yes: two `Engine`s are resident simultaneously for the duration of a fade.**

- State: `let mut prev: Option<Engine>` plus `blend_start: Instant` and `blend_ms: u32`
  (`main.rs:1156-1158`).
- Started by `Msg::Library { id, ms }` (`main.rs:1323`) — playlist advance, activate, MQTT
  select, boot resume — and by `Msg::Crossfade { env, ms, id }` (`main.rs:1418`), the
  ad-hoc push path. The playlist supplies `ms` from its `X` field
  (`playlist.rs:44`, parsed at `playlist.rs:154`, sent at `playlist.rs:329`).
- Residency: `prev = engine.take(); blend_start = Instant::now(); blend_ms = ms`
  (`main.rs:1387-1390`, `main.rs:1450-1453`) — only when `ms > 0 && engine.is_some()`.
- Per-frame blend (`main.rs:1572-1586`): progress `t` = `elapsed*65536/blend_ms`, capped
  at 65536 (`main.rs:1556-1561`); the **incoming** engine's frame is copied into
  `sink.stage()`, then `prev.frame(delta)` runs and is blended on top with
  `blend_px` — a 16.16 lerp per channel, `main.rs:750-753`. **Both engines render every
  frame of the fade**; both costs land in `vm_us` (`docs/firmware.md:600-602`).
- Teardown: `drop_prev(&mut prev)` (`main.rs:958`) — `*prev = None` **then**
  `patterns::unpin_prev()`, order load-bearing. Called on fade completion
  (`main.rs:1589`), at the top of every new load (`main.rs:1332`, `main.rs:1422`
  — "never THREE"), and on a failed protocol resize (`main.rs:1310`).
- Memory accounting: a crossfade **deliberately does not record `engine_heap`**
  (`main.rs:1344-1346`, `main.rs:944-948`) because the outgoing engine is still alive, so
  `/api/status`'s `engine_heap` is the last clean measurement. The incoming engine is
  still budget-checked by `try_budgeted_engine` (`main.rs:924`), which measures
  `HEAP.free()` **after** the build with `prev` resident — so a fade is exactly the case
  where the `RUNTIME_FLOOR` check is tightest. Flash side: the outgoing pattern may be
  executing in place from its mapped store extent, so `patterns::pin_prev_from_running()`
  pins that extent for the fade (`main.rs:1348`, `main.rs:1426`).

So the precedent is real but narrow: **N=2, bounded in time, one blend mode (lerp), no
per-layer transform, and the second engine is never budgeted as a steady-state cost.**

---

## 2. MEMORY BUDGET

### The rules (`crates/luxel-core/src/budget.rs` — shared by firmware AND the wasm playground)

| constant | value | line |
|---|---|---|
| `RUNTIME_FLOOR` | 20 KiB | `budget.rs:29` |
| `BUDGET_HEADROOM` | 4 KiB | `budget.rs:33` |
| `MIN_ARRAY_BUDGET` | 16 KiB | `budget.rs:38` |
| `TIGHT_PERCENT` | 85 | `budget.rs:45` |
| `ARENA_RESERVE` | 256 KiB | `budget.rs:79` |
| `BYTES_PER_ELEMENT` | 8 (`size_of::<Value>()`, statically asserted) | `budget.rs:105-106` |
| `vm::DEFAULT_ARRAY_BUDGET` | 10,236 elements (the PB-compat ledger) | referenced `budget.rs:98` |
| `vm::MAX_ARENA_SLOTS` | 2,559 | `docs/boards.md:1131` |

- `array_budget(heap_free) = max(heap_free − 24 KiB, 16 KiB)` (`budget.rs:60`).
- `external_array_budget(heap_free, arena_free) = max(arena_free − 256 KiB,
  array_budget(heap_free))` (`budget.rs:88`).
- `load_base(heap_free, engine_heap) = heap_free + engine_heap` (`budget.rs:131`) — the
  outgoing engine is dropped before the incoming one is measured, so the editor must add
  it back. **A crossfade breaks this assumption**, and the code says so.
- `load_headroom(base_free) = base_free − RUNTIME_FLOOR` (`budget.rs:146`).
- `fit(resident, peak, base_free)` → `Fits` / `Tight` / `Over` (`budget.rs:168`).
- Enforcement: `try_budgeted_engine` (`main.rs:924`) builds, reads `HEAP.free()`, and
  **drops the engine** if it is under `RUNTIME_FLOOR`, reporting the user-facing
  "pattern too large for this device" vmerr (`main.rs:963`).
- `array_budget_now()` (`main.rs:876`) routes to the external arena on `psram-arena`.

### What an Engine costs at rest

`Engine::from_program_budgeted_at_ext` (`engine.rs:275`) allocates, in order: `Vm::new`
(globals, operand stack, locals, arena slot vector); zero-filled sensor arrays if exported
(`engine.rs:291-306`); top-level init, which is where pattern arrays land; the `controls`
Vec (`engine.rs:333-350`); and `pixels: vec![[0u8;3]; pixel_count]` (`engine.rs:353`).
Later, optionally: `gamma_lut` 256 B, `remap_lut` 768 B, `map_coords` 12 B/px (coordinate
maps only, not procedural grids). The `Program` is owned **or borrowed**: on the library
path the bytecode is deserialized in place from the flash mapping
(`deserialize_lean_static`, `main.rs:1355`), so a library pattern's code costs no heap at
all; the ad-hoc `POST /api/code` path owns its code words.

Measured resident costs (`/api/status` `engine_heap`, `budget.rs:216-280` and
`docs/boards.md:1152-1156`):

| case | resident engine |
|---|---|
| rainbow default @4096 px (S3 panel) | ~5,000 B (`budget.rs:270`) |
| Aurora 2D @4096 px, arrays in PSRAM | 14,068 B (`docs/boards.md:1168`) |
| 2048-element array probe @4096 px, DRAM | 29,746 B → 13,683 B with the PSRAM arena (`docs/boards.md:1152-1156`) |
| gallery median @300 px | 13,000–22,000 B (`budget.rs:238-247`) |
| "Infinite Snake v2" @300 px | 32,454 B (`budget.rs:250`) |
| "2D Fireworks Fade" @4096 px | 43,838 B — **Over** (`budget.rs:290`) |

### The PSRAM arena (Gitea #253)

`firmware/src/psram.rs`. 8 MB octal PSRAM, Seengreat S3 only, `psram-arena` feature.

- **What lives there:** `ArrRepr::Owned` element storage — pattern arrays — and nothing
  else (`psram.rs:11-16`, `docs/boards.md:1107-1112`).
- **What must stay internal** (`psram.rs:17-31`): HUB75 DMA framebuffers and bitplane
  tables (DMA cannot reach PSRAM at all); the engine's per-frame pixel buffer; the
  pipeline's travelling frame; the crossfade stage buffer; the strip output buffer; the
  VM's operand stack, locals, globals and `Vm::arrays` slot vector; everything the WiFi
  blob mallocs.
- **The API is a hook, not an allocator you can call**: `arena::install(alloc, dealloc)`
  (`arena.rs:58`) stores two `unsafe fn` pointers; `ArenaAlloc` (`arena.rs:73`) is what
  `ArrVec<T>` (`arena.rs:106`) allocates through. Contract (`arena.rs:17-28`): once,
  before any `Vm` exists, never undone, `dealloc` must accept pointers from *either* heap.
  There is **no general-purpose "give me N bytes of PSRAM" call** — a compositor's extra
  frame buffers could not use it as written, and per `psram.rs:20-22` should not anyway.
- It is a **second, separate `EspHeap`** (`psram.rs:88`), so `esp_alloc::HEAP.free()` and
  therefore `RUNTIME_FLOOR` keep their old meaning; `/api/status` reports `psram_free` /
  `psram_total` on this board only. Measured cost: +0.16 % of VM time for a 2048-element
  array read 4096×/frame (`docs/boards.md:1150-1158`).

### Rough table — what a second resident engine costs

Per-engine fixed cost is the pixel buffer (3 B/px) plus the program/globals/arrays. Using
the measured corpus above:

| pixel count | pixel buffer (3 B/px) | typical light pattern (rainbow-class) | typical heavy pattern | second engine total, light → heavy |
|---:|---:|---:|---:|---:|
| 300 | 900 B | ~13–18 KB (`budget.rs:238-247`) | ~32 KB (`budget.rs:250`) | **~14 KB → ~33 KB** |
| 1024 | 3,072 B | ~16–21 KB | ~35 KB | **~19 KB → ~38 KB** |
| 4096 | 12,288 B | ~5 KB (rainbow, no arrays, `budget.rs:270`) to ~22 KB | 43.8 KB (`budget.rs:290`) | **~17 KB → ~56 KB** |

Extra buffers per layer, beyond the engine:

| buffer | 300 px | 1024 px | 4096 px |
|---:|---:|---:|---:|
| one extra RGB frame (`[u8;3]`) | 900 B | 3,072 B | 12,288 B |
| one alpha/coverage plane (`u8`) | 300 B | 1,024 B | 4,096 B |
| RGB + alpha per layer | 1,200 B | 4,096 B | 16,384 B |

### Which tiers could afford N layers

Idle `heap_free`, measured:

| board | idle `heap_free` | source |
|---|---:|---|
| Athom / classic ESP32 @60 px | 104,832 B | `docs/boards.md:1528`, `budget.rs:274` |
| Athom, `small-chip` + WiFi tuning | 125,460 B | `docs/boards.md:1514-1517` |
| Seengreat S3 @4096 px, rainbow resident | 51,704 B | `budget.rs:268`, `docs/boards.md:611` |
| Seengreat S3 @4096 px under a bulk pattern | 38,988–51,488 B | `docs/boards.md:1357-1363` |
| Seengreat S3, with the device blur+glow chain on | 26,928 B (−12.3 KB, Gitea #446) | `docs/bulk-render.md:1191-1194` |

Against `RUNTIME_FLOOR` = 20 KiB, the headroom for *layers* is `heap_free +
engine_heap − 20 KiB`:

- **Classic ESP32 / C3 at ≤300 px (104 KB idle):** headroom ~85 KB. **N=3 comfortably,
  N=4 for light patterns** (4 × ~18 KB = 72 KB). Buffers are negligible at this size.
- **Classic ESP32 at 1024 px:** headroom ~80 KB; **N=2 safe, N=3 tight** — and the WS2812
  encode buffer scales with pixel count, which is why strip boards cap at 2048
  (`docs/boards.md:991-994`).
- **Seengreat S3 @4096 px, no PSRAM arena:** headroom ~36 KB (`budget.rs:269`). A second
  rainbow-class engine (~17 KB) fits; a second heavy one does not. **N=2 only, cheap
  layers only** — already the crossfade's regime.
- **Seengreat S3 @4096 px with the arena:** arrays leave DRAM, so a layer costs program +
  globals + the 12 KB pixel buffer (Aurora 2D: 19,748 → 14,068 B, `docs/boards.md:1168`).
  Headroom is still ~36 KB: **N=2 solid, N=3 plausible for array-heavy/code-light layers**,
  N=4 not without moving the per-layer pixel buffers — which `psram.rs:20-22` forbids. The
  binding constraint at 4096 px is **the 12 KB RGB buffer per layer in internal DRAM**, not
  the arrays. The *program* ceiling is untouched by PSRAM (`docs/boards.md:1178-1185`).

---

## 3. TIME BUDGET

### The panel's clock

| quantity | value | source |
|---|---:|---|
| rescan period @30 MHz, 7 planes | **8.67 ms (115.3 Hz)** | `docs/boards.md:1702-1706`, `hub75.rs:66-76` |
| HUB75 compose (`out_us`) @4096 px, current #329 packer | **2,166–2,221 µs** | `docs/boards.md:1983` |
| compose before #329 (per-pixel path) | 7,224–7,272 µs | `docs/boards.md:1982` |
| core-0 slack per rescan | **1.3 ms → 6.5 ms** after #329 | `docs/boards.md:1986` |
| `pipe_us` @4096 px, chain OFF | **3–81 µs** | `docs/boards.md:1211-1217`, `docs/bulk-render.md:1171` |
| `pipe_us` @4096 px, blur 50 | 4,556 µs (~1.1 µs/px) | `docs/bulk-render.md:1173` |
| `pipe_us` @4096 px, blur 50 + glow 60 | **8,801 µs — over the 8.66 ms rescan** (#446) | `docs/bulk-render.md:1174-1189` |
| hand-off memcpy @4096 px | ~50 µs | `docs/firmware.md:846-847` |
| non-vsync pacing floor | 8 ms → 125 fps cap | `pipeline.rs:654-660` |

### Render cost per pixel, Xtensa

| case | µs/px | source |
|---|---:|---|
| **empty `render(index){}` @4096 px, S3** | **1.85** (7,591 µs/frame — the dispatch floor, ~440 cycles) | `docs/boards.md:1210-1225` |
| one `rgb()` per pixel, S3 | 3.42 | `docs/boards.md:1211` |
| `rainbow` per-pixel, S3 | 4.72–4.74 | `docs/boards.md:1357`, `docs/firmware.md:717` |
| `bulk-rainbow` (`renderFrame` + `fillHSV`) | **0.677** (7.0× faster) | `docs/boards.md:1358` |
| `bulk-comet-trails` | **0.105** (431 µs/frame) | `docs/boards.md:1359` |
| `bulk-sprite-scroll-2d` (a `blit` per frame) | **1.251** (5,125 µs) | `docs/boards.md:1361` |
| `bulk-canvas-ripples-2d` | 2.417 | `docs/boards.md:1362` |
| `bouncing-balls-2d` per-pixel | 10.049 | `docs/boards.md:1380` |
| `ripples-2d` per-pixel | 57.227 | `docs/boards.md:1382` |
| whole-library median @4096 px, 225 patterns | **111,211 µs/frame, fps median 9** | `docs/perf-sweep-s3.md:10-12` |
| classic ESP32 (Athom) rainbow @256 px | 6.18 | `docs/firmware.md:715` |
| classic ESP32, `kaleidoscope-2d`, with IRAM placement | 150.20 (626.23 without) | `docs/firmware.md:714-716` |

Two caveats that shape any design here:

- **The `renderFrame` interpreted walk is itself +2.101 µs/px ≈ 504 cycles on Xtensa —
  slower than the native per-pixel entry it replaces** (`docs/bulk-render.md:1086-1108`).
  `renderFrame` wins only via hoisting and native bulk ops, never by loop shape.
- **`vm_us` is dominated by I-cache layout, not dispatch** (`docs/firmware.md:641-660`,
  Gitea #312/#318): a 1.3 KB code move was worth −46.5 % on one pattern and +38 %/+57 % on
  others. Any new builtin (a text renderer) must be measured with `tools/patbench.mjs` on
  a real pattern, not `tools/opbench.mjs`, and the result is per-chip.

### What this says about two patterns per frame at 64×64

Per-rescan budget is 8.67 ms; core 0 needs 2.2 ms for compose and ~50 µs for the
(off) outpipe, leaving ~6.5 ms of core-0 slack. Core 1 has the whole rescan period.

- **Two per-pixel interpreted layers: no.** The dispatch floor alone is 7.6 ms per layer
  *before any pattern body*, so two empty per-pixel layers = ~15.2 ms → a ~65 fps ceiling,
  and any real pattern body pushes it far below the rescan. The library median layer is
  111 ms/frame — one such layer already runs at 9 fps.
- **Two bulk (`renderFrame`) layers: comfortably yes.** 0.1–2.4 µs/px is 0.4–9.9 ms per
  layer; two cheap bulk layers (comet-trails + sprite-scroll = 0.43 + 5.1 ms) fit inside
  one rescan. Four bulk layers at `bulk-rainbow` cost (2.77 ms) is 11 ms — over one
  rescan, under two.
- **The compositing blend itself is cheap**: the crossfade's `blend_px` is 3 multiply-adds
  per pixel; at 4096 px the whole blend is well under 1 ms (compare `palette_remap_frame`,
  a 3-multiply luma + table lookup per pixel, folded into a `pipe_us` of 4.5 ms *for a
  blur*, which is 6 multiply-adds per pixel per pass).
- **The device post-process chain is the real competitor for the window**, not the
  compositor: blur+glow alone is 8.8 ms and already overruns the rescan (#446).
- Watch Gitea **#367**: `out_us` is *not* content-independent in practice — it tracks how
  hard core 1 is working (3.2 ms under a cheap pattern, 6.5 ms under `ripples-2d`),
  suspected memory-bus contention (`docs/boards.md:1395-1404`). A compositor that saturates
  core 1 will push the compose up by ~2.5 ms/frame, i.e. it shifts the panel's compose
  ceiling by nearly 2×.

---

## 4. GEOMETRY KNOWLEDGE

### Where geometry is known

| layer | what it knows | file:line |
|---|---|---|
| board | `PANEL_COLS`/`PANEL_ROWS` = 64/64, compile-time const-generic | `hub75.rs:49-50` |
| board | `MAX_PIXELS` per board (2048 strip / 4096 panel) | `docs/boards.md:988-1000` |
| firmware | `devicemap::MapData::{Coords{dims,coords}, Grid{w,h}}` — the persisted device map | `devicemap.rs:36-39` |
| firmware | `board_default()` — a `hub75` board IS a `PANEL_COLS × PANEL_ROWS` grid | `devicemap.rs:65-77` |
| firmware | `devicemap::init()` — flash blob, else board default, else nothing | `devicemap.rs:254` |
| engine | `Engine::set_map(dims, coords)` / `set_map_vec` — normalizes per axis, runs grid detection | `engine.rs:662`, `engine.rs:676` |
| engine | `Engine::set_grid_map(w, h)` — **procedural**, zero heap, `MapData::grid` | `engine.rs:432` |
| engine | `Engine::set_default_grid_map()` — ceil(√n) square grid, installed only when the pattern is 2D/3D-only | `engine.rs:411`, applied at `engine.rs:383-403` |
| engine | `Engine::grid() -> Option<GridMap>` — the `Copy` 6-byte descriptor | `engine.rs:629` |
| engine | `Engine::installed_map() -> Option<&MapData>` | `engine.rs:625` |
| engine | `Engine::preferred_dims() -> u8` (0/2/3) — what the pattern *wants* | `engine.rs:760` |
| outpipe | `detect_grid(dims, coords) -> Option<GridMap>` — recovers a W×H grid from a coordinate map | `outpipe.rs:255` |
| VM | `vm.frame_grid: Option<GridMap>` — what the bulk/grid builtins read | set at `engine.rs:437`, `engine.rs:691` |

### Is there ONE source of truth?

**No — there are three, and they can disagree.**

1. **`devicemap::MAP`** (`devicemap.rs:59`) is the device's persisted intent: a coordinate
   blob, a `grid w h`, or nothing. Serialized under `patterns::MAP_KEY` with a `GRID_TAG =
   0x80` discriminator (`devicemap.rs:57`, `devicemap.rs:147`).
2. **`Engine.grid`** (`engine.rs:184`) is what the *post-process chain and the bulk
   builtins* actually use, and it is `Some` in cases the device map knows nothing about —
   notably `set_default_grid_map()`, installed automatically for a pattern that only
   exports `render2D`/`render3D`, or `renderFrame` + a coordinate-using bulk op
   (`engine.rs:383-403`, `uses_coordinate_bulk_op` at `engine.rs:1303`).
3. **`Engine.preferred_dims()`** (`engine.rs:760`) is what the *pattern* wants, which is
   a third, independent axis.

So "device geometry kind" is a derived quantity: a strip board with a `render2D`-only
pattern is running on a fabricated √n grid that `/api/map` will report as
`{"installed":false}`.

### What the API exposes

- **`GET /api/map`** (`server.rs:2222`, body built by `devicemap::to_json` at
  `devicemap.rs:228`) → `{"installed":bool,"dims":D,"count":N,"kind":"grid"|"coords"
  [,"w":W,"h":H]}`. This is the only endpoint that reports geometry at all, and it
  reports the **device map**, not the engine's effective grid.
- **`POST /api/map`** (`devicemap::set_from_wire`, `devicemap.rs:204`): body is either
  `<dims> <v0> <v1> …` (raw 16.16 per axis per pixel) or `grid <w> <h>`; empty/invalid
  clears (falling back to `board_default()`). Note a full 64×64 coordinate map is 48 KB
  and **cannot be POSTed** — the request buffers are 4 KB — which is why `grid w h` exists
  (`devicemap.rs:12-17`, `docs/boards.md:1098-1101`).
- **`GET /api/config`** (`server.rs:2182`) → `{"pixels","max","protocol"[,"data_pin",
  "data_pin_default","data_pin_next","data_pins"]}`. **No geometry.** The `data_pin*`
  fields are absent on panel boards (`server.rs:2193`) — that absence is today's only
  implicit "this is a panel" signal.
- **`GET /api/status`** (`status_json`, `server.rs:313`) → `pixels` (`server.rs:~514`),
  `max_pixels`, and on a panel `rescan_hz` (`server.rs:361-363`) and the `drops`/`pass`
  blocks. **No `dims`, no `w`/`h`, no map kind, no `preferred_dims`.**

### What the UI would need in `/api/status`

To always know the mode from the poll it already makes, `/api/status` would need the
**engine's effective** geometry, not the device map's:

- `geom.kind`: `"strip" | "grid" | "coords3d" | "coords2d"` — derived from
  `Engine::grid()` + `Engine::installed_map().dims`, not from `devicemap::MAP`;
- `geom.w` / `geom.h` when `Engine::grid()` is `Some` (6 bytes, already `Copy` and already
  read every frame at `main.rs:1568`);
- `geom.source`: `"user" | "board" | "default"` — so the UI can distinguish a real 64×64
  panel from a pattern-fabricated √n grid;
- `pattern_dims`: `Engine::preferred_dims()` (0/2/3), already computed and already exposed
  through the wasm build as `lx_preferred_dims` (`crates/luxel-wasm/src/lib.rs:646`).

All four are cheap — no allocation, no flash, a handful of `push_u32` calls. Today the UI
must correlate `/api/map` + `/api/config` + `/api/status` and still cannot see the
engine's fabricated grid.

---

## 5. PATTERN STORAGE & ASSETS

### The pattern store

Partition `storage, data, spiffs, 0x210000, 0x100000` (1 MiB, `firmware/partitions.csv`),
split (`patterns.rs:166-185`):

- **key area** — 128 KiB / 32 pages, `sequential-storage` map (`patterns.rs:177`);
- **extent region** — 896 KiB, flash-mapped read-only at boot (`patterns.rs:182-185`,
  `map_ext` at `patterns.rs:438`). Inside it: a 4 KiB ad-hoc header page, 32 KiB ad-hoc
  source, 2 × 64 KiB ad-hoc bytecode sides, then the **file log** at rel `0x49000`,
  `LOG_LEN = 749,568 B = 732 KiB` (`patterns.rs:380-399`).

Patterns are **not** in sequential-storage: they live in an append-only packed log
(`firmware/src/patlog.rs:30-50`) — 48-byte header + name + source + LXBC, each 4-byte
padded, `commit` written last, `dead` flipped in place for deletes. No TOC; boot walks it
(`patterns::reload`, `patterns.rs:707`).

Reserved keys — the complete namespace, all `u32` above the pattern seq space:
`FORMAT_KEY` `0x7FFF_FFFF` (4 B, `FORMAT_VERSION = 6`, `patterns.rs:230`), `PLAYLIST_KEY`
`0x7FFF_FFFE` (playlist text, `:311`), `PLAYSTATE_KEY` `0x7FFF_FFFD` (1 B, `:312`),
`MAP_KEY` `0x7FFF_FFFC` (serialized map, `:313`), `RESUME_KEY` `0x7FFF_FFFB` (~16 B,
`:315`), `PALETTE_KEY` `0x7FFF_FFFA` (≤64 B, `:318`).

`store_blob`/`read_blob` (`patterns.rs:322-354`) are the only blob API. **`BLOB_MAX =
3840 B`** (`patterns.rs:214`) — "safely under one 4 KiB page alongside the u32 key + item
header". Over that, `store_blob` returns `false` and the caller degrades silently
(`devicemap.rs:209-212`).

Per-pattern limits: `MAX_NAME` 64 B (`patlog.rs:93`), `MAX_SOURCE` 32 KiB
(`patlog.rs:99`), `MAX_BC` 40 KiB (`patlog.rs:101`), `MAX_RECS` 192 (a RAM-index heap
guard; exceeding it sets `OVERFULL` and the store goes **read-only**, `patterns.rs:606`,
`patterns.rs:828-833`). HTTP-side `MAX_UPLOAD` = 80 KiB (`server.rs:1095`). Real ceiling
is bytes: ~119 library patterns fill 722 KiB of the 732 KiB log
(`docs/firmware.md:318-327`).

GC: delete/re-save only write the `dead` word — no erase (`patterns.rs:1273`,
`patterns.rs:1483`). Space returns only via `compact` (`patterns.rs:1643`), which runs
**only from a save that ran out of room**, never on activation, to keep playlist churn
wear-free (`patterns.rs:1610-1613`). Bound: ≤183 erases + 183 page writes, each its own
fenced op with a 1 ms yield.

`GET /api/patterns` → `{"patterns":[{"id","name"},…]}` (`patterns.rs:903`,
`server.rs:2247`); `GET /api/patterns/<id>` → `{"id","name","source"}` served straight out
of the mapping with no source `Vec` (`patterns.rs:963`). `id = seq ^ 0x5eed_1e55`
(`patterns.rs:196`).

### Is the store a home for fonts / sprites / scenes?

- **(a) Font blob, 1–6 KB — no, not in the key area.** `BLOB_MAX = 3840` kills 4–6 KB
  outright, and sequential-storage stores unaligned and *relocates on GC*, which the
  module docs call out as exactly wrong for anything mapped (`patterns.rs:56-60`). The
  file log would hold it physically (4-byte aligned by construction, `patlog.rs:23-27`,
  and 6 KB ≪ `MAX_BC`), but every log record *is a pattern*: `Rec` has only
  `src_len`/`bc_len`, `save()` requires non-empty source **and** bytecode
  (`patterns.rs:1296`, `:1305`), `list_json` would surface the font as a pattern, and a new
  record type means a `patlog::VER` bump (`patlog.rs:97`) — which retires every existing
  record (the no-migration rule, `patterns.rs:73-78`).
- **(b) Sprite bitmaps — same answer, worse.** Many small records each cost 48 B + name +
  padding, eat into `MAX_RECS` 192, and make compaction (up to 366 fenced flash ops) more
  likely.
- **(c) A scene/composition record of a few hundred bytes — yes, cleanly.** A new reserved
  key beside `PALETTE_KEY`, `store_blob`/`read_blob` — exactly the playlist's shape
  (`playlist.rs:247`) and the resume record's (`resume.rs:104-107`). See §8(iv).

### The assets partition

`assets, data, spiffs, 0x310000, 0x0F0000` = **983,040 B (960 KiB)**, identical on every
board (`assets.rs:41-43`). Current bundle ~641 KB, ~342 KB free
(`docs/boards.md:1028-1041`).

Bundle format (packer `web/tools/pack-assets.mjs:1-13`, parser `assets.rs:183-269`):

```
"LUX2" u32 count { u8 path_len, path, u8 ctype_len, ctype,
                   u8 gzip, u32 len, u32 offset, u8[8] etag } … blobs
```

Offsets are relative to region start; **more than 64 entries is refused**
(`assets.rs:212-214`). Gzip is per-file and automatic when it is smaller
(`pack-assets.mjs:46-48`) — and **the firmware has no inflate anywhere**; it only forwards
`Content-Encoding: gzip` (`server.rs:2022-2023`).

Serving: the TOC is parsed into RAM at boot (`assets.rs:183`); bodies go to the socket
**straight out of the cache-MMU mapping** in 4 KiB slices with a yield
(`server.rs:684-693`), falling back to `read_chunk` + `Timer::after(1ms)` when unmapped
(`server.rs:697-711`).

Upload: **`POST /api/assets` is fully streaming, whole-bundle replace**
(`AssetsService`, `server.rs:983-1062`). `Content-Length` is `expected`; the body is read
through a **4 KiB** heap buffer into `AssetWriter::write` (`server.rs:1006-1040`), which
erases each 4 KiB sector just-in-time interleaved with network reads and validates the
magic on the first chunk (`assets.rs:297-341`); `commit` requires `written == expected`,
invalidates cache lines and re-parses the TOC (`assets.rs:343-359`). **No per-file upload
path exists**; peak RAM is 4 KiB.

### Could a font reuse the assets path?

Mechanically, yes: `assets::lookup("/font.bin")` gives `AssetEntry { offset, len }` and
`assets::mapped()` gives the region as `&'static [u8]`, so
`mapped()[offset - REGION_START ..][..len]` is a zero-copy `&'static [u8]` — the same
arithmetic the server already does at `server.rs:684-686`. Three real caveats:

1. the packer would gzip it if compressible and the firmware cannot inflate — the packer
   needs a no-compress rule for the font extension (`pack-assets.mjs:46-49`);
2. blob starts are not word-aligned, which matters if you want `&[u32]` (contrast
   `patlog.rs:23-27`, which guarantees 4-byte alignment);
3. **everything in `assets.rs` except `read_chunk` compiles out under `hosted-ui`**
   (`assets.rs:20-25`, `firmware/Cargo.toml:100`), so a hosted-ui image would have no font.

### Other runtime blob read paths a font could reuse

1. **`flashmap::map` + leak → `&'static [u8]`** (`flashmap.rs:130-160`, `leak` at
   `flashmap.rs:93`). Two live consumers — `assets::map_region` (`assets.rs:95`) and
   `patterns::map_ext` (`patterns.rs:438`), 14–15 × 64 KiB entries each; 29 of the classic
   ESP32's 64 DROM0 entries are in use, so entries remain (`docs/firmware.md:250-255`).
   Rules: task context only, never ISR; invalidate after writing; keep the `read_nor`
   fallback.
2. **The VM code stream** — `patterns::code_of` → `deserialize_lean_static`
   (`patterns.rs:1016-1019`, `bytecode.rs:765`) executes LXBC **in place** from the
   mapping under the pin set (`patterns.rs:1501-1604`). The closest existing analogue to
   "firmware holds a `&'static [u8]` flash blob for a whole boot".
3. **`assets::read_chunk`** (`assets.rs:148`) — the stack-safe flash reader, always built
   even under `hosted-ui`.
4. **Build-time rodata** — `build.rs:5-15` compiles `library/rainbow.js` to
   `OUT_DIR/default.lxbc` and `main.rs` `include_bytes!`s it, served as `BcLoc::Default`
   (`patterns.rs:1746`). Cheapest of all for 1–6 KB — no partition, no upload protocol, no
   alignment question — at the cost of OTA-slot bytes and no updatability without an OTA.

---

## 6. LANGUAGE / VM HOOKS

### The array model

`Value` is an 8-byte 4-variant enum, `Num(Fx) | Arr(u32) | Fun(u32) | Builtin(u32)`
(`vm.rs:49`). `Value::Arr(u32)` is an **arena index**, not a pointer; the arena is
`Vm::arrays: Vec<ArrRepr>` and `ArrRepr` is `Owned(ArrVec<Value>)` or `Const(u32)`
(`vm.rs:853`). Reads go through `ArrView` (`vm.rs:878`) — `Owned(&[Value])` or
`Const(&[u32])`, the latter being raw 16.16 words **read straight from flash**. Writes go
through `Vm::arr_mut` (`vm.rs:1538`), which materializes a `Const` entry into owned
storage first (copy-on-write), charging only the byte delta.

`array(n)` → `alloc_array_zeroed` (`vm.rs:2690`): budget check before reserving,
`try_reserve_exact` (fallible — never panics on device), zero-fill. Two ledgers in
`charge_array` (`vm.rs:2642`): the PB element budget (10,236 units, each array costing
`len + 4`) and the byte budget (`len*8 + 32`; a const entry is a flat 32 until COW,
`vm.rs:732`). Slot cap `MAX_ARENA_SLOTS = 2559` (`vm.rs:768`). **Arrays are never freed.**

### The builtins table

`vm.rs:425-447`. There is **no arity field and no function pointer** — the table is
`BuiltinDef { name: &'static str, kind: BKind }`, where `BKind` is `Impl(Builtin)` (a
C-like enum, `vm.rs:214`) or `Todo`. The index in `BUILTINS` **is** the builtin id and is
append-only. Dispatch is three tiers: `builtin_fast` (`vm.rs:2751`, inlined into the
interpreter loop), `call_builtin` (`vm.rs:2856`, pops args into `[Value; MAX_ARGS=16]` via
`pop_args_into` at `vm.rs:1965`), `builtin_hot` (`vm.rs:2909`), `builtin_cold`
(`vm.rs:3099` — where every array/canvas/bulk op lives). Arity is implicit: missing args
read as 0, extras ignored; the compiler only rejects >15 args (`compile.rs:1958`).

**A builtin can take an array handle and mutate it, and can write pixels directly.**

| builtin | impl | touches |
|---|---|---|
| `canvasGet(buf,w,x,y)` | `vm.rs:3045` | read-only `ArrView`, bilinear sample |
| `canvasSet(buf,w,x,y,v)` | `vm.rs:3966` | `arr_mut` → `data[cell(y)*w + cell(x)] = v` |
| `canvasAdd(buf,w,x,y,v)` | `vm.rs:3990` | same cell, accumulate, returns new value |
| `blur2D(arr,w,h,r)` | `vm.rs:3867` | in-place separable box blur over `arr_mut` |
| `blit(hA,sA,vA,w,h,col,row,mode)` | `vm.rs:4161` → `bulk.rs:880` | reads 3 arrays *or scalars*, writes the RGB888 **frame buffer** |

**The "canvas" is a pattern-owned arena array**, row-major `w × h` of scalar `Fx`, with no
colour space — the pattern decides whether a cell is hue, value, or heat. The **frame
buffer** is a different thing: `Vm::frame: Vec<[u8;3]>` (`vm.rs:988`), RGB888, lent *by
move* from `Engine::pixels` only during `renderFrame`; empty otherwise, which is the
entire guard that makes every bulk op a silent no-op elsewhere. `fillCanvas` (`bulk.rs:785`)
bridges canvas arrays → frame pixels; `blit` (`bulk.rs:880`, `blit_into` `bulk.rs:887`,
`paste` `bulk.rs:925`) is the exact precedent a sprite/text draw wants — clip the source
rect to the grid up front, then `put(dst, texel, mode)` with the 4 blend modes at
`bulk.rs:88-104`/`bulk.rs:110`: **0 replace, 1 add (saturating), 2 max (lighten), 3 keyed
(black transparent)**. Its own doc comment already says "Sprites, text and scrolling are
`blit(..., col - t, row, 3)`" (`bulk.rs:870`). Arguments are uniformly "array or scalar"
via `Src` (`bulk.rs:189`, `srcs` at `bulk.rs:263`).

### The const pool, and strings

Pool table entry is `{ u32 start, u16 len }` (`vm.rs:148`); entries are `len` raw 16.16
words in the program's 4-aligned word region after the code (`bytecode.rs:652-657`, decode
`bytecode.rs:904-928`, caps `MAX_DATA_ARRAYS 4096` / `MAX_DATA_ELEMS 65_536`).
`compile.rs:1862-1871` interns an all-numeric `ArrayLit` (content-deduplicated,
`intern_data` at `compile.rs:2077`) and emits `Insn::ConstArr(u16)`; at runtime
`alloc_const_array` (`vm.rs:2678`) makes an arena slot that **shares the flash words**.

**Could a string literal become a const byte array?** The containers exist but nothing
does it today. The lexer has a `Str` token (`lex.rs:74-78`) whose own comment says "the
pattern language has no string values — this token is legal ONLY as `assert()`'s message
argument"; the parser hard-errors anywhere else (`parse.rs:344-360`, `parse.rs:836-839`).
There is **no string `Value` variant**. But LXBC already ships a **deduplicated string
table** — `msgs`, `n_msgs × str8`, ≤255 B each, `MAX_ASSERT_MSGS = 4096`
(`bytecode.rs:61`), kept even by lean decodes because it is user-facing
(`bytecode.rs:928-937`, `docs/spec/bytecode.md:87-92`), indexed by a u16 in the `Assert`
opcode. That is the ready-made precedent for shipping text to a compiler-less device.
Note the const pool is 4 bytes per element, so a byte-packed font in it wastes 4× unless a
new section or an explicit packing convention is added. `docs/ideas.md:77-79` lists "a real
string type [L] ★★ … gated behind a use case".

### Render entry points and `renderFrame`

`render_targets(prog)` (`engine.rs:1538`) scans four well-known names — `render`,
`render2D`, `render3D`, `renderFrame` — preferring an exported fn, falling back to a global
of that name (late binding). `resolve_render` (`engine.rs:1558`) re-runs **every frame**
after `beforeRender`, and `renderFrame` wins unconditionally over the three per-pixel
entries regardless of map dimensionality.

`renderFrame()` is a zero-arg export called **once per frame**. `Engine` lends it the whole
frame buffer by move (`frame_buffer_out`/`frame_buffer_in`, `engine.rs:1286`/`1293`), and
~16 native bulk builtins (ids 166–181) write RGB888 in place. The buffer is **not cleared
between frames** — persistence is the feature, which is how `fade(k)` + a few `setPixel`s
make a trail with no pattern-side array (`docs/bulk-render.md:57-64`). The brush
(`Vm::pixel`, written by `hsv()`/`rgb()`/`paint()`/`oklch()`) resets to black at the top of
each frame and is sticky within it; shape ops read it. `post_chain()` + `finish_frame()`
run exactly as on the per-pixel path (`engine.rs:1189-1197`), so `setFrameRate`,
`timeScale`, `setBlur`, `setGlow` and the output palette behave identically.
`uses_coordinate_bulk_op()` (`engine.rs:1303`) gives a `renderFrame`-only pattern the
default √n grid **only** if it names a coordinate/grid op. Issue **#405** is the campaign
converting library patterns to this shape (`docs/bulk-render.md:551-1656`).

### UI controls and annotations

Control discovery is a **runtime export-name scan, not an annotation**
(`engine.rs:335-350`): for each exported fn, try each prefix in `CONTROL_PREFIXES`
(`engine.rs:98-107` — `slider`, `hsvPicker`, `rgbPicker`, `toggle`, `trigger`,
`inputNumber`, `showNumber`, `gauge`). **The label is a bare prefix strip**
(`sliderSpeed` → `Speed`, `engine.rs:76-79`) — no camelCase splitting, no per-control text
metadata; the UI prints it raw (`web/src/components/Controls.svelte:76`).

`//#` hint comments exist but are **numeric-only and parsed in the JS layer, never in the
compiler or the blob** — keys `min`, `max`, `step`, `default`, value regex
`(-?\d*\.?\d+)` (`web/src/lib/hints.ts:1-56`, twin in `tools/verify/hints.mjs`, documented
`docs/lang.md:1087-1130`). So there is **no string channel from pattern source to runtime**
except the assert-message table, and identifiers.

### IRAM gates

`iram-vm` = `Vm::run` only (13,572 B); `iram-builtins` = `call_builtin` + `builtin_hot`
(7,304 B), with `builtin_cold` deliberately left in flash (`vm.rs:2852-2854`); `iram-math`
= `hsv_to_rgb` + the per-pixel `fmath`/`noise` entry points (9,412 B). A text/sprite
builtin is a once-per-frame op with a big body and **belongs in `builtin_cold` next to
`Blit`** (`vm.rs:4161`), body in `bulk.rs` — never in the hot tiers or the IRAM budget.

---

## 7. API SURFACE

Dispatcher: `firmware/src/server.rs:1446` (`struct Api`), body `:1449-2300`. Every response
is the one `Reply` type; all error bodies are **HTTP 200** + `{"ok":false,"error":…}`; 404
only for unrouted paths. `HEAD` is unhandled → 404 (deliberate, `:1441`).

**POST** (`:1465`) — body → response:

| path | line | body | response |
|---|---|---|---|
| `/api/ota` | 1470 | raw app image, streamed 4 KiB | `{"ok":true,"bytes":N}`, reboots |
| `/api/assets` | 1480 | LUXA/LUX2 archive, streamed | `{"ok":true,"bytes":N,"files":N}` |
| `/api/code` | 1504 | LXP1 envelope, streamed | `{"ok":true}` |
| `/api/patterns` | 1515 | LXP1 envelope + name, streamed | `{"ok":true,"id":"<hex>"}` |
| `/api/wifi` | 1528 | `ssid\npassword` | `{"ok":true,"ssid":…}`, reboots |
| `/api/apmode` | 1555 | — | `{"ok":true,…}`, reboots |
| `/api/clock` | 1587 | i16 tz minutes −840..840 | `{"ok":true,"tzMinutes":N}` |
| `/api/output` | 1609 | `<order> <gamma_t> <cap_ma> [<curve_t> <blur%> <glow%>]` | `{"ok":true,order,gamma,capMa,brightCurve,blur,glow}` |
| `/api/output/palette` | 1667 | `<amount%> <pos> <r> <g> <b> …` | `{"ok":true}` |
| `/api/sync` | 1685 | `off`\|`leader`\|`follower` | `{"ok":true,"mode":…}` |
| `/api/sensors` | 1714 | binary `SB1.0\0…END\0` | `{"ok":true}` |
| `/api/events` | 1728 | binary `EV1\0` frame | `{"ok":true}` |
| `/api/mqtt` | 1739 | `host\nport\nuser\npass` | `{"ok":true,"enabled":bool}` |
| `/api/brightness` | 1770 | `0..31` | `{"ok":true,"brightness":N}` |
| `/api/config` | 1791 | pixel count `1..=MAX_PIXELS` | `{"ok":true,"pixels":N}` |
| `/api/datapin` | 1824 | GPIO number or `default` (strip boards only) | `{"ok":true,"data_pin":N}`, reboots |
| `/api/protocol` | 1866 | protocol name | `{"ok":true,"protocol":…}` |
| `/api/map` | 1890 | `<dims> <raw16.16…>` or `grid <w> <h>`; empty = clear | `{"ok":true,"installed":bool,"count":N}` |
| `/api/playlist` | 1901 | D/X/I/C line format | `{"ok":true}` |
| `/api/playlist/{play,stop,next,prev}` | 1905-1910 | index (play only) | `{"ok":true}` |
| `/api/control` | 1921 | `name raw0 [raw1 raw2]` | `{"ok":true}` |
| `/api/var` | 1922 | `name raw` | `{"ok":true}` |
| `/api/patterns/<id>/activate` | 1924 | — | `{"ok":true}` |

**DELETE** (`:1938`): `/api/output/palette` → `{"ok":true}`; `/api/patterns/<id>` →
`patterns::delete` JSON. **OPTIONS** (`:1956`): any path → 204 + ACAO/Allow-Methods/
Allow-Headers/Max-Age (exactly 4 headers; `MAX_HEADERS = 4` with a compile-time assert at
`:148`).

**GET** (`:1977`): `/` → flash `index.html` else embedded (`:2036`); `/min` → embedded
(`:2046`); `/api/status` → the big JSON (`status_json` `:307`); `/api/wifi` →
`{ssid,source}`; `/api/brightness` → `{brightness,max}`; `/api/apmode` → `{ap}`;
`/api/output` → `{order,gamma,capMa,brightCurve,blur,glow,palette[],paletteAmount}`;
`/api/clock` → `{synced,local,tzMinutes}`; `/api/sync` → `{mode,timeMs,leader}`;
`/api/mqtt` → `{enabled,host,port,user,hasPass,connected}`; `/api/config` →
`{pixels,max,protocol[,data_pin*]}`; `/api/playlist`; `/api/map` →
`{installed,dims,count,kind,w,h}`; `/api/protocol` → `{protocol,options[]}`;
`/api/pixels` → `application/octet-stream`, 3 B/px (`:2230`); `/api/pattern` →
`text/plain` source streamed from flash; `/api/pattern.lxp` → LXP1 envelope streamed;
`/api/controls` → `[{kind,label,name}]`; `/api/vars`; `/api/readouts`; `/api/patterns` →
`{patterns:[{id,name}]}`; `/api/patterns/<id>` → `{id,name,source}`; any other GET → flash
asset with strong ETag + 304 shortcut (`:2256`), or a 307 captive-portal redirect in AP
mode; fallback 404 (`:2290`). **There are no WLED-compat HTTP routes** — `takeover.rs` /
`wledfs.rs` are a flash self-install path, not a `/json/state` shim.

### The mirror (`crates/luxel-cli/src/serve.rs:1470`) and its drift

Hand-rolled HTTP/1.1, `Connection: close` on every response, one thread per connection,
bound to 127.0.0.1 only (`serve.rs:2107`). Same route set, with:

1. **Firmware-only:** `POST /api/ota`, `/api/assets`, `/api/datapin` — the mirror 404s all
   three.
2. **Mirror-only:** `POST /api/pins` (`serve.rs:1745`), digital/analog pin injection. The
   firmware has no equivalent.
3. **`GET /api/status` shapes differ materially.** The mirror (`serve.rs:757-786`) emits
   only `fps,out_fps,rescan_hz,pixels,max_pixels,slot:"native",version,heap_free,
   engine_heap,live,vmerr`. The firmware adds `frame_us,vm_us,pipe_us,out_us,dropped,
   heap_largest,assets_mapped,code_mapped,store{},src,bc,web[]` and, on panels,
   `drops{},swap{},pass{}`. **A v2 UI reading timings or `store` works only against a
   device.**
4. `POST /api/output/palette` echoes the palette on the mirror (`serve.rs:1643`) but
   returns a bare `{"ok":true}` on the firmware (`:1667`).
5. `docs/api.md:405` ("the mirror omits `kind`") is **stale** — it does report it.
6. `POST /api/clock` parses i16 on firmware, i32 on the mirror; same clamp.
7. Reboot semantics differ (the mirror just returns the same body).
8. **CORS coverage differs.** The mirror adds ACAO to *every* response; the firmware only
   where `.cors()` is called — **flash assets, `/`, `/min`, the 307 and the 404 carry no
   ACAO**, contradicting the module header at `server.rs:15`.
9. Neither strips query strings — both 404 on `/api/status?x=1`.
10. Firmware is keep-alive; the mirror closes every connection.
11. Firmware caps uploads at 80 KiB (`MAX_UPLOAD`, `server.rs:1093`); the mirror has **no
    cap** (`vec![0u8; content_length]`, `serve.rs:1395`).

### The flat-dispatcher constraint

`server.rs:1433-1445` states it: chaining picoserve `.route()` calls nests one router type
per route, and polling that chain put a multi-KB stack frame per level on the executor —
at 11 routes it overflowed the main stack into the WiFi blob's `.bss` and surfaced as wild
pointer crashes inside radio code. A `match` keeps the poll frame constant. It also stays a
`PathRouterService` rather than `MethodRouter` because the latter's HEAD arm wraps the
writer in a private `IgnoreBody<W>` — a second `W` type duplicating every GET
instantiation (Gitea #167, −24 KB).

Matching is on the **full encoded path string**, not the first segment (`:1463`). Nested
paths are plain literals in the same flat match; dynamic segments are guard arms doing
string surgery (`r if r.starts_with("/api/patterns/")`, `:1924`/`:1950`/`:2250`).
**Adding a route costs one match arm** — no nesting, no per-route future, no stack growth.
The real constraints are that every arm must return the single `ApiResponse = Reply`
(`:225`) through the one `write_to` at the bottom of each method arm (`:1934`, `:2284`),
**a new body kind is a new `ApiBody` variant** (`:80-103`, plus three methods on
`impl Content`, `:105-145`), a runtime header value is `HVal::Owned` (`:56-73`), headers
cap at 4, and a handler with a large sub-future must be `Box::pin`ed and delegated
(`OtaService`/`AssetsService`/`PatternService`, `:1471`/`:1481`/`:1505`/`:1516`).

### The socket pool — what a chattier v2 UI must respect

`WEB_TASK_POOL_SIZE = 3` (2 under `small-chip`), `server.rs:2327`, spawned at
`main.rs:684`. Per socket: 4 KiB TCP rx + 4 KiB tx (`server.rs:2470-2471`) plus a 4 KiB
HTTP buffer allocated **per connection** and freed at close (`:2483`); if that fails the
connection gets a bare 503. Each slot also carries ~8.6 KB of static task arena
(`docs/boards.md:1514`). Timeouts (`CONFIG`, `:2335-2348`): 5 s start-read, 1 s persistent
start-read, **45 s read-request** (one timer for a whole OTA body), 5 s write, keep-alive on.

**On a 4th concurrent request nothing accepts it** — all slots are in `accept()`/serving,
so the SYN is refused at TCP level and the client sees `ERR_CONNECTION_REFUSED`, not a
queue or a 503 (`:2305-2326` documents Chromium dropping `luxel.wasm` over exactly this).
The client-side mitigation already exists: `web/src/lib/fetchgate.ts` — `MAX_INFLIGHT = 2`,
6 retries with ~10 s exponential backoff, 30 s attempt cap, buffering the whole body inside
the gate. Per-slot lifecycle is exported as `SLOT_STAGE` (`:2355`) and surfaced as `web` in
`/api/status`; `QuickCloseSocket` (`:2377`) force-aborts after a 2 s shutdown grace because
browsers on keep-alive otherwise pinned a slot for the full 45 s.

### Websocket / SSE

**None. Everything is request/response polling.** Both `/ws` implementations were removed —
`server.rs:2306` (the preview websocket cost 32 KB of static connection buffers and the
size diet cut it), `server.rs:1467` ("the old /ws incident: stack-guard overflow"),
`serve.rs:1366`. No `text/event-stream`, no chunked encoding anywhere: every response sets
an exact `Content-Length`, which is the whole point of `ApiBody::content_length`.
"Streaming" here means incremental writes *inside* a fixed Content-Length, newline-padded
if a mid-response flash read fails (`:803-815`).

The playground polls (`web/src/App.svelte`): `/api/status` at **1000 ms** whenever
connected (`:751-756`, with an explicit comment at `:744-750` that a tighter poll starves
slow patterns, #259); `/api/playlist` at 1000 ms only while that tab is open; `/api/mqtt`,
`/api/sync`, `/api/clock`, net-live `/api/status` at 2000 ms only on the Settings tab.
`/api/pixels` is **not polled by the playground at all** — the browser renders its own
preview through the wasm engine.

### Body limits, auth

Streamed pattern uploads cap at 80 KiB. **Every other POST is capped implicitly by the
4 KiB per-connection HTTP buffer** (`read_all`, `:1571`) — this is the sharp edge: a
per-pixel `POST /api/map` exceeds it and is silently treated as "clear the map"
(`docs/api.md:439-441`), and `/api/playlist` and `/api/output/palette` share the ceiling.
The body copy is fallible (`try_reserve_exact`, `:1573`). **There is no auth of any kind** —
no token, no pin, no origin allowlist; port 80 on the LAN with `ACAO: *`. (`patterns::pin_code`
at `patterns.rs:1546` is a flash-mapping pin, unrelated to access control.)

---

## 8. FEASIBILITY NOTES

Preliminary, not decisions.

### (i) N resident engines vs one engine rendering layers as sub-programs

**N resident engines** is the path of least invention: the machinery exists
(`prev: Option<Engine>` + `blend_px`, `main.rs:1156`/`1572-1586`), it already survives the
pin/compaction hazards (`pin_prev_from_running`, `main.rs:1348`), and each layer keeps its
own controls, vars, clock, `beforeRender` delta, `setFrameRate` cap and post-chain — which
is what "OBS scene with independent sources" actually means. Costs: each layer is a full
engine (§2 table: ~14–33 KB at 300 px, ~17–56 KB at 4096 px) plus a 3 B/px buffer that
**must stay in internal DRAM** (`psram.rs:20-22`), and each layer's `Program` decode path
needs its own flash pin — today there are exactly two pin slots (`patterns.rs:1532-1604`).
The budget model also has a hole: `load_base` explicitly assumes the outgoing engine is
dropped before the incoming one is measured (`budget.rs:114-131`), and a crossfade already
falsifies that; N layers falsifies it permanently. **`budget.rs` would need a per-layer
accounting concept, and `/api/status`'s `engine_heap` would need to become a sum or a
vector.**

**One engine, layers as sub-programs** avoids all of that — one `Program`, one globals
array, one pixel buffer — but it means a layer cannot be an existing stored pattern, which
throws away the library and the playlist. There is a middle road the code already hints at:
`renderFrame` + `blit`/`fillCanvas` is *already* a compositor DSL inside one engine, with
four blend modes and clipping (`bulk.rs:88-104`, `bulk.rs:880`); `library/bulk-sprite-scroll-2d.js`
is a working example at 1.25 µs/px. A composition where the *layers* are patterns but the
*blend* is native is the shape the existing code supports best.

**Hard blocker for N≥3 at 4096 px:** the 12 KB per-layer internal-DRAM frame buffer against
~36 KB of headroom (`budget.rs:269`), with the device blur+glow chain already costing
12.3 KB of that (#446). **Cheap win:** N=2 as a *persistent* mode rather than a timed fade
is a small change to the existing crossfade — hold `t` at a user-set opacity instead of
ramping it, and expose `blend_px`'s mode alongside the four `bulk::Blend` arms.

### (ii) Text as a builtin over a bitmap font blob

**Where the font bytes live at runtime.** Four candidates, in ascending cost:

1. **A const array in the pattern's own bytecode** — `ConstArr` + `alloc_const_array`
   (`vm.rs:2678`) gives an arena array that shares the program's flash words, COW, 32 B of
   ledger. This works **today, with no firmware change at all**; today's
   `library/scrolling-text-marquee-2d.js` does something cruder — it passes eight literal
   row-bytes per glyph as scalar args into a runtime `array()`
   (`loadGlyph`, `library/scrolling-text-marquee-2d.js:79`), i.e. it pays heap for what a
   const array would keep in flash. Cost: 4 bytes of flash per glyph
   *bit* if stored naively as one element per cell, or 4 bytes per 32-bit row-word if
   packed — an 8×8 96-glyph ASCII font is 768 row-bytes ≈ 3 KB packed as 16.16 words.
2. **Build-time rodata via `include_bytes!`** (`build.rs:5-15`, `patterns.rs:1746`) —
   cheapest at runtime, `&'static [u8]` with no partition and no alignment question, but it
   costs 1–6 KB of the 1 MiB OTA slot and is not updatable without an OTA.
3. **The assets partition** — `assets::mapped()[offset-REGION_START..][..len]` is a
   zero-copy `&'static [u8]` (§5), with three caveats: the packer would gzip it and the
   firmware has no inflate; blob starts are not word-aligned; and it all compiles out under
   `hosted-ui`.
4. **A new flash-mapped region** — `flashmap::map` + leak (`flashmap.rs:130`), the same
   discipline `assets::map_region` and `patterns::map_ext` use. Most work, most flexible.

**Cost per glyph draw.** An 8×8 glyph is 64 texels. `blit`'s `paste` (`bulk.rs:925`) is a
pre-clipped double loop with one `texel_at` + one `put` per cell, i.e. the same kernel as
`bulk-sprite-scroll-2d` at **1.251 µs/px over the whole frame** for one blit per frame.
Per-glyph the marginal cost is ~64 `put`s — sub-microsecond territory; a 20-character line
is ~1280 texels, well under a millisecond. **Text drawing is not a performance problem on
any board.**

**It must be in the wasm build, and it is, for free** — `luxel-core` is one crate compiled
into the firmware, the CLI, and `crates/luxel-wasm`, which runs the same `Engine::frame`
(`crates/luxel-wasm/src/lib.rs:447`). A builtin appended to `BUILTINS` (`vm.rs:447`) + a
`Builtin` variant + one `builtin_cold` arm is automatically identical in all three, with no
`FORMAT_VERSION` bump (appending is backward-compatible). **The only thing that would not
be automatic is the font bytes** — a device-side flash blob has no wasm equivalent, which
argues strongly for options (1) or (2) over (3)/(4).

**The hard blocker is strings, not drawing.** There is no string `Value`, no string literal
outside `assert()`, and `//#` hints are numeric-only (§6). So `drawText("HELLO", …)` cannot
be written today. Three ladder rungs: (a) glyph *indices* in a const array — works now, zero
VM change, ugly to author; (b) reuse the `msgs` str8 table machinery for a second
`strings` section plus an opaque `Value::Str(u16)` handle that only builtins consume — a
contained change that keeps every existing blob valid and gives the playground the same
literals; (c) a real heap string type — the "[L] ★★ big VM change" `docs/ideas.md:77-79`
gates. **(b) is the cheap win**: the container, the dedup, the lean-decode preservation and
the u16 index are all already written and tested.

### (iii) Sprites as stored bitmap assets drawn by a builtin

The drawing half is **already shipped**: `blit` with keyed mode 3, pre-clipped, grid-space
(`bulk.rs:880-949`). What is missing is (a) a source that is not three parallel `Fx` arrays
— an RGB888- or 1-bit-packed source would need a new `Src` variant (`bulk.rs:189`) or a new
builtin beside `Blit`; and (b) grid-independence — `blit` no-ops when `vm.frame_grid` is
absent, and `gridWidth()` returning 0 is the documented probe.

Storage is the awkward part. The pattern store is **not** a good home for many small
bitmaps (§5(b)): each record is pattern-shaped, costs 48 B + name + padding, eats into
`MAX_RECS = 192`, and a new record type needs a `patlog::VER` bump that retires the whole
library. The assets partition (960 KiB, ~342 KiB free) is the natural place, but its upload
is **whole-bundle replace only** with a 64-entry cap — so "upload a sprite" would mean
"repack and re-push the entire web bundle", ~15 s of erases. **This is the real blocker for
user-uploadable sprites and fonts**, and it is the same blocker for both. A per-file asset
upload, or a third small partition with its own streaming writer, is the enabling change.
Note the partition table is already identical on every board including the 16 MB Seengreat
(`docs/boards.md:1028-1041`), so there is no room to carve one out without a new table.

### (iv) A composition record persisted like the playlist

**This one is cheap and clean.** A new reserved key beside `PALETTE_KEY` (e.g.
`0x7FFF_FFF9`) with `store_blob`/`read_blob` (`patterns.rs:322-354`) is exactly the
playlist's and the resume record's shape. Constraints:

- **3840 B hard cap** (`BLOB_MAX`, `patterns.rs:214`) and silent failure — the writer must
  check the bool the way `devicemap.rs:209` does. At ~80 B per layer (pattern id, blend
  mode, opacity, 6-float transform) that is ~45 layers' worth, far past anything the heap
  allows, so the cap is not binding.
- **The POST body is capped at ~4 KiB by the per-connection HTTP buffer** (§7), which
  matches the blob cap — a line-oriented text format like the playlist's `D/X/I/C`
  (`playlist.rs:147-160`) keeps it well inside both.
- One sequential-storage transaction per edit over the 32-page key area, through the shared
  `PageStateCache` (`patterns.rs:264-276`) — load-bearing: without it a 650 B POST measured
  81,143 fenced reads and rebooted the board (#292).
- `GET`/`POST /api/scene` is one flat match arm each (§7), plus a `Msg::` variant for the
  render task. The playlist is the template end to end, including the `/play`, `/stop`,
  `/next` sub-paths as flat literals.

### (v) What the playground needs for an identical preview

The playground already shares `luxel-core` — the same `Engine`, the same `post_chain`, the
same `budget.rs` constants, the same `bulk.rs` blend modes, the same RNG. So a compositor
and a text builtin written in `luxel-core` are identical by construction. Two real gaps:

1. **The playground does not run the device outpipe at all.** `lx_frame`
   (`crates/luxel-wasm/src/lib.rs:447`) calls `Engine::frame` and returns the bytes; there
   is no call to `outpipe::apply`, `blur_frame`, `gamma_lut` or the power cap anywhere in
   `crates/luxel-wasm` or `web/src`. So today's preview already diverges from the device by
   the whole Settings chain (palette, blur, glow, brightness curve, colour order, power
   cap). Any compositor whose layers each carry their own post-chain **must** have this
   fixed, or the preview will be wrong in a new and more confusing way. The fix is small —
   `apply_outpipe` is 90 lines in `main.rs:769` over functions that already live in
   `luxel-core::outpipe`; lifting it into `luxel-core` and exposing it as `lx_outpipe(...)`
   would make both hosts share one chain. **Cheap win, independent of everything else.**
2. **Geometry.** The playground has `lx_set_map_grid` (`lib.rs:609`) and
   `lx_preferred_dims` (`lib.rs:646`) but must be *told* the device's grid. Today it reads
   `/api/map`; §4's proposed `geom` block in `/api/status` would make the 1 Hz poll
   sufficient.
3. Font bytes, per (ii): if the font lives in device flash the playground cannot see it.
   Const-array or `include_bytes!`-in-`luxel-core` avoids the problem entirely.

### Summary

**Hard blockers.** No string type or string literal path to the runtime (text). Whole-bundle-
only asset upload with a 64-entry cap (user-uploadable fonts and sprites). The 12 KB
internal-DRAM frame buffer per layer against ~36 KB of headroom at 4096 px (N≥3). The
budget model's assumption that only one engine is resident (`budget.rs:114-131`). No
websocket/SSE and a 3-socket pool (any UI that wants live layer feedback).

**Cheap wins.** A `geom` block in `/api/status` (§4). Lifting `apply_outpipe` into
`luxel-core` so the playground and device share one output chain (v). A scene blob under a
new reserved key, modelled on the playlist (iv). Persistent-opacity N=2 as a generalization
of the existing crossfade (i). A `strings` section reusing the `msgs` str8 machinery plus an
opaque handle, which unblocks text without a heap string type (ii). And sprite drawing
itself is already shipped — `blit` mode 3 (iii).

