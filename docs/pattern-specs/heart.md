# heart
kind: 2D
sensors: no

## What it looks like
A filled heart shape, centered horizontally on the display, that rhythmically swells and shrinks ("beats") roughly once a second — a smooth pulse, not a snap. The heart's fill is a rainbow gradient that runs vertically across the shape, and the whole gradient slowly drifts around the hue wheel over time. Edges are softly antialiased so the outline looks clean even on low-resolution matrices. Everything outside the heart is black.

## Geometry / algorithm
The heart is built from classic construction geometry rather than a lookup image:

- The bottom half is a square rotated forty-five degrees (a "diamond"); its lower two edges form the pointed bottom of the heart.
- The top is two semicircles sitting on the diamond's upper two edges, forming the rounded lobes. The circle diameter equals the diamond's side length.
- A fixed aspect ratio relates total heart height to the diamond side length (derived analytically from that construction), so the shape stays correct at any size.

Per frame (state between frames is just one phase value):
- A slow sawtooth phase drives a half-sine that modulates the heart's height between roughly half and four-fifths of display height — this is the beat. The heart stays anchored so its vertical center stays put; derived quantities (diamond side, circle center, tip position) are recomputed each frame from the current height.
- (The original has commented-out optional code to also make the heart's position wander slowly; not required.)

Per pixel:
- Exploit left–right symmetry: mirror the horizontal coordinate about the heart's axis so only half the shape is evaluated.
- Translate the vertical coordinate relative to the heart's bottom tip.
- Region tests: in the lower (diamond) zone, the pixel is inside if it lies inside the diagonal edge line; near the edge, compute perpendicular distance to that line (a simple diagonal-projection factor since the line is at forty-five degrees). In the upper zone, test against the semicircle (with a sub-case for above vs. below the notch between the lobes); near the boundary compute distance to the circle (radial distance from the circle's center minus its radius).
- Coverage value: fully inside → full intensity; within a small antialiasing distance of the boundary → intensity ramps linearly from full down to zero across that distance; otherwise zero. The antialiasing distance is a small tunable fraction of the display (a few percent) — worth exposing, since ideal value depends on matrix resolution.
- Final brightness is the coverage value squared (softens the falloff / rough gamma).

## Color
Fully saturated rainbow. Hue = a scaled copy of the pixel's vertical position plus the slowly advancing time phase, so the heart shows a vertical hue gradient (roughly half the hue wheel top-to-bottom) that continuously cycles. Outside the heart: off.

## Controls
No formal sliders. Several tuning values are exported as plain variables (heart height, position, antialiasing distance) so they can be poked from the editor, but there is no UI. Reasonable to expose antialias distance and beat rate as sliders in a reimplementation.

## Timing
Beat cycle: a bit over a second per swell-and-shrink. Hue drift: same-order cycle (the gradient visibly crawls).

## Layout assumptions
Pure normalized 2D coordinates; no pixel-count hardcoding. Works on any 2D-mapped layout.
