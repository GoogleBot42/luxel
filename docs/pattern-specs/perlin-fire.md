# Perlin fire
kind: 2D
sensors: no

## What it looks like
A convincing fire effect on a 2D matrix: a hot column of flame centered horizontally, brightest and most saturated toward one vertical edge (the fire's base) and thinning to black toward the other, with flame shapes continuously rising and slowly morphing. A mode selector switches among four noise flavors that change the fire's character: (1) soft billowing clouds of flame, (2) sharp branching "tendrils" from ridged fractal noise, (3) layered fractal noise that the author considers the best fire approximation, and (4) turbulence that reads as a blackened rolling fireball. Motion is a steady upward drift plus a much slower shape-shift.

## Algorithm
This is a demonstration of a family of smooth-noise builtins. State between frames: none beyond the ambient clock; each frame recomputes two phase offsets and a coordinate transform.

Per frame:
- Pick the active noise function from a fixed list of four, indexed by the mode control:
  1. plain 3D smooth (Perlin-style) noise, rescaled from its signed range to zero-to-one;
  2. a ridged multi-fractal variant (a few octaves, moderate gain, lacunarity slightly above two-ish — qualitatively: a handful of octaves with each octave roughly half the amplitude);
  3. fractal Brownian motion (a few octaves), rescaled from signed to zero-to-one;
  4. a fractal turbulence variant (a few octaves).
- Compute a vertical scroll offset and a morph offset. Key trick: the noise field in this engine repeats seamlessly with a fixed large period, so each offset is a sawtooth of time scaled to exactly that repeat period — the animation therefore loops with no visible seam and traverses lots of unique noise. The vertical scroll completes its full period in roughly a minute or two scaled by the rising-speed setting (visually the flames rise briskly, since one noise unit is a fraction of that period); the morph offset's full period is several times longer, scaled by the morph-speed setting, so shape change is gentle.
- Reset the coordinate transform, translate so the horizontal axis is centered on zero, then scale both axes up by the "fire scale" zoom factor.

Per pixel:
1. Sample the chosen noise at (transformed x, transformed y plus the scroll offset, morph offset) — the third noise dimension is time, morphing the flame.
2. Multiply by a horizontal envelope that peaks at the center column and falls off linearly toward the sides (roughly reaching zero just past the visible edges), making a hot central column.
3. Multiply by a vertical ramp proportional to the height coordinate, so intensity grows from nothing at one vertical extreme to full at the other — that bright extreme is the fire's base, and flames fade as they get farther from it.
4. Clamp to the top of the range so the palette does not wrap.
5. Draw using the palette indexed by this value, with the same value also used as brightness.

Randomness: none explicit — all variation comes from the deterministic noise field.

Layout assumptions: normalized 2D map coordinates. No pixel-count hardcoding; works at any matrix size. (Only render2D exists; on a 1D strip nothing renders — an implementer could optionally add a 1D fallback that treats index/count as one axis.)

## Colors
A classic blackbody-ish fire palette defined as a gradient with stops qualitatively: black at the bottom, reaching pure deep red early on (around the first fifth), ramping through orange to full yellow around the four-fifths point, and ending at white at the very top. Low noise values are dark embers, mid values orange-yellow flame, peaks white-hot.

## Timing
Flames visibly rise on a sub-second-to-few-seconds rhythm depending on the rising-speed control; the overall shape morphs over tens of seconds. Both loops are seamless (see the repeat-period trick above).

## UI controls
- **Slider** ("mode") with a numeric readout showing one through four: selects which of the four noise flavors is used.
- **Slider** ("scale"): zoom of the flame detail, from roughly life-size to about an order of magnitude zoomed, changing how large the flame structures appear.
- **Slider** ("rising speed"): how fast flames rise. Inverted (right = faster) and with a squared response curve, spanning roughly a 25-fold range.
- **Slider** ("morph speed"): how fast the flame shapes mutate. Same inverted, squared mapping and range.
