# Single Color Picker - wide or spot
kind: 1D
sensors: no

This is a simple, mostly-static utility pattern; short spec on purpose.

## What it looks like
The whole strip shows a single user-chosen color. Depending on a "sharpness" slider, that color is either spread across the entire strip (brightest at a chosen spot, gently dimming away from it) or concentrated into a tight bright spot at the chosen location with black everywhere else. Nothing animates on its own; the display only changes when the user moves a control.

## Algorithm
No per-frame state or animation. Per pixel: compute the absolute distance between the pixel's normalized position along the strip (index divided by pixel count — automatically layout-agnostic) and the chosen focus location (also normalized). Take one minus that distance, raise it to a "sharpness" exponent, and use the result to scale the picked color's brightness. Hue and saturation come straight from the picker.

- Exponent near zero: the falloff term flattens toward one everywhere — uniform wash of the color.
- Exponent of one: linear fade from the focus point.
- Large exponent (the slider tops out around a couple orders of magnitude above one): a sharp spot.

Defaults on load: a violet-ish hue at full saturation/brightness, focus at the strip's midpoint, a moderate exponent.

## UI controls
- Color picker (hue/saturation/brightness): the displayed color and its peak brightness. All three components are stored to exported variables so they can also be driven via the API.
- Slider, "sharpness": mapped through a squaring curve before scaling to the exponent range, so the low end of the slider has fine control (wide washes) and the top end reaches very spiky. Values below one act as a blur/spread rather than a sharpen.
- Slider, "location": the normalized position of the brightness peak along the strip.

## Notes
1D renderer only. Trivial by design — its point is demonstrating a picker plus two sliders driving exported variables.
