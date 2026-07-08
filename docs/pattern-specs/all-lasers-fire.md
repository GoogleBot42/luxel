# All Lasers Fire
kind: 1D+2D
sensors: no

## What it looks like

On a 2D panel: volleys of intense laser-like beams and sparks that appear to
fire upward from the bottom of the display. Over each cycle the picture
morphs between two regimes — moments of near-perfect order (a regular grid or
fan of bright points/beams) and moments of chaotic spray (beams shattering
into irregular bursts). Every bright feature has rainbow chromatic fringing:
a white-hot core edged with red/green/blue separation, like light through a
prism or a badly aligned projector. Cycle length is user-adjustable from
frantic (several volleys per second) to a slow multi-second evolution.

On a 1D strip it degrades gracefully to a line of racing, splintering sparks
with the same rainbow fringing.

## Algorithm

The image is three copies of one scalar field — one per additive color
channel (red, green, blue) — each evaluated with a *tiny* offset in position
and a small offset in time phase. Because the field is spatially chaotic,
these tiny offsets decorrelate the channels just enough to produce colored
fringes around every feature while cores where all three agree stay white.

Per frame:

- A sawtooth time base with the user-set period; also convert it to an angle
  over a full turn.
- A "chaos factor" that starts each cycle at a small value and shrinks
  linearly to zero as the cycle completes (one minus the sawtooth, times a
  small constant).
- Three per-channel "envelope" values: the user's blast-scale setting plus a
  smooth unit oscillation of the time base, each channel's oscillation phase
  nudged slightly later than the previous one.

Per pixel (2D render):

1. Flip the vertical axis so the origin sits at the bottom — the beams then
   radiate from the bottom corner upward. (A comment in the original invites
   changing this to suit the mounting orientation.)
2. Compute the pixel's distance from the origin.
3. For each channel in turn (red, then green, then blue):
   - Compute a scale factor = distance × that channel's envelope × the
     *tangent* of (distance × chaos factor − the time angle, advanced a
     small extra phase step per channel).
   - Divide the (slightly per-channel-offset) coordinates by this scale
     factor and take the fractional part of each, giving a repeating unit
     tile.
   - Measure the distance from that fractional position to a fixed point
     slightly past the tile's center, invert it (a small numerator divided
     by that distance) to get a bright point-light falloff, then divide by
     the pixel's distance from the origin so features near the origin blaze
     hotter.
   - Cube the result for hard contrast (dim regions crushed to black, bright
     cores kept).
   - Before the next channel: nudge the working coordinates down-left by a
     minuscule fixed offset and reduce the stored distance by the matching
     diagonal amount, so each channel samples an almost-but-not-quite
     identical field.
4. Emit the three results directly as RGB.

The tangent is the heart of the effect: as its argument sweeps through a
cycle, the scale factor runs from enormous (one giant tile — a single beam)
through moderate (an orderly repeating grid of sparks) to near a pole
(explosive chaos), and its sign flips give sudden re-fires. The
distance-dependent term inside the tangent means different radii hit poles at
different times, so rings of chaos propagate outward. As the chaos factor
decays to zero within each cycle, the picture relaxes toward pure order
before the next cycle re-arms it.

1D fallback: the strip is treated as a horizontal line along the field's
bottom edge; the x coordinate is the pixel index divided by (a few times the
square root of the pixel count), so longer strips automatically span a
proportionally wider slice of the field. No other layout assumptions.

No randomness anywhere — all chaos is deterministic from the tangent.
Per-frame state is just the time base and the derived envelopes.

## Colors

Not palette-based. Pure additive RGB: independent red, green, and blue
renderings of the same field. Visually: white/near-white cores with rainbow
chromatic-aberration edges — thin red, green, and blue fringes around every
beam and spark, on black.

## Controls

- Slider, "speed" concept: sets the cycle period. The response is inverted
  and cubed so the right end of the slider is dramatically faster than the
  left; a tiny floor keeps it from ever fully stopping.
- Slider, "blast scale" concept: overall size of the laser features, from
  very fine (small fraction of the default) up to roughly ten times, linear.

Both current values are exported as inspectable variables.

## Notes

Key non-obvious ingredients: (a) tangent poles as a free chaos/order
oscillator; (b) triple-rendering with sub-pixel spatial offsets and small
temporal phase offsets per channel to fake chromatic aberration; (c) cubing
each channel for contrast; (d) the per-cycle decaying chaos factor so every
cycle ends in order and re-arms.
