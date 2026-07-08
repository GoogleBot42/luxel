# Orv - Christmas Tree
kind: 2D (the 3D renderer just delegates to the 2D one, ignoring depth)
sensors: no

Note: upstream metadata tags this as sound/sensor-reactive, but the code reads no
sensor or audio inputs. All "twinkle" comes from pseudo-randomness. (A small
color-picker debugging helper exists but is commented out.)

## What it looks like
A static "drawing" of a Christmas tree on an LED wall/matrix: a green triangular
tree in the middle of the panel, a small warm-yellow block on top as the tree
topper, a strip of brown ground along the bottom, and a dusky mid-blue night sky
filling the rest. On top of this static scene, small warm lights twinkle: on the
tree they read as tree lights, in the sky they read as stars. Rainbow-colored
"ornaments" appear as short fixed blocks scattered over the tree, and two pale
slanted stripes cross the tree as garland. Twinkles fade in and out over roughly
a second or two each, continuously, at random places.

## Coordinate assumptions
Assumes a 2D mapped installation with normalized coordinates. The pattern shifts
the horizontal axis so x runs from about minus one half to plus one half with
zero at the panel's horizontal center; y runs from zero at the top to one at the
bottom. Everything is drawn in that centered space. The 3D entry point simply
calls the 2D one with x and y, so a 3D map renders as a projection.

## State kept between frames
- A per-pixel brightness buffer, one entry per pixel, holding the current
  twinkle intensity contribution at that pixel.
- A pool of "star" particles, sized at roughly one-twentieth of the pixel count.
  Each particle has (a) a signed value that acts as both its brightness
  contribution and its drift rate, and (b) a fractional pixel position (a raw
  index into the strip, not a coordinate).

## Per-frame work (before rendering)
1. Multiply every entry of the per-pixel buffer by a decay factor slightly
   below one (so lit pixels fade over dozens of frames).
2. For each particle:
   - If its value has decayed into a small band around zero, respawn it: give it
     a new value drawn uniformly from a small symmetric range around zero
     (so values can be negative), and a new random pixel position anywhere on
     the strip.
   - Multiply its value by a decay factor just under one (slow exponential
     decay toward zero).
   - Drift its position by an amount proportional to its value and the frame
     delta (very slow — the motion is subtle). Wrap the position around the
     ends of the strip.
   - Add the particle's (signed) value into the per-pixel buffer at the integer
     pixel under its position. Negative-valued particles actively dim pixels,
     which keeps the field from saturating and makes twinkles asymmetric.

## Per-pixel rendering (2D)
Work through these region tests in order; the first match wins:
1. **Tree topper**: if the pixel is within a narrow horizontal band around
   center (absolute x below about a tenth) and in a band near the top (y between
   roughly a tenth and a third), draw a fixed warm light-yellow (golden, fairly
   saturated, full brightness).
2. **Tree triangle**: if absolute x is less than roughly seven-tenths of y and y
   is above the ground line (below about 85% down the panel), the pixel is on
   the tree:
   - **Ornaments**: a deterministic test on the raw pixel index selects short
     runs (blocks of about two consecutive indices out of every ten or so).
     Those pixels get a fully-saturated, fully-bright hue that advances slowly
     with pixel index, so ornaments come out in assorted rainbow colors, fixed
     in place. (This relies on the raw wiring order, so ornament placement is
     essentially arbitrary speckle — that is the intent.)
   - **Twinkle lights**: otherwise, if this pixel's twinkle-buffer value exceeds
     a small threshold, draw a warm golden light whose brightness is the buffer
     value squared and then boosted by a large gain (squaring plus gain gives a
     snappy sparkle response).
   - **Garland**: otherwise, if the pixel falls in either of two thin diagonal
     bands (computed as y minus a small x-proportional slant lying in one of two
     narrow ranges around mid-tree), draw a pale, low-saturation gold at
     moderate brightness.
   - **Foliage**: otherwise, saturated green at high-but-not-full brightness.
3. **Ground**: if y is below the tree's base line (bottom ~15% of the panel),
   draw a dim, earthy orange-brown.
4. **Sky**: for everything else, if the twinkle-buffer value exceeds the same
   small threshold, draw a star: a desaturated pale gold with the same
   squared-and-boosted brightness; otherwise draw the background, a medium-dark
   dusky blue.

## Colors, qualitatively
- Topper: warm light yellow/gold, bright.
- Ornaments: full rainbow assortment, vivid.
- Tree lights and stars: warm candle-gold; tree lights more saturated, sky
  stars paler.
- Garland: pale champagne gold, subdued.
- Foliage: rich green.
- Ground: muted brown-orange.
- Sky: dusky slate blue.

## Timing feel
Individual twinkles bloom and fade over the order of a second. The particle
respawn cadence keeps a steady sprinkle of new twinkles going; nothing else in
the scene moves.

## UI controls
None active.

## Hardcoding / portability notes
Nothing depends on an absolute pixel count (particle count scales with it), but
the scene geometry assumes the map fills the unit square with y increasing
downward and the pattern centered after the horizontal shift. Ornament spacing
uses raw index arithmetic, so on a different wiring it produces a different
(but equally acceptable) speckle. The particle-position wrap check only clamps
to the ends after a single step, which is fine at these tiny drift speeds.
