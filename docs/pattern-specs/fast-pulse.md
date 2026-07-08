# fast pulse
kind: 1D
sensors: no

## What it looks like

A single narrow, intense pulse of light sweeps back and forth along the strip. The
pulse has a white-hot core with saturated colored edges. The color of the fringes
drifts slowly through the whole rainbow. The sweep is not linear: because the pulse's
position is driven by a sine-shaped oscillation, it races through the middle of its
travel and lingers at the turnaround points. Over one full color cycle (several
seconds), the pulse wraps around the strip a couple of times in each direction.

## Algorithm

This is a short, nearly stateless pattern — no arrays, no persistent per-pixel state.

Per frame:
- One master clock: a slow sawtooth ramp with a period of several seconds. It serves
  double duty as (a) the hue for the whole frame and (b) the phase driver for the
  pulse position.

Per pixel:
- Compute a phase = (an oscillating offset + the pixel's fractional position along
  the strip), wrapped to the unit interval. The oscillating offset is a sine-shaped
  wave of the master clock, scaled to span roughly two full wraps — this is what makes
  the pulse travel around the strip multiple times per cycle and reverse direction
  smoothly.
- Feed that wrapped phase into a triangle wave (peak in the middle of the interval)
  to get a raw brightness that peaks at exactly one position along the strip.
- Raise the brightness to a high power (around the fifth) to sharpen the broad
  triangle into a narrow pulse with long dim tails.
- Saturation trick: pixels whose sharpened brightness is above a high threshold
  (roughly the top tenth of the range) get zero saturation — the very peak of the
  pulse renders white, while the shoulders stay fully saturated. This gives the
  "white-hot core with colored fringe" look with a one-line comparison.
- Emit HSV: hue = master clock value, saturation = that threshold result,
  value = sharpened brightness.

## Colors

Continuous rainbow drift for the fringe color; the pulse core is white. Background
is effectively black (the tails of the pulse fall off very fast due to the power
sharpening).

## Controls

None.

## Layout assumptions

Pure 1D. Uses the pixel's fractional position, so it scales to any strip length
automatically. Nothing hardcoded.

## Notes

Trivial-to-small pattern; the two clever bits are the power-sharpening of a triangle
wave into a pulse, and using a brightness threshold as a boolean saturation to
whiten the peak.
