# twinkly stars
kind: 1D
sensors: no

(Despite being catalogued with sound patterns, this pattern uses no sensor input — only randomness.)

## Visual behavior
The whole strip glows steady, fully saturated blue at full brightness. Every so often a random pixel "twinkles": it snaps instantly to pure white, then eases back to blue over roughly a second (a few dozen frames), like starlight glinting on a night-sky tree. Twinkles are independent per pixel, sparse (roughly one-in-a-hundred chance per pixel per frame), and constantly scattered across the strip.

## Algorithm
- State: one counter per pixel recording how many frames have elapsed since that pixel last twinkled.
- Per pixel, per frame:
  - If the pixel is mid-recovery (counter below the recovery length, a few dozen frames), set saturation proportional to counter ÷ recovery-length (so it climbs linearly from white back to full blue) and increment the counter.
  - Otherwise, roll a random chance (about one in a hundred by default); on success set saturation to zero (white) and restart the counter at the beginning of the recovery ramp.
- Hue is fixed at blue and brightness fixed at full at all times; only saturation animates.
- The random check is implemented via a probability helper that converts a "1 in N" chance into a threshold test on a random draw, with a manual round-half-up (the platform has no rounding builtin).

## Colors
Solid pure blue background; twinkles are pure white fading back through pale blue to full blue.

## Timing
Frame-count based, not time based: recovery takes a fixed number of frames, so the twinkle fade speed depends on frame rate (faster strips = quicker twinkles). Obvious fix: drive the recovery ramp from elapsed-time deltas instead of frame counts, and scale the twinkle probability by elapsed time as well.

## UI controls
None. Two tuning constants exist in the source (recovery length in frames; twinkle probability), intended to be edited by hand — natural candidates for sliders.

## Notes
Trivial pattern; per-pixel counter array scales with pixel count, no other layout assumptions.
