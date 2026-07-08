# Rocket by Tony Hampton
kind: 1D
sensors: no

(Despite being catalogued as sensor-reactive, this pattern reads no sound or sensor inputs; it is a purely generative physics simulation.)

## What it looks like
A rocket launch on a linear strip, oriented with pixel index 0 as the ground and the far end as the sky. A short bright segment (the rocket body, default white) starts at the bottom, accelerates upward — slowly at first, then with a sudden "boost" kick after a delay — and streaks off the top, after which the launch resets and repeats. Behind/below the rocket, a shower of exhaust sparks spews downward from the nozzle: sparks are white-hot when fresh, fade toward the chosen exhaust color (default orange-red), and as they die they flicker with random cool-hued (blue/purple-ish or random) pops and sparkles. Everything leaves glowing trails because pixels cool gradually rather than clearing each frame. A full flight takes a user-set time, several seconds by default.

## Algorithm
This is a spark/particle system derived from the classic "Sparks" pattern, with an added rocket kinematics model. All the work happens in the per-frame step; the per-pixel renderer just outputs a persistent buffer.

State kept between frames:
- Three full-strip accumulation buffers, one per color channel (red, green, blue), acting as additive "light energy" per pixel.
- A pool of sparks (pool size is about one-sixth of the pixel count), each with: position (fractional pixel index), energy, and an individual hue (only used in multicolor mode).
- Rocket position, rocket velocity, and elapsed time since launch.

Per frame:
1. Cooling: every pixel's stored RGB energy is multiplied by a decay factor slightly below one, computed from the frame's elapsed time so the trail-fade rate is roughly frame-rate independent (clamped so it never fails to decay).
2. Rocket kinematics: a base acceleration is derived from the strip length and the user's flight-time setting (chosen so an unboosted rocket would traverse the strip in about that time under constant acceleration). Once elapsed time passes the boost-delay setting, acceleration is multiplied by the boost factor. Velocity integrates acceleration; position integrates velocity (proper time steps in seconds). When the rocket passes the top of the strip, position, velocity, and elapsed time all reset to zero — the launch loops forever.
3. Active spark count: only a fraction of the spark pool is simulated, proportional to the rocket-size setting relative to its maximum — bigger rockets emit denser exhaust.
4. Spark update, for each active spark:
   - If its energy has hit zero (dead): first, a "pop and sparkle" burst — a few (about three) neighboring pool slots are re-seeded as mini-sparks with small random energies, positions within a pixel or two of the dying spark, and fully random hues. Then the dead spark itself respawns at the rocket's current position with high energy (full scale plus a random bonus of roughly a third more), and, in multicolor mode, a fresh random hue.
   - Energy decays by a friction rate inversely proportional to strip length (so behavior scales with strip size), floored at zero.
   - Position moves *downward* (toward index 0) at a rate proportional to energy squared — fast, hot sparks race away from the nozzle; dying ones crawl.
   - Sparks that leave either end of the strip are parked at the rocket position with zero energy (they will respawn next frame).
   - The spark deposits light additively into the RGB buffers at its integer pixel: brightness is energy squared (gamma-ish), hue is the exhaust-picker hue (or the spark's own random hue in multicolor mode), except when brightness is below about half, where the hue is shifted by a small random amount toward the cooler side — this is the end-of-life color flicker. Saturation is the exhaust picker's saturation scaled up as brightness falls, so hot fresh sparks render near-white and cooling ones show full color. The HSV triple is converted to RGB via a hand-rolled standard sector-based HSV→RGB conversion and summed into the buffers.
5. Rocket body: the configured number of consecutive pixels starting at the rocket's position get the rocket picker's color added into the buffers.

Per pixel (render): read the three channel buffers at that index, clamp each channel to full scale, output as RGB.

Randomness: spark respawn energy, burst mini-spark energy/offset, all multicolor hues, and the end-of-life hue flicker are all uniform random draws.

Layout: scales with pixel count throughout (spark pool size, friction, acceleration all derive from it); assumes a 1D strip with a meaningful "up" direction. No fixes needed.

Quirk worth knowing: because dead sparks always sit at exactly zero energy, the "just fizzled" burst condition effectively fires every frame for every dead spark right before it respawns — in practice the burst-and-respawn is one combined event, which keeps the exhaust continuously churning with random-colored sparkle noise near where sparks die.

## Colors
- Rocket body: user-picked, default white.
- Exhaust: user-picked base (default a hot orange-red). Fresh sparks render nearly white regardless of the picked hue (saturation is suppressed at high energy), cooling through the picked color, then flickering into shifted cooler/random hues as they fizzle. With the multicolor toggle on, every spark takes a random hue instead of the picked one.
- All light mixes additively in RGB, so overlapping sparks whiten and bloom.

## Controls
- Slider, flight time: from about a second to about twenty seconds for an unboosted full-strip flight.
- Slider, rocket size: body length from one pixel up to about twenty; also proportionally scales the number of active exhaust sparks.
- Slider, boost delay: from about half a second to several seconds before the boost kicks in.
- Slider, boost multiplier: acceleration multiplier from none (1x) up to about two orders of magnitude.
- HSV picker, exhaust color.
- HSV picker, rocket body color.
- Toggle, multicolor exhaust: random per-spark hues instead of the picked exhaust color.

## Timing feel
Launches loop continuously. Early flight is a slow crawl, then the boost produces a satisfying sudden whoosh. Trails linger for a large fraction of a second after light passes.
