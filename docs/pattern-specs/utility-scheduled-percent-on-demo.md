# Utility: Scheduled Percent-On Demo
kind: 1D (index-agnostic; every pixel identical, so it works on anything)
sensors: no — but it reads the device's real-time wall clock (hour, minute, second). It is NOT sound- or motion-reactive despite being filed that way; there is no sensor-board input.

## What it is
A minimal tutorial/utility pattern, not a visual effect. It demonstrates computing "how far are we through a daily scheduled on-window" from the wall clock, and visualizes that fraction by mapping it to hue. All pixels always show the same solid color.

## Behavior
Two sliders pick a start hour and an end hour, each quantized to a whole hour of the 24-hour day. Every frame the pattern reads the current hour, minute and second, converts them to a fractional hour-of-day, and computes the fraction elapsed between the start hour and the end hour (window length wraps around midnight when end precedes start). While the current hour is inside the window, that fraction (0 at window start, approaching 1 at window end) is exposed and used directly as the hue — so over the course of the scheduled window the whole display sweeps once slowly through the entire rainbow, red at the start, back toward red at the end. Outside the window the fraction is zero, which — since saturation and brightness stay at maximum — renders solid red, not off. (It demonstrates the schedule variable; it does not actually black out.)

## Controls
1. **Begin time** — slider mapped across the 24 hours of the day, floored to a whole hour.
2. **End time** — same mapping.

## Implementation notes / known flaws (faithful port vs. fix)
- All clock reads and schedule math happen inside the per-pixel render function; they are pixel-independent and belong in per-frame setup. Fixing this is safe and invisible.
- The window-length calculation handles wrap past midnight, but the "are we inside the window" test is a plain `start <= hour < end` comparison that does NOT handle overnight windows — a window like late evening to early morning never tests true. Port the bug or fix it, but document the choice.
- Several intermediate values (current h/m/s, window length, on-fraction) are exported as inspectable variables for the tutorial's sake; mirroring that is optional.

This pattern is trivial by design; the spec above is complete.
