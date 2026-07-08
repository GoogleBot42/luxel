# Nyan Lights
kind: 1D (hand-rolled 2D matrix on a serpentine strip)
sensors: no

## What it looks like

Low-res pixel art of the "Nyan Cat" meme on an LED matrix: a rectangular
pop-tart with a golden-brown crust outline and pink frosting dotted with
darker sprinkles, a gray cat head (ears, cheeks, dark bead eyes) overlapping
one side of the tart, with gray paws and a tail. Trailing across the rest of
the panel is the signature rainbow — horizontal bands of red through violet —
which waves up and down. The whole thing runs a two-frame animation flipping
several times per second: the rainbow's column groups bob one row up/down in
alternation and the cat sprite jiggles one pixel sideways, giving the classic
GIF wiggle.

## Algorithm

Layout assumption (hardcoded): a matrix on the order of thirty columns by ten
rows (a few hundred pixels), wired serpentine — even rows run one direction,
odd rows reversed. The renderer converts the strip index to (column, row) by
integer division/modulo against the hardcoded width and un-mirrors odd rows.
The sprite buffers are also sized to that hardcoded pixel count. Obvious fix:
implement as a true 2D renderer using the device's pixel mapper (which
already handles serpentine wiring) and derive dimensions from the map, or at
minimum expose the width as a single parameter.

Static setup (runs once): three parallel arrays — hue, saturation, brightness
— one entry per logical pixel, all initially zero. A small helper stamps a
named color (hue+saturation pair, optional brightness, defaulting to full)
into a given logical index; every call site also applies a small constant
index offset (nudging the whole sprite a couple of pixels — an artifact of
how the art was plotted, not meaningful). The sprite is hand-placed pixel by
pixel:

- Crust: the top edge, both vertical edges, and bottom edge of a rectangle
  roughly eight columns wide and eight rows tall in the left-middle of the
  panel.
- Frosting fill: the rectangle's interior in pink, with darker "berry"
  sprinkles at every other column on every other row (a sparse checker).
- Cat: dim gray pixels forming a head to the right of the tart — ears on
  top, cheek outline, a mouth line — plus paws below and a short tail at the
  head's left; two very dim near-black pixels for bead eyes; two pink cheek
  dots.

Per frame: accumulate elapsed time; every fifth-of-a-second-ish, toggle a
boolean "shifted" flag (two-frame animation).

Per pixel (render):

1. Convert index → (column, row) with serpentine un-mirroring, then back to a
   logical row-major index.
2. If the sprite hue at that index is nonzero, the pixel belongs to the
   sprite: on alternate frames read the entry one position over instead
   (this is what makes the sprite jiggle), and paint that stored HSV if it
   too is nonzero.
3. Otherwise, if the column is past roughly the first third of the panel,
   the pixel may be rainbow: columns are grouped in runs of about four, and
   alternate groups use the animation flag directly vs. inverted. A group's
   band occupies about six consecutive rows starting a couple rows from the
   top, offset down one row depending on its (possibly inverted) flag —
   adjacent column groups therefore bob out of phase, producing the wave.
   The row within the band (adjusted by the same offset) indexes a small
   lookup of rainbow hues, drawn at full saturation and brightness.
4. Anything else is black.

No randomness. State between frames: just the millisecond accumulator and
the flip flag.

## Colors

- Crust: warm golden orange-brown, fully saturated.
- Frosting: light pink (a red-family hue at roughly half saturation).
- Berry sprinkles: deeper pink (same hue family, high saturation).
- Cat body/outline: dim, nearly desaturated warm gray.
- Eyes: same gray family, very dim — reads as near-black beads.
- Rainbow bands, top to bottom: red, orange/amber, yellow-green, green,
  cyan-leaning blue, violet.

## Controls

None.

## Timing

Two frames alternating several times per second — a deliberately choppy,
GIF-like cadence rather than smooth motion.

## Notes / quirks

- Transparency sentinel: "hue is exactly zero" means "no sprite here". Since
  zero is also true red on the hue wheel, red sprite pixels are impossible;
  the art dodges this by using a hue of exactly one full turn (numerically
  nonzero, visually red) for the pinks. An implementer should use an explicit
  transparency flag instead.
- On alternate frames the sprite lookup reads the neighboring entry and only
  paints if that neighbor is opaque; sprite-edge pixels can end up not
  written at all that frame, relying on the previous frame's value
  persisting. Treat "transparent after shift" as black (or fall through to
  the rainbow/black logic) for cleaner behavior.
- Leftover unused state in the source (a thrust-width value, a frame
  counter, a commented-out width toggle) has no visual effect; ignore it.
