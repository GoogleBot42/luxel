# Color Pick Fade
kind: 1D
sensors: no

This pattern is trivial: soft pulses of one user-chosen color drifting along the strip.

## What it looks like
The strip shows a handful (four) of evenly spaced, soft-edged brightness pulses of a single solid color, gliding steadily along the strip against black. Gentle, low-key mood lighting; peak brightness is deliberately capped around half of maximum.

## Algorithm
State: one free-running cyclic clock, advanced per frame; its period is set by the speed slider. No other state; no randomness.

Per pixel: take the pixel's normalized position along the strip, multiply by a small integer (four) so the pattern repeats that many times across the strip, add the clock phase, wrap into the unit interval, and evaluate a triangle wave on it. Raise the result to the fourth power (narrowing each pulse into a soft bump with long dark gaps) and halve it. That is the brightness; hue and saturation come straight from the sliders.

Layout: position is normalized by total pixel count — no hardcoding; works on any strip length. Only a 1D renderer is provided; on 2D displays it just follows wiring order (obvious upgrade: add a 2D renderer using one axis as the position).

## Colors
A single flat color chosen by the user (defaults to a warm orange), fading smoothly to black between pulses.

## Controls
- Slider, "Hue": picks the pulse color around the color wheel.
- Slider, "Saturation": full color down to white.
- Slider, "Speed": sets the drift cycle length — note it scales the period, not the rate, so higher values are slower; range runs from sub-second frenzy to roughly a minute per cycle, defaulting to a leisurely several-seconds-to-teens drift.

## Timing
Default: pulses drift through a full cycle in on the order of ten seconds; adjustable per the speed slider.
