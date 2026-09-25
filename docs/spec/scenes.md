# Scenes — wire format, compositing, sprites

A **scene** is an ordered stack of layers drawn onto one frame. The record,
the blend kernels, the sprite format and the JSON shape all live in
`luxel-core` (`scene.rs`, `compose.rs`) so the firmware, the `luxel serve`
mirror and the wasm playground cannot disagree about any of them. Hosts only
plumb.

Gitea #477 (blend), #478 (record + native layers), #481 (sprites),
#482 (composite thumbnails). Design: `docs/design/webui-v2/proposal.md` §5.5,
§5.5b.

## 1. The wire record

One block of `\n`-separated lines. Layers are listed **bottom → top**: the
first `L` is the base. Grammar, spacing and forward-compatibility rules are
the playlist's.

```
S <id> <name…>                     id = 8 lowercase hex, or `-` on POST
                                   ("assign one"); name = rest of line,
                                   ≤ 64 B UTF-8, no newline
L <type> <x> <y> <w> <h> <blend> <opacity> <key> <fit> <flags>
   type     pat | text | sprite | color
   x y w h  integers in layout pixels; the layer's box. w or h = 0 → full layout
   blend    normal | add | lighten | multiply | mask
   opacity  0..100
   key      none | black | luma
   fit      fill | contain | tile
   flags    bitmask: 1 visible · 2 flipx · 4 flipy · 8 rot180
```

Lines that bind to the most recent `L`:

| line | applies to | meaning |
|---|---|---|
| `N <name…>` | all | layer display name, ≤ 32 B |
| `I <patternId>` | pat, sprite | store pattern id (8 hex) |
| `C <name> <raw…>` | pat | control override, raw 16.16 ints — playlist grammar |
| `P <mode>` | pat | projection override (`index\|x\|y\|z\|xy\|xz\|yz`) |
| `R <pct> <pos>:<rrggbb> …` | pat | colour ramp; `pct` 0..100, ≥ 2 stops, `pos` 0..255 ascending, ≤ 32 stops |
| `T <source> <arg…>` | text | `lit <utf8…>` \| `clock <fmt>` \| `slot <n 0..7>` |
| `F <font> <rrggbb> <align> <scroll> <speed>` | text | font `tiny\|regular\|large`; align `l\|c\|r`; scroll `none\|left\|right\|up\|down\|bounce`; speed px/s |
| `K <rrggbb>` | color | the wash colour |

`<fmt>` ∈ `HH:MM` · `HH:MM:SS` · `hh:MM` · `hh:MM:SS` · `MM-DD` · `YYYY-MM-DD`.

