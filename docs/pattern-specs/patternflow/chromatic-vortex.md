# Chromatic Vortex

Idea spec written from Patternflow preset preset_0629.h (CC BY-SA 4.0 per its SPDX header, by Seunghun Lee; the brief said MIT); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A spiral of rings centred on the panel, rendered three times — once each in red, green and blue — with each colour channel's spiral slightly offset in radius, like a lens with strong chromatic aberration. Where the three channels coincide the spiral arm is white; where they drift apart you see red, green and blue fringes on either side of the arms, and cyan/magenta/yellow where two overlap. The spiral both rotates and flows outward continuously, and the channel offsets themselves breathe with radius and time, so the fringes swell and collapse in waves travelling out from the centre. On a 128x64 panel the vortex is centred and the arms are cut off by the short edges, which gives a widescreen tunnel feel. Very psychedelic; at high split it becomes an almost pure RGB moiré.

## The idea

One analytic spiral wave per colour channel, each channel evaluated at a slightly different radius that oscillates with distance and time. Separating the channels spatially is the cheapest way to get chromatic aberration, and it turns a plain spiral into something that looks optically rich.

## Algorithm in words

- State: one time accumulator.
- Per pixel: distance and angle from the panel centre.
- For each channel compute a radial offset = sine of (a slow function of distance minus time), scaled by the split control, with the three channels phase-shifted from each other and green and blue using smaller and opposite-signed offsets than red.
- Each channel's intensity = sine(offset distance × density − time + angle + a per-channel phase), remapped from −1..1 to 0..1. Adding the angle makes it a one-armed spiral; subtracting time makes it flow outward.
- Scale the three channels by a colour-bias control (it trades red against green and boosts blue as it rises). If all three are near their peak, force pure white.

## Knobs

- K1 Split Dist — how far apart the channel spirals drift; zero gives a clean white spiral, high gives strong RGB fringing.
- K2 Swirl Speed — rotation/outflow speed.
- K3 Wave Density — how tightly wound the spiral is (ring spacing).
- K4 Color Shift — channel balance, sliding the overall tint from cool to warm.
- Audio: none directly; Split is the most dramatic knob to drive from a beat.

## Making it better in Luxel

- Add a spiral-arm-count control (multiply the angle term by an integer) and a twist control (angle term plus a fraction of log-radius) — a whole family of tunnels from one pattern.
- Let the aberration be radial *and* tangential (offset the angle per channel too) so fringes twist like a real lens.
- Cost: three trig evaluations of a near-identical phase per pixel, plus atan2 and sqrt. Precompute per-pixel radius and angle once into arrays (or a canvas at half resolution) and the per-frame cost becomes a few sines. `beatSin` on the split amount gives a music-feeling pulse without audio hardware.
- Post chain: a light `setBlur` softens the stair-stepped rings on a 64x64 panel; `setGamma` keeps the fringe colours from washing out.
- Strips: the 1D cut (distance from strip centre) is a nice RGB-fringed pulse expanding from the middle — worth a strip mode.

## Payoff

3/5 — genuinely trippy optics for little cost; worth porting with arm-count and twist added.
