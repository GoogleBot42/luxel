# KITT (w/ color picker)
kind: 1D
sensors: no

## What it looks like

The classic Knight Rider scanner: a single bright dot of one user-chosen color
sweeps back and forth along the strip, bouncing off each end, dragging a short
comet tail of decaying brightness behind it. The sweep is fast — a full
end-to-end pass takes only a fraction of a second — and the tail dies out in
roughly a tenth of that scale, so it reads as a tight, snappy scanner rather
than a long lazy comet.

## Algorithm

State kept between frames:

- A floating-point "head" position along the strip.
- A direction flag (+1 or -1).
- A per-pixel brightness buffer, one entry per pixel.

Per frame:

1. Remember the integer pixel the head occupied last frame.
2. Advance the head by direction × elapsed-time × speed. Speed is chosen
   proportional to the pixel count, so the sweep takes the same wall-clock
   time regardless of strip length (a fraction of a second end to end).
3. If the head passes either end, clamp it to that end and reverse direction.
4. Set the brightness buffer to full for *every* integer pixel between last
   frame's head position and this frame's (stepping in whichever direction
   the head moved). This gap-filling matters: at high sweep speed the head
   can jump several pixels per frame, and without it the trail would have
   holes.
5. Decay every pixel's buffered brightness linearly by elapsed-time times a
   small fade rate, clamping at zero. The decay rate is such that a
   full-brightness pixel goes dark in on the order of a tenth of a second.

Per pixel (render): read the buffered brightness, cube it (this sharpens the
falloff so the tail looks like a hot core with a fast perceptual fade rather
than a linear smear), and emit it as the value channel of an HSV color whose
hue comes from the color picker and whose saturation is always full.

No randomness. No layout hardcoding — everything is expressed in terms of the
runtime pixel count.

## Colors

One solid hue everywhere, chosen by the user; fully saturated; brightness is
the cubed trail value. Default behavior with hue at the wheel's origin gives
the canonical red scanner.

## Controls

- One HSV color picker ("primary color" concept). Only the hue component of
  the picked color actually affects the output — the picked saturation and
  brightness are stored but never used (saturation is forced to full and
  brightness is the trail). An implementer may faithfully reproduce this
  quirk or simply expose a hue-only picker.

## Notes

The only clever bits are the between-frame gap filling (step 4) and the
brightness cubing for a punchy tail.
