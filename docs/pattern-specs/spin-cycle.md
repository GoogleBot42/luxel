# spin cycle
kind: 1D
sensors: no

## What it looks like

A handful (about five) of sharp bright bands race along the strip on a
several-second loop, dark between them. Each band is painted from a
compressed slice of the rainbow — the slice only spans about half the color
wheel at any instant — and that slice continuously rotates around the wheel
while the density of hue striping across the strip "breathes" between roughly
five and ten repetitions. The overall impression is spinning multicolored
bars whose colors churn like a washing machine.

## Algorithm

Stateless apart from the frame clock. This is a simple pattern; the whole
thing is one hue formula and one brightness formula per pixel.

Per frame: sample a sawtooth time base with a several-second period. (The
original samples the same-period sawtooth twice into two variables; they are
always equal, so a single time base suffices.)

Per pixel:

- Hue: take the pixel's normalized position along the strip and multiply it
  by a repetition count that oscillates smoothly between about five and
  about ten (base of about five plus an equal-amplitude smooth oscillation
  of the time base). Add a scrolling offset (a smooth oscillation of the
  time base, scaled up a couple of times). Then reduce the result modulo
  one-half — this folds all hues into a half-wheel window — and add the raw
  sawtooth so the window itself rotates steadily around the full wheel once
  per cycle. Full saturation.
- Brightness: a triangle wave over (normalized position times about five,
  plus the sawtooth times about ten, fractional part) — this yields about
  five triangular bright bands that translate along the strip several times
  per cycle. Cube the triangle value so the bands are narrow and punchy with
  dark gaps.

No randomness, no layout assumptions beyond using the runtime pixel count
for normalization.

## Colors

Full-saturation spectrum colors; at any moment only about half the wheel is
present, and which half rotates continuously, so over a few seconds every
hue appears.

## Controls

None.

## Notes

Near-trivial. The only subtlety is the modulo-by-a-half hue fold plus the
added sawtooth, which produces the "rotating compressed rainbow" look, and
the cubed triangle for crisp bars.
