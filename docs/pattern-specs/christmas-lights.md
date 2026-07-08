# Christmas Lights
kind: 1D
sensors: no

## What it looks like
Classic static Christmas string lights: the strip is divided into short blocks of a few pixels each, colored in a repeating three-way pattern of red, dim white, and green. Every several seconds (on the order of five), the three colors rotate positions among the blocks in a step change — no fading, an instant swap. Between swaps the display is completely static.

## Algorithm
State between frames:
- A millisecond accumulator fed by the per-frame delta.
- Three "role" slots (first, second, third color of the repeating triple).

Per frame: add delta to the accumulator. When it exceeds a threshold of several seconds, reset it and advance the three roles to the next of three fixed assignments, cycling forever. The three assignments are rotations of the triple (red, white, green) — i.e. each color steps to the next block position each interval.

Per pixel: divide the pixel index by the block size and take which third of the repeating triple it falls in (block index modulo three) to pick the first, second, or third role's color.

Rendering trick worth knowing: the original encodes each color as a single hue-like number and uses one special sentinel value to mean "white" — when a pixel's role carries that sentinel, it renders with zero saturation at slightly under half brightness; otherwise it renders fully saturated at full brightness with the value's fractional part as hue (values above one wrap around the hue wheel). A reimplementation can simply use an explicit three-entry color list instead.

## Known quirk (fix recommended)
Only the first role is initialized at startup; the other two roles are unset until the first interval elapses. Unset roles default to zero, which renders as full red — so for the first several seconds two thirds of the blocks show red instead of white/green. Fix: initialize all three roles at startup.

## Layout assumptions
1D by index. Block size is hardcoded to a few pixels (three). Works at any pixel count, but the block size should be exposed as a slider (roughly one to a dozen pixels) for different installations.

## Colors
Three-stop repeating set: pure saturated red, soft dimmed white (noticeably less bright than the colored blocks), pure saturated green.

## UI controls
None in the original. Sensible additions: a slider for block size, a slider for rotation interval.

## Timing feel
Colors hold static for several seconds (around five), then instantly rotate one block position. Very low activity; a background/ambient pattern.

## Verdict
Near-trivial pattern: block coloring plus a slow three-state rotation timer. The only subtleties are the white-sentinel encoding and the uninitialized-roles startup bug.
