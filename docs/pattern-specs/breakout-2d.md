# Breakout 2D
kind: 2D
sensors: no

## Visual behavior
A self-playing game of Breakout on a matrix. The top third of the display is a wall of rainbow-colored bricks (hue varies by row and slowly shimmers). A white ball bounces around the middle, knocking out bricks one at a time; a white paddle bar along the bottom row slides to intercept it. In the default "demo" mode the paddle plays itself, tracking the ball with a little human-like error. When all bricks are cleared, the wall and ball reset and the game starts over. Ball motion is stepped but smoothed by anti-aliasing; a full game takes on the order of a minute or two.

## Layout assumptions
Assumes a **square matrix**: both width and height are taken as the square root of the pixel count. Obvious fix: use the real mapped width/height (or make them configurable) instead of assuming square.

## State kept between frames
- A 2D grid of brick flags (rows = about one third of the matrix height, columns = matrix width), each brick present/absent.
- Ball position and velocity, both in unit-square coordinates. Speed is a fixed constant such that the ball crosses the display in roughly a dozen game ticks.
- Paddle center (unit coordinate), paddle half-width, previous paddle center (to measure paddle speed), demo-mode flag.
- An accumulator that fires a **game tick at a fixed interval of roughly a tenth of a second**, decoupling game speed from render frame rate.
- A slowly cycling shimmer phase (triangle wave with a period of several seconds) used to animate brick colors.

## Per-tick game logic (fixed timestep)
1. Record how far the paddle moved since last tick (used as "spin").
2. If demo mode is on, set the paddle center to the ball's horizontal position minus a small random offset (up to about one pixel-width) — deliberately imperfect tracking.
3. Advance the ball by its velocity.
4. Collision tests, in priority order:
   - **Fell off the bottom** (vertical coordinate past the bottom edge): reset the ball — place it just below the brick wall at a random horizontal position, heading downward in a random direction within a modest cone around straight down.
   - **Ceiling**: undo the move, negate vertical velocity.
   - **Side walls**: undo the move, negate horizontal velocity.
   - **Paddle row** (ball's row equals the bottom row): if the ball is within the paddle's half-width of the paddle center, undo the move, negate vertical velocity, and subtract a small fraction (about a tenth) of the paddle's recent movement from the horizontal velocity — backspin/english.
   - **Brick region** (ball's row inside the brick rows): if the brick at the ball's cell is present, remove it; undo the move and negate vertical velocity. If that was the last brick, reset the whole wall and the ball.

## Per-pixel render
Compute the pixel's row/column and its distance to the ball in pixel units.
- **Bottom row**: if within the paddle half-width of the paddle center, white with brightness falling off linearly with distance from paddle center; else black.
- **Brick rows**: if the brick at this cell is present, full-brightness saturated color with hue proportional to the row (rainbow banding) plus about a third of the shimmer phase, so all bricks drift through nearby hues together over several seconds. If the brick is gone, fall through to ball drawing.
- **Elsewhere (and in cleared brick cells)**: draw the ball as an anti-aliased white dot — if the pixel is within roughly one pixel of the ball, white with brightness falling off linearly with distance; else black.

## Controls
- **Paddle position** (slider): manually sets the paddle center (only meaningful with demo off).
- **Paddle width** (slider): sets the paddle half-width across a moderate range (narrowest is about a fifth of the display, widest a bit over half).
- **Demo mode** (slider used as a toggle: anything above zero = on): auto-play on/off.

## Non-obvious points
- Fixed-timestep game logic in the pre-frame hook keeps game speed independent of render rate.
- "Undo move then flip a velocity component" is the whole collision-response model — no swept collision; fine at these speeds.
- The paddle-spin term is what keeps the demo from settling into a repeating loop.
- Original quirk: row/column math occasionally mixes width and height (harmless because square is assumed); a clean reimplementation should use each dimension consistently.
