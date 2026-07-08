# lightning ZAP!
kind: 1D
sensors: no

## What it looks like

Pure white "lightning bolt" segments flash onto the strip and rapidly fade to black. Each flash lights a short run of contiguous pixels at full brightness; successive flashes continue from where the previous one ended, so over a few staccato bursts the lightning appears to "zap" its way progressively down the whole strip. The rhythm is irregular: mostly rapid-fire flashes tens of milliseconds to a few hundred milliseconds apart, with the occasional longer beat. Once a pass reaches the end of the strip, the strip goes dark for a longer rest (up to roughly a second), then a new pass begins from the start. Everything is white-on-black; the fade-out of each flash takes only a small fraction of a second, leaving a brief afterglow trail.

## State kept between frames

- A brightness buffer with one entry per pixel (the only thing the renderer reads).
- A write cursor: the strip position where the next flash segment will start.
- A countdown timer (in real time) until the next flash.

## Per-frame work (before rendering)

1. Decay every entry in the brightness buffer: each frame it loses a fraction of its value proportional to elapsed time (an exponential fade fast enough that a full-brightness pixel visibly dies out in a small fraction of a second), plus a minuscule constant subtraction so values actually reach true zero instead of asymptotically hovering.
2. Subtract the elapsed time from the countdown timer.
3. When the timer reaches or passes zero, fire a new flash:
   - Choose a random segment length between two bounds derived from the strip length — roughly between one-fifteenth and one-sixth of the total pixel count (so the effect scales with strip size; nothing is hardcoded to a specific count).
   - Set that many consecutive buffer entries to full brightness starting at the cursor, advancing the cursor as it goes, stopping early if the end of the strip is reached.
   - Rearm the timer with a random delay. The delay is drawn uniformly over a small window, given a small floor, and then squared — squaring skews the distribution so most gaps are very short (tens of ms) but some stretch to a few hundred ms, giving the jittery, lightning-like cadence.
   - If the cursor has reached the end of the strip, reset it to zero and instead rearm the timer with a much longer uniform random delay (with a floor of roughly a third of its maximum), on the order of a third of a second up to a bit over a second. This is the dark pause between full passes.

## Per-pixel render

Just look up the pixel's value in the brightness buffer and emit it as brightness with zero color saturation (i.e. white). A hue is nominally supplied but is irrelevant because saturation is zero; an implementer can treat the output as pure white at the stored brightness. No gamma shaping is applied beyond the raw value.

## Randomness

Three independent uses: flash segment length, inter-flash delay (squared-uniform, short-skewed), and the end-of-pass rest duration (plain uniform with a floor).

## Controls / palette

No UI controls, no palette — monochrome white. Timing and geometry constants are plain top-level tunables in the original; exposing sliders for flash rate and bolt size would be a natural enhancement but is not part of the spec.
