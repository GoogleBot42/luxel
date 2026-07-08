# sound - blinkfade
kind: 1D
sensors: yes

## What it looks like

Individual pixels blink on at random positions in saturated colors, then fade out.
With music playing, ignitions come in bursts on loud passages and the overall
twinkle density stays roughly constant no matter how loud or quiet the room is —
the pattern self-calibrates. The color of newly lit pixels drifts slowly over time
and additionally shifts with the pitch of the dominant sound, so a bass-heavy
passage tints new blinks differently than a treble-heavy one. Loud sound also makes
existing pixels fade out faster, so hits read as a churn: a burst of fresh blinks
plus quicker decay of the old ones.

## Sensor inputs

Requires the sensor expansion (microphone). Two conceptual inputs, delivered as
externally-updated globals:
- Overall sound energy (a single loudness scalar, averaged).
- Dominant frequency (the strongest pitch present, as a frequency value).

## Algorithm

State kept between frames:
- Per-pixel brightness array and per-pixel hue array (sized to the pixel count).
- A PI (proportional–integral) controller's state: its accumulated integral term,
  plus fixed proportional gain, integral gain, and clamp bounds for the integral.
- A brightness-feedback accumulator: during each render pass, every pixel adds its
  (clamped to unit) displayed brightness into this accumulator; it is read and
  reset at the start of the next frame.

Per frame (before render):
1. Auto-gain: compute the error between a target fill fraction (around a fifth of
   the strip lit, measured as average displayed brightness) and the actual fill
   from the previous frame's feedback accumulator. Feed that error to the PI
   controller — integral term accumulates the error (clamped to a generous range),
   and the output "sensitivity" is proportional gain × error + integral gain ×
   integral. This sensitivity scales everything sound-related, so the pattern
   converges to the same visual density in a quiet room or at a concert.
2. For every pixel:
   - Decay its stored brightness by a small time-proportional base amount plus a
     term proportional to (sound energy × sensitivity) — louder means faster fade.
   - If the brightness has reached zero or below, re-ignite it: new brightness =
     sound energy × sensitivity × a fresh uniform random number (so ignitions are
     random per pixel and scale with loudness). Assign it a new hue = a slowly
     cycling base hue (full cycle on the order of several seconds) plus a modest
     offset derived from the dominant frequency passed through a triangle fold
     (frequency normalized against a scale on the order of the audible range, so
     pitch maps to a bounded hue shift).

Per pixel (render):
- Displayed value = stored brightness scaled up a few times, then squared — the
  squaring gives punchy contrast and perceptually nicer fades.
- Add the clamped displayed value into the feedback accumulator for the controller.
- Emit HSV at full saturation with the pixel's stored hue.

## Colors

Fully saturated hues; the base drifts continuously around the whole color wheel,
with a pitch-dependent offset on top. No fixed palette — over time all hues appear.
Background is black.

## Timing

Base hue cycles over several seconds. Individual pixels typically live for a
fraction of a second to a couple of seconds depending on loudness. The auto-gain
settles over a few seconds after a big change in ambient volume.

## Controls

No UI controls. The fill target and PI gains are internal constants; the
sensitivity and sensor values are observable as exported variables.

## Layout assumptions

Pure 1D, arrays sized from the pixel count — scales to any strip. (Works on any
geometry since position is not used.)

## Notes / clever bits

The heart of the pattern is the closed feedback loop: displayed brightness is
summed during render, compared with a target fill fraction, and a PI controller
turns the error into a sensitivity multiplier applied to both ignition brightness
and decay rate. That is what makes it genuinely volume-independent, rather than
just thresholding the microphone. In silence the integral winds up until even
tiny noise triggers blinks; in loud environments it winds down.
