# Animated Asterisks 2D
kind: 2D
sensors: no

## What it looks like

An asterisk: several straight line segments, each spanning the full display and passing through the center, evenly fanned out over a half-turn so together they form a star/asterisk figure. The whole figure rotates continuously at a steady rate (a revolution every couple of seconds at defaults). Each arm has its own hue, evenly distributed around the color wheel, and all hues drift together over time. Arms have soft edges — brightest along their centerline, fading linearly to black at their edge. By default the arm thickness slowly breathes thick and thin (a triangle-wave modulation over roughly a minute), which at the thin extreme dissolves the arms into sparkly particle-like dots (an aliasing effect the author considers a feature) and at the thick extreme floods most of the matrix with overlapping color. Background is black.

## Algorithm

State between frames: per-line arrays (sized to a maximum of a few dozen lines) holding each line's two endpoints and its hue.

Per frame:
- Derive the current rotation angle from a sawtooth time phase scaled to a full turn, and a hue base from a second, slower sawtooth.
- If width animation is enabled, recompute the arm half-width from a triangle wave with a period around a minute, mapped through the same formula the width slider uses.
- For each of the N active lines: hue = base hue + (line index / N), wrapped. Angle = rotation angle + half-turn × (line index / N) — i.e. lines evenly spread across a half-turn (a half-turn suffices because each segment spans the whole display through the center). Endpoints = center of the unit square plus/minus half-length times (cos, sin) of that angle, so every segment runs corner-to-corner-ish through the middle.

Per pixel:
- Loop over the lines in order; compute the pixel's true Euclidean **distance to the line segment** (not the infinite line): if the pixel projects beyond either endpoint (checked via dot-product signs against the segment direction), use distance to the nearer endpoint; otherwise use the perpendicular distance (cross-product magnitude over segment length).
- The first line whose distance is under the arm half-width wins: emit that line's hue at full saturation, with brightness = one minus (distance / half-width) — a linear falloff to the edge. Return immediately (no blending between overlapping arms; earlier lines occlude later ones near the center).
- If no line matches, emit black.

No randomness.

**Layout assumption**: the matrix is assumed square — its width is inferred as the square root of the pixel count, and that inferred width scales the default/derived line widths (widths are chosen in roughly per-pixel-row units). On non-square layouts widths will be off. Obvious fix: derive width from the actual map dimensions or express widths directly as fractions of the unit square.

**Performance note** (from the author): per-pixel segment distance for many lines is heavy; on some LED protocols too many lines starves the output timing and corrupts the first row, and a buffered output mode fixes it. An implementer on different hardware can ignore this, but should keep the early-exit in the per-pixel loop.

## Colors

Full-saturation rainbow. N arms take N evenly spaced hues spanning the whole wheel, and the entire set drifts continuously around the wheel over seconds to tens of seconds. Arms fade to black at their edges; background is black.

## Controls

All sliders:
- **Number of lines**: one up to a few dozen. Default: about half a dozen.
- **Line width**: from hairline (sub-pixel, giving the particle/sparkle effect) up to arms wide enough to cover the whole matrix. The scale of this slider adapts to matrix size and line count (more lines → thinner maximum per line).
- **Animate width**: acts as a toggle (on except at the very bottom of travel); when on, overrides the width slider with the slow triangle-wave breathing described above. Default on.
- **Rotation speed**: inverted and eased (quadratic) — pushing the slider up speeds rotation; range spans roughly a tenfold speed change, from a revolution every few seconds down to several revolutions per second. Never fully stops.
- **Color speed**: same inverted/eased shape for the hue drift, ranging from a hue cycle in well under a second up to one taking tens of seconds.

## Timing

Rotation: a full turn every couple of seconds at defaults. Hue drift: a full wheel in ten-to-fifteen seconds at defaults. Width breathing: about a minute per thick-thin-thick cycle.
