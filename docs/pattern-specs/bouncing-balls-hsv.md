# bouncing balls - hsv
kind: 1D
sensors: no

## What it looks like
A handful of single-pixel "balls" (default around eight) bounce on a black background as if dropped under gravity, with the strip acting as a vertical column: each ball falls, hits the "ground" at pixel zero, rebounds a little lower each time, and when its bounces have nearly died out it is relaunched at full height. Each ball has its own fixed, fully saturated hue, evenly spaced around the color wheel, so the ensemble is a rainbow of independent bouncing dots. Because each ball loses slightly different energy per bounce, they start in sync (all dropped together) and progressively desynchronize into pleasing chaos. A full drop-to-first-bounce takes on the order of a second; the bounce trains compress into rapid flutters near the ground before relaunch.

## Algorithm
Classic one-dimensional projectile kinematics, evaluated in absolute time (frame-rate independent):

State kept between frames, per ball:
- Timestamp of the last ground strike.
- Current rebound (impact) velocity.
- Per-ball coefficient of restitution: starting near ninety percent and decreasing very slightly with ball number (the decrement scales inversely with the square of the ball count), so every ball damps at a marginally different rate — this is what desynchronizes them. No randomness anywhere; the pattern is fully deterministic.

Also kept: a running clock (sum of frame deltas) and full-strip scratch buffers of per-pixel hue and brightness (saturation is everywhere full).

Per frame (before render), for each ball:
- Compute time since its last ground strike, then height = half·gravity·time² + reboundVelocity·time, with gravity negative and the launch height normalized so a full-energy rebound just reaches the top of the strip.
- If height goes negative: clamp to zero, multiply the rebound velocity by that ball's restitution coefficient, and record a new ground-strike time. If the rebound velocity has decayed below a small threshold, reset it to the full initial drop velocity (ball relaunches to full height).
- Map height linearly to a pixel index over the usable strip (floor to integer) and stamp that pixel in the scratch buffers with the ball's hue at full brightness. Later balls overwrite earlier ones on collisions — no blending (acknowledged limitation of this variant).

Per pixel (render): read hue/saturation/brightness from the scratch buffers, then zero that pixel's brightness immediately after it has been read, so the buffer self-clears each frame with no separate clearing pass.

## Direction / symmetry modes
A hardcoded mode constant selects one of four layouts:
1. balls bounce from the strip's start;
2. mirrored, bouncing from the strip's end;
3. symmetric from both ends toward the middle (physics runs on half the strip, second half mirrors the first);
4. symmetric from the middle toward both ends (the same half-buffer, mirrored the other way).
In the symmetric modes the usable pixel span is half the total count; in the one-sided modes it is the full count.

## Colors
Each ball's hue is its index as a fraction of the ball count — i.e. hues evenly distributed around the full wheel, all at full saturation and brightness, on black.

## Controls
None exported; ball count and direction mode are edit-the-source constants. Obvious improvement: a slider for ball count and a selector/slider for the four direction modes.

## Layout assumptions
Fully proportional to pixel count; nothing hardcoded to a specific strip length.

## Notes / gotchas
- The "clear on read" trick in render assumes every buffer cell is read exactly once per frame per clearing pass; in the symmetric modes the first (non-clearing) half must render before the mirrored (clearing) half.
- The relaunch-on-tiny-velocity rule is what makes the pattern perpetual; without it every ball would flatline at the ground.
- Ball positions floor to integers, so near the apex a ball visibly dwells on one pixel — this reads naturally as the slow-at-the-top of a real trajectory.
