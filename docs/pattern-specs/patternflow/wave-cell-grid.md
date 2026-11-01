# Wave Cell Grid

Idea spec written from Patternflow preset preset_0601.h (CC BY-SA 4.0, Seunghun Lee); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A grid of square cells lights on and off in a wave that travels diagonally across the board. Lit cells are filled with a colour that cycles through a cosine rainbow (each cell's colour follows its own wave phase, so a band of lit cells is a rainbow gradient) and their one-pixel border shines at full brightness, giving a tiled "window panes" look; the fill brightens the further above the threshold the cell's signal is. Alternate columns can be pushed out of phase with their neighbours, so the wave breaks into a staggered zipper pattern. Unlit cells are black, except that a faint cyan scan line sweeps horizontally across the panel, lighting the borders of dark cells only where it passes — like a radar sweep revealing the empty grid. Clean, architectural, slightly "building façade / stadium board".

## The idea

A grid of cells gated by a travelling wave, with rainbow phase colouring and an independent scan line revealing inactive cells. It works because the gating makes clear blocky motion and the scan line keeps the empty grid alive.

## Algorithm in words

- State: time accumulator from the speed knob.
- Cell size from K1 (turning K1 up makes cells smaller).
- Per cell: phase = time + weighted column + weighted row; even columns add the phase-split knob. Signal = sine of phase mapped to 0..1.
- Per pixel: determine whether it is on the cell's border (first or last row/column of the cell).
- If signal is above the threshold (K3): brightness = full on the border, otherwise a base level plus a share of how far above threshold. Colour = three phase-shifted sines of (phase + a little per column) for red, green, blue, times brightness.
- Else: a horizontal scan wave (sine of x minus time, faster than the cell wave); where it is near its peak, border pixels get a dim cyan.

## Knobs

- K1 — grid density (bigger knob = smaller cells).
- K2 — speed.
- K3 — threshold: low lights most cells, high lets only crests through.
- K4 — phase split between alternate columns.
- No audio.

## Making it better in Luxel

- `renderBulk`: one `fillRect` per lit cell for the fill and four thin rects (or one rect then a smaller dark rect) for the border; scan line as one `fillRect` column band — a few hundred ops instead of per-pixel work.
- Fade cells in and out with an easing on the signal instead of a hard gate, and give lit cells an OKLCH palette (rainbow at constant lightness, or a two-colour gradient) rather than the RGB cosine rainbow, which has uneven brightness.
- Leave a short `fade()` afterglow when a cell switches off — the board looks much more physical.
- Audio: map spectrum bins to columns (cell height as level) for a blocky equaliser mode.
- Near-duplicate of Pulse Dot Grid (same diagonal wave over a cell grid): combine into one "cell wave" pattern with a cell-shape option (dot / square / frame). Strips: 1D segments lighting in a travelling wave — fine. Main form: 2D pattern.

## Payoff

Score: 2/5 — tidy but generic; worth keeping only as a shape mode of a combined cell-wave pattern.
