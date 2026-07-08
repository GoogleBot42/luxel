# sinpulse 3D
kind: 1D+2D+3D
sensors: no

This is a short, classic "plasma" pattern — near-trivial, so this spec is brief.

## What it looks like

Smooth rainbow plasma: soft blobs of full-spectrum color drift and interfere across the display while the spatial scale of the blobs slowly "breathes" between coarse (one or two blobs across the display) and fine (several). Dark regions separate bright saturated regions; because brightness is aggressively gamma-shaped, most of the display is dim with glowing colorful crests. Motion is continuous and fluid: the two drift phases each cycle in a handful of seconds, and the zoom breathes over roughly ten seconds, so it reads as an unhurried, hypnotic drift with a slow scale pulse.

## Per-frame state

Three time-derived scalars, recomputed each frame (no persistent state):
- Two phase angles, each a sawtooth over a full circle; one cycles somewhat faster than the other (their periods differ by very roughly a factor of two), so their combination never visibly repeats.
- A zoom factor: a triangle wave over a several-second period, mapped so it swings between roughly one and roughly four.

## Per-pixel work

The real renderer is the 3D one. For each pixel at world coordinates (x, y, z):

- Compute a field value: the sum of one plus a sine of (x times zoom plus the first phase), plus a cosine of (y times zoom plus the second phase), plus a sine of (z times zoom plus the first phase minus the second phase), all halved. This lands roughly in a 0..2-ish range centered near 1, and serves double duty.
- Hue = that field value directly (wrapping through the rainbow).
- Brightness = the field value cubed, then halved — the cubing crushes the low end to black and makes the bright crests pop.
- Saturation = full.

## Renderer fallbacks

- The 1D renderer calls the 3D one with x = the pixel's normalized position along the strip and y = z = 0.
- The 2D renderer calls the 3D one with the mapped x, y and z = 0.

So the same field is simply sliced at lower dimensionality. No pixel-count hardcoding beyond the normalized 1D projection, which is the standard idiom.

## Controls / colors

No UI controls. Colors are the full hue wheel; no palette.
