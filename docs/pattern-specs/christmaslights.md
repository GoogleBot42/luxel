# ChristmasLights
kind: 1D
sensors: no

## What it looks like
The strip is divided into fixed-size blocks of consecutive LEDs (a couple dozen per block). Blocks take one of three looks — vivid red, vivid green, and a dimmed soft white at roughly half brightness — repeating in sequence down the strip. Every couple of seconds all blocks switch simultaneously, each block stepping to the next look in the cycle, so the whole strip appears to rotate its coloring one role at a time. It reads as classic blinking holiday string lights: static blocks, hard synchronized switches, no fading.

Important honesty note: the source's own comment claims the colors are red and blue, but the actual encoded hues wrap around the color wheel to red and green. The spec above describes real output (red / green / soft white); a reimplementation should match that.

## Algorithm
State between frames: an elapsed-time accumulator and three "role" values (the look currently assigned to block-position 1, 2, and 3 of each repeating triplet). Roles are encoded as slightly-offset hue codes; one specific code is treated specially as "white."

Per frame: add the frame delta to the accumulator. When it exceeds the blink interval (about two seconds), reset it and rotate the three role assignments through a fixed three-state cycle, so each block position steps red → green → white → red…

Per pixel: integer-divide the pixel index by the block size, take it modulo three to find which of the three roles applies, then emit that role's color: the "white" code renders desaturated at roughly half brightness; the other two render as fully saturated full-brightness hues (one at the red point of the wheel, one about a third of the way around, i.e. green).

No randomness. Layout-independent apart from the hardcoded block size (fine for any strip length; block size is an obvious slider candidate, as is the blink interval).

Startup quirk in the original: two of the three role values are uninitialized until the first switch fires, so for the first interval the second and third blocks both render as red. Not worth reproducing deliberately; initializing all three roles is the sane behavior.

## Colors
Pure saturated red; pure saturated green; soft white at noticeably reduced brightness. Black never appears.

## Controls
None exposed. Block size and blink interval are edit-the-source constants.

## Notes
Trivial-plus: a three-state block rotation on a timer. The only subtlety is the hue-wrapping trick used to encode three roles in one number, which a reimplementation should replace with a plain enum/state variable.
