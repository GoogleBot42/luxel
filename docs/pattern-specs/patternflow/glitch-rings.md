# Glitch Rings

Idea spec written from Patternflow preset preset_0707.h (CC BY-SA 4.0 per its SPDX header, by Seunghun Lee; the brief said MIT); the preset has no real name (its display name is "Untitled pattern"), so the name here describes what it draws. Prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

Hard-edged concentric rings pulsing outward from the panel centre in a deliberately crude palette: black, a deep violet-blue, a bright azure, and a large region of pure saturated red. The ring brightness is posterized into a few flat steps, so the rings are bold flat bands rather than smooth gradients — it looks like a cheap synth-pop music video or a CRT test card gone wrong. Two glitch effects ride on top: the rings are wobbled sideways by a sine of horizontal position, so their outlines are rippled rather than round; and every so often whole horizontal rows tear sideways by a jittering amount, like VHS tracking errors, in bands that sweep up and down the panel. On a 128x64 panel the rings are ellipses stretched to the panel's aspect (the coordinates are normalised per axis), filling the whole frame.

## The idea

A radial sine pulse, quantized into a handful of flat bands and mapped through a stark palette, then broken up with sinusoidal wobble and intermittent horizontal row tearing. The idea is "analogue video glitch over a simple pulse".

## Algorithm in words

- State: one time accumulator, wrapped.
- Per row: compute a pseudo-noise from two sines of the row index and time; when it exceeds a threshold set by the glitch control, that row gets a horizontal shift that jitters rapidly with time and scales with the glitch amount. Otherwise no shift.
- Per pixel: normalise x (plus the row shift) and y to the panel's half-extents, take the distance from centre, and compute a sine of (distance × ring frequency − time + a wobble term that is a sine of x scaled by the glitch amount).
- Map the result to 0..1, floor it into the number of steps set by the quantize control, and look the stepped value up in a hard-stop colour ramp (black for the lowest band, violet-blue, azure, then red for the upper half).

## Knobs

- K1 Glitch — both the ring wobble and how often/how far rows tear; zero is a clean pulse.
- K2 Speed — pulse and glitch speed.
- K3 Freq — ring density.
- K4 Quantize — number of posterization steps, from very coarse bands to nearly smooth.
- Audio: none directly; mapping onsets to Glitch would make tears fire on the beat.

## Making it better in Luxel

- Make the tear events discrete and eased: pick random bands at random times (`hash2` on a coarse time tick), shift them with `easeOutElastic` so they snap and settle, and occasionally split channels (RGB offset) inside a torn band for a proper datamosh look.
- Replace the fixed garish ramp with a palette control (`setPalette`/`paint` or `setOutputPalette`) — the posterized stepping is the identity, the specific red/blue is not.
- Cost: precompute per-pixel radius once (it does not change), so a frame is one sine per pixel plus a lookup; the row shifts can be applied by sampling a radius canvas at a shifted x. In `renderBulk`, a `blit` of shifted row strips of a pre-rendered ring frame is cheaper still.
- Overlaps "Warped Wave" (also posterized rings with wobble) and "Tile Waves"/"Origin" (radial ripples); a single "rings" pattern with wobble, tear, posterize and tile modes would cover all four. Strips: a 1D version (pulse from the centre, with "tears" as segment jumps) works fine.

## Payoff

3/5 — bold and energetic but crude; the glitch-tear mechanic is the part worth keeping, folded into a combined rings pattern.
