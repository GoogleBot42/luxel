# StarGen polar 2D
kind: 1D+2D+3D (designed for a polar-mapped 2D disc; 3D is the primary entry point, 2D takes an equatorial slice, 1D is a degenerate radial fallback)
sensors: no

## Coordinate convention (critical)
This pattern assumes the pixel map is stored in **spherical/polar coordinates, each normalized to the 0..1 range**, not cartesian:

- first coordinate = radius from the center (0 = center, 1 = rim),
- second coordinate = angle around the vertical axis (0..1 = one full turn),
- third coordinate = azimuth from the pole (physics convention; the value at the halfway point of the range lies on the equatorial plane).

The 2D renderer simply calls the 3D one with the azimuth pinned to the equatorial midpoint. The 1D renderer treats pixel position along the strip as radius with angle zero. The intended target is a circular LED disc of a couple hundred pixels with a polar map; the pattern itself does not hardcode a pixel count (except that one mode allocates a per-pixel working buffer sized from the runtime pixel count, which is fine).

## Overall behavior
This is a **playlist of a dozen distinct polar "star/figure" animations** that auto-advances, spending a fixed dwell time in each mode, with a distinctive **stochastic "shimmer" crossfade** between consecutive modes. Most modes draw a mathematically defined closed curve in polar space (star polygons, ellipses, a heart, etc.) as a bright anti-aliased line; several are pure white-on-black, others are tinted.

### Playlist / crossfade machinery
- A master clock runs the full playlist cycle: dwell time per mode times the number of modes. The current mode index is the integer part of progress through the playlist.
- Each mode has an optional per-frame setup function and a per-pixel draw function. Every frame, the current mode's setup runs; when inside a crossfade window, the *next* mode's setup also runs so both are live.
- Crossfade: the last small fraction of each mode's dwell (a few seconds' worth, scaled so it stays a constant absolute duration regardless of dwell length) is a transition window. During it, **each pixel independently and randomly chooses per frame** whether to be drawn by the outgoing or the incoming mode. The probability of choosing the incoming mode eases from 0 to 1 across the window (shaped by a smooth sinusoidal easing, not linear). This produces a sparkling, dissolving "shimmer" between scenes rather than an alpha blend — and it is cheap, since each pixel only ever runs one renderer.

### Global UI controls (sliders)
1. **Mode override** — quantized slider: at zero, run the auto-advancing playlist; otherwise pin the display to one selected mode.
2. **Dwell time per mode** — scales how long each mode lasts, from nearly instant up to about a minute.
3. **Rotation offset** — adds a constant to the angular coordinate so the whole display can be rotated to match the physical installation's orientation.

### Line-drawing helper (used by nearly every mode)
A "proximity" function compares two scalars and returns full brightness when equal, falling off to zero once they differ by more than a half-width, with the falloff squared for a gamma-corrected soft edge. Default half-width is roughly an eighth of a unit (make it a tunable; wider suits low pixel counts). A wrapped variant does the same for angular quantities using a triangle-wave distance so values just below 1 and just above 0 read as close. Each mode draws its figure by asking "is this pixel's radius near the curve's radius at this pixel's angle?"

Modes use a small internal utility that makes slow sinusoidal oscillators (0..1) with a chosen period in seconds; periods below are approximate.

## The twelve modes, in playlist order

