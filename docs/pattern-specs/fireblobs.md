# fireblobs
kind: 1D
sensors: no

## What it looks like
Soft, rounded blobs of fire-colored light fade in and out at random places along the strip, overlapping and blending like slow embers. At any moment several blobs are alive at once. Each blob swells to full brightness and dies away over a few seconds; new ones appear roughly twice a second, so the strip always has activity but never a hard edge or sudden pop. The overall impression is a calm, organic fire shimmer with no travel or directionality — blobs breathe in place.

## Algorithm
The pattern is two independent "blob generator" layers composited additively, each with its own color.

Per layer, state kept between frames:
- A running clock in seconds (accumulated from frame deltas).
- A small fixed pool (on the order of ten slots) of blob records: alive flag, birth time, and a center position chosen uniformly at random along the strip (normalized position).
- The time at which the next blob is allowed to spawn.
- A per-pixel intensity buffer for the layer, plus final per-pixel red/green/blue buffers for the composite.

Per frame (all heavy work happens in the pre-render step; the per-pixel render just reads buffers):
1. Advance the clock by the frame delta.
2. Clear the layer's intensity buffer.
3. If the clock has passed the layer's next-spawn time and a free pool slot exists, spawn a blob: mark a slot alive, record birth time, pick a fresh uniform-random center position, and set the next-spawn time about half a second in the future. (The two layers use slightly different spawn intervals so they never lock in step.)
4. For every live blob: compute its age as a fraction of the layer's blob lifetime (one layer's lifetime is a few seconds; the other's is about a third longer). If past its lifetime, kill it and free the slot. Otherwise its temporal brightness envelope is a triangle wave of that age fraction — linear rise to a peak at mid-life, linear fall to zero.
5. Each live blob paints a spatial bump into the intensity buffer: it covers a window roughly one fifth of the strip wide, centered on its position (clipped at strip ends). Across that window the profile is a half-cycle of a sine — zero at both edges, peaking in the middle. Each pixel in the window accumulates (adds) the product of the temporal envelope and the spatial profile. Overlapping blobs sum.
6. Composite: each layer's summed intensity is soft-limited — clamped so nothing exceeds about half scale, then rescaled back up to full — which lets overlapping blobs saturate gracefully into a flat-topped plateau instead of blowing out. The two limited layer intensities are then each multiplied by their layer color (as red/green/blue weights) and added per channel into the output buffers.

Per pixel in render: read the three channel buffers and output each channel squared (a gamma-like curve that deepens the dark ends and makes the blob edges feel softer).

Randomness: only the spawn positions (uniform along the strip). Spawn timing and lifetimes are deterministic.

Layout: fully resolution-independent — everything works in normalized strip position scaled by the pixel count. No hardcoding to fix.

Note on the original: the source contains a bookkeeping slip where the live-blob counter used in the spawn gate is never actually updated, and its slot-scan loops stop at the first dead slot (so blobs after a gap in the pool stall). Implement the evident intent instead: a properly maintained pool with a cap of about ten concurrent blobs per layer, scanning all slots.

## Colors
- Layer one (the shorter-lived, slightly slower-spawning layer): bright orange-gold — full red, roughly half green, no blue.
- Layer two (longer-lived, slightly faster-spawning): dim ember red — red at about half strength with only a trace of green and blue.
Their sum reads as fire: orange cores over a deep red wash, with the squaring step keeping shadows black.

## Controls
None.

## Timing feel
New blobs appear about every half second per layer; each blob lives a few seconds (rising then falling). Nothing moves spatially; the animation is purely blobs breathing in and out.
