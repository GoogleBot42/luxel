# angle and radius from coordinates
kind: 2D+3D (native 3D renderer; the 2D renderer delegates to it at mid-height)
sensors: no

This is primarily a teaching pattern demonstrating how to derive polar coordinates (angle and radius) from mapped pixel positions. The visual is a single effect built on those helpers.

## What it looks like
A narrow, bright "radar beam" or spoke sweeps around the center of the mapped area, completing a revolution in a few seconds. Brightness falls off sharply away from the beam (high-contrast: the beam is thin with dark space between passes). Hue is tied to each pixel's distance from the 3D center of the map: pixels near the center sit at one end of the hue wheel and the hue walks around the wheel as radius grows, so the beam appears as concentric color rings swept by a rotating brightness window. On a true 3D map, the beam's rotation rate varies smoothly with height — roughly twice as slow at one extreme of the vertical axis as at the other — so the spoke shears into a slowly twisting helix rather than rotating as a rigid plane.

## Algorithm
No state is kept between frames beyond the ambient clock; everything is a pure function of pixel position and time.

Helper functions (the pedagogical payload):
- A four-quadrant arctangent built from the single-argument arctangent with quadrant case analysis. This existed only to work around a firmware bug in an old engine version — a reimplementation should just use a standard two-argument arctangent.
- "Unit angle": take the angle of the pixel about the map center (coordinates recentered by subtracting the mid-point), shift it so it is always positive, and normalize a full turn to the zero-to-one range. The convention happens to put zero at "north."
- 2D radius and 3D radius: Euclidean distance from the recentered mid-point (in two or three axes respectively).

Per pixel, each frame:
1. Compute the unit angle of the pixel about the center and add a time-based phase that increases steadily; the phase's cycle period grows linearly with the pixel's height coordinate (base period of a few seconds per revolution, roughly doubling from one end of the height axis to the other). Increasing phase makes the beam rotate; the comment in the original calls the direction clockwise.
2. Feed that sum through a triangle wave (peak in the middle of its cycle) to get a brightness ramp, then raise it to a high power (order of ten) to sharpen it into a thin spoke.
3. Hue = the pixel's 3D radius from the map center. Saturation is full.

The 2D renderer simply calls the 3D one with the height coordinate fixed at the middle, for compatibility with flat maps.

Incidental details, safe to omit: a short-period timer is computed each frame but never used; and for the very first pixel each frame the pattern exports that pixel's coordinates and radius as watchable debug variables — purely diagnostic, no visual effect.

Layout assumptions: expects normalized map coordinates spanning zero-to-one on each axis with the interesting center at the midpoint. No pixel-count hardcoding.

## Colors
Full-saturation rainbow indexed by distance from center: the center sits at the red end of the wheel and hue progresses around the wheel with radius (a corner of a unit cube is most of the way around the wheel). Brightness is the rotating spoke described above; unlit regions are fully dark.

## Timing
A few seconds per beam revolution at one height extreme, stretching to roughly double that at the other extreme; the twist between layers is what animates on 3D maps.

## UI controls
None.
