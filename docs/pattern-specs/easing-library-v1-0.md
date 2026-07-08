# Easing Library v1.0
kind: 1D+2D
sensors: no

This is primarily a code library, not a visual effect: a collection of thirty standard easing functions with a built-in demo that steps through them. The functions are the well-known set documented publicly at easings.net — implement them from that public reference (curve shapes, standard overshoot/bounce constants and breakpoints are all published there; do not copy anything from this pattern beyond the list below).

## The library
Each easing maps a unit-interval input to (mostly) a unit-interval output. Ten families, each in "in", "out", and "in-out" variants:
1. Sine
2. Quadratic
3. Cubic
4. Quartic
5. Quintic
6. Exponential (with exact-zero/exact-one endpoint guards so the exponential never leaves the range at the ends)
7. Circular
8. Back (intentionally overshoots below zero at the start / above one at the end; uses the standard published overshoot constant)
9. Elastic (intentionally overshoots; endpoint-guarded like exponential; uses the standard published period constants)
10. Bounce (the standard piecewise-parabola formulation with the published breakpoints; "in" and "in-out" variants are derived from "out" by the usual reflections)

A few of the "back" demo entries are pre-scaled/offset slightly so their overshoot still fits on the display; keep that idea (shrink toward mid-range) rather than exact factors.

## The demo
State: an elapsed-time accumulator (built from the per-frame delta), the index of the currently shown easing, and a ping-pong parameter used to run a marker along the curve.

Every several seconds (about five), the demo advances to the next easing function, wrapping to the first after the last. On each advance it also snapshots-and-resets a pair of running min/max trackers (exported for debugging, so you can watch each function's actual output range, including overshoots).

The ping-pong parameter sweeps smoothly from zero to one and back over a couple of seconds, repeating; it resets when the function changes.

### 1D renderer
Each pixel's hue is the current easing evaluated at the pixel's normalized strip position, at full saturation and brightness. So the strip becomes a rainbow whose color distribution is warped by the easing curve: "in" curves bunch the low hues, "out" curves bunch the high hues, elastic/bounce make the rainbow wiggle back and forth. Position is normalized by pixel count — layout-agnostic.

### 2D renderer (a function grapher)
Treats the display as a unit-square plot of the current easing:
- For each pixel, evaluate the easing at the pixel's x. If the pixel's y is within a small tolerance of that value, light it — hue equal to the eased value, full saturation/brightness — drawing the curve as a rainbow-colored line. The tolerance is scaled inversely with the square root of the pixel count so the line stays about one pixel thick on any matrix size (nice touch worth keeping).
- A pure white marker dot: pixels in a thin horizontal band just above the vertical midline light up white when their x is within a small tolerance of the easing evaluated at the ping-pong parameter. The dot therefore shuttles left-right along the x axis at the eased speed — you literally watch the easing's velocity profile.
- Pixels exactly on the main diagonal (x equals y) show dim gray: a straight-line reference to compare the curve against (the source notes this line is optional).
- Everything else black.
- The 2D path is also where the running min/max of the curve values get tracked.

## UI controls
None — the demo advances automatically on its timer.

## Notes
No randomness anywhere; fully deterministic. The only per-frame work is the timekeeping; everything else is per-pixel evaluation of the current easing. The exported debug variables (elapsed time, current function index, ping-pong parameter, current and previous min/max) are part of the pattern's teaching purpose and worth reproducing.
