# SaberDeploy Tutorial
kind: 1D
sensors: no (a UI toggle stands in for a momentary hardware pushbutton; comments in the original suggest wiring it to a GPIO read instead)

This is a deliberately simple tutorial pattern; the spec is short because the effect is trivial.

## What it looks like
A "light saber blade" of solid, fully saturated red that extends from the start of the strip toward the far end when triggered, then retracts back on the next trigger. Pixels are either full-on red or completely off; the lit region is a contiguous run starting at index zero. At the default speed a full deploy/retract takes on the order of a second or two at typical frame rates.

## Algorithm
State kept between frames:
- an animating flag (is the blade currently moving),
- a direction sign (extending vs. retracting),
- the fraction of the strip currently lit (a value from none to all),
- the previous frame's button state (for edge detection).

Per frame:
1. Edge-detect the button: only the transition from not-pressed to pressed counts (the release edge is ignored). On a press edge, flip the direction sign and set the animating flag. Pressing mid-animation reverses the blade in place rather than waiting for it to finish.
2. While animating, add (direction × step size) to the lit fraction each frame.
3. If the lit fraction passes either end of its range, clamp it to the boundary and clear the animating flag.

Per pixel: light the pixel (full saturation red, full brightness) if its index is below lit-fraction × total pixel count, otherwise off.

Layout: fully layout-agnostic 1D; scales with pixel count.

Known weakness worth fixing in a reimplementation: the advance step is applied **per frame**, not per unit of time, so deploy speed depends on frame rate. The obvious fix is to scale the step by the frame's elapsed-time delta.

## Colors
Pure saturated red only (hue fixed at the red end of the wheel). Brightness is binary on/off.

## UI controls
- **Toggle** ("deploy/retract"): simulates a momentary pushbutton; each off-to-on transition toggles deploy vs. retract.
- **Slider** ("speed"): sets the per-frame advance step across roughly an order-of-magnitude range, from quite slow to fast. Even at the slider minimum the blade still moves (the mapping has a small floor added so it never reaches zero).

It also exports its internal state variables (moving flag, direction, lit fraction, button states) so they can be watched in a debugging/variable-watcher view; this is cosmetic for the tutorial and optional in a reimplementation.
