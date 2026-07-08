# cube fire 3D
kind: 1D+2D+3D
sensors: no (declares sound inputs but never uses them — see note)

## What it looks like
Roiling volumetric blobs of colored flame drifting through a 3D volume: soft glowing cells swell, merge, and wink out, with the hottest cores washing out toward white while the fringes stay saturated. The overall hue slowly cycles around the wheel (a lap every several seconds), and there is a mild rainbow gradient across the volume so opposite corners differ in tint. The cell size itself slowly breathes larger and smaller over several seconds. Reads as "plasma fire" more than literal flames — there is no upward bias.

## Sensor note
The source exports bindings for a spectrum array and overall sound energy (the sensor-board hookup), but neither is referenced anywhere in the render path. The pattern is not actually sound-reactive; a reimplementation can omit them (or wire them in as an enhancement, clearly marked as new behavior).

## State / per-frame work
No arrays; only phases. Each frame compute:
- Three independent sawtooth time phases with slightly different short periods (each cycling in several seconds; a nearby ratio like roughly 10 : 13 : 8.5 keeps them from locking, which is what makes the motion feel non-repeating). A speed constant in source (not a UI control) divides all three periods together.
- A slowly breathing spatial-scale factor: a triangle/sine-type wave on a several-second period, mapped into roughly the one-quarter-to-three-quarters range. This is the cell-size "breathing".

## Per-pixel (3D) work
- Hue = the first time phase plus a small positional term (each of x, y, z contributes about a fifth of its normalized value), giving the slow global cycle plus a gentle spatial gradient.
- Raw intensity = the product of three triangle/sine-type waves, one per axis: each takes that axis's coordinate times the breathing scale factor, plus a wave of one of the three time phases as a moving offset. Multiply the three axis waves together and amplify by around an order of magnitude — the product of three unit waves is mostly small, so amplification makes only the coincident crests visible as blobs.
- Saturation = that amplified intensity minus one. So dim regions get sub-zero saturation (clamped: effectively full-ish desaturation doesn't matter since they're dark) — the key part is that as intensity climbs past about twice unity, saturation falls below full and the blob core bleaches toward white, like heat.
- Brightness = the amplified intensity cubed, for hard black between blobs and blown-out cores (values above one just clamp at full).

## 2D / 1D fallbacks
The 2D renderer calls the 3D one with the depth axis fixed at zero (a planar slice of the volume). The 1D renderer calls the 3D one with the normalized pixel index as x and the other axes zero (a line through the volume). Implement all three the same way.

## Colors
Full-spectrum cycling hues; each blob is a saturated colored glow whose core whitens with intensity. Background is black.

## Controls
None. (A speed constant exists in source only; exposing it as a slider is the obvious improvement.)

## Layout notes
Fully mapper-driven and resolution-independent; assumes normalized coordinates roughly in the unit cube.

## Non-obvious bits
- The whole effect is a separable product of three phase-offset axis waves — extremely cheap, yet the incommensurate time periods plus the breathing scale make it look like real turbulence.
- Overdriving the product and then deriving saturation from (intensity minus one) is the trick that fakes "white-hot cores" without any palette.
