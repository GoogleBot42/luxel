# Upward waves 3D using accelerometer
kind: 3D
sensors: yes (3-axis accelerometer)

## What it looks like
On a 3D-mapped fixture (designed for a mapped LED cube), horizontal bands of light continuously rise "upward" — and "upward" means real-world up: tilt or flip the whole fixture and the bands keep climbing against gravity, as if the animation were a liquid level that stays oriented no matter how you hold the object. The bands sweep from bottom to top roughly once or twice per second, giving a lively pulsing-fountain feel. Color varies with distance from the vertical center axis: pixels near the core of the fixture sit at the cool end (cyan/blue), shifting through blue-violet and around toward warm hues as you move outward, so each rising band is a concentric ring of rainbow. Shaking or accelerating the fixture makes the bands flare brighter; at rest they peak at normal full brightness.

## Sensor inputs
- Conceptual input: the 3-axis accelerometer vector (which at rest measures gravity, telling you which way is down; under motion it also carries shake energy).
- Two things are derived from it each frame:
  1. Orientation — the direction of gravity, expressed as two spherical angles (tilt from vertical, and compass direction of the tilt).
  2. Overall g-force magnitude — the length of the acceleration vector, rescaled so that resting gravity is about one; this directly multiplies band brightness, so shaking over-drives the bands and free-fall dims them.

## Algorithm

### Frame-setup step (state kept between frames)
- Persistent state: the three smoothed acceleration components. Each frame they are updated with a simple first-order IIR low-pass (exponential moving average): new = old x (smoothing factor) + sample x (one minus factor), with the factor around two-thirds to three-quarters — heavily smoothed so orientation changes feel damped and fluid rather than jittery.
- The raw axes are permuted and sign-flipped to match the author's physical mounting (their sensor axes did not match the map axes). A reimplementation should treat this remap as build-specific.
- From the smoothed vector, compute the two spherical angles of the gravity direction: the polar angle (angle away from the vertical axis; handled piecewise to avoid divide-by-zero when the vector lies in the horizontal plane or points straight down) and the azimuth angle (direction of the projection onto the horizontal plane, again with the axis-aligned cases special-cased). Both are then offset by a quarter turn to line up with the map convention.
- A sawtooth phase advancing on a period of well under a second drives the band motion.
- Coordinate pipeline: reset the world transform, translate so the fixture's map is centered on the origin, then rotate about two axes by the derived angles so that the map's nominal "up" axis is re-aligned with true gravity. There is also a build-specific optional pair of fixed rotations (disabled by a code constant) that pre-tips a cube onto its corner to match a corner-standing display stand.

### Per-pixel render (receives the transformed 3D coordinates)
- Hue: radial distance from the vertical axis (using only the two horizontal coordinates), scaled by roughly one and a half and offset by half the color wheel. Center pixels land at the cool half of the wheel; hue advances (wrapping) with radius.
- Brightness: a triangle wave over (vertical coordinate minus the moving phase) creates a periodic band along the vertical axis. The wave is shifted down and re-expanded so only its upper half survives, negatives clipped to zero — leaving one bright band per period with dark gaps. It is multiplied by the g-force magnitude before clipping, then raised to about the fourth power to sharpen the band into a crisp, narrow stripe with soft falloff.
- Full saturation everywhere.

## Colors
A radial rainbow: cool cyan/blue at the fixture's core, sweeping through blue-violet toward warm hues (wrapping the wheel) at the outer edges. Saturation is always full; only brightness carves out the rising bands, so unlit areas are black.

## Controls / configuration
No slider UI. Two compile-time constants: the smoothing strength of the accelerometer filter, and the on/off switch for the corner-standing-cube pre-rotation. Several internals are exported for the variable watch list (angles, filtered axes, g-force) purely for debugging.

## Timing
Bands rise continuously, completing a sweep in well under a second (feels like one to two sweeps per second). Orientation response is deliberately laggy-smooth — tip the cube and the animation re-levels over a noticeable fraction of a second rather than instantly.

## Layout assumptions
Requires a 3D pixel map with coordinates normalized to the unit cube (it recenters them itself). No pixel-count assumptions. The axis permutation/negation and the sensor-scale factor (raw samples are a small fraction of a g, hence the rescale to make gravity about one) are hardware/build-specific and should be configurable.

## Non-obvious notes
- The whole "gravity-stabilized" trick is done in the transform stage, not per pixel: two rotations derived from the smoothed gravity vector are pushed onto the coordinate transform each frame, so the per-pixel code stays trivial.
- The spherical-angle helpers are written piecewise by hemisphere/quadrant specifically to dodge divide-by-zero and to keep the inverse-tangent in the right range; using a proper two-argument arctangent builtin is the clean reimplementation.
- Multiplying brightness by g-force before the fourth-power sharpening means shake-induced flares also widen the bands, not just brighten them — a subtle part of the reactive feel.
