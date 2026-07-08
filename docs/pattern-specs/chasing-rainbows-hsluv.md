# Chasing Rainbows & HSLuv
kind: 1D
sensors: no

## What it looks like
One full rainbow gradient laid across the strip, slowly scrolling. The point of the pattern is comparative: a mode selector switches between six different ways of mapping a linear position ramp to rainbow colors, so you can judge which looks most perceptually even on real LEDs. Naive hue-wheel mode shows the familiar over-wide cyan/green region; the corrected modes stretch the red/orange/pink region and compress cyan; the final mode uses a perceptually uniform colorspace so every hue appears equally bright and saturated (it looks slightly dimmer/pastel compared to the raw modes — the author deliberately dims the other modes to a comparable brightness, roughly two-thirds, so the comparison is fair).

## Algorithm
Per pixel, compute a base hue fraction: pixel position as a fraction of the strip, minus an animation phase, minus a user shift offset, wrapped to the unit interval. The animation phase is a sawtooth whose period is inversely proportional to the speed slider (speed near zero freezes it). Then map that hue fraction to a color by the selected method:

1. **Plain hue wheel** — feed the fraction straight to the standard HSV rainbow at full saturation.
2. **Half-sinusoid warp** — pass the fraction through a sine-based smoothstep (a quarter-wave reshaping) before the HSV call, stretching pink/red/orange and compressing cyan.
3. **Exponential "gain" warp** — a symmetric two-halved power-curve easing (linear at exponent one, increasingly stretch-one-side/compress-the-complement as the exponent rises toward about three), applied to the fraction, plus a wrap-around offset that chooses *which* hue gets stretched. Exponent and offset are slider-controlled.
4. **Classic 9-stop rainbow table, via HSV** — a well-known LED-library rainbow defined as nine fixed RGB stops (red, orange, yellow, green, aqua, blue, purple, pink, back to red). At startup the stops are converted once to HSV via a standard RGB-to-HSV routine; per pixel the hue fraction is scaled across the eight segments and the two neighboring stops are linearly interpolated in HSV.
5. **Same 9-stop table, interpolated directly in RGB** — identical structure but lerps the raw RGB stops; author finds it marginally nicer.
6. **Perceptually uniform colorspace (HSLuv)** — a full fixed-point port of the public HSLuv reference implementation: hue/saturation/lightness in a CIELUV-based space converted through LCh → LUV → XYZ → linear RGB with sRGB gamma, including the gamut-boundary computation that finds the maximum in-gamut chroma for a given lightness and hue. Saturation and lightness are slider-controlled (lightness defaults near half).

## The caching trick (important)
The colorspace conversion is far too slow to run per pixel per frame (the author observed a roughly 60-fold frame-rate collapse when computed live). So in mode 6 the pattern precomputes three per-pixel arrays (red, green, blue) covering one full hue revolution across the strip, and refreshes this lookup table only about ten times per second from the per-frame hook (accumulating frame deltas until roughly a tenth of a second has passed). The renderer just scales the pixel's hue fraction back to a table index and reads the cached RGB. Scrolling stays smooth because the phase offset is applied when computing the table index, not baked into the table. The table is also computed once at startup.

## State between frames
The mode, slider values, the delta accumulator for cache refresh, and the three pixel-count-sized RGB cache arrays. Modes 1–5 are stateless per pixel.

## Controls
- Number-input, "rainbow mode": integer selecting among the six methods.
- Slider, "speed": scroll rate; bottom of range freezes.
- Slider, "color shift": static hue-phase offset.
- Slider, "gain amount": exponent for mode 3 (one to about three).
- Slider, "hue to stretch": offset choosing the stretched hue in mode 3.
- Slider, "HSLuv saturation" and slider, "HSLuv lightness": inputs to the mode-6 table (take effect at the next cache refresh).
- Three leftover development/tuning sliders and some min/max watch variables that do nothing visible — omit them in a reimplementation.

## Layout assumptions
1D by pixel index; the mode-6 cache is sized to the pixel count, so hue resolution equals strip length. No hardcoding. The unused pastel-variant ("HPLuv") conversion routines in the source are dead code and can be skipped.
