# Blue Holiday Star 2D
kind: 2D
sensors: no

(Note: despite being tagged sound-reactive in some catalogs, this pattern reads no sensor inputs; the "twinkle" is noise-driven.)

## What it looks like
A single eight-pointed star (two overlapped four-pointed stars, one axis-aligned and one turned an eighth of a turn) glows at the center of a mapped 2D display. It is icy cyan-blue with a core that washes out to near-white, and long tapering rays that stay saturated blue as they fade into black. The star gently "twinkles": its rays continuously swell and slim in an organic, non-periodic way, and the whole figure rotates very slowly. Calm, wintry, ornament-like.

## Core trick: Minkowski distance
The four-pointed star shape comes from a generalized distance function: instead of the Euclidean root-of-sum-of-squares, use the p-th root of (|x|^p + |y|^p). With an exponent p between zero and one, the iso-distance contours of this function are concave four-pointed stars (points along the axes). Brightness at a pixel is a size constant divided by this distance, capped at full, then raised to a high power (fourth-to-fifth power) to sharpen the falloff into crisp rays.

## State and per-frame work
- A time accumulator advances by frame time divided by the speed setting, wrapping after a very long interval (about an hour) to avoid precision loss.
- Each frame the coordinate transform is reset and the display recentered on the origin; a rotation is applied whose angle grows slowly with the accumulator (wrapping every half-turn), so the star imperceptibly spins.
- A smooth 1D stream of perlin-style noise is sampled at the accumulator (with fixed arbitrary values for the other noise coordinates). Two scaled copies of this noise value become this frame's twinkle amounts — one modest, one somewhat larger.

## Per-pixel work
1. First star (axis-aligned rays): Minkowski distance of the pixel with a fixed exponent around one-half; brightness = (a base size plus the smaller twinkle amount) divided by that distance, capped at one, raised to roughly the fifth power. The noise thus modulates this star's size.
2. Rotate the pixel coordinates by a precomputed eighth-turn (sine/cosine cached at startup — a small optimization worth keeping).
3. Second star (diagonal rays): Minkowski distance of the rotated point, but here the exponent itself is a base value around a third minus the larger twinkle amount — the noise modulates the pointiness of this star. Brightness = a fixed size constant divided by that distance, capped, raised to roughly the fourth power.
4. Average the two brightnesses.
5. Color: fixed cyan-blue hue. Saturation is a constant somewhat above full minus the brightness, so the bright core desaturates toward white while dim ray tips remain deep blue (relies on saturation clamping to the valid range). Brightness as computed.

## Controls
One slider, "speed": inversely mapped to the timebase divisor (with a floor so it never fully stops); higher = faster twinkle and rotation. Full range spans roughly stately-slow to lively.

## Timing feel
Twinkle undulates on a timescale of a second or so at defaults; rotation is far slower — a fraction of a degree per second, only noticeable over minutes.

## Layout assumptions
Requires a mapped 2D display; fully resolution-independent otherwise. No hardcoded pixel counts.
