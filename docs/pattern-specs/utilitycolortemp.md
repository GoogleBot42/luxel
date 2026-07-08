# UtilityColorTemp
kind: 1D (index-agnostic — every pixel identical, so it works on any layout)
sensors: no

This is a trivial utility pattern, not an animated effect: it fills the entire display with a single solid color corresponding to a blackbody color temperature chosen by a slider.

## What it looks like
The whole strip is one uniform color along the incandescent/blackbody locus: candle-flame deep orange at the warm end, through warm white and neutral white, to pale sky blue at the cool end. With the slider parked at the very bottom, it instead demos itself: the color slowly and smoothly sweeps back and forth through the warm-to-white range (roughly candle to daylight) over a cycle of several seconds.

## Algorithm
State: one number, the selected color temperature. The slider maps linearly onto a temperature range from about one thousand kelvin to about fifteen thousand kelvin (the approximation is only trustworthy inside that range).

Each frame, pick the working temperature: if the slider is below the bottom of the valid range (roughly its lowest fifteenth of travel), substitute a triangle-wave-driven temperature oscillating between roughly one thousand and eight thousand kelvin (the demo mode); otherwise use the slider's temperature. Then convert temperature to red/green/blue:

- The conversion is a **piecewise analytic curve fit to published blackbody color tables** (Mitchell Charity's blackbody data, via the well-known Tanner Helland approximation approach). Work in units of hundreds of kelvin.
- Below a mid-range threshold (around the temperature where daylight white sits, roughly 6 500–7 000 K): red is pinned at full; green follows a logarithmic curve of temperature; blue is zero below roughly 2 000 K and rises linearly above that.
- Above that threshold: blue is pinned at full; red and green each fall off as power laws of temperature (red faster than green).
- All three channels are clamped to the valid range.

Per pixel: output that same RGB triple everywhere. The three channel values are computed once per frame, not per pixel.

## Colors
The blackbody locus only: deep ember orange → amber → warm white → neutral white → cool bluish white → pale blue. Never saturated non-thermal colors.

## Controls
- **Color temperature** (slider): sets the temperature across the whole valid range; bottom of travel enables the auto-sweep demo.

## Layout assumptions
None — no pixel count or geometry dependence. Only a 1D-style renderer is defined, but since it ignores the index it suits any display.

## Notes
Intended as a reusable utility: the temperature-to-RGB routine is the point, and the exact curve coefficients should be (re)derived by fitting the public blackbody table per the referenced method rather than copied.
