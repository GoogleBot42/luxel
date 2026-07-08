# xorcery 2D/3D
kind: 2D + 3D (the 2D renderer simply calls the 3D renderer with depth pinned to zero)
sensors: no

## What it looks like
A constantly morphing field of blocky, mirror-image "digital" cells — nested rectangles and
Sierpinski-like reflected structures — that ripple, breathe and dissolve into one another. The
geometry has a distinctly bitwise/glitchy character: patterns reflect around the center and
subdivide into self-similar blocks rather than flowing smoothly. Brightness pulses in and out so
regions bloom to vivid color and fade to black; the hue drifts continuously through the full
rainbow with a diagonal spatial gradient laid over the whole volume. The overall feel is
hypnotic and medium-paced: fine detail shimmers on a timescale of a second or two while the
larger structure evolves over tens of seconds.

## Algorithm
No state persists between frames beyond the engine's global clocks. Each frame, precompute a
handful of phase values from free-running sawtooth clocks with different periods:

- two clocks with the same medium period of several seconds, one kept as a raw ramp, the other
  scaled to a full circle for use as a sine phase;
- a slower clock (roughly five times longer) used through a triangle shaping;
- another circular-phase clock a bit slower than the medium ones, used through a sine.

Per pixel (3D):

1. Center each coordinate on the middle of the space, scale it up by a small integer factor,
   and take the **bitwise XOR of the three scaled coordinates**. (Pixel Blaze arithmetic is
   fixed-point, so XOR on these values produces the characteristic reflected/blocky
   self-similar cells.) Divide the XOR result by a fairly large constant to bring it back to a
   small magnitude.
2. Multiply that by a time-varying zoom factor: a triangle wave on the slow clock (large swing)
   plus a sine on the other circular clock (smaller swing). This makes the block structure zoom
   and counter-zoom over tens of seconds.
3. Take that product modulo a slowly "breathing" modulus — a moderate baseline plus a smaller
   triangle-wave excursion on the medium clock. Feed the remainder through a sine-like
   full-wave shaping (the builtin 0-to-1 sinusoid), and add a sine of the medium circular
   clock. Call this the raw field value.
4. Brightness: sum the absolute values of the raw field and the breathing modulus, add the
   medium ramp clock, wrap to the unit interval, square it, run it through a triangle wave,
   then cube the result. The squaring/triangle/cubing chain creates high contrast: most of the
   field sits near black with sharp bright ridges.
5. Hue: a triangle wave of the raw field scaled down to a narrow band (so the field only
   perturbs hue slightly), plus the average of the three raw coordinates (a diagonal rainbow
   gradient across the volume), plus the medium ramp clock (steady hue rotation).
6. Full saturation always.

## Colors
Full-spectrum rainbow, fully saturated, on black. No fixed palette — hue rotates continuously,
with a diagonal position gradient and small field-driven perturbations, so neighboring blocks
sit in nearby hues.

## Controls
None.

## Layout assumptions
None — purely coordinate-driven via normalized 2D/3D world coordinates; works on any mapped
layout. On an unmapped 1D strip nothing renders (no 1D entry point); the obvious fix is a 1D
wrapper that calls the 3D renderer with the normalized index as one coordinate and zeros for
the others.

## Non-obvious tricks
The whole effect hinges on XOR of centered, up-scaled fixed-point coordinates: that single
operation is what generates the reflected, recursively subdivided block geometry that would be
very hard to get with ordinary smooth math. Everything else is contrast shaping and slow
multi-clock modulation to keep it evolving without obvious repetition.
