# Example: modes and waveforms
kind: 1D
sensors: no

## What it looks like
An educational demo, not a decorative effect. The strip displays a static grayscale (white-on-black) picture of a waveform — the shape repeats about four times along the strip's length — and roughly every two-thirds of a second it snaps to the next waveform in a fixed list of about a dozen. There is no motion within a mode; the show is the succession of different brightness shapes: ramps, triangles, smooth sine humps, hard on/off bars, dash-dot groupings, beat-frequency interference patterns, and so on.

## Algorithm
State between frames: an accumulated-elapsed-time counter and the current mode index.

Per frame: add the frame's elapsed time to the counter; when it exceeds roughly two-thirds of a second, subtract that threshold and advance the mode index, wrapping around the list. (A commented-out line pins the mode for study; not needed in a reimplementation, though a debug pin is a nice touch.)

Per pixel: map the pixel's normalized position to an input spanning zero to about four (so the waveform repeats about four times across the strip), feed it to the current mode's shaping function, and use the result directly as the brightness of an uncolored (white) pixel — hue and saturation are zero.

The modes are a list of about a dozen small unit-interval shaping functions, stored as function values in an array and dispatched by index. Conceptually:
1. Plain wraparound of the input's fractional part — a repeating linear ramp (sawtooth).
2. A triangle wave — linear up, linear down.
3. A smooth sine-shaped wave — rounded crests.
4. A square wave with even duty — hard on/off with no transition.
5. Triangle of a triangle — folded linear shapes.
6. Sine-wave of a triangle.
7. Triangle of a sine-wave.
8. Sine-wave of a sine-wave.
9. A square wave (biased duty, mostly-on) applied to a sine-of-triangle — yields a dash-dot-dash grouping.
10. A sine wave multiplied by a triangle at a slightly higher, non-integer frequency ratio — multiplication darkens where either is dark.
11. The average of a sine wave and a triangle at a few-times-higher non-integer frequency — a blend.
12. A double-frequency triangle minus a lower-frequency sine — interference with negative parts clipped to black.
13. The absolute difference of a triangle and a double-frequency sine — a "distance between waveforms" shape.

All waveform primitives take input in cycles (period one) and return values in the zero-to-one range; the square-wave primitive takes a duty-cycle argument. Exact frequency multipliers don't matter beyond "slightly detuned" vs "a few times higher" vs "double".

Randomness: none. Layout: nothing hardcoded — position is normalized, so any pixel count works. The repeat count (~4) is a stylistic constant.

## Colors
Grayscale only: black through white. No hue anywhere.

## Controls
None.

## Timing feel
Each mode holds for roughly two-thirds of a second, then an instant switch; the full tour of all modes takes several seconds and loops forever.

## Clever bits
- Storing the shaping functions as first-class values in an array and indexing into it is the point of the example (a mode-dispatch idiom), along with accumulating per-frame elapsed time into a rollover counter as a simple mode timer.
- Subtracting the threshold on rollover (rather than resetting to zero) preserves leftover time so mode durations stay accurate.
