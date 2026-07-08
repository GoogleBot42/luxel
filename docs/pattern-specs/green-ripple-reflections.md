# green ripple reflections
kind: 1D
sensors: no

## What it looks like
Soft green ripples with white glints, like moonlight reflecting off gently moving water. Several overlapping waves of light slide along the strip — some drifting one way, some the other, one swaying back and forth — and where they interfere you get bright crests and dark troughs. The crests are green where one particular wave is strong and wash out toward white where it is weak, so highlights sparkle between green and pale white. Each underlying wave cycles over a couple to a few seconds, but because their periods are close-but-different the combined pattern never visibly repeats. Overall brightness is deliberately capped around half, keeping it moody.

## Algorithm
No per-pixel state; everything is a pure function of pixel position and three phase clocks.

Per frame: advance three independent sawtooth phase clocks with periods of roughly two, three, and two-and-a-half seconds (close but incommensurate, so the composite motion has a very long effective repeat). Each is scaled to a full circle of phase per cycle.

Per pixel, three spatial components are computed from the pixel's fractional position along the strip:
1. A sine wave with about five full spatial cycles across the strip, phase-shifted by the first clock so it drifts steadily in one direction; it is then squared — making it non-negative, sharpening its crests, and doubling its apparent spatial frequency.
2. A sine wave with about three full spatial cycles, phase-shifted by the second clock in the opposite sense so it drifts the other way; left signed (it can go negative).
3. A triangle wave spanning roughly one and a half spatial cycles, whose phase does not drift continuously but instead sways back and forth sinusoidally under the third clock (position term plus a sine of the clock, folded into range with a modulus).

Brightness: average the three components, square the result, and halve it. Note the average can be negative (component two is signed); squaring folds those negative troughs back into faint positive light rather than clipping to black, and the squaring also deepens contrast so most of the strip sits dim with pronounced bright crests.

Color: hue is fixed at green. Saturation is driven by the first (squared-sine) component: full green where that wave is strong, desaturating toward white where it is weak. This coupling is what produces the "reflection" glints — bright spots created mostly by components two and three, in places where component one is near zero, come out white instead of green.

No randomness, no layout hardcoding — works at any pixel count, 1D by fractional position.

## Color
Single-hue green palette that continuously desaturates to white at the glint points; blacks/dark greens in the troughs. Never exceeds moderate brightness.

## Controls
None.

## Non-obvious details
- Reusing one wave both as a brightness ingredient and as the saturation control is the whole trick: it guarantees the white sparkles land exactly where the green wave "isn't", which reads as specular reflection.
- Squaring the summed field (rather than clamping) means destructive interference produces faint glow, not hard black, giving the water-like softness.
- Three slightly-different clock periods on similar spatial frequencies is a cheap way to get organic, non-repeating motion from pure sines.
