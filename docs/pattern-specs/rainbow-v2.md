# Rainbow v2
kind: 1D
sensors: no

This is a trivial pattern: the classic scrolling rainbow with a full set of tweak sliders.

## Appearance
A rainbow gradient laid along the strip, scrolling smoothly and endlessly. Depending on slider settings it ranges from a full rainbow spread across the strip to the entire strip pulsing as one uniform hue cycling through the color wheel. Base cycle time is several seconds per full hue revolution.

## Algorithm
- Per frame: advance a single sawtooth phase from a built-in timebase; the period is scaled by the speed slider and its sign by the direction setting.
- Per pixel: hue = the global phase plus (pixel index ÷ pixel count) × a spread factor; saturation and brightness come straight from their sliders. Hue wraps around the color wheel naturally. No state beyond the phase; no randomness. Uses the runtime pixel count — no hardcoding.

## Controls (five sliders)
- **Color spread** ("color mod"): remapped so the slider's *top* end means zero spread (whole strip one hue) and lower values fan up to one full rainbow across the strip (with the gradient running in the reverse spatial direction, since the remap yields a negative spread). A port may want to make this mapping less surprising.
- **Speed**: scales the phase period. Quirk: because the slider multiplies the *period*, higher slider values are actually slower; near zero it gets very fast. Consider inverting for a port.
- **Direction**: a slider used as a toggle — below halfway scrolls one way, above halfway the other (implemented by negating the timebase period).
- **Saturation**: full color down to white.
- **Brightness/lightness**: overall value.
