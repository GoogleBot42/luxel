# Cellular Automata 1D
kind: 1D
sensors: no

## What it looks like
An elementary (Wolfram-style) one-dimensional cellular automaton played out
live on the strip: each pixel is a cell, and roughly ten times per second the
whole strip steps to the next generation. Depending on the chosen rule number
you get chaotic sparkle, growing fractal triangles, gliding structures, or
quickly-dying blinkers. Colors sweep through a configurable slice of the
rainbow; in the default mode long-lived cells grow brighter and shift hue with
age, giving an organic, glittery look. Periodically (configurable, tens of
seconds; or never) the automaton resets and reseeds itself.

## Algorithm
State kept between frames:
- Two cell-state arrays the size of the strip (current and next generation),
  accessed through two references that are swapped after each generation —
  double buffering by pointer swap, so no copying and only the current array
  needs zeroing at reset.
- A per-pixel "hue/age" array.
- A running normalization maximum, an accumulated-lifetime clock, and a
  step-timer accumulator.

Generation step (runs only when the step timer passes a fixed interval of
roughly a tenth of a second; between steps the display holds still):
- For every cell, gather its left neighbor, itself, and its right neighbor
  (wrapping around both ends of the strip) into a 3-bit number (left as the
  high bit).
- The rule is an integer from 0 to 255 treated as a bit table: the new cell
  value is the bit of the rule number at the position given by that 3-bit
  neighborhood code. This is the classic Wolfram encoding — the rule number's
  binary representation *is* the entire transition function.
- Coloring, two modes:
  - Age mode (default): if the cell is alive in the new generation, its age
    counter (stored in the hue array) increments; if dead it resets to zero.
    Track the maximum age seen this generation for normalization.
  - Rule-index mode: the hue value is the 3-bit neighborhood code divided by
    seven — up to eight discrete colors showing *which* rule case fired,
    giving a bright, digital, retro-computer-display look. Normalization is
    pinned to one in this mode.
- Swap the two state array references.

Reset/reseed: an accumulated-lifetime clock triggers reinitialization once it
exceeds a configurable lifetime (zero meaning run forever). Reinitialization
zeroes the current state and the age array, then seeds: if the seed count is
one, light exactly the center cell; otherwise light up to N cells at uniformly
random positions (collisions allowed, so occasionally fewer than N).

Per pixel (render):
- Normalize the pixel's hue/age value by the generation maximum to get a
  zero-to-one quantity.
- Hue = a palette offset plus that quantity times a palette width (both from
  sliders), full saturation, and brightness equal to the quantity squared
  (squaring dims young/low-index cells and makes veterans pop).

Randomness: only the seeding positions at (re)initialization.

Layout assumptions: purely index-based 1D; neighborhoods wrap at the ends.
The center-cell special case uses half the pixel count as the index. Nothing
hardcoded to a specific strip length.

## Colors
Fully saturated hues from a rainbow palette, windowed: one slider slides the
window's starting hue around the wheel, another sets how much of the wheel the
window spans (from near-monochrome to full rainbow). In age mode brightness and
hue both climb with cell age, so the oldest cells are the brightest and
furthest along the palette. In rule-index mode you get up to eight distinct
flat colors. Dead cells are black.

## UI controls (all sliders)
- Starting cells: how many random seed cells on reset, from one (center cell
  only) up to about half the strip. Touching it forces an immediate restart.
- Rule: selects the rule number across the full 0–255 range. (Odd-numbered
  rules tend to strobe, since lone empty cells switch on then die.)
- Lifetime: how long before auto-reseed, from zero (forever) up to roughly half
  a minute.
- Color mode: acts as a two-position switch (age mode vs. rule-index mode).
- Palette width: how much of the hue wheel the display spans.
- Palette offset: where on the hue wheel the palette starts.

## Timing feel
Generations tick about ten times per second — fast enough to feel alive, slow
enough to watch structures move. Full pattern lifecycle (seed → evolve →
reseed) defaults to a decent fraction of a minute.

## Non-obvious details
- The rule-number-as-bit-table trick makes the entire CA update one mask/shift
  test per cell.
- Double-buffering via reference swap is required for correctness (all cells
  must update from the same generation) and is also the cheap way to do it.
- Brightness-squared plus max-age normalization is what turns a binary
  automaton into something with visual depth.
- The automaton runs once immediately at pattern load (seed on startup).
