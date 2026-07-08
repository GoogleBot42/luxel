# amoeba
kind: 1D
sensors: no

## Provenance note
This is machine-generated code (compiled from a small declarative particle-system description into unrolled Pixel Blaze source). The spec below describes the declared design, which is what should be reimplemented — the generated code contains some bookkeeping quirks (noted at the end) that should not be copied.

## What it looks like
A soft, dim purple strip over which fuzzy blue blobs drift slowly in both directions — like glowing amoebas swimming past each other. Each blob fades in, glides along the strip, and fades out over a few seconds. Independently, brief dark "dimples" pop in and out at random positions all along the strip a few times per second, momentarily denting the brightness where they land. The overall feel is organic, unhurried, and constantly bubbling.

## Architecture: three particle pools plus a gradient
All the work happens once per frame in the pre-render step, which fills per-pixel scalar buffers; the per-pixel render step just reads out a precomputed RGB buffer (with a squaring gamma applied per channel).

A wall-clock accumulator (seconds, integrated from the frame delta) is the only clock.

There are three pools of "pulse" particles. Each particle contributes a smooth bump to a scalar field over the strip:

- Spatial profile: a half-sine arch, about one fifth of the strip wide, centered on the particle's position; zero outside that window.
- Temporal envelope: a triangle over the particle's lifetime — fades in to full at mid-life, then back out.
- A pixel's field value is the sum of contributions from all live particles in that pool (bumps can overlap and add).

The three pools:

1. **Rightward drifters** — up to eight concurrent. Spawned one at a time whenever a randomized interval (about a second, jittered up or down by roughly a fifth) has elapsed and a slot is free. Each gets a uniformly random start position along the strip and a small uniformly random rightward velocity (crossing a few percent of the strip per second — slow enough that a particle traverses maybe a tenth of the strip in its lifetime). Lifetime: a few seconds. A particle also dies early if its bump has fully exited the strip.
2. **Leftward drifters** — identical in every respect but with velocities mirrored leftward.
3. **Stars (dimples)** — up to eight concurrent, spawned at a fixed brisk cadence (several per second), at uniformly random *stationary* positions, with a short lifetime of about a second and the same bump width and triangle envelope.

## Combining the fields into color
Per pixel, per frame:

1. Sum the rightward and leftward drifter fields into one "flies" field.
2. Map that field through a two-stop linear RGB gradient: at zero, a half-intensity purple (equal parts red and blue, no green); at one (and above, clamped by the gradient's end behavior), pure full blue. So background purple where no blob is present, saturating to vivid blue at blob centers; overlapping blobs push further toward the pure-blue end.
3. Compute a darkening mask from the star field: one minus (the star field clamped to unit range, scaled by a large fraction — leaving only roughly fifteen percent brightness at a dimple's core).
4. Final channel value = the minimum of the gradient color channel and that darkening mask. Because the mask is applied via a per-channel minimum against a scalar, dimples cap brightness rather than tinting.
5. At render time, square each channel (simple gamma) before output.

## Colors
Two-stop palette only: muted mid-brightness purple through vivid pure blue. Dimples read as neutral darkening toward near-black, not as a third color.

## Layout assumptions
None hardcoded — everything is expressed in fractions of the strip and uses the runtime pixel count. Purely 1D; on a 2D install it would just follow strip order.

## UI controls
None.

## Timing summary
Drifter lifetime: a few seconds. Drifter spawn rate: about one per second per direction. Dimple lifetime: about a second; dimple spawn rate: several per second. Nothing is frame-rate dependent except via the integrated clock.

## Quirks in the generated code (do not replicate)
- The intended "max concurrent" counter is mis-tracked (a shared counter that is never actually used for gating); in practice the cap is enforced by the fixed slot array size. Just enforce the cap properly.
- The generated slot loops stop scanning at the first empty slot, so a live particle sitting after an empty slot can be skipped for a frame. Correct behavior is to update/draw every live particle regardless of slot order.
