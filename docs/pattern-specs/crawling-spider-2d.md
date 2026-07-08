# Crawling Spider 2D
kind: 2D
sensors: no

## What it looks like
A red-orange spider — a small round body with eight thin radiating legs — crawls in a straight line across a black 2D display, taking on the order of ten seconds per pass. As it walks, its legs scissor rhythmically back and forth and its body sways slightly side to side, which sells the crawling motion. Each time it finishes a pass and re-enters, it approaches from a new random direction, which makes it feel unsettlingly alive. Requires a mapped 2D display.

## Algorithm
State between frames: a wall-clock accumulator (seconds, wrapped after a long period), the current random crawl angle, and the previous frame's crawl position (used to detect wraparound).

Per frame:
1. Crawl progress is the time accumulator taken modulo a period of several seconds (order of ten), normalized to a unit ramp. The spider's position along its travel line sweeps linearly from well off one edge to well off the opposite edge (the sweep spans about three display-widths, so there is off-screen dead time between passes).
2. A leg-swing angle oscillates as a fast sine of time (several swings per second, small amplitude — a tenth of a radian or so).
3. When the crawl position wraps (new pass detected by comparing to last frame's position), pick a new travel direction uniformly at random over the full circle.
4. Set up the engine's 2D coordinate transform for this frame: move the origin to the display center, rotate by the crawl angle, then translate along the travel axis by the crawl position, plus a small perpendicular offset proportional to the leg-swing angle (this is the body sway).
5. The spider has exactly two leg definitions, each a line segment anchored at the body origin, described by an endpoint direction and a length (one shorter/steeper, one longer/shallower). Each frame, rotate leg definition A by the leg-swing angle and leg definition B by slightly less than the opposite of that angle, storing the rotated copies. Counter-rotating the two legs produces the scissoring gait.

Per pixel (in the transformed, spider-centered coordinate frame):
1. Compute a radial distance from the origin, skewed by subtracting a small fraction of x — this makes legs on the spider's front side effectively longer than the back ones.
2. Compute a second radial distance measured from a point slightly offset from the origin along x — this is distance from the abdomen center.
3. Mirror the pixel into the first quadrant by taking absolute values of both coordinates. This is the key trick: only two legs are ever drawn, and the four-fold mirror symmetry replicates them into eight.
4. For each of the two (rotated) legs, compute a leg brightness: zero if the pixel's radial distance exceeds the leg's length or if the pixel direction points away from the leg's direction (dot-product sign test); otherwise one minus (perpendicular point-to-line distance divided by a line-width constant). Take the max of the two legs.
5. Pass the leg brightness through a smoothstep that clips away the dim fringe (thresholding roughly the lower half) so legs render as crisp thin lines.
6. Abdomen: brightness contribution equal to how far inside the abdomen radius the pixel is (linear falloff), maxed with the leg brightness. The abdomen radius is roughly a tenth of the display.
7. Hue: inside the abdomen, a fixed warm red-orange base hue; outside, the base hue plus half the abdomen-distance — so leg color shades gradually from red-orange near the body toward amber/yellow-green at the leg tips. Saturation full; the computed brightness is the value.

Randomness: only the per-pass travel direction.

Layout assumptions: unit-square 2D mapping and support for a translate/rotate coordinate-transform pipeline applied before the per-pixel renderer (if the engine lacks one, apply the inverse transform to each pixel's coordinates manually). No pixel-count hardcoding; fine as-is on any mapped 2D layout with enough resolution to resolve thin lines.

## Colors
Black background. Body: saturated deep red-orange. Legs: gradient from the body's red-orange through orange/amber toward the tips. No other colors.

## Controls
None.

## Timing
One full crossing takes on the order of ten seconds (including brief off-screen time between passes). Leg scissoring oscillates a few times per second.

## Clever bits
- Quadrant mirroring (absolute-value both coordinates) means two leg definitions render as eight legs.
- Because all legs meet at the origin, a leg is fully described by one endpoint: the point-to-line distance formula simplifies, and a dot-product sign check plus a radius check confines drawing to the correct half-line segment.
- The front/back leg-length asymmetry is done by skewing the radial distance with a term linear in x rather than defining more legs.
- Body sway comes free by coupling the frame translation's perpendicular component to the leg-swing angle.
