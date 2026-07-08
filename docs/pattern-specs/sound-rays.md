# sound - rays
kind: 1D
sensors: yes

## What it looks like
A scrolling "chart recorder" of the room's sound. A write head crawls slowly along the strip; wherever it passes, it stamps a bright dot whose brightness reflects how loud the dominant tone was at that instant and whose hue reflects that tone's pitch. Because the whole recorded trail is drawn offset by the head position, the history appears to stream steadily along the strip (in the reverse of the strip's native index direction), so bursts of sound become moving rays/streaks of color that fade into darkness where the room was quiet. On top of the pitch-derived hue there is a gentle rainbow gradient along the strip and a slow continuous hue rotation, so even a monotone input drifts through colors.

## Sensor inputs (conceptual)
- The magnitude of the strongest frequency bin (dominant-tone loudness) — drives brightness.
- The frequency of that strongest bin (dominant pitch) — drives hue; the pitch is normalized against a ceiling of a few kilohertz, so bass maps near one end of the hue wheel and treble sweeps further around it.
- Overall sound energy is declared as an input but is not actually used; only the dominant-bin pair matters.

## State kept between frames
- Two circular buffers, each one entry per pixel: one for recorded brightness values, one for recorded hues.
- A fractional write-head position that wraps around the pixel count.
- The last brightness value written (fed back into the gain controller).
- The state of an automatic-gain controller: a proportional-integral (PI) feedback loop. Its target is to keep the recorded brightness hovering around mid-scale. Each frame the error (mid-scale minus last written value) is accumulated into a clamped integral term; the output gain is proportional term plus integral term. The integral starts biased fairly high and is clamped to a wide non-negative range, so the pattern boots sensitive and then settles. This makes the display self-calibrate to quiet or loud rooms over a few seconds.

## Per-frame work
1. Update the PI controller to get the current sensitivity gain.
2. Advance a slow global hue phase (one full lap in several seconds).
3. Advance the write head proportionally to elapsed time — on the order of a few tens of pixels per second, wrapping at the pixel count. Note the head position stays fractional but is used directly as a buffer index (Pixel Blaze array indexing truncates); a reimplementation should truncate when indexing.
4. At the head, store: brightness = (dominant magnitude × gain), squared; hue = normalized dominant pitch.

## Per-pixel work
- Reverse the pixel index (so motion runs opposite the native direction), then add the head position and wrap by pixel count to pick a buffer slot — this is what makes the stored history scroll.
- Brightness = stored value squared again (so quiet moments crush to black and only real peaks glow — effectively a fourth-power curve overall).
- Hue = stored pitch hue + (position along strip scaled so the whole strip spans about a quarter of the hue wheel) + the slow global hue phase. Full saturation always.

## Colors
Full-saturation rainbow hues on black. Hue is pitch-driven, tinted by a quarter-wheel positional gradient and a continuous slow rotation; no fixed palette.

## Controls
None (self-adjusting gain replaces a sensitivity knob).

## Layout notes
Fully pixel-count-relative; no hardcoding. 1D only; on a matrix it would just paint in index order.

## Non-obvious bits
- The PI auto-gain is the heart of it: it chases "average peak ≈ mid-scale," so the display looks lively at any volume.
- Recording squared values and squaring again at render time gives strong contrast without ever clipping visibly.
- Scrolling is achieved by moving the read offset, not by shifting buffer contents — cheap and smooth.
