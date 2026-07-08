# Rock sparks
kind: 2D
sensors: yes (real-time wall clock; drives a digital GPIO output — no audio/accelerometer input)

## Overview
A three-mode medley pattern (a community remix of two well-known patterns plus a checkerboard) that cycles automatically between modes every few seconds, blending between them with a per-pixel stochastic dither crossfade. Independently of the visuals, each frame it also switches a digital output pin on or off based on the time of day — the pin is driven high only during a two-hour late-evening window (from about 9pm until about 11pm) and low otherwise, apparently to switch a relay for house lights. This requires a synced real-time clock; there is no sound reactivity despite the name.

## Mode sequencer
- A global sawtooth clock spans the whole mode cycle (three modes at a few seconds each — roughly ten seconds for a full loop). Its integer part picks the current mode; the fractional structure drives crossfades.
- Every frame, the per-frame setup functions for **all** modes run (the author left a note that only the two active ones are needed — an easy optimization for a port).
- Crossfade: during roughly the last third of a mode's time slot, each pixel independently and randomly chooses to render either the outgoing or the incoming mode, with the probability of choosing the new one easing from zero to one (the linear progress is passed through a sine-shaped easing). This produces a sparkly temporal dither rather than an alpha blend — cheap and it works with any renderer.
- **Quirk:** the crossfade progress is computed from the mode clock modulo *two* while there are *three* modes (the framework this was lifted from assumed two). Consequence: only some mode transitions actually get the dither fade; others cut hard. Reproduce or fix, but note it.

## Mode A — "confetti dust"
Purely random per-pixel sparkle; no state between frames.
- Very sparse: each pixel has roughly a one-in-ten chance per frame of being lit at all, and the brightness value is cubed so most lit pixels are dim with occasional bright pops.
- The hue scheme itself rotates on its own short cycle (about four seconds, in four phases): in the first phase sparks are reds/pinks clustered tightly around both ends of the hue wheel (mostly deep red, some the other side of red); in the second phase they're greens with slight downward jitter; in a third phase saturation is forced full; otherwise saturation is full for most pixels with roughly one in seven sparks rendered white.
- **Quirk:** in some phases neither hue nor saturation is freshly assigned, so pixels reuse whatever stale hue value the previous evaluation left in a shared global — the visual result is streaks of "leftover" color. A faithful port needs mode-level persistent scratch variables rather than per-pixel locals.
- Colors are written through a high-color-depth output call where available.

## Mode B — "spotlights / searchlights"
Three simulated moving light beams playing over a 2D plane, like concert searchlights viewed from above.
- Per frame: a fast phase accumulator scaled by a speed setting, plus a slow gentle bob (a small triangle-wave vertical/horizontal drift with a period of several seconds) that shifts the whole scene.
- Per pixel: for each of the three sources (spaced evenly along one axis, each with its own phase offset), the code checks whether the pixel is on the lit side of that source's sweep plane, computes an angular coordinate of the pixel relative to the beam direction (the beam direction oscillates sinusoidally per source), and derives two intensities: one for a narrow center beam and one for an offset edge beam. Intensity is a "focus" constant divided by the absolute sine of the angular distance (clamped by a "drive" ceiling so it doesn't blow up at the beam core), then squared for gamma. Whichever of center/edge is brighter contributes its color, scaled by intensity, additively into the pixel; contributions from all three sources accumulate and the sum is clamped before display.
- Default look: a vivid green center beam with violet/indigo beam edges sweeping and crossing.
- Controls (all exported to the UI):
  - Color picker "center beam color" and color picker "edge beam color" (defaults: strong green; violet-blue).
  - Slider "width": angular offset between the center and edge beams — widens the colored fringe (a tiny floor keeps it nonzero).
  - Slider "focus": overall brightness/tightness of each beam (tiny floor as well).
  - Slider "drive": overdrive ceiling — at high settings beams saturate hard ("goes to eleven": ranges from unity up to about an order of magnitude, curved quadratically).
  - Slider "speed": sweep speed, spanning roughly a factor-of-three range above a nonzero base.
  - Slider "vertical sweep": biases motion between mostly-horizontal and mostly-vertical sweeping.
- A one-time translation shifts coordinates so the origin is at the panel center.

## Mode C — rotating rainbow checkerboard
A checkerboard that spins about the panel center while zooming.
- Per pixel: coordinates are shifted to center, rotated by an angle that completes a revolution every few seconds, then shifted back (with an intentional-or-accidental asymmetric re-shift that offsets the board). A zoom factor derived from a triangle wave (period a few seconds) varies the number of visible squares from under one to a few across.
- Cell parity (checkerboard test on the floored scaled coordinates) gates the pixel fully on or off; hue drifts as a function of the rotated coordinates plus the zoom clock, folded so it never reaches the very top of the hue range (values past about two-thirds wrap back) — a rainbow that avoids re-crossing red.
- **Quirk:** the per-pixel renderer for this mode is (re)assigned from inside its own per-frame setup function every frame — harmless but pointless; a port should just define it normally.

## Layout assumptions
Needs a 2D mapping with coordinates normalized zero-to-one (author used LED rings). Nothing hardcodes pixel count.

## Timing feel
Modes hold for a few seconds each; the dust mode's palette phases turn over every second or so; searchlight sweeps take a second or two per pass; the checkerboard makes a full rotation in several seconds. GPIO/house-lights check happens every frame but only changes at hour granularity.
