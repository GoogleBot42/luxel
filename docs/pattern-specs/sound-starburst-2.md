# sound - Starburst 2
kind: 1D
sensors: yes

## What it looks like
A set of evenly spaced bright "comet heads" sweeps along the strip in one direction while a mirror-image set sweeps the opposite way, so pairs of heads repeatedly cross and pass through each other. Each head leaves a glowing trail behind it that decays over a fraction of a second. When the music is loud and rising (bass hits, swells), the heads flare bright, their trails linger longer, and the whole strip's hue drifts slowly and smoothly. When the sound falls off, trails snuff out quickly and the hue jitters rapidly, giving a flickery rainbow shimmer during quiet moments. One full traversal of the strip takes several seconds.

## Sensor inputs
- A 32-band audio frequency spectrum array.
- A single overall sound-energy value.

Per frame the pattern sums the lowest handful of spectrum bands ("lows") and all the rest ("highs"), adds them, and feeds the total into two exponential moving averages: a short-window one and one with roughly twice the window. The comparison "short average above long average" is the pattern's beat/rising-energy detector. Head brightness is the overall energy value scaled up by a large factor (roughly two orders of magnitude), gated to nonzero only while energy is rising.

## State kept between frames
Four per-pixel buffers: brightness, a per-pixel fade coefficient, saturation, and (nominally) an age counter that is written but not meaningfully used. Also the two moving averages, two hand-rolled phase accumulators advanced by frame delta (a "fast" one cycling several times per second and a "slow" one cycling over a few seconds), and the currently active hue phase.

## Per-frame work
1. Advance a sawtooth position phase with a period of several seconds (using the engine's global time oscillator), and compute its reflection (one minus it) for the opposite-direction set.
2. Advance the fast and slow hue phase accumulators by frame delta divided by their respective period constants, wrapping at one.
3. Update the two moving averages from the spectrum sums.
4. For each of N heads: place it at the pixel given by (position phase + head-number/N, wrapped) times the pixel count, and likewise for its mirror using the reflected phase. At each head pixel, stamp: brightness = the energy-derived flare value; saturation = a low value (so the head itself is whitish); and the per-pixel fade coefficient = a "slow fade" value close to one.
5. Choose the active hue phase: the slow accumulator while energy is rising, the fast accumulator otherwise. This is the trick that keeps color calm during sustained loud passages and lets it strobe through hues when quiet.

## Per-pixel work (render)
- Hue = active hue phase, plus (only when energy is falling) a mild positional gradient spanning a few hue cycles across four strip-lengths, plus (only when the rainbow toggle is on) a strong positional gradient of several full hue cycles across the strip.
- Saturation is multiplied by a factor slightly above one each frame and clamped at full, so a freshly stamped whitish head re-saturates to full color over a handful of frames — the white "hot core" turns colored as the trail ages.
- The per-pixel fade coefficient is pulled down to the "fast fade" value (noticeably below the slow one) whenever energy is falling, and the pixel brightness is multiplied by this coefficient every frame. Result: trails persist while the music is hot and collapse quickly in silence.
- Output is hue/saturation/brightness.

Note: fading is applied once per rendered frame, so decay speed is frame-rate dependent. A faithful reimplementation can keep this; a cleaner one would scale the decay by frame delta.

## Controls
- Slider, "how many leaders": sets the number of head pairs, from one up to about a dozen.
- Slider used as an on/off toggle (above halfway = on), "rainbow": enables the strong positional hue gradient so the strip shows many rainbow cycles instead of a near-uniform hue.

## Layout assumptions
Pure 1D by pixel index; scales with pixel count. The only fixed size is the 32-band spectrum, which matches the sensor hardware.
