# Christmas Candy Cane
kind: 1D
sensors: no

## What it looks like
Classic candy-cane stripes: the strip is divided into eight equal segments that alternate red / white / red / white (four pairs total, regardless of strip length). The whole striped pattern scrolls steadily along the strip in one direction, wrapping seamlessly around the end. Motion is a slow, constant crawl — the pattern drifts by a fraction of a pixel each frame, so a full lap takes on the order of tens of seconds on a typical strip.

## Algorithm
- Segment length is the pixel count divided by eight (computed once at startup, so it scales to any strip length — nothing is hardcoded to a specific count).
- State between frames: a single scroll offset. Each frame it is advanced by a small constant amount (a fraction of one pixel per frame) and reset to zero once it exceeds the pixel count.
- Per pixel: add the scroll offset to the pixel index, wrap modulo the pixel count, then decide which of the eight segments the shifted index falls in. Even segments are red, odd segments are white. (The original does this with an explicit chain of eight range tests rather than a modulo-by-two on the segment number; the latter is the obvious cleaner reimplementation and behaves identically.)
- No randomness.

## Colors
Only two colors: a pure saturated red rendered at roughly half brightness, and pure white (zero saturation) at full brightness. Note the asymmetry: white segments are noticeably brighter than the red ones.

## Controls
None exported. Speed and segment count are internal constants.

## Timing
Scroll speed is a fixed per-frame increment, i.e. frame-rate dependent — the pattern moves faster on faster hardware. The obvious fix in a reimplementation is to scale the advance by the frame delta so speed is wall-clock stable. Visually it should feel like a slow, even conveyor-belt crawl.

## Notes / quirks
- Because the range tests in the original use a mix of strict and inclusive comparisons, a pixel landing exactly on certain segment boundaries would be skipped (left un-set for that frame); the scroll offset is fractional so this essentially never fires. A reimplementation can safely just use segment parity.
- Trivial pattern overall: scrolling two-color barber-pole stripes.
