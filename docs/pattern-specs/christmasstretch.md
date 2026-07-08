# ChristmasStretch
kind: 1D
sensors: no

## What it looks like
The strip is divided into contiguous blocks of a fixed number of pixels, cycling through three colors in sequence: saturated red, saturated green, and a dim white (white at a bit under half brightness, so it reads as a muted accent between the two vivid colors). Classic Christmas-light look. Over a cycle of about two seconds, the block boundaries slide steadily along the strip while one color's bands gradually narrow and another's widen — a "stretching" of the pattern. At the end of each cycle the pattern snaps back and the three colors rotate roles (each color moves to the next block position), then the slide/stretch repeats. The net feel is a slow, hypnotic creep of holiday-colored bands that periodically reshuffle.

## Algorithm
State kept between frames:
- An elapsed-time accumulator, incremented by the frame delta and reset to zero when it exceeds the cycle length (about two seconds).
- Three "role" slots (first/second/third block color). Each time the accumulator wraps, a small three-state machine rotates which of the three colors occupies each role.

Per-frame: only the timer update and (on wrap) the color rotation.

Per-pixel (render): compute a phase fraction = accumulator / cycle-length, ranging 0→1 across the cycle. Take the pixel's block coordinate = pixel index divided by the block size, add the phase fraction, and reduce modulo three. Compare that against thresholds that themselves shrink with the phase fraction: if it is below (one minus phase) the pixel takes the first role's color; below (two minus phase) the second; otherwise the third. Adding the phase makes the whole banding drift toward the start of the strip by one block width per cycle; subtracting the phase from the thresholds is what makes the first band shrink and the last band grow within each cycle (the "stretch").

Coloring: the three role values are encoded as slightly-different hue-like tags. Two of them render as fully saturated hues (one at the red end of the wheel, one in the green region). The third is special-cased: it renders with zero saturation at a bit under half brightness, i.e. dim white.

No randomness. Fully deterministic.

## Layout assumptions
- Block size is a hardcoded pixel count (a couple dozen pixels per block), so the look depends on strip length; on short strips you may see only one or two blocks. Obvious fix: derive block size from the total pixel count (e.g. total divided by a desired band count), or expose it as a slider.
- The cycle duration is also a hardcoded constant; a speed slider would be the natural improvement.

## Controls
None exported. (Candidates if reimplementing: sliders for cycle speed and block size.)

## Timing
One slide/stretch cycle takes about two seconds; a given color returns to a given block role every three cycles (roughly six seconds).

## Notes
- The pattern's source comment claims red/blue, but the actual output is red, green, and dim white.
- The clever bit is doing both a positional drift and a threshold shrink with the same phase value, which produces the combined scroll-and-stretch motion from two trivial comparisons.
- The special-cased "tag" trick (using near-identical hue values as enum labels and testing equality in render to decide saturation/brightness) is an implementation quirk; a reimplementation should just use a proper three-entry color table of (red, green, dim white) rotated each cycle.
