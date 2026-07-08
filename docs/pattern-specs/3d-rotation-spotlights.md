# 3D Rotation / Spotlights
kind: 3D
sensors: no

## Overview
A 3D pattern (needs a mapped 3D installation; the author tuned it on a cube of six 8x8 matrices, a few hundred pixels) that renders a double cone — an hourglass shape, apex at the center of the volume, opening along one axis in both directions — and tumbles it continuously around a wandering rotation axis. On the surface of a walled cube it reads as two magenta-rimmed "spotlight" discs with hot white centers gliding and morphing across the faces, meeting at pinch points as the cone axis sweeps past.

## Per-frame work
- Three independent triangle waves with slightly different periods (each on the order of two to three seconds) each span roughly minus-one to plus-one; together they form a 3D vector that serves as the rotation axis. Because the periods differ, the axis wanders through direction space without repeating quickly (a Lissajous-like drift).
- A fourth, faster sawtooth (a bit over a second per cycle) scaled to a full turn gives the rotation angle about that axis.
- From axis + angle, a standard 3x3 axis-angle rotation matrix is built once per frame (normalize the axis to unit length first; then the classic Rodrigues/Wikipedia rotation-matrix formula with the usual precomputed sine, cosine, and one-minus-cosine products). Building it per frame rather than per pixel is the key performance trick — the author notes the matrix multiply alone halves the frame rate, so per-pixel matrix construction would be far worse.
- A single scalar "speed" constant divides all four clock periods; a "scale" constant (on the order of a tenth, expressed by the author as an inverse of pi squared) sets how wide the cones open.

## Per-pixel work
- Shift world coordinates so the origin is the center of the mapped volume (subtract a half from each axis).
- Multiply the position by the frame's rotation matrix (a helper returns the rotated triple through globals for speed in this language).
- Compute a signed "inside-ness": the absolute value of the rotated pixel's coordinate along the cone axis, minus the distance from that axis scaled by the opening factor (square root of the sum of the two transverse coordinates squared, each divided by the scale). Positive means inside the double cone; the magnitude is a rough distance from the cone surface in world units. Using different scales for the two transverse axes yields elliptical cones — a documented variation.
- Clamp that signed distance to plus/minus one (without the clamp the coloring math misbehaves far from the surface — the author invites experimenting).
- Color: fixed magenta hue. Saturation is one minus the inside-ness, so deep inside the cone the color washes out to white while the rim stays saturated magenta. Brightness is (one plus the signed distance) raised to the fourth power — outside pixels near the surface get a rapidly decaying glow, giving a smooth sub-pixel-feeling antialiased border, and pixels well outside go black.

## Controls
None exported; speed and cone width are source constants. A port could reasonably expose them as sliders.

## Timing feel
The tumble is lively — a full rotation in roughly a second — while the axis itself drifts over a few seconds, so the motion never looks like a fixed-axis spin. Author suggests running at reduced global brightness (about a quarter) because the white cores are intense.

## Non-obvious details
- The whole effect is "signed distance field of a double cone, rotated by a per-frame axis-angle matrix, shaded by distance": saturation and brightness are both functions of the same signed distance, one linear, one a steep power curve.
- Triangle waves (not sines) drive the axis components, so the axis lingers near the extremes slightly differently than a circular sweep would; the mismatch of the three periods is what keeps the choreography non-repeating over short spans.
