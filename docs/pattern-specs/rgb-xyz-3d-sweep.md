# RGB-XYZ 3D Sweep
kind: 3D
sensors: no

A simple 3D-map diagnostic pattern. (It was catalogued as sensor-reactive, but it uses no sensor inputs at all.)

## What it does
A glowing planar band sweeps through the mapped volume along each axis in turn, always in the positive direction: a red band travels along X, then a green band along Y, then a blue band along Z, and the cycle repeats. Each axis sweep takes about a second; the full three-axis cycle takes about three seconds. The purpose is visual verification that a 3D pixel map's axes point the expected directions (with the caveat, noted by the author, that the strip's color-order setting must already be correct or the colored sweeps mislead).

## Algorithm
Per frame: a sawtooth phase over the ~three-second cycle is split into thirds; the current third selects the active axis, and the position within the third gives sweep progress from start to finish. The progress is then stretched so the band's travel starts fully *outside* the low end of the volume and finishes fully outside the high end — the band slides completely on and completely off rather than popping in at the edges. This is done by widening the travel range by one band-width on each side.

Per pixel (3D renderer with x, y, z in the unit cube): the coordinate along the active axis is compared to the band's current center. If the pixel lies within one band-width of the center (the band spans roughly a fifth of the volume on each side of center, i.e. total width somewhat under half the axis), it lights in the active axis's color; otherwise it is black.

Two band styles exist as a code constant (not a UI control):
- **Simple:** flat full brightness across the band with crisp edges — best for spotting pixel-alignment problems.
- **Smoothed (the default):** brightness follows a sinusoidal bump across the band — zero at both edges, peak in the middle — computed from the pixel's fractional position within the band with a quarter-cycle phase offset so the peak lands at the band center. More visually pleasing.

No state between frames beyond the time phase; no randomness; no per-pixel buffers.

## Colors
Fully saturated primaries keyed to axis: red for X, green for Y, blue for Z. (The "green" hue constant is actually slightly off the exact green third of the hue wheel, and "blue" slightly off the blue two-thirds point — visually they read as green and blue; an implementer can use true primaries.) Off-band pixels are black.

## Layout assumptions
Requires a 3D pixel map with normalized coordinates spanning the unit cube. Works with any pixel count. No hardcoding of counts.

## UI controls
None. Obvious improvements if porting: expose the band style as a toggle and the band width / sweep speed as sliders.

## Timing
Roughly one second per axis, three seconds per full cycle, continuous loop.
