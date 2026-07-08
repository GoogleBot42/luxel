# novas
kind: 1D
sensors: no

## What it looks like
Soft glowing bursts ("novas") appear at random places along the strip, roughly every second and a half on average. Each burst starts small, narrow, and intensely bright — white-hot at its core — then expands outward and dims over several seconds, its color settling into its tint as it fades: some bursts fade toward a crimson/hot-pink, others toward an orange-red. Several bursts (up to about a dozen total) can overlap at once; where they overlap, the brighter one wins rather than the light stacking. The overall feel is a calm, continuous field of blooming and dying embers. The pattern was machine-generated from a declarative effect description (a "pulser" combinator language), which is why it consists of two nearly identical halves.

## Algorithm
Two independent, structurally identical pulse generators run side by side; they differ only in tint. Each generator owns a small fixed pool (about half a dozen slots) of concurrent pulses.

State kept between frames, per generator:
- Per slot: alive flag, birth timestamp, position (random, uniform along the strip), lifetime (random, uniform over roughly four to six seconds).
- A "next spawn" timestamp.
- A per-pixel intensity buffer (one scalar per pixel).
Globally: a running clock in seconds, and combined per-pixel red/green/blue output buffers.

Per frame (all heavy work happens here; the per-pixel renderer just reads buffers):
1. Advance the clock by elapsed time.
2. For each generator: clear its intensity buffer. If the clock has passed the next-spawn time and a slot is free, activate one slot: pick a uniform-random position along the strip, a uniform-random lifetime of several (roughly four to six) seconds, record the birth time, and schedule the next spawn about a second and a half later with mild bell-curve jitter (implemented as a sum of a few uniform randoms — an approximate normal with a small spread, mean about a second and a half).
3. For each live pulse: compute its age and age-as-fraction-of-lifetime. If past its lifetime, kill it. Otherwise:
   - Temporal envelope: the square of (one minus fractional age) — starts at full intensity the instant it's born and decays quadratically to zero. No attack ramp; it pops on.
   - Width: grows linearly with age, starting at about a tenth of the strip and widening by roughly a third of the strip per second of age (in normalized strip units).
   - Spatial envelope: a half-sine hump across the pulse's current width, centered at its position, clipped to the strip ends.
   - The product of temporal and spatial envelopes is *added* into the generator's intensity buffer over the covered pixels (pulses within the same generator do stack).
4. Combine into RGB: each generator has a tint — one is red with a touch of blue (crimson / hot pink), the other red with a touch of green (orange-red). For each pixel, each generator's color is its intensity multiplied channel-wise by a blend from its tint toward pure white, where the blend weight is the intensity itself — so faint regions show the tint and intense cores wash out to white. The final pixel takes, per channel, the maximum of the two generators' contributions (a brightest-wins merge, not additive).

Per pixel (render): read the combined RGB buffers and output each channel squared, for gamma correction.

Randomness: spawn positions (uniform), lifetimes (uniform over a narrow several-second range), spawn intervals (approximately normal around ~1.5 s via summed uniforms).

Layout: everything is expressed in normalized strip fractions and scales with pixel count; assumes a 1D strip. No hardcoding to fix.

## Colors
Two-tint palette, each running white-hot-core to tint to black: one family is black through crimson/pink to near-white; the other black through orange-red to near-white. Background is black.

## Controls
None.

## Timing feel
A new bloom about every second and a half; each bloom lives four to six seconds, brightest at birth, expanding as it dies. The strip is never empty for long but never crowded (about six concurrent per tint family, max).

## Non-obvious points
- The white-core effect comes from lerping the tint toward white *by the intensity value itself* before multiplying by intensity — a cheap "incandescence" trick.
- Merging the two color families with per-channel max instead of addition prevents overload and keeps overlapping novas visually distinct.
- Because intensities within one generator add and can exceed unit range before the tint-lerp, cores saturate to white quickly when pulses overlap.
