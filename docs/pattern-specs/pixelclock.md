# pixelClock
kind: 1D
sensors: no (uses the device's real-time wall clock, not the sensor board)

## What it looks like
An analog clock rendered on a strip or ring: three colored blocks sit on a very dim
near-white background and tick around the layout — one block for the seconds (jumping
once per second), one for the minutes, and one wider block for the hours. Best on a
circular ring of pixels, like a wall-clock face.

## Algorithm
Stateless; everything is computed per pixel from the wall clock.

At startup the strip is divided two ways:
- into sixty equal regions (for seconds and minutes), and
- into twelve equal regions (for hours),
in both cases using the integer floor of pixel count divided by the region count as the
region width.

Per pixel, determine which sixty-division region and which twelve-division region the
pixel falls in, then pick the first matching rule:

1. If the pixel's sixty-division region equals the current wall-clock second: draw the
   seconds color.
2. Else if it equals the current wall-clock minute: draw the minutes color.
3. Else if the pixel's twelve-division region equals the current hour (wrapped to a
   12-hour face): draw the hours color.
4. Otherwise: a very faint, colorless glow (a few percent brightness) as the clock-face
   background.

So seconds occlude minutes, and minutes occlude the hour block where they overlap.

## Colors
Despite source comments claiming "white", the actual hands are colored:
- Seconds: a saturated blue at roughly half brightness.
- Minutes: a saturated green at full brightness.
- Hours: a warm red, slightly desaturated (a touch pink), at full brightness.
- Background: extremely dim neutral white.

## Controls
None. Requires the device's clock to be set (e.g. via network time); with an unset
clock the hands sit at whatever the default epoch time is.

## Timing
The seconds block steps once per second around the layout; minutes and hours move as
real time does.

## Layout assumptions (and fix)
The sixty and twelve region counts are hardcoded, and region width uses an integer
floor, so:
- Pixel counts that aren't a multiple of sixty leave trailing pixels whose region index
  exceeds all clock values — they permanently show only the dim background.
- Fewer than sixty pixels makes the region width floor to zero (division-by-zero /
  degenerate behavior).

Obvious fix: map clock values to pixel positions proportionally (pixel's normalized
position times sixty, or times twelve) instead of fixed-width integer regions, which
handles any pixel count and distributes remainder pixels evenly.

This is a simple pattern; nothing else to it.
