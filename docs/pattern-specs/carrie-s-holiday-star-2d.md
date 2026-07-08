# Carrie's Holiday Star 2D
kind: 2D
sensors: no

(The corpus labels this pattern as sound/sensor-reactive, but the source uses no
sensor inputs at all — the "twinkle" comes from smooth noise. Spec it as a pure
2D pattern.)

## What it looks like

An eight-pointed star centered on the display: four long rays pointing
up/down/left/right and four shorter diagonal rays between them. The star
gently "twinkles" — the reach and brightness of the rays swell and shrink
organically, like starlight shimmering — and the whole figure rotates very
slowly. The star's core is bright and washed toward white; the ray tips fade
to a deeply saturated color. The base hue drifts gradually around the color
wheel over the course of minutes, so the star slowly changes color (it tends
to sit in cyan/blue territory early on). A speed slider controls how fast the
twinkle/rotation evolves.

## Algorithm

State between frames:
- A time accumulator advanced each frame by elapsed time divided by a
  user-controlled period, wrapped modulo a large value (about an hour's worth)
  so it never loses float precision. All motion derives from this.
- A base hue that is nudged upward by a tiny fixed amount every frame (note:
  per *frame*, not per unit time, so hue drift speed is frame-rate dependent —
  the obvious fix is to scale the nudge by elapsed time).

Per frame:
1. Reset the coordinate transform, translate so the display center is the
   origin, and rotate the whole frame by an angle that increases slowly with
   the time accumulator (a full turn takes on the order of minutes at default
   speed).
2. Sample smooth (perlin-style) noise at a point that moves with the time
   accumulator. This single noise value drives the twinkle: it is scaled two
   different ways to produce (a) a modulation of the axis-aligned star's ray
   reach and (b) a modulation of the diagonal star's "pointiness".

Per pixel (given centered, rotated coordinates):
- The star shape comes from a **generalized (Minkowski) distance**: instead of
  the usual square-root-of-sum-of-squares, use the p-th root of the sum of
  p-th powers of the absolute coordinates. For p between zero and one, the
  iso-distance contours are four-pointed stars with concave sides — this is
  the whole trick.
- **Axis-aligned star:** brightness is a fixed-plus-twinkle numerator divided
  by the Minkowski distance of the pixel (with an exponent a bit above one
  half), clamped to full brightness, then raised to a high power (around the
  fifth) to sharpen the falloff so rays are crisp.
- **Diagonal star:** rotate the pixel coordinates by an eighth of a turn
  (precomputed sine/cosine, done once at startup, applied per pixel), then do
  the same trick with a somewhat smaller numerator and an exponent that is
  itself modulated by the twinkle noise — so the diagonal rays change shape,
  not just length. Sharpen with a slightly lower power (around the fourth).
- Average the two star brightnesses.
- **Center-pixel fix:** on layouts with an odd pixel count, the exact middle
  pixel lands on the coordinate origin where the distance is zero and the
  math misbehaves (dark center). The pattern special-cases that one index
  (middle of the strip) and forces it to near-full brightness. This assumes
  the display's center pixel is the middle index of the strip — true for
  typical serpentine matrices with odd dimensions; a more robust fix would
  detect coordinates very near the origin instead of using the index.
- Color: hue = the slowly drifting base hue; brightness = the star value;
  saturation = a constant somewhat under two minus the brightness, so bright
  areas desaturate toward white at the core while dim ray tips stay fully
  saturated. (Saturation above one just clamps.)

## Controls

- **Speed** (slider): sets the master time period. Mapped so higher = faster;
  inversely mapped onto the period with a floor so it can never fully stop.

## Timing feel

Twinkle undulates on the order of a second or two; global rotation takes tens
of seconds to minutes per revolution; hue drift takes minutes to circle the
wheel.

## Layout assumptions

Requires a mapped 2D display with normalized coordinates. No hardcoded pixel
count except the odd-center-index brightness fix noted above.
