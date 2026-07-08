# fast pulse 3d
kind: 1D+2D+3D
sensors: no

## What it looks like
Sharp, narrow bright pulses sweep rapidly through the display. Each pulse has a
blazing white-hot core with a saturated colored fringe; the fringe color slowly cycles
through the whole rainbow over several seconds. In 3D the pulses are glowing planes
that sweep back and forth through the volume while their orientation slowly tumbles;
in 1D they are bright dots/bands racing along the strip. The sweep is sinusoidal, so
pulses whip through the middle of their travel and linger briefly at the extremes.

## Algorithm
Per-frame state (no persistence beyond the current frame):
- A master phase ramping zero-to-one over several seconds. It serves double duty as
  the current hue and as the driver of the pulse motion.
- Three sine oscillators with different periods, all in the several-second range (one
  matches the master period, one roughly half of it, one roughly two-thirds). Their
  instantaneous values act as per-axis weights, i.e. the components of a slowly
  tumbling direction vector.

Per pixel, 1D renderer:
- Compute a sawtooth-style folded value: take a smoothly oscillating offset (a sine
  shaping of the master phase, scaled up by about two) plus the pixel's normalized
  position, wrap into the unit interval, and feed through a triangle wave. This yields
  a brightness that peaks along a moving locus.
- Raise that brightness to roughly the fifth power — this is what turns broad waves
  into thin, hard-edged pulses with dark gaps.
- Saturation is binary: fully saturated except where brightness is in the top roughly
  tenth of its range, where saturation drops to zero — producing the white-hot core.
- Hue = the master phase (uniform across the display at any instant, cycling over
  several seconds).

Per pixel, 3D renderer: same recipe, but the position term is the dot product of the
pixel's (x, y, z) coordinates with the three oscillator values, and the moving offset
is scaled about three-fold instead of two. Because the axis weights are sinusoids of
different periods, the pulse planes continuously change orientation. The white-core
threshold is slightly more generous (top roughly fifth of the range). The 2D renderer
simply calls the 3D one with a zero third coordinate, so flat matrices get a 2D slice
of the same effect.

Nothing is randomized; the motion is fully deterministic and loops on the beat pattern
of the mismatched oscillator periods. No layout assumptions beyond normalized
coordinates; the 1D path uses index divided by pixel count, so it scales to any strip.

## Colors
Full-spectrum rainbow hue shared by all pixels at once, cycling continuously; pulse
cores blow out to white. Background between pulses is essentially black thanks to the
power-law sharpening.

## Controls
None.

## Timing feel
Fast. Pulses cross the display multiple times in a few seconds; the hue takes several
seconds per full rainbow; the 3D plane orientation drifts on a similar several-second
timescale so the overall texture never exactly repeats quickly.
