# Pulse Dot Grid

Idea spec written from Patternflow preset preset_0528.h (CC BY-SA 4.0, Seunghun Lee); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A halftone grid of round dots, one per square cell. Each dot's size swells and shrinks with a smooth wave that travels diagonally across the grid, so bands of big dots sweep over the panel like a pulse through an LED stadium screen or a halftone ripple. Cells whose energy falls below a cut-off are completely dark, so the background is stark black and the wave appears as travelling clusters of dots. Colour alternates in a checkerboard: warm orange-to-yellow dots in one set, emerald-to-teal dots in the other, both brightening with energy; the centre of each large dot burns hot white. An optional "chaos" term adds a high-frequency per-cell jitter so neighbouring dots no longer follow the wave cleanly — the grid starts to flicker and sparkle. Bold, graphic, rhythmic.

## The idea

Halftone dots whose radius is driven by a travelling wave — the classic "dot screen" animation. It works because a grid of size-modulated dots reads as an image of the wave, with strong graphic impact on coarse LED grids.

## Algorithm in words

- State: time accumulator from the speed knob.
- Cell size from K1, between small and medium.
- Per cell: energy = a sine of (a weighted sum of column and row indices + time), mapped to 0..1 — a diagonal travelling wave. If chaos is up, add a faster, steeper per-cell sine (different weights, opposite direction) scaled by chaos, clamp.
- If energy is below the sparsity threshold, the cell is dark.
- Otherwise, for each pixel in the cell: compute the squared distance from the cell centre; it is inside the dot if under a radius that grows with energy. An edge profile (one at the centre to zero at the rim) shapes colour.
- Even-parity cells: red full, green rising with energy, a touch of blue at the core → orange to yellow. Odd cells: green full, blue rising with energy, a little red at the core → emerald to teal.
- The innermost part of each dot is forced to white.

## Knobs

- K1 — cell size.
- K2 — wave speed.
- K3 — sparsity threshold: low fills the grid with dots, high leaves only the crests.
- K4 — chaos: adds jittery per-cell noise.
- No audio.

## Making it better in Luxel

- `renderBulk` with one `splat` (soft) or `fillCircle` (hard) per cell, plus a smaller white `splat` for the core — a few hundred ops per frame, no per-pixel distance tests; radius sub-pixel smooth.
- Drive energy with `simplex3` or a radial wave from a wandering centre for richer motion; a second wave in the other colour set at a different speed gives interference between the two checkerboards.
- Colour via two OKLCH ramps (or a palette per parity) so warm and cool sets have equal perceived brightness.
- Audio-reactive: map `frequencyData` bins to columns (a dot-matrix spectrum analyser) — a natural, very effective mode.
- Overlaps with Wave Cell Grid (square cells, same diagonal wave) — combine into one "cell wave" pattern with a dot/square shape option. Strips: a 1D version is a pulsing dashed line — fine but plain.

## Payoff

Score: 3/5 — good-looking halftone pulse and an excellent base for a dot-matrix audio visualiser.
