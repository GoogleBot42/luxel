# Bubble Column
kind: 1D (designed for a vertical, well-diffused strip)
sensors: no

## What it looks like
A tube of colored "fluid" with bright pale bubbles rising through it — like an aquarium airstone or a lava-lamp column. The background fluid glows dimly in a base color whose hue swirls subtly along the strip, as if the liquid were slowly stirring. Bubbles (around ten of them alive at once) enter at the bottom, accelerate as they rise, and slide off the top. They don't arrive at a steady rate: a hidden "valve" opens and closes irregularly, so bubbles come in bursts followed by lulls. Each bubble is a small, sharply peaked glow a few pixels wide, noticeably brighter and much paler (desaturated) than the fluid. When bubbles overlap they interact with a slightly fizzy, effervescent look rather than smoothly summing.

## Algorithm
State between frames:
- per-bubble position (in pixel units along the strip) and per-bubble velocity,
- a per-pixel brightness buffer for bubble contributions,
- a running clock (seconds, wrapped after about an hour).

Initialization: all bubbles start positioned beyond the top of the strip, so the display begins as plain fluid and bubbles trickle in naturally through the normal reinjection mechanism.

Per frame:
1. Compute the valve state: a smooth pseudo-random value from perlin noise sampled along the time axis (two time-derived coordinates advancing at different rates), rescaled to a 0..1 range. This makes bubble release wax and wane organically over several-second scales.
2. Fill the per-pixel buffer: for each pixel, sum contributions from every bubble. A bubble's contribution is a linear falloff with distance (zero beyond the bubble radius, which is a few pixels) raised to the fourth power — a very sharp, rounded peak. The inner loop bails out early once a pixel's accumulated value already exceeds the dim fluid level; this is both an optimization and the source of the "effervescent" merge look (overlapping bubbles clip each other instead of blending smoothly). Removing the early-out gives smoother merging, at some cost.
3. Move bubbles: advance each position by velocity times elapsed seconds; add a small constant per-frame acceleration to velocity so bubbles speed up as they rise (buoyancy feel). A bubble that has fully passed the top of the strip (it's allowed to run a little past the end so its glow decays off-screen naturally) is reinjected at the bottom only if the current valve value exceeds the openness threshold; otherwise it waits off-screen, which is what creates the burst/lull rhythm. Reinjected bubbles get a fresh random speed: a base speed plus a uniform random spread of about plus-or-minus three-quarters of that base.

Per pixel (render): compute a small hue perturbation from perlin noise sampled along the strip (about a dozen noise cells over the length) and drifting slowly with time — this is the fluid "swirl". If the pixel's bubble value is at or below the dim fluid threshold, draw fluid: base hue plus the perturbation, fully saturated, at low fixed brightness (well under a tenth of full). Otherwise draw bubble: same perturbed hue but roughly half saturation, with brightness equal to the accumulated bubble value — so bubbles read as bright, milky versions of the fluid color.

Layout: fully driven by the actual pixel count; nothing hardcoded to a specific strip length. Bubble radius and bubble count are fixed constants (a natural pair of extra sliders if desired).

## Colors
Default fluid: a deep blue at full saturation, glowing dimly. Bubbles: the same hue family but pale/washed-out and bright. The hue of both wanders gently along the strip and over time thanks to the noise perturbation — enough to feel liquid, not enough to change the identity of the color.

## Controls
- Color picker ("fluid hue"): sets the base hue of the fluid (and therefore of the bubbles, which are the pale version of it).
- Slider ("bubble valve" / release rate): controls how readily new bubbles are released at the bottom. Higher slider = valve threshold easier to meet = more frequent bubbles; low settings make bubbles rare.

## Timing
Bubbles take a few seconds to traverse a typical strip, visibly accelerating. Bursts and lulls in bubble release play out over several seconds. The fluid hue swirl evolves over tens of seconds.

## Non-obvious bits
- The valve driven by smooth noise, gating reinjection of recycled bubbles, is what makes the rhythm feel organic instead of periodic.
- The fourth-power falloff makes bubble edges crisp on a diffused strip, and the early-exit accumulation deliberately produces the fizzy interaction between overlapping bubbles.
- Letting bubbles travel slightly past the strip end before recycling avoids a visible pop at the top.
