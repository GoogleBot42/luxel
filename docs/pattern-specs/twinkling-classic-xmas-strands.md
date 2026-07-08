# Twinkling Classic Xmas Strands
kind: 1D
sensors: no

## Purpose and look
Emulates a classic multicolor Christmas light strand: every pixel holds one fixed color from a small holiday palette (about five colors), and individual bulbs "twinkle" — briefly swelling to a bright peak, then settling back to a dim resting glow. Meanwhile the whole strand slowly cross-fades among a few palette moods (vivid classic, warm washed-out pastel, cool/wintry) over tens of seconds. Designed for widely spaced bulb strands rather than dense strips; on a dense strip it looks like a static confetti of colored blocks that shimmer.

## Structure

### Per-pixel color assignment (done once at startup)
Each pixel is permanently assigned one of about five palette slots. Assignment is random but constrained two ways:
1. No two adjacent pixels get the same color slot.
2. Colors are kept locally balanced: model it as drawing colored balls from an urn without replacement, where a drawn color's ball is returned to the urn only after the draw has moved several pixels (roughly one palette-length) down the strand. So a recently used color is temporarily less likely, and over any short window the colors appear in nearly equal amounts, like a manufactured strand.
Concretely: start the urn with one ball per color; for each pixel after the first, pick uniformly among the balls excluding the previous pixel's color, remove the picked ball, and once the draw index passes a lag of about the palette size, re-add the ball for the color chosen that-lag-ago.

### Palettes
Three palettes, each holding the same five conceptual slots (red, green, amber/gold, blue, purple), tuned per mood:
- Palette A "classic": fully vivid red, green, amber, blue, and a purple sitting near magenta.
- Palette B "aged pastels, warm": washed-out warm red, a very dim desaturated green, washed amber, a dim muted teal-ish blue, and a strong pinkish purple slightly dimmed.
- Palette C "cool": a very pale dim red (almost dusty rose), vivid green, a soft warm-tinged white replacing amber, vivid blue, vivid violet.

Hues are stored through an inverse "perceptual hue" mapping and pushed through the forward mapping at render time: the forward map is a smooth S-curve on the hue circle that expands the crowded warm region so rendered rainbows look more perceptually even. A reimplementation can either replicate that pair of inverse/forward warping functions (a sine-based S-curve and its arcsine inverse) or simply store final hues directly — visually equivalent since inverse-then-forward is identity.

### Palette cross-fade (per frame)
A wall-clock accumulator wraps over a user-set cycle length (several seconds up to about half a minute). The fraction through the cycle selects a current palette and a blend fraction toward the next palette (wrapping around all three). Per pixel, the current and next palette entries for that pixel's slot are interpolated: hue along the shortest way around the hue circle, saturation and brightness linearly. At render time saturation is eased upward (square-root) and brightness eased downward (squared) — a gamma-ish correction so mid-fade colors don't look washed out or muddy.

### Twinkle (per pixel, per frame)
Every pixel gets a stable random phase offset in 0..1, generated once at startup by a deterministic xorshift-style PRNG seeded randomly (any per-pixel stable pseudorandom value works). A global twinkle clock runs on a period proportional to the pixel count and shrinking as the "twinkles" control rises (more twinkles = shorter cycle). Each pixel's brightness multiplier follows this cycle at its own phase offset:
- For most of the cycle the multiplier sits at a dim resting level (about a quarter).
- For a brief window (about a second's worth) it swells smoothly to a peak somewhat above full brightness (clipped at full), giving the peak a slightly flattened, held-at-max quality, then falls back. The swell shape is a smooth bump built from a reciprocal-of-sine curve; any smooth attack/decay bump with a flattened top reads the same.
Two extra behaviors keyed to the twinkle control: each frame each pixel also has a tiny random chance (growing with the control) of being momentarily blanked, adding sparkly flicker; with the control at maximum the multiplier is wrapped instead of clipped, causing pixels to blink out at the height of their swell (harsh strobe-y sparkle); with the control at zero all pixels sit at the steady dim resting level (no twinkle at all — a calm classic strand).

## Controls
- Slider "cycle time": total time to fade through all palettes, from several seconds to about half a minute.
- Slider "twinkles": twinkle density/intensity, from none (steady) through gentle occasional swells to constant hard sparkle.
- Slider "auto-fade palettes" (acts as a toggle: on above midpoint): when on, palettes cross-fade automatically over the cycle; when off, the manual palette slider takes over.
- Slider "manual palette select": sweeps continuously through the three palettes (including blends between adjacent ones) when auto-fade is off.
Defaults on first run: auto-fade on, medium twinkle, cycle in the mid-teens of seconds.

## Layout assumptions
1D, index-based; any pixel count. The twinkle period scaling with pixel count keeps roughly constant twinkles-per-second-per-strand regardless of length — preserve that proportionality.

## Non-obvious points worth keeping
- The urn-with-delayed-replacement color assignment is the signature detail; naive uniform random assignment produces visible clumps and repeats that this pattern deliberately avoids.
- Per-pixel twinkle phases must be stable across frames (precomputed), otherwise the effect becomes noise.
- Slot colors interpolate hue via shortest circular distance to avoid rainbow sweeps mid-fade.
