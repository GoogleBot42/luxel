# sparkfire
kind: 1D
sensors: no

## What it looks like
A fire effect that burns along the strip from the start (index 0) toward the far end. Bright ember "sparks" launch from the bottom, accelerate as they travel, and leave trails of heat behind them. The heat diffuses upward along the strip and cools, so the overall look is licking flames: mostly deep red/orange near the base of activity, flickering toward yellow and near-white where heat piles up, fading to black where it cools. A spark takes on the order of a few seconds to cross the strip (this depends on strip length), and there are a handful of sparks alive at once, so the motion feels continuous and organic rather than periodic.

## State kept between frames
- A per-pixel "heat" buffer, one value per pixel.
- A handful (roughly five) of sparks, each with a scalar speed and a position expressed in pixel-index units.

At startup each spark gets a random modest speed and a random position anywhere along the strip; after that, sparks always restart from position zero.

## Per-frame work (before rendering)
All motion is scaled by elapsed frame time, further multiplied by a small global speed factor so the numbers stay tame.

1. **Cooling.** Every pixel's heat is reduced two ways at once: a small subtractive amount proportional to elapsed time, and a slight multiplicative decay (retaining almost all of the value each frame). If the subtractive amount alone would exceed the pixel's current heat, the pixel snaps to exactly zero instead.

2. **Upward diffusion / convection.** Walking from the far end of the strip backward toward the start (skipping the first few pixels), each pixel is replaced by a weighted average of the several pixels just below it (the four nearer-to-start neighbors), with the weights biased so the farthest-below neighbors count most (weights roughly 1:1:2:3 over a total of about 7). Because the loop runs top-down, each pixel reads its lower neighbors' still-unmodified values from this frame. The net effect is that heat continuously smears upward along the strip, like rising flame.

3. **Spark update.** For each spark:
   - If its speed has reached zero or below, it respawns: random speed (up to a moderate cap) and position reset to the start of the strip.
   - Its speed increases by a constant acceleration times elapsed time (linear speed growth).
   - Its position advances by the *square* of its speed times elapsed time — so travel starts slow and rapidly runs away as the spark ages.
   - If the new position passes the end of the strip, the spark's position and speed are zeroed (it will respawn next frame) and no heat is deposited.
   - Otherwise, the spark deposits heat into every pixel index it crossed this frame (from its old integer position up to its new one). The amount deposited per pixel shrinks as the spark's speed grows (clamped so it never goes negative) — fast, old sparks are dimmer, which makes the base of the fire hottest and the fast travelers read as fading embers. The deposit is additive on top of existing heat, at roughly half strength.

## Per-pixel rendering
Each pixel's heat value drives a classic fire palette, built directly from heat rather than a lookup table:
- **Hue** sits in the narrow red-to-yellow range, moving from red toward yellow proportionally to the *square* of heat (so only genuinely hot pixels shift off red).
- **Brightness** is proportional to heat, roughly doubled, so mid-range heat already reads fully bright.
- **Saturation** is full until heat exceeds unity, after which it falls off steeply — overheated pixels bleach toward white.

Qualitative palette: black → deep red → orange → golden yellow → near-white at the very hottest spots.

## Layout assumptions
Purely 1D, indexed renderer. Spark positions, speeds, and the friction of travel are all in raw pixel-index units, so the *time* a spark takes to cross the strip grows with pixel count and the effect's pacing changes with strip length. Obvious fix if porting: normalize spark position/speed to a 0..1 strip fraction (or scale acceleration by pixel count) so the feel is length-independent.

## UI controls
None. The spark count, acceleration, overall speed, and the two cooling rates are code constants; they'd make natural sliders.

## Non-obvious bits
- Advancing position by speed *squared* is what gives sparks their convincing "launch" feel — an accelerating rocket rather than constant drift.
- Depositing heat along the entire span crossed in a frame (not just the endpoint) keeps trails gap-free even when a fast spark skips many pixels per frame.
- Cooling combines subtractive and multiplicative decay, which gives a sharper extinguish at low heat than either alone.
