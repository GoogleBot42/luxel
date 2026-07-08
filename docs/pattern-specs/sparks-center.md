# sparks center
kind: 1D
sensors: no

## What it looks like
Small points of light continuously shoot out of the middle of the strip in both directions, like sparks off a grinding wheel viewed edge-on. Each spark starts fast and bright at the center, decelerates under friction as it travels outward, and dims as it slows (brightness tracks speed). Fast/bright sparks read as white-hot; slower ones settle into a deep blue-indigo before dying out. With a couple dozen sparks alive at once, the center region sparkles constantly while the outer ends see only occasional survivors. A single spark's flight lasts on the order of a second or two, direction chosen at random per launch.

## State kept between frames
- A couple dozen sparks (around twenty), each with a signed velocity (sign = direction) and a position in pixel-index units.
- A per-pixel brightness buffer, cleared every frame (no trails/persistence).

## Per-frame work
Frame time is scaled down by a global factor. Then, per spark:
1. **Respawn.** If the spark's speed magnitude has decayed to (essentially) zero, relaunch it: position at the center of the strip, speed drawn uniformly from a moderate range (roughly a third to two-thirds of the launch-speed scale), and the sign flipped with a coin toss to pick a direction.
2. **Friction.** A constant deceleration opposing the direction of motion is applied, scaled by frame time. The friction constant is inversely proportional to strip length (halved again), so sparks decelerate more gently on longer strips and travel a proportionate distance.
3. **Motion.** Position advances by velocity times frame time.
4. **Bounds.** If the spark passes either end of the strip, its position and velocity are zeroed — the zero velocity triggers a respawn on the next frame.
5. **Deposit.** The pixel at the spark's (integer-truncated) position gets the spark's speed magnitude *added* to it. Overlapping sparks stack. (A just-killed spark deposits zero at index zero, which is invisible.)

Note the deliberate physics: brightness is the spark's *speed*, so sparks fade as they decelerate, and a spark that runs out of momentum mid-strip fades to black there and immediately relaunches from the center.

## Per-pixel rendering
The buffer value is squared (gamma-like emphasis so faint sparks stay subtle), then drawn at a fixed blue-indigo hue with saturation that decreases as value rises — so bright, fast sparks bleach toward white while slow ones are saturated blue. Brightness is the squared value itself.

Qualitative palette: black → deep indigo-blue → icy blue → white at the fastest/hottest.

## Layout assumptions
Pure 1D indexed pattern; no map needed. Positions are in raw pixel units but the friction is derived from pixel count, so the visual proportions (how far sparks get before dying) are automatically length-independent — a nice touch. Launch point is hardcoded to the middle index; that is the pattern's identity, not a bug.

## UI controls
None. Spark count and the global speed scale are code constants and would make natural sliders.

## Timing feel
Continuous, arrhythmic sparkle; individual spark lifetimes on the order of a second or two, no global cycle.
