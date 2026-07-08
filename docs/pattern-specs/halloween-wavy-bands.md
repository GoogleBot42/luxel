# Halloween Wavy Bands
kind: 1D+2D (2D is the primary renderer; 1D delegates to it along a horizontal slice)
sensors: no

## What it looks like
A set of vertical color bands (about ten across the display) in a Halloween palette — mostly warm oranges and red-oranges interleaved with a few violet/purple bands. The bands are not straight: their edges wobble organically, columns swell and pinch in width, and a sinusoidal wave ripples them horizontally. Between bands the color falls off to black, giving dark, softly antialiased seams. The whole field drifts slowly; the organic wobble evolves noticeably faster than the sideways drift. Motion is smooth and continuous, hypnotic rather than flashy. On a 1D strip it looks like the same wavy bands sampled along one line: colored segments sliding and breathing.

## Algorithm
State between frames: a single running clock, accumulated from per-frame elapsed time in seconds and wrapped after roughly an hour to avoid precision loss. Each frame derives two phase values from that clock: one drifting slowly in the negative direction (used for horizontal wave motion) and one advancing about twice as fast in the positive direction (used to animate the noise field).

Per pixel (2D):
1. Perturb the vertical coordinate by subtracting a modest fraction (roughly a third) of a 3D-ish perlin noise sample. The noise is sampled at about double the spatial frequency of the display in both axes, with the faster clock phase as the third (time) axis. This makes individual columns vary in apparent width and wobble organically.
2. Perturb the horizontal coordinate by adding a small sinusoidal offset (amplitude around a sixth of one band width) whose phase depends on the (already perturbed) vertical coordinate plus the slow clock phase. This creates the traveling horizontal wave in the band edges.
3. Quantize the perturbed horizontal coordinate into N equal bands (N is about ten, a hardcoded count — obvious improvement: expose it as a slider). The band number, wrapped into range, indexes a fixed hue table.
4. Brightness: compute the pixel's fractional position within its band, fold it into a triangle so it is full brightness at band center and zero at both band edges, then apply a slight gamma (exponent a bit above one) to deepen the dark seams. This darken-toward-edges approach is a deliberate substitute for hard black edge lines: it antialiases much better at low LED resolution.

The 1D renderer simply calls the 2D one with the pixel's normalized strip position as the horizontal coordinate and a fixed vertical coordinate about a quarter of the way down.

## Colors
A hardcoded table of about ten hues, used at high (but not full) saturation: predominantly red-orange/orange/amber entries with two or three violet-to-purple entries mixed in — classic Halloween orange-and-purple. Band seams fade to black. No UI to change the palette (another obvious enhancement point).

## Controls
None.

## Timing
Sideways band drift: very slow, on the order of many seconds to cross one band. Noise wobble: clearly livelier, a few seconds per visible change. Wave ripple: gentle, continuous.

## Non-obvious bits
- The vertical noise distortion is applied before the horizontal sine distortion, so the wave phase itself varies with the wobbled y — the two distortions compound rather than layering independently.
- The triangle-with-gamma edge darkening (rather than drawing black borders) is the key trick for a clean look on coarse LED grids.
