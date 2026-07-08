# Stacker
kind: 1D
sensors: no

## What it looks like
A one-dimensional "Tetris-ish" filling effect. The strip is split into a small
number of equal segments (one to six). In every segment simultaneously, a short
bright block travels inward from both ends of the segment toward its center
(the motion is mirrored, so it looks like two blocks converging). When the
traveling block reaches the pile that has accumulated at the center, it locks
on and the pile grows outward by one block width; a new traveling block then
starts again from the segment edges. When the pile finally fills the whole
segment (reaches the edges), everything clears and the stacking starts over.
All segments show identical, synchronized motion. Pixels that are neither pile
nor traveling block are dark.

## Algorithm
State kept between frames (simulated once, for a single half-segment, and
reused for every segment via symmetry):

- segment count, segment length (total pixels divided by segment count,
  rounded down), and the index of the segment midpoint;
- the traveling block's current position within the half-segment;
- how many pixels are currently "stacked" at the center;
- an accumulator of elapsed milliseconds since the block last moved;
- a slowly advancing cyclic phase used only for the rainbow color mode.

Per frame: add the frame's elapsed time to the accumulator. Whenever the
accumulator passes the user-set step interval, advance the block one pixel and
zero the accumulator. If the block's position has passed the inner boundary of
the pile, grow the pile by one block width and reset the block to the segment
edge; if the pile has reached (or passed) the midpoint, reset the pile to
empty. Also advance the rainbow phase from a wall-clock cyclic timer.

Per pixel: reduce the pixel index modulo the segment length, then fold it
around the segment midpoint (distance-from-edge folding) so both halves of a
segment mirror each other. Then classify:

1. If the folded position is inside the piled region (beyond the pile
   boundary), color it with the current color mode (see Colors).
2. Else if it is within half a block width of the traveling block's position,
   color it with the user's second ("traveling") color.
3. Otherwise, black.

Randomness: none — fully deterministic.

Layout assumptions: pure 1D strip; works for any length. Minor quirk: because
the segment length is the floor of pixels/segments, any leftover pixels at the
far end of the strip render as a partial extra copy of a segment's beginning.
The obvious fix is to distribute the remainder or blank the leftover pixels.

## Colors
Two user-picked colors: one for the stacked pile, one for the traveling block.
The pile color additionally depends on a selectable color mode (three modes):

1. Solid: the pile is exactly the first picked color.
2. Animated rainbow: hue varies along the half-segment (position divided by
   the half-segment length) plus a continuously drifting time phase, giving a
   scrolling rainbow across the pile; a full hue drift cycle takes roughly a
   couple of seconds. Saturation and brightness still come from the first
   picker.
3. Color bands: the hue is quantized per block — the pixel's position divided
   by the block size, rounded, then multiplied by a golden-ratio-like constant
   (about six tenths) so consecutive blocks land on well-separated hues.
   Again saturation/brightness come from the first picker.

The traveling block always uses the second picked color verbatim. Background
is off/black.

## Controls
- Color picker "Color 1": hue/saturation/brightness of the stacked pile.
- Color picker "Color 2": color of the traveling block.
- Slider "Speed": maps to the delay between single-pixel block movements,
  from roughly a quarter second per step at the slow end down to essentially
  every frame at the fast end (inverted: right = faster).
- Slider "Size": traveling-block width, from one pixel up to around ten.
- Slider "Segments": number of equal segments, one through six; changing it
  reinitializes the animation.
- Slider "Color mode": selects among the three pile color modes (a slider
  acting as a 3-way selector).

## Timing
Default block step rate is quick — on the order of a hundred steps per second
— so a segment fills over a few seconds. The rainbow drift completes a cycle
in about two seconds.

## Non-obvious points
- Only one half-segment is ever simulated; modulo plus fold-around-midpoint
  in the per-pixel path replicate it to every segment and both halves, which
  is what makes the converging/mirrored look essentially free.
- The color modes are best implemented as a small table of hue-functions of
  the folded pixel position, indexed by the mode selector.
