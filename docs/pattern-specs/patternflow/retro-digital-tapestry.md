# Retro Digital Tapestry

Idea spec written from Patternflow preset preset_0628.h (CC BY-SA 4.0 per its SPDX header, by Seunghun Lee; the brief said MIT); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A chunky, blocky field of glowing cells on a near-black background, like a woven textile drawn by a demoscene coder. The panel is cut into square cells a few pixels across; each cell is either "on" (lit with a saturated, slightly pastel colour) or "off" (black, with only a faint dark-violet grid line along its top and left edges, so the empty areas read as graph paper). Which cells are lit comes from integer bit logic on the cell's column and row, so the lit cells form the classic self-similar bitwise textures: XOR plaid and Sierpinski-like triangles, diagonal stripe lattices, nested squares, concentric rounded blocks.

Motion is in two layers. The *structure* steps: an integer clock ticks a few times a second and shifts the bit pattern, so blocks flip on and off in discrete jumps — a mechanical, marquee-like rhythm. The *colour* flows: a smooth travelling interference of a horizontal and a vertical sine wave sweeps hue across the lit cells, so the blocky structure is washed by soft diagonal rainbows. Saturation also breathes slowly, so the whole thing periodically goes slightly milky then vivid again. On a 128x64 panel it fills edge to edge with no focal point; it reads as a living wallpaper. It is compelling because of the contrast between hard digital stepping and soft analogue colour; it can feel busy at small cell sizes.

## The idea

Light a grid of coarse cells using a bitwise function of the cell coordinates plus a stepped clock, and colour the survivors with a smooth sine-interference hue field. Bit logic gives intricate fractal-ish structure for free; the smooth hue keeps it from looking like a test pattern.

## Algorithm in words

- State: one time accumulator driven by the speed control.
- Per frame: derive the cell size from the scale control, the logic mode from the mode control, and a spatial frequency for the hue waves from the wave control. Compute a few integer "ticks" from time at slightly different rates (each mode uses one).
- Per pixel: find the cell column and row. Combine them with the chosen mode's rule to get an integer: XOR of column and row plus tick; AND of column and row (scaled) plus a faster tick; a weighted sum of column and row XOR'd with the tick; column XOR (row plus tick) masked to a small range; or a quantized squared distance from the corner plus a slow tick (blocky concentric arcs).
- Test one fixed middle bit of that integer. If set, the cell is lit: hue = a base offset + a smooth sine(x)·cos(y) travelling wave + a small offset taken from the low bits of the integer (so neighbouring lit cells differ slightly in hue). Value is high-but-not-full; saturation follows a slow sine so it breathes.
- If not set, the pixel is black, except on the cell's top/left edge where a very dim violet grid line is drawn.

## Knobs

- K1 Cell Scale — size of the blocks, from tiny (dense, noisy) to large (bold, few cells).
- K2 Speed — how fast the bit pattern ticks and the colour waves travel.
- K3 Logic Mode — selects which bitwise rule builds the texture (stepping through a handful of distinct looks).
- K4 Wave Mod — spatial frequency of the colour interference, from broad washes to tight rainbow banding.
- Audio: none directly; under Patternflow's audio edition sound can be mapped to any knob (Speed is the natural target).

## Making it better in Luxel

- Make the logic mode a real enumerated control and add more rules (OR, multiplication modulo, column·row modulo a prime, Munching-squares style comparisons with the tick). Crossfade between modes instead of hard-switching.
- Colour with OKLCH rather than HSV so the lit cells have even perceived brightness across hues; drive hue from a small palette (`setPalette`/`paint`) so the look can be retro-CGA, amber terminal, or vapor pastel.
- Cost: the per-pixel work is integer per *cell*, not per pixel. Compute one colour per cell into a small canvas (cells are at least several pixels wide) and upscale with `fillCanvas`/`blit` in `renderBulk`, drawing the grid lines with `fillRect` strokes — far cheaper on a 128x128 chain than per-pixel trig.
- Ease the bit-pattern steps (a short fade on each tick using an easing curve) so cells flip with a soft flash rather than a hard cut; optional `setGlow` makes lit cells bleed slightly over the grid lines.
- Overlaps the existing `xorcery-2d-3d` library pattern — consider merging them as modes of one "bit tapestry" pattern. Strips: a 1D variant (bit logic on pixel index and tick) works as a "data stream" strip pattern but loses most of the charm.

## Payoff

3/5 — a solid, cheap, retro-digital wallpaper; best merged with the existing XOR pattern as one multi-mode piece.
