# perlin fire wind tunnel
kind: 2D
sensors: no

## What it looks like
A swirling vortex of wind-blown flame on a 2D panel. Noise-generated fire tongues stream continuously in one direction (as if blown past the panel), while the whole field is twisted into a spiral around the panel center — pixels near the center rotate hard, pixels farther out rotate gently, so the flames wrap into a tunnel/whirlpool. On top of that, a sinusoidal "wind wobble" sways the flame columns side to side. Hot cores desaturate toward white-hot; cooler fringes are deep and dark. The vortex itself revolves at a steady rate of a few seconds per revolution; the flame texture streams by faster or slower per a speed control; the underlying noise character slowly evolves over several minutes without ever visibly looping.

## Per-frame state and clocks
No persistent state between frames; everything derives from clocks:

- A sawtooth driving the tunnel rotation angle, several seconds per full revolution.
- Two more sawtooths of similar but different periods driving the wind wobble phase (one directly, one through a triangle-smoothed wave), so the sway itself meanders rather than ticking metronomically.
- A very slow coordinate that walks the noise field's third dimension across one full noise-lattice repeat over several minutes — the noise functions tile smoothly at their lattice period, so this gives minutes of unique evolution before seamlessly wrapping.
- A second, faster coordinate (rate set by the speed control) that scrolls the noise field along y, producing the streaming-flame motion. It also spans one lattice repeat so it wraps seamlessly.

Each frame also resets the coordinate transform: translate so the panel center is the origin, then scale both axes by the density (zoom) control.

## Per-pixel work
Given centered, density-scaled (x, y):

1. **Wind wobble**: add a horizontal offset to x equal to the wind control times the sine of (a function of the pixel's vertical position plus the two wobble clocks), attenuated by a small factor and weighted so the sway is stronger toward one vertical extreme (weight proportional to density minus y). Net effect: flame columns lean and sway, more at one end than the other.
2. **Tunnel twist**: compute an angle = (rotation clock scaled to a full circle) plus (density divided by the pixel's distance from center). Rotate the (x, y) point by that angle with the standard rotation formula. The inverse-distance term is the whole trick: nearer pixels get wildly more rotation, shearing any texture into a spiral tunnel. (The author notes you can tunnel-ify anything this way.)
3. **Flame intensity**: sample the currently selected noise flavor at (x, y compressed by half plus the fast scroll coordinate, the slow evolution coordinate), take magnitude, clamp to unit range. Four selectable flavors, all standard fractal-noise variants: plain perlin (doubled absolute value), ridged multifractal, fractional-Brownian-motion, and turbulence — each with typical lacunarity/gain and roughly three octaves.
4. **Color**: hue = base-hue control plus a small offset that grows with intensity (hotter areas shift slightly along the wheel, e.g. red toward orange at the default); saturation = a value somewhat above one minus the intensity, so only the hottest cores drop below full saturation and bleach toward white; brightness = intensity cubed, crushing the fringes toward black for a fiery contrast curve.

## Colors
With the hue control at zero: black through deep red, orange tongue mid-tones, to near-white cores. The hue slider shifts this entire ramp anywhere on the color wheel (green fire, blue fire, etc.); the intensity-linked hue offset always spreads the hot end a little further along the wheel than the base.

## UI controls (all sliders)
- **Hue**: base color of the fire ramp.
- **Mode**: selects among the four noise flavors (with a companion numeric readout showing the selected mode index). Different flavors give qualitatively different flame textures — plain billow, sharp ridges, soft fractal haze, roiling turbulence.
- **Density**: zoom/scale of the effect, from roughly a quarter scale to about triple; also feeds the twist strength and wobble weighting, so it changes the character, not just the size.
- **Wind**: amplitude of the horizontal sway, from none to full.
- **Speed**: how fast the flame texture streams; inverted mapping (slider up = shorter scroll period = faster), spanning roughly from leisurely (a minute-ish per texture cycle) down to fast (seconds).

## Layout assumptions
Pure 2D via mapped normalized coordinates; no pixel-count or wiring assumptions. Needs a 2D pixel map. (No 1D fallback is provided — one could be added, but the tunnel is inherently 2D.)

## Non-obvious bits
- The tunnel is not raymarched or polar-mapped: it's just per-pixel rotation by an angle inversely proportional to radius. Cheap and generic.
- Scrolling the noise sample coordinates across exactly one noise-lattice repeat via slow clocks gives long non-repeating animation that still loops seamlessly.
- Letting saturation exceed full range at low intensity and dip below only near peak intensity is what produces the tight white-hot cores.
