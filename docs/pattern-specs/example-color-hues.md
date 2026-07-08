# Example: color hues
kind: 1D
sensors: no

## Purpose
A tutorial/demo pattern, the color-focused sibling of the "time and animation" example. Every pixel is at full saturation and full brightness; the only thing that varies is hue as a function of position. It cycles automatically through roughly a dozen static "modes", each showing a different way to map position to hue. Nothing animates within a mode — the only motion is the mode switching itself, a bit less than once per second.

## What it looks like
A rapid slideshow of static colorings of the strip: a rainbow repeated a few times along its length; solid red; solid green; solid blue; solid red again (demonstrating that the top of the hue range wraps back to red); narrow repeated rainbow slices with a hard sawtooth edge; the same span of hues but with smooth triangle-wave transitions; a sine-eased version; hard two-tone stripes (alternating between two cool hues); several "texture" modes where multiplied or subtracted waveforms produce banded, layered multicolor patterns; a mode where a coarse quantization error is overlaid on a gradient making stepped bands; and a symmetric gradient centered on the strip.

## Algorithm
State kept between frames: an accumulated elapsed-time counter and the current mode index — nothing else.

Per frame: add the frame delta to the accumulator; when it passes a threshold just under a second, subtract the threshold and advance the mode index, wrapping.

Per pixel: normalize index to 0..1 and scale by a small integer factor (about four) so position-driven modes repeat about four times along the strip. Feed that spatial value to the active mode's function; use the result directly as hue with full saturation and brightness. (Hue values outside 0..1 wrap.)

Modes are stored as an array of tiny single-argument functions and dispatched by index — that array-of-lambdas structure is the teaching point; keep it.

Mode catalog, conceptually:
1. Hue equals position — repeated full rainbows.
2. Constant hue at the bottom of the range — red.
3. Constant hue one-third around — green.
4. Constant hue two-thirds around — blue.
5. Constant hue at the top of the range — wraps to red again.
6. Position wrapped into a narrow band (about a fifth of the wheel) via modulus — repeated warm rainbow slivers with sharp edges.
7. Triangle wave of position scaled into the same narrow band — smooth back-and-forth through those hues.
8. Sine-like wave of position scaled into the narrow band — smooth but nonlinearly distributed.
9. Half-duty square wave of position, scaled to half the wheel and offset toward green/blue — hard alternating stripes.
10. Product of a sine-like wave of position and a triangle wave at a few times the frequency, scaled small — fine multicolor banding texture.
11. A wrapped, scaled wave of position minus a scaled triangle of position, offset toward blue — more layered texture with hue discontinuities.
12. Position plus its own coarse modulus remainder, scaled down — a gradient with visible stepped "error" bands.
13. Absolute-value fold of scaled position around the strip midpoint — a symmetric hue gradient mirrored about the center.

Exact math per mode need not match; each mode's described visual character is the requirement.

## Colors
Full-vividness spectrum colors throughout; specific hues only as noted per mode (red / green / blue anchors, warm slivers, cool stripes).

## Controls
None. (Source includes a commented-out hook to pin one mode for study; optional to expose.)

## Timing
Mode advances a bit less than once per second, forever. Within a mode, the image is static.

## Layout assumptions
1D, any pixel count; position is normalized so nothing is hardcoded.
