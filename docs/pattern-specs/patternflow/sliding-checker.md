# Sliding Checker

Idea spec written from Patternflow preset preset_0513.h (CC BY-SA 4.0, Seunghun Lee); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A checkerboard of square tiles where the two tile colours are not fills but *glyphs*: one set of tiles shows a small solid square of colour floating in its centre, the alternate set shows a thin white frame hugging its border. Each horizontal row of tiles slides left and right as a unit along a sine, with the phase stepping from row to row, so the board ripples like an accordion or a row of sliding drawers: rows lag one another, the checkerboard shears into staggered bricks and then realigns. Bold, graphic, two-tone (one hue + white on black), very "op-art / Bauhaus". On a 128x64 panel it is a field of a few dozen to a few hundred tiles depending on tile size.

## The idea

A checkerboard whose alternating cells are "filled dot" and "hollow frame", with whole rows of cells oscillating horizontally out of phase. The contrast of solid versus outline gives a crisp graphic texture, and the per-row phase lag makes a simple sine feel like a wave travelling down the board.

## Algorithm in words

- State: time accumulator; all four knob values are eased toward their targets each frame (a quick exponential follow, hue taken the short way round the wheel) so turning a knob glides rather than jumps.
- Time advances at a small base rate plus the speed knob, so the board never quite stops.
- Tile size is interpolated between large and small by the frequency knob.
- For each row of tiles: horizontal shift = sine of (row index × a step + time) × tile size × the shift-amount knob, snapped to whole pixels. Shifts can reach several tiles at the top of the knob.
- For each pixel: apply its row's shift to x, find the tile and the position inside it (centred, −½..½).
- Tiles with even (column + row) parity: light only a centred square covering a bit over half the tile, in the chosen hue.
- Odd tiles: light only a thin border band near the tile edges, in white.

## Knobs

- K1 — hue of the solid squares.
- K2 — speed of the row oscillation.
- K3 — shift amount: zero is a still checkerboard; up high the rows slide multiple tiles and the board becomes a chaotic brick shear.
- K4 — tile size, from big chunky tiles to tiny ones.
- All four glide smoothly to new values. No audio.

## Making it better in Luxel

- Draw it with `renderBulk`: per tile, one `fillRect` (square) or four thin `fillRect`s (frame) — a few hundred shape ops a frame instead of per-pixel parity tests; at large tiles it is nearly free.
- Ease the row motion with something more characterful than a sine: per-row `easeInOutBack` or `easeOutBounce` slides that start staggered (a "drawer cascade"), holding at aligned positions between moves.
- Shift columns as well as rows on alternate cycles, or let the inner square rotate/scale with the row's velocity, for a more kinetic piece.
- Two-hue OKLCH scheme (complementary pair) instead of hue + pure white for a softer variant; add sub-pixel offsets with anti-aliased edges on 128x128 for smoother motion.
- Best form: a single 2D pattern; the knob-smoothing idea is worth adopting generally. Not for strips (a 1D version is just sliding dashes).

## Payoff

Score: 3/5 — clean, bold op-art that is cheap to render; benefits a lot from eased, staggered motion.
