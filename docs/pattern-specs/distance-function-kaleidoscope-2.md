# distance function kaleidoscope 2
kind: 2D
sensors: no

## What it looks like
An abstract, slowly evolving kaleidoscope on a 2D matrix. Nested diamond-and-square bands radiate from a drifting, rotating center, and where the bands interfere they shatter into self-similar, Sierpinski-like fractal filigree that appears to zoom endlessly inward. Brightness is organic and smoky (fractal ridge noise), with deep blacks and sharp bright veins; hue washes slowly through the whole wheel while the fractal structure adds local hue variation. The whole frame drifts around the panel, breathes, and swings through a full rotation and back over a couple of minutes. Everything is temporally smoothed, so it feels liquid rather than strobing. The author describes it as experiments with an infinite zoom built from differences between distance functions.

## Algorithm
### Per-frame global work
- A fast ramp used to animate the distance bands: a sawtooth over a long base period whose value gets multiplied up by roughly sixteen where used, so bands sweep inward at a comfortable rate (roughly ten seconds per band-cycle).
- Two extremely slow sawtooth offsets (periods measured in tens of minutes) that scroll the second and third axes of the noise field across its seamless repeat period.
- A wandering angular phase: sample smooth noise along a very slowly advancing coordinate, amplify it, and feed it through a sine-shaped wave — yielding an organically meandering value in the unit range.
- A slow base-hue ramp cycling the full wheel over on the order of a minute or two.
- The coordinate transform, rebuilt every frame: center the map on the origin; scale so the visible area covers roughly half a unit around the center (zooming in on the structure); translate by a smoothly wandering offset (sine of a slow-noise sample, amplitude around a third of the panel) independently derived per axis so the center strolls around; rotate by a full turn times a sine-shaped wave of a long-period clock, so the whole image swings through a complete revolution and back over a couple of minutes with sinusoidal easing.

### Per-pixel work
Persistent state: two arrays sized to the pixel count, one for smoothed brightness, one for smoothed hue (frame-to-frame temporal filters). Memory therefore scales with pixel count; otherwise layout-agnostic on any normalized 2D map.

1. Compute the pixel's normalized angle about the origin (full turn mapped to the unit range).
2. Compute two different distance metrics from the origin: taxicab distance (sum of absolute coordinates) pulled down by a small constant offset, and chessboard distance (max of absolute coordinates).
3. **The signature trick:** subtract the animated ramp (scaled up ~16x) from each distance, then combine the two results with a **bitwise XOR of their fixed-point representations** (the engine's numbers are binary fixed-point, so XOR acts on the bits of the fractional value). XOR of two smooth ramps produces self-similar, fractal, Sierpinski-triangle-like interference. Feed the XOR result through a triangle wave to fold it into the unit range. This folded interference value drives both hue and brightness. An implementer must reproduce the fixed-point-XOR behavior (e.g. scale to integers with a generous fractional bit budget, XOR, scale back) — floating-point XOR-as-integers of raw bits will not look the same.
4. Hue: half the interference value plus the slow base hue, then blended into the per-pixel hue memory with a small blend factor (roughly a tenth new per frame) so hue changes lag smoothly; finally folded through a triangle wave before display.
5. Brightness: sample ridged multifractal noise (a few octaves, lacunarity a bit under two, gain a bit under one) at first-axis coordinate = interference value plus half of a triangle wave of (three times the pixel angle plus the wandering angular phase) — this stamps a three-fold angular symmetry that slowly writhes — with the other two axes at the very slow scroll offsets. Subtract a small floor so weak noise goes fully black.
6. Blend that brightness into the per-pixel brightness memory (roughly a third new per frame), then apply a smooth threshold window (soft ramp between a low cut and a high saturation point) and finally raise to the fourth power for deep contrast.
7. Emit at full saturation.

Randomness: none from a random generator; all wandering comes from deterministic smooth noise sampled along slow clocks.

Vestigial content in the original, safe to omit: several additional distance metrics are defined but unused (plain Euclidean, a sine-of-squared-distance, and deviation from the mean point distance), plus multiple commented-out alternative brightness formulas — leftovers of the author's experimentation.

## Colors
Full rainbow over time: a slow global hue cycle with the fractal interference adding up to about half a wheel of local variation, folded so it stays smooth. Saturation is always full; blacks are true black thanks to the noise floor subtraction, threshold window, and fourth-power contrast.

## Timing
Band motion: a band-cycle roughly every ten seconds. Hue: a full wheel in a minute or two. Rotation and drift: a couple of minutes per swing. Noise-field scroll: tens of minutes (imperceptibly slow morphing). Temporal smoothing gives everything a fraction-of-a-second lag that reads as fluidity.

## UI controls
None.
