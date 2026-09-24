# Text — strings, fonts, the draw builtins and the text slots

Status: **implemented** (`crates/luxel-core/src/text.rs`, `vm.rs` ids
188..=192, `parse.rs`/`compile.rs` for the literals). Gitea #483 (C1), #484
(C2), #485 (C3). One implementation in `luxel-core`, so the device, the
mirror (`luxel serve`) and the playground draw the same pixels.

## 1. Strings without a string type

The pattern language has no string value and gains none. A quoted literal is
legal in exactly one place — the argument list of `drawText`, `textWidth` or
`font`, called by name (`parse::TEXT_BUILTINS`). Anywhere else the parser
still rejects it:

```
the pattern language has no string values — a quoted string is only allowed
as `assert(cond, "message")` or as an argument of drawText(), textWidth() or
font()
```

A literal is interned into the program's **message table** (`intern_msg`, the
same pool `assert()` uses: ≤255 B, truncated on a char boundary with a
trailing `...`, deduplicated, kept by a lean decode) and compiled to
`Const(Num)` holding its index. See `bytecode.md` §msgs — this is why text
needed **no LXBC format bump**.

### Text handles

A *text handle* is a plain number, so `builtin_sig` needs no new return shape
and the JIT sees an ordinary numeric argument:

| handle | means |
|---|---|
| `k ≥ 0` | message-table entry `k` — what a string literal compiles to |
| `k < 0` | text slot `−(k + 1)` — what `textSlot(n)` returns |

Out of range in either direction reads as the empty string. A pattern may
pass any number; nothing errors.

## 2. Builtins

Ids 188..=192, appended to `BUILTINS` (append-only) and dispatched from
`builtin_cold`. All five return a number and write nothing, so
`vm::builtin_sig`'s default arm covers them and `jit::BUILTIN_ENTRIES` needs
only its length bumped.

| id | signature | returns |
|---|---|---|
| 188 | `drawText(handle, x, y[, align])` | advance width in px (0 without a grid) |
| 189 | `textWidth(handle)` | advance width in px |
| 190 | `drawNumber(v, x, y, digits, decimals)` | advance width in px |
| 191 | `font(handle)` | the active face's index (0 tiny, 1 regular, 2 large) |
| 192 | `textSlot(n)` | the handle of slot `n` |

Common contract, which is `blit`'s:

- **Grid space, top-left origin, y growing down.** `x`/`y` are grid cells,
  floored.
- **The brush colour** — whatever the last `hsv()`/`rgb()`/`paint()` set,
  quantized exactly as every other bulk op quantizes it. The brush starts
  each frame black.
- **Silent no-op without a regular grid.** No grid (or a grid smaller than
  the frame) → nothing drawn, `0` returned, no error. `textWidth` is pure
  arithmetic and answers anyway, so a pattern can measure before it learns
  it cannot draw.
- **Clipped at all four edges**; cells past the end of the frame (the tail of
  an over-provisioned last row) are skipped.
- Outside `renderFrame` the frame buffer is empty, so every draw is a no-op.

`align`: `0` left (default), `1` centre, `2` right — `x` is the anchor.
Anything else reads as left, matching the "missing args read as 0"
convention.

`drawNumber`: `digits` is the **minimum** integer digits (zero-padded, max
10), `decimals` the fraction digits (max 4), rounded half-up and carrying
into the integer part; a negative value gets a leading `-` unless it rounds
to zero. Fixed-point aware — the fraction comes from the 16.16 word, so
`drawNumber(0.5, x, y, 1, 2)` draws `0.50`.

`font(handle)` is **modal and persistent across frames** (like
`setFrameRate` and the palette, unlike the brush). An unrecognised name
changes nothing, so `font("")` reads the active face.

```js
export function renderFrame() {
  if (gridWidth() == 0) return
  clear()
  rgb(1, .6, 0)
  font("tiny")
  var w = textWidth(textSlot(0))
  drawText(textSlot(0), gridWidth() / 2, 1, 1)   // centred
  drawNumber(time(.1) * 100, 1, 8, 3, 1)         // 000.0 … 100.0
}
```

## 3. Fonts

Three PSF2 blobs, `include_bytes!`d — identical on every host, no upload
path, no attribution plumbing. Provenance, licence text and the exact
conversion command are in `crates/luxel-core/fonts/README.md`; the converter
is `tools/fonts/bdf2psf2.py`.

| name | source | cell | advance | bytes | licence |
|---|---|---|---|---|---|
| `tiny` | Tom Thumb | 3×6 | 4 | 602 | BSD-3-Clause |
| `regular` | X11 misc-fixed 5×7 | 5×7 | 6 | 697 | public domain |
| `large` | Spleen 5×8 | 5×8 | 6 | 792 | BSD-2-Clause |

