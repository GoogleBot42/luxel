# Rainbow rocket sparks
kind: 1D
sensors: no

## What it looks like
A fiery "rocket" travels along the strip toward the start end, looping around
forever. The rocket head is a short block of saturated fire colors (red through
orange to yellow), and trailing just behind it is a longer zone of randomly
flickering white sparks against black. The rest of the strip is dark. One full
traversal of the strip takes a couple to a few seconds. Because the fire colors
are painted from a gradient fixed to the strip rather than to the rocket, the
flame zone appears to shed a plume of exhaust behind it as it moves, cycling
red-to-yellow several times over the course of the trip.

## Algorithm
This is a stateless "everything is a traveling wave" pattern — no per-frame
state beyond a single looping sawtooth phase.

Per frame: sample a sawtooth clock whose full cycle is a couple-few seconds;
this is the shared travel phase.

Per pixel:
- **Spark window**: evaluate a traveling square wave of the pixel's normalized
  position plus the phase, with a duty cycle around fifteen percent. Pixels
  inside this window are eligible to spark. (Adding the phase to the position
  makes the window travel toward the low-index end.)
- **Spark decision**: a pixel actually sparks only if it is in the window AND a
  fresh uniform random draw beats a high threshold — roughly a one-in-twenty
  chance per pixel per frame. This gives dense white noise-flicker inside the
  window and nothing outside.
- **Fire window**: a second traveling square wave, same phase but offset ahead
  of the spark window by about five percent of the strip, with a narrower duty
  cycle (a bit over half the spark window's width). This is the rocket head.
- **Fire hue**: a repeating spatial sawtooth over pixel position, cycling
  through the red-to-yellow portion of the hue wheel about eight times along
  the strip. Crucially this depends only on position, not on time, so the flame
  zone reveals whatever part of the gradient it is currently passing over —
  that is what produces the "expelled exhaust" look instead of a plume glued to
  the rocket.
- **Compose in HSV**: saturation is one inside the fire window and zero
  elsewhere; brightness is one if the pixel is fire or a spark and zero
  otherwise. So each pixel ends up exactly one of: black (off), fully
  saturated red-to-yellow fire, or pure white spark (white achieved simply by
  zeroing saturation — the hue is computed but ignored).

## Colors
Fire: saturated red through orange to yellow, repeating gradient. Sparks: pure
white. Background: black.

## Controls
None.

## Notes for the implementer
- No pixel-count hardcoding; everything is in normalized position.
- The sparks re-randomize every frame (no persistence), which at typical frame
  rates reads as furious crackling.
- The trick worth keeping: build motion from phase-shifted square waves and a
  static positional hue ramp; there is no particle state at all.
