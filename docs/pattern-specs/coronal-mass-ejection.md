# Coronal Mass Ejection
kind: 2D
sensors: no

## What it looks like
A blazing star sits at the center of the panel: a small, always-white-hot core surrounded by ragged plasma flares that lick outward in all directions. The flares are discrete tongues — not a smooth glow — that writhe, stretch, and occasionally hurl detached "hot bits" toward the edges before they fade, very much like solar prominences and coronal mass ejections seen in telescope footage. Near the core everything is desaturated white; farther out the flares take on strong color. The overall hue is not fixed: it drifts slowly around the entire color wheel over ten-to-fifteen seconds, so the star gradually shifts from one plasma color to the next. Motion feel: the turbulence churns on a slow, organic timescale (several seconds), with flares predominantly streaming radially outward.

## Algorithm
Stateless between frames apart from three clock phases; all the structure comes from evaluating a noise field per pixel.

### Frame-setup step
Three free-running sawtooth phases are computed each frame:
- a slow hue phase cycling over roughly ten-to-fifteen seconds;
- two independent noise-scroll phases with long, deliberately different periods (each on the order of ten minutes), scaled up by a large factor before use so the noise field drifts continuously and the animation never visibly loops.

One-time setup: the coordinate system is translated so the panel center is the origin, and the noise generator's tiling/wrap intervals are configured — crucially, a small wrap in the first noise dimension chosen so the field tiles seamlessly around the circle (see the polar trick below), with very large wraps in the other two dimensions.

### Per-pixel render
1. Polar conversion: the centered Cartesian coordinates are converted so that the first coordinate becomes the angle around the center and the second becomes the radius. Everything after this operates in angle/radius space, which is why the effect is naturally radial.
2. Turbulent noise: sample multi-octave "turbulence" Perlin-style noise (around three octaves, moderate lacunarity-and-gain settings) at (angle, radius minus one scroll phase, second scroll phase), and invert it. Subtracting a moving phase from the radius coordinate makes the noise field stream radially outward over time; the third dimension scrolling on its own clock makes the shapes evolve as they move.
3. Flare extraction: push the inverted noise through a smooth threshold (a smoothstep ramp over roughly the top third of the value range) so only the strongest noise ridges survive as distinct flares with soft edges — this is what turns a continuous field into separate tongues and detached blobs.
4. Core: compute a second brightness term that is essentially "one minus (radius times the noise value, offset by a small fraction of the core size, divided by the core size)". Because it uses radius multiplied by the noise, the core's boundary is itself noisy: near the center this term saturates to full brightness (guaranteeing a permanently white-hot core about a tenth of the panel's half-width in size), and where a strong flare overlaps the core region it extends the bright zone outward, letting flares appear to erupt from the surface.
5. The pixel brightness is the maximum of the flare term and the core term, then cubed to deepen contrast (dim structure fades to black, highlights pop).
6. Color mapping (the clever bit):
   - Hue = the slow global hue phase, minus a small fraction of the brightness — so the brightest plasma is hue-shifted slightly behind the base color, giving hot regions a subtly different tint rather than a flat single hue.
   - Saturation = radius scaled by a large factor (several times), minus the brightness. At small radii this is at or below zero → the core renders pure white regardless of hue. Saturation grows quickly with distance from center, and high brightness suppresses it — so the very hottest ejected blobs stay whitish even far from the core, reading as "super-hot flare bits".
   - Value = the cubed brightness.

## Colors
No fixed palette. At any instant: a white core, whitish-hot flare interiors, and flare bodies/edges in one saturated hue that slowly circles the entire wheel over ten-to-fifteen seconds (fiery orange one moment, green, blue, violet later). The gradient at any moment reads as "white → pale tint → deep saturated color → black" moving outward along a flare.

## UI controls
None. The core size is an internal constant (with the small offset derived from it, about a quarter of it); the obvious enhancements would be sliders for core size and hue-cycle speed, but the original exposes nothing.

## Timing
Hue cycle: ten-to-fifteen seconds per full revolution. Flare churn: continuous, organic, no visible loop (multi-minute noise periods, deliberately different so they never sync). Outward streaming of flares is stately — a given tongue evolves over a few seconds.

## Layout assumptions
Requires a 2D map with normalized coordinates; assumes the interesting area is centered (it recenters the unit square itself). Works on any resolution; on non-square or ring layouts it degrades gracefully since everything is polar. No pixel-count hardcoding.

## Non-obvious notes
- The angle-as-noise-axis trick: by converting to polar coordinates and then configuring the noise generator's wrap interval in the angle dimension to match one full turn, the turbulence field is seamless around the circle — no visible seam where the angle wraps.
- Radial motion is achieved not by moving geometry but by scrolling the noise-lookup coordinate along the radius axis.
- The single "max(flare, noisy-core)" composition plus the radius-driven saturation formula does all the work of core, corona, and ejected-blob rendering in one expression per pixel — there is no particle system, despite the effect looking like one.
