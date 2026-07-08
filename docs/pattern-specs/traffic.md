# Traffic
kind: 2D
sensors: no

## What it looks like

Bright, constantly morphing, fractal-ish geometric line art on a 2D panel: thin glowing contour lines that weave into moiré lattices, concentric families of curves, diamonds, and grids, continuously deforming from one configuration to the next. The line cores are near-white; away from the cores the color is a fully saturated hue that cycles steadily around the rainbow every few seconds. The whole texture also slowly wobbles/rotates as a unit, and the overall figure repeats as tiles across the display. The author describes it as hypnotic and hard to photograph — it never settles.

## Algorithm

Coordinate frame: each frame the transform is reset, coordinates are recentered on the middle of the map, and a uniform zoom (user scale setting) is applied.

State kept between frames:
- A time accumulator advanced by the frame delta in seconds divided by the speed setting, wrapping after about an hour.
- A shape clock derived from it: a small offset plus (the accumulator divided by a few) folded modulo a moderate range — this confines the pattern-shaping parameter to the "interesting" region and makes the overall figure evolve through a long ramp (on the order of a minute) before jumping back.
- A pair of mixing weights: one is a tiny sinusoidal wobble (amplitude well under a percent) of the accumulator; the other is one minus that. These drive the slow global rotation/shear.

Per pixel (2D renderer):

1. Minsky-style shear-rotate: replace x with (x times the big weight plus y times the tiny weight), then replace y with (y times the big weight minus the *already-updated* x times the tiny weight). Using the updated x in the second step — the signature of the Minsky circle algorithm — is deliberate; it's what makes the deformation interesting rather than a clean rotation.
2. Tiling: fold each coordinate with a triangle-fold (multiply by the repeat count, take modulo two, distance from one) and scale up several-fold. This mirrors/tiles the figure across the display and expands the working range.
3. Line field: combine the folded x and y with one of four selectable two-argument functions — their sum, their maximum, their product, or their Euclidean distance. Take the sine of (the shape clock times that combined value). The pixel's line intensity is the line-width setting divided by (the line-width setting plus the absolute value of that sine) — an inverse falloff that is one exactly on the sine's zero contours and decays away from them, with the width setting controlling how fat the glowing contour lines are. As the shape clock grows, the sine's frequency rises, so the contour families get denser and shift continuously.
4. Color: hue is the raw time accumulator (wrapping around the hue wheel, so a full rainbow cycle every few seconds) plus a small fraction of the line intensity; saturation is a value above one minus the line intensity — clamped to full saturation everywhere except near line cores, where it drops and the line whitens; brightness is the line intensity squared, so everything off-line falls to black quickly.

Randomness: none — fully deterministic.

Layout assumptions: needs a 2D map; nothing tied to pixel count. No 1D renderer.

## Colors

Black background; thin luminous lines whose cores bleach toward white and whose flanks take the current hue. The hue itself cycles continuously through the entire rainbow every few seconds, so the piece is always monochrome-plus-white at any instant but never the same color for long.

## Controls

- Slider, "line width" concept: sets contour thickness over roughly a small-to-moderate range, eased quadratically so the low end has fine control. Thin = delicate wireframe; thick = bold glowing bands.
- Slider, "speed" concept: inverted mapping (right = faster); scales the time divisor over about a three-to-one range. (It also computes a second movement-speed value that nothing else uses — dead code; omit it.)
- Slider, "repeats" concept: integer tile count from one to about six; higher = more, smaller copies of the figure.
- Slider, "scale" concept: uniform zoom from a bit under unity to about double.
- Slider, "mode" concept: selects which of the four combining functions is used (sum / max / product / distance), each giving a distinct family of figures — diagonal line lattices, square/diamond grids, hyperbola-like webs, and concentric rings respectively.

## Timing

Hue cycles fully in a few seconds. The figure morphs continuously; the shape clock's full ramp before wrapping takes on the order of a minute, and the wrap causes a sudden reset to a sparser figure. The global wobble is a slow, subtle sway on a several-second period.

## Non-obvious details

- The "rotation" intentionally reuses the already-modified x when computing y (Minsky circle trick), so the transform is a shear pair, not a true rotation — this accumulating asymmetry is the source of the fractal-ish distortion.
- The wobble weights differ from a pure rotation matrix (one weight is near one, the other near zero, rather than cosine/sine of the same angle), which slightly scales as it shears; the effect depends on this being tiny.
- The inverse falloff (width over width-plus-distance-from-contour) gives lines with hot cores and long soft tails — much smoother on LEDs than a threshold would be.
- Driving saturation *down* with line intensity (from an over-unity baseline that clamps to full) is how the white-hot cores are produced without a separate white pass.
