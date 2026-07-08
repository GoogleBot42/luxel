# Spinwheel 2D
kind: 2D
sensors: no

## What it looks like

A colorful, flower-like radial spinner on a 2D matrix. Bright points of light are arranged like petals in concentric rings around the center; the whole arrangement rotates, but the rotation speed continuously swells and ebbs (it never spins at a constant rate — it accelerates, slows, and drifts, giving an organic "pinwheel caught in gusts" feel). Simultaneously the rings breathe radially in and out. Hues sweep across the display over time and also vary with position, so different petals show different colors; the bright cores of the petals wash out toward white/desaturated while their fringes are richly saturated. The background between petals is dark. The author's own comment describes it as simple functions combined to produce complex, unpredictable motion — the visual result is deliberately hard to predict, mandala-like.

## State kept between frames

Only two per-frame scalars, both recomputed each frame from clock time; no per-pixel or accumulated state.

- Angular driver: the product of a sawtooth ramping over a couple of seconds and a triangle wave with a several-second period, scaled by pi and negated. Because a modulating triangle wave multiplies a repeating ramp, the effective rotation rate oscillates — this is what makes the spin speed up, slow down, and feel erratic.
- Radial driver: a triangle wave with a period of a few seconds, scaled by a moderate speed constant (several units), which pushes the ring pattern in and out radially.

## Per-pixel render (2D)

1. Re-center coordinates so the origin is the middle of the 0..1 map.
2. Convert to polar: an angle term = arctangent of the position plus the angular driver amplified by a large factor (order of tens), and a radius term = Euclidean distance from center plus the radial driver.
3. Ring index: take the integer part of the radius term and divide by a full circle constant; this gives each concentric band a small per-ring weight. If the weight is exactly zero (the innermost band), substitute a fixed constant of roughly golden-ratio size so the center band still lights.
4. Take the fractional parts of both the angle term and the radius term. Together these form local coordinates within a "cell" of the polar grid.
5. Compute an intensity as a small constant divided by the sum of squares of those two fractional parts, scaled down and multiplied by the ring weight. This is an inverse-square hotspot: it blows up near the corner of each polar cell and falls off quickly, producing one bright petal point per cell. (Values can exceed the displayable range near the singularity; the pixel API clamps.)
6. Color: hue = the angular driver plus the product of the centered x and y coordinates plus the intensity (so hue drifts with time, varies across the quadrant, and shifts inside each petal); saturation = one minus the intensity (hot cores desaturate to white); brightness = the intensity itself.

## Layout assumptions

Pure world-coordinate 2D pattern using the 0..1 mapped coordinates; works on any mapped 2D layout, no pixel-count hardcoding. No 1D fallback renderer is provided — adding one (e.g. projecting along one axis) would be an obvious extension.

## Controls

None. The internal speed constant would be the natural candidate for a slider.

## Non-obvious tricks

- The "petals" are not drawn explicitly; they emerge from an inverse-square falloff over the fractional parts of jittered polar coordinates — essentially a polar grid of point lights.
- Multiplying two simple periodic time functions (ramp × triangle) yields the characteristic non-uniform, direction-ambiguous spin.
- Feeding the intensity into all three of hue, saturation, and value is what gives petals their white-hot cores with colored rims.
