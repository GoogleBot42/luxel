# Rainbow Smiley
kind: 2D
sensors: no

## What it looks like
A static pixel-art smiley face displayed on a square LED matrix. The face disc itself (the round head, minus eyes and mouth) is solid white. Everything that is *not* the white face — the background around the head, plus the eye and mouth cutouts inside it — is filled with an animated rainbow: hue sweeps across the pixels in wiring order and continuously cycles, completing a full color rotation about every couple of seconds. The image itself never moves; only the rainbow fill animates.

## Algorithm
This is a bitmap-stamping pattern. A small square image (sixteen by sixteen) is baked into the pattern as data: a nested array of rows and columns, each cell holding a packed color value carrying red, green, blue channels plus an alpha/mask flag. The image is a classic smiley: a filled disc with two eyes and a smiling mouth. Face pixels are stored as white with the mask flag clear; background and the facial-feature cutouts are stored as black with the mask flag set.

Per frame: advance a single sawtooth phase clock (period around two seconds) used as the rainbow's base hue offset. (There is also vestigial machinery for a scrolling viewport offset and multi-frame animation — helper accessors for image/frame/row/column counts, a wrap-around pixel getter, and a loop timer — but the scroll step is disabled, so offsets stay at zero. A faithful reimplementation only needs the static lookup; note the getter wraps the column index around the image width when the viewport would run past the edge.)

Per pixel (2D renderer): the pattern assumes a square matrix — it takes the square root of the total pixel count as the edge length, scales the normalized 2D coordinates by that, and truncates to get integer image coordinates. Note the axes: the first normalized coordinate selects the image *row* and the second selects the *column*, so the image may appear transposed/rotated depending on the mapper. Look up the stored cell:
- If the mask flag is set (background, eyes, mouth): draw the animated rainbow — hue equals the phase clock plus the pixel's fraction of the way through the strip by linear index (so the full hue wheel is spread across the panel in wiring order, typically appearing as stripes or a serpentine gradient), at full saturation and full brightness.
- Otherwise (face pixels): draw the stored RGB directly, which for this image is plain white.

No state besides the phase clock. No randomness.

Packed-color detail (only matters if reimplementing the storage scheme): the four channels are packed into one number by shifting each channel to a different bit range of the platform's fixed-point number format — one channel in the upper integer bits, one in the lower integer bits, and two down in the fractional bits — with matching shift-and-mask extractors. A reimplementation is free to store the four channels any equivalent way (or just store "white vs. masked" booleans for this particular image).

Layout assumptions / hardcoding: hardcoded to a sixteen-by-sixteen source image, and the edge-length-from-square-root trick assumes a square matrix whose pixel count is a perfect square. Obvious fix: take width and height from the image data itself and map normalized coordinates against the actual image dimensions, and/or sample with independent x/y scaling so non-square layouts letterbox sensibly.

## Color
- Face: pure white.
- Background + eyes + mouth: fully saturated rainbow cycling through the entire hue wheel, spatially spread across the panel, temporally rotating.

## Controls
None.

## Timing
Rainbow hue completes a full cycle in roughly two seconds. Nothing else animates.

## Non-obvious details
- The alpha/mask channel of the embedded bitmap is used as a stencil selector: "masked" cells get the procedural rainbow, "unmasked" cells get their literal stored color. That inversion (the black background gets the rainbow, the drawn face stays white) is the whole trick.
- The rainbow's spatial gradient is keyed to the linear pixel index, not the 2D coordinates, so its visual direction depends on the matrix wiring order.
