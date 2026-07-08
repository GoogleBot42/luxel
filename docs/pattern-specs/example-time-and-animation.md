# Example: time and animation
kind: 1D
sensors: no

## Purpose
A tutorial/demo pattern. It is entirely monochrome (white light, brightness-only) and exists to show off a catalog of motion styles built from time-driven waveforms. It cycles automatically through roughly a dozen distinct "modes", holding each for well under a second, so it reads as a rapid-fire slideshow of animation techniques.

## What it looks like
White brightness patterns sweeping over the strip, one style at a time: a band drifting steadily left; the same drifting right; bouncing back and forth (first with a sharp linear reversal, then with a smooth eased reversal); a hard-edged chaser of alternating on/off blocks; various warbling, stretching, and interference-textured washes where two waves at slightly different rates beat against each other; a "kinetic" combination that overshoots; and a glitchy conveyor-belt look built from the absolute difference of two waveforms. The whole catalog repeats forever. Each mode holds for a bit over half a second, so the overall feel is busy and demo-like.

## Algorithm
State kept between frames:
- An accumulator of elapsed time (summed from per-frame deltas) used as the mode-switch timer.
- The index of the currently active mode.
- Two free-running sawtooth phases produced by the engine's global time function, at two different, fairly quick periods that are deliberately not simple multiples of each other (roughly a 5:3 ratio). Both are recomputed every frame in the pre-frame step. Having two unrelated periods is what makes the interference/beat-pattern modes interesting.

Per frame: add the frame delta to the accumulator; when it exceeds a threshold (a bit over half a second), subtract the threshold and advance the mode index, wrapping around.

Per pixel: normalize the pixel index to 0..1, scale it up by a small integer factor (about four), so most modes repeat about four times along the strip. Pass that spatial value plus the primary time phase into the active mode's function; the result (0..1) is used directly as the brightness of a white (zero-saturation) pixel.

The modes are stored as an array of small single-expression functions taking (spatial position, time phase); the frame loop just indexes into that array. That dispatch-through-an-array-of-lambdas structure is the main teaching point and should be preserved.

Mode catalog, conceptually (order matters only cosmetically):
1. Position plus time, wrapped — constant drift one direction.
2. Position minus time, wrapped — drift the other direction.
3. Position offset by a triangle wave of time — linear bounce back and forth.
4. Position offset by a sine-like wave of time — eased bounce.
5. A 50%-duty square wave of position-plus-time — a hard-edged moving chaser.
6. Position offset by a triangle wave whose input is itself a triangle of time multiplied by time — irregular, accelerating bounce.
7. Position offset by a wave-of-a-wave of time — warbly drift.
8. A square wave of (triangle of a wave of time) plus position — bouncing hard-edged blocks.
9. Product of two sine-like waves of position, each offset by one of the two different time phases — beating interference.
10. A wave of (a wave of position+time, plus a wave of position−other-time, plus position minus time) — rich wave texture.
11. A wave of position offset by a wave-of-wave of time, where a small fraction of position also feeds the inner wave — stretchy effect.
12. A wave of position (shifted and) scaled by one-plus-a-wave-of-time, multiplied by a wave of the other time phase plus position — zoomed and blended.
13. Twice a triangle of position-plus-a-wave-of-time, minus a wave of scaled position offset by a wave of the second time phase — kinetic, can exceed unit brightness (clipped by the color call).
14. Absolute difference between a triangle of position-minus-a-triangle-of-the-second-time-phase and a wave of doubled position offset by a triangle of the first phase — glitchy conveyor belt.

Exact formulas don't need to match; the goal per mode is the described visual character. Waveform vocabulary: "wave" = sine-shaped 0..1 wave over a unit period; "triangle" = linear up-down 0..1; "square" = duty-cycled 0/1.

## Colors
None — grayscale/white only. Hue and saturation are fixed at zero; only brightness varies.

## Controls
None. (The source has a commented-out line for pinning a single mode while studying it; a reimplementation could optionally expose a mode-select control, but stock behavior is auto-cycling.)

## Timing
Mode changes a little more often than once per second. The two underlying time phases cycle every few seconds each. No exact values matter; keep the "quick demo reel" feel.

## Layout assumptions
Pure 1D over the pixel index; works at any pixel count since position is normalized. No hardcoded lengths.
