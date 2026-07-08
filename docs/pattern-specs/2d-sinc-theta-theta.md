# 2D sinc(theta)/theta
kind: 2D
sensors: no

## What it looks like
Three sets of concentric ripple rings — one pure red, one pure green, one pure blue — expand continuously outward across a 2D panel, each from its own slowly wandering center point. Where rings from different channels cross they blend additively into yellows, cyans, magentas, and white, producing a shimmering moiré/interference look, like three colored stones dropped repeatedly into the same pond. The ring centers drift around (and frequently off) the panel, so waves often sweep in from off-screen. The whole thing is bright, saturated, and hypnotic. At defaults the motion is slow and stately — a ring-expansion cycle takes on the order of a minute or two — but a speed slider can push it to a few seconds per cycle.

## Algorithm
Layout: requires a 2D map with unit-square normalized coordinates. Pixel-count independent except that two sliders scale their range by the square root of the pixel count (a proxy for the panel's edge length) — a reasonable heuristic that carries over as-is.

Per frame, advance three phase angles, each a repeating time ramp scaled into radians and multiplied by a user "scale" factor:
- a main ring phase (the ripples' outward motion), period set by the speed slider;
- a faster phase used to move the ripple centers along one axis — its rate is the main period divided by one of the "size" sliders' values;
- a third phase, at a fixed small integer fraction of the main period, moving the centers along the other axis. (The second "size" slider is defined and exposed but never actually referenced in the animation — a dormant control in the original. The obvious fix is to wire it into this third phase the same way the first size slider drives the second phase.)

Per pixel, for each of the three color channels independently:
1. Compute the channel's ripple center: each coordinate is the cosine of one of the two center-motion phases, with a different fixed phase offset per channel (offsets of a quarter to a half turn), so the three centers trace related but distinct Lissajous-like wandering paths. Since cosine spans a range about twice the panel width, centers spend much of their time outside the visible area.
2. Compute the pixel's Euclidean distance to that center.
3. Ring field: take the cosine of (distance scaled by a full turn, minus the main ring phase, plus the channel's own phase offset), subtract from one so it oscillates between zero and about two... specifically the value is one minus (that cosine divided by the distance). Dividing the oscillating term by distance is the sinc-flavored trick: near the center the oscillation is huge (blowing out to a hot, saturated core) and it decays hyperbolically with distance, so rings fade gracefully as they travel outward.
4. Raise the result to a small integer contrast exponent ("gamma"), which thins the rings and deepens the dark gaps between them.
5. Emit the three channel values directly as red, green, and blue; values routinely exceed the displayable maximum near the centers and are clamped by the output, giving broad saturated cores.

No state between frames beyond the time ramps. No randomness. Note the ring field can go slightly negative near a center between crests; with an integer exponent this is well-defined and simply clamps to black on output — keep the exponent integral.

## Colors
Fixed additive RGB: pure red, green, and blue ring systems whose overlaps make the full secondary palette and white. Background between rings is black. Not user-recolorable.

## Controls (all sliders)
- "Speed" — period of the ring animation, from a few seconds up to a couple of minutes (inverted: right is faster).
- "Scale" — spatial density of the rings and the rate of phase advance together, from about a quarter of natural density up to several times denser (more, tighter rings and busier motion).
- "Size B" — divides the main period to set how fast the centers wander along one axis; range tied to the panel's edge length in pixels. Right is slower wandering.
- "Size C" — intended as the analogous control for the other axis, but inert in the original (see fix above).
- "Gamma" — integer ring-contrast exponent, from unity (soft, wide, washed rings) up to around a half dozen (thin crisp rings, mostly-black gaps).

## Non-obvious notes
- The whole "sinc" character comes from dividing the ring oscillation by the radius before exponentiation — without that division you get uniform concentric rings; with it you get the bright singular core and 1/r decay envelope that make it read as a ripple/impulse.
- The three channels use the same machinery differing only in fixed phase offsets (applied to both the center path and the ring phase), which is what keeps the three ring systems related-but-desynchronized rather than either identical or unrelated.
