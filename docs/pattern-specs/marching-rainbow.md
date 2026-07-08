# marching rainbow
kind: 1D
sensors: no

A short, simple 1D pattern: overlapping rainbow waves march along the strip with a shimmering interference-like brightness.

## What it looks like

Bands of slowly shifting rainbow color travel along the strip. Brightness is the difference of two triangle-ish waves moving at different speeds and very different spatial frequencies, so bright regions ripple and beat against a faster fine-grained ripple, giving a marching, slightly strobing texture. Hue drifts smoothly and nonlinearly, producing stretched and compressed rainbow segments rather than a uniform gradient. The overall feel is busy and continuous, with the main cycle repeating every several seconds and a faster component cycling about twice as fast.

## Algorithm

No state beyond two per-frame clock values.

Per frame: sample two sawtooth clocks, one with a period of several seconds, the other half that.

Per pixel (position = index divided by pixel count, so it adapts to any strip length — no hardcoding):

- Brightness = (smooth wave of clock-one plus position, one spatial cycle across the strip) minus (smooth wave of clock-two minus position at about ten times the spatial frequency, plus a small constant phase). Where the difference goes negative the pixel is simply dark, so roughly half the strip is off at any moment, forming the marching bright bands.
- Hue = a smooth wave applied to (a smooth wave of (a smooth wave of clock-one plus position)) minus position — i.e., the same slow traveling wave fed through itself twice more, then offset by position. The nested waves are what warp the rainbow nonlinearly.
- Full saturation always.

"Smooth wave" here means the engine's built-in sine-like wave mapping the unit interval to a 0..1 hump.

## Colors

Full-spectrum rainbow, fully saturated, on black.

## Controls

None.

This pattern is close to trivial — the only subtlety is the triple-nested wave for hue and the negative-clipping wave difference for brightness.
