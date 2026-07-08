# millipede 1d/2d controls
kind: 1D+2D
sensors: no

## What it looks like
Inspired by the rippling waves that travel along a millipede's legs. The strip (or, in 2D, the angle around a chosen center point) is divided into many short rainbow-hued segments ("legs"), and waves of brightness ripple continuously along them. The combination of a smoothly drifting hue ramp broken into repeating sawtooth segments, plus a traveling brightness wave, produces a mesmerizing leg-wave motion. In 2D the effect swirls around a center point, and the display can additionally be split into a few concentric rings that carry the wave at staggered phases, like a spiral of ripples. Motion is continuous and fluid; at the default speed a wave cycle takes on the order of a second, and the speed slider spans a wide range.

## Algorithm
State: none between frames beyond two clock phases computed each frame — a primary sawtooth clock and a second one at half the rate (twice the period). Both periods are inversely proportional to the speed setting.

1D per pixel:
1. Start with the pixel's normalized position along the strip as a base hue.
2. Add a "leg" term: take (normalized position plus the slower clock), multiply by half the leg count, and keep only the remainder modulo one-half. This adds a repeating sawtooth (0 up to a half, then snapping back) on top of the base hue ramp — that snap is what creates the visible segment boundaries, and because the slower clock is inside the term, the segment pattern itself drifts along the strip over time.
3. Brightness = a smooth sine-shaped wave of (that final hue value plus the slower clock). Because brightness is a function of the hue ramp, the bright crests travel along the color gradient. Square the brightness for gamma correction.
4. Emit fully saturated hue/brightness.

2D per pixel:
1. Translate coordinates so the user-chosen center is the origin.
2. Compute the radius and the angle around the center.
3. Quantize the radius into N equal concentric bands ("tiers"); each band contributes a fixed phase offset (the band's quantized radius) to the brightness wave, so adjacent rings ripple out of step, giving a spiral/staggered look.
4. The angle, normalized to 0..1 around the circle, plays exactly the role the strip position played in 1D: same leg-segment remainder trick on the hue, same traveling brightness wave (plus the ring phase offset), same gamma squaring.

Randomness: none. Layout: 1D uses normalized position, so any pixel count works; 2D assumes a normalized 0..1 map. No hardcoding to fix.

## Colors
A full rainbow: hue sweeps the whole color wheel across the strip (or around the center), chopped into repeating segments so you see many small rainbow ramps. Always fully saturated; brightness modulation does the animation. Troughs of the wave go to black.

## Controls
- Slider ("legs"): number of segment repeats, quantized to integers from one up to about twenty. More legs = finer, busier segmentation.
- Slider ("speed"): overall animation rate, quantized to integers over a wide range (roughly a 60:1 span).
- Slider ("position X") and slider ("position Y"): move the 2D swirl's center point anywhere on the map. (No effect in 1D.)
- Slider ("tiers"): number of concentric phase-offset rings in 2D, quantized to small integers (one to five). One tier = a pure angular swirl; more tiers = staggered ripple rings. (No effect in 1D; the tier count is also exported as a readable variable.)

## Timing feel
Continuous smooth motion. Mid-slider speeds feel like a comfortable ripple, roughly a cycle per second; the extremes go from very slow undulation to a fast shimmer.

## Clever bits
- The whole "millipede leg" illusion comes from one remainder operation: adding a time-drifting ramp scaled by the leg count and wrapped at one-half onto the hue creates both the segment boundaries and their travel, with no per-segment state.
- Driving brightness with a wave of the (already segmented) hue value makes the light crests follow the color structure, so color and motion feel locked together.
- Using half of the leg count and wrapping at one-half (rather than the full count wrapped at one) keeps the hue excursion within a tasteful sub-range so segments stay recognizably ordered along the rainbow.
