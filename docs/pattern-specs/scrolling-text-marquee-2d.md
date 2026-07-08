# scrolling text marquee 2D
kind: 1D+2D
sensors: no

(Note to integrator: this pattern was labeled sound/sensor-reactive upstream, but the source uses no sensor inputs whatsoever — no spectrum, sound energy, or accelerometer. It is a pure text scroller.)

## What it looks like
On a small LED matrix (nominally 8×8): a short text message scrolls smoothly right-to-left, marquee style, in a single warm-white color on black. The default message is a friendly greeting followed by punctuation (something like "  Hello?!?" with a couple of leading spaces so it doesn't start mid-glyph). The message loops forever. At the default speed, a few characters pass by per second — comfortably readable.

On a bare 1D strip it becomes a persistence-of-vision / light-painting tool: the strip continuously displays what would be the leftmost column of the marquee, so waving the strip (or a long-exposure photo) reveals the text. In this mode the text repeats vertically down the strip with blank gap rows between repetitions (about half a character-height of line spacing), and the color cycles through the rainbow over time with an additional hue ramp along the strip, so streaks come out multicolored.

## Algorithm
### Font
A fixed bitmap font: each glyph is an 8×8 grid of on/off bits, covering the printable ASCII range (space through tilde). The original embeds a well-known public-domain 8×8 font (any equivalent 8×8 ASCII bitmap font is acceptable; do not attempt to match glyph-for-glyph). Codepoints below space are treated as user-definable custom glyph slots; the original stores one custom glyph there (a retro-styled question mark borrowed from an old home-computer font) plus a programmatically tweaked copy of it in the adjacent slot, purely as a demo of building sprites at runtime. The default message uses those custom slots for some of its punctuation.

Storage detail: because the original platform limits array count/size and its numbers are 16.16 fixed-point, the font is bit-packed four glyph-row bytes per array element, with helper routines to pack/unpack a byte at a given slot, plus routines to store a glyph from eight row-bytes, fetch a glyph into a scratch 8-row buffer, and store back a (possibly modified) scratch glyph. This packing is a memory workaround, not part of the visual behavior — on a platform without those limits, store the font any convenient way, but do keep the three glyph operations (store by rows, fetch to scratch, store from scratch) since custom-glyph definition is part of the pattern's surface.

### Message
A fixed-length list (a dozen or so entries) of character codes, exported so it can be rewritten remotely (e.g. over a websocket API) without editing the pattern. Message length in columns = message length × glyph width.

### Scrolling state (kept between frames)
- A circular column buffer the size of the display: rows × columns of on/off cells (one bit per cell; the original notes you *could* pack color per cell but keeps it monochrome).
- A "head" pointer: which buffer column currently corresponds to the display's leftmost physical column.
- A pointer into the message's overall column stream (wraps at the end of the message, giving the endless loop).
- A millisecond accumulator.

### Per frame
Accumulate elapsed time. Whenever it crosses the per-column shift period (subtract, don't reset, so cadence stays accurate), advance the scroll by one column:
1. Work out which message character and which column-within-glyph the stream pointer refers to.
2. Fetch that glyph and extract that single column as 8 bits (one per row).
3. Write those bits into the buffer column at the head pointer (overwriting the column that just scrolled off the left edge).
4. Advance both the head pointer (mod display width) and the message column pointer (mod total message columns).

The per-column period is derived from a scroll-speed constant expressed in characters per second: period = one second ÷ speed ÷ glyph width. Default speed is a few characters per second (good for a matrix); the comments suggest roughly ten times faster for POV strip use.

### 2D render (per pixel)
Map normalized y (top-left origin, y increasing downward) to a buffer row and normalized x to a physical column; the buffer column is the physical column offset by the head pointer, modulo display width. Emit the text color at full value if the cell bit is set, black otherwise. Hue and saturation are globals, exported for remote control, defaulting to a warm white (slightly orange hue, mostly saturated — but never state it numerically; "warm incandescent white" is the target).

### 1D render (per pixel)
- Flip the index so pixel zero (usually the power end) is the bottom of the text.
- Row = index modulo (glyph height × one-and-a-half), i.e. glyphs repeat down the strip with half-glyph blank line spacing; indices falling in the gap render black.
- Column: always the buffer column at the head pointer (the marquee's leftmost column).
- Hue: a slow triangle-ish oscillation over time (full cycle on the order of a minute or two) minus the pixel's position fraction, giving a rainbow gradient along the strip that drifts over time. Full saturation from the same global; value = the cell bit.
- A commented-out alternate mode stretches one full glyph across the whole strip instead of repeating.

## Layout assumptions & fixes
Display size (8 rows × 8 columns) and glyph size (8×8) are hardcoded and coupled. The comments say to change the row/column constants to match your matrix (fewer rows scales the text to fill). Obvious improvement: derive display rows/columns from the pixel map's actual dimensions instead of constants, and decouple "glyph rows" from "display rows" so text can render on taller matrices with margin. The glyph-width coupling (8-bit rows) is inherent to the font format and fine to keep.

## Controls
No sliders/toggles/triggers. Remote-settable exported variables instead: the message (array of character codes), and the text hue and saturation. Suggested improvement for our platform: expose speed and the message as proper controls (the message ideally as a text field, speed as a slider).

## Timing
A few characters per second scroll by default; each one-column step is a small fraction of a second. The 1D rainbow drift is much slower — a cycle takes on the order of a minute.

## Clever bits
- The circular column buffer means only one glyph column is decoded per scroll step — per-pixel work is just an array lookup, so rendering stays cheap regardless of message length.
- The same scroll state drives both a readable 2D marquee and a 1D POV/light-painting column, making the pattern useful even without a pixel map.
- Custom glyph slots below the printable range let users define their own symbols, including runtime-generated variants (fetch, mutate rows, store to a new slot) for glyph animation.
