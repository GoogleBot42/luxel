# Raindrops 2D
kind: 2D
sensors: no

## What it looks like
Rain falling on a still pool seen from above. At random moments and random places, a bright point appears and spreads outward as an expanding circular ripple; ripples cross, interfere, reflect subtly, and die away over a second or two. Beneath the ripples is a static mottled "sea floor" in watery blue-green tones; wave crests brighten it and wash it slightly toward white, troughs darken it. Overall feel: calm, organic, continuous light rain.

## State kept between frames
- Two 2D height-field buffers the size of the pixel grid, used as ping-pong surfaces (previous and current wave state), swapped each simulation step via pointer swap (never copied).
- One static 2D background image of hues, generated once at startup.
- A countdown to the next raindrop (randomized), plus two elapsed-time accumulators: one for the drop scheduler and one for the fixed-rate simulation step.

## Initialization
- Allocate the three grids.
- Generate the background: for each cell, average several triangle/sine-type waves of different arguments — position sums, a random-slope directional wave, radial distance from a corner, and radial distance from the center — then map that mildly around a blue-green base hue (a narrow hue band centered on aqua/blue, deviation only a small fraction of the wheel). One random value chosen at startup makes each boot's floor texture unique.

## Per-frame work
- Accumulate elapsed time into both timers.
- When the drop timer passes its randomized threshold: set a single interior cell (random position, avoiding the one-cell border) to full height in the *previous* buffer, then pick a fresh random threshold whose upper bound comes from the rate slider, and reset that timer.
- When the simulation timer passes a fixed step of roughly a thirtieth of a second (chosen because it "looks right" and decouples wave speed from frame rate): run one ripple step and reset it.

## Ripple step (the classic two-buffer water blur)
Swap the two buffers. For every interior cell (a one-cell border is left untouched so no clipping/wrapping logic is needed): new value = (average of the four orthogonal neighbors in the *other* buffer) minus the cell's own current value, then multiplied by a damping factor a bit under one. There is no real physics — this neighbor-average-minus-self recurrence naturally propagates rings outward and the damping fades them over roughly a second or two.

## Per-pixel render
Map the normalized 2D coordinates to integer grid cells. Brightness = a modest constant floor plus the wave height at that cell, then squared (gamma). Color = the background hue at that cell, with saturation reduced as brightness rises past full (saturation is set to a bit-more-than-one minus brightness, so tall crests whiten). Note the wave height can be negative in troughs, darkening below the floor.

## Colors
Watery aquas and blues (a narrow band of blue-green hues) over dark; ripple crests brighten and desaturate toward white foam.

## Controls
- One slider, "raindrops" (rate): sets the upper bound of the random inter-drop interval, from under a couple hundred milliseconds at maximum rate to on the order of a second and a half at minimum. Drops still arrive at random times up to that bound.

## Layout notes
Grid dimensions are hardcoded (a 16×16 matrix by default) as two source constants the user is told to edit; the coordinate-to-cell mapping quantizes whatever the mapper provides into that grid. Obvious fix: derive width/height from the pixel map or expose them as controls. The author warns memory grows with area (roughly three grids of floats), limiting it to several hundred pixels on small controllers. Also note the two buffers index as [x][y] while allocation nests the other way — harmless on a square grid, but a faithful non-square port should keep row/column indexing consistent.

## Non-obvious bits
- The whole water effect is the well-known two-buffer cellular blur (neighbor average minus self, damped) — cheap, stable, and convincing with zero trig at runtime.
- Fixed-timestep simulation with pointer-swapped buffers keeps ripple speed constant regardless of render frame rate.
- Leaving a one-pixel dead border eliminates all boundary conditions.
