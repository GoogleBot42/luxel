# Bouncy Boxes
kind: 2D (a 1D fallback renderer exists but just outputs black)
sensors: no

## What it looks like
On a cylindrical LED matrix — several times wider around than it is tall — four solid square blocks glide around at constant speed, wrapping seamlessly around the cylinder horizontally and bouncing off the top and bottom edges. When squares meet they collide like billiard-ball rigid bodies: they never interpenetrate, they push apart and exchange velocity along the collision axis, and they never slow down (speed is renormalized every frame, so the scene stays permanently lively). Each square is filled with a rainbow hue that is offset a quarter-wheel from its siblings and slowly rotates through the whole wheel over roughly ten seconds; within each square the hue additionally shifts outward from its center, giving a radial two-tone gradient. Square edges are slightly soft (about one pixel of falloff). The black background carries optional "digital glitch" garnish: random single-pixel sparkles in random colors, an occasional short horizontal streak, and up to three "tearing" bands — randomly chosen rows whose entire content (background band plus any square passing through) is shifted sideways by a random amount for a fraction of a second, like a corrupted video frame. Torn rows show as pale, nearly white bands where empty, and squares crossing them appear sliced and offset.

## Geometry and layout assumptions
Hardcoded for a specific mapping: a cylinder a few dozen columns around and eight rows tall (aspect about four to one), wired as one strip in column-major boustrophedon (serpentine) order — the renderer ignores the supplied world coordinates and derives (column, row) directly from the pixel index: column = index divided by the height, row = remainder, with the row order reversed on every other column. Square side length is half the height. Obvious fix: derive width/height from the pixel count or use the supplied normalized 2D coordinates instead of index math, and scale square size, speeds, and shift magnitudes proportionally.

## State kept between frames
For each of the four squares: horizontal position (in wrapped column units, top-left corner), vertical position, and a 2D velocity. Initial conditions spread the squares evenly around the cylinder at varied heights with varied diagonal velocities (speeds of a few cells per second). Also: three tearing-band records (row, signed shift, active flag, hue), a coarse glitch frame counter, and the four animated base hues.

## Per-frame simulation
1. Clamp the frame delta to a small maximum (a few percent of a second) so physics stays stable across hiccups.
2. Integrate: advance each square by velocity times delta; wrap horizontally; reflect off top/bottom (mirror the overshoot and negate vertical velocity).
3. Collision resolution, iterated several times per frame over all six unordered pairs: measure center-to-center offsets using the shortest signed horizontal distance around the cylinder; if the boxes overlap on both axes, resolve along the axis of least penetration — push both apart by a large fraction of the penetration (split evenly, minus a tiny slop tolerance), re-apply wrap and edge bounce, then, if they are approaching along the collision normal, apply an equal-and-opposite elastic impulse (full restitution) that swaps the approaching velocity component.
4. Speed renormalization: rescale every square's velocity to a fixed target speed, so collisions redirect motion but total liveliness never decays.
5. Advance the four base hues: one slow global rainbow phase (period around ten seconds), with the four squares offset by quarters of the wheel.
6. Recompute glitch/tear state: a coarse counter ticks many times a second for sparkles; a separate counter ticks at the user-set tear rate (up to about ten changes per second). From the tear counter, deterministically hash out three candidate bands: each gets a random row, a random signed horizontal shift up to about a third of the circumference, a random hue, and an on/off decision against the tear-probability control. All randomness here is a stateless hash of the counter (a sine-fract style hash), so a band holds perfectly still between ticks — that stability is what sells the "frozen corrupted frame" look.

## Per-pixel rendering
Recover (column, row) from the index as above and sample at the cell center. If this pixel's row matches an active tear band, add that band's shift (wrapped) to the sampling x — the physical mapping is untouched; only the sample position lies, which is what creates the tear illusion while squares slide through it.
- Evaluate each square's coverage at the (possibly shifted) sample point: a soft box function equal to one inside, fading linearly to zero within about a pixel of the border (computed from the minimum distance to the four edges, horizontal distance taken the short way around the cylinder).
- If any coverage is nonzero: take the square with the greatest coverage; hue = that square's base hue plus an offset proportional to the distance from the square's center (normalized by the half-diagonal) spanning over half the wheel at the corners; brightness = total coverage of all squares (clipped), plus a small boost inside tear bands; full saturation.
- Else if in an active tear band: paint the band's hue at very low saturation (near-white) at the user-set tear brightness.
- Else sparkles/streaks: each pixel has a per-tick hashed chance (the sparkle-rate control) of lighting at the sparkle brightness; additionally, one short horizontal streak (a few pixels wide, hashed row and center per tick) lights at a somewhat reduced brightness. Sparkle hue is hashed per pixel per tick (fully random confetti), with saturation from a control.
- Otherwise black.

## Colors
Squares: fully saturated spectrum hues, four-way symmetric around the wheel, slowly rotating, each with a radial hue gradient from its center. Tear bands: pale near-white with a faint random tint. Sparkles: random vivid (or desaturated, per control) confetti. Background: black.

## Controls (all sliders)
- Sparkle rate: probability of background sparkles (default off).
- Sparkle brightness (default off).
- Sparkle saturation: vivid confetti down to white noise.
- Tear chance: probability each of the three candidate bands is active per tick (default: rare).
- Tear rate: how many times per second the tear configuration re-rolls (up to around ten).
- Tear brightness: how visible empty torn rows are (default: medium).

## Timing
Squares cross the display in a few seconds; full hue rotation about ten seconds; tear configuration re-rolls several times per second; sparkles refresh many times per second.

## Non-obvious points
- Impulse collisions plus per-frame speed renormalization is the key trick: perfectly elastic, non-overlapping bounces with zero long-term energy drift.
- All horizontal math (positions, collision normals, edge distances) must use shortest-signed-distance around the cylinder or squares will pop at the seam.
- Tearing shifts the sample position, not the output mapping, so moving squares get convincingly sliced by torn rows.
- Deterministic hash-of-frame-counter randomness (not per-frame fresh randomness) makes glitches hold still between ticks instead of shimmering.
