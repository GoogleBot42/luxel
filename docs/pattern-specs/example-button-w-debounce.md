# Example - Button w/ debounce
kind: 1D
sensors: yes (a physical push-button on a GPIO pin; no sound/accelerometer)

## Purpose
This is a tutorial/example pattern, not a decorative effect. It demonstrates how to read a
push-button on a digital input pin with software debouncing, and uses each confirmed press to
advance a "mode" counter. The visual output is deliberately trivial: the whole strip is a single
solid color that changes when the button is pressed.

## What it looks like
Every pixel shows the same fully saturated, full-brightness color. Each time the button is
pressed (and the press survives debouncing), the whole strip snaps to the next color in a fixed
cycle, then wraps back to the first. Nothing animates between presses.

## Hardware setup
One digital input pin is configured at startup with an internal pull-up resistor, so the line
idles high and reads low when the button (wired to ground) is pressed.

## Algorithm
State kept between frames:
- the last accepted (debounced) button state,
- an accumulating debounce timer (milliseconds of disagreement),
- the current mode index.

Per frame (before rendering):
1. Sample the pin.
2. If the raw reading matches the last accepted state, reset the debounce timer to zero.
3. If it differs, add the frame's elapsed time to the timer.
4. Once the timer exceeds a short threshold — on the order of a few hundredths of a second —
   accept the new state as real. If the newly accepted state is "pressed" (line pulled low),
   increment the mode index modulo the number of modes.

Per pixel: emit a solid HSV color whose hue is the mode index divided by the mode count
(saturation and brightness both at maximum). With the default small mode count this yields a few
hues spaced evenly around the color wheel (e.g. red, green, blue for three modes).

## Colors
Evenly spaced, fully saturated, full-brightness hues around the color wheel; one per mode.

## Controls
None in the UI. The only input is the physical button. The number of modes and the pin number are
plain constants a user is expected to edit in code.

## Layout assumptions
None; works on any pixel count since every pixel is identical.

## Notes for reimplementation
The debounce approach is "integrate disagreement": the timer only accumulates while the raw
reading disagrees with the accepted state, and resets on agreement, so glitches shorter than the
threshold never flip the state. Mode advances exactly once per press (on the accepted
high-to-low transition), not while held.
