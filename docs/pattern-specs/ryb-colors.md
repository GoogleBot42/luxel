# RYB colors
kind: 2D
sensors: no

## What it looks like
A full color wheel painted across the mapped display, slowly rotating about the center like a
turntable — one full revolution takes on the order of ten seconds. The wheel can be shown in
two flavors: the ordinary HSV hue wheel, or a painter's **RYB (red-yellow-blue) wheel**, in
which the spacing of hues matches the traditional artist color wheel — green occupies a much
narrower slice, orange and purple get more room, and complements sit opposite the way painters
expect (e.g. red opposite green, yellow opposite purple). The RYB version reads as warmer and
more "pigment-like" than the neon HSV wheel.

## Algorithm
Per frame: reset the coordinate transform, move the origin to the center of the display, and
rotate by an angle that advances continuously with a medium-period clock (several seconds per
revolution).

Per pixel: compute the angle of the (transformed) pixel around the origin and normalize it to
the unit interval — that fraction is the hue position. Radius is ignored, so the wheel is
constant along each ray from the center.

- In HSV mode, the hue position, a saturation setting, and a brightness setting go straight to
  the standard HSV conversion.
- In RYB mode, the hue is first converted through an HSL-flavored variant of the usual
  hue-to-RGB sextant algorithm: the brightness slider behaves like HSL lightness (darkening
  toward black at one end) and the saturation slider desaturates toward white/gray, computed
  with the standard HSL-to-HSV value/saturation conversion. The resulting three channel values
  are each **squared** (gamma-ish shaping that favors LED appearance), and then treated not as
  RGB but as **R, Y, B primaries** which are mapped to actual RGB.

The RYB-to-RGB mapping is trilinear interpolation over the corners of a unit cube whose eight
corners are assigned reference colors (the published "RYB color model" technique: white at the
origin corner, then yellow, red, orange, blue, green, purple, and black at the other corners),
using a smoothstep-style cubic ease on each axis instead of straight linear blending. This
implementation uses a simplified corner set (pure primaries/secondaries rather than the
original softened values), which lets most of the interpolation collapse to a handful of terms
per channel.

State between frames: none beyond the rotation clock.

## Colors
Entire hue circle. HSV mode: standard evenly-spaced rainbow. RYB mode: artist's wheel —
red through orange to yellow occupies roughly a third, yellow through a compressed green to
blue another third, blue through purple back to red the rest; at full settings the colors are
vivid and slightly warm.

## Controls
- Slider, "wheel type": effectively a two-position switch (rounded to nearest end) choosing
  RYB wheel vs. standard HSV wheel.
- Slider, "brightness": overall lightness; in RYB mode it acts like HSL lightness (bottom is
  black).
- Slider, "saturation": color purity; in RYB mode low values wash toward white/gray in the
  HSL sense.

## Layout assumptions
Needs a 2D mapped layout centered anywhere (it re-centers itself). No 1D renderer; a
reasonable 1D adaptation is to treat the normalized strip position as the angle directly.

## Non-obvious tricks
The interesting part is the two-stage color path in RYB mode: hue → sextant conversion (with
HSL-style lightness/saturation semantics) → per-channel squaring → cube-corner interpolation
that reinterprets the channels as red/yellow/blue paint amounts. The cubic-eased trilinear
corner blend is what produces the non-uniform hue spacing of the painter's wheel.
