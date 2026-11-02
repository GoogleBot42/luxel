# Hatched Warp

Idea spec written from Patternflow preset preset_0510.h (CC BY-SA 4.0, Seunghun Lee); prose only, no code — the Luxel implementation is an original work built from this description.

## What it looks like

A slowly churning plasma that is never shown as a gradient. Instead, the smooth field is cut into four tiers, and each tier gets its own *texture* rather than its own shade: the lowest tier is a solid block of the base hue, the next is pure black, the third is a sparse grid of isolated single dots in a second hue, and the top tier is a fine diagonal hatch of stripes in a third hue — the three hues spaced evenly around the wheel. The result reads like a screen-printed or risograph poster in motion — flat colour, empty space, halftone dots and engraving lines, all flowing as blobby islands that stretch and wobble across the whole 128x64 panel. Because the black tier always separates the solid tier from the textured ones, the islands have crisp outlines. The textures are locked to the pixel grid while the islands move over them, so the motion reads as a mask sliding over fixed print screens.

Compelling as a graphic-design look; the textures are one pixel fine, so on a 64x64 panel at a distance the dot tier and hatch tier read as dimmer versions of their hue, and the eye mostly sees "four-colour plasma".

## The idea

Quantize a domain-warped sine plasma into bands and render each band as a different print texture (solid, empty, stipple, hatch) instead of a different colour level. It works because texture is a second visual channel that survives on a coarse LED grid — it gives the flat-colour look depth without needing gradients.

## Algorithm in words

- State: one time accumulator advanced by the speed knob.
- Per pixel: scale the coordinates by the zoom knob. Build a horizontal sine of x and a vertical cosine of y, each with its phase pushed sideways by a sine/cosine of the *other* axis (the cross-warp, scaled by the distortion knob), and with the y term also scrolling over time. Sum the two into a value symmetric about zero and normalize it to 0..1.
- Tier the normalized value into four bands of uneven width (a fairly wide bottom band, a narrow black gap, a middle band, a top band).
- Bottom band: fill with the base hue, fully saturated and bright.
- Black band: leave dark.
- Middle band: light only pixels whose x and y both fall on a sparse lattice (every few pixels in each direction) in the second hue.
- Top band: light pixels on alternating diagonal stripes a couple of pixels wide in the third hue.
- No smoothing, no anti-aliasing, no audio.

## Knobs

- K1 — base hue; the other two hues follow at even spacing around the wheel.
- K2 — animation speed, from frozen to fast.
- K3 — zoom: small values give a few huge islands, large values a busy fine pattern.
- K4 — warp amount: zero gives a plain plaid of crossed sine bands; turning it up makes the islands swirl and pinch into organic shapes.
- No audio response.

## Making it better in Luxel

- Use `simplex3` (or a two-octave domain warp with `curl2`) instead of crossed sines so the islands drift organically rather than in plaid-shaped grid motion; one noise sample per pixel is affordable at 4096 px.
- Let the texture scale with the panel: dot spacing and hatch period as a control in real pixels, so a 128x128 chain can use larger, legible screens. Optionally rotate the hatch angle slowly so the print screens themselves move.
- Pick the three hues in OKLCH (equal lightness, spread chroma) so solid, dot and hatch tiers have balanced weight; offer a "riso" palette preset (fluorescent pink / teal / yellow on black).
- A smoothstep band edge with a one-pixel dither would make tier boundaries less jaggy without losing the flat look.
- Best form: a single 2D pattern. It does not suit strips — the textures need two dimensions (a 1D variant could use dash patterns instead, but it loses the point).

## Payoff

Score: 3/5 — a distinctive print-poster aesthetic worth having once the field is upgraded to noise; the crossed-sine base is plain.
