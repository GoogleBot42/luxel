# Mapping Helper Single and 10x
kind: 1D (utility; intended to aid 2D/3D map building)
sensors: no

Note: despite external tagging, this pattern reads no sound or sensor input whatsoever. It is a pure timer-driven mapping/identification aid.

## What it looks like
A single bright white cursor pixel steps along the strip a few times per second. As it advances, the strip fills in behind it in blocks of ten pixels, each block dimly lit in one color from a five-color cycle: red, green, blue, yellow-amber, violet. The result is a growing ruler: you can read off tens by block color and the exact current pixel by the white dot. When the cursor wraps back to the start, the entire strip flashes a dim purple for one step, wiping the ruler, and the fill begins again.

## Algorithm
State between frames:
- Cursor index (which pixel is currently white).
- Start index of the block of ten containing the cursor.
- A color counter cycling one through five.
- A millisecond accumulator.

Per frame: if the cursor has run past the end, reset it to zero. Recompute the block start (cursor rounded down to a multiple of ten). If the block start changed since last frame, advance the color counter (wrapping five back to one). Accumulate delta; when roughly a third of a second has elapsed, advance the cursor one pixel and reset the accumulator.

Per pixel, in order of application (later rules overwrite earlier ones):
1. If the pixel lies inside the current block of ten, paint it the current cycle color at low brightness (about one fifth of full; the blue stop is boosted a bit above the others to read comparably).
2. If the pixel is the cursor, paint it full white (overrides the block color).
3. If the cursor is at position zero (the wrap step), paint every pixel a dim purple — this is the full-strip wipe frame.

## Critical non-obvious mechanic: framebuffer persistence
Each frame only the *current* block of ten and the cursor are actively painted. Previously completed blocks remain visible only because the target platform does not clear the pixel buffer between frames — unwritten pixels keep their last color. That is exactly what produces the accumulating multi-colored ruler, and the dim-purple full-strip write on wrap is what erases it. A reimplementation must either (a) reproduce persistent-framebuffer semantics, or (b) explicitly repaint all already-passed blocks each frame from the deterministic block-index-to-color mapping (block number modulo five selects the color); option (b) is cleaner and gives identical visuals.

## Startup quirk
The color counter starts unset (zero), so the very first block after power-on may render uncolored until the counter first advances; after the first wrap everything is regular. Initializing the counter to the first color fixes this.

## Layout assumptions
1D by raw index; block size of ten is hardcoded (that is the point of the tool — humans count in tens), and the step interval (roughly a third of a second) is hardcoded. Both could be constants at the top or sliders. Scales to any pixel count.

## Colors
Cursor: full-brightness white. Block cycle, all dim: red, green, blue, warm yellow (red plus most of green), violet (red plus blue). Wipe frame: dim purple, distinct from the violet block color mainly by covering the whole strip at once.

## UI controls
None.

## Timing feel
Cursor advances about three pixels per second; a block of ten takes about three seconds; a full pass over a few hundred pixels takes a couple of minutes, then wipes and repeats.
