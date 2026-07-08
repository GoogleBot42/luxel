# tree setup pattern
kind: 3D
sensors: no

## Purpose and appearance
A static diagnostic pattern for verifying/adjusting a 3D pixel map of a cone-shaped tree (e.g. a mapped Christmas tree). It paints four vertical colored stripes running top to bottom, one per compass quadrant around the tree's axis: roughly red on the left, a green on the front, a teal/cyan on the right, and purple at the back. Pixels near the middle of each quadrant are lit in that quadrant's color; pixels near quadrant boundaries are dark, so you see four crisp vertical bands separated by dark gaps. Nothing animates — it only changes when the user moves the slider.

## Algorithm
Coordinate setup (done once, using the built-in map-transform facilities): shift the map so its center is at the origin, then rotate about the vertical axis by an eighth of a turn so that a quadrant *seam* is centered at the back rather than a stripe (equivalently, so stripe centers land at left/front/right/back).

Per pixel (3D renderer, given mapped x/y/z):
1. Compute the azimuth of the pixel around the vertical axis via two-argument arctangent of the horizontal coordinates, normalized to the unit range (with the offset noted above baked in).
2. Quantize that normalized angle into four equal sectors; the sector index (divided by the sector count) directly becomes the hue, giving the four fixed quadrant colors listed above at full saturation.
3. Compute a triangle wave over the angle with four peaks per revolution, phased so each peak sits at the center of a sector — this yields a value that is highest mid-stripe and falls to zero at sector boundaries.
4. Threshold that triangle value against the slider: pixel is fully on if above the threshold, fully off otherwise (hard on/off, no gradient).

No state between frames; no randomness; no pixel-count assumptions. Height (z) is unused — stripes are purely angular, hence "top to bottom".

## Controls
One slider, concept "stripyness" (stripe thickness): it is the on/off threshold. Low values make the stripes wide (almost touching); high values make them narrow slivers at each quadrant's center. Default is mid-range.

## Colors
Four fixed, fully saturated hues equally spaced around the color wheel starting at red: red, a yellow-leaning green, a cyan/teal, and a purple/violet. Off pixels are black.

## Notes
Trivial-to-moderate pattern; the only subtleties are the pre-rotation of the coordinate frame so the seam hides at the back, and the triangle-wave-plus-threshold trick to get symmetric, adjustable stripe widths.
