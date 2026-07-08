# color twinkle bounce
kind: 1D
sensors: no

A small, stateless, purely mathematical pattern — a few lines of trigonometry. Short spec accordingly.

## What it looks like

Soft glowing crests of light, spaced roughly a dozen pixels apart, with dark gaps between them. The whole comb of crests sways back and forth along the strip in a smooth sinusoidal "bounce," each sway taking a few seconds, travelling a bit less than one crest-spacing in each direction. Meanwhile the colors continuously drift through the whole rainbow over several seconds. Within each crest the hue also varies with position — the center of a crest is fairly uniform, with color shifting toward the dim fringes — so different crests and crest edges show different rainbow slices at any instant. Peak brightness is deliberately capped around half of full.

## Algorithm

No state between frames beyond the global clocks; no randomness; every pixel is a pure function of (pixel index, time).

Per frame: sample a repeating clock a few seconds long and convert it to an angle. (The source defines two such clocks, but they have the identical period and phase, so they are always equal — the behavior is exactly that of a single clock. A reimplementer could either keep one clock or deliberately give them different periods to decouple the sway of brightness from the sway of color.)

Per pixel:

- Compute a sine over the raw pixel index (spatial angular frequency around half a radian per pixel, i.e. wavelength of roughly a dozen pixels), phase-shifted by a term proportional to the sine of the clock angle with an amplitude of a few radians. Normalize to a zero-to-one wave. The oscillating phase term is what produces the bounce.
- Brightness = that wave raised to the fourth power, then halved. The fourth power is the "twinkle" trick: it sharpens the broad sine humps into narrow bright crests with wide dark valleys.
- Hue = a slowly advancing sawtooth ramp (several seconds per full rainbow revolution) plus the same spatial sine term (un-normalized, so its span covers the color wheel about twice per spatial wavelength; hue simply wraps). Because brightness and hue share the same underlying sine, hue variation is slowest exactly at the bright crest centers and fastest at the dim edges.
- Emit fully saturated HSV.

## Layout assumptions

The spatial wave is a function of the raw pixel index, not of position normalized by pixel count — so the crest spacing is a fixed number of pixels regardless of strip length. On any strip it works, but longer strips just show more crests. If scale invariance is wanted, normalize the index by pixel count and multiply by a chosen crest count; the original's character depends on absolute pixel spacing, though.

## Controls

None.

## Timing

Sway cycle: a few seconds. Full rainbow drift: several seconds. Both free-running and unsynchronized in feel (though mathematically the sway repeats on its own period).
