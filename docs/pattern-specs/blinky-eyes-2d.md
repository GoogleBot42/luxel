# Blinky Eyes 2D
kind: 2D
sensors: no

## What it looks like
A pair of cartoon eyes on a black background: two white elliptical outlines side by side, each containing a round deep-blue iris. The irises dart together to one side (left or right at random), hold, and glide back to center, every second or so. Independently, the eyes blink — the ellipses squash vertically closed and reopen, hiding the irises during the blink — every couple of seconds at randomized intervals. The overall effect is a creature watching you from the matrix. It is meant for moderately high-resolution rectangular matrices; on small grids the ellipse outlines fall apart.

## Algorithm
Configuration (source-level constant, not a UI control): the number of eyes, one or two. Also fixed geometry constants: maximum eye half-width and half-height (each a modest fraction of the display), iris radius (a bit over a tenth of the display width), and an outline-thickness threshold.

State between frames — two independent little state machines, each driven by accumulating real elapsed time in seconds:

**Gaze state machine.** Alternates between "idle" (irises centered) and "moving". Idle lasts a randomized interval of roughly half a second to about a second; then a direction (left/right) is chosen by coin flip and a "moving" phase begins lasting a fixed fraction of a second (roughly a third). During the move, the iris's horizontal offset from eye center follows a smooth sine-shaped hump of the phase — it glides out to one side and back to center in one continuous motion (maximum excursion is about a quarter of an eye-width). After the move, the next idle interval is re-randomized.

**Blink state machine.** Alternates between "open" and "blinking". Open lasts a randomized one-to-two seconds; a blink lasts about half a second. While open, the current ellipse half-height equals its maximum. While blinking, each frame the half-height is multiplied by a smooth wave envelope of the blink phase (starting from the wave's falling half so it closes first), then clamped between a small floor and the maximum — producing an accelerating squash toward a nearly-shut slit and reopening. After the blink, the next open interval is re-randomized.

Per-pixel work (2D renderer):
1. Re-center coordinates so the origin is the middle of the display; if two eyes, also scale x by two and fold each half toward its own eye center, so the same eye is drawn twice, mirrored placement. y is just centered.
2. Compute the pixel's distance to the current iris center (the gaze offset applied on x; when two eyes, the x component is scaled back down to keep the iris circular).
3. Compute an "ellipse metric": the norm of (x divided by the eye half-width, y divided by the *current* half-height). This is 1 exactly on the ellipse boundary, <1 inside.
4. If not mid-blink and the pixel lies within the iris radius: draw iris — deep blue at full saturation, with brightness growing quadratically from dark at the iris center to bright at its rim (a ring-like iris).
5. Otherwise draw the eye outline in white: pixels inside the ellipse get a brightness equal to the ellipse metric (minus a small thickness threshold, floored at that threshold), raised to roughly the sixth power — so only pixels very near the boundary glow, giving a thin soft white outline; pixels outside the ellipse are black.

Randomness: uniform random draws for gaze direction, idle-gap length, and open-eye interval length. Movement/blink durations themselves are fixed.

Layout assumptions: unit-square 2D mapping, aspect handled implicitly. No pixel-count hardcoding. The tuned geometry constants suit ~20x10 and larger; on very different displays the constants want re-tuning (the obvious improvement is exposing eye count, iris radius, and eye size as sliders).

## Colors
Iris: deep saturated blue, dark at center brightening to the rim. Eye outline: pure white, soft-edged thin line. Background: black.

## Controls
None exported. (Eye count and geometry are compile-time constants; see suggestion above.)

## Timing
Blinks every one-to-two seconds (randomized), each lasting about half a second. Gaze shifts about every half-to-one second (randomized), each glide taking roughly a third of a second. All timing is delta-time based, frame-rate independent.

## Clever bits
- Both eyes are one eye: the x fold means the ellipse/iris math is written once.
- The blink is achieved purely by animating the ellipse's vertical semi-axis inside the ellipse equation — the outline naturally squashes into a slit.
- The steep power-law shaping of the ellipse metric turns a filled ellipse test into a thin antialiased outline with no explicit line drawing.
