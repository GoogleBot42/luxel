# Lotus Bloom

Idea spec written from Patternflow preset preset_0512.h (CC BY-SA 4.0, Seunghun Lee); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A single flower/mandala outline centred on the panel. Its edge is a closed curve whose radius rises and falls with angle, giving a ring of rounded petals that slowly rotate. A second ripple running outward along the radius makes the petal edges wobble and "breathe", so the flower looks alive. Layers from the centre out:
- a dim core disc filled with a crawling diagonal stripe texture in a contrasting hue (the stripes march across over time),
- a soft band just inside the outline that glows brightest at the edge and fades toward the centre, in a saturated hue near the base,
- a bright, pale (desaturated) outline a couple of pixels thick,
- outside, a faint aura of thin radial spokes (alternating angular slivers) in a third hue that fade out within a short distance of the outline.
Everything else is black. On a 128x64 panel the flower occupies roughly the middle of the screen, about two thirds of the panel height across, so it is clearly a centrepiece object, not a full-field pattern. It looks like a sacred-geometry or VJ "lotus" loop.

## The idea

Draw a polar rose as a distance-from-curve field (|radius − petal curve|) and colour different distance bands differently — outline, inner glow, core, outer aura. It works because one signed distance gives several concentric visual layers for free.

## Algorithm in words

- State: time accumulator from the speed knob.
- Per pixel: compute angle and radius from the panel centre.
- Target radius = a base radius plus a petal term (sine of angle × petal count, rotating with time) plus a radial ripple (sine of the radius itself moving outward over time) scaled by the fold knob.
- Distance d = |radius − target|.
- If d is tiny: bright, pale outline in the base hue.
- Else if inside the curve and within a moderate band: inner glow, value falling linearly from the edge inward, in a slightly shifted hue.
- Else if well inside (under a fraction of the target radius): core, lit only on diagonal stripes that crawl a step per tick of time, in a hue far round the wheel.
- Else if outside and within a wider band: aura, lit only on alternating thin angular sectors, dim and fading with distance, in a fourth hue.
- Sizes are in absolute pixels, tuned for a 64-pixel-high panel.

## Knobs

- K1 — base hue (all layer hues shift together).
- K2 — speed of rotation, breathing ripple and core crawl.
- K3 — petal count, from a few broad lobes up to a many-petalled star.
- K4 — fold: the strength of the radial ripple; zero gives a clean rose, high values make the outline wobble and even pinch into extra rings.
- No audio.

## Making it better in Luxel

- Express sizes in normalized coordinates so the flower fits any panel (64x64, 128x128, 128x64) and stays round on non-square panels.
- Anti-alias the outline with `smoothstep` on the distance and colour the glow with OKLCH so the inner gradient is even across hues; add `setGlow` for a soft bloom on the outline instead of hard bands.
- Animate the petal count/shape with easings (e.g. bloom open with `easeOutBack` and close again) and let the aura spokes rotate counter to the petals for depth; layering two roses with different petal counts in add mode gives a richer mandala.
- Precompute angle and radius once (a per-pixel table via `setPixelState`, or cache in arrays) since they never change — the per-pixel `atan2` + `sqrt` is the main cost on the ESP32.
- Optional audio: petal amplitude or ripple from `energyAverage` makes it a strong music visual. Best as one 2D pattern; for strips, a radial pattern on a ring map works, but on a straight strip it degenerates to bands.

## Payoff

Score: 3/5 — a solid, pretty centrepiece; needs anti-aliasing and resolution independence to shine.
