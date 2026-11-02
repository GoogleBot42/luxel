# Poincaré Sphere

Idea spec written from Patternflow preset preset_0715.h (CC BY-SA 4.0 per its SPDX header, by Seunghun Lee; the brief said MIT); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A glowing wireframe grid seen through a strong spherical lens, on a black background. Grid lines are thin, with white-hot cores and a coloured glow on either side; the glow colour is a two-colour blend that slides from cyan-blue to violet-purple as the tone knob turns. In the middle of the panel the grid is fine and regular; moving outward the cells stretch and swell as if the grid were wrapped over a ball, up to a ring-shaped "horizon" where the lines spread out and thin; past that ring the grid reappears mirrored and stretched, like the inside of a bubble or a reflection in a fisheye. The vertical lines scroll one way and the horizontal lines scroll the other way at a different rate, so the grid slides diagonally under the lens and every crossing point swims through the distortion. On the wide 128x64 panel the lens is round (scaled by the short axis), so the extra width shows more of the mirrored outer region at the left and right. Hypnotic and clean; it looks like a retro vector-graphics demo.

## The idea

Push an infinite, scrolling rectangular grid through a radial lens mapping that compresses distance from the centre; at high curvature the mapping folds back on itself, producing a sphere-like rim and a mirrored outer zone. Grid lines come from the product of two sines being near zero.

## Algorithm in words

- State: a time accumulator (wrapped).
- Per frame: line count from the density control; curvature from the curvature control.
- Per pixel: centred coordinates normalised by the panel's half-height (so the lens is circular). Squared radius r². Lens scale = 1 / (1 + r² × curvature); lens coordinates = centred coordinates × that scale.
- Line field: sine of (lens x × line count × a full turn + time) and sine of (lens y × line count × a full turn − time × a smaller factor); multiply their absolute values. Near zero means the pixel is on a vertical or horizontal grid line.
- Below a small threshold: intensity = how close to zero (linear ramp). Colour = blue at full intensity, plus red scaled by the tone control and green scaled by its complement, all × intensity. Very close to zero → pure white. Otherwise black.

## Knobs

- K1 Color Tone — slides the line glow from cyan-blue to violet.
- K2 Speed — scroll speed of the grid under the lens.
- K3 Grid Density — how many grid lines cross the lens.
- K4 Curvature — lens strength, from almost flat (plain scrolling grid) to a strong bulge with a folded, mirrored rim.
- Audio: none directly; Curvature pulsing to a beat would read as the lens "breathing".

## Making it better in Luxel

- Animate the lens: let the lens centre wander slowly (simplex drift) and let curvature breathe with `beatSin`, so the bubble rolls around the panel.
- Use proper anti-aliased lines: compute distance to the nearest line in lens space divided by the local lens derivative, so line width stays constant on screen and the rim does not get blobby. Colour with OKLCH and add `setGlow` for the vector-monitor bloom instead of the hard white core.
- Offer other lens maps as a mode (barrel, pincushion, true stereographic sphere with shading, a hyperbolic disc tiling — which is what the name "Poincaré" really evokes) and other lattices (triangular, polar).
- Cost: radius and lens scale are per-pixel constants unless the lens moves — precompute them into arrays; per frame it is two sines per pixel. On a 128x128 chain a half-res canvas plus `fillCanvas` is fine since the look is soft-edged.
- Strips: no (the effect is inherently 2D).

## Payoff

3/5 — elegant retro vector-demo look; with a wandering lens and constant-width lines it would be a 4.
