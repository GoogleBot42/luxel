# Synchronized Random Numbers
kind: 1D
sensors: no

This is as much a coding demonstration as a visual pattern: it shows how to build a deterministic, seedable pseudo-random number generator on the engine's fixed-point math (where naive multiplication would overflow), so that multiple devices — or multiple runs — can produce the *same* "random" sequence. The visible effect is a scrolling rainbow whose per-pixel hues get progressively jittered by a random walk.

## Visual behavior
Starts as a clean full-spectrum rainbow laid once across the strip, scrolling smoothly along it with a cycle time of several seconds. Each frame every pixel's hue also takes a tiny random step up or down. At first this reads as subtle sparkle/grain on the rainbow; over time the per-pixel offsets random-walk further apart, so the rainbow gets noisier and more speckled the longer the pattern runs (the walk is unbounded, but hue wraps around the wheel so it never breaks — the pattern just dissolves toward confetti over minutes).

## Algorithm
### The PRNG (the point of the pattern)
- A textbook linear congruential generator in the style of the classic BSD rand(), with a power-of-two modulus sized to fit the platform's fixed-point integer range. Its state is a single number carried between calls; each call advances state = (multiplier × state + increment) mod modulus, then returns the state scaled into a zero-to-max fraction.
- Because the engine's numbers are limited-range fixed point, multiplier × state would overflow if done directly. The pattern therefore implements:
  - an overflow-safe modular addition: reduce both operands mod the modulus, and detect the "sum exceeds modulus" case by comparison-with-difference rather than by actually forming a possibly-overflowing sum;
  - an overflow-safe modular multiplication via the binary double-and-add method (a.k.a. Russian-peasant multiplication): iterate over the bits of one operand, conditionally accumulating the other operand with the safe modular add, doubling it (again via safe add) each step. Cost is logarithmic in the operand.
- The generator's state (and one intermediate product, apparently for debugging) is exported as an inspectable/settable variable — that is what makes it "synchronized": external code can seed all devices identically and they will emit identical sequences.
- Quirk to preserve or fix: one line computes the raw (non-modular) multiplier-times-state product into the exported debug variable using the modular-multiply helper *without passing a modulus*, which in the original engine makes the modulus argument zero/undefined; it exists only for inspection and does not feed the visual output. A reimplementation can drop it or compute it safely.

### The visual
- Persistent state: the PRNG state, plus an array (sized to pixel count) of per-pixel hue offsets, all starting at zero.
- Per frame: advance a global sawtooth timebase (several seconds per cycle), then for every pixel draw one PRNG sample scaled to a very small "sparkliness" magnitude and add it, minus half that magnitude, to the pixel's stored offset — i.e., a zero-mean random walk step of tiny amplitude (on the order of one percent of the hue wheel per step).
- Per pixel at render: hue = global time phase + pixel's fractional position along the strip + that pixel's accumulated offset; full saturation, full brightness.
- Layout: fully parameterized by pixel count, no hardcoding.

## Colors
The entire fully saturated rainbow at full brightness — every hue, always at maximum vividness. No blacks or pastels.

## Controls
None. (The "sparkliness" step size is a constant in the source; exposing it as a slider would be a natural enhancement but is not part of the original.)

## Non-obvious details
- The per-pixel offsets are never damped or wrapped; the visual coherence decays over runtime by design of the demo, not by oversight — the point is watching identical noise evolve identically on synchronized devices.
- Every pixel consumes one PRNG draw per frame in index order, so the sequence consumed is deterministic given the seed and pixel count; a reimplementation must draw in the same per-pixel order to stay bit-compatible with a synchronized peer.
