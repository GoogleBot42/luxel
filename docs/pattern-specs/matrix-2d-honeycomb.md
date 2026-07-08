# matrix 2D honeycomb
kind: 1D (self-mapped to a 2D matrix; should be ported to a true 2D renderer)
sensors: no

## What it looks like
A classic full-rainbow "plasma" on a small LED matrix. Soft cellular blobs of color drift and morph across the panel. Each blob region shimmers in brightness with a fairly quick pulse (a few seconds per pulse), while the overall shape of the pattern warps slowly — the spatial scale of the blobs breathes in and out over tens of seconds to a minute, and the whole palette slowly rotates around the color wheel on a similar slow timescale. Brightness contrast is high: bright saturated ridges separated by near-black valleys, because brightness is sharply gamma-boosted.

## Layout assumptions (important)
The pattern is written as a plain per-index renderer but internally converts the pixel index into (column, row) coordinates of a rectangular matrix:

- The matrix width is hardcoded as a source constant (eight columns in the original).
- A hardcoded boolean chooses between straight and serpentine (zigzag) wiring: when serpentine, every other row's column coordinate is mirrored.

Obvious fix for a reimplementation: implement it as a 2D renderer that receives normalized x/y from the pixel mapper, dropping both the width constant and the zigzag flag. (Despite the name, there is nothing honeycomb-specific — it's a rectangular row/column decode.)

## State between frames
No persistent state. Each frame recomputes a handful of slow oscillator values from the global clock:

- Two phase oscillators, each a triangle-smoothed wave of a slow clock scaled up to a full circle in radians. Their periods differ slightly (both on the order of a minute) so the pattern never exactly repeats.
- A "spatial frequency" value that breathes between roughly two and seven, on a timescale of tens of seconds. This controls how many blob cells fit across the panel.
- A slow unit-range oscillator (tens of seconds) used as a hue offset — this is what rotates the palette.
- A faster sawtooth (a few seconds per cycle) used as a phase for the brightness shimmer.

No randomness anywhere; the motion is entirely quasi-periodic beating between incommensurate oscillator periods.

## Per-pixel work
For each pixel, after decoding (x, y) as fractions of the matrix width:

1. Compute a scalar plasma field: the average-ish combination of one, plus the sine of (x times the spatial frequency, phase-shifted by the first oscillator), plus the cosine of (y times the spatial frequency, phase-shifted by the second oscillator), all halved. This yields a smoothly varying field roughly in the 0..1½ range.
2. Brightness: take a triangle-smoothed wave of (field + the fast sawtooth phase), then cube it. The cubing crushes mid values toward black, producing crisp bright cells on a dark background; the added sawtooth makes each iso-contour of the field pulse in brightness every few seconds.
3. Hue: fold the fractional part of the field with a triangle wave (so hue sweeps up then back down across the field rather than wrapping abruptly), halve it (limiting the instantaneous spread to about half the color wheel), then add the slow rotating hue offset.
4. Emit fully saturated HSV color.

## Colors
Full spectrum over time. At any instant the panel shows roughly half the color wheel spread across the blob field, fully saturated, and that window slides continuously around the entire wheel over tens of seconds. Dark regions are simply low-brightness, not desaturated.

## UI controls
None.

## Non-obvious bits
- Folding the plasma field with a triangle wave before using it as hue avoids the ugly hard seam you'd get where a wrapped hue jumps from end back to start.
- Using the same field for both hue and (phase-shifted, cubed) brightness makes the bright cells track the color cells, giving the "blobby" coherent look.
