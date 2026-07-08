# Mandelbrot 2D
kind: 2D
sensors: no

## What it looks like
A living fractal on a 2D matrix: swirling filaments and lobes of rainbow color around black holes and tendrils. The view continuously morphs — the fractal's shape stretches, blooms, and collapses as an animation parameter sweeps back and forth over a cycle of ten-or-so seconds, while the whole rainbow palette independently rotates through hues every few seconds. Points "inside" the fractal are pure black; the boundary regions band through the full hue wheel. On a small matrix (16×16-ish) it reads as constantly-reforming psychedelic paisley rather than a recognizable fractal zoom.

## Algorithm
Non-obvious core fact: despite the name, this is mathematically an animated **Julia set**, not a Mandelbrot set. Each pixel's coordinate is used as the *starting point* of the iteration, and a single frame-global complex constant is added each step — that's the Julia construction (Mandelbrot would use the pixel coordinate as the added constant). The animation moves that constant around, which is exactly why the whole shape morphs dramatically; a true Mandelbrot render would be static. Reproduce the Julia behavior; keep the pattern's name.

State between frames: none beyond the two frame-global animation values below (recomputed each frame from the clock).

Per frame:
- A triangle wave with a period on the order of ten to fifteen seconds, recentered around zero and scaled to swing roughly plus-or-minus one, drives the Julia constant. The constant = a hand-tuned base point plus this sweep offset. The base point sits near the fractal boundary (real part somewhere around minus one, imaginary part a small positive fraction — chosen by the author by eye so the sweep path crosses visually rich territory). The same sweep offset is added to both real and imaginary parts, but the imaginary contribution is scaled down to roughly forty percent of the real one, so the constant travels along a shallow diagonal line through parameter space.
- A sawtooth phase cycling every few seconds provides a global hue rotation offset.

Per pixel (2D render):
1. Take the normalized map coordinates and recenter so the display's middle is the origin, spanning about half a unit in each direction (i.e. the view window covers roughly a unit-wide square centered on the origin of the complex plane; kept small partly so intermediate values fit the platform's 16.16 fixed-point range).
2. Standard escape-time iteration: repeatedly square the complex value and add the frame's Julia constant. Stop when the squared magnitude exceeds the conventional escape bound (magnitude two) or when the iteration cap is reached.
3. Color: if the point escaped, hue = the global hue offset plus (escape iteration ÷ iteration cap) — so faster escapes get hues earlier in the wheel, later escapes later, producing the banded rainbow — at full saturation and full brightness. If it never escaped (an "inside" point), output black.

Randomness: none. Fully deterministic from the clock.

Layout: requires a 2D pixel map with normalized coordinates; no pixel-count hardcoding. Works on any matrix size, but per-pixel cost is iteration-cap × pixel count, so the author warns big displays need a lower cap. There is no 1D fallback renderer.

## Colors
Full rainbow wheel, fully saturated, at full brightness in escaped regions; solid black for non-escaping regions. No fixed palette — the hue wheel itself rotates continuously, so any given band cycles through every color over a few seconds.

## Controls
One slider, "iteration depth" (detail vs. speed trade-off): maps the slider range onto an integer iteration cap from about five at the low end to just under twenty at the top. Low = chunkier, blobbier fractal but faster frames; high = finer boundary detail but heavy CPU (the author notes the platform can become unresponsive if pushed too far on large displays — worth keeping the cap conservative, though our platform may tolerate more). Default cap is in the mid-teens, tuned for a 16×16 matrix.

## Timing
- Shape morph (parameter sweep): one full back-and-forth in roughly ten to fifteen seconds.
- Hue rotation: a full trip around the color wheel every few seconds.

## Notes for reimplementation
- The escape test compares squared magnitude against the squared escape radius to avoid a square root — standard trick, keep it.
- Brute force per-pixel per-frame; no caching or incremental tricks. Frame rate degrades gracefully with the iteration slider.
- The base constant and sweep amplitude were hand-tuned as a pair with the default iteration cap; if a reimplementation's view looks empty or dull, nudge the base point along the fractal boundary rather than assuming a math error. Any base point near the boundary with a similar shallow-diagonal sweep gives an equivalent look.