1. **Orbits / ellipse** — an ellipse whose two axis lengths breathe independently over several-second periods, rotating around the center at a slowly varying rate; occasionally the angle is skewed proportionally to radius, smearing it into a galaxy-like spiral hint. Colored a warm golden-orange (hue drifts briefly only at one extreme of an oscillator), fairly high saturation that dips slightly when the ellipse is nearly circular. The exact center pixel (radius exactly zero) is painted a separate half-bright color whose hue sweeps slowly — a little colorful heart at the center.
2. **Six-lobed sinus star** — a snowflake-like figure: target radius is a large slowly breathing base plus a cosine ripple with six lobes around the angle, the lobes rotating over a few seconds. Pure white, moderately wide line.
3. **Sinus shimmer** — same construction as the previous mode but with the angular frequency several times higher (lobes many times finer, an accidental aliasing effect) and the base radius fixed near the rim; on a real disc it reads as snowy sparkling texture rather than a discernible figure. Pure white.
4. **Star of David (kaleidoscopic)** — the angle domain is cut into twelve alternating sectors; alternate sectors mirror the line equation of a straight chord (the polar equation of a line: some scale divided by the cosine of the offset angle), and the chord scale breathes with one slow oscillator while the rotation drifts with another. The result flickers through abstract kaleidoscopic states that periodically resolve into a clean six-pointed star. Pure white.
5. **Star over Bethlehem** — eight thin radial rays (angular proximity test on the angle folded eight ways), whose length/spread is shaped by radius raised to a slowly breathing power. Warm coloring: white-hot near the center (saturation grows steeply with radius) shading to orange further out, with hue slightly warmer toward the rim. Every other set of rays (the diagonal ones) fades in and out over a few seconds via a triangle-wave mask, so it alternates between a four-ray and eight-ray star.
6. **Pentagram** — the classic five-pointed star polygon drawn with the standard polar star-polygon equation (reciprocal cosine of a scaled arccos-of-cosine of five times the angle), rotating steadily over the better part of a minute. Its overall scale swells and shrinks over many minutes via a damped-sinc-like function of a very slow triangle LFO — long stretches of gentle size drift punctuated by big swings. Line thickness is tied to the scale and additionally wanders randomly within bounds (a tiny random step each frame, clamped), so the stroke weight organically thickens and thins. Pure white.
7. **Decagram** — a ten-pointed star polygon via the same star-polygon equation, quite thick stroke, rotating extremely slowly (period of several minutes). The central area (inner third or so of the radius) is filled solid. Pure white.
8. **Hexagram** — six-pointed star polygon, moderate stroke, rotating slowly (tens of seconds). Pure white.
9. **Heart** — a heart curve (a known polar heart equation built from sine, and a square-root-of-absolute-cosine term) whose scale pulses with a slow oscillator; drawn in red whose saturation follows the pulse so it whitens as it shrinks. The angular origin is shifted a half-turn so the heart points the right way.
10. **Bird flap** — a V-shaped pair of wings: a chord-like reciprocal-cosine curve of the absolute angular distance from a fixed heading, with a "dihedral angle" term animated by the absolute value of a cosine over a few seconds — a convincing wingbeat. Wing bend also breathes on its own oscillator. Line width grows with radius so wingtips are softer/broader. Colored a warm ember orange whose brightness "breathes" on yet another oscillator.
11. **Rainbow spirals** — the radius is scaled by a slow LFO and wrapped modulo another LFO, then compared against the angle scaled by a third LFO — producing multi-armed spirals that wind, unwind and change arm count over tens of seconds. Hue is the sum of radius and angle passed through a perceptual-rainbow correction (a sine-based reshaping of hue that de-emphasizes the overlong green band), giving full vivid rainbows along the arms.
12. **Snowglobe** — the odd one out: it ignores the polar coordinates entirely and runs a 1D "sparks" simulation over the pixel index array (a derivative of the classic sparks pattern). About twenty particles are born at random strip positions with random speeds in either direction, decelerate under friction (friction scaled inversely with pixel count), get culled and respawned when they stop or run off either end, and deposit their speed as energy into a per-pixel accumulation buffer each frame (buffer cleared every frame; time step deliberately slowed by an order of magnitude). Rendering maps energy to brightness (squared) and to a whitening of an icy blue — faint traces are deep blue, hot ones are white. On a polar disc the pixel order makes this read as concentric drifting snow sparkle.

## Cleverness worth preserving
- The stochastic per-pixel dither crossfade (cheap, and visually a "shimmer").
- The unified "draw a polar curve as a proximity test on radius" approach with squared-falloff anti-aliasing.
- The star-polygon family generated by one equation with the point count and a pinch factor as the only differences.
- The perceptual rainbow correction used in the spiral mode.
