# Geometry Morphing Demo 2D
kind: 2D
sensors: no

## What it looks like
A single geometric figure (circle, plus/cross, six-pointed star, square, triangle, hexagon — in that repeating order) sits centered on the 2D surface while the whole scene slowly spins. Every couple of seconds the current shape smoothly melts into the next one: edges bend and flow between silhouettes rather than cross-fading. The figure is drawn in shifting rainbow colors that band outward with distance from the shape's edge; hues also drift over time. The shape can be a filled solid or just an outline, per a user control.

Rhythm: the pattern alternates between a "hold" phase (shape displayed steady) lasting about a second and a "morph" phase (blending into the next shape) also lasting about a second — so a new shape roughly every two seconds. The whole scene completes one full rotation in several seconds (on the order of five to ten).

## Algorithm
This is a pure per-pixel shader driven by signed distance functions (SDFs), the shader-toy style of rendering. No per-pixel state; all animation state is global.

Per-frame state and work:
- A morph clock accumulates elapsed time. Each time it exceeds about a second it resets and toggles a hold/morph flag. On entering the hold phase, the "current shape" index advances to the previously-targeted "next shape", and the next-shape index steps forward cyclically through the list of six shapes.
- A blend fraction is the morph clock's progress through its ~one-second window (0 at the start, 1 at the end).
- A rotation angle advances continuously with wall-clock time (one full turn per several seconds). The coordinate transform is rebuilt each frame: translate so the display's center is the origin, then rotate by that angle. (Coordinates arrive in the unit square, so the translation is by half a unit on each axis.)

Per-pixel work (given transformed x, y):
- Evaluate the SDF of the current shape at (x, y) with the user-selected size. During the morph phase, also evaluate the next shape's SDF and take the linear interpolation of the two distances weighted by the blend fraction. Interpolating raw signed distances is what produces the organic melting between silhouettes.
- Decide if the pixel is lit: in outline mode, a pixel is lit when the absolute distance is within the line-width threshold (a band straddling the shape boundary). In filled mode, a pixel is lit when the signed distance is below the threshold (interior plus the boundary band).
- If lit: brightness ramps down as distance grows relative to the line width (so edges are soft, and in filled mode the interior — where signed distance is negative — is at or above full ramp value); the brightness is squared before output for gamma. Saturation starts above full near the boundary and falls off as the pixel gets farther from the boundary relative to the object size (mostly it stays fully saturated). Hue equals the signed distance plus a slowly cycling time offset — this creates concentric rainbow contour bands that follow the shape's outline and continually drift through the spectrum.
- Unlit pixels get zero brightness.

The six SDFs are the standard 2D distance functions (see Inigo Quilez's catalog): circle, axis-aligned square, equilateral triangle, regular hexagon, six-pointed (hexagram) star, and a plus-shaped cross whose arm thickness is a small fraction of its length. The cross's interior distance is known to be slightly imperfect but visually acceptable. A tiny sign-of helper (returns +1/0/−1) is used inside the star SDF.

Layout: fully mapped-2D; no pixel-count assumptions. Works on any 2D-mapped layout with coordinates normalized to the unit square.

## Colors
Rainbow. Hue is tied to distance-from-edge plus a slow global drift, giving nested rainbow contour rings inside/around the shape that migrate over time. Near the boundary colors are vivid and fully saturated; brightness fades softly at the edge band. Background is black.

## Controls
- Slider "size": scales the figure from tiny up to a moderate fraction of the display (max around half the display's half-width).
- Slider "filled": acts as a toggle — below halfway the shape is outline-only, above halfway it is filled solid.
- Slider "line width": thickness of the outline / edge softness band. Response is quadratic (squared slider value scaled to a modest maximum) so the low end gives fine control over thin lines.

## Non-obvious points
- The morphing is achieved by lerping the *signed distance values* of two different shape functions, not by blending images — an elegant trick that yields smooth intermediate silhouettes for free.
- Because hue is derived directly from the signed distance, the rainbow banding automatically conforms to whatever hybrid silhouette the morph currently has.
- The dispatch is table-driven: an array of shape functions and a two-entry array of lit-test predicates (outline vs. filled) selected by the fill flag.
