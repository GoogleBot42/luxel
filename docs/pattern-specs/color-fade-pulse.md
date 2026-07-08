# color fade pulse
kind: 1D
sensors: no

## What it looks like
A scrolling rainbow strip overlaid with a handful of narrow, bright pulse peaks that
drift along the strip, while the whole thing periodically washes out toward white and
back to fully saturated color. The overall impression is a fast-moving colorful shimmer:
sharp bright ridges gliding over a dimmer rainbow background whose vividness breathes
in and out.

## Algorithm
No state is kept between frames beyond three phase values recomputed each frame from
free-running sawtooth timers (each timer ramps from zero to one and wraps). Three
independent timers run at different speeds:

- a fast one driving the hue scroll,
- a slower one (converted to an angle spanning a full circle) driving the saturation wave,
- a medium one driving the brightness-pulse motion.

Per pixel, using the pixel's normalized position along the strip (position divided by
total pixel count — fully layout-proportional, nothing hardcoded):

1. **Hue** = twice the normalized position, minus the fast timer phase. Two full hue
   cycles are laid across the strip and the whole rainbow scrolls steadily in one
   direction. (Hue wraps naturally.)
2. **Saturation** = a sinusoid, remapped from its signed range into the zero-to-one
   range, whose argument is the slow circular phase plus half a hue-circle's worth of
   positional offset across the strip. So saturation forms one long spatial wave over
   the strip that slides with time: parts of the strip periodically fade to
   white/pastel and re-saturate.
3. **Brightness** = a triangle wave evaluated on the fractional part of (medium timer
   phase + four times the normalized position). That puts roughly four triangular
   brightness peaks across the strip, moving with time. The triangle value is then
   raised to the fourth power and cut to about half amplitude. The fourth power
   sharpens the broad triangles into narrow bright spikes with long dark valleys
   between them; the halving keeps peak brightness moderate.

Output is set in hue/saturation/value space.

## Colors
Full spectral rainbow (all hues, twice across the strip), continuously scrolling.
Saturation swings between fully vivid and washed-out near-white in a slow spatial wave.
Background between pulses is near-black because of the sharpened brightness curve.

## Controls
None.

## Timing
Everything is brisk: the hue scroll completes a cycle in well under a second, the
brightness pulses drift with a cycle time of a second or two, and the saturation wash
breathes over several seconds. The three periods are unrelated, so the combined look
never visibly repeats.

## Non-obvious bits
- Raising the triangle brightness wave to the fourth power is what turns a bland
  triangle gradient into distinct "pulses" — that's the core trick of the pattern.
- Because the three timers have incommensurate periods, hue, saturation, and
  brightness phases slide against each other, giving a rich look from very little code.

This is a small pattern; the above is the whole of it.
