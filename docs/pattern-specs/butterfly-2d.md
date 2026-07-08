# Butterfly 2D
kind: 2D
sensors: no

## What it looks like
A single procedurally generated butterfly centered on the matrix, wings patterned in several complementary hues with dark vein-like mottling. It sits nearly still, begins to flap, flaps faster and faster until — at peak flutter — it drifts upward off the top of the display ("flies away"). When the flapping dies back down to stillness, a brand-new butterfly appears: new wing outline, new wing pattern, new color family, new resting tilt. Every butterfly is different. The whole live/fly-away/reborn cycle takes several seconds.

## Core techniques (the clever bits)
1. Wing silhouette as a polar signed-distance field: convert the pixel position to angle and radius, then define the wing boundary radius as a constant plus a weighted sum of about seven sinusoidal harmonics of the angle (a small trigonometric series — this is what produces the scalloped, lobed butterfly outline; the specific weights matter less than having a mix of low odd/even harmonics plus one high-frequency ripple term). Subtract the pixel's radius to get an inside/outside field, then push it through a smooth threshold over a narrow band for antialiased edges.
2. Infinite variety via seeded noise: add a modest amount of two-seed perlin noise (seeds randomized per butterfly) to the SDF before thresholding — each seed pair deforms the outline differently, so each butterfly's wing shape is unique.
3. Symmetry: take the absolute value of the horizontal coordinate (after centering) so one wing computation yields both wings mirrored about the body axis.
4. Flap by anisotropic scaling: the horizontal coordinate fed into the SDF is multiplied by a "flap" factor oscillating between well under one and roughly double — compressing and stretching the wings horizontally reads convincingly as flapping seen face-on. Before that, small fixed shaping: vertical is flipped and stretched somewhat, and horizontal gets a mild vertical-dependent pinch (a low-amplitude sine of the vertical coordinate) to give the body/wing-root taper.
5. Frequency-modulated flapping: the flap oscillator's frequency is itself a slow sine of elapsed time (period around several seconds), sweeping from zero (motionless) up to rapid flutter and back. This one mechanism produces the whole behavioral arc: stillness → gentle flapping → frantic flutter → calm.
6. Golden-ratio hue stepping: the base hue advances by the golden-ratio conjugate (~0.62 of the hue wheel) for each new butterfly, and the within-wing hue bands are also spaced by golden-ratio steps — giving well-distributed, non-repeating, complementary color schemes for free.

## Per-frame work (state kept between frames)
State: an accumulated elapsed-time base (wraps after about an hour), the current shape/texture seeds, base hue, resting tilt angle, and a vertical fly-away offset.
- Compute the flap frequency from the slow sine sweep; compute the flap scale factor from a triangle/sine oscillator at that frequency.
- If the frequency is essentially zero (the still moment of the sweep): regenerate the butterfly — new random shape seed and color seed, reset the vertical offset, pick a new random tilt within roughly a third of a half-turn either side of upright, and advance the base hue by the golden step.
- If the frequency is above a high threshold (near peak flutter): increment the vertical offset a little each frame, so the butterfly slides steadily off-screen.
- Reset the coordinate transform, translate the origin to the display center, and rotate by the tilt angle (using the engine's built-in 2D transform stack so per-pixel code sees pre-transformed coordinates).

## Per-pixel work
- Mirror the horizontal coordinate; add the fly-away offset to the vertical coordinate.
- Silhouette brightness: evaluate the wing SDF at (horizontal × flap factor, vertical).
- Wing texture: a second perlin lookup at a moderately zoomed-in scale, with the horizontal input also multiplied by the flap factor (so the pattern shifts as the wings beat) and a fixed seed pair including the per-butterfly color seed; remap to a zero-to-one value.
- Quantize the texture value into about five equal bands; each band selects a hue = base hue + band index × golden step (wrapped) — hence up to five complementary hues patterning the wings.
- Saturation: a value somewhat above full minus the texture value — the highest-texture regions desaturate toward white highlights.
- Brightness: silhouette × the texture value cubed, floored at a tiny minimum so the whole wing interior stays faintly visible; the cubing carves dark vein-like structure into the wings.

## Colors
Not fixed: each butterfly gets a fresh base hue, with up to five golden-ratio-spaced companion hues inside the wings, mostly at strong saturation with near-white sparkle in the densest texture, over a black background. Reads as jewel-toned stained-glass wings.

## Controls
None. Obvious optional additions: flap-cycle period, fly-away speed.

## Timing
One flap-frequency sweep (still → flutter → still): around several seconds. Individual wing beats range from none to several per second at peak. A new butterfly appears each cycle.

## Layout assumptions
Normalized 2D coordinates with the engine's transform stack (translate + rotate); no pixel-count dependence. Needs a real 2D map; a 3D variant would need a projection decision.
