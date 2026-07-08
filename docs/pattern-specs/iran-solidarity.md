# Iran - Solidarity
kind: 3D
sensors: no

## What it looks like

A solidarity tribute rendered on a 3D-mapped installation treated as a cylinder: an Iranian-flag-style tricolor wraps around the vertical axis in three horizontal bands — green on top, white in the middle, red at the bottom — with the band boundaries gently undulating like waving cloth. A dark "void" or interference sector sweeps continuously around the cylinder, blacking out (with sparkly static) everything it covers; the void's angular width breathes wider and narrower on a slow sine. Opposite the void sits the "safe zone," and centered there on the white band floats a round (or stylized heart-shaped) emblem — the "lion heart" — which slowly pulses between visible and faded like a heartbeat. Optionally the void leaves a fading ghost-trail of the flag behind it as it rotates. The whole scene is heavily dimmed by default (a somber "gloom" master fade). Motion is slow and solemn: the void takes several seconds per revolution, the flag wave and the pulses cycle over a few seconds each.

## Geometry and layout assumptions

Requires a 3D map. The x/y plane is converted to an azimuth angle around the map's center (centered at the middle of the unit square); the z coordinate is height. Only 3D is implemented — no 1D/2D fallback. Works on anything cylinder-like (matrix wrapped into a tube, tree, globe).

## State kept between frames

Three independent phase accumulators (flag wave, void rotation, void breathing) each integrated from the frame delta times its own speed parameter — integration is skipped when the corresponding speed is zero, freezing that motion in place rather than snapping. A fourth accumulator drives the emblem's heartbeat (its own speed times a multiplier). Each frame also precomputes: the breathing void threshold and the azimuth of the safe-zone center (a quarter-turn behind the void angle, plus a user nudge), normalized into one full turn.

## Per-pixel rendering

1. **Waving flag:** the pixel's height is offset by a small sine of (twice its azimuth plus the wave phase) — amplitude about a tenth of the height — producing the two-lobed cloth ripple. The offset height is split into three bands via two smooth (smoothstep-style) transitions: below roughly a third = red, above roughly two-thirds = green, between = white. Band colors mix smoothly at the boundaries. The red is a strong pure red; the white is a slightly cool/blue-tinted bright grey; the green is a medium green with a hint of blue.
2. **Emblem:** only where the pixel is solidly in the white band. Compute the pixel's angular distance from the safe-zone center via the dot product of the two unit direction vectors (arc-cos), scaled to act as a horizontal distance, and the offset height's distance from mid-height as vertical. In circle mode the distance is the plain euclidean radius; in heart mode the vertical is shifted downward by an amount proportional to the square root of the absolute horizontal distance (the classic heart-curve trick) and the result slightly shrunk. A soft-edged mask (narrow smooth transition around the size threshold) defines the emblem. Its color interpolates, via a "morph" parameter, between near-black (a dark void emblem) and a golden yellow (a sun emblem); the emblem's opacity throbs with the heartbeat sine, fading down by a configurable depth, and the masked color is alpha-blended over the flag.
3. **Void and trail:** a sine of (azimuth minus the void's rotation angle) is compared to the breathing threshold; pixels beyond it are inside the void sector. Inside the void: if trails are enabled and the pixel is on the trailing side of the safe-zone center (which side depends on rotation direction; signed angular difference normalized to plus/minus a half-turn), and within the trail's angular length, the flag colors are multiplied by a fade that is one at the leading edge and zero at the trail's end, squared for a nicer falloff. Otherwise the pixel "glitches": with high probability (independent random draw per pixel per frame — this is what makes the void sparkle/crawl) it goes fully black; the remainder keep the flag color dimmed by a fixed factor.
4. **Gloom:** finally everything is multiplied by (one minus the gloom parameter), floored at zero. Default gloom is high, so the piece runs quite dark.

## Colors (qualitative)

- Tricolor: strong red / cool bright white / medium slightly-teal green, smoothly blended at wavy boundaries.
- Emblem: morphs from near-black to golden yellow.
- Void: black with sparse dim flickers of the underlying flag.

## Controls

No slider functions are exported; instead roughly a dozen named parameters are exported as live-settable variables (adjustable from the variable watcher or over the websocket API, defaults in source):

- flag wave speed; void rotation speed (sign sets direction); void breathing speed
- trail length (zero disables; up to about half a turn)
- gloom (master darkness, zero = bright, one = black)
- glitch density (probability a void pixel is black) and void dim level (brightness of the surviving void pixels)
- base clear-zone width and extra width added by breathing
- emblem: morph (dark-to-gold), size, manual angular nudge, shape selector (circle vs heart)
- heartbeat speed, a speed multiplier, and pulse depth (how far it fades)

## Non-obvious details

- The safe zone / emblem is *defined relative to the void* (a quarter-turn behind it), so the emblem rides around the cylinder just ahead of or behind the blackout — the void never scrubs the emblem out; it always sits in the surviving sector.
- Freezing any motion by zeroing its speed leaves its phase intact (accumulators only advance when speed is nonzero).
- The per-pixel random glitch is re-rolled every frame, giving the void an animated static texture with no stored state.
- The heart shape comes from warping the vertical distance by the square root of the horizontal distance before the radial test — a compact one-line heart.
