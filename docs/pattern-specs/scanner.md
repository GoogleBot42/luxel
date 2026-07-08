# Scanner
kind: 1D
sensors: no

## What it looks like
A KITT/Cylon-style scanner, doubled. The strip is split at its midpoint into two independent
halves. In each half a bright "lead" dot sweeps back and forth between that half's two ends,
leaving a comet trail that fades behind it. The two dots start at the opposite outer ends of the
strip and move in mirrored directions, so at default settings the motion looks symmetric: dots
sweep inward toward the middle, bounce, sweep back out, and repeat. Trails fall off steeply
(brightness is cubed at output), giving crisp bright heads with quickly dimming tails. A full
one-way sweep of a half takes on the order of a second at default speed, adjustable much faster
or slower.

## Algorithm
State kept between frames: each half's lead position (a float in pixel units), each half's
current direction (±1), and a per-pixel brightness array covering the whole strip (sized one
larger than the pixel count as an overflow guard for the fill loop below).

Per frame:
1. Advance each lead position by direction × elapsed-ms × speed. When a lead passes its half's
   boundary (the midpoint on the inner side, the strip end on the outer side), clamp it to the
   boundary and flip its direction.
2. Gap filling: because a fast lead can jump several pixels in one frame, walk from the previous
   frame's integer lead position to the new one (in whichever direction it moved) and set every
   crossed pixel's brightness to full. This guarantees an unbroken trail at any speed.
3. Decay: for every pixel in each half, subtract elapsed-ms × a decay rate, clamping at zero.
   The decay rate is what the "trail length" control changes (smaller rate = longer trail).

Per pixel: brightness = stored value cubed (aggressive gamma so tails shorten visually). Hue:
- Rainbow mode ON: hue is proportional to pixel position at double rate, i.e. the full strip
  spans the color wheel twice — each half carries one complete rainbow. The moving dot therefore
  reveals rainbow-colored trails frozen in place along the strip.
- Rainbow mode OFF: the entire strip shares one hue, which oscillates smoothly through the color
  wheel over time (a triangle-style sweep, period from under ten seconds to a couple of minutes
  depending on the color-shift control).
Saturation is always full.

The midpoint is computed by halving (pixel count − 1) with a floor, and the two halves are
defined as index ranges on either side of it, so odd and even counts both work.

## Controls (all exported as sliders)
- Speed slider: scales lead movement speed; the raw slider value is rescaled and clamped into a
  sensible band (roughly a 7× range between slowest and fastest). Full slider = fastest.
- Trail-length slider: inversely mapped to the decay rate and clamped (roughly a 16× range);
  full slider = longest trails.
- Color-shift slider: inversely mapped to the single-hue cycle period and clamped (about a 13×
  range); full slider = fastest color cycling. Only matters when rainbow mode is off.
- Rainbow slider used as a toggle: any nonzero value enables rainbow coloring; zero disables it.
  Defaults to on. (If the host UI supports a real toggle control, that is the honest kind.)

## Colors
Either a positional double rainbow (all hues, fully saturated) or a single fully saturated hue
slowly cycling through the whole wheel. Black background; trails are dimmer versions of the same
hue.

## Layout assumptions
Any 1D strip; everything is derived from the pixel count.

## Notes for reimplementation
- The gap-filling walk (step 2) is the non-obvious part; without it high speeds leave dotted
  trails.
- In single-hue mode the hue is only computed per frame, so the first frame before any
  computation could momentarily render an undefined hue in a careless port — initialize it.
- The brightness array holds raw linear values; all shaping happens at render time via the cube.
