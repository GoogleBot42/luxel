# static random colors
kind: 1D
sensors: no

## What it looks like
A completely still image: every pixel holds its own random color, frozen. Colors lean strongly toward vivid, fully saturated hues, with an occasional pastel or near-white pixel mixed in. All pixels are at full brightness. The arrangement never changes while the pattern runs; loading the pattern again (or restarting it) produces a brand-new random arrangement.

## Algorithm
The clever core: the pattern wants per-pixel randomness that is stable across frames. It achieves this with a small deterministic pseudorandom generator that is re-seeded to the same value at the start of every frame.

- At pattern initialization (once, at load), pick a random seed using the platform's true random source. This seed is kept for the life of the pattern run.
- State between frames: just that seed (plus the PRNG's working register, which is reset every frame).
- Per frame (before rendering): reset the PRNG's internal state to the seed.
- Per pixel (rendered in index order): draw two successive values from the PRNG.
  - First value → hue, used directly across the full hue wheel.
  - Second value → saturation, but biased toward the top: take the raw 0..1 value, cube it, and subtract from one. This makes most pixels highly saturated while still occasionally yielding washed-out/near-white pixels.
  - Brightness is always full.

Because the PRNG restarts from the same seed each frame and pixels consume values in a fixed order, every pixel sees the same random numbers every frame — hence a static image without storing a per-pixel array.

The PRNG itself is a classic xorshift on a 16-bit-style integer: three rounds of XOR-ing the register with shifted copies of itself (left, right, left by differing small shift amounts). To convert the integer register to a 0..1 fraction, the value is divided by a modest constant and the fractional part is taken. Any decent deterministic PRNG with the same reset-per-frame discipline reproduces the effect; the exact generator only changes which specific colors appear.

Layout: no pixel-count assumptions at all; works on any strip length or shape (each pixel just consumes the next random values in index order). Note this means the mapping from pixel to color depends on render order/index, which is fine for the intended effect.

## Colors
Uniformly random hues across the entire wheel. Saturation distribution is heavily weighted toward fully vivid, tapering off so pastels are uncommon and near-whites rare. Full brightness everywhere.

## UI controls
None.

## Timing
No animation. The image is static; it only changes when the pattern is reloaded.

## Non-obvious notes
- The reseed-every-frame trick is the whole point of the pattern: static per-pixel randomness with O(1) memory and no arrays.
- Renderers that evaluate pixels out of order or in parallel would scramble the (irrelevant) pixel-to-color assignment but still produce a valid static random image, as long as each pixel deterministically consumes the same PRNG draws every frame.
