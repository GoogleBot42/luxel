# Dire Spider 2D
kind: 2D
sensors: no

A Halloween-flavored 2D matrix pattern: a glowing orange spider crawls repeatedly across the display while a swirling toxic-green mist radiates around it. Requires a mapped 2D display.

## Visual behavior
A spider silhouette — eight legs radiating from a central point plus a small round abdomen — crawls across the panel from one edge to the other, taking on the order of ten seconds per pass. Its legs scissor back and forth a couple of times per second as it walks, and the whole body sways slightly side-to-side, which sells the crawl. When it finishes a pass it re-enters from a new, randomly chosen direction each time (the author notes the random re-entry angle makes it much creepier). Around and behind the spider, wispy green "poison spray" tendrils swirl outward in a spiral, morphing continuously. The background is black by default (a slider can lift it to a dim green haze).

## Algorithm

### Cleverness up front
Two tricks carry this pattern:
1. Symmetry: only two leg segments are actually defined. Before testing them, the pixel's coordinates are mirrored into the first quadrant (absolute value of both axes), so each drawn leg appears four times — eight legs from two definitions.
2. All motion is done in the coordinate transform, not the drawing: each frame the pattern resets the map transform, recenters the origin, rotates by the current crawl direction, and translates along the crawl axis. The spider is always drawn at the origin; the world moves under it.

### Per-frame state and work
- A wall-clock accumulator in seconds (wrapped after about an hour) drives everything not tied to the built-in clocks.
- Crawl position: a sawtooth of the wall clock, period on the order of ten seconds, mapped so the spider traverses from about one-and-a-half display-widths on one side to the same distance on the other (fully off-screen at both ends).
- Crawl direction: a random full-circle angle, re-rolled exactly when the sawtooth wraps (detected by comparing this frame's crawl position with last frame's).
- Leg gait: a small oscillation angle, a sine of the wall clock at a couple of cycles per second, amplitude around a tenth of a radian. The two stored leg segments are each rotated about the origin by this angle — one positively, the other negatively and slightly less — using an ordinary 2D rotation. A fraction of the same oscillation is also fed into the transform's lateral translation so the body sways as it walks.
- Two noise clocks: a slow one (several seconds) that morphs the mist shape, and a faster one (a few seconds) that scrolls the mist radially outward. Both are sawtooth clocks scaled up into noise-space units.

### Per-pixel work (after the transform, coordinates are spider-centric)
- Compute polar-ish quantities: distance from origin; that distance skewed slightly by a fraction of the x coordinate (this makes the legs on one side effectively longer — front legs longer than back); the angle around the origin normalized to one turn; and distance from a second point offset a little along x (the abdomen center).
- Mist: sample ridged multi-octave noise (a few octaves) in a cylindrical space — angle scaled by an angular density (the noise wrap is configured to that same density so the seam at the angle wraparound is invisible), radius minus the radial-scroll clock, and the morph clock as the third axis. Raise the sample to the fourth power to sharpen it into wisps, and attenuate it linearly to zero at the display edge so the mist hugs the spider.
- Legs: for each of the two (rotated) leg segments, compute a brightness: zero if the pixel's radius exceeds the leg length or the pixel is in the opposite half-plane from the leg (dot-product sign test); otherwise one minus (perpendicular distance from the pixel to the infinite line through the origin and the leg endpoint, divided by the leg width). Take the max over both legs, then pass it through a smooth ease (smoothstep over roughly the upper half of its range) to give the legs soft edges.
- Abdomen: a filled disc — brightness proportional to how far inside the abdomen radius the pixel is, scaled up several-fold so it saturates quickly to full.
- Compositing: body brightness is the max of legs and abdomen. If the mist sample beats the body brightness at this pixel, draw mist: green hue nudged slightly by mist intensity, saturation reduced a bit at high intensity (hot cores go whitish-green), brightness = mist value floored at the background level. Otherwise draw the spider: a warm reddish-orange base hue over the abdomen, drifting warmer/yellower with distance from the abdomen along the legs, full saturation.

## Controls
- Slider, "leg width": scales the leg line half-width between roughly a tenth and a fifth of display width.
- Slider, "background level": lifts the minimum mist brightness from black up to a moderate dim glow.
- Implementation note: in the original, the background level's declared default is orphaned by a variable-naming slip, so the background is effectively off until the slider is first moved. Just give the background control a sensible near-zero default.

## Colors
Spider: deep reddish-orange at the abdomen shading toward amber/orange along the legs, fully saturated. Mist: green, desaturating toward whitish-green at its brightest. Background: black (or dim green if raised).

## Timing
Crawl pass: order of ten seconds. Leg scissoring: a couple of oscillations per second. Mist morph: several seconds; radial mist drift: a few seconds.

## Layout assumptions
Assumes a 2D mapped display with normalized coordinates; no pixel-count hardcoding. Looks best on a roughly square matrix since the spider is drawn in normalized units.
