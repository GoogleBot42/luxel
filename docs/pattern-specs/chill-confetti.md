# chill confetti
kind: 1D
sensors: no

## What it looks like

Gentle confetti: individual pixels pop on at full brightness in near-matching hues and fade smoothly away over a second or two. New pops appear many times per second, scattered randomly across the strip, so at any moment a few dozen pixels glow at various stages of fading. The shared base hue drifts slowly around the color wheel, so the whole strip's tint gradually migrates — through one full rainbow in a few tens of seconds — while adjacent pops stay in a tight family of nearby hues. Calm, low-energy, screensaver-like.

## Algorithm

Persistent state: two per-pixel arrays (a hue and a brightness for every pixel), two independent elapsed-time accumulators, and a slowly-advancing global "current hue".

Per frame, two timers accumulate the frame delta:

- Fade tick, roughly ten times per second: every pixel's brightness is multiplied by a decay factor moderately below one (around five-sixths), giving an exponential fade-out lasting on the order of a second or two before a pixel is visually dark.
- Draw tick, slightly more often than the fade tick: one pixel is chosen uniformly at random; its brightness is set to full and its hue is set to the current global hue plus a small random jitter (up to a few percent of the color wheel either side, wrapped). Then the global hue advances by a tiny fixed step. The step size and tick rate combine so the base hue circles the whole wheel in a few tens of seconds.

The global hue starts at a random point on the wheel each time the pattern loads.

Per pixel (render): emit the stored hue at full saturation with the stored brightness. No gamma shaping is applied, so the fade is a plain exponential in the value channel.

Nothing is hardcoded to a particular pixel count; both arrays are sized from the configured count.

## Colors

Fully saturated hues from the whole wheel, but at any instant clustered within a narrow band around the drifting base hue, on a black background. No palette; hue space is used directly.

## UI controls

None. Fade rate, spawn rate, hue jitter width, and hue drift speed are edit-in-source constants (all four are natural candidates for sliders).

## Timing

Individual sparkle lifetime: roughly a second or two of visible fade. Spawn rate: on the order of a dozen per second. Full hue-wheel drift: a few tens of seconds. Because the ticks are delta-accumulated, behavior is frame-rate independent.
