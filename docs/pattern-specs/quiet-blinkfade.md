# quiet blinkfade
kind: 1D
sensors: no

## What it looks like
A sparse, calm twinkle in a single color: individual pixels light up at a random modest brightness (never more than about half), fade smoothly and quickly to black over up to about a second, then stay dark for several seconds before relighting. Because the dark dwell is much longer than the lit time, only a small fraction of the strip is lit at any instant — hence "quiet". All sparks are the same fixed hue in the purple/magenta region, fully saturated, on black.

## Algorithm
State kept between frames: one value per pixel.

The value is a combined brightness-and-dead-timer. It decreases linearly at a constant rate per unit of real time (scaled by the frame delta, so the fade speed is frame-rate independent). It keeps decreasing past zero into negative territory; while negative the pixel is simply dark. When it reaches a fixed negative floor, the pixel "respawns": the value is set to a fresh uniform-random brightness between zero and the modest cap.

The decay rate, respawn cap, and negative floor are proportioned so that the lit phase lasts at most about a second and the dark phase lasts several times longer (roughly a six-to-one dark-to-lit ratio at maximum brightness).

Initialization: every pixel's value is seeded uniformly at random across the entire range from the negative floor up to the cap, so the population is fully desynchronized from the first frame — no startup wave.

Per pixel (render): if the value is positive, output the fixed hue at full saturation with brightness equal to the value squared (squaring gives a gentle ease-out and avoids a visible linear ramp; it also makes the low tail vanish gracefully). If the value is zero or negative, output nothing (dark). The positive check matters because squaring a negative dead-timer would otherwise light the pixel back up.

Randomness: uniform random draws only — once at init for phase spread, and once per respawn for the new peak brightness.

## Layout assumptions
None; scales with pixel count.

## Controls
None exported. Natural extensions: sliders for spawn density (the negative floor depth), fade speed, and a hue picker.

## Timing
Fade-out of a bright spark: up to about a second. Dark interval before rebirth: several seconds. Per-pixel cycles are unsynchronized.

## Notes
The single-scalar trick — letting one per-pixel number serve as both the visible brightness (when positive) and the countdown-to-respawn (when negative) — is the whole pattern; it needs no separate timers or state machines.
