# Spinning Crosses

Idea spec written from Patternflow preset preset_0515_4.h (CC BY-SA 4.0, Seunghun Lee); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A grid of square cells, each holding a plus-shaped cross (like a cog or a turnstile) that spins continuously about the cell centre. Neighbouring cells spin in opposite directions — checkerboard style — so the board looks like a meshing gear train or a field of counter-rotating propellers. The crosses' arms are filled with a solid colour (warm orange in one checkerboard colour, cyan in the other) and edged with a white outline; each cell has a dim violet square "housing" border. Because the clipping square rotates with the cross, the arm tips sweep beyond the cell and clip oddly at times, giving a mechanical, slightly clunky motion. High contrast, industrial, two complementary colours on black.

## The idea

Checkerboard of counter-rotating crosses — the classic "gear grid" optical toy. It works because opposite spin in adjacent cells reads instantly as meshing gears and creates illusory motion along the grid lines.

## Algorithm in words

- State: time accumulator from the speed knob.
- Cell size from K1, between small and large.
- Per pixel: find the cell and the pixel's offset from the cell centre. The rotation angle is time × a rate, negated on odd-parity cells.
- Rotate the offset by that angle. A point is on the cross if its smaller rotated coordinate is under the arm half-thickness and its larger rotated coordinate is within most of the half-cell.
- On the cross: if close to the arm edge, white outline; else fill colour — warm or cool depending on cell parity — at a value set by K4.
- A thin band just outside the cross's square extent (in rotated space) draws a dim violet frame.

## Knobs

- K1 — cell size (grid complexity).
- K2 — spin speed.
- K3 — arm thickness: thin lines to chunky blades.
- K4 — fill brightness of the arm interiors (edges stay white).
- No audio.

## Making it better in Luxel

- Actual gear look: make the cross a rounded cog (arm count as a control: 3–6 spokes) drawn with `drawLine` capsules per spoke in `renderBulk` — a few `drawLine` calls per cell instead of per-pixel rotation math, and anti-aliased for free.
- Keep the housing frame axis-aligned (not rotating) and clip arms to the cell so the mechanism looks precise; spoke tips of neighbours should just miss each other, which sells the meshing.
- Animate with stepped, eased rotation (tick a quarter turn with `easeOutBack`, pause, tick again) for a clockwork feel instead of constant spin.
- OKLCH warm/cool pair, glow on the white edges. Optionally rotate speed from `energyAverage`.
- Best form: one 2D pattern. Not for strips.

## Payoff

Score: 3/5 — fun mechanical illusion; needs anti-aliased spokes and stepped motion to look intentional.
