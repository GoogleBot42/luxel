# 2 Colors
kind: 1D
sensors: no

A completely static pattern: the strip is painted in alternating blocks of two user-chosen colors. There is no animation at all — the display only changes when the user moves a control.

## Visual behavior
The strip alternates between color A and color B in equal-width contiguous blocks: N pixels of A, then N pixels of B, repeating down the strip. The block width N is set by a slider from a single pixel up to roughly fifteen pixels wide.

## Algorithm
- State: a small flat array holding two HSV triples (color A in the first three slots, color B in the next three) plus one block-width number. All of it is driven purely by the UI controls; nothing is computed per frame (there is no per-frame hook).
- Per pixel: divide the pixel index by the block width, take the floor, and use its parity (even/odd) to select color A or color B. Emit that color's stored hue/saturation/value.
- Slider mapping: the slider's zero-to-one value is scaled to a max of about fifteen and rounded up (with a tiny epsilon so the very bottom of the slider still yields a width of one, never zero — width zero would divide by zero).
- No randomness, no time. Works on any pixel count; a 2D mapping would just show the blocks in index order.

## Colors
Entirely user-chosen via two color pickers. Defaults before the user touches anything: color A comes up as a fully saturated, full-brightness red-ish hue (hue at the start of the wheel); color B comes up black (all zero), so the strip initially looks like red blocks separated by dark gaps.

Quirk in the original worth knowing: the initialization block that was presumably meant to seed sensible defaults is garbled (it assigns to the wrong slots and uses self-assignment tricks), and it only ends up forcing color A's saturation and brightness to full. A reimplementation should just deliberately seed two visible default colors rather than reproducing the broken init.

## Controls
- Color picker "Color 1" (HSV picker): sets the first block color.
- Color picker "Color 2" (HSV picker): sets the second block color.
- Slider "Spacing": sets the block width, from one pixel up to roughly fifteen pixels.
