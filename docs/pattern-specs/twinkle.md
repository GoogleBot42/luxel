# Twinkle
kind: 1D
sensors: no

## What it looks like
An icy, star-field twinkle on a dark background. Individual pixels flare up quickly to a pale blue-white glint, then die away more gradually. New twinkles keep appearing in small random bursts, scattered fairly evenly along the whole strip, so the strip always has a sparse population of stars at different phases of their life. Each individual twinkle lives on the order of a couple of seconds; new bursts appear several times per second, so the overall texture is lively but never crowded.

## Algorithm
State kept between frames (both sized to the pixel count):
- a per-pixel brightness value, and
- a per-pixel "age" (elapsed lifetime in time units). Age of zero means the pixel is dark/inactive.

Additionally: an accumulator that paces spawning, and a round-robin segment counter (see below).

Per frame (advance step, before rendering):
1. For every pixel: if its age has exceeded the fixed twinkle lifetime, reset both its age and brightness to zero (dead). Otherwise, if the pixel is alive (age nonzero), recompute its brightness from its age using the envelope curve described below, then advance its age by the frame's elapsed time.
2. Spawning: accumulate elapsed time; when the accumulator passes a fixed spawn interval (a small fraction of a second), spawn a burst and reset the accumulator. A burst contains a uniformly random number of new stars, from zero up to a modest maximum (on the order of a dozen or so). Each new star is placed by an even-spread trick: the strip is conceptually divided into as many equal segments as the maximum burst size; a persistent counter walks these segments round-robin, and each new star gets a uniformly random position *within* the current segment before the counter advances (wrapping). A star is spawned by setting that pixel's age to a tiny nonzero value (one frame's worth), which marks it alive. Spawning onto an already-lit pixel simply restarts it. A bounds check skips any candidate index at or past the end of the strip.

Brightness envelope: age is mapped linearly onto a curve parameter running from zero (birth) to a high single-digit value (death). The brightness is a "fast attack, slow decay" pulse: proportional to the square of the (scaled) parameter multiplied by a decaying exponential of that same parameter — i.e. a gamma-distribution-like skewed bump. Constants are chosen so the peak reaches roughly full brightness early in the lifetime, then it tails off long and smooth toward zero. (Because the parameter range ends well into the exponential tail, the star fades essentially to black before it is reclaimed.)

Randomness: uniform random for burst size and for position within a segment; the segment round-robin is deterministic and is what keeps twinkles evenly distributed instead of clumping.

Layout: fully pixel-count agnostic; works on any strip length. Renders as 1D by strip index only.

## Color
A single fixed color for all stars: cool blue leaning to white — a blue hue at quite low saturation, so lit pixels read as frosty blue-white. Only brightness varies (per the envelope). Background is black.

## Controls
None exported. The obvious tunables (twinkle lifetime, spawn interval, maximum burst size) are compile-time values at the top of the pattern; natural improvement is to expose them as sliders (lifetime, density/rate).

## Non-obvious details
- The skewed rise/decay envelope (quadratic ramp times exponential decay) is what makes it feel like a real twinkle rather than a symmetric pulse.
- The segment round-robin spawn placement is the clever bit: purely uniform random spawning would clump; walking segments guarantees spatial spread while positions stay random within each segment.
