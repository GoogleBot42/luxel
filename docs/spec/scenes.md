# Scenes — wire format, compositing, sprites

A **scene** is an ordered stack of layers drawn onto one frame. The record,
the blend kernels, the sprite format and the JSON shape all live in
`luxel-core` (`scene.rs`, `compose.rs`) so the firmware, the `luxel serve`
mirror and the wasm playground cannot disagree about any of them. Hosts only
plumb.

Gitea #477 (blend), #478 (record + native layers), #481 (sprites),
#482 (composite thumbnails), #985 (strips), #997 (design size). Design: `docs/design/webui-v2/proposal.md` §5.5,
§5.5b.

## 1. The wire record

One block of `\n`-separated lines. Layers are listed **bottom → top**: the
first `L` is the base. Grammar, spacing and forward-compatibility rules are
the playlist's.

```
S <id> <name…>                     id = 8 lowercase hex, or `-` on POST
                                   ("assign one"); name = rest of line,
                                   ≤ 64 B UTF-8, no newline
U <w> <h>                          optional design size, each 1..4096; between
                                   the S and the first L (§2 "Design size")
L <type> <x> <y> <w> <h> <blend> <opacity> <key> <fit> <flags>
   type     pat | text | sprite | color
   x y w h  integers in layout pixels — DESIGN units in a scene with a `U`
            line (§2 "Design size"); the layer's box. w or h = 0 → the rest of
            the layout from x / y, the whole layout at 0 (§2 "Geometry"; a
            fill/contain sprite's natural size instead — §4); on a strip the
            box is a segment: x, w along it, y 0, h 0 or 1 (§2 "The canvas")
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
| `I <id>` | pat, sprite | store id (8 hex): a pattern id on `pat`, a SPRITE id on `sprite` (own namespace, §4) |
| `A <fps>` | sprite | frame-rate override, 0..30 (`0` = still); outranks the record's own `fps` |
| `C <name> <raw…>` | pat | control override, raw 16.16 ints — playlist grammar |
| `P <mode>` | pat | projection override (`index\|x\|y\|z\|xy\|xz\|yz`) |
| `R <pct> <pos>:<rrggbb> …` | pat | colour ramp; `pct` 0..100, ≥ 2 stops, `pos` 0..255 ascending, ≤ 32 stops |
| `T <source> <arg…>` | text | `lit <utf8…>` \| `clock <fmt>` \| `slot <n 0..7>` |
| `F <font> <rrggbb> <align> <scroll> <speed>` | text | font `tiny\|regular\|large`; align `l\|c\|r`; scroll `none\|left\|right\|up\|down\|bounce`; speed px/s |
| `K <rrggbb>` | color | the wash colour |
| `X <flag…>` | all | per-layer flags past the `L` line's 0..15: `clip` (§2 "Clip layers"), `abs` (§2 "Design size": this layer keeps grid pixels); unknown words ignored |
| `M <ms> <x> <y> <opacity> <ease> [<frame>]` | all | one keyframe of the layer's motion — below |

`<fmt>` ∈ `HH:MM` · `HH:MM:SS` · `hh:MM` · `hh:MM:SS` · `MM-DD` · `YYYY-MM-DD`.

**Defaults** when a binding line is absent: `N` = `Text` / `Sprite` / `Color`,
and the EMPTY string for a `pat` layer (only the host knows a pattern's name);
`F regular ffffff l none 0`; `T lit ` (empty); `K 000000`; no controls, no
projection override, no ramp, no `A` — a sprite layer without one plays
the record's own `fps` (§4), no `X` (not clipped, not pinned) — and no `M`:
the layer stands where `L` puts it; no `U` (layout pixels). `serialize`
writes the `U` line straight after the `S` line, and a layer's ONE `X` line
(`X clip`, `X abs` or `X clip abs`, in that order) after its body lines and
before its `M` lines.

### Keyframes (`M` lines)

A layer's `M` lines are its **track**: where its box origin and opacity are,
and which sprite frame it holds, at a time measured from the scene's start.

```
M <ms> <x> <y> <opacity> <ease> [<frame>]
   ms       0..3600000, ascending (equal = an instant jump); the LAST key's ms
            is the loop period
   x y      the box ORIGIN in layout px — design units under `U`, mapped
            exactly like the L origin (§2 "Design size"); the box size is
            the L line's; `=` keeps the L line's value
   opacity  0..100, or `=`
   ease     linear | step | in | out | inout | sine | back | bounce | elastic
            — the curve of the segment that ENDS at this key
   frame    optional, 0..255: the sprite frame held from this key until the
            next one (ignored on non-sprite layers)
