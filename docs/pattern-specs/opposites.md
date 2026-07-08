# opposites
kind: 1D
sensors: no

## What it looks like
A smooth, organic interference pattern. Bright colored blobs swell and shrink along the strip, separated by wide dark gaps. The blobs are drawn from two hue families that sit roughly opposite each other on the color wheel (hence the name), and the whole palette slowly drifts around the wheel over time. Motion feels liquid: two underlying swells travel in opposite directions along the strip, and where they align the strip lights up; where they cancel it goes nearly black. The overall rhythm is a gentle few-second pulse; nothing is abrupt.

## Algorithm
Stateless between frames apart from two free-running sawtooth clocks read each frame:
- Clock A cycles in roughly five to seven seconds.
- Clock B cycles about twice as slowly as clock A.

Per pixel (using the pixel's normalized position, zero to one, along the strip):
1. Wave 1: a smooth sinusoid (zero-to-one triangle-of-sine style "wave" builtin) of (clock A plus position) — a swell traveling one direction.
2. Wave 2: the same kind of sinusoid of (clock B minus position) — a slower swell traveling the opposite direction.
3. Wave 3: a sinusoid of the fractional part of (position + wave 1 + wave 2) — a warped composite of the first two.

Hue: take wave 3 modulo a value of roughly a third, giving a narrow hue band. If the result is above about half of that band, keep it; otherwise shift it by half the color wheel. This splits the output into two clusters of hues that are near-complementary. Finally add clock A so both clusters drift slowly around the wheel together.

Saturation: always full.

Brightness: the product of the three waves, each first lifted by a small offset (about a tenth) so the product never quite pins to zero; then the product is squared. The triple product creates brightness only where all three waves coincide; squaring deepens the darks and sharpens the bright blobs.

Note: the computed hue can exceed one; rely on hue wrapping (standard HSV hue wraparound), as the original does.

## Layout assumptions
Pure 1D, position-normalized — works at any pixel count with no changes. No hardcoding.

## Colors
Fully saturated. Two complementary-ish hue clusters at any given moment (e.g. a warm cluster opposite a cool cluster), continuously rotating through the whole rainbow over tens of seconds. Gaps between blobs are true black or near-black.

## UI controls
None.

## Timing feel
Blobs pulse and drift on a cycle of a few seconds; the palette takes on the order of ten seconds or more to visibly shift; full rainbow rotation is slower still.

## Clever bits
- The "opposites" effect comes from folding a wave into a narrow hue band and conditionally offsetting half of the band by half the wheel — cheap complementary color pairs.
- Brightness as a squared triple product of offset waves yields sharp, sparse highlights from purely smooth inputs.
