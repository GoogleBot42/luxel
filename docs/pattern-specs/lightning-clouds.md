# Lightning clouds
kind: 2D
sensors: no

## What it looks like
A dim, moody night sky of deep-blue clouds drifts slowly sideways across the matrix, the cloud shapes themselves slowly morphing. At random moments a lightning flash erupts at a random spot: for about half a second the clouds near that spot light up brightly from behind — dense cloud areas glow hardest, and the very core of the flash washes out toward white — then the glow decays back to the dim ambient sky. The rest of the time the display is quite dark, just faint blue cloud texture.

## Algorithm
State between frames:
- A lightning countdown timer (milliseconds remaining in the current flash; zero when idle).
- The current flash's 2D position.

Per frame:
- Decrease the countdown by the elapsed frame time, clamping at zero.
- If the countdown has reached zero, roll a random chance — a few percent per frame — to start a new flash: reset the countdown to the flash duration (about half a second) and pick a new flash position uniformly at random over a region noticeably larger than the visible area (so some strikes land partly or wholly off-screen, which reads as distant lightning).
- Advance two very slow clocks (periods on the order of a minute and of tens of seconds). One, scaled up by a large factor, is used as the noise function's third coordinate (time axis) so the cloud shapes evolve; the other, similarly scaled, is added to the horizontal coordinate so the cloud field translates steadily sideways.

Coordinate setup (once, at pattern start): recenter the unit square on the origin and zoom out several-fold, so the visible window shows a several-unit-wide slice of noise space and pixel coordinates run roughly from minus-two to plus-two.

Per pixel:
1. Cloud density: fractal "turbulence" noise (perlin-based turbulence with a couple of octaves, a lacunarity of about two, and a high per-octave gain near one) sampled at (x plus the horizontal drift, y, time coordinate). Result is treated as a zero-to-one-ish density.
2. Lightning intensity: if a flash is active, compute the pixel's Euclidean distance to the flash position; intensity is (a radius constant of about one and a half, minus that distance) times the fraction of flash time remaining — a broad radial glow that fades linearly over the flash's lifetime and can go negative far away (effectively zero). Inactive: zero.
3. Backlit-cloud glow: cloud density squared, times the lightning intensity, times a small boost factor (about two). Squaring the density is the key trick: thick cloud regions light up disproportionately, giving the "lit from within/behind" look.
4. Base visibility: cloud density times an ambient floor — normally a very small constant (a few percent), but while any flash is active the floor is raised to a modest fraction of the remaining-flash fraction, so the whole sky brightens slightly during a strike.
5. Final brightness: the larger of the base visibility and the backlit glow.
6. Color: a fixed deep sky-blue hue. Saturation sits around half normally and is reduced by the local glow amount (floored at fully desaturated), so flash cores whiten. Brightness as computed above.

## Colors
Single-hue palette: near-black through dim desaturated deep blue for ambient clouds, up through bright icy blue to white at flash cores.

## Layout / controls
Pure 2D world-coordinate rendering; no layout assumptions beyond a mapped 2D display. No UI controls. No sensors; randomness alone drives strike timing and placement.

## Timing feel
Flashes last about half a second and occur irregularly — typically a few per couple of seconds at fast frame rates (the per-frame trigger probability makes strike frequency frame-rate dependent; a reimplementation might prefer a per-second-normalized probability). Cloud drift and shape evolution are slow, over tens of seconds.
