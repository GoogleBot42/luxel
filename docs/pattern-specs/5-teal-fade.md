# 5 Teal Fade
kind: 1D
sensors: no

This pattern is trivial: the entire strip is a single fixed teal/cyan color at full saturation, and its brightness rises and falls together as one smooth sine-shaped fade. Every pixel is identical — there is no spatial variation, no randomness, no state between frames, and no UI controls.

Algorithm: each frame reads one sawtooth clock; each pixel outputs a hardwired teal hue, full saturation, and a brightness equal to a smooth wave of that clock. The full bright-to-dark-to-bright cycle is slow — on the order of half a minute. (The source also computes a second, much faster clock that is never used; omit it.)

Looks like: a calm, slow teal breathing effect. Works on any pixel count; nothing is hardcoded to layout.
