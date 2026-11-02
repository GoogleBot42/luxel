# FireFlies
kind: 1D
sensors: no

## What it looks like
Warm amber points of light drift slowly along the strip in both directions, like fireflies. Each one glows brightest when it is moving fastest, leaves a short fading trail behind it, gradually slows to a stop and dims out, then a new one is born at a random spot with a fresh random speed and direction. Several fireflies (about one per ten pixels) are alive at once. Positions wrap around: a firefly drifting off one end reappears at the other. The feel is gentle and continuous — individual lives last a handful of seconds.

## Algorithm
This is a fork of a classic "sparks" particle pattern, slowed way down, with longer particle lifetimes and wrap-around ends.

State kept between frames:
- A brightness accumulation buffer, one slot per pixel (the trail canvas).
- Per particle (count = one plus about a tenth of the pixel count): a signed velocity and a position in pixel units.

Per frame:
1. The frame's elapsed time is scaled down by an order of magnitude — this is the "slowed down" part of the fork.
2. Every slot of the pixel buffer is multiplied by a constant a little below one (about a tenth lost per frame), producing the fading trails. Note: this decay is per-frame, not time-scaled, so trail length depends on frame rate; the obvious fix is to make the multiplier a function of elapsed time.
3. For each particle: if its speed has decayed into a small dead zone around zero, respawn it — new position uniformly random along the strip, new velocity uniformly random in a symmetric range (so direction is random) up to a modest maximum. Then multiply the velocity by a slow decay factor (also per-frame, same fix applies), advance the position by velocity times the scaled elapsed time, wrap the position to the opposite end if it passes either end, and add the particle's *signed velocity* into the pixel slot under its current position.

Per pixel: read the buffer slot, and set brightness to the square of the stored value boosted by roughly an order of magnitude; hue is a fixed warm amber/orange, fully saturated.

Randomness: only at particle respawn (position and velocity). Layout: scales to any pixel count automatically (particle count derives from it); 1D only.

## Colors
Single fixed color: warm amber/orange at full saturation. Only brightness varies — black background, dim trails, bright fast-moving heads. No hue animation.

## Controls
None exposed. Natural candidates to add: hue picker, speed, particle count, trail decay.

## Timing
A firefly's life — birth, drift, slowdown, fade — takes a few seconds. Trails persist for a fraction of a second to a second behind the head. Motion is slow: crossing a meaningful fraction of the strip takes seconds.

## Non-obvious details
- Brightness comes from the particle's velocity, not a stored "energy": fast = bright, and since displayed brightness is the *square* of the accumulated value, particles moving in the negative direction (which deposit negative values into the buffer) still glow — squaring makes both directions visible. A reimplementer who deposits absolute speed instead will get a nearly identical look, but must not clamp negatives to zero before squaring, or half the fireflies vanish.
- The respawn trigger is "speed decayed into a small window around zero," which ties lifetime to the velocity decay rate rather than a timer; that gives natural variation since faster-born fireflies live longer.
- Both decays (trail and velocity) being per-frame rather than delta-based means the pattern runs differently at different frame rates; worth normalizing in a reimplementation while keeping the default feel at a typical high frame rate.