**Defaults** when a binding line is absent: `N` = `Text` / `Sprite` / `Color`,
and the EMPTY string for a `pat` layer (only the host knows a pattern's name);
`F regular ffffff l none 0`; `T lit ` (empty); `K 000000`; no controls, no
projection override, no ramp.

**Errors.** Any parse error rejects the whole block with a message naming the
line: `scene: line 4: unknown blend "foo"`. That string is what the API hands
the console verbatim.

**Forward compatibility.** An unknown line tag is ignored, and so is a binding
line that does not apply to the layer it follows — an older host survives a
newer console's push. A binding line before the first `L`, or any line before
the `S`, is an error.

**Round trip.** `serialize` emits only the lines that differ from the
defaults, so `parse`∘`serialize` is the identity on a record and
`serialize`∘`parse` is a fixed point on the text. A `SCENES_KEY` blob is
scene blocks concatenated; `parse_all` splits on the `S` lines and reports
blob-global line numbers.

### Rust API (`luxel_core::scene`)

```rust
pub enum LayerKind { Pattern, Text, Sprite, Color }
pub enum Blend { Normal, Add, Lighten, Multiply, Mask }
pub enum Key { None, Black, Luma }
pub enum Fit { Fill, Contain, Tile }
pub struct Rect { pub x: i16, pub y: i16, pub w: u16, pub h: u16 }   // w|h 0 = full
pub struct LayerStyle { pub rect: Rect, pub blend: Blend, pub opacity: u8, pub key: Key,
                        pub fit: Fit, pub visible: bool, pub flipx: bool,
                        pub flipy: bool, pub rot180: bool }
pub enum TextSource { Lit(String), Clock(ClockFmt), Slot(u8) }
pub enum Align { Left, Center, Right }
pub enum Scroll { None, Left, Right, Up, Down, Bounce }
pub struct TextLayer { pub source: TextSource, pub font: Font, pub color: [u8; 3],
                       pub align: Align, pub scroll: Scroll, pub speed: u16 }
pub struct Ramp { pub pct: u8, pub stops: Vec<(u8, [u8; 3])> }
pub struct PatternLayer { pub id: String, pub controls: Vec<(String, Vec<i32>)>,
                          pub proj: Option<u8>, pub ramp: Option<Ramp> }
pub enum LayerBody { Pattern(PatternLayer), Text(TextLayer), Sprite { id: String }, Color([u8; 3]) }
pub struct Layer { pub name: String, pub style: LayerStyle, pub body: LayerBody }
pub struct Scene { pub id: String, pub name: String, pub layers: Vec<Layer> }

pub fn parse(block: &str) -> Result<Scene, String>;
pub fn parse_all(blob: &str) -> Result<Vec<Scene>, String>;
pub fn serialize(s: &Scene, out: &mut String);
pub fn push_json(s: &Scene, out: &mut String);
pub fn pattern_layers(s: &Scene) -> usize;      // counts pat layers only
pub fn valid_id(s: &str) -> bool;               // 8 lowercase hex
pub const MAX_NAME: usize = 64;
pub const MAX_LAYER_NAME: usize = 32;
```

`Font` and `ClockFmt` are re-exported from `luxel_core::text`. Scene ids are
host-assigned 8-hex (device: like pattern ids; mirror: counter-hash;
playground: random); pattern and scene ids are different namespaces.

## 2. Compositing (`luxel_core::compose`)

Frames are RGB888 `[[u8; 3]]`. Everything composites in **row-major canvas
space**: a `Canvas` carries the frame plus the `GridMap` that maps a
`(row, col)` cell onto a pixel index, the way `bulk.rs`'s `paste` addresses a
frame. Serpentine wiring, panel rotation and the output chain stay the
existing pipeline's business.

Per pixel, with `B` beneath, `L` the layer and `α = opacity/100 × key factor`:

| key | factor |
|---|---|
| `none` | 1 |
| `black` | 0 if `L == [0,0,0]`, else 1 |
| `luma` | `luma(L)/255` |

| blend | result |
|---|---|
| `normal` | `B + α(L−B)` |
| `add` | `min(255, B + αL)` |
| `lighten` | `max(B, αL)` per channel |
| `multiply` | `B·L/255`, faded back to `B` by `α` |
| `mask` | `B·luma(L)/255`, faded back to `B` by `α` |

A keyed-out pixel (`α = 0`) leaves `B` untouched in **every** mode — which is
what makes `black` usable under `add` and `lighten` too. Integer math
throughout; `α` is carried in 1/65536ths so the crossfade percentages are
exact.

### The crossfade, generalized

The firmware's timed crossfade (`blend_px`: `(a·(65536−t) + b·t) >> 16`) is
the degenerate case — **two layers, `normal`, `key none`, `opacity = t`** —
and `composite_frame` reproduces it bit for bit (unit-tested at
t ∈ {0, 0.25, 0.5, 1}). A scene→scene transition composites the incoming
scene's layers on top at `opacity = t` and drops the outgoing stack when `t`
reaches 100.

### Geometry

`w` or `h` = 0 means "the whole layout" on that axis. The box clips at every
edge; a box wholly outside the grid draws nothing. `rot180` is `flipx` and
`flipy` together, so the three flags collapse to two mirrors, applied
**within the box**.

`fit`, v1:

* `pat` — the box CLIPS a full-layout render. The pattern still sees the whole
  grid; the layer shows a window onto it. `fit` is otherwise ignored.
* `sprite` — `fill` and `contain` place the sprite 1:1 at the box origin;
  `tile` repeats it across the box.
* `text` — ignored.

Like every `bulk.rs` kernel, a draw op is a **silent no-op without a regular
grid**.

### API

```rust
pub struct Canvas<'a> { pub px: &'a mut [[u8; 3]], pub grid: &'a GridMap }

pub fn composite_frame(dst: Canvas, src: &[[u8; 3]], style: &LayerStyle);
pub fn fill_color(dst: Canvas, rgb: [u8; 3], style: &LayerStyle);
pub fn blit_sprite(dst: Canvas, sprite: &SpriteView, frame: u16, style: &LayerStyle);
pub fn draw_text_layer(dst: Canvas, scratch: &mut [[u8; 3]], text: &str,
                       layer: &TextLayer, style: &LayerStyle, scroll_px: i32);
pub fn blend_px_mode(dst: &mut [u8; 3], src: [u8; 3], mode: Blend, alpha: i32);
```

`draw_text_layer` renders the glyphs into a caller-owned full-frame `scratch`
and composites that black-keyed — which is how a text layer gets box
clipping, blend modes and flips for free without `text::draw` needing an
alpha channel. Text is therefore **always black-keyed**, like a sprite.

### `Compositor` — the host-facing driver

```rust
impl Compositor {
    pub fn new(grid: GridMap) -> Self;
    pub fn set_grid(&mut self, grid: GridMap);
    pub fn grid(&self) -> GridMap;
    pub fn set_scene(&mut self, scene: &Scene);   // rebuilds layer runtime state
    pub fn set_text(&mut self, layer: usize, s: &str);
    pub fn text_source(&self, layer: usize) -> Option<&TextSource>;
    pub fn advance(&mut self, dt_ms: u32);        // scroll + sprite frame clocks
    pub fn begin(&mut self, dst: &mut [[u8; 3]]);  // clears dst to black
    pub fn pattern_layer(&mut self, dst: &mut [[u8; 3]], layer: usize, src: &[[u8; 3]]);
    pub fn native_layer(&mut self, dst: &mut [[u8; 3]], layer: usize, sprite: Option<&SpriteView>);
    pub fn layer_count(&self) -> usize;
    pub fn layer_kind(&self, layer: usize) -> Option<LayerKind>;
    pub fn layer_kinds(&self) -> impl Iterator<Item = LayerKind> + '_;
    pub fn resident_bytes(&self) -> usize;
}
```

Layers are drawn in order, bottom → top: `pattern_layer` with that layer's
engine frame, or `native_layer` for text / sprite / colour. The compositor
owns the scroll phase, the sprite frame clock, the resolved text buffers
(≤ 64 B) and the per-layer ramp LUT cache; there is **one** shared 3 B/px
scratch, allocated lazily, so the single-owner rule the HUB75 pipeline
depends on is not broken.

**Clock and slot text are the HOST's to resolve.** The compositor reads no
wall clock and no slot table: `text_source(i)` says what a layer wants and
`set_text(i, s)` supplies it, truncated to 64 B on a char boundary.

### `SceneDriver` — the one full-frame walk (Gitea #732)

No host writes that walk itself. `SceneDriver::frame` is a whole frame —
the buffer sizing, the millisecond accounting, the clock advance and the
layer-kind dispatch — and the firmware's render task, the `luxel serve`
mirror and the wasm playground each call it with a `SceneHost` supplying
the only two things that genuinely differ per host: where layer *i*'s
engine frame (or sprite) comes from, and what string a text layer draws.

```rust
pub trait SceneHost {
    fn pattern_frame(&mut self, layer: usize, delta: Fx) -> Option<&[[u8; 3]]>;
    fn sprite(&mut self, layer: usize) -> Option<SpriteView<'_>>;
    fn text(&mut self, layer: usize, source: &TextSource) -> Option<&str> { None }
}

pub struct SceneDriver { /* the sub-ms remainder */ }
impl SceneDriver {
    pub const fn new() -> Self;
    pub fn step_ms(&mut self, delta: Fx) -> u32;
    pub fn frame<H: SceneHost + ?Sized>(&mut self, comp: &mut Compositor,
        dst: &mut Vec<[u8; 3]>, n: usize, delta: Fx, host: &mut H) -> bool;
}
```

`None` from `pattern_frame` or `sprite` means the layer has nothing to draw
from — unbound in the console, over budget or undecodable on the device —
and that layer simply does not draw. `None` from `text` leaves the layer's
text as it stands, which is what a `lit` layer wants (`set_scene` seeds it)
and what a host that pushes resolved text in out of band wants (the wasm
binding's `lx_comp_text`).

Two properties the driver is required to keep, one from each host it
replaced:

* **The destination is sized fallibly.** `frame` returns `false` when `dst`
  cannot be grown to `n` and draws nothing; the clocks still advance. On a
  board whose largest free block is a few kilobytes the host's staging
  buffer is exactly the allocation that fails, and a frame the device
  cannot afford is a frame not drawn, not a reboot (#702, #728). The
  capacity survives, so this is one `try_reserve_exact` per activation and
  a compare per frame after it — and the `resize` that follows leaves `dst`
  black, which is what `begin` would do (`begin` stays for hosts that clear
  a buffer they size themselves).
* **The frame delta's sub-millisecond remainder is carried.** `step_ms`
  accumulates raw 16.16 ms and hands `advance` whole milliseconds, so sixty
  16.666… ms steps are one second of scroll and not 960 ms of it. The
  firmware truncated per frame until #732, which is why a caption crawled
  ~4 % slower on the panel than in the preview showing that same panel.

`crates/luxel-core/tests/scene_driver.rs` pins the driver frame by frame
over a deliberately non-integral delta sequence, and `tools/wasm-smoke.mjs`
asserts the carry across the C ABI.

### Per-layer colour ramp

`R` cooks a 256-entry luma → colour LUT (768 B) with
`outpipe::fill_palette_lut`, cached behind the compositor's scene epoch, and
runs `outpipe::palette_remap_frame` over a **scratch copy** of the pattern's
frame at `pct·256/100` — the engine's own buffer is never written. This is
the same output-palette stage the Settings page applies device-wide, per
layer.

## 3. `/api/scenes` JSON

```json
{"id":"…","name":"…","layers":[{
  "type":"pat","name":"…","x":0,"y":0,"w":64,"h":64,
  "blend":"normal","opacity":100,"key":"none","fit":"fill",
  "visible":true,"flipx":false,"flipy":false,"rot180":false,
  "pat":{"id":"…","controls":{"speed":[0.5]},"proj":"x",
         "ramp":{"pct":100,"stops":[[0,"000000"],[255,"ffffff"]]}}
}]}
```

The last member is one of:

* `"pat": {...}` as above — `proj` and `ramp` absent when unset; control
  values decimal, like the playlist's GET.
* `"text": {"source":"lit","text":"HI" | "source":"clock","fmt":"HH:MM" | "source":"slot","slot":0,
  "font":"regular","color":"ffffff","align":"l","scroll":"none","speed":0}`
* `"sprite": {"id":"…"}`
* `"color": "ff8800"`

`GET /api/scenes` wraps them:
`{"active":"<id>"|null,"layers_max":N,"used":<blob bytes>,"max":3840,"scenes":[…]}`.
Routes, storage (`SCENES_KEY`, `BLOB_MAX` 3840 B) and the playlist `I S<id>`
item are in `docs/api.md`.

## 4. Sprites — a sprite-tagged PATTERN

A sprite is not a new store record: it is an ordinary pattern whose **first
source line** is the tag

```
// @sprite w=<w> h=<h> frames=<n> fps=<f>
```

with `w,h ≤ 64`, `frames ≥ 1`, `fps` 0..30 (`0` = static). Keys may appear in
any order and unknown keys are ignored. Immediately after it, three top-level
array literals in this order and with these names, each `w*h*frames` elements
in row-major, frame-major order:

```js
var sprH = [...]   // hue        0..1
var sprS = [...]   // saturation 0..1
var sprV = [...]   // value      0..1 — v = 0 is TRANSPARENT
```

Then an ordinary `renderFrame` body that plays the sprite alone with
`blit(sprH, sprS, sprV, w, h, col, row, 3)` (mode 3 = black-keyed), cycling
frames at `fps`. So a sprite compiles, stores, previews, plays and shares
exactly like a pattern; the tag is the only thing that marks it, and it is
how the compositor, the store browser and the editor all tell the two apart.
The ≤ 16-colour palette is an EDITOR rule, not a format rule.

**Why top-level literals.** An all-numeric array literal interns into the
program's const pool (`ArrView::Const`), whose words are the program's own —
memory-mapped flash on the device. Top-level initialization runs when the
engine is BUILT, so the compositor reads a sprite's pixels **without ever
stepping it**, and the pixels cost no RAM. Pinned by
`compose::tests::sprite_arrays_are_const_pool_entries_before_any_frame`.

```rust
pub struct SpriteTag { pub w: u16, pub h: u16, pub frames: u16, pub fps: u8 }
pub fn parse_sprite_tag(source: &str) -> Option<SpriteTag>;

pub struct SpriteView<'a> { pub w: u16, pub h: u16, pub frames: u16, pub fps: u8,
                            pub h_: ArrView<'a>, pub s: ArrView<'a>, pub v: ArrView<'a> }
pub fn sprite_view<'a>(engine: &'a Engine, source: &str) -> Option<SpriteView<'a>>;
```

Sprites are **always black-keyed** whatever the record's `key` says — `v = 0`
quantizes to `[0,0,0]`, which is the format's transparency and the rule the
library sprites already rely on. The frame shown is
`(elapsed_ms · fps / 1000) mod frames`.

## 5. wasm (`crates/luxel-wasm`, C ABI)

```
lx_comp_new(w: u32, h: u32) -> i32      // handle; row-major grid
lx_comp_free(ch: i32)
lx_comp_set(ch, ptr, len) -> i32        // 0 ok; -1 + the parse error in the response buffer
lx_comp_bind(ch, layer: u32, engine_handle: i32)   // pat AND sprite layers; -1 unbinds
lx_comp_text(ch, layer: u32, ptr, len)
lx_comp_frame(ch, delta_raw: i32) -> *const u8     // w·h·3 RGB, valid until the next call
lx_comp_layer_count(ch) -> u32
```

`lx_comp_frame` steps every bound **pattern** engine through `Engine::frame`,
exactly as `lx_frame` does (frame-rate cap and time scaling included), then
composites the stack. A **sprite** layer is bound to an engine too, but only
its program's data arrays are read — it is never stepped. Feed the composite
through `lx_outpipe` the way a pattern frame is fed, to see what the wire
would carry.

TypeScript wrapper: `Luxel.compositor(w, h)` → `Compositor` with
`setScene(wire) → string | null`, `bind(layer, engine | null)`,
`setText(layer, s)`, `layerCount()`, `frame(dtMs) → Uint8Array`, `free()`
(`web/src/lib/luxel.ts`). `Engine.handle` exposes the wasm handle `bind`
takes.
