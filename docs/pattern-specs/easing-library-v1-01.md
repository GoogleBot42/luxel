# Easing Library v1.01
kind: 1D+2D
sensors: no

## What this is
Primarily a **code library**, not a visual effect: a collection of the standard web easing curves (the well-known set documented at easings.net), each mapping a progress value in the unit interval to an eased output, mostly also in the unit interval. A demo animation is bundled that cycles through every curve so you can see each one. A reimplementation should provide the library functions plus the demo renderers.

## Library contents
Thirty easing functions: the ease-in, ease-out, and ease-in-out variants of each of these ten families — sine, quadratic, cubic, quartic, quintic, exponential, circular, back (overshoots slightly past the ends on purpose), elastic (oscillates past the ends on purpose), and bounce (piecewise parabolic, like a ball settling). All are pure functions of a unit-interval input; the exponential and elastic ones special-case exact zero and exact one inputs to return exactly zero/one. These are all publicly documented curves; use the canonical formulations.

## Demo behavior
- The demo holds an ordered list of all thirty curves and shows one at a time, switching to the next after a fixed dwell of **about five seconds** each, looping forever.
- Within each dwell, a progress parameter ping-pongs: it ramps from zero to one over the first half of the dwell, then back down to zero over the second half.
- Because the back-family curves overshoot below zero and above one, the demo slightly scales and offsets those three entries so their output stays on-screen. (The elastic ones are shown raw.)

## 1D render
A single white dot at the strip position given by the current curve evaluated at the ping-pong progress; every other pixel is black. Visually: a white dot sweeping up and back along the strip once per dwell, with the "character" of the motion (acceleration, overshoot, bouncing) changing every few seconds as the curve changes. The dot test is "pixel's normalized index within about one pixel-width of the eased value".

## 2D render (assumes a roughly square matrix)
Draws the curve itself as a graph, plus markers:
- For each pixel, evaluate the current curve at the pixel's normalized horizontal coordinate. If the pixel's vertical coordinate is within roughly one pixel-row of that value (tolerance derived from the square root of the pixel count), light it with a **rainbow hue equal to the eased value** — so the plotted curve sweeps through the rainbow from one end to the other.
- A **white marker** near mid-height that tracks the eased position horizontally (within a small horizontal tolerance), acting as the moving "output" indicator.
- A faint **gray diagonal reference line** where the horizontal and vertical coordinates are exactly equal (linear identity, for comparison); the original notes this can be removed.
- The same 1D-style white dot rule is also applied first (by pixel index), a leftover that mostly just adds a stray white pixel.
- Everything else black.

The original also exports the running minimum and maximum of the curve's output for inspection in the variable watcher; this is debug sugar, optional.

## Controls
None.

## Layout notes
The 2D tolerance assumes width ≈ height ≈ square root of the pixel count (square matrix). Obvious fix: derive tolerances from actual matrix dimensions.

## Notes
This pattern is close to trivial visually; the value is the library. Implement the thirty canonical easing functions correctly (including the intentional overshoot of back/elastic) and the simple graph-plotting demo described above.
