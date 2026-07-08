# Cylon
kind: 1D
sensors: no

## What it looks like
The classic "Cylon eye" / scanner: a single bright blip sweeps from one end
of the strip to the other and back, endlessly, leaving a smoothly fading tail
behind it in the direction it came from. The head is sharp and bright; the
tail decays quickly at first glance because brightness falloff is
deliberately steepened. Default sweep takes on the order of a second per
pass; the tail fades out over about a second as well.

## Algorithm
State kept between frames:
- a per-pixel intensity buffer sized to the pixel count;
- the head position as a fractional pixel index;
- a direction flag (plus/minus one);
- the user's chosen hue and saturation, speed, and fade rate.

Per frame:
- Advance the head position by direction times speed times the frame's
  elapsed milliseconds (speed is expressed in pixels per millisecond, so
  motion is frame-rate independent). If it passes either end, clamp it to
  that end and flip the direction.
- Set the buffer entry at the head's integer (floored) position to full
  intensity.
- Decay every buffer entry linearly: subtract fade-rate times the frame's
  elapsed milliseconds, clamping at zero. The tail exists purely because the
  buffer persists between frames and is decayed rather than cleared.

Per pixel: read the pixel's buffer intensity, cube it, and use the result as
the brightness of an HSV color with the user's hue and saturation. Cubing a
linearly decaying value converts the linear tail into a short, punchy,
fast-falling glow.

Randomness: none. Layout: any 1D strip; the default and slider-scaled speeds
are proportional to the pixel count, so the traversal time is roughly
constant regardless of strip length. Only a 1D renderer exists.

## Colors
A single user-picked hue and saturation (default fully saturated red-end
hue of zero, i.e. red) on black. Brightness comes solely from the decaying
intensity buffer. Note: the color picker also reports a brightness component
and the source stashes it, but it is never used in rendering — implementers
may ignore it or (better) multiply it in as an easy improvement.

## Controls
- Color picker "Color": hue and saturation of the beam (picked brightness is
  effectively unused — see above).
- Slider "Speed": scales the sweep rate. Mostly linear in strip-lengths per
  unit time, with a tiny constant floor so it never fully stops; at the top
  end a pass takes a bit under half a second, and near the bottom it crawls.
  Note the slider's full-scale rate is somewhat faster than the pattern's
  initial default, so the default is not reproducible mid-slider exactly —
  not important to match precisely.
- Slider "Fade": tail decay rate with a small constant floor. Low values give
  a long tail persisting around a second or more; high values shrink the tail
  to just a few pixels behind the head.

## Timing
Defaults: roughly half a second to a second per end-to-end pass; a touched
pixel fades from full to black in about a second (before the cubic
steepening, which makes it look faster).

## Non-obvious points
- This is a trailing-buffer pattern, not a computed-tail pattern: the tail
  shape automatically reflects the head's actual speed history (slower sweep
  = shorter spatial tail), which looks more organic than an analytic tail.
- Only the single pixel under the head is stamped each frame; at very high
  speeds and low frame rates the head can skip pixels, leaving unlit gaps in
  the tail. Stamping the span between the previous and current head positions
  is a reasonable robustness improvement.
- The cubic brightness curve is essential to the look; with linear brightness
  the tail reads as a long dim smear.