```

| ease | curve (the pattern builtin it shares a body with, `luxel_core::ease`) |
|---|---|
| `linear` | straight |
| `step` | holds the previous key, jumps on arrival |
| `in` / `out` / `inout` | `easeInCubic` / `easeOutCubic` / `easeInOutCubic` |
| `sine` | `easeInOutSine` |
| `back` | `easeOutBack` (overshoots, then settles) |
| `bounce` | `easeOutBounce` |
| `elastic` | `easeOutElastic` |

Evaluation (`compose::eval_keys`, run by `Compositor::advance` every frame and
by `set_scene` before the first one): the clock is ms since the scene was set,
modulo the period (a period of 0 — one key, or all at 0 — is a static pose).
Before the first key the first key's values hold; between two keys the values
run from the earlier to the later along the LATER key's ease, and round to
whole layout pixels (v1 — no sub-pixel blit). Opacity clamps to 0..100 under
an overshooting ease; positions do not. A key's `frame` outranks both the
layer's `A` rate and the record's own clock until the next key; a key without
one hands the sprite back to its clock. The track's clock carries over a
`set_scene` like the scroll phase (§2), so editing a scene does not restart
its motion. A seamless loop repeats its first key's values at the last.

At most **16** keys a layer (`too many keyframes (max 16)`), which is what
bounds a track's share of the 3,840 B scene blob; a key earlier than the one
before it is `keyframe times must not go backwards`, and any bad field is
`bad M field "<token>"`. An older host ignores `M` lines (unknown tag), so a
keyed scene still draws there — standing still.

**Errors.** Any parse error rejects the whole block with a message naming the
line: `scene: line 4: unknown blend "foo"`. That string is what the API hands
the console verbatim.

**Forward compatibility.** An unknown line tag is ignored, and so is a binding
line that does not apply to the layer it follows — an older host survives a
newer console's push. So is a binding line that arrives BEFORE the first
`L`: it has no layer to bind to and reads as an unknown line (both parsers
have done this since the first scene commit, 30938c29 — an earlier version
of this paragraph called it an error, which the code never did). That rule
is what makes the `U` line safe on older firmware: such a host ignores it
and draws the scene's numbers 1:1, and nothing in the stored blob is
rejected. Any line before the `S` IS an error, and so — on a host that knows
`U` — is a `U` after the first `L` (`a U line after the first L line`) or
one whose fields are not two integers 1..4096 (`bad U field "<token>"`;
trailing words are ignored like any line's). `A` is the one exception to the
ignore rule: on a `pat`/`text`/`color` layer it is `scene: line N: A on a
non-sprite layer`, because a frame rate the user typed is worth naming
rather than dropping.

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
pub enum LayerBody { Pattern(PatternLayer), Text(TextLayer),
                     Sprite { id: String, fps: Option<u8> },   // fps = the `A` override
                     Color([u8; 3]) }
pub enum Ease { Linear, Step, In, Out, InOut, Sine, Back, Bounce, Elastic }
pub struct Keyframe { pub ms: u32, pub x: Option<i16>, pub y: Option<i16>,   // None = `=`
                      pub opacity: Option<u8>, pub ease: Ease, pub frame: Option<u8> }
pub struct Layer { pub name: String, pub style: LayerStyle, pub body: LayerBody,
                   pub keys: Vec<Keyframe>, pub clip: bool,   // clip = `X clip`
                   pub abs: bool }                            // abs = `X abs`
pub struct Scene { pub id: String, pub name: String,
                   pub space: Option<(u16, u16)>,             // the `U` line
                   pub layers: Vec<Layer> }

pub fn parse(block: &str) -> Result<Scene, String>;
pub fn parse_all(blob: &str) -> Result<Vec<Scene>, String>;
pub fn serialize(s: &Scene, out: &mut String);
pub fn push_json(s: &Scene, out: &mut String);
pub fn pattern_layers(s: &Scene) -> usize;      // counts pat layers only
pub fn valid_id(s: &str) -> bool;               // 8 lowercase hex
pub const MAX_NAME: usize = 64;
pub const MAX_LAYER_NAME: usize = 32;
pub const MAX_KEYS: usize = 16;
pub const MAX_KEY_MS: u32 = 3_600_000;
pub const MAX_SPACE: u16 = 4096;                // per axis of a `U` line
```

`Font` and `ClockFmt` are re-exported from `luxel_core::text`. Scene ids are
host-assigned 8-hex (device: like pattern ids; mirror: counter-hash;
playground: random); pattern and scene ids are different namespaces.

## 2. Compositing (`luxel_core::compose`)

Frames are RGB888 `[[u8; 3]]`. Everything composites in **row-major canvas
space**: a `Canvas` carries the frame plus the `GridMap` that maps a
`(row, col)` cell onto a pixel index, the way `bulk.rs`'s `paste` addresses a
frame (row-major, serpentine, or — Gitea #948 — a chain's wire-order tiling,
which `blend_box` walks cell by cell). Serpentine wiring, panel rotation and the output chain stay the
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

### The canvas

