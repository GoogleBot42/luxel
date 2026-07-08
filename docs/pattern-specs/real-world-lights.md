# Real World Lights
kind: 1D
sensors: no

Note: this pattern is catalogued under sound/sensor-reactive but uses no sensor input at all. It is a lighting-emulation utility.

## What it looks like
A single slider selects one of thirteen presets, each imitating the color (and in a few cases the flicker) of a real-world light source. Most presets fill the whole strip with one steady solid color; three presets add gentle organic movement; one has a static spatial stripe pattern.

The thirteen presets, in slider order:
1. Candlelight — warm orange, flickering (animated).
2. Warm white incandescent.
3. Soft white incandescent.
4. Cool white incandescent.
5. Uranium-glass fluorescence — vivid yellow-green glow with slow internal brightness variation (animated).
6. High-pressure sodium lamp — deep orange-amber, steady.
7. Mercury-vapor lamp — pale minty green-white, steady.
8. Sodium-vapor lamp — strongly orange, steady.
9. Warm fluorescent tube — white with a pinkish cast, steady.
10. Cool fluorescent tube — white with a pale blue cast, steady.
11. LED grow light (vegetative) — static alternating bands drifting between blue and violet/purple across the strip.
12. Ultraviolet/black-light tube — saturated violet-purple, steady.
13. Cherenkov radiation — intense saturated blue with slow shimmering brightness variation (animated).

## Algorithm
Two parallel dispatch tables indexed by the selected preset: one table of per-frame setup functions, one of per-pixel renderers. Each frame runs the selected setup, then the selected renderer per pixel. This table-of-closures dispatch is the pattern's main structural idea and makes adding presets trivial.

- The three incandescent presets share one helper that converts a black-body color temperature (given in hundreds of kelvins; warm around three-thousand K, soft around four-thousand, cool around six-to-seven-thousand) to an RGB triple using the standard log/power-curve approximation of the black-body locus, clamped to unit range. The result is a shared solid color for all pixels.
- The steady presets simply set a fixed color (some as RGB triples, some as hue-plus-saturation at full brightness) and paint every pixel with it.
- Animated presets use a smooth 1D noise function built by summing four sine-shaped waves of the pixel's normalized position at different, non-harmonic spatial frequencies (roughly a dozen to a few dozen cycles across the strip), each drifting in time at slightly different rates (four time phases derived from a shared speed factor; full drift cycles take on the order of ten seconds, scaled per preset). The sum is normalized to roughly a plus/minus-one range.
  - Candle: brightness follows the noise with a floor at roughly a third so it never goes dark, and the hue is nudged slightly (a small fraction of the hue wheel) by a triangle-wave function of noise plus position — the flame both dims and shifts subtly between orange tones.
  - Uranium glass: fixed yellow-green hue at full saturation; brightness follows the noise with a floor around a quarter.
  - Cherenkov: fixed blue hue; same idea but the noise is sampled at a stretched (about one-third) coordinate so the shimmer is spatially broader, floor around a fifth.
- Grow light: per pixel, a sine-shaped wave of position with several dozen cycles across the strip is squared and scaled to about a third of a hue-wheel span, then added to a blue base hue — producing repeating blue-to-purple gradient bands. Static in time.

## State between frames
Only the selected preset index, the shared color scratch variables, and the four drifting time phases. No per-pixel buffers.

## Controls
- Slider, "light type": divides its range evenly into the thirteen presets.

## Layout assumptions
1D by normalized pixel index; fully pixel-count independent. Spatial frequencies are relative to strip length, so on very long strips the noise and grow-light bands compress proportionally (acceptable; no fix needed).
