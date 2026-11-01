# Wave Saw

Idea spec written from Patternflow preset preset_wave_saw.h (CC BY-SA 4.0 per its SPDX header, by Seunghun Lee; the brief said MIT); prose only, no code — the Luxel implementation is an original work built from this description. (Its lineage note says it was designed as a Blender wave-texture node setup and converted.)

## What it looks like

Bold parallel stripes in three flat colours — a thin white stripe, a red stripe, then a wide blue stripe, repeating — sliding steadily across the panel like a barber pole or a candy-striped awning. There is no shading: each stripe is a flat colour with a hard edge (a sawtooth profile cut into three bands). An angle knob rotates the stripes to any heading; a scale knob sets how many stripes fit across the panel; and a distortion knob adds fractal-noise warping so the straight stripes turn into wavy, marbled, wood-grain-like contours, with a fourth knob setting the size of the noise features. With distortion off it is a clean graphic pattern; with distortion on it looks like poured paint or a topographic map in flag colours.

## The idea

A recreation of a 3D-software "wave texture": rotate coordinates, take a linear phase along one axis, optionally add fractal noise to the phase, wrap it as a sawtooth, and colour it through a hard-stop ramp. Noise-perturbed stripes are a classic marble/wood-grain technique.

## Algorithm in words

- State: a phase that advances at a fixed rate; the four knob values.
- Per pixel: centred coordinates (x normalised to the half-width, y to the full height, so stripe spacing is not square). Rotate by the angle. Phase = rotated x × the scale control × a stripe-density factor + the running phase. If distortion is on, add distortion × a two-octave fractal noise sampled at the rotated coordinates × the noise-scale control.
- Wrap the phase into 0..1 per stripe period. Colour with a constant (no blending) three-stop ramp: a narrow first band white, a second band red, the rest blue.
- The original wires the push-buttons to reset each knob to its default.

## Knobs

- K1 angle — stripe heading (full turn).
- K2 scale — stripe density.
- K3 dist — noise distortion amount, from straight stripes to marbled contours.
- K4 dscale — noise feature size.
- Audio: none directly.

## Making it better in Luxel

- Animate the noise: sample `simplex3` with time on the third axis so the marbling slowly flows, rather than the stripes sliding through a frozen warp. Optionally slowly rotate the angle on its own.
- Make the ramp a palette control with several hard-stop presets (the flag-ish white/red/blue, candy, sunset bands, monochrome) and a softness control that blends stops (constant → smooth), using OKLCH so soft blends stay clean.
- Cost: with distortion off a frame is pure linear arithmetic per pixel — in `renderBulk`, `fillGradient` along the rotated axis (or per-row spans) is enough; with distortion, render the noise at reduced resolution into a canvas and upscale it, since the warp is low-frequency.
- Strips: works well — a 1D sawtooth band sequence scrolling along a strip, with noise jitter, is a nice "marquee" strip pattern.
- The library has many stripe/plasma patterns; this earns a place only as a general "banded noise texture" with palette presets.

## Payoff

2/5 — a clean but basic striped texture; worthwhile only as a palette-driven banded-noise utility pattern.
