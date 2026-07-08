# twinkle
kind: 1D
sensors: no

This is a simple pattern. Despite being catalogued as sound-reactive, the source contains **no sensor input** — only a commented-out hint that the author once considered feeding the hue array from spectrum data. Implement it as a plain non-reactive twinkle.

## What it looks like

Random pixels ignite at full brightness in a randomly chosen hue, then fade smoothly to black over roughly a second (adjustable from a quick blink to a long afterglow). A steady drizzle of one to several new ignitions per frame keeps the strip alive with scattered colored twinkles.

## Algorithm

State: two arrays sized to a fixed constant of a couple hundred fifty entries — one holding each slot's hue, one holding its brightness/intensity. (This size is **hardcoded**; the obvious fix is to size both arrays to the actual pixel count. As written, strips longer than the constant repeat the twinkle field via index wrap-around, and shorter strips just use a prefix.)

Per frame (pre-render):
1. Every slot's intensity is decremented by a constant amount per frame: a user-controlled decay term (scaled down by about an order of magnitude) plus a small fixed baseline so it always fades even at the slider's minimum. Note this is frame-rate dependent (no delta compensation) — a faithful port may keep that, but scaling by elapsed time would be the obvious improvement.
2. A user-controlled number of slots (one to several) are picked uniformly at random; each picked slot's intensity is set to full and its hue is re-rolled uniformly at random between a user-set low hue bound and high hue bound (if the bounds cross, the range collapses to the low bound).

Per pixel (render): output the slot's hue at the pixel's index (wrapped modulo the array size), a user-set saturation, and the slot's current intensity as brightness. Intensity is allowed to go negative between refreshes; negative just renders as off.

## Colors

Fully user-defined: hues are drawn uniformly from a slider-bounded wedge of the color wheel, at a saturation from half-saturated pastel up to fully vivid. Defaults give the full rainbow, fully saturated.

## UI controls (all sliders)

- **High hue bound** — upper end of the random hue range.
- **Low hue bound** — lower end of the random hue range.
- **Saturation** — mapped from half to full saturation (never fully washed out).
- **Ignition rate** — quantized to an integer count of new twinkles per frame, from one to about six.
- **Decay speed** — how fast lit pixels fade; low end is a slow afterglow of a couple seconds, high end blinks out in a fraction of a second.

## Timing feel

Continuous gentle sparkling; each twinkle lives from a fraction of a second up to a couple of seconds depending on the decay slider (and on frame rate, per the note above).
