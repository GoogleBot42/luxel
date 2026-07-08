# Sunrise Alarm Clock
kind: 1D
sensors: no (uses the real-time clock — hour/minute/second/weekday — not the sensor board)

## What it looks like
The strip is a horizontal slice of sky playing out a whole day. Before the configured
wake-up window it is completely dark. About half an hour before wake time, a sunrise
begins: dawn colors (by default deep red on the "east" end blending to dark blue on the
"west" end) bloom out of black, then over the next half hour resolve into a daytime sky
(default: soft light blue). Through the day a small bright "sun" — a patch a couple of
pixels wide — travels slowly from one end of the strip to the other, its position
proportional to how far the day has progressed between wake and sleep time. Drifting
cloud patches (default mid-gray) wander across the sky the whole time. Starting half an
hour before the configured sleep time the sequence runs in reverse as a sunset, ending
in black. The strip is also gently vignetted (dimmer at both ends) and gamma-shaped.

On startup / after saving, it plays a "preview day": simulated time starts just before
wake time and runs at roughly one simulated hour per real second, so you watch a full
sunrise-day-sunset in well under a minute; after the simulated day passes midnight it
switches to tracking the real clock.

## Algorithm
State between frames:
- A "current time of day in hours" value (fractional). In demo mode it is advanced by
  the frame delta so one real second equals one simulated hour; in normal mode it is
  rebuilt each frame from the clock's hour/minute/second plus a locally tracked
  millisecond estimate.
- Millisecond tracking (clever bit): the platform clock only exposes whole seconds, so
  the pattern accumulates frame deltas into a millisecond counter and re-anchors it
  every time it observes the clock's seconds value roll over. This gives smooth
  sub-second animation from a coarse clock.
- Two small palettes, each holding four colors, rebuilt every frame (see below), plus a
  phase-progress value in the range zero to one.

Per frame, the day is classified into phases by comparing the time of day against the
effective wake time (the weekend wake time is substituted on Saturday/Sunday, except in
demo mode) and the sleep time. Each sunrise/sunset half-phase lasts a fixed half hour:
1. First half of sunrise (the half hour before wake): sky blends from all-black toward
   a dawn gradient (east dawn color on one end, west dawn color on the other); clouds
   blend from black toward being tinted by the east dawn color.
2. Second half of sunrise (half hour after wake): dawn gradient blends into uniform
   noon-sky color; clouds blend from dawn-tinted to their daytime color.
3. Day: everything holds at noon sky / noon clouds; the phase-progress value instead
   measures the fraction of the day elapsed.
4. First and second halves of sunset: exact mirror of the sunrise phases, with the
   east/west dawn colors swapped end-for-end so the sunset glows on the opposite side.
Outside the wake-to-sleep window (plus the half-hour shoulders) every pixel is black.

Per pixel:
- Sky color = bilinear blend of the four palette entries, using normalized position
  along the strip on one axis and phase progress on the other. Same for the cloud color
  from its palette.
- Sun: the sun's start/stop positions (in pixels) are computed per frame from the
  day-fraction; before wake and after sleep they are pushed off the ends. A pixel
  inside the sun span gets the sun color added on top at roughly double strength; the
  two edge pixels get a fractional (anti-aliased) share so the sun moves smoothly
  rather than jumping pixel to pixel.
- Clouds: a perlin-noise field sampled at (position scaled up by about an order of
  magnitude plus time-of-day drift, time-of-day) gives a cloudiness amount, biased to
  be centered around half, scaled by the cloudiness slider, and clamped to zero-one.
  The sky color is blended toward the cloud color by that amount. The noise drift rate
  is boosted heavily in real-clock mode (since real hours advance slowly) and kept
  modest in demo mode.
- Vignette: brightness envelope = a half-sine hump across the strip (zero-ish at the
  ends, full in the middle) raised to a slider-controlled power.
- Gamma: each color channel is raised to a slider-controlled exponent (between one and
  two) before output, then multiplied by the vignette envelope.

Layout assumptions: fully proportional to pixel count except the sun width, which is a
fixed couple of pixels regardless of strip length — the obvious fix is to make sun
width a fraction of the pixel count.

## Colors (defaults, all user-configurable)
- East dawn: deep red. West dawn: dark navy blue.
- Noon sky: soft light blue (the picker value is internally halved so it stays muted).
- Sun: warm near-white yellow, added additively so it blooms over the sky.
- Clouds: mid gray (also internally halved).
- Off state and pre-dawn base: pure black.

## Controls
- Color picker "east dawn color" — sunrise/sunset glow on one end.
- Color picker "west dawn color" — the opposite end's dawn tint.
- Number input "wake time" — decimal hours (e.g. half past six = six point five);
  setting it in demo mode also rewinds the preview to just before sunrise.
- Number input "weekend wake time" — used instead on Saturday/Sunday.
- Number input "sleep time" — decimal hours on a 24-hour scale.
- Color picker "noon sky color".
- Color picker "sun color".
- Color picker "cloud color".
- Slider "cloudiness" — zero hides clouds entirely; higher values make cloud cover
  denser and more frequent.
- Slider "vignette" — how strongly the strip ends fade to dark.
- Slider "gamma" — perceptual brightness curve; higher values deepen dim colors.

## Timing feel
Real deployment: phases track wall-clock hours (half-hour sunrise halves, hours-long
day). Demo: the entire day compresses to under a minute. Clouds drift visibly over
seconds; the sun creeps imperceptibly in real mode.
