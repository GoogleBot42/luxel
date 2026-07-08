# DAFTPUNK

kind: 2D
sensors: no

## What it looks like

Bold red pixel-font text scrolling smoothly right-to-left across an LED matrix on a black background, marquee style. The default message is "DAFTPUNK" rendered in chunky custom letterforms (bolder than the stock font also embedded in the pattern), looping forever. Scroll rate is a couple-few characters per second — each one-column step happens every few tens of milliseconds, so motion reads as smooth. Designed for an 8-pixel-tall matrix (the author targeted a small commercial 8x8 panel and demoed it on video).

This is really a general-purpose **text scroller framework** with an embedded 8x8 bitmap font covering the printable ASCII range; the "DAFTPUNK" message is just the demo payload.

## Algorithm

### Font storage
An 8-row-tall, 8-bit-wide public-domain bitmap font (the author credits a specific open font repo) covering printable ASCII. Storage is the clever part: the engine's numbers are 16.16 fixed point and its bitwise operators only act on the upper 16 bits, so the font packs **four glyphs' worth of one row into each array element** (four 8-bit fields per 32-bit word — two above the binary point, two below). There is one array per glyph row (8 arrays), each long enough for the full character count divided by four. Pack/unpack helpers select the right element and 8-bit "bank" within it, disassembling the word into four bytes and reassembling it, with shifts in both directions to reach the fields below the binary point. Glyph slots corresponding to the ASCII control range are left free for user-defined custom glyphs; the demo stores eight custom extra-bold letters (D, A, F, T, P, U, N, K) there and the message is a list of those slot indices.

A helper loads any glyph slot into a global 8-row scratch array of plain bytes; another stores from that scratch array back into a slot (the author notes this enables programmatic sprite animation — copy, mutate, re-store).

### Scrolling
State between frames:
- A **render buffer**: 8 rows by a few dozen columns (wider than the physical display) of on/off cells, used as a **circular buffer over columns**.
- A **write pointer**: which buffer column is currently the leftmost displayed column.
- A **message column pointer**: which column of the overall message (message length times per-character column width) to decode next.
- A **millisecond accumulator**.

Per frame: add the frame delta to the accumulator. Whenever it exceeds the per-column shift period (derived as: one second, divided by desired characters-per-second, divided by columns-per-character), subtract the period and advance one column: decode the next column of the current message character from the font (test each row's bit at the current column offset within the glyph), write that column of bits into the buffer at the write pointer, then advance both pointers with wraparound (message pointer wraps to loop the text). Characters are spaced by giving each character one more column than the glyph is wide — the extra column reads past the 8-bit data and yields a blank separator column.

Note there is **no shifting of buffer contents** — scrolling is achieved purely by advancing the pointer that says where "column zero" is, and overwriting the column just behind it. Cost per step is one column, not the whole buffer.

### Rendering
Per pixel, the 2D renderer maps the normalized vertical coordinate to a buffer row (times the row count, floored) and the normalized horizontal coordinate to a buffer column (times the buffer column count, floored, plus the write pointer, wrapped). If the cell is set, emit fully saturated red at full brightness; else black. (The source passes wildly out-of-range saturation/value numbers which the HSV call clamps to full — effectively "pure red".)

### Quirks / portability notes
- The parameter order in the 2D render entry point treats the **first** world coordinate as vertical and the second as horizontal — i.e. swapped relative to the conventional (x, y) order. On the author's mapping this comes out correct; a reimplementer should just ensure the vertical axis picks the row (top = row zero) and horizontal picks the column, and verify orientation against a real mapping.
- Row count (8) and buffer column count (a few dozen) are hardcoded, as are glyph dimensions and message length. Obvious fix: derive display rows/columns from the pixel map dimensions and size the buffer from the display width plus one character of headroom; make the message a resizable array.
- The horizontal mapping multiplies by the **buffer** width, not the display width — on a display narrower than the buffer, adjacent physical pixels sample the buffer several columns apart unless the mapping compensates. A clean reimplementation should map physical columns one-to-one to consecutive buffer columns.
- The message array is exported so it can be rewritten live over the network protocol (change the text without editing code).

## Colors

Pure vivid red text on black. Single color only; no gradients.

## Controls

No UI sliders/toggles. The message content is externally settable via the exported array (network variable access), and scroll speed is a named constant intended to be edited.

## Timing

A couple-few characters per second scroll rate; one column step every few tens of milliseconds; the 8-character demo message loops roughly every few seconds.
