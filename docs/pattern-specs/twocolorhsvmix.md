# TwoColorHSVMix
kind: 1D
sensors: no

## What it looks like
The strip displays a smooth blend between two user-picked colors, arranged as a sinusoidal wave along the strip: regions of pure color A give way through in-between blends to pure color B and back, and the whole wave scrolls continuously along the strip. Optionally, a symmetric envelope dims the strip toward both ends (bright in the middle, fading to black at the tips), with adjustable strength. Scroll speed spans a very wide range, from crawling to fast.

## UI controls
1. **Primary color** — HSV color picker (hue, saturation, brightness all honored).
2. **Secondary color** — HSV color picker.
3. **Period/speed slider** — sets how fast the wave scrolls. The mapping is exponential (the slider position feeds an exponent), covering several orders of magnitude of cycle time centered around a few-second cycle; one end is very fast, the other extremely slow.
4. **Envelope slider** — at zero, no end dimming; as it increases, a half-sine window (peaking at the strip center, zero at both ends) is raised to a growing power and multiplied into brightness, progressively narrowing the lit region toward the center.

## Algorithm
State: one master phase from a sawtooth clock with the slider-set period.

Per pixel:
- A blend weight in 0..1 is a sine-shaped wave of (normalized strip position plus the master phase), so the blend pattern repeats once per strip length and translates over time.
- Saturation and brightness are plain linear interpolations between the two picked colors by that weight.
- Hue interpolates by the **shortest way around the hue circle**: if the two hues are more than half the wheel apart numerically, interpolate across the wrap point (e.g. orange to purple passes through red) instead of the long way through yellow/green/blue. Implemented by lifting the smaller hue by a full turn, interpolating, and wrapping back; the helper also swaps its arguments (mirroring the weight) so the result is direction-consistent. Otherwise interpolate hues directly.
- Multiply brightness by the envelope: the sine of pi times the pixel's (slightly inset) normalized position, raised to the envelope slider's power. The inset (position computed against pixel count plus one, index shifted by one) keeps the very ends from being computed at exactly zero of the sine — preserve the idea that the window never evaluates degenerate endpoints.
- Emit as HSV.

No hardcoded pixel counts; scales with strip length naturally.

The one non-obvious piece is the wraparound-aware hue interpolation — everything else is a straightforward two-color sine crossfade with a windowing function.
