# policeLights
kind: 1D
sensors: no

This is a trivial pattern: emergency-vehicle style alternating flashers.

## Appearance
The strip is divided into fixed-size contiguous blocks (about ten pixels each, hardcoded). Alternating blocks show two colors — red and a blue-violet — at full saturation and full brightness. A few times per second (roughly every fifth of a second) the two colors swap, so each block flips red↔blue in strict alternation. No fading; hard cuts.

## Algorithm
- Per frame: accumulate elapsed milliseconds; when the accumulator exceeds the blink interval, reset it and swap the two hue values held in state. The two hues are the top of the hue wheel (red) and a hue about seven-tenths of the way around (blue-violet).
- Per pixel: divide the pixel index by the block size and use the parity of the result to pick which of the two current hues to show. (Quirk: the source takes the parity via a bitwise test on a fractional quotient, relying on the platform truncating to integer — a port should just floor the quotient before taking parity.)

## Layout / fixes
Block size and blink interval are hardcoded constants; the obvious improvement is to expose both as sliders (or derive block size from pixel count so any strip gets a similar number of blocks). Also, one of the two color-state variables is only initialized after the first swap; initialize both up front.

## Controls
None exported.