2,091 B total. Each blob is a 32-byte PSF2 header then `95 × charsize` bytes
of MSB-first row-packed glyphs for code points `0x20..=0x7E` **in order**, no
Unicode table — glyph index is `codepoint − 0x20` and lookup is one
multiply. Any other code point draws `?`.

Advance = **cell width + 1**, uniformly: the inter-glyph gap is not baked
into the cell, which is what puts Tom Thumb on its designed 4 px pitch.
`textWidth` counts the trailing gap too (`chars × advance`), so two runs laid
side by side is one add.

## 4. Text slots

Eight device-level strings, ≤ 64 B UTF-8 each, truncated on a char boundary.
They are how content reaches a panel without any pattern code: `POST
/api/text`, MQTT, a Home Assistant text entity, the playground's
`Luxel.setTextSlot()` — and a scene's `T slot <n>` text layer.

```rust
pub const SLOTS: usize = 8;
pub const SLOT_MAX: usize = 64;
pub fn set_slot(n: u8, s: &str);
pub fn with_slot<R>(n: u8, f: impl FnOnce(&str) -> R) -> R;
pub fn clear_slots();
```

Not persisted across reboot (v1). `n ≥ 8` is ignored on write and reads
empty. The wasm export is `lx_text_slot_set(n, ptr, len)`.

The table is **allocated on the first non-empty write**, never before: 8 ×
64 B of `.bss` is 544 B of internal SRAM on every board whether or not
anything ever draws a word, and `tools/stack-check.sh` measures exactly that
trade (statics come out of the main task's stack headroom — it caught this
one). A device that never receives a `POST /api/text` pays one pointer. The
allocation is on the control path, so it happens well after
`wait_config_up()`; if it fails, the write is dropped and rendering
continues.

**Single-writer rule.** `luxel-core` is `no_std` and does not depend on
`critical-section`, so the table is a `static` behind an `UnsafeCell` with a
documented contract, the same idiom `arena.rs`'s hook uses: `set_slot` is a
control-path call and must never run concurrently with a render (i.e. with
`with_slot`) or with another `set_slot`, and `with_slot`'s closure must not
call `set_slot`. On the device both the API task and the frame loop live on
one executor and a frame never yields mid-render, so it holds by
construction; the mirror and the playground are single-threaded.

## 5. Clock and number formatting

`text::format_clock(fmt, h, m, s, y, mo, d) -> String` renders the
layouts a scene's `T clock <fmt>` line names. Out-of-range fields clamp
rather than erroring — a device with no time sync draws `00:00`, not
nothing.

| `ClockFmt` | wire name | example |
|---|---|---|
| `Hm24` | `HH:MM` | `09:05` |
| `Hms24` | `HH:MM:SS` | `09:05:03` |
| `Hm12` | `hh:MM` | `9:05` |
| `Hms12` | `hh:MM:SS` | `9:05:03` |
| `MonthDay` | `MM-DD` | `09-24` |
| `Date` | `YYYY-MM-DD` | `2026-09-24` |

The 12-hour forms are 1..12 and **not** zero-padded, which is the LED-clock
convention.

`text::format_number(v, digits, decimals, &mut TextString)` is `drawNumber`'s
formatter, available to hosts directly. `TextString` is `TextBuf<24>` — a
fixed-capacity `no_std` string that derefs to `str`.

## 6. Rust API

```rust
pub enum Font { Tiny, Regular, Large }      // from_wire/as_str, cell_width, height, advance
pub enum Align { Left, Center, Right }      // from_code (0/1/2), from_letter/letter ("l"/"c"/"r")
pub enum ClockFmt { Hm24, Hms24, Hm12, Hms12, MonthDay, Date }  // from_wire/as_str
pub const FONTS: [Font; 3];
pub const CLOCK_FMTS: [ClockFmt; 6];

pub fn draw(dst: &mut [[u8;3]], grid: &GridMap, x: i32, y: i32,
            text: &str, font: Font, rgb: [u8;3]) -> i32;
pub fn draw_aligned(dst: &mut [[u8;3]], grid: &GridMap, x: i32, y: i32,
                    text: &str, font: Font, rgb: [u8;3], align: Align) -> i32;
pub fn width(text: &str, font: Font) -> u32;
pub fn format_number(v: Fx, digits: u8, decimals: u8, out: &mut TextString);
pub fn format_clock(fmt: ClockFmt, h: u8, m: u8, s: u8, y: u16, mo: u8, d: u8) -> String;
pub fn set_slot(n: u8, s: &str);
pub fn with_slot<R>(n: u8, f: impl FnOnce(&str) -> R) -> R;
pub fn clear_slots();
```

`draw`/`draw_aligned` composite into a row-major frame through
`GridMap::index`, so serpentine wiring is handled for the caller exactly as
it is for `bulk::paste`.
