# RGBW Mapping Tester
kind: 1D
sensors: no

## Purpose and behavior
Trivial diagnostic pattern, not a decorative effect. The entire strip shows one solid color at a time, cycling forever through four equal-length phases: pure red, pure green, pure blue, then white. If the observed order or colors are wrong, the controller's color-order / LED-type settings need fixing.

## Algorithm
- Per frame: advance a repeating sawtooth phase whose full cycle lasts several seconds (each color therefore holds for over a second).
- Per pixel: pick the color purely from which quarter of the cycle the phase is in; every pixel is identical. Output is set in RGB directly (not HSV).
- The white phase drives all three channels at roughly one-third intensity instead of full, deliberately keeping total power draw about the same as the single-channel phases.

## State
None beyond the shared cycle phase. No layout assumptions; works at any pixel count.

## UI controls
None.
