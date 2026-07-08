# Sun rays through trees
kind: 2D
sensors: no

## What it looks like
A warm sun sits at the top-center of the panel, just above the visible frame, so only its lower glow is in view. From it, bright shafts of light fan downward across the whole display, as if sunlight were streaming through a forest canopy. The shafts flicker and drift slowly as turbulent "foliage" noise moves through them, and irregular flare tongues lick outward from the sun itself. Near the sun the light washes out to near-white; farther away it takes on the chosen body color with subtle hue shimmer. Motion is slow and organic — the noise field crawls over tens of seconds, with no hard beats or jumps.

## Algorithm
Setup (once): the coordinate origin is translated so the sun sits at the horizontal center of the panel, slightly above the top edge. The noise generator is configured to wrap in its first dimension with a period approximately equal to a full turn in radians, so the noise field tiles seamlessly around the angular seam (this hides the discontinuity where the angle wraps — a non-obvious but important trick).

Per frame: two animation clocks are advanced. One is a very slow master clock (a loop on the order of a couple of minutes) used as the "time" axis of the noise. The other is a somewhat faster clock (a loop on the order of tens of seconds to a half-minute) that continuously offsets the noise along the radial axis, which makes structure appear to stream outward/inward from the sun.

Per pixel:
1. Convert the pixel's (x, y) into polar coordinates about the sun: an angle and a distance.
2. Sample multi-octave turbulence noise at (angle, distance minus the radial-drift clock, master clock) — roughly three octaves, moderate lacunarity, modest per-octave gain — and invert it so bright regions are where turbulence is low.
3. Shape that value two ways and take the larger:
   - a smooth threshold that keeps only the top portion of the noise range (this carves the continuous field into discrete flare tongues), and
   - a term that grows as the product of distance and noise falls toward the sun's core (this guarantees a solid bright disc at the sun, with a core radius on the order of the coordinate scale and a small inner offset around a quarter of it).
4. Save this pre-ray value; it later drives a hue shift.
5. Multiply in the "rays" filter: a triangle/sine-style periodic wave over the angle, whose frequency is one user-set integer count of rays, phase-modulated by a second, lower-amplitude wave over the angle at a second user-set count (a wave-of-a-wave). The modulation depth of the inner wave is small (roughly a third). The ray filter is cross-faded with the unfiltered value by a user "ray strength" amount, so at zero strength there are no rays and at full strength the rays fully gate the flares.
6. Square the result to deepen contrast.
7. Color: hue is the user's base hue plus a signed shimmer proportional to the saved flare value (centered so typical flare levels give near-zero shift; the shimmer range is set by a user amount, defaulting to a modest fraction of the hue wheel). Saturation is the user's saturation scaled by a term that increases steeply with distance from the sun and decreases with brightness — so pixels close to the sun, and the brightest ray cores, desaturate toward white. Brightness is the shaped value times the user's brightness.

No state persists between frames other than the two clocks. Randomness comes entirely from the coherent noise field; there is no per-frame RNG. Layout: fully map-driven 2D, no pixel-count hardcoding; it needs a 2D mapping and looks best on a panel.

## Colors
Default is a warm golden-amber body color. The overall look is: black background, through the chosen warm color in the ray shafts, to near-white at the sun and at flare cores. The hue-shimmer control lets flares wander a little around the base color (e.g. amber drifting toward orange-red or yellow-green at the extremes).

## Controls
- Slider "ray strength": blends between pure turbulent flares (low) and strongly banded discrete rays (high). Default fairly strong.
- Slider "ray count A": integer number of primary ray bands around the sun, from one up to roughly nine. Default a handful.
- Slider "ray count B": integer count of the secondary wobble wave that perturbs the primary rays, same range. Default a few.
- HSV color picker "base color": the body color of the rays/sun.
- Slider "color variation": how far the flare-driven hue shimmer can wander from the base hue. Default modest.

## Timing
Slow and ambient: the turbulence evolves over a cycle lasting on the order of a couple of minutes, while radial drift completes a loop in roughly half a minute. Nothing changes abruptly.

## Non-obvious details
- The polar conversion plus angular noise-wrap is what makes rays radiate cleanly from an off-screen point with no visible seam.
- The wave-of-a-wave phase modulation is what keeps the rays from looking like a static geometric asterisk — they bend and shimmer slightly.
- Desaturating by "steep-with-distance minus brightness" is a cheap way to get a white-hot core that re-saturates into color as light travels outward.
