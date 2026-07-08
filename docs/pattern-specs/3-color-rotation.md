# 3 color rotation
kind: 1D
sensors: no

## What it looks like (intended)
The strip is divided into short segments of a handful of pixels. Segments carry a repeating three-color sequence — aqua/cyan, magenta/pink, warm yellow — and every fraction of a second the whole sequence shifts one segment along the strip, producing a slow marching-barber-pole of three colors.

## What it actually does (as written — buggy)
The segment role is computed modulo three, which yields values zero, one, and two — but the code only paints roles matching one, two, and *three*. The "three" branch can never fire, so the yellow color never appears; and role-zero segments are never painted at all, so on the target platform (which does not clear the pixel buffer between frames) they simply retain their previous color. Net visible effect: segments alternate between cyan and magenta, with each segment appearing to hold magenta for two beats and cyan for one; yellow is absent. A reimplementer should decide whether to reproduce the intent (three colors marching) or the artifact; recommend implementing the intent and noting the fix: compare against zero/one/two (or index a three-entry color table by the modulo result) so all three branches are reachable.

## Algorithm
State between frames: a frame counter and a phase counter.

Per frame: increment the frame counter; every time it passes several dozen frames, reset it and advance the phase by one. Note this is frame-count-based, not wall-clock-based — rotation speed varies with render frame rate. The obvious fix is to accumulate the per-frame delta and advance the phase on a fixed time interval instead.

Per pixel: segment number = pixel index divided by segment size, rounded to nearest (note: rounding to nearest, not floor, makes the first and last segments appear half-width). Role = (phase + segment number) modulo three; role selects one of the three fixed colors.

## Layout assumptions
1D by raw index; works at any pixel count. Segment size defaults to about five pixels and is an exported/watchable variable rather than a proper slider — expose it as a slider (one to a few dozen pixels) in a reimplementation.

## Colors
Three fully-saturated, full-brightness stops: aqua/cyan, magenta/pink, warm yellow-gold. No fading; hard segment edges and instant color steps.

## UI controls
None declared. Suggested: segment-size slider, rotation-speed slider.

## Timing feel
A step every several dozen frames — on typical hardware somewhere around a fraction of a second to a second per step, but frame-rate-dependent (see fix above).

## Verdict
Trivial pattern (segmented three-color chase) with two instructive defects: an unreachable modulo branch that silently drops one color and leaves unpainted pixels relying on framebuffer persistence, and frame-count timing instead of delta timing.
