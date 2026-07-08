# wanderedges
kind: 1D
sensors: no

## Visual behavior
Soft green "fireflies" appear at random spots along the strip, swell up, and fade away over a few seconds each. Because of how intensity maps to color (see the clever bit below), a strong pulse does not look like a solid green blob: as it swells, its center darkens back toward black while two bright green *edges* spread outward from it, then converge again as it fades — hence the name. Where pulses overlap, their combined intensity likewise pushes into the dark end of the palette, so merging glows appear to split into wandering bright rims. The overall feel is organic and slow: a new glow starts roughly twice a second (when there is room), several are alive at once, and each lives for a few seconds. The pattern's own description: green fireflies wander back and forth, merging and diverging.

## Algorithm
This is generated code (from a declarative pattern compiler), so implement the *intent*, not the mechanics.

State kept between frames:
- A running clock in seconds, advanced by the frame delta.
- A fixed-size pool of pulse slots (around ten). Each slot stores: alive flag, birth time, and a position along the strip (a fraction of strip length).
- The next time a spawn is allowed.

Per frame (all heavy work happens in the pre-render step; buffers are sized to the pixel count, so any strip length works):
1. Clear a per-pixel intensity accumulator buffer.
2. Spawning: if the clock has passed the next-spawn time and a free slot exists, activate a slot with a uniformly random position along the full strip, record its birth time, and set the next-spawn time a short interval later (well under a second).
3. For each live pulse, compute its age relative to a lifetime of a few seconds. If expired, kill it. Otherwise its temporal envelope is a half-sine over its life (smooth fade-in to a peak at mid-life, then fade-out). Its spatial footprint spans roughly a fifth of the strip, centered on its position, shaped as a triangle (peak at center, linear falloff to the ends, clipped at strip boundaries). Add temporal-envelope × spatial-shape into the accumulator for each covered pixel.
4. Map each pixel's accumulated intensity to a color: first clamp it to about two-thirds of full scale, then scale it up by half again so the usable range exactly spans the palette; look the result up in the gradient palette described below with linear interpolation between stops.
5. The per-pixel render step just outputs that stored color, squaring each channel first (a gamma-style correction that deepens the darks).

Randomness: only the spawn position of each pulse (uniform over the strip). Everything else is deterministic.

Known source quirks (do NOT reproduce): the generated code's count of live pulses is incremented/decremented on a different variable than the one it tests, and both its spawn scan and its update scan stop at the first dead slot instead of skipping it, so slots after a gap are neither aged nor drawn that frame. Implement the obvious intent instead: maintain the pool correctly, update and draw every live pulse, and cap concurrent pulses at the pool size.

## Colors
A single-hue palette, indexed by intensity: black at the very bottom, staying black through the low range, rising through dim green to a vivid pure green at the midpoint, then symmetrically falling back through dim green to black at the top. Only the green channel is ever nonzero. The channel-squaring on output makes the dim greens read as a deep forest glow.

## The clever bit
The palette is peaked at *mid* intensity and returns to black at *max* intensity. A pulse therefore inverts as it strengthens: pixels at moderate intensity glow brightest, while the pulse's core (and any overlap between pulses) crosses the peak and goes dark. Visually this converts simple stationary additive pulses into moving bright edges that expand, merge, and contract — the pulses never actually move; only the iso-intensity contours do.

## Timing
- New pulse roughly every half second while slots are free.
- Each pulse lives a few seconds (several times the spawn interval), with a smooth sinusoidal rise and fall.
- With defaults, expect several pulses coexisting at any moment.

## Controls
None.
