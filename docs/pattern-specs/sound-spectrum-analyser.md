# Sound - Spectrum Analyser
kind: 2D (matrix required; no 1D fallback)
sensors: yes

## What it looks like
A classic music spectrum analyser on an LED matrix: one vertical bar per column, bass on the left, treble on the right. Bars bounce with the music. Each column also carries a floating "peak dot" — a single white pixel marking the recent maximum of that bar, which sinks steadily when the music drops. Bar colors are a rainbow spread horizontally across the matrix, and the whole rainbow scrolls sideways quickly (a full hue cycle in around a second), so the bars continuously shift color. Bars auto-scale: after a few seconds of quiet or loud material the display adapts so the tallest bar rides near (but not at) the top.

## Conceptual inputs
- A frequency-spectrum array from the sound sensor: a few dozen bands (order of 32) of energy values, low frequencies first.

## Algorithm
Layout assumptions: the matrix width in pixels is a hardcoded number (a couple dozen in the original) and height is derived as total pixel count divided by width. The map is expected to be a grid with origin at top-left, so the renderer flips the vertical coordinate to draw bars growing upward from the bottom. Obvious fix: expose width as a config value or UI control instead of hardcoding, or derive it from map metadata if available.

State kept between frames, all sized to the column count:
- current bar heights (in whole rows),
- peak positions per column,
- a millisecond accumulator for peak decay,
- auto-gain state: a slow rolling average of the loudest bar, plus a small proportional-integral controller state (gain term, integral accumulator with clamped range).

Per frame:
1. Auto gain: compute a sensitivity multiplier via a PI controller whose error is (target fullness minus rolling average of the loudest bar). Target fullness is high but below full — around nine-tenths — so the loudest bar hovers near the top without constant clipping. Sensitivity is floored at unity so silence doesn't amplify noise into a full display.
2. Peak decay: an accumulator of elapsed milliseconds; every tenth of a second or so, every column's peak marker drops by one row.
3. For each column: map the column to a spectrum band using a logarithmic curve (log of one plus the column's normalized position, scaled to the band count). This gives bass more columns than a linear mapping would — perceptually much better. Multiply that band's energy by the sensitivity, clamp to full scale, and quantize to whole rows to get the bar height. Raise the column's peak marker to just under the bar top if the bar exceeds it. Track the frame's loudest (pre-clamp) bar and fold it into the rolling average with a slow exponential blend (time constant of a couple seconds at typical frame rates).

Per pixel (2D): convert normalized coordinates to integer column and row (row flipped so zero is the bottom). Hue is the pixel's horizontal position plus a fast-cycling time phase, wrapped. The pixel is lit if it is below the bar's fill height, or if it sits exactly on the column's peak row. Saturation is full except on the peak row, where it drops to zero — that is what makes peak dots white. Unlit pixels are black.

## Colors
Full-saturation rainbow sweeping horizontally and scrolling with time; white peak dots; black background. Qualitative only — no fixed palette.

## Controls
None in the UI; the hardcoded matrix width is the one thing a user must edit.

## Timing
Bars respond essentially instantly to sound. Peak dots fall about ten rows per second. The rainbow completes a hue cycle in roughly a second. Auto-gain adapts over a few seconds.

## Non-obvious bits
- The PI-controller auto-gain is the heart of the pattern: it makes the display usable across wildly different volumes with no sensitivity knob. The integral term is clamped to a bounded range to prevent windup.
- The log-curve column-to-band mapping compensates for the linear spacing of the spectrum bands.
- Peak dots are drawn by zeroing saturation on exactly one row, and that row stays lit even when the bar beneath it has fallen — the white dot floats above the bar.
