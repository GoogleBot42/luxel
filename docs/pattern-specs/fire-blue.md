# fire - blue
kind: 1D
sensors: no

## What it looks like
A classic flickering fire simulation along a strip, but recolored cold: instead of red-orange flames, tongues of icy blue light lick up the strip. The base of the "flame" glows steadily blue-white to white, tapering through cyan and deep blue into black toward the far end, with random sparks and flare-ups making the flame height dance and shimmer many times per second. It reads unmistakably as fire, just gas-flame blue.

## Algorithm
This is the well-known one-dimensional cellular "heat" fire simulation (the Fire2012-style algorithm), stepped at a fixed tick rate rather than every render frame.

### State kept between frames
- A per-cell "heat" array (one value in the unit range per simulated pixel).
- A time accumulator that gates simulation steps.

### Per simulation tick (roughly fifty times per second)
Elapsed frame time accumulates; whenever it crosses a small fixed step (a few hundredths of a second) one simulation tick runs, in three phases:
1. Cooling — every cell loses a random amount of heat, uniformly drawn up to a small cooling constant, then clamped to the unit range. (The symmetric modes, below, use a somewhat larger cooling constant to compensate for the shorter simulated strip.)
2. Upward drift — iterating from the far end toward the base, each cell is replaced by a weighted average of the two cells below it, with the cell two-below counted twice. This convects and diffuses heat away from the base, forming rising tongues.
3. Sparking — with a roughly coin-flip probability per tick, a random cell within roughly the first tenth of the strip receives a large heat injection: a guaranteed substantial minimum plus a random extra, clamped to full. These sparks are what feed the flame.

### Per pixel (render, every frame)
The pixel's heat value is scaled into an index into a precomputed palette (a couple hundred entries) and the palette color is emitted as RGB. Rendering happens every frame but only reads the heat array; the simulation only advances on its fixed tick.

### Direction/symmetry modes
A compile-time mode constant selects how simulated cells map to physical pixels:
- flame rising from the start of the strip (identity mapping);
- flame rising from the end (reversed mapping);
- symmetric from both ends toward the middle, or from the middle toward both ends — in these two modes only half the pixel count is simulated and the same half is mirrored onto both sides of the strip.

## Colors
The palette is a "heat ramp" recolored to blue, built once at startup as three parallel channel lookup tables. Qualitatively, in ascending heat order: black, through deepening pure blue, then blue brightening through cyan (the second channel ramps in), then on to pure white (the last channel ramps in over the hottest third). So: black → deep blue → vivid blue → cyan → white-hot. Each channel ramps linearly over one third of the palette and then holds at full.

## Controls / configuration
No slider UI. Four tune-in-code constants: cooling rate (higher = shorter flames), spark probability (higher = busier, taller fire), simulation tick length (lower = faster flicker), and the direction/symmetry mode. A reimplementation could sensibly expose the first three as sliders and the mode as a picker.

## Timing
Simulation ticks tens of times per second, giving fast, organic flicker; the flame's overall shape churns continuously with no repeating period.

## Layout assumptions
Fully driven by the runtime pixel count (comments note it was tuned on a strip of several dozen pixels); on much longer strips the fixed cooling rate will make flames proportionally shorter relative to strip length. Obvious fix: scale the cooling constant inversely with pixel count, or expose it as a control.

## Non-obvious notes
- Decoupling simulation ticks from render frames keeps flame speed consistent regardless of frame rate and lets fast hardware render smoothly between ticks.
- Restricting sparks to the bottom tenth of the strip plus the double-weighted lower neighbor in the drift phase is what creates the strong directional "rising" character.
- Heat values live in the unit range throughout; only the palette lookup quantizes them.
