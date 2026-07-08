# sound - spectromatrix agc
kind: 2D (implemented through the 1D renderer with a hardcoded matrix width and zigzag decode)
sensors: yes

## What it looks like
On a LED matrix, sound paints shifting plasma-like ribbons: smooth interfering diagonal bands drift and swirl across the panel, and wherever the music currently has energy in a given frequency band, the corresponding ribbon regions light up and bloom. Loud transients flare hot — peaks desaturate toward white while quieter lit areas stay richly colored — and everything leaves short decaying trails, giving a smoky, phosphor-like persistence. Hues slowly rotate around the wheel over several seconds. Crucially, an automatic gain control keeps the panel looking alive at any volume: in a quiet room it cranks sensitivity until whispers register; at a loud party it backs off so only a small fraction of the panel is lit at once. The overall coverage target is sparse — think under a tenth of the panel glowing at typical moments.

## Sensor inputs
- A multi-band audio spectrum array (32 bands, low to high frequency) from the sensor expansion board. This is the only sensor input.

## Algorithm
Layout: hardcoded matrix width (a mid-teens value) with a zigzag/serpentine row decode done in the pattern; height is implied by pixel count ÷ width. Obvious fix: derive geometry from the pixel map or expose width/zigzag as controls.

State kept between frames:
1. A per-band running average array (one slot per spectrum band), an exponential moving average with a window of about a second and a half. This is each band's personal "recent baseline."
2. A per-pixel brightness persistence buffer, decayed multiplicatively each frame it's touched (retaining most of its value), which produces the trails.
3. An automatic-gain state: a proportional-integral (PI) controller whose integral term is clamped to a bounded range. Its error signal is (target lit fraction − actual lit fraction from the previous frame), where actual lit fraction is the sum of all clamped per-pixel brightnesses accumulated during the last render pass, divided by pixel count. Its output is a global sensitivity multiplier applied to the raw spectrum. The proportional gain is small and the integral gain a few times larger; the integral starts at a moderate positive value so the pattern responds immediately at power-on.

Per frame:
- Advance two independent sawtooth time bases, one with a period of a few seconds and a slightly faster one, mutually incommensurate so the motion never visibly loops.
- Run the PI controller on the coverage error to update sensitivity, then zero the coverage accumulator for the coming frame.
- Fold the current (sensitivity-scaled) spectrum into the per-band running averages, weighted by frame delta over the averaging window; averages are floored at a tiny positive value to avoid division/degeneracy issues.

Per pixel:
- Decode x and y from the index, un-zigzagging alternate rows.
- Compute a fractional band coordinate spanning the spectrum: take a smooth wave of (x scaled by width plus a wave of the slow time base), add a smooth wave of (y scaled by width minus the same moving offset), average them, add the faster time base, and pass the result through a triangle fold before scaling to the top band index. The two position-plus-time waves interfering is what creates the drifting plasma ribbons; the triangle fold keeps the coordinate bouncing across the band range instead of jumping at wrap.
- Sample both the raw spectrum and the running-average array at that fractional coordinate using linear interpolation between adjacent bands.
- Brightness = (current sensitivity-scaled band level minus that band's running average), i.e. only above-baseline energy shows, amplified strongly, and additionally weighted upward for bands whose baseline is itself energetic (a large multiple of the average plus a constant floor of about a half) — this makes the dominant part of the spectrum bloom harder. Negative values are cut to black; positive values are squared to add contrast/punch.
- Hue = a small fraction of the color wheel spread across the band range (so at any instant the panel spans a modest hue arc, not the full rainbow) plus the slow time base, which rotates the whole palette continuously.
- Saturation = full minus brightness, so the hottest peaks whiten.
- Blend into the per-pixel persistence buffer (old value mostly retained plus the new brightness), display the buffered value, and add its clamped value into the coverage accumulator feeding next frame's AGC.

Randomness: none — all motion is deterministic waves; all liveliness comes from the audio.

## Colors
At any moment: a narrow band of neighboring hues (a rainbow arc) painted across the ribbons, fully saturated at low intensity, bleaching to white at peaks, over black. Over several seconds the arc migrates through the entire rainbow.

## Controls
None exposed. Natural slider candidates in the source constants: target coverage fraction, trail fade, averaging window, ribbon drift speed, and matrix width/zigzag.

## Notes — the clever parts
- The AGC loop is closed through the render pass itself: rendered brightness is the sensor for the controller, so "how lit does the panel look" is regulated directly rather than inferring loudness. The clamped integral term prevents wind-up during long silence or sustained noise.
- Subtracting each band's own recent average makes the display respond to change (onsets, beats) rather than sustained tones, and normalizes across the spectrum's natural low-frequency tilt.
- Squaring brightness plus inverse saturation gives a cheap "white-hot core, colored halo" look without extra palette machinery.
