# Sierpinski Rainbow 2D
kind: 1D+2D
sensors: no

## What it looks like
On a 2D matrix: a Sierpinski-triangle-like fractal texture fills the display — nested self-similar triangular voids — rendered in continuously cycling rainbow colors. The fractal structure itself is static (it is a pure function of pixel position), but brightness ripples flow through it and the whole hue field slowly rotates through the rainbow, so it reads as a shimmering, breathing fractal. On a 1D strip it degrades to a symmetric rainbow ripple that is mirrored about the strip's midpoint, with waves of brightness pulsing outward/through it.

Motion feel: two independent slow drifts, each a handful of seconds per cycle, at slightly different rates (one roughly a third slower than the other), so the brightness ripples and the hue rotation slide against each other and the overall look never exactly repeats on a short timescale.

## Algorithm
State between frames: just two free-running cyclic clocks (phases in the unit interval), updated once per frame. One clock drives the brightness-ripple phase; the other, slightly slower one drives the hue rotation. No other state; every pixel is computed independently each frame.

Per-pixel work, 2D renderer:
1. Compute a "fractal coordinate" for the pixel by taking the bitwise AND of the x and y world coordinates. This is the whole trick: coordinates are fractions in the unit interval, and on this platform numbers are fixed-point, so bitwise-ANDing the two fractional values ANDs their binary fraction bits. The set of positions where bits coincide/survive forms the classic Sierpinski pattern (the same reason Pascal's triangle mod 2 is Sierpinski). The result is a per-pixel scalar that carries the fractal structure.
2. Feed that scalar through a sine-shaped wave function (mapping the unit interval to a smooth 0..1 hump), then through the same wave function again with the first clock's phase added, and once more with the same phase added again — i.e. two nested wave evaluations offset by the ripple clock. Square the final value to deepen contrast. That squared value is the pixel's brightness.
3. Hue = the raw fractal scalar plus the second clock's phase (wrapping); saturation is full. So hue varies across the fractal structure and the whole palette rotates over time.

Per-pixel work, 1D renderer: identical wave chain and coloring, but the "fractal coordinate" is replaced by a folded position ramp: 1 at the strip's midpoint falling linearly to 0 at both ends (i.e. mirror symmetry about the center). One extra wave-shaping step is applied to this ramp before the time-offset wave chain (the structure is otherwise the same as 2D).

Randomness: none. Fully deterministic.

Layout assumptions: none problematic. The 1D path derives its ramp from the total pixel count (fine, not hardcoded to a specific count). 2D path assumes world coordinates normalized to the unit square, as usual. Note for implementers: the 2D version's character depends entirely on fixed-point bitwise AND semantics for fractional coordinates; if your engine uses floats, emulate it by scaling both coordinates up by a power of two large enough to expose ~8+ fraction bits, truncating to integers, ANDing, and scaling back down — resolution of the emulation controls fractal depth.

## Colors
Full rainbow, fully saturated, continuously cycling. Brightness runs from black through to full brightness with a squared (contrast-boosted) response, so dark regions dominate and bright fractal filaments pop.

## Controls
None.

## Timing
Both clocks cycle in the several-seconds range; the hue clock is roughly 30% slower than the ripple clock. Nothing frame-rate dependent.

## Clever bits
- The entire fractal is one bitwise AND of the two coordinates — Sierpinski for free from fixed-point arithmetic.
- Chaining the same smooth wave function into itself (with a time offset injected at each stage) turns a static scalar field into rich, non-repeating-looking interference ripples with almost no code.
