# Midsummer Sea

Idea spec written from Patternflow preset preset_0712.h (CC BY-SA 4.0 per its SPDX header, by Seunghun Lee; the brief said MIT); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A synthwave-postcard seascape at sunset, composed for a tall portrait view and turned a quarter turn onto the 128x64 panel. From the far end: a sky that grades from black at the zenith through deep indigo and violet to warm orange near the horizon, crossed by faint diagonal cloud streaks; a white sun disc with an orange glow halo, sitting somewhere between just-touching the horizon and well up in the sky; a horizon line about a third of the way down; then a band of sea that takes up most of the remaining view, with wave bands that are thin and tightly packed near the horizon and wide near the viewer (a cheap but effective perspective), in violets and indigos; a brighter, swaying column of light on the water under the sun — the sun's reflection path — widening toward the viewer; and white glitter sparkles that blink on wave crests, densest inside the sun path. At the near end, a strip of warm orange-to-peach sand with a faint grain, and a wash of surf that surges up the beach and retreats on a slow cycle with a rippling white foam line at its leading edge and darker wet sand behind it.

The colour comes from a single brightness-to-colour ramp (black → indigo → violet → orange-red → peach → white), which is why it has that cohesive retro-sunset look. Motion is gentle: wave bands roll toward the viewer, the sun path sways, glitter twinkles a few times a second, surf breathes. It is calm, pretty, and immediately readable as a place.

## The idea

A tiny procedural landscape: split the view into sky, sea and beach bands, give each band one or two cheap animated effects, compute a single scalar "brightness" per pixel, and let one artful colour ramp do all the colouring. Perspective comes from a reciprocal-depth wave frequency.

## Algorithm in words

- State: a time accumulator.
- Geometry: a horizon a third of the way into the tall view, a beach line near the near end, a sun on the centre line whose height follows the sun control.
- Sky pixels: base brightness rising toward the horizon; if inside the sun's radius (slightly squashed vertically) full brightness, otherwise add an exponential halo by distance. Add a small fixed boost where two crossed sines (a slowly drifting cloud field) exceed a threshold, only in the upper sky.
- Sea pixels: depth = position between horizon and beach, 0..1. Perspective term = reciprocal of depth (plus a small offset). Wave band = sine of perspective term + a sideways wobble (scaled by the waves control) + time at a rate that increases with depth. Brightness = base that darkens with depth + band × an amplitude that grows with depth and the waves control. Sun path: a column around the sun's x, widening with depth and swaying with a sine, adds brightness. Glitter: a hashed value over a fine grid, reseeded a few times a second, lights a pixel fully when above a threshold set by the glitter control (lower threshold inside the sun path) and the wave band is near a crest.
- Beach pixels: sand brightness with a slight gradient and per-cell hash grain. A surf edge line advances and retreats on a slow sine (reach scaled by the waves control) with a sideways ripple; pixels on the sea side of the edge are darker wet water, and a thin band right at the edge is near-white foam.
- Clamp and map every pixel through the ramp.

## Knobs

- K1 Waves — sea roughness: wave amplitude, sideways wobble, sun-path sway and surf reach.
- K2 Speed — overall animation rate.
- K3 Sun — sun height, from setting on the horizon to high in the sky.
- K4 Glitter — sparkle density on the water.
- Audio: none directly.

## Making it better in Luxel

- Best as a **scene**: a sky gradient layer, a sun sprite (the library already has a `sun-spin` sprite and a `sunrise` scene — natural companions), a sea pattern layer, and a beach layer, composed with blend modes. Then the same sea can sit under a moon, a clock text layer, or a different sky.
- Animate the sun on a slow day cycle (sun height eased with `easeInOutSine` over minutes) and let the colour ramp follow it — dawn, golden hour, dusk, night with a moon path — using OKLCH ramps for clean gradients.
- Use `simplex3` for clouds and for a less regular sea surface; keep the reciprocal-depth wave frequency, which is what sells the perspective.
- Cost: the bands are horizontal in the scene's own frame, so per-row values (depth, perspective term, base brightness) can be computed once per row; glitter can be a sparse set of `splat`s rather than a per-pixel hash. Lay out from the grid's aspect so it works landscape or portrait.
- Overlaps the library's `ocean` and `sunset` patterns — consider retiring weaker ones in favour of this composition. Strips: a 1D vertical cut (sky gradient → sun → sea → sand with surf) makes a lovely strip pattern for a vertical strip.

## Payoff

4/5 — a genuinely pretty, readable little landscape; the strongest "scene" candidate in this batch.
