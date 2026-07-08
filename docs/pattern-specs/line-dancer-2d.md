# Line Dancer 2D
kind: 2D
sensors: no

## What it looks like

A bright, rainbow-striped ribbon of light snakes and twists across the 2D surface, like a glowing line being whipped around and wound into spirals — the author describes it as inspired by an old PC screensaver's twisting-line effect. The line continuously coils and uncoils around the center of the display, with the amount of curl pulsing as it moves. A "breathing" zoom makes the stripes widen and narrow on a cycle of a few seconds. Optionally, the whole image is repeated as a kaleidoscope: the picture is sliced into several equal pie wedges around the center, each wedge showing a rotated copy of the image, and the whole kaleidoscope slowly spins.

Motion feel: fluid and hypnotic, medium-fast by default; the innermost region twists more violently than the edges.

## Algorithm

Coordinate frame: 2D coordinates are shifted so the origin sits at the center of the mapped area (a global translation by half a unit on each axis).

State kept between frames:
- A running time accumulator, advanced each frame by the elapsed time in seconds, wrapping after a very long period (many minutes) so it never overflows.
- A derived animation clock: the accumulator multiplied by the speed setting.
- A "zoom" value, recomputed each frame as a triangle-shaped oscillation with a period of roughly five seconds, ranging from zero to one.

Per-pixel work (2D renderer):

1. Kaleidoscope stage (only when more than one wedge is selected): take the pixel's polar angle around the center, fold it modulo the wedge angle (a half-turn divided by the wedge count, doubled — i.e., the full circle divided evenly into the chosen number of slices), add a continuously increasing rotation proportional to the running time (so the kaleidoscope spins), then convert back to Cartesian coordinates at the original radius. This maps every pixel back into one "master" wedge of the image.

2. Twist stage: compute a value from the twist setting minus the pixel's distance from center scaled up by a factor a bit over two — call it the local radius term. The local rotation angle is that term squared, times the sine of (that term plus the animation clock). This makes rotation strength grow rapidly toward the center and oscillate over time. The pixel's horizontal coordinate is then replaced with the standard rotation formula's x-output using that angle (only the rotated x is used; y is not rewritten). This asymmetric, radius-dependent rotation is what draws the twisting line.

3. Shading: brightness is one minus a triangle-wave of the twisted horizontal coordinate scaled by (a factor of several) times the zoom value; the result is squared for contrast, giving a bright core line with dark gaps. Hue is a linear ramp along the twisted horizontal coordinate scaled by zoom, offset by the zoom itself and by the local rotation angle normalized by a full turn — producing rainbow banding that shears along the twist.

Layout assumptions: fully map-driven; no pixel-count hardcoding. Needs a 2D map. There is no 1D fallback renderer.

## Colors

Full continuous rainbow, fully saturated. Hue sweeps smoothly along the ribbon; brightness carves the ribbon out of blackness (dark background, vivid multi-colored stripes, brightest at stripe centers).

## Controls

- Slider, "speed" concept: scales the animation clock over roughly a 1-to-10 range. Faster twisting/undulation at higher values.
- Slider, "twist" concept: sets the twist parameter over a modest range (roughly from a bit over one to about twice that). Higher values pull the twisting region outward and tighten the coiling.
- Slider, "reflections/sides" concept: selects the kaleidoscope wedge count from one (kaleidoscope off) up to seven, in integer steps.

## Timing

Default motion completes visible undulation cycles in around a second or two; the zoom "breathing" takes about five seconds per cycle; the kaleidoscope rotation is a slow continuous drift.

## Non-obvious details

- The twist uses only the x-output of a rotation (y left as-is), which smears rather than rigidly rotates — this asymmetry is what makes it read as a dancing line instead of a spinning texture.
- Rotation strength scales with the square of the distance-derived term, concentrating chaos at the center while edges stay calm.
- The wedge-fold does not mirror alternate wedges, so wedge seams show a rotational (not reflective) repeat. (The source notes mirroring as an optional variation for seamless tiling.)