Every host composites onto ONE grid, derived from the device's **Layout**
(the installed map), never from whichever pattern happens to be running:

| Layout | canvas |
|---|---|
| a strip — no map at all, or a 1-D map (`dims` 1) | `n`×1, **one row in index order** (`outpipe::strip_grid`, Gitea #985) |
| a regular 2-D grid — a panel, a `matrix`, a 2-D map `outpipe::detect_grid` recognises | that grid (tiled in wire order on a HUB75 chain, #948) |
| an irregular 2-D map, any 3-D map | none — every layer draws nothing |

The strip row holds for any `n` from 1 to 65,535 and ignores a 1-D map's
coordinates: index order is the strip's own order, which is the order its
pattern layers write their frames in. A 2D pattern on a mapless strip still
renders on the square grid the engine fabricates for it — the canvas is the
row regardless, so the pattern's frame is read in index order like any
other. The ENGINE's grid is untouched (`detect_grid` still refuses a 1-D
map), so no pattern sees anything new: only the compositor gets the row.

On a strip a box is a **segment**: `x` and `w` place it along the strip, `y`
is 0 and `h` is 0 or 1 — both mean the one row (a `y` past 0 is off the
canvas and draws nothing, like any box outside the grid). `flipx` reverses
the segment in place (`flipy` is a no-op on one row), `fit tile` repeats a
1-row sprite along it, and ramps, masks, clip layers, keyframes (`M` lines
move `x`) and every blend mode work unchanged — they never knew the grid's
shape. A WLED-style "zones" scene on a 150 px strip is two pattern layers on
two segments — pixels 0..74 show the first pattern's own 0..74, pixels
75..149 the second pattern's 75..149 run backwards (`flags` 3 = visible +
`flipx`):

```
S - Zones
L pat 0 0 75 0 normal 100 none fill 1
I 1a2b3c4d
L pat 75 0 75 0 normal 100 none fill 3
I 5e6f7a8b
```

**Text draws nothing on a one-row canvas.** No glyph fits one row, so a
text layer there is skipped outright — no error, no partial glyph tops, and
no coverage for a clip layer above it (such a clip layer draws nothing,
exactly as over empty text). The same scene on a panel draws its text.

### Geometry

