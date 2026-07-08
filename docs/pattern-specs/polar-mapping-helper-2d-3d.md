# Polar mapping helper 2D / 3D
kind: 1D+2D+3D
sensors: no

(Note: this pattern was catalogued as sensor-reactive, but it uses no sound or sensor inputs at all — it is a pure time-driven diagnostic utility.)

## Purpose and what it looks like

A diagnostic/calibration utility, not a decorative effect. It assumes the pixel map has been built in **polar (2D) or spherical (3D) coordinates**, normalized so each coordinate spans the unit range: first coordinate = radius from origin, second = rotation angle about the vertical axis (half of the range = a half turn), third = polar angle from the top axis (mid-range = the equatorial plane). It cycles through four test animations, each shown for several seconds (about six per mode, roughly half a minute for the full cycle), so the user can visually verify their map. Overlaid on every mode:

- A single white pixel chases through the strip in index order, completing one pass per mode period.
- Pixel index zero blinks a repeating identification sequence: solid white for a while, then brief separated flashes of red, green, and blue, then dark — useful for spotting RGB channel-order misconfiguration. The whole sequence repeats every couple of seconds.
- The last pixel pulses red with a triangle-wave brightness several times over that same couple-second period.

## The four modes (in cycle order)

1. **Radar sweep:** everything dark except a red beam at a single azimuth angle that sweeps continuously around the vertical axis, one revolution per mode period. Brightness falls off with angular distance from the beam (wrap-aware), with a squared (gamma-like) falloff, reaching zero beyond a modest angular half-width.
2. **Axes and expanding shell:** black background. Three colored cones mark the cartesian axes: red along the +x direction (azimuth near zero, restricted to near the equatorial plane), green along +y (azimuth a quarter-turn away, also equatorial), blue along +z (polar angle near the top; no equatorial restriction). Note the azimuth comparisons are done at doubled angular rate so the normalized azimuth reads correctly. Simultaneously, a spherical shell sweeps outward: brightness peaks where the pixel's radius matches a value that ramps from center to edge once per mode period, and the shell is tinted by the same three axis-proximity measures but with a much wider (several times) angular tolerance, so the shell shows reddish/greenish/bluish sectors near the respective axes. Shell drawing only replaces the background when its intensity is non-trivial.
3. **Mapper-style rainbow by index:** hue = pixel's index fraction plus a moving time offset, so a rainbow marches along the strip in index order. Brightness is a dim base level everywhere, boosted to bright (then gamma-squared) for the band of indices near the moving time position — a bright "you are here" window sweeping through index space once per mode period.
4. **Octants:** space divided into octants by three boolean tests, each contributing one primary at a fixed modest brightness: red where azimuth is in the back half-turn, green where azimuth is in the middle half of its range, blue where the polar angle is below the equator. Combinations yield the eight distinct octant colors (black through white). Brightness is deliberately capped low because one octant is fully white (current/heat caution).

## Algorithm notes

- The mode functions are stored in an array of function references indexed by a mode number computed from a sawtooth clock; the 3D renderer dispatches through it, then draws the chase and the two endpoint indicators on top.
- **Renderer fallbacks:** the 2D renderer calls the 3D one with the polar angle pinned to the equator; the 1D renderer calls the 2D one with radius = index fraction and angle zero. So the utility runs on any layout.
- Two proximity helpers do most of the work: a linear "nearness" of two scalars clamped to zero beyond a default half-width (about an eighth of the unit range, overridable), squared for gamma; and a wrap-aware angular version using a triangle-wave of the difference so nearness works across the wraparound. The default width is a compile-time knob the user is invited to enlarge for low-pixel-count builds.
- A source comment offers a switch between the single-pixel chase and a faded-tail variant (the shipped default is single pixel), and a commented line to freeze the cycle on one mode.
- The chase highlights only the index that matches the current clock position each frame, so on large installations at moderate frame rates many pixels are skipped — a known, documented quirk.
- All clocks derive from the engine's sawtooth time utility; the blink clock and the mode-progress clock are independent.

## Colors

Diagnostic primaries throughout: pure red, green, blue, white, plus a full rainbow in mode 3. No palettes.

## Controls

None exposed. Tuning (seconds per mode, proximity width, chase tail style, mode freeze) is by editing constants in source.

## Timing

Each mode lasts several seconds; identification blink sequence repeats about every two seconds; red end-pixel pulse cycles several times within that.
