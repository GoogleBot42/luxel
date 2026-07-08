# firework rocket sparks
kind: 1D
sensors: no

## What it looks like
A small, intensely bright "rocket" — a block of a few fully-lit, saturated warm-colored
pixels — glides along the strip, completing a full pass every few seconds and wrapping
around. Trailing a fixed distance behind it, random white sparks crackle: individual
pixels in a small zone flicker on for single frames like sputtering embers. The rest of
the strip is black. The rocket's color slowly shifts through reds/oranges/yellows as it
travels, because its hue is tied to its absolute position on the strip.

## Algorithm
No state beyond a single per-frame phase value that ramps zero-to-one every few
seconds. Everything else is computed per pixel per frame:

- A smooth periodic wave (sine-like, zero-to-one) is evaluated at (phase + normalized
  pixel position). This produces a crest that travels along the strip once per cycle.
- The same wave is evaluated a second time with the position shifted forward by a fixed
  ten-pixel offset. This second crest defines the "rocket" location.
- Rocket mask: a pixel belongs to the rocket only where the shifted wave is extremely
  close to its peak — the threshold is so tight (within a fraction of a percent of the
  maximum) that only a couple of pixels qualify at any instant. This is the trick that
  carves a razor-thin solid block out of a broad smooth wave.
- Spark brightness: where the unshifted wave is near its peak (within the top few
  percent) AND a fresh per-pixel random draw clears a high bar (roughly a one-in-twenty
  chance per frame), the pixel briefly lights at the wave's value; otherwise zero. The
  double condition confines sparks to a small zone offset from the rocket while keeping
  them sparse and strobe-like.
- Coloring: rocket pixels are fully saturated at full brightness, hue taken from a
  spatial ramp — the pixel index maps into a narrow warm band (roughly the red-to-
  yellow fifth of the hue wheel), repeating every twenty pixels, so the rocket's color
  evolves as it moves. Non-rocket pixels are rendered with zero saturation, so the
  sparks come out pure white regardless of hue, and their brightness is just the spark
  value (black when no spark fires). A random hue is also drawn for spark pixels but is
  irrelevant since their saturation is zero — a re-implementer can skip it.

Quirks / cleanup notes:
- A second time-phase value is computed each frame but never used; drop it.
- The ten-pixel rocket/spark separation and the twenty-pixel hue-cycle length are
  hardcoded in pixel units; on very long or short strips the proportions change. The
  obvious fix is to express both as fractions of the pixel count.
- The apparent width of the rocket and spark zones depends on strip length (they are
  defined by wave thresholds over normalized position), so denser strips get slightly
  wider zones.

## Colors
Rocket: saturated warm hues (reds through oranges toward yellow), position-dependent.
Sparks: pure white, frame-length flashes. Background: black.

## Controls
None.

## Timing feel
One full traversal of the strip every few seconds; sparks flicker at frame rate,
each typically lasting a single frame.