`w` or `h` = 0 means **the rest of the layout from the box's origin** on that
axis: `grid.w − x` for an `x` > 0 (and `grid.h − y` for a `y` > 0), the whole
layout for an origin at 0 or before it (Gitea #1026). Before #1026 a zero
axis was the layout's full extent wherever it started, so `75 0 0 0` on a
150 px strip was the box 75..224: unflipped it clipped to the same pixels,
but `flipx` mirrored the whole overhanging box and sampled past the end —
the visible half drew black. Now that box is 75..149 and a flip mirrors
exactly it, which makes `L pat 75 0 0 0 … fill 3` a length-independent
second zone. Every shipped scene's pattern, colour and sprite boxes either
start at 0 or are unflipped, so none changed (the `library_scenes.rs`
digests, all four grids). A TEXT layer keeps the full-extent frame for its
alignment and scroll wrap (so `neon-sign`'s glow copies at `x` ±1 still sit
one pixel either side of the tube) and composites through the rest-of-layout
box. The box clips at every edge; a box wholly outside the grid draws nothing. `rot180` is `flipx` and
`flipy` together, so the three flags collapse to two mirrors, applied
**within the box**.

`fit`, v1:

* `pat` — the box CLIPS a full-layout render. The pattern still sees the whole
  grid; the layer shows a window onto it. `fit` is otherwise ignored.
* `sprite` — §4 "Placement": a zero `w` (or `h`, off a strip) is the natural
  size for `fill`/`contain` and the whole layout for `tile`.
* `text` — ignored.

Like every `bulk.rs` kernel, a draw op is a **silent no-op without a canvas**
(the last row of the table above).

### Design size (`U <w> <h>`, Gitea #997)

A scene's boxes are layout pixels, so a composition authored on one panel
sits in the top-left corner of a bigger one, and a 5×8 glyph is 8 of 128
rows. A `U <w> <h>` line declares the size the scene was **designed at**;
its boxes and keyframe positions are then design units, mapped onto the real
grid once, when the scene is set (`Compositor::set_scene`) and again when
the grid changes (`set_grid`) — no per-pixel and no per-frame cost beyond
mapping a keyed layer's two current keys.

```
S - Fire Clock
U 32 32                       coordinates are in a 32×32 design space
L text 0 16 0 0 normal 100 none fill 1
T clock HH:MM
F large 5a1404 c none 0
L pat 0 0 0 0 lighten 100 none fill 1
I @pat/perlin-fire
X clip
```

With the design size `(rw, rh)` and the canvas `(gw, gh)` (`n`×1 on a
strip, "The canvas" above):

```
no U line, or the layer has X abs  →  identity: the L numbers, 1:1
strip  (rh == 1 || gh == 1):  f = gw/rw (rational), ox = 0; y and h are not mapped
panel:
   z0 = min(gw / rw, gh / rh)                      integer division
   z0 ≥ 1 → f = z0                                 whole-number zoom
   z0 = 0 → f = min(gw/rw, gh/rh)                  rational, by cross-multiplying
   ox = (gw − ⌊rw·f⌋) / 2,  oy = (gh − ⌊rh·f⌋) / 2   the design area centred
z = max(1, ⌊f⌋)    zooms natural-size sprites (sw·z × sh·z), tile cells, glyph
                   cells, advance and line height, and the scroll speed
                   (an `F` speed is design px/s, ×z)
map(v) = ⌊v·f⌋     i64, floor division (monotone for negative v)
placed axis (n > 0): a = o + map(v),  b = o + map(v + n),  size = max(1, b − a)
zero axis   (n = 0): origin = map(v) with NO centring offset, size 0 (the rest of the real layout)
natural sprite:      both origins o + map(v), drawn at sw·z × sh·z
M key x/y:  mapped exactly like the L origin on that axis, BEFORE the
            interpolation; `=` stays the L origin (mapped)
```

* **Edges are mapped, not sizes**, so two adjacent design boxes stay
  adjacent at any `f` — no gap, no overlap (a strip's zones stay gapless at
  any length).
* **A placed axis never resolves to 0** (`max(1, …)`): a resolved size of 0
  would flip the box to "the whole layout".
* **A zero axis is still the whole REAL layout**: `0 0 0 0` covers a
  letterboxed panel's margins too, which is what a backdrop wants. Its origin
  is scaled but not offset, so on a panel letterboxed vertically a zero-`h`
  text box at `y` 16 lands 16·f from the top of the PANEL, not of the design
  area; give the box an `h` to place it inside the design area.
* **Below the design size** (`z0` = 0, e.g. a `U 32 32` scene on a 16×16
  matrix) positions and box-fit content shrink; pixels never do — natural
  sprites, tile cells and glyphs stay 1:1.
* **A keyed layer keeps its `L` box's mapped size** while its origin moves;
  the track interpolates mapped (grid) positions, so motion on a ×4 panel
  moves in 1-px steps rather than 4-px jumps.
* **`X abs`** opts ONE layer out: its numbers stay grid pixels, its content
  1:1, its scroll speed grid px/s. Absolute and design-unit layers mix freely
  (a fine snowfall tile over a scaled tree). In a scene without `U` it means
  nothing.
* `fill`/`contain` sprites already scale nearest-neighbour to their box,
  which now maps; clip layers are per pixel and need nothing; text still
  draws nothing on a one-row canvas.
* **Backwards compatibility**: a scene without `U` takes an early return
  (`Place::IDENTITY`) and draws byte-identically —
  `crates/luxel-core/tests/library_scenes.rs` digests every shipped scene at
  64×64, 32×32, 128×128 and 150×1, and printed the same 176 lines (44 scenes × 4 grids) on
  `origin/master` and on the #997 branch. On firmware older than #997 a `U`
  scene draws at its design numbers 1:1 (§1 "Forward compatibility") —
  exactly the picture it had before it declared one.

Why one integer zoom and not percent fields or anchors: a new token INSIDE
the `L` line (`10%`) fails an older host's integer parse, so the stored blob
is rejected and the device starts with no scenes — an OTA rollback, which
the boot-loop guard can trigger on its own, would wipe them all; 1 % steps
are also 1.28 px at 128 and 3 px on a 300 px strip, and percent does nothing
for glyph or sprite size. Anchors (`c`, `b`, `tr`) centre a fixed-size
composition but do not scale it — on 128×128 everything is still tiny, just
in the middle. A per-axis 0..1 fixed point distorts the aspect on 64×32 and
gives pixel art no integer zoom. Boxes, sprites and glyphs scaling by ONE
integer keeps a fire box over its fire sprite and a scrim behind its text,
and a composition that fits its design size fits every multiple of it. The
full rationale is `docs/design/scenes-designed-size.md`.

Worked examples (`U 32 32` unless said):

**A. fire-clock** — `L text 0 16 0 0`, large "12:34" (30 px advance), `c`:

| canvas | f, z | box | text anchor | glyph rows |
|---|---|---|---|---|
| 32×32 | 1, 1 | y16, full width | x1 | 16–23 |
| 64×64 | 2, 2 | y32 | x2 | 32–47 |
| 128×128 | 4, 4 | y64 | x4 | 64–95 |
| 150×1 | strip | text draws nothing, so a clip fire above it draws nothing | | |

**B. campfire** — embers box `8 0 16 28`, fire sprite natural 16×14 at
`8 17`, ground `0 30 0 0`:

| canvas | embers box | fire sprite | ground |
|---|---|---|---|
| 32×32 | [8,24)×[0,28) | (8,17) 16×14 | rows 30+ |
| 64×64 | [16,48)×[0,56) | (16,34) 32×28 | rows 60+ |
| 128×128 | [32,96)×[0,112) | (32,68) 64×56 | rows 120+ |
| 64×32 | z0=1, ox=16: [24,40)×[0,28) | (24,17) 16×14 | rows 30+, full width (zero axis) |

**C. neon-sign glow at 128** — `L text -1 12 0 0`: `x` is a zero axis, so
its origin is map(−1) = −4 and the box is the real width; the glyphs are 4×
and centre-aligned, so the glow copy sits exactly one scaled pixel (4 px)
left of the tube.

**D. strip zones** — `U 150 1`, `L pat 0 0 75 0` and `L pat 75 0 75 0 … 3`:

| strip | f | zone 1 | zone 2 |
|---|---|---|---|
| 150 | 1 | [0,75) | [75,150) |
| 300 | 2 | [0,150) | [150,300) |
| 61 | 61/150 | [0,30) | [30,61), gapless |

**E. sunrise keyframes at 128** — the box `4 3 22 22 contain` maps to
(16,12) 88×88 and `contain` fits an 11×11 sun at 8×; `M 0 0 24 0 linear` maps to
(0,96) and `M 20000 8 24 0 in` to (32,96), and the rise is interpolated in
grid pixels.

**F. silent-night snow, `X abs`** — `L sprite 0 0 0 0 … tile 1` + `X abs`:
the 16×16 snowfall tile repeats 1:1 over the whole 128×128, so the flakes
stay fine, while the tree beside it (`4 6`, 24×26) maps to (16,24) 96×104.

The unit tests next to `compose::Place` pin each rule (zoom and centring,
the never-0 rule, strips proportional and gapless at every length 1..400,
below-design shrink, `X abs`, keys in design units, `set_grid` re-mapping,
scaled glyphs, natural sprites and tile cells, the scroll speed).

### API

```rust
pub struct Canvas<'a> { pub px: &'a mut [[u8; 3]], pub grid: &'a GridMap }

pub fn composite_frame(dst: Canvas, src: &[[u8; 3]], style: &LayerStyle);
pub fn fill_color(dst: Canvas, rgb: [u8; 3], style: &LayerStyle);
pub fn blit_sprite(dst: Canvas, sprite: &SpriteView, frame: u16, style: &LayerStyle); // zoom 1
pub fn draw_text_layer(dst: Canvas, scratch: &mut [[u8; 3]], text: &str,
                       layer: &TextLayer, style: &LayerStyle, scroll_px: i32);
                       // draws nothing on a one-row grid
pub fn blend_px_mode(dst: &mut [u8; 3], src: [u8; 3], mode: Blend, alpha: i32);
pub fn eval_keys(keys: &[Keyframe], base: &LayerStyle, t_ms: u32) -> KeyState; // §1 "Keyframes"

// "Design size" above: a scene's `U` resolved against a grid
pub struct Axis { pub num: i32, pub den: i32, pub off: i32 }   // v ↦ off + ⌊v·num/den⌋
pub struct Place { pub x: Axis, pub y: Axis, pub zoom: u16 }
impl Place {
    pub const IDENTITY: Place;                                  // no `U`: 1:1
    pub fn new(space: Option<(u16, u16)>, grid: &GridMap) -> Place;
}

// luxel_core::text — glyphs magnified by a whole number (z 1 = draw_aligned)
pub fn draw_scaled(dst: &mut [[u8; 3]], grid: &GridMap, x: i32, y: i32, text: &str,
                   font: Font, rgb: [u8; 3], align: Align, z: u16) -> i32;

// luxel_core::outpipe — the canvas for a 1-D Layout ("The canvas" above):
// Some(n×1) for layout_dims 0 (no map) or 1, n in 1..=65535; None otherwise
pub fn strip_grid(layout_dims: u8, n: usize) -> Option<GridMap>;
```

`draw_text_layer` renders the glyphs into a caller-owned full-frame `scratch`
and composites that black-keyed — which is how a text layer gets box
clipping, blend modes and flips for free without `text::draw` needing an
alpha channel. Text is therefore **always black-keyed**, like a sprite — so
`mask` on a text layer cannot confine a pattern to the glyphs (the keyed-out
pixels leave what is beneath untouched). Filling text with a pattern is a
**clip layer** above it, below.

### Clip layers (`X clip`)

A layer with `X clip` draws only where the layer beneath it drew: its α at
each pixel is multiplied by that layer's **coverage** — 1 where the layer
below had α > 0 after its own key, opacity and box, 0 elsewhere. Photoshop's
clipping mask: `[background pat] [text] [fire pat, X clip]` is fire inside
the glyphs over the background, and a sprite below works the same way (its
texels are the coverage — index 0 is not, an opaque black texel is). The
clip layer keeps its own blend, opacity, key and box inside that shape.

* A run of clip layers all clip to the **base** under the run — the nearest
  non-clip layer below — as in Photoshop, so `[text] [fire, X clip]
  [sparks add, X clip]` puts both inside the text.
* `X clip` on the first layer (or on a run that starts at the first layer)
  is a no-op: there is nothing below to clip to.
* A base that draws nothing this frame (an unbound pattern, a sprite id the
  store no longer has, empty text) has no coverage, so its clip layers draw
  nothing either.
* The coverage is **1 bit per pixel** (512 B at 4096 px, 2 KB at 16384 px),
  one mask per compositor, taken fallibly like the text scratch — out of
  the external arena (PSRAM) on boards that have one — and only while a
  scene with a clip layer is drawing. A frame whose base cannot get the mask
  composites its clip layers **unclipped** (the treatment is dropped, never
  the layer — #702's rule). It is indexed by frame slot, so serpentine
  wiring and tiled chains need nothing special; the base and its clip
  layers take the per-pixel path instead of the row kernel, every other
  layer is unchanged.

`X` is a binding line rather than flag bit 16 because an older host REJECTS
`flags > 15` (`flags must be 0..15` refuses the whole scene) but ignores an
unknown line: there, the clip layer draws unclipped.

### `Compositor` — the host-facing driver

```rust
impl Compositor {
    pub fn new(grid: GridMap) -> Self;
    pub fn set_grid(&mut self, grid: GridMap);
    pub fn grid(&self) -> GridMap;
    pub fn set_scene(&mut self, scene: &Scene);   // rebuilds layer runtime state
    pub fn set_text(&mut self, layer: usize, s: &str);
    pub fn text_source(&self, layer: usize) -> Option<&TextSource>;
    pub fn advance(&mut self, dt_ms: u32);        // scroll, sprite and keyframe clocks
    pub fn begin(&mut self, dst: &mut [[u8; 3]]);  // clears dst to black
    pub fn pattern_layer(&mut self, dst: &mut [[u8; 3]], layer: usize, src: &[[u8; 3]]);
    pub fn native_layer(&mut self, dst: &mut [[u8; 3]], layer: usize, sprite: Option<&SpriteView>);
    pub fn layer_count(&self) -> usize;
    pub fn layer_kind(&self, layer: usize) -> Option<LayerKind>;
    pub fn layer_kinds(&self) -> impl Iterator<Item = LayerKind> + '_;
    pub fn place(&self) -> Place;                 // the `U` line on this grid
    pub fn layer_rect(&self, layer: usize, sprite: Option<&SpriteView>)
        -> Option<(i32, i32, i32, i32)>;          // where it draws NOW, below
    pub fn layer_zoom(&self, layer: usize) -> u16;
    pub fn resident_bytes(&self) -> usize;
}
```

`layer_rect` is the layer's box after the design size and after its
keyframe track, in grid cells, with a zero `w`/`h` turned into the whole
layout on that axis — except a natural-size sprite, which answers its
record's size × `layer_zoom` when `sprite` is given and `w`/`h` 0 when it
is not (the compositor holds no records). `layer_zoom` is 1 without a `U`
line and on an `X abs` layer. The wasm binding exports both as
`lx_comp_layer_rect` (§5).

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

**The ramp clamps at both ends.** Below the first stop the first stop's
colour holds; above the last stop the last stop's colour continues. A ramp
whose last stop is at `pos` 176 paints 176..255 in that colour — cutting the
brightest pixels to black is said explicitly, with a black last stop. This is
a property of `outpipe::fill_palette_lut`, so the device output palette, a
pattern's `setOutputPalette` and this per-layer ramp cannot disagree; it is
*not* `paint()`'s edge rule, whose black above the last stop is Pixel Blaze's
own and stays (Gitea #787).

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

A scene with a design size carries `"space":[32,32]` right after `"name"`
(absent without a `U` line). A clip layer carries `"clip":true` right after
`rot180`, and an `X abs` layer `"abs":true` right after that (each absent
otherwise),
and a layer with a track carries `"keys":[{"ms":0,"x":4,"y":40,"opacity":0,"ease":"linear","frame":2},…]`
right after `rot180`: `x` / `y` / `opacity` are absent for a `=`, `frame`
absent when the key holds none, and `keys` itself absent when the layer has
no `M` lines.

The last member is one of:

* `"pat": {...}` as above — `proj` and `ramp` absent when unset; control
  values decimal, like the playlist's GET.
* `"text": {"source":"lit","text":"HI" | "source":"clock","fmt":"HH:MM" | "source":"slot","slot":0,
  "font":"regular","color":"ffffff","align":"l","scroll":"none","speed":0}`
* `"sprite": {"id":"…","fps":12}` — `fps` present ONLY when the layer
  overrides the record's rate (the `A` line); absent means "play the
  record's own"
* `"color": "ff8800"`

`GET /api/scenes` wraps them:
`{"active":"<id>"|null,"layers_max":N,"used":<blob bytes>,"max":3840,"scenes":[…]}`.
Routes, storage (`SCENES_KEY`, `BLOB_MAX` 3840 B) and the playlist `I S<id>`
item are in `docs/api.md`.

## 4. Sprites — the `LXSP` record (Gitea #740)

A sprite is a FIRST-CLASS record — its own store, its own ids, its own
`/api/sprites` routes (docs/api.md "Sprites") — and not a pattern. Jeremy
reversed the sprite-tagged-pattern design of #481 on 2026-09-24 ("confusing
and may allow cheating and running more patterns than are allowed"); the
tag is documented at the end of this section only so the console's one-
release migration reader can be understood.

ONE byte layout is the flash bytes, the wire bytes, the playground's stored
bytes and the wasm compositor's input. Little-endian, byte-addressable:

```text
off  size        field
0    4           magic  "LXSP"
4    1           version = 1
5    1           w        1..=64
6    1           h        1..=64
7    1           frames   1..=255
8    1           fps      0..=30   (0 = static)
9    1           colors   0..=255  (palette entries)
10   1           name_len 1..=64   (UTF-8 bytes)
11   1           flags    0        (reserved; readers ignore, writers write 0)
12   name_len    name
+    3*colors    palette  [r,g,b] × colors, RGB888
+    w*h*frames  index    one byte per texel, frame-major then row-major
                          (i = frame*w*h + row*w + col); 0 = TRANSPARENT,
                          k in 1..=colors = palette[k-1]
```

Total length = `12 + name_len + 3·colors + w·h·frames`, and a record whose
length is anything else is invalid, as is an index byte above `colors`. The
cap is `SPRITE_MAX_BYTES` = 16 KiB (the device's request buffer): a 64×64
sprite holds 3 frames, 32×32 holds 15, 16×16 holds 63.

**Transparency is index 0, not black.** An opaque black texel is a palette
colour like any other. (The tagged-pattern format keyed on `v == 0`; the
console's migration maps that to index 0.) The layer's `key` word is still
ignored for a sprite layer.

**The palette cap is the format's**: up to 255 colours per sprite, enforced
in exactly one place — `sprite::check` on the device and `colorIndex` in the
console's codec, which refuses the 256th with `sprite: 255 colours max`. The
16-cell editor rule of #481 is gone (Gitea #741, "why are sprites limited to
16 colors?").

```rust
pub struct SpriteView<'a> { pub w: u8, pub h: u8, pub frames: u8, pub fps: u8,
                            pub name: &'a str, /* palette, index */ }
impl SpriteView<'_> {
    pub fn parse(bytes: &[u8]) -> Option<SpriteView<'_>>;   // validated view, in place
    pub fn texel(&self, i: usize) -> Option<[u8; 3]>;         // None = transparent
    pub fn frame_at(&self, elapsed_ms: u32) -> u8;            // (elapsed·fps/1000) mod frames
    pub fn frame_at_fps(&self, elapsed_ms: u32, fps: u8) -> u8; // …at a layer's `A` rate
}
pub fn check(bytes: &[u8]) -> Result<(), &'static str>;      // the `sprite: …` reasons
pub const fn record_len(name_len, colors, w, h, frames) -> usize;
```

`luxel_core::sprite` is `no_std` and allocation-free: on the device the view
borrows the record where it lies in the memory-mapped store, so a sprite
layer costs no RAM and holds **no engine** — `SceneHost::sprite` hands the
driver a `SpriteView` over the store's bytes (firmware), over a per-layer
copy (`lx_comp_sprite`, wasm) or over the mirror's in-memory record. A
resident scene PINS the sprite records it draws exactly as it pins its
pattern layers, so a compaction never moves them out from under a frame.

**Placement (`fit`, decided in #741; zero boxes #985).** With a box size set,
`fill` stretches the frame to the box (nearest neighbour), `contain` scales
it uniformly to fit inside the box and centres it, `tile` repeats it 1:1
across the box. A ZERO `w` or `h`:

* on a `fill` / `contain` layer draws the sprite at its NATURAL size at
  `(x, y)`, 1:1 — except that on a one-row canvas (a strip, §2) `h` 0 is the
  row, so only `w` 0 means natural size there and `x 0 w 0` is a segment;
* on a `tile` layer is **the whole layout on that axis** (the rest of it
  from an `x`/`y` > 0, §2 "Geometry"), as for a pattern
  layer: `L sprite 0 0 0 0 … tile` wallpapers any panel and covers any strip
  without its size typed in, and `0 40 0 24` is a full-width band. Before
  #985 a zero box fell back to the natural size under `tile` too (one copy
  of the tile); every shipped scene used an explicit box, so none changed
  (`crates/luxel-core/tests/library_scenes.rs` digests them).

In a scene with a design size (§2 "Design size") the natural size and the
tile cell are magnified by the whole-number zoom `z` — every texel a `z`×`z`
block, so pixel art stays crisp — and `fill`/`contain` scale to the mapped
box as before. An `X abs` sprite layer stays 1:1.

`fit` still means nothing on any other layer kind. The frame
shown is `frame_at_fps(ms since the scene was set, rate)`, per layer, where
`rate` is the LAYER's `A <fps>` override when it has one and the record's own
`fps` otherwise (Jeremy, 2026-09-26). `A 0` holds frame 0 however many frames
the record carries, and an `A` above the record's rate speeds it up; both go
through the record's one piece of arithmetic so the device, the mirror and the
playground cannot round differently. Changing the override does NOT restart
the layer's clock — it carries over a `set_scene` like the scroll phase.

**The tag this replaced (readable by the console for ONE release).** Before
#740 a sprite was a pattern whose first line was
`// @sprite w=<w> h=<h> frames=<n> fps=<f>` followed by three top-level
literals `var sprH = […]`, `var sprS = […]`, `var sprV = […]` (HSV 0..1,
`w·h·frames` each, `v = 0` transparent), which the compositor read out of
the compiled program's const pool. The device no longer parses it. The
console converts such a pattern the first time it sees one — decode, `POST
/api/sprites`, re-point every scene layer that named the pattern id at the
new sprite id, delete the pattern — via `spriteFromTaggedPattern` in
`web/src/lib/sprite.ts`, which is the only remaining reader.

## 5. wasm (`crates/luxel-wasm`, C ABI)

```
lx_comp_new(w: u32, h: u32) -> i32      // handle; row-major grid
lx_comp_free(ch: i32)
lx_comp_set(ch, ptr, len) -> i32        // 0 ok; -1 + the parse error in the response buffer
lx_comp_bind(ch, layer: u32, engine_handle: i32)   // pat layers; -1 unbinds
lx_comp_sprite(ch, layer: u32, ptr, len) -> i32   // sprite layers: the LXSP record; len 0 clears;
                                                  // -1 + `sprite: …` in the response buffer
lx_comp_text(ch, layer: u32, ptr, len)
lx_comp_frame(ch, delta_raw: i32) -> *const u8     // w·h·3 RGB, valid until the next call
lx_comp_layer_count(ch) -> u32
lx_comp_layer_rect(ch, layer: u32) -> *const i32   // [x, y, w, h, zoom] — Compositor::layer_rect
                                                  // + layer_zoom, after `U` and the keyframe
                                                  // track; natural sprites sized from the
                                                  // lx_comp_sprite record; valid until the next
                                                  // call; null for a bad handle or layer

lx_palette_lut(ptr, n_stops: u32, amount_pct: u32) -> *const u8
                                        // 768 B: the cooked RAMP table, r,g,b per
                                        // brightness 0..255, valid until the next call.
                                        // `ptr` is n_stops x (pos, r, g, b) bytes — the
                                        // same four `POST /api/output/palette` and an
                                        // `R` record take. Cooked by
                                        // `outpipe::fill_palette_lut` and blended by
                                        // `outpipe::palette_remap_frame` themselves,
                                        // over a greyscale frame (luma([i,i,i]) == i),
                                        // so entry i is what the device turns a pixel
                                        // of brightness i into at this amount.
```

`lx_comp_frame` steps every bound **pattern** engine through `Engine::frame`,
exactly as `lx_frame` does (frame-rate cap and time scaling included), then
composites the stack. A **sprite** layer holds no engine: `lx_comp_sprite`
copies its record into the compositor slot and the driver reads texels from
that copy (§4). Feed the composite
through `lx_outpipe` the way a pattern frame is fed, to see what the wire
would carry.

`lx_palette_lut` exists so the browser never re-derives that table (Gitea #748):
the ramp editor's bar and both of its preview cells are the engine's own
arithmetic. `Luxel.paletteLut(stopBytes, amountPct)` wraps it and returns a
COPY, because the buffer is the module's; `lib/gradient.ts`'s `rampLut` is the
synchronous fallback for the first paint before the module is up, and
`web/tests/paletteLut.test.mjs` pins the two against each other.

TypeScript wrapper: `Luxel.compositor(w, h)` → `Compositor` with
`setScene(wire) → string | null`, `bind(layer, engine | null)`,
`setSprite(layer, bytes | null) → string | null`, `setText(layer, s)`,
`layerCount()`, `layerRect(layer) → {x, y, w, h, zoom} | null`,
`frame(dtMs) → Uint8Array`, `free()`
(`web/src/lib/luxel.ts`). `Engine.handle` exposes the wasm handle `bind`
takes.
