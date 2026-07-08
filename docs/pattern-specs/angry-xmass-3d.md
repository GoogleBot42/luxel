# Angry Xmass 3D
kind: 3D (render3D only; no 1D/2D fallback — on a strip without a 3D map it will not display)
sensors: no

## What it looks like
An aggressive, strobing Christmas effect on a 3D-mapped installation (e.g. a mapped tree).
Space is carved into hard-edged bands of red and green (the classic Christmas pair) with thin
flashing white streaks, and the whole thing flickers violently: every single frame the pattern
randomly re-picks which spatial axis the bands are aligned to, so the band orientation jitters
chaotically between three variants many times per second. Combined with very fast phase motion,
the result reads as an angry, glitchy red/green strobe — the name is accurate. Not epilepsy-
friendly; a reimplementer may want to note that.

## Algorithm
Per frame:
- Advance three sawtooth phases from the built-in timebase: one slow (several seconds per
  cycle — computed but never actually used), one fast (a fraction of a second per cycle), and
  one extremely fast (a few hundredths of a second per cycle).
- Draw one uniform random number and use it to pick, with equal probability, one of three
  variants. Because this happens every frame with no persistence, the variant changes
  essentially every frame.

Per pixel (given normalized x, y, z world coordinates), all three variants share the same
structure and differ only in which axes they read:
- Hue: take a square wave (roughly half duty cycle) of (a chosen axis + the fast phase). The
  square wave yields only two levels, and the low level is clamped up to about a third of the
  hue circle — so hue is binary: green-region or red-region. This produces hard spatial bands of
  red and green sliding along the chosen axis.
- Saturation: a square wave with a very high duty cycle (on ~nine tenths of the time) of
  (a chosen axis + the ultra-fast phase). Saturation is full almost everywhere, but drops to
  zero in a thin slice that races along its axis, producing fleeting white streaks/flashes.
- Brightness: a triangle wave of (the product of all three coordinates x·y·z + the fast phase).
  The coordinate product creates curved, hyperbola-like interference sheets of light and dark
  through the volume, animating with the fast phase.

Variant differences: one variant runs both hue and saturation along the depth axis; another runs
both along the first horizontal axis; the third mixes them (hue along depth, saturation along
the vertical axis). Brightness uses the coordinate product in all variants.

## Colors
Only two hues ever appear — a green and a red (the two Christmas primaries) — plus white where
the saturation slice hits, and black in the dark parts of the brightness interference pattern.

## Controls
None.

## Timing feel
Everything is fast: the band phase scrolls in a fraction of a second per cycle, the white slice
cycles many times per second, and the axis-variant reshuffles every frame. The one slow phase in
the code is dead weight.

## Layout assumptions
Requires a 3D pixel map with normalized coordinates. Nothing depends on pixel count.

## Notes for reimplementation
- The per-frame random variant selection is the core of the "angry" character; do not smooth or
  slow it if fidelity is the goal (though a rate control would be an obvious kind improvement).
- The comparison chain for the selector technically leaves a gap exactly at the two boundary
  values (no branch fires there), skipping a frame's draw for those measure-zero cases; a clean
  port can just use inclusive ranges.
- Mind the clamp trick for hue: max() of a two-level square wave against a constant of about a
  third is what turns "0 or 1" into "green or red" (hue wraps, so the high level equals red).
