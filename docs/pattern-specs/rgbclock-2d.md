# RGBclock 2D
kind: 2D
sensors: no (uses the device's wall-clock time — hour/minute/second — not the sound or motion sensor board)

## Visual behavior
An analog clock face rendered on a matrix. Three hands sweep around the center: **red for seconds, green for minutes, blue for hours**, plus an optional dim-to-bright **white sub-second hand** whipping around once per second. Hands are drawn as glowing angular gradients rather than thin lines, and where they overlap the pure RGB channels add, producing yellow/cyan/magenta blends. All hands move **smoothly and continuously** (the minute and hour hands creep, the second hand sweeps rather than ticks). Depending on mode, hands appear as full rays, concentric ring segments, or pulses that ripple outward, and the whole face can "breathe" in sharpness over a period from about a second up to about a minute.

## Time handling (per frame)
- Read the device clock's second-of-minute. Maintain a fractional-second accumulator advanced by the frame delta; whenever the whole-second value changes, reset the fraction to roughly half a frame (jitter compensation) and latch the new second.
- Smooth second = latched second + fraction. Smooth minute = clock minute + smooth second scaled into it. Smooth hour = clock hour modulo twelve + smooth minute scaled into it. Each hand's angle is its smooth value normalized to one full turn (seconds and minutes over sixty, hours over twelve).
- Also advance a looping animation phase whose period is set by the speed slider (about one second at one extreme to about a minute at the other); this drives the breathing and the animated hand modes.
- Effective sharpness = base sharpness plus a smooth oscillation (sine-like wave of the animation phase, centered) scaled by the breathe amount.

## Per-pixel render
Convert the pixel to polar coordinates about the display center: an angle normalized to the unit interval with zero at twelve o'clock, and a radius. (The original hand-rolls its arctangent because of a firmware bug of that era; a reimplementation can use a standard two-argument arctangent.)

Two independent mode selectors shape the drawing; each pixel first runs the selected **radial mode**, then the selected **hand mode**.

### Radial modes (four, chosen by slider) — produce a per-hand radial intensity filter
1. **Equidistant rings**: each hand gets its own repeating triangle-wave ring pattern in radius, phase-offset per hand so the hands live on interleaved concentric arcs; slightly over-driven then clamped to full so ring crests are solid. The zoom control multiplies the radius, packing in more rings.
2. **Clustered rings**: similar but each hand's rings repeat at a different spatial frequency, blending more; the hour hand instead gets a filter that is strong at the center and fades out by mid-radius, and the sub-second hand is confined to a narrow band near the center.
3. **Hard bands**: each hand is a solid annulus (on/off by radius window) — sub-second and hour near the center, minute a bit further out, second further still. Combined with the thresholded hand mode this gives a chunky, pixel-art look.
4. **Rays**: no radial shaping at all; every hand runs from center to edge.

### Hand modes (four, chosen by slider) — turn angle + radial filter into channel intensities
For all modes the core measure is an **angular triangle-distance**: a triangle wave of (pixel angle + hand angle) peaking when the pixel sits opposite/on the hand. The base gradient is: (a strength constant slightly above one, minus that triangle value), multiplied by the hand's radial filter, then raised to the **effective sharpness power** — high powers give crisp thin hands, low powers give wide soft glows. The strength slider lifts the constant so more of the face clears the threshold before the power is applied.
1. **Gradient**: each channel (white/red/green/blue for sub-second/second/minute/hour) is its gradient value directly; the white sub-second value (clamped to full, scaled by the sub-second brightness control) is added to all three channels.
2. **Threshold**: same gradients but binarized at the halfway point — hard-edged hands.
3. **Pulse**: the gradient gets an added ripple term — a triangle wave of (angular distance minus the animation phase) at modest amplitude — so waves of brightness roll outward along each hand as the phase cycles.
4. **Beam shot**: the gradient is instead *multiplied* by a doubly-folded triangle of (angular distance minus the animation phase), making discrete bright packets shoot from the hand outward each cycle; the sub-second hand stays thresholded.

## Colors
Strictly the three primaries, one per hand, plus additive white for the sub-second hand. All blending is additive in RGB, so overlaps produce secondary colors naturally.

## Controls (all sliders)
- **Radius mode**: selects among the four radial modes (slider quantized to four positions).
- **Hand mode**: selects among the four hand-drawing modes (quantized to four positions).
- **Sharpness**: hand crispness; response is squared so the low end is fine-grained, with the maximum a large power (a few dozen).
- **Strength**: intensity/width boost of the hands (squared response, up to about doubling the base constant).
- **Breathe**: amplitude of the sharpness oscillation; leftmost disables it.
- **Speed**: animation period, from about once a second to about once a minute (inverted: right = slower).
- **Distance/zoom**: radius multiplier, from unity up to several times — zooms the ring structure.
- **Sub-second brightness**: white hand from fully off to bright (squared response).

## Non-obvious points
- Pure per-channel math means no HSV work at all; the "clock" is three monochrome layers composited by addition.
- The jitter-compensated fractional second (resetting to half a frame at each second boundary) is what makes the sweep hand look fluid instead of stuttering at second changes.
- Raising the clamped gradient to a variable power is the single trick behind both soft-glow and needle-thin looks.
- Requires the device to actually know the wall-clock time (e.g. via network time sync); with no time source the face still renders but the hands are meaningless.
