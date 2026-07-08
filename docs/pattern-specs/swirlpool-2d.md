# Swirlpool 2D
kind: 2D
sensors: no

## What it looks like
On a matrix, a ring of small bright dots orbits the center, each dot leaving a
comet-like fading trail, so the whole thing reads as a set of interlocking
spiral arms — a swirling whirlpool. Each orbit takes a bit over a second. On a
much slower timescale (tens of seconds to minutes, user-adjustable) the arms'
individual orbit centers glide between two arrangements: spread apart around
the display versus collapsed toward/through the middle, so the figure
continuously morphs between a wide multi-lobed swirl and a tight braided knot.
The arms are rainbow-tinted, each arm a different hue, and the whole hue
assignment drifts slowly over time. Trails persist for very roughly half a
second before fading to black.

## Algorithm
State kept between frames: two persistent off-screen buffers ("canvas"), one
holding a brightness value per pixel and one holding a hue per pixel. The
canvas is sized to the pixel count, treated as a square grid: its width is
taken as the square root of the pixel count. **This assumes a square matrix**;
the obvious fix is to derive width/height from the actual mapped matrix
dimensions (or expose them) instead of assuming a perfect square.

Per frame (before rendering):
1. A "swirl phase" is computed as a slow triangle wave (period set by the swirl
   speed control); if the swirl-animation control is off, this phase is forced
   to zero.
2. Every canvas brightness value is multiplied by a decay factor just under
   one, so old pixels fade out exponentially.
3. For each of N arms (N set by a control): the arm gets a fixed angular offset
   evenly spaced around a full circle (arm number over arm count, times a full
   turn). A point is computed at:
   - a shared fast rotation angle (full revolution in roughly a second and a
     bit) plus the arm's angular offset,
   - positioned at the display center plus a radius of about a quarter of the
     display, along the sine/cosine of that angle,
   - **plus** the sine/cosine of the arm's own fixed offset, scaled by a factor
     that runs from full (+1) down through zero to a negative value (about
     minus 0.4, i.e. one minus root-two) as the slow swirl phase rises. That
     term is what moves each arm's orbit center outward/inward and slightly
     past center, producing the morph between spread and collapsed
     arrangements. (The sines/cosines are clamped to the unit range, which is
     a no-op — harmless redundancy in the original.)
4. The point's fractional x,y is converted to a canvas cell index (floor of
   x times width, plus floor of y times height, times width). If the index is
   in range, that cell's brightness is set to full and its hue is set to the
   arm's fraction of the color wheel multiplied by a slowly advancing phase.
   That phase advances with a period set by the color-speed control, and the
   period itself is wobbled by another slow triangle wave (on the order of a
   couple of minutes), so the coloring never settles into a fixed loop.

Per pixel (render): the incoming normalized x,y is converted back to the same
canvas cell index (the supplied pixel index is ignored), and the cell's stored
hue is shown at full saturation with brightness equal to the stored value
squared (squaring steepens the trail falloff so trails look punchier).

No randomness is used; everything is deterministic wave math.

## Colors
Full-saturation rainbow. Each arm gets its own hue, arms spread across the
color wheel proportionally to their index, and the entire assignment drifts
around the wheel over tens of seconds. Background fades to black.

## Controls (all sliders)
- **Number of arms**: from two up to roughly the square root of the pixel
  count (i.e. the matrix width). More arms = denser swirl.
- **Color speed**: sets how quickly the hue assignment drifts (range scales
  with matrix size).
- **Animate swirl** (a slider acting as a toggle): only at the far-left
  position is the slow morph animation enabled; anywhere else freezes the
  swirl phase at zero (arms stay in the spread arrangement). A real toggle
  would be a cleaner reimplementation, but match the slider semantics.
- **Swirl speed**: period of the slow morph; slider is inverted (right =
  faster). Even at fastest it is on the order of a minute per cycle.

## Non-obvious bits
- The effect is a draw-into-persistent-buffer-and-decay technique: only a few
  points are drawn per frame, and the exponential decay plus the squared
  brightness at output produce the trails.
- The hue stored is arm-fraction *times* the drifting phase (a product, not a
  sum), so when the phase is small all arms are near red and they fan out
  across the spectrum as the phase grows — the rainbow "opens and closes".
- Frame-rate dependence: the decay is applied per frame, not per unit time, so
  trail length varies with frame rate. A time-corrected decay would be an
  improvement but changes the look on slow devices.
