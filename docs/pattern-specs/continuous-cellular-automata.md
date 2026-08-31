# Continuous Cellular Automata
kind: 2D (requires a mapped matrix; concept credited to Wolfram's continuous-valued cellular automata)
sensors: no
status: DIVERGED (2026-08-31) — per Jeremy's review the port was reworked into a
self-evolving live waterfall (ring topology, drifting rule offset, convergence
watchdog). The three-parent averaging rule, value→hue mapping, hue drift and
dark-favouring brightness curve below still apply; the hidden oversized grid,
row re-derivation, EMA smoothing, Pan/Depth viewport, difference mode and seed
toggle no longer describe the shipped pattern.

## What it looks like
An ever-evolving field of nested triangular / fractal-like structures, the signature look of Wolfram-style automata rendered in continuous grayscale-turned-color. Bright filaments and dark voids form branching, self-similar shapes that shimmer and slowly morph as the automaton continuously re-derives itself. Brightness encodes cell value; hue is tied to cell value as well and additionally drifts slowly around part of the color wheel over roughly half a minute, so the whole tapestry gradually changes color family. Updates are deliberately smoothed, so instead of hard row-by-row stepping you see a gentle continuous melt.

## Algorithm
### The automaton
- State: a 2D grid of cells, each holding a value in 0..1, kept between frames. The grid is substantially larger than the display:
  - Its rows number about twice the displayed rows.
  - Its columns number the displayed columns plus two extra margin regions, each as wide as the number of grid rows. Rationale: information propagates one column per row (diagonally), so this margin guarantees edge artifacts can never reach the displayed viewport.
- Rule: each cell in a row is computed from the three cells above it (upper-left, upper, upper-right): average the three parents, optionally scale by a multiplier (nominally one), add a user-set offset in 0..1, then keep only the fractional part. The fractional wrap is what creates the chaotic/fractal structure.
- The first and last columns of each row can't use three parents; they use a weighted average of the two available cells above (double weight on the directly-above cell), with no offset or wrap. (These live in the hidden margin anyway.)
- Seeding (top row): either a single maximal-value cell at the center of an otherwise-zero row (classic; yields perfectly symmetric patterns), or every cell set to an independent uniform random value. Re-seeding happens when the seed-mode control crosses its midpoint.
- Amortized computation: only ONE row is recomputed per frame, cycling round-robin from the second row to the last and wrapping back. Since each row derives from the one above, the whole grid continuously re-derives itself from the (persistent) seed row over successive frames. This is done to stay within per-frame execution budget; it also gives the pattern its living, rolling quality.

### Display smoothing
- A second, display-sized grid holds an exponential moving average: each frame, every viewport cell blends a modest fraction (around a tenth) of the current automaton value into its previous smoothed value. This hides the row-scan updates and makes changes melt smoothly. A blend fraction in the several-percent range is noted as especially pleasing.

### Rendering (per mapped pixel)
- Map the pixel's normalized x/y to a smoothed-grid cell (floor to integer row/column against the physical matrix dimensions).
- Value: either the smoothed cell value directly, or, in "difference mode", the absolute difference between the cell and its left neighbor (first column shows black) — this suppresses the broad vertical stripes the rule tends to produce and highlights structure edges.
- Brightness is the value squared (dark cells go very dark; structure pops).
- Hue: start from a base that decreases as value rises (spanning roughly a third of the wheel), add the slow global drift phase, wrap, then pass through a sinusoidal easing (a triangle-of-the-wheel remap centered on the hue) that compresses the ends of the range — the net effect is a smooth, band-limited palette rather than a full hard rainbow: bright cells and dim cells sit at different but related hues, all sliding together over time.

## Colors
Full saturation throughout. At any instant the palette spans a limited arc of related hues (e.g. drifting from oranges-into-greens, later blues-into-purples), with brightness from black through mid-tones to full. Reads as monochrome-plus: value-structure first, color wash second.

## Controls (all sliders)
- Rule offset ("Param"): the amount added before the fractional wrap — the single most powerful control; different values give dramatically different automaton families (stripes, triangles, chaos).
- Pan: shifts the displayed viewport horizontally across the wider hidden grid.
- Depth: shifts the viewport vertically down the hidden grid (later generations).
- Difference mode ("Elim Stripes"): treated as a toggle (on above midpoint) — show neighbor deltas instead of raw values.
- Random seeds: treated as a toggle; flipping it re-seeds the top row (random noise vs. the single centered dot).

## Timing
- Hue drift: one slow cycle in roughly half a minute.
- Structural evolution: one grid row updates per frame, so a full re-derivation of the grid takes a few dozen frames — at typical frame rates the pattern visibly reorganizes over a second or so, smoothed by the fade.

## Notes / hardcoding
- The physical matrix dimensions are hardcoded (a mid-teens-square matrix). Obvious fix: derive them from the pixel map or expose them as constants; hidden-grid sizes are already expressed in terms of them.
- The original notes two available optimizations (halve work for symmetric seeds; skip cells that can't influence the viewport) — neither is implemented.
- Total array storage is sized to fit the platform's array-element budget; a reimplementation with different matrix sizes should keep (hidden grid + smoothing grid) within whatever budget applies.
