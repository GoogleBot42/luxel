# portal
kind: 1D
sensors: no

## What it looks like

Slow, hypnotic ripples on a 1D strip. Every few seconds a pulse of energy is born near the middle of the strip (its exact birth point jitters around the midpoint) and spreads outward as a smooth hump that keeps widening while its intensity dies away over several seconds. The intensity field is rendered through a banded "fire portal" palette: as a pulse's local intensity rises from nothing, the strip shows a series of thin bright warm bands — red, then red-orange, then orange, then amber — separated by dim ember-red gaps; where intensity climbs higher still (pulse cores, or two pulses overlapping) the color crosses through black into a deep violet/indigo glow. The visual effect is concentric colored rings marching outward from the portal's center, hottest and strangest in the middle. Background is black.

This is machine-generated code from a declarative pattern-compiler (the intent is spelled out in a header comment), so the spec below describes the declared intent first, then the implementation's deviations.

## Algorithm (intended design)

State between frames: a wall-clock accumulator in seconds, plus a small pool (about four slots) of pulse records, each holding: alive flag, birth time, and birth position. Also a per-pixel scalar intensity buffer, and per-pixel color buffers.

Per frame:
1. Clear the intensity buffer.
2. **Spawning**: if the clock has passed the next-spawn deadline and a pool slot is free, start a new pulse: its position is drawn from an approximately normal distribution — implemented as the average-ish sum of three uniform randoms — centered at the strip midpoint with a small standard deviation (under a tenth of the strip), so pulses cluster near the middle. Schedule the next spawn a couple of seconds later.
3. **Pulse update**: for each live pulse, compute its age; if older than its lifetime (several seconds — roughly twice the spawn interval, so two pulses should normally coexist), kill it. Otherwise: temporal envelope = a linear decay from full at birth to zero at death (sawtooth-decay time shape). Spatial width = grows linearly with age, from around a tenth of the strip at birth to wider than the whole strip by death. Over the span of pixels covered by that width centered on the pulse position (clamped to the strip), add to the intensity buffer: temporal envelope × a half-sine hump across the covered span (zero at both edges, peak in the middle).
4. **Colorize**: map each pixel's summed intensity through a piecewise-linear gradient (linear interpolation between stops; clamped at both ends) into red/green/blue channels, stored per pixel.

Per pixel in render: emit the stored color with each channel **squared** (a gamma-style curve that deepens the dark bands and sharpens the bright ones).

## The gradient (qualitative stop list)

Input is the intensity value (nominally 0 to 1, overlaps can exceed the top and clamp):
- zero → black;
- then four evenly spaced thin bright bands over the lower half of the range, stepping through pure red, red-orange, orange, and amber — each bright band immediately dropping back to a dim brick-red between bands (sharp sawtooth-like brightness banding);
- just past those bands (around mid-range) → black again;
- then one long smooth ramp across the entire upper half of the range ending in a moderately bright violet/indigo at maximum.

Because each pulse's own peak reaches only into the lower/middle of that range as it ages, the violet zone mostly appears where pulses overlap or when a pulse is young and at full strength.

## Implementation deviations (bugs to be aware of)

The hand-off from the declarative description to code introduced two defects; a reimplementer should implement the *intended* behavior above, but know that the original behaves differently:
- The live-pulse counter that gates spawning increments/decrements a different (undeclared, auto-zero) variable than the one actually checked, so the check never blocks; the pool-slot search is the only real limit.
- Both the slot-search and the update loop stop at the **first non-live slot** rather than scanning the whole pool. Consequence: if slot zero frees while a later slot is still live, the later pulse is never updated or drawn again yet stays flagged live forever — slots leak. With the given spawn interval and lifetime, after the first cycle the pattern degrades to a **single pulse at a time**, reborn every several seconds, and the overlap-driven violet cores become rare. A correct implementation (scan all slots) restores the intended two-pulse overlap.

## Layout

Pure 1D, scaled by pixel count; no hardcoding. No randomness beyond the spawn position. No controls.

## Timing

A new pulse every couple of seconds (as intended); each lives several seconds, widening the whole time. The overall rhythm is a slow breathing, one ripple crest every few seconds.
