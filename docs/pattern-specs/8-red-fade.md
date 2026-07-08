# 8 Red Fade
kind: 1D (index-independent; works on any layout)
sensors: no

This pattern is trivial.

The entire display shows a single uniform pure red that smoothly fades up from black to full brightness and back down, following a triangle-smoothed wave. One full fade cycle takes roughly half a minute. Every pixel is identical every frame — the pixel index is ignored.

Details:
- Hue fixed at red, full saturation; only brightness (value) animates.
- Brightness = a triangle/smoothed wave of a slow sawtooth clock. No gamma shaping beyond that.
- The source also computes a second, faster clock each frame that is never used — omit it.
- No state, no randomness, no layout assumptions, no UI controls.
