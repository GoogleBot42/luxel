# 1 White Fade
kind: 1D
sensors: no

This pattern is trivial: the entire strip breathes in plain white.

## Visual behavior
Every pixel shows the same thing at the same time — pure white that smoothly fades up from black to full brightness and back down again, over and over. The fade follows a smooth sinusoid-like ease (a "wave" shaper applied to a repeating time ramp), so it feels like gentle breathing rather than a linear ramp. One full breathe cycle takes roughly half a minute.

## Algorithm
- Per frame: sample one built-in repeating time ramp (period on the order of half a minute) and pass it through a smooth 0-to-1-to-0 wave shaper to get a global brightness value. (The original also created a second, faster time ramp that is never used — vestigial; omit it.)
- Per pixel: emit a color with zero saturation (i.e. white) at the global brightness. A hue value is nominally set but is irrelevant because saturation is zero.
- No state between frames beyond the built-in time ramps. No randomness. No layout assumptions — works on any pixel count, any geometry, since every pixel is identical.

## Colors
White only, from off through dim white to full white.

## Controls
None.
