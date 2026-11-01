# Firefly Hollow

Idea spec written from Patternflow preset preset_0713.h (CC BY-SA 4.0 per its SPDX header, by Seunghun Lee; the brief said MIT); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A dusk meadow, composed for a tall portrait view and turned a quarter turn onto the 128x64 panel. The sky darkens from a dim red-brown glow near the horizon to near-black at the top; soft, drifting clouds are lit warm orange-yellow from below and only appear in the upper half of the sky, fading out toward the horizon. A gently rolling hill silhouette crosses the middle of the view as a dark grey band. In the foreground, a ragged row of near-black grass blades of varying heights sways in the wind, each blade with a faint lit tip. Over it all float a dozen-odd fireflies: soft round glows that wander on slow, independent figure-eight-like paths over the lower part of the scene and pulse on and off — each brightens, holds briefly, and goes dark at its own rhythm, so at any moment only some are lit. Lit fireflies glow warm yellow-white through orange at their edges. The palette ramp is black → dark grey → red → yellow → orange → pink → warm white, with everything above a moderate brightness clamped to warm white, so firefly cores are creamy and the landscape stays in deep, dark, warm tones.

It is atmospheric and slow; the twinkling fireflies against the near-black silhouettes are the whole appeal.

## The idea

A layered night landscape (sky gradient, clouds, hill, swaying grass) as a dark stage, with a small number of independently wandering, independently blinking soft glows on top. The darkness makes each firefly read clearly.

## Algorithm in words

- Setup: give each firefly fixed random-ish motion rates, a blink rate, and random starting phases.
- Per update: advance each firefly's phases. Its position is a sine-based wander: x sweeps across most of the width at its own rate; y hovers in the lower-middle of the view with a main oscillation plus a small secondary one. Its brightness is a sine clipped at zero and cubed — dark most of the time, with short soft-peaked flashes. Advance two grass-sway phases and a cloud drift offset (cloud drift speeds up with wind).
- Per column (the view's horizontal axis): hill height = sum of two slow sines; grass blade height = hashed per pair of columns.
- Per pixel: base sky brightness rising toward the horizon. In the upper half, add clouds: two octaves of drifting value noise, faded toward the horizon, passed through a smoothstep threshold. Below the hill line, replace with a dark hill value. Grass top = view bottom minus blade height plus wind sway (two sines across columns); below it, near-black, with a lit one-pixel tip. Then add every lit firefly's Gaussian glow (radius set by the glow control, skipped beyond a few radii). Clamp and map through the ramp.

## Knobs

- K1 Flies — how many fireflies are active (up to a small maximum).
- K2 Speed — overall pace of wandering, blinking, sway and cloud drift.
- K3 Glow — firefly halo radius, from pinpoints to large soft blobs.
- K4 Wind — grass sway amplitude and cloud drift speed.
- Audio: none directly; mapping sound to Glow would make the fireflies swell with the music.

## Making it better in Luxel

- Make it a **scene**: a static-ish landscape layer (sky, clouds, hill, grass) under a fireflies layer composited with an additive blend. The library already has `fireflies` and `colourful-fireflies` patterns — this spec's improvement over them is the stage; reuse the best firefly motion as the top layer.
- Draw fireflies with `splat` in additive mode in `renderBulk` instead of summing Gaussians at every pixel (the per-pixel loop over all flies is the expensive part). Give the flash an asymmetric envelope (fast `easeOutExpo` attack, slow decay) — real firefly flashes, and occasionally sync a few neighbours.
- Clouds: `simplex3` with time on the third axis gives smoother clouds than value noise; render them to a small canvas and upscale. A slow moon or a few twinkling stars (hash-based) in the dark upper sky would fill the empty top.
- Lay out from the grid's aspect so the meadow runs along the long axis of a landscape panel instead of being turned sideways.
- Strips: a fireflies-only strip version (soft pulsing glows wandering along the strip over a dim warm gradient) is a classic, cheap strip pattern.

## Payoff

4/5 — moody and lovely; the landscape-plus-glows composition is exactly what scenes are for.
