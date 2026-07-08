# NaturalLightSync
kind: 1D (layout-independent — every pixel shows the same color, so it works on any geometry)
sensors: no (uses the device's real-time clock)

## What it looks like
The whole strip glows a single uniform white whose color temperature tracks the time of day, like a circadian lamp. Around midday it is a neutral-to-cool daylight white; toward sunrise and sunset it warms through incandescent tones; through the night it stays parked at its warmest setting. Changes are imperceptibly slow — the color drifts over the course of hours, never within a frame.

## Algorithm
Configuration lives in constants at the top of the pattern (not UI controls): a sunrise hour and a sunset hour on a 24-hour clock, plus a maximum (coolest, midday) and minimum (warmest) color temperature. Defaults are roughly "standard daylight" for the max and "warm incandescent / early-sunset" for the min.

Per frame:
1. Compute the current time of day as a fractional hour from the clock's hour, minute, and second.
2. If the time is between sunrise and sunset, set the color temperature by evaluating a downward-opening parabola in time-of-day: its peak (coolest temperature) is at the midpoint between sunrise and sunset, and it passes through the minimum temperature exactly at sunrise and sunset. So the temperature rises smoothly from warm at sunrise to cool at solar noon and back down to warm at sunset.
3. Outside those hours, hold the minimum (warmest) temperature.
4. Convert the color temperature to an RGB triple using a standard piecewise blackbody-approximation curve fit (the well-known Kelvin-to-RGB approximation): below a mid-range threshold the red channel is pinned at full and green/blue follow logarithmic/linear fits of the temperature (blue clamped to zero at very warm temperatures); above the threshold blue is pinned at full and red/green follow inverse power-law fits. All channels are clamped to the displayable range.

Per pixel: emit that one RGB triple. No per-pixel variation, no state between frames beyond the derived color, no randomness.

The current color temperature and time-of-day are exported as watchable variables for debugging.

## Colors
A pure blackbody-white ramp: candle/ember warm white → incandescent → neutral white → daylight white. Never saturated hues.

## Controls
None exposed. The sunrise/sunset hours and the temperature range are edit-the-source constants; the obvious improvement is to expose all four as sliders. Note also that correctness depends on the device clock being set (e.g. via NTP or the app).

## Notes
This is close to trivial — a clock-driven solid color. The only substance is the parabolic day-arc and the Kelvin→RGB curve fit.
