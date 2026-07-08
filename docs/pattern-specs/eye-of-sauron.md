# Eye of Sauron
kind: 2D
sensors: no

## What it looks like
A great fiery eye filling a 2D display, in the style of the movie version: a wide, almond-shaped oval of churning flame with a dark vertical slit pupil at its center. Wispy flame tendrils radiate from the pupil to the rim and continuously stream outward; their shapes slowly writhe and morph. Flame brightness runs from black through deep red into golden yellow with white-hot filament cores. The outward streaming is constant and hypnotic; the overall texture takes several minutes before it repeats.

## Algorithm
Per frame, two phase clocks are maintained, both mapped onto the wrap period of the noise field (the noise function repeats seamlessly on a fixed integer period, so each clock sweeps zero-to-that-period):
- A slow "morph" clock: one full sweep takes on the order of several minutes (the source comments it works out to seven-to-eight minutes of unique noise). It feeds the third noise dimension, slowly reshaping the flame filaments.
- A faster "radial scroll" clock: one full sweep takes a couple-to-few minutes, but because it is subtracted from the radial noise coordinate it produces continuous visible outward streaming at a comfortable flame-like pace.

Also per frame, the coordinate transform is reset and rebuilt: translate so the display center is the origin, then scale up by roughly a factor of three, with the vertical axis scaled roughly 40% more than the horizontal. Because coordinates are magnified more vertically, the visible eye is a wide, flattened almond. Finally the noise field's wrap is set on its first axis to exactly the current angular-density value (see clever bits).

Per pixel (coordinates arrive post-transform, centered on the eye):
1. Convert to polar: radius via Euclidean distance from center; angle normalized so a full turn spans the unit interval.
2. Sample ridged fractal noise ("ridge" variant of perlin-style fractal noise, a few octaves, roughly half gain per octave, lacunarity just above one) at coordinates: (angle times angular density, radius times radial density minus the radial-scroll clock, the morph clock). Ridge noise is what gives sharp bright filament veins rather than soft clouds.
3. Oval edge fade: compute how far inside the outer boundary the pixel is (boundary radius is the transform scale minus one, in transformed units), clamp to the unit range, and attenuate the noise value by the square of that inwardness — so the flame fades to black at the almond rim with an accelerating (sharper near the edge) falloff.
4. Pupil: subtract from the value a term equal to a "dilation" amount minus the distance from center computed with the horizontal coordinate multiplied by a "slitness" factor (clamped so it only ever subtracts, never adds). Weighting x heavily makes the dark region a tall narrow vertical slit — the cat-eye pupil. Larger dilation makes the dark region bigger; larger slitness makes it narrower/more slit-like.
5. Clamp the value to at most one (so the palette does not wrap around from white back to black), then paint using the value as both palette position and brightness.

No state other than the two clocks; no randomness at all — everything is deterministic noise-field sampling. Fully layout-agnostic (normalized 2D coordinates only). 2D renderer only.

## Color
A four-stop palette: black at the bottom, through deep red reached fairly early (around the first fifth), holding through orange to bright yellow near the top (around four-fifths), ending in white at the very top. Because the same scalar drives both palette position and brightness, low-value areas are simultaneously dark-colored and dim.

## UI controls (all sliders)
- "Angular density": integer, from two up to somewhere under twenty. Number of flame tendrils around the eye. Must be an integer because it doubles as the angular noise wrap period.
- "Radial density": small fractional range up to about two. Stretches or compresses the noise along the radius (long streaks vs fine grain).
- "Dilation" (misspelled in the original UI): pupil size, from noticeably smaller than default to nearly double.
- "Slitness": from one (round pupil) up to a several-fold horizontal compression (narrow slit).

## Clever bits worth preserving
- Setting the noise field's wrap period on the angular axis to exactly the (integer) angular density makes the noise tile seamlessly around the full circle — no visible seam where the angle wraps.
- Animating along the noise wrap period with slow clocks buys minutes of non-repeating animation from a compact noise domain.
- The single scalar driving both palette lookup and brightness keeps the fire grading coherent for free.
