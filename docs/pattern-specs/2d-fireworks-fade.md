# 2D Fireworks Fade
kind: 2D (declared; most sub-modes only use the horizontal coordinate, so it behaves largely 1D)
sensors: no (but it reads the real-time clock and drives a GPIO output pin)

## Big picture
This is not one effect — it is a mini playlist engine cycling through six sub-effects, with a stochastic per-pixel dissolve between them. The overall theme is patriotic/holiday-display: red-white-blue sparkle, pulses, flag chases, plus two utilitarian "house lights" modes. It is clearly tuned for one specific physical installation (hardcoded segment boundary, GPIO relay, clock-gated hours); a faithful reimplementation should parameterize those.

## The playlist engine (the clever part)
- Each of the six modes owns a per-frame function and a per-pixel function, held in two parallel arrays. A master clock advances one mode every fifteen-or-so seconds; total cycle is that times six.
- The last roughly tenth of each mode's slot is a crossfade window. During it, a progress value ramps from zero to one. Per pixel, per frame: draw a random number and compare it against an eased (S-curve) version of the progress; the comparison decides whether this pixel is rendered by the outgoing mode or the incoming one. The result is a temporal dither/dissolve — pixels stochastically flip to the new effect, more of them as the fade progresses. No blending math, works between arbitrary effects.
- Every mode's per-frame function runs every frame regardless of which mode is showing (the author notes this could be optimized to just the two active ones). This matters: the particle mode's simulation keeps evolving even while hidden.
- Side channel: a specific GPIO output pin is set high only when the device clock reads within a particular two-hour late-evening window (roughly nine to eleven PM) and low otherwise — presumably switching a relay for other decorations. Optional/installation-specific; omit or make configurable.

## Mode 1 — sparkle dust
Sparse random twinkle: each frame, each pixel independently lights with a small probability (a couple percent); everything else is black. The color scheme steps through phases on a repeating cycle of a few seconds: first phase, hues in a tight band around red (jittered slightly to either side of the red point); second phase, a tight band of blues; third phase, pure white (saturation forced to zero); a final phase falls through without setting a hue — sloppy in the original (it reuses whatever hue was computed last), acceptable to treat as "hold previous scheme". In the colored phases about one pixel in ten is rendered white instead of colored. Uses the high-color-depth output call for smoother dim values.

## Mode 2 — pulsing plasma bands
A plasma-style effect: per pixel, brightness is a triangle wave of (a moving oscillator term + horizontal coordinate times coefficient A + vertical coordinate times coefficient B + coefficient C), where A, B, C are three slow sinusoids of different multi-second periods computed per frame. The brightness is raised to a high power (around fifth) to thin the bright bands and widen the dark gaps. Where brightness exceeds a high threshold the pixel desaturates to white. Hue creeps downward slowly and alternates between a red-family value and a blue-family value on a repeating cycle — so drifting diagonal bands of red and blue with white-hot cores sweep and tilt across the surface.

## Mode 3 — red/white/blue accent lights
Static architectural lighting: only every fourth pixel is lit, cycling red, white, blue, red, white, blue along the strip; all others are black. A hardcoded index boundary a few dozen pixels in shifts the every-fourth alignment by one for the remainder of the strip — pure adaptation to one specific installation's wiring; parameterize or drop. Implementation detail worth noting: it counts lit pixels via a variable reset when pixel index zero renders, which assumes pixels render in ascending index order — fine on stock hardware but fragile; a reimplementation can compute the color from the index arithmetic directly.

## Mode 4 — center-out sparks (the "fireworks")
A 1D particle simulation, persistent across frames:
- A pool of sparks, sized proportional to the strip (about one spark per six pixels). Each spark has a signed energy (sign = direction of travel) and a position in pixel units. A separate per-pixel heat buffer holds accumulated intensity.
- Initialization scatters sparks along the strip with energy decreasing with distance from center (as if already in flight), directions pointing away from center.
- Per frame: the heat buffer decays multiplicatively (frame-rate-compensated, capped so it never decays too slowly); each spark loses a little energy to friction (sign-preserving), moves by energy times elapsed time, and deposits its absolute energy as heat at its current pixel. A spark whose energy has decayed to near nothing, or that runs off either end, respawns at the strip's center with a fresh random energy (within a bounded range about half the maximum, random within roughly ±a third of that) and a random direction.
- Per pixel: brightness is the heat squared (gamma-ish). Color is by position: the left two-fifths of the strip renders in red, the middle fifth in white, the right two-fifths in blue; within the colored zones, saturation is reduced as heat rises so the hottest pixels flash white. The heat lookup converts the normalized horizontal coordinate back to a pixel index — i.e., it assumes x maps linearly along the strip.
- Net effect: a continuous fountain of bright sparks streaming from the center toward both ends, decaying to embers — reads as fireworks bursting from the middle, tinted as a flag triptych.

## Mode 5 — warm-white house lights
Static: every fourth pixel warm white (an incandescent-ish tint, noticeably warm, not amber) up to the same hardcoded boundary, then every seventh pixel after it; all else black. Two sliders exist ("skip" spacing and "start offset", each mapping to a small integer) but the renderer ignores them — vestigial; a reimplementation should either wire them up or drop them.

## Mode 6 — flag comet trio
Three single-pixel dots chase along the strip and wrap every several seconds: a blue leader, a white dot several pixels behind, a red dot the same distance behind that (overlaps sum, so coincident dots brighten/whiten). Everything else black. Position is computed from a shared clock times pixel count; the comparison lights a pixel when it is within one pixel of the dot's position.

## Controls
- Two sliders (spacing and offset for the house-lights mode) — nonfunctional as shipped, see Mode 5.
- No other UI. Mode duration and crossfade fraction are top-level constants worth promoting to sliders.

## Timing
Each mode holds for about fifteen seconds; full playlist about a minute and a half; dissolves last a second or two. Dust color phases: seconds each. Plasma drift: several-second sinusoids. Comet lap: about five seconds.

## Layout assumptions / fixes
- Hardcoded segment boundary index in modes 3 and 5 → expose as a parameter or derive from pixel count.
- Spark mode assumes normalized x is linear pixel position → fine for strips/matrix rows; for true 2D maps it should index by pixel index instead.
- GPIO pin number and clock window → configuration, not code.
