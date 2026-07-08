# Coral Plasma
kind: 3D (implements only a 3D renderer; needs a mapped installation with x/y/z coordinates)
sensors: no

## Visual behavior
An organic, slowly tumbling plasma of glowing filaments. Bright vein- or coral-branch-like ridges of light wind through the mapped volume against a dark background; the whole structure slowly rotates on all three axes at once while gently zooming in and out ("breathing"), so the filaments appear to writhe and morph. Hues shimmer across the filaments in bands and the overall color mood drifts over tens of seconds. Ridge cores wash out toward pale/white; dim regions fall to black. Everything is smooth and slow — no beats or flashes; a full character change takes on the order of half a minute to a minute.

## Algorithm
All animation is done by moving the *sampling frame*, not by recomputing any field:

Per frame, build a 3D coordinate transform applied to every pixel's mapped position:
1. Translate so the unit cube of pixel coordinates is centered on the origin.
2. Uniform scale that breathes with a triangle wave between a base zoom and about double it, over a period of roughly a minute.
3. Rotate about each of the three axes. Each rotation angle is a full turn multiplied by a smooth sine-like oscillation, each axis on its own period (each somewhere around half a minute, deliberately slightly different so they never sync). Because the angles oscillate rather than increase, the volume rocks through full revolutions and back rather than spinning uniformly.

Also advance a slow phase (cycle around half a minute) used for global hue drift.

Per pixel (given transformed x, y, z):
1. Sample ridged fractal noise ("perlin ridge" style) at the 3D point: several octaves (about five), with a lacunarity a bit above one and a gain around three quarters — i.e., a dense, fine-veined ridge field. Square the result to sharpen ridges and deepen valleys.
2. Hue: a triangle wave of (ridge value + sum of the three coordinates) contributes spatial hue banding spanning roughly a third of the wheel; the slow global phase adds a further drift of about a fifth of the wheel; plus a constant offset. Net effect: hues live in the middle of the spectrum (greens/teals through blues into violets), banded along the filaments and slowly cycling.
3. Saturation: full in dark areas, reduced by up to half where the ridge value is strongest, so filament cores pale toward white.
4. Brightness: a smoothstep of the ridge value with a low threshold (values below roughly a tenth are black, easing up to full), then squared — this is what isolates the glowing filaments and keeps the background truly dark.

## State
None beyond the current transform and drift phase; the noise field is stateless and re-sampled each frame. Works at any pixel count; requires a 3D pixel map (an obvious extension is adding 2D/1D wrappers that fabricate missing coordinates).

## UI controls
None.

## Non-obvious techniques
- Ridged (inverted-crease) fractal noise, squared, is what produces branch/vein shapes instead of ordinary blobby plasma.
- Animating purely via a per-frame affine transform (translate–scale–rotate of the sample space) gives rich 3-axis motion for constant per-pixel cost.
- Feeding the noise value itself into the hue (alongside position) makes color follow the filament structure rather than sitting in independent stripes.
