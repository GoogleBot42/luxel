# Example: Smooth Speed Slider
kind: 1D
sensors: no

## What it looks like
The classic scrolling rainbow: one full hue wheel stretched across the strip, sliding continuously along it. Visually identical to the stock default rainbow — the point of this pattern is the *technique* for its speed slider: dragging the slider while running changes the scroll speed smoothly, with no jump or skip in the animation.

## Algorithm
This is a teaching example about speed control. State between frames: a single phase value in the unit interval.

Per frame: instead of deriving the phase from the global animation clock, accumulate it manually — add (elapsed milliseconds since last frame × current speed) to the phase, wrapping with modular arithmetic to stay in the unit interval. Because only the *rate* of accumulation changes when the slider moves, the phase itself never jumps; a clock-derived phase would leap to a new value the instant its period changed, causing a visible skip. The stated tradeoff: slightly more code, and the animation can no longer stay synchronized with other devices driven by a shared clock.

Per pixel: hue = phase + (pixel index / pixel count), full saturation, full brightness. Fully proportional to strip length; no hardcoding.

Speed handling: the raw slider value is squared before scaling, giving much finer control at the slow end while still reaching the maximum at full deflection. The scale factor converts milliseconds to phase-per-frame such that the maximum speed is about one full rainbow cycle per second; slider at zero freezes the scroll.

## Colors
Full spectrum rainbow, fully saturated, full brightness — one complete hue cycle across the strip.

## UI controls
- Slider, "speed": scroll rate, from stopped to roughly one cycle per second, with a squared response curve for fine low-speed control.

## Timing
Continuous smooth motion; at max speed about one second per full cycle, and arbitrarily slower toward the bottom of the slider.

## Non-obvious notes
- The whole pattern exists to demonstrate delta-based phase accumulation versus clock-derived phase: accumulate rate × elapsed-time when you need glitch-free live speed changes; use the shared clock when you need multi-device sync.
