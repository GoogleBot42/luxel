# Matrix 2 tone pulse
kind: 1D (index-only renderer; written with an 8-wide matrix in mind but does its own index math)
sensors: no

## What it looks like
A pulsing, plasma-like field restricted to exactly two colors: patches of a yellow-green tone and patches of its complementary blue-violet tone, blooming and dissolving. Crucially, regions never blend one hue into the other — they always fade down through black and back up into the other color, so the display reads as two-tone with dark seams. The texture churns continuously; an additional slower "zoom" oscillation makes the pattern's spatial scale swell and shrink over roughly ten-plus seconds, giving a breathing feel.

## Algorithm
Per frame: maintain two independent phase clocks (full-turn angular sawtooths with different periods, each a few seconds), plus a slowly oscillating scale factor that sweeps between one and several (period on the order of ten seconds).

Per pixel (from the index alone):
- Derive a pseudo row coordinate as the index divided by a small constant (three) — note this is *not* the declared matrix width, so rows are fractional; likely a happy accident kept for its look.
- Derive a pseudo column coordinate as the index modulo the *time-varying scale factor* — a non-integer, animated modulus, which is the main source of the churning, aliased plasma texture.
- Optional zigzag handling: a config flag mirrors alternate rows for serpentine-wired matrices (with the hardcoded width constant).
- Compute a single scalar: half of (one plus a sine of the column scaled by scale-over-width plus phase clock one, plus a cosine of the row scaled similarly plus phase clock two) — a classic two-wave interference plasma. Wrap it just below one.
- That one scalar drives everything: brightness is its cube (deep contrast, mostly-dark field with bright blooms), and hue is derived from it as follows.

Two-tone hue mapping (the interesting part):
- A "fade through black" factor is a triangle wave of the scalar at doubled frequency; it multiplies the brightness, forcing brightness to zero exactly where the mapping switches between the two tones, so hue transitions are always hidden in black.
- The scalar is folded so that its lower half maps into a narrow hue band (width about a tenth of the wheel) centered on the chosen primary tone, and its upper half maps into the same-width band centered on the complementary hue (half a wheel away). Within each band the hue sweeps across the band, giving slight shading variation inside each colored patch.

State between frames: none beyond the clocks. No randomness at all — fully deterministic.

Layout assumptions / fixes: the matrix width is a hardcoded small constant and the row divisor is a different hardcoded constant; the zigzag flag is a compile-time boolean. Obvious fix: derive width from the pixel map or expose it, use integer row = floor(index / width), and make zigzag a UI toggle. (Or better: implement as a true 2D renderer using mapped coordinates.)

## Colors
Exactly two tones: a primary in the yellow-green region and its complement in the blue-violet region, both fully saturated, each with a slight internal hue gradient, always separated by black. Overall dark with bright pulsing blobs.

## Controls
None exposed. Tone choice, band width, matrix width, and zigzag are edit-the-source constants. A natural improvement: a color picker for the primary tone and a toggle for zigzag.

## Timing
The two interference phases cycle every few seconds (slightly different periods so the pattern never exactly repeats quickly); the scale/zoom breath takes roughly ten to fifteen seconds per cycle.

## Non-obvious details
- One scalar field drives hue, brightness, and the black-seam mask simultaneously; the doubled-frequency triangle mask is what guarantees the two tones never touch.
- Taking index modulo a *fractional, animated* number is technically an aliasing artifact, but it is what gives the pattern its glitchy, shifting cell structure — reimplement it as-is rather than "fixing" it to integer coordinates, or the character changes.
