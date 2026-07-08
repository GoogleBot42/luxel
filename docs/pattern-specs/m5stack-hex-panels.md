# M5Stack Hex panels
kind: 3D
sensors: no

## Overview
A six-mode demo pattern written for a cluster of small hexagonal LED tiles (37 pixels per tile) mounted flat and mapped in 3D, where the x/y plane is the face of the tiles and the z coordinate distinguishes one tile from the next. A single slider selects which of the six modes runs. All modes share a common set of slow global clocks and a common "proximity" helper (described below) that turns distance-between-two-values into a soft, gamma-shaped brightness bump — this is the core trick behind nearly every mode.

## Shared machinery
- Per frame, the pattern resets the coordinate transform and translates the map so the origin sits at the center of the mapped area (shifting by half in x and y). In the rotating-line mode only, it additionally rotates the whole coordinate frame continuously, completing a revolution every few seconds.
- Three sawtooth clocks are computed each frame: a "fast" one cycling in roughly three seconds, a stretched/offset copy of it that overshoots slightly past both ends of its range (so sweeps fully enter and exit the visible area rather than popping), and a "slow" one cycling in roughly six or seven seconds used mostly for hue drift.
- Per pixel, the renderer computes the polar angle of the pixel around the center and a normalized radius. The radius normalization divides by a hardcoded magic constant that happens to be the maximum radius of the author's particular map, mapping it to a zero-to-one range. **Hardcoding note / fix:** this constant should instead be derived from the map (max distance from center), or the map should be pre-normalized.
- Proximity helper: given two scalar values and a half-width, it returns full brightness when they are equal, falling off linearly to zero at the half-width, then squares the result for a gamma-corrected soft edge. A zero half-width falls back to a small default (about an eighth). Almost every mode paints "brightness where quantity A is near sweeping quantity B."

## The six modes (slider position, low to high)
1. **Radiating rainbow rings.** Hue is a blend of the pixel's radius and the slow clock (halved so the hue drifts gently); brightness is a fairly wide soft ring where the radius matches the sweeping stretched clock (slightly sped up). Reads as colored rings breathing outward from the center every few seconds.
2. **Rotating rainbow bar.** The whole frame spins (one revolution every few seconds). Hue comes from a fraction of the radius plus the slow clock; brightness is a soft band where the diagonal sum of x and y (averaged and recentered) matches the sweeping clock. Reads as a colored bar sweeping across while everything rotates — a lighthouse-beam feel.
3. **Radar wedge on a ring.** Brightness is the product of two proximity terms: a wide ring where radius matches the sweeping clock, and a wedge where the polar angle matches a rapidly rotating target angle (the sweep clock scaled up several-fold and wrapped around the circle). Hue drifts slowly with the slow clock. Reads as a bright arc segment spiraling around as the ring expands.
4. **Warm panel-by-panel wipe.** Each pixel's x is offset by several times its z coordinate — since z encodes which tile a pixel belongs to, this lays all tiles out along a virtual line. A narrow soft band (half-width roughly one part in five of the whole span, i.e. a handful of pixels) sweeps along the combined diagonal of that virtual line every few seconds. Fixed warm orange/amber hue, high saturation, brightness cubed for a punchy hot core. Reads as a fiery wipe traveling tile to tile in sequence. **Hardcoding note:** the band width is expressed as a ratio tied to the author's total pixel count; better expressed as a fraction of the layout span.
5. **Blue iris with wandering dark pupil.** The whole field is a soft blue (moderately desaturated). A "pupil" point orbits the center on a small circle (radius about a quarter of the field, one orbit every few seconds). Brightness is one minus the product of two wide proximity bumps in x and y centered on the pupil, so a soft dark hole glides around on a blue background — like an eye looking around.
6. **Hardcoded status/strobe demo.** Uses the pixel index modulo the per-tile pixel count (37) to pick out three fixed four-pixel clusters on each tile. Everything not in those clusters is painted a bright crimson/pink. One cluster blinks dim red at a 50% duty cycle of the fast clock; another blinks dim blue at roughly 80% duty with a phase offset; the third cluster stays dark. Reads like indicator lights blinking inside a solid bright field. **Hardcoding note:** both the tile size and the cluster indices are literals for this specific product; a port should parameterize tile size and cluster membership.

## Controls
- One slider ("mode select"): divides its range into six equal bins choosing the active mode. (The bin math slightly compresses the top of the slider range so the maximum position still lands in the last bin.)

## Timing feel
Everything is leisurely: sweeps and orbits complete in a few seconds; hue drift takes twice that. Nothing is frame-rate dependent — all motion derives from the global time sawtooths.

## Non-obvious details
- The stretched sweep clock deliberately ranges a bit below zero and above one so soft-edged sweeps fully clear the display at both ends instead of lingering.
- The proximity function squaring is doing double duty: soft antialiased edges plus rough gamma correction.
- Mode dispatch is a function-reference table indexed by the slider, not a switch — worth mirroring for extensibility, but any dispatch works.
