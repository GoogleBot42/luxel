# Rainstorm
kind: 2D
sensors: no

## What it looks like

Deep-blue rain streaking down a 2D display, with occasional lightning. The
rain is a set of vertical columns, each containing short bright streaks that
fall at their own speed — some columns fast and bright, others slow and dim —
so the whole field reads as convincing rainfall rather than a uniform
scroll. Every so often the scene "flashes": a burst of near-white light blooms
from the center of the display, briefly washing out the blue, then dies away
with a flicker. The rain angle, rain speed, streak density, and lightning
frequency are all adjustable.

## Algorithm

State between frames:
- A time accumulator advanced by elapsed time times a user-set rate, wrapped
  modulo a large value (about an hour) so speed changes never cause a visual
  jump and precision never degrades.
- A per-frame lightning intensity value.

Per frame:
1. Derive a fast-running rain clock as the accumulator times roughly ten.
2. **Lightning:** sample smooth (perlin-style) noise at coordinates that move
   with time (one axis moving much faster than the other), roughly double it,
   clamp to the unit range, and cube it. Cubing turns the mostly-mid-level
   noise into rare sharp spikes — that is the flash flicker. Then gate it: the
   flash is only allowed during the tail portion of a repeating cycle a few
   units long; a slider sets how much of each cycle is "flash-permitted", from
   never to always. Result: irregular, naturalistic lightning at a controllable
   average rate.
3. Set up the coordinate transform: center the origin, scale up by about two
   with the vertical axis flipped (so rain falls downward), and rotate by the
   user-chosen angle.

Per pixel (transformed coordinates):
- **Flash radius term:** compute (a constant around the diagonal half-length,
  times the frame's flash intensity) minus the pixel's distance from center.
  Positive near the center when a flash is active, growing with flash
  strength — this makes stronger strikes light a wider area.
- **Perspective trick to limit streak length:** compute the distance from the
  pixel to a virtual point a couple of units below the display, scale it by a
  small factor, add one, and multiply both coordinates by it. This slight
  radial stretch keeps the streaks from being unbroken full-height lines.
- **Column randomness:** quantize the x coordinate into a column index (the
  column width set by a slider) and use it to seed a deterministic
  pseudo-random generator, so every pixel in a column gets the same stable
  random draw each frame. From that draw derive a per-column value in the unit
  range (via one minus the sine of the draw) that acts as the column's
  "personality": it sets both the streak fall speed and the column's peak
  brightness, and also skews a spatial frequency term so different columns
  have differently spaced streaks.
- **Streaks:** brightness along the column is the absolute value of a sine of
  (rain clock times the column speed, plus y times the column's spatial
  frequency), clamped, multiplied by the column personality, then raised to
  the fourth power to sharpen the streaks into short bright dashes on a dark
  background.
- **Color:** hue is fixed at a pure blue. Saturation starts at full, is
  reduced slightly by streak brightness (so bright streak cores whiten a
  touch) and reduced strongly by the flash intensity (so lightning washes the
  whole scene toward white). Brightness is the streak value plus the flash
  intensity times the cube of the flash radius term — the cube concentrates
  the bloom near the center with a soft falloff.

## Controls (all sliders)

- **Speed:** rain fall rate, mapped between a slow drizzle and a fast downpour.
- **Angle:** rotates the entire scene through a full turn, letting rain fall
  at any slant.
- **Scale:** sets the streak column width (inverted: slider right = wider,
  fewer columns; the range spans roughly a factor of three or four).
- **Lightning:** sets the fraction of each cycle in which flashes may occur,
  from none to nearly continuous.

## Timing feel

Streaks traverse the display in roughly a second at default speed. Lightning
arrives in irregular clusters, typically a flash or two every several seconds
at middling slider settings, each flash lasting a fraction of a second with a
natural flicker.

## Layout assumptions

Requires a mapped 2D display; no pixel-count hardcoding. Works at any
resolution, though streaks need a reasonable number of rows to read as rain.
