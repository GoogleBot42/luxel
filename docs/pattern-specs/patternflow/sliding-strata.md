# Sliding Strata

Idea spec written from Patternflow preset preset_0518.h (CC BY-SA 4.0, Seunghun Lee); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

The panel is cut into horizontal bands, each filled with a soft, cloudy noise texture. Every band slides sideways on its own — alternate bands in opposite directions, each at its own speed — and also lurches back and forth by a slowly wandering offset, so the stack looks like sheared geological strata, marbled slabs or a tape being torn and dragged. Each band darkens toward its top and bottom edges, so the bands look like rounded, pillowy strips with dark seams between them. Colour alternates band to band: one set in warm red-orange tones, the alternate set rotated around the wheel by a controllable amount (from neighbouring hues to complementary), with the noise lightly varying the hue within each band. Brightness follows the noise, so bands have dark gaps and bright clouds.

## The idea

Slice a smooth noise texture into strips and drag each strip independently — a "data-moshed" / glitch shear applied to an organic texture. It works because the contrast between organic content and rigid horizontal displacement is a recognizable glitch-art look.

## Algorithm in words

- State: time accumulator from the speed knob.
- Band index = row × band frequency (the band count knob); also keep the fractional position within the band.
- Per band: direction from parity; a speed multiplier from value noise of the band index; a horizontal offset = value noise of (band index, slow time) × the shear knob, so each band wanders back and forth smoothly.
- Per pixel: sample 2D value noise at the sheared x (pixel x + wander offset + travel) and y, at a moderate scale.
- Brightness = noise × a gain × a sine bump across the band (zero at edges, full in the middle).
- Hue = small noise-driven variation around red for even bands; the same plus the split offset for odd bands. Saturation fixed.

## Knobs

- K1 — palette split: the hue offset between alternating bands.
- K2 — speed of travel.
- K3 — shear amount: how far each band wanders.
- K4 — band frequency: a few thick bands to many thin ones.
- No audio.

## Making it better in Luxel

- Use `simplex2` (or `perlinFbm` with two octaves) for the texture — smoother and less blocky than value noise; precompute per-band offsets once per frame in `beforeRender`.
- Performance: render one noise row texture into a canvas and `blit` it per band with an offset (grid space), or reuse the same noise row stretched vertically — the content of a band is mostly horizontal, so a small canvas sampled with `fillCanvas` is far cheaper than full-res noise.
- Make the bands' height vary (thick and thin strata) and occasionally snap with `easeOutElastic` jerks for a stronger glitch beat; `beat(bpm)` can time the jerks.
- Choose band colours from a palette (`setPalette` + `paint`) — e.g. sandstone ochres, or neon — rather than raw HSV.
- Strips: on a strip each band becomes a segment of scrolling noise — a decent 1D variant. Main form: 2D pattern.

## Payoff

Score: 3/5 — a pleasant glitch-strata look with clear upgrades; not unique, but good in a playlist.
