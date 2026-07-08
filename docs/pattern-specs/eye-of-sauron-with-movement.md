# Eye of Sauron with movement
kind: 2D
sensors: no

## What it looks like

A fiery, lidless eye filling a 2D matrix, styled after the Eye of Sauron from the film adaptations. Wispy flame tendrils radiate outward from the center in a ring of fire, continuously streaming outward while their shapes slowly morph (the morph cycle is long — many minutes before it repeats). The fire is rendered through a heat palette: black through deep red, up through yellow, to near-white at the hottest points. The overall glow fades out toward the edges in an oval (taller than wide) shape. At the center sits a dark vertical slit pupil. The eye "looks around": the pupil (and the angular origin of the tendrils) darts to a new random gaze point at irregular intervals — sometimes twitching rapidly, sometimes holding still for a second or two — and each move is eased rather than snapped, so it feels alive and menacing.

## Colors

A single fixed palette used for everything: black at the bottom, reaching a strong pure red about a fifth of the way up, blending through orange to yellow around four-fifths, and topping out at white. The palette index and the brightness are driven by the same intensity value, so hotter regions are simultaneously brighter and whiter.

## State kept between frames

- Current eased gaze position (a 2D offset) and the target gaze position it is easing toward.
- A countdown timer until the next gaze retarget, and a memory of how long the previous dwell was.
- Two slow clocks read fresh each frame: a very slow "morph" phase (full cycle over several minutes) and a faster "radial flow" phase (full cycle over a couple of minutes), both scaled to span the noise field's natural wrap length so the animation loops seamlessly.

## Per-frame work

1. Recompute the two noise-animation phases from the clock. The noise field used repeats over a fixed span; by scaling a long sawtooth clock to exactly that span, the animation never shows a seam — the trick buys many minutes of unique, perfectly looping motion.
2. Set up the coordinate transform: recenter the 0..1 map on the origin, then scale up by about three in x and noticeably more (roughly forty percent more) in y. The vertical stretch is what makes the fade region an upright oval.
3. Configure the noise field to wrap in its first dimension at exactly the angular-density setting, so the tendril pattern tiles seamlessly around the full circle.
4. Ease the gaze: each frame the current position moves a large fraction (roughly forty percent) of the remaining way toward the target — an exponential ease that starts fast and settles.
5. Decrement the retarget timer by elapsed time. When it expires: pick a new random dwell between a few tens of milliseconds and about two seconds; pick a new random target offset uniformly in a small square around center, with its scale proportional to how long the previous dwell was (a long stare earns a big jump; rapid twitches stay near where the eye already looks); remember the new dwell for next time.

## Per-pixel render (2D, in transformed coordinates)

1. Convert to polar: radius = distance from center; angle = arctangent of the position offset by the gaze position, normalized to a 0..1 turn. Only the angle and the pupil use the gaze offset; the radius does not, so the flame ring stays centered while the tendrils and pupil swing.
2. Sample ridge-style fractal noise (the "ridged" variant that folds noise into sharp creases — this is what makes the wispy licking-flame filaments) in three dimensions: angle scaled by the angular density, radius scaled by the radial density minus the radial-flow phase (streaming the tendrils outward), and the slow morph phase as the third axis. A few octaves with standard frequency doubling and amplitude halving, with a ridge offset slightly above one.
3. Oval edge fade: compute how far inside the outer boundary the pixel is (clamped to a unit range), and attenuate the intensity by the square of that distance, so the fade is gentle inside and sharp right at the rim.
4. Pupil: subtract from the intensity a cone-shaped darkness centered on the gaze point — the pupil's horizontal axis is compressed by the slit factor so the dark region is a narrow vertical slit; the dilation setting is the cone's height/size. Subtracting (rather than masking) lets the strongest flame licks encroach on the slit edges.
5. Clamp the intensity so it never exceeds the top of the palette (preventing hot spots from wrapping back around to black), then emit it as both palette lookup and brightness.

## UI controls (four sliders)

- Angular density: how many flame tendrils fit around the circle — from very few and broad to a dozen-plus fine wisps (steps in whole numbers so the wrap stays seamless).
- Radial density: how stretched the tendrils are along the radius — low values give long streaming licks, high values give busy fine-grained flames.
- Dilation: pupil size, from small to large.
- Slitness: how narrow the pupil slit is, from a round-ish pupil to a razor-thin vertical slit.

## Non-obvious tricks

- Seamless angular tiling of the noise via its wrap configuration, keyed to the angular-density slider, is essential; without it there would be a visible seam where the angle wraps.
- Gaze-jump distance proportional to previous dwell time produces convincingly organic saccades: micro-twitches while "focused," big sweeps after a long stare.
- The original leaves an unused exported debug value and a commented-out whole-eye translation; neither affects behavior and neither should be reimplemented.
- Ridge noise (not plain noise) is the key to the flame look.
