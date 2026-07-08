# Three Red Pixels (mathy)
kind: 1D
sensors: no

This pattern is deliberately simple — it is a teaching example about frame-rate-independent motion and modular arithmetic.

## What it looks like
A solid, fully bright blue strip with three single red pixels spaced evenly (one third of the strip apart), all crawling steadily in one direction and wrapping around at the end. Motion is smooth and slow — on the order of ten pixels per second — and continues forever with no other change.

## Algorithm
State: one fractional position value, persisted across frames.

Per frame: advance the position by (speed × elapsed frame time), where speed is expressed in pixels-per-second and the frame delta is converted from milliseconds to seconds — this makes the motion identical regardless of frame rate. Wrap the position back into strip range with a modulus by the pixel count.

Per pixel: compute the pixel's offset behind the moving position (adding a full strip length before subtracting so the value never goes negative), then take that offset modulo one-third of the pixel count (the spacing, computed as the floor of pixel count divided by the number of dots). If the floor of the result is zero — i.e. the pixel lies within one pixel-width of the position or of one of its equally spaced images — the pixel is red; otherwise blue. Saturation and brightness are full everywhere.

No randomness. The dot count is fixed at three via a variable, so generalizing to N dots is trivial (and is the obvious enhancement, e.g. exposing it as a slider). The moving position is exported for external inspection/adjustment.

Layout note: works for any pixel count; when the count is not an exact multiple of the dot count, the leftover tail past the last full spacing interval can briefly show an extra red pixel as the pattern sweeps by — a harmless artifact of the flooring.

## Color
Fully saturated primary blue background; fully saturated primary red dots. Full brightness throughout.

## Controls
None (speed and dot count are internal constants; both are natural slider candidates).

## Timing
A slow, constant crawl, roughly ten pixels per second; time to lap the strip scales with strip length.

## Non-obvious details
The three dots are not tracked separately: a single moving position is mirrored by taking pixel offsets modulo one-third of the strip, so one comparison per pixel yields all three dots. Delta-scaled movement (pixels-per-second × seconds-elapsed) is the other point of the exercise: motion speed stays correct at any frame rate, and fractional position accumulates smoothly between pixel steps.
