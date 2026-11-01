# Tri March

Idea spec written from Patternflow preset preset_0716.h (CC BY-SA 4.0 per its SPDX header, by Seunghun Lee; the brief said MIT); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A lattice of separate, fully saturated triangles on a black background, alternately pointing up and down in a checkerboard arrangement, each inset in its own cell so black gaps separate them. The whole lattice marches steadily across the panel in one direction (any heading, set by a knob, and either way along it), like a conveyor of confetti. Each triangle pulses in brightness at its own phase, so a shimmering ripple of light passes through the marching formation, and each has its own hue. At the default palette setting neighbouring triangles have quite different hues — a rainbow confetti look; at the low end every triangle shares one slowly drifting hue, which looks calmer and more graphic. The triangle size runs from small (many triangles, busy) to large (a handful of bold shapes on the panel).

## The idea

A scrolling lattice of alternating triangles, each tile individually hashed for phase and hue. Simple geometric motion graphics: a moving pattern of shapes with per-tile shimmer.

## Algorithm in words

- State: a time accumulator that can run forward or backward.
- Per frame: triangle height from the size control; width = height × the equilateral ratio. Offset = heading direction (from the angle control) × time × a speed factor.
- Per pixel: subtract the offset, divide by cell width/height, take integer cell column and row and the local position within the cell (centred). Cells whose column+row parity is even hold an upward triangle, odd ones a downward triangle, defined by a pair of linear inequalities that leave a margin to the cell edges.
- Inside a triangle: a per-cell seed from a linear combination of column and row; brightness = sine of (seed + time), mapped to a mid-to-full range; hue = seed × the palette control + a little of the brightness + a slow time drift, wrapped. Full-saturation HSV. Outside: black.

## Knobs

- K1 Triangle size — cell size, from fine to bold.
- K2 Speed — march speed, reversible (negative values march backward).
- K3 March angle — heading of the march.
- K4 Color palette — how strongly hue varies between cells: one shared hue at the bottom, rainbow confetti higher up.
- Audio: none directly.

## Making it better in Luxel

- Tessellate properly: make the triangles share edges (a true triangular lattice) with a thin adjustable gap, so the pattern can go from separated confetti to a seamless mosaic. Offer square, hex and diamond lattices as modes.
- Replace the plain per-cell sine with a travelling wave or `simplex3` field sampled at cell centres, so shimmer moves through the lattice coherently (a wave of light rolling across tiles) rather than looking random; and add occasional per-tile "flip" events animated with `easeOutBack`.
- Colour via a palette or OKLCH so hue variety does not produce muddy or uneven brightness.
- Cost: hue and brightness are per-cell, so compute per cell and draw each triangle as a shape; in `renderBulk` a scanline fill per triangle (or `fillRect` strips) is far cheaper than per-pixel inequality tests on large panels.
- Strips: a 1D analogue (alternating short segments with gaps marching along the strip) is easy and works.

## Payoff

3/5 — crisp, cheerful motion graphics; simple but a good base for a lattice-shapes pattern with modes.
