# Bessel Chaos

kind: 1D
sensors: no

## What it looks like

Interference-pattern chaos in the blue/cyan family. Around a wandering focal point on the strip there are broad, slow, breathing blobs of light; moving away from that point the bands compress rapidly into fine, shimmering, chaotic ripples. The focal point pans back and forth along the strip, and with the default control setting it dwells near each end and then snaps quickly across the middle. Everything else — band phase, color spread — drifts on independent slow cycles (several seconds to tens of seconds), so the overall texture never visibly repeats. The author's stated goal (per their comment) was chaotic-looking behavior with **no randomness at all** — only incommensurate deterministic oscillators.

## Algorithm

Stateless per frame apart from a handful of scalars recomputed each frame; no arrays.

Per frame:
- A **phase driver**: a sine-shaped oscillation (period on the order of ten-plus seconds) scaled so it sweeps across roughly two full turns of angle. This phase feeds the per-pixel sines below (one wave gets twice this phase, the other gets it negated), so the interference bands drift and counter-drift.
- A **color-spread divisor** that oscillates over tens of seconds between a small value and about triple it — controlling how far hues deviate from the base color.
- A **pan position**: a sine-shaped oscillation with a period around fifteen seconds, passed through a symmetric ease-in-out shaping function whose sharpness is set by the UI slider. The shaping curve is an odd-root-style ease built from a power function (documented by the author with a graphing-calculator link); because a fractional root of a negative number misbehaves, it is applied mirror-image on each half of its domain. At high slider values the exponent becomes very large, turning the smooth sweep into "dwell at the ends, fast transition through the middle."
- A third fast clock is sampled but **never used** — dead code; the implementer may omit it.

Per pixel:
- Compute a signed coordinate: the pixel's normalized position, minus a half (centering), minus the pan value, all scaled up by a small factor (about three). Call it d.
- Form a strongly nonlinear "squeeze" of it: d cubed times a factor of about ten. This is what makes spatial frequency explode away from the focal point — near the pan center the argument changes slowly (broad bands), far away it changes extremely fast (fine chaos).
- Two sine waves: one of (the squeeze times d — i.e. a quartic in d — plus twice the frame phase), one of (the squeeze alone minus the frame phase).
- Brightness: average the two waves, then cube the average. Cubing an average that ranges over positive and negative values yields signed results; negative values clamp to black in the HSV call, so large regions are dark, and the surviving positive crests are sharply peaked.
- Hue: start from the middle of the hue wheel (the cyan region) and offset it by the second wave divided by the frame's color-spread divisor. When the divisor is small the colors range widely around cyan (into blues, greens, violets); when large, the strip is nearly monochrome cyan.
- Saturation is always full.

No pixel-count hardcoding; all positions are normalized, so it scales to any strip.

## Colors

Base color sits in the cyan/turquoise region. Hues breathe outward from there — toward blue/violet on one side and green on the other — with the spread itself slowly pulsing over tens of seconds. Deep black between crests; fully saturated colors elsewhere.

## Controls

- **Slider — "transition speed"** (default set fairly high): shapes the panning motion of the focal point. At the low end the pan is a smooth continuous sweep; at the high end the focal point parks near each end of the strip and jumps across quickly.

## Timing

Pan round-trip: on the order of fifteen seconds. Band drift: roughly ten-second-scale phase cycle. Color-spread breathing: a couple tens of seconds. All periods deliberately non-commensurate so the composite never repeats visibly.

## Non-obvious details

- The "chaos" is entirely deterministic: it comes from a cubic/quartic spatial argument inside sines (a chirp), driven by several slow oscillators with unrelated periods.
- Cubing the averaged waves both sharpens the crests and exploits negative-to-black clamping to create the dark background for free.
