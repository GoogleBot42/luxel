# MagVortex

Idea spec written from Patternflow preset preset_0719.h (CC BY-SA 4.0 per its SPDX header, by Seunghun Lee; the brief said MIT); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A swirling particle vortex, like charged particles spiralling in a magnetic field or matter falling into a black hole. A few dozen glowing particles orbit the panel centre, each drawing a continuous trail that fades over time, so the panel fills with curved streaks arranged in a spiral. Depending on the inflow knob the particles spiral inward and vanish at the centre (respawning at the outer edge), spiral outward and fly off past the edges (respawning near the centre — an explosion-like bloom), or simply circle at fixed radii (concentric rings of light). Orbits are larger than the short axis of the panel, so on the 128x64 panel many streaks sweep off the top and bottom edges and back, framing the core. Particles have different speeds and brightnesses, so the trails layer. The tone mapping is contrasty: faint trails are dim violet, overlapping or fresh trails go magenta and hot pink-red, and the densest spots (the core in inflow mode, or crossing points) jump to pink-white and white. Long trail decay gives full luminous spiral galaxies; short decay gives crisp comet streaks.

## The idea

Polar-coordinate particles with an angular speed and a radial drift, drawn as interpolated line trails into a decaying accumulation buffer, then tone-mapped through a contrast curve and a violet-to-white ramp. Accumulation plus decay is what turns a few dozen dots into a dense, glowing vortex.

## Algorithm in words

- State: a panel-sized accumulation buffer (0..1); a few dozen particles, each with radius, angle, a personal speed factor and a brightness.
- Setup: scatter particles at random radii (from near the centre out past the panel edges) and angles; random speed factors and brightnesses.
- Per update:
  - Multiply the buffer by the trail-decay factor.
  - Per particle: remember its current screen position; advance its angle by spin × its speed factor × the velocity step, and move its radius against the inflow control (positive = inward, negative = outward) × the velocity step.
  - Respawn rules: inward flow — if it reaches the centre, respawn at a random large radius and angle; outward flow — if it goes far past the edge, respawn at a tiny random radius; neutral — if it gets too close or too far, respawn at a mid radius.
  - Draw a line from the old to the new screen position in roughly one-pixel steps, adding a fraction of the particle's brightness to each buffer cell (saturating at full).
- Per draw: map each buffer value through a piecewise-linear contrast curve (lifts faint trails a little, steepens the middle and top), with empty cells at a near-black floor, then through the colour ramp.

## Knobs

- K1 TrailDecay — trail persistence, from short streaks to long luminous arcs.
- K2 Velocity — overall step size; negative values run the whole vortex backward.
- K3 SpinPull — angular speed (how much the particles circle versus fly straight in/out).
- K4 InflowPitch — signed radial flow: inward spiral, pure orbits near the middle of its range, outward bloom at the other end.
- Buttons reset each knob to its default. Audio: none directly; Velocity or Spin driven by sound would make the vortex surge with the music.

## Making it better in Luxel

- `renderBulk` is a perfect fit: use the persistent frame with `fade` for decay and `drawLine` in additive blend mode for each particle's step — no accumulation array at all, and a few dozen line draws per frame is very cheap even on a 128x128 chain. Use `setOutputPalette` for the contrast ramp (the frame is drawn in greyscale brightness, recoloured at the end).
- Add physics flavour: a gentle differential rotation (inner particles orbit faster, Keplerian), a little `curl2` turbulence on top of the spiral, and per-particle hue so a galaxy has blue outer arms and a white-gold core. Respawn with an eased fade-in so particles do not pop.
- Let the vortex centre wander or let two vortices orbit each other (binary star) as a mode.
- `beatSin` on the inflow gives a breathing in/out pulse without audio. Strips: a 1D version (particles bouncing toward/away from the strip centre with fading trails) is workable but loses the spiral.

## Payoff

4/5 — a striking, very cheap-to-do-well particle piece; ideal for renderBulk.
