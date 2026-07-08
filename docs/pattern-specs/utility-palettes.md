# Utility: Palettes
kind: 1D (demo chase; the point of the pattern is the reusable palette library)
sensors: no

## Purpose
This is primarily a utility/library pattern, not an effect: it implements
gradient-palette support (in the spirit of FastLED's gradient palettes) for an
HSV-oriented pattern language, plus a self-running demo. The demo is a simple linear
rainbow chase whose color lookup is remapped, about once a second, through each of
roughly fifteen example palettes in turn.

## What it looks like when running
A smooth color gradient chases along the strip (one full traversal in roughly ten
seconds). Every second or so the entire color scheme switches to the next demo
palette: sometimes hard-banded discrete colors (posterized), sometimes smooth
gradients — rainbows, blues and violets, fiery red-to-yellow with a brightness fade
along the strip, warm-white monochrome ramps, complementary colors fading through
white and black, earth tones and browns, pink-to-orange, black-violet-pink-orange-
yellow, dusky pink-to-purple, minty greens, greens with pinks, and a dawn-sky
gradient.

## Palette representation
A palette is a flat array of fixed-point numbers of a single agreed-upon length (the
demo uses eight stops; the length is a shared global since arrays don't know their
own size). Each entry packs two things:
- Integer part: the stop's position along the input range, expressed on a scale of
  zero to one-thousand-minus-one.
- Fractional part: the output value at that stop (a hue, saturation, or brightness in
  the zero-to-one range).

Positions must be sorted ascending. Separate arrays are used for hue, saturation, and
brightness; a full "grading" palette is a three-element array holding one of each.

## Core remap algorithm
Given an input in zero-to-one and a palette, first wrap the input into zero-to-one
the same way hue wrapping works. Scale it to the position range. Then:
- If the input position falls between two adjacent stops, compute the fraction of
  the way between them and interpolate the two stops' output values (subject to the
  mode below).
- If the input position falls outside the span of stops (before the first or after
  the last), treat the gap from the last stop, wrapping around the end of the scale
  back to the first stop, as one more segment and interpolate across it. This makes
  palettes seamless for cyclic inputs.

Three interpolation modes:
1. Posterize: no interpolation; return the earlier stop's output as-is, giving hard
   bands of discrete colors. (Out-of-span inputs return the last stop's value.)
2. Hue interpolation: linear interpolation taking the *shortest path around the hue
   wheel* — if the two stop values differ by more than half the wheel, go the other
   way around (the signed distance is adjusted by a full turn), and the result wraps
   into zero-to-one. This is what lets a palette fade red-to-violet through magenta
   rather than trudging through the whole rainbow — or, by inserting intermediate
   stops, force the long way (e.g. red to violet via green requires explicit
   waypoints, since each segment independently takes its own shortest path).
3. Plain interpolation: linear, no wheel wrapping — appropriate for saturation and
   brightness, where fading from high to low should never detour "through the wrap".

The grading helper runs one input through a hue palette (mode 2), a saturation
palette (mode 3), and a brightness palette (mode 3) and emits the pixel color
directly — full color grading from a single scalar input.

## Demo scaffolding
Per frame: a slow clock phase for the chase, and a mode index derived from a longer
clock cycle so each demo palette holds for about a second before advancing.
Per pixel: the pixel's normalized position minus the chase phase (wrapped) is the
input scalar; it is passed to the current mode's small lookup lambda, which applies
one or more palettes and sets the color. Some modes also use the raw (un-animated)
pixel position as a second input, e.g. the fiery mode dims brightness toward the far
end of the strip; one mode deliberately feeds transformed inputs (doubled-and-
wrapped, or reversed) into the saturation and brightness palettes for variety.
Several modes square their saturation or brightness outputs before display because
convex curves look better on LEDs.

## Colors (qualitative stop lists of notable demo palettes)
- Two rainbow modes: one posterized into hard rainbow bands, one smooth; plus a
  "perceptually even" rainbow that spaces hue stops to counteract the wheel's
  green/cyan bloat.
- Blues and violets, shown both posterized and smoothly graded.
- Fire: deep reds holding through mid-strip then rising through orange to yellow,
  with brightness falling off along the strip.
- Warm monochrome: a stepped ramp of warm whites, highlighting how white's apparent
  color temperature shifts with brightness.
- Complementary opposites: red to cyan and purple to green, fading through white
  (via desaturation dips) and through black (via brightness dips), never passing
  through the in-between hues.
- Earth tones/browns: narrow warm hue range with strong saturation and very low
  brightness.
- Pink to orange skipping most of red; black-through-violet-pink-orange-yellow;
  pink-to-purple passing through a desaturated middle; minty greens; minty greens
  with pinks; and a dawn sky (deep warm horizon through pale mid-tones to blue).

## Controls
None. Mode advancing and chase motion are automatic.

## Layout assumptions
Any strip length; everything is normalized by pixel count. Timing note: clock periods
are expressed against the platform's peculiar time-unit base (about sixty-five
seconds), which the code converts so the chase is about ten seconds and the mode
dwell about one second.

## Clever / non-obvious
- Packing (position, value) pairs into single fixed-point numbers keeps palettes as
  compact flat arrays in a language without structs.
- The explicit three-mode design separates "hue math" (wheel-wrapping shortest path)
  from "scalar math" (no wrap), which is the key correctness insight for HSV palette
  interpolation.
- Wrap-around segment handling makes any palette seamless for cyclic hue inputs even
  if its stops don't start at position zero or end at the top.
