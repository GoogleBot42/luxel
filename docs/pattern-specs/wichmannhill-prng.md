# Wichmann–Hill PRNG
kind: 1D
sensors: no

This is a utility/demonstration pattern, not a designed visual effect. Its purpose is to provide a seedable, deterministic pseudorandom number generator (the classic published Wichmann–Hill generator, 1982) as reusable code, with a throwaway demo renderer to exercise it.

## What it looks like
Per-pixel colored static: every pixel gets a fresh random hue and a fresh random brightness every frame, at full saturation. The result is fast, flickering rainbow noise across the whole strip. No structure, no timing envelope.

## Algorithm
Implement the standard Wichmann–Hill generator exactly as published in the literature (it is a public, well-documented algorithm — take the three linear-congruential update rules and the three moduli, all near thirty thousand, from any published reference; do not invent your own constants). Summary of its shape, per the public definition:
- State: three integer seeds, each meant to be in the range from one up to about thirty thousand. Here they are initialized at pattern load to independent random integers in that range. A helper exists to set all three seeds explicitly, giving reproducible sequences (that is the whole point of the pattern).
- Each draw: each seed is updated by its own small multiplicative-congruential step (the published implementation form uses a multiply-of-remainder minus a small-multiple-of-quotient formulation to stay within limited-precision arithmetic — matching that formulation matters on a fixed-point VM, so mirror the published low-precision-safe form rather than the naive multiply-then-mod form).
- Output: the sum of the three seeds each divided by its respective modulus, taken modulo one — a uniform value in the unit interval.

Bookkeeping for debugging (all exposed as watchable/exported variables): a counter of how many draws have been made, which rolls over into a second counter after about thirty-two thousand calls (to dodge fixed-point overflow); the most recent per-frame draw; and running minimum and maximum of every value ever drawn (to eyeball uniformity — they should crawl toward zero and one).

Per frame: one draw is made purely so its value can be watched externally.

Per pixel: two draws — one becomes the hue, one becomes the brightness; saturation is full. Min/max tracking is updated on all draws.

Layout: index-agnostic; works on any pixel count; 1D renderer only (2D/3D mapped displays would fall back per platform rules, which is fine).

## UI controls
None.

## Notes for implementer
- The value of this pattern is the seedable generator and its exported debug surface, not the visuals. Keep the exported names/roles conceptually: three seeds, call counters, last result, running min and max.
- The demo intentionally draws fresh values every frame with no persistence, so the display never repeats unless reseeded identically and frame counts match.
