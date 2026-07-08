# ChristmasPewPew
kind: 1D
sensors: no

## Overview / what it looks like
A Christmas-colored re-skin of a "laser volley" comet pattern. A handful of bright green and red projectiles ("lasers") continuously fly along the strip from start to end at differing random speeds, each dragging a fading tail behind it. The whole strip additionally glows with a faint constant deep-red ambient, so "empty" pixels are dim red rather than black. Where projectiles overlap, their colors add and can wash toward brighter/whiter. The feel is a steady stream of pew-pew shots — each shot takes on the order of a second or a few to cross the strip, faster ones and slower ones mixed.

## State kept between frames
- A persistent per-pixel color buffer (the trail canvas), one packed color per pixel.
- Per projectile (default: eight of them): a fractional position along the strip, a velocity, and a fixed color.

## Colors
A small round-robin palette assigns each projectile either a medium-dim pure green or a medium-dim pure red; in this fork the mix is weighted toward green (roughly five green to three red out of eight). The ambient wash is a much fainter pure red. Qualitatively: red and green comets with exponentially fading tails over a faint red underglow. Because the projectile colors are dim and drawing is additive with saturation clamping, repeated overlap brightens without hue-shifting badly.

## Per-frame work (all in the pre-render step)
1. **Fade pass:** every pixel in the trail buffer has each color channel multiplied by a decay constant a bit below one (roughly one-fifth of the remaining brightness lost per frame — frame-rate dependent by design), with truncation toward zero so trails do eventually hit black.
2. **Advance and draw each projectile:** the new position is old position plus (elapsed milliseconds × a small speed scale × that projectile's velocity). Velocities are drawn uniformly from about one to several units, so speeds vary several-fold between shots. Crucially, the projectile is not drawn as a single pixel: every integer pixel index it passed over since the previous frame (from just past the old position up to the new position) gets the projectile's color **added** channel-wise into the trail buffer, clamped at channel maximum. This gap-filling makes fast shots draw continuous streaks instead of dotted lines.
3. **Respawn:** when a projectile's position passes the end of the strip, it resets to position zero and rolls a fresh random velocity (color is kept for life).

## Per-pixel render
Output = trail buffer value plus the constant ambient color, per channel, clamped, then scaled from byte range down to unit range and emitted as RGB (not HSV).

## Layout assumptions
Pure 1D by pixel index; correctly uses the runtime pixel count everywhere (spawn positions, wrap check, buffer size). No hardcoding to fix. On a 2D/3D mapping it would just follow wiring order.

## Controls
None exported; the natural knobs already isolated as top-of-file constants are projectile count, tail decay rate, and overall speed scale — obvious slider candidates.

## Non-obvious implementation details
- Channel packing: each pixel's color is stored as a single number with the three byte-sized channels packed via bit shifts — and it deliberately exploits the platform's fixed-point number format (fraction bits below the integer point) to hold the third channel in the fractional part. A reimplementation on a platform with real integers/floats should just use a plain packed integer or three arrays; the fixed-point trick is an artifact, not essential.
- All animation happens in the pre-render step; the renderer is a pure read of the buffer, which keeps per-pixel work trivial.
- Fading is per-frame (not per-millisecond), so tail length varies with frame rate; a faithful-feel port may want to make the decay delta-scaled.
