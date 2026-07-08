# blink fade
kind: 1D
sensors: no

A classic twinkle effect: every pixel independently pops on at a random brightness, fades slowly to black, then instantly re-ignites with a fresh random brightness and a new hue. The overall impression is a gently shimmering field of colored sparkles whose color scheme slowly drifts around the rainbow.

## Visual behavior
At any moment the strip is a mix of pixels at all stages of their fade: some just flashed bright, most are somewhere in mid-decay, some are dark and about to re-ignite. Because each pixel restarts at a random height on the fade curve, restarts are desynchronized and the field looks organic rather than strobing. Each individual fade takes several seconds. Hues are not uniform: at any instant the strip carries a gentle positional gradient spanning roughly a fifth of the color wheel, and the whole palette slowly rotates through the full rainbow over a handful of seconds.

## Algorithm
State kept between frames: two arrays sized to the pixel count — one holding each pixel's current brightness level, one holding each pixel's assigned hue.

Per frame (before rendering), for every pixel:
- Decrease its stored brightness by a small amount proportional to the frame's elapsed time (frame-rate independent linear decay). The rate is tuned so a full-brightness pixel takes on the order of a few seconds to reach black.
- If the brightness has reached or dropped below zero, "re-ignite" the pixel: set its brightness to a fresh uniform random value between zero and full, and assign it a new hue computed as (a) a global slowly-cycling time value (several seconds per full color-wheel revolution) plus (b) a positional offset shaped as a triangle wave over the pixel's fractional position along the strip, scaled to roughly one fifth of the hue range. The triangle shaping makes the positional gradient rise then fall symmetrically along the strip, so the two ends match hues.

Per pixel at render time: read the stored hue and brightness, square the brightness before display (a cheap gamma-ish curve that makes the tail of each fade look smoother and the pops punchier), and emit at full saturation.

Randomness: a single uniform random draw per re-ignition (the restart brightness). Hue is deterministic given time and position.

Layout: fully parameterized by pixel count; no hardcoding. Works on any length; on 2D mappings it twinkles in index order, which usually still looks fine.

## Colors
Always fully saturated rainbow hues. At any instant the strip spans a narrow rainbow band (about a fifth of the wheel) arranged as a symmetric gradient along the strip, and that band continuously drifts around the entire wheel. Brightness per pixel runs from black up to fully bright versions of those hues.

## Controls
None.

## Non-obvious details
- A pixel's hue is frozen at the moment it re-ignites and stays fixed for its whole fade; only newly ignited pixels pick up the drifted palette. This gives pleasant color variety since neighbors ignited at different times carry slightly different hues.
- Restarting at a *random* brightness (rather than always full) is what keeps the pixels permanently desynchronized and makes the density of bright sparkles feel constant.
- Squaring the brightness at render time is the perceptual trick that makes the linear decay read as a natural-looking fade.
