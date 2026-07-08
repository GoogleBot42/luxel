# Emoji Animation #2
kind: 2D
sensors: no

## What it looks like
On a square LED matrix you see a cartoon face. Two large, lively "googly eyes" (soft glowing oval outlines with a colored iris inside each) sit centered on the panel, drawn on top of a low-resolution pixel-art background that supplies the rest of the face: eyebrows, a small nose, and a mouth. The background art cycles through three hand-drawn frames a bit faster than once per second, so the eyebrows and mouth flip between expressions (a neutral magenta zig-zag smile, angled brows with a yellow smile, and angry zig-zag brows with a red frown), while the overlaid eyes independently blink and glance left and right at random moments. The overall feel is a face that keeps changing its mood while its eyes dart around and blink like a living creature.

## Algorithm

### Layered rendering (the clever part)
Each pixel is decided by a priority test in the per-pixel render step:
1. If the pixel falls inside an iris disc (and the eyes are not mid-blink), draw the iris.
2. Otherwise, if it falls inside the eye's oval, draw the grayscale eye outline.
3. Otherwise, sample the background bitmap for the current animation frame.
So the procedurally animated eyes are composited over a frame-flipping bitmap without any explicit blending — just an if/else chain.

### Background bitmap animation
- Three full-color bitmap frames are stored as literal nested arrays of red/green/blue byte triples, each frame a sixteen-by-sixteen grid.
- Persistent state: current frame index and an elapsed-time accumulator. Each frame-setup step adds the elapsed time; when the accumulator passes a threshold a bit under one second, it resets and the frame index advances cyclically through the three frames.
- Per pixel, the normalized panel coordinates are scaled by the grid size, floored, and clamped to look up the bitmap texel; the byte values are normalized to unit range and emitted as RGB.
- Frame art, qualitatively: frame one has two horizontal red eyebrow bars, a two-by-two red nose block, and a magenta zig-zag "W"-shaped mouth; frame two has red diagonal brows angled outward-down, the red nose, and a yellow smile (curved down at the edges, flat in the middle); frame three has red zig-zag/chevron "angry" brows, no separate nose row change, and a red frown arc. Everything else is black.

### Eyes
Geometry: normalized coordinates are re-centered so the panel middle is the origin. The horizontal coordinate is scaled by the number of eyes; with two eyes, each half of the panel is shifted so it gets its own local origin, producing two side-by-side eyes from one set of math. Eye shape is an ellipse noticeably wider than it is tall (roughly half the panel wide, a fifth tall at full open).

Persistent state between frames: a blink flag and blink-phase timer, a move flag and move-phase timer, the glance direction (left or right), the current iris horizontal offset, and the current eye height.

Blink logic (frame-setup step): when not blinking, the eye height stays at maximum; once the phase timer exceeds the blink interval (randomized after each blink to between about one and two seconds), a blink starts. During a blink, which lasts around half a second, the eye height is repeatedly multiplied by a smooth wave-shaped factor so the lids squeeze closed and reopen smoothly, clamped so the eye never quite reaches zero height. While blinking, the iris is not drawn at all — only the narrowed outline.

Glance logic (frame-setup step): when idle the iris is centered. After a randomized wait (a fraction of a second to about a second), a glance begins in a randomly chosen direction (fifty-fifty left or right). During the glance, lasting a fraction of a second, the iris offset follows a smooth sinusoidal sweep out and back, displacing the iris a modest fraction of the eye width. When the glance ends, the wait until the next one is re-randomized.

Iris drawing: inside a disc of a fixed radius around the (possibly offset) iris center, brightness rises with the square of the normalized distance from the iris center — darkest at the very center, brighter at the rim — giving a subtle shaded, dimensional look. Color is a fixed hue and saturation (default a vivid green).

Eye outline drawing: for pixels inside the eye ellipse, compute a normalized elliptical distance from the eye center; push it through a very steep power curve (roughly sixth power) so the interior of the eye is nearly black and brightness rises sharply toward the ellipse boundary. The result is emitted as grayscale, reading as a soft white oval ring. A small thickness parameter offsets/floors this distance to control how fat the glowing rim looks.

### Personality presets
A preset selector re-scales the base parameters once near startup: "sleepy" shrinks eye height, blinks more often, glances less; "alert" enlarges eye height, blinks rarely, glances often; "nervous" blinks and glances very frequently with quicker, jerkier moves. An "asymmetric eyes" switch makes the right eye slightly smaller (both iris and width) for a quirky look. Non-obvious quirk in the original: the preset multipliers are applied whenever both phase timers happen to be exactly zero, which is intended as "first frame only" but can in principle re-apply and compound; a reimplementation should apply the preset exactly once.

## Colors
- Background art: pure saturated red for brows/nose and most mouths; one frame's mouth is magenta, another's is yellow; background is black.
- Iris: a single vivid hue, default green, adjustable hue and saturation, shaded darker at its center.
- Eye outline: neutral grayscale (white glow on black).

## Parameters / UI
All the tunables are exposed as exported variables rather than registered slider controls, so in the original there is no slider UI — they are edit-in-code or watch-list values. A reimplementation could reasonably promote them to sliders. Conceptually: eye count (one or two), maximum eye width, maximum eye height, iris radius, outline thickness, iris hue, iris saturation, blink interval, blink duration, glance interval, glance duration, personality preset (four modes), and asymmetric-eyes switch.

## Timing
Background expression flips a bit faster than once per second. Blinks happen every one-to-two seconds and take about half a second. Glances happen roughly every second and take a fraction of a second. All timers accumulate real elapsed time, so behavior is frame-rate independent.

## Layout assumptions
The background bitmap is hardcoded to a sixteen-by-sixteen grid and the art assumes a square panel with normalized 2D coordinates; on other resolutions the bitmap is nearest-neighbor sampled (it still works, just blocky or cropped in aspect). Obvious fix: derive nothing from pixel count (it already samples by normalized coordinates — which is why it tolerates other matrix sizes), but keep the grid-size constant in one place so different art sizes can be swapped in. The bitmap row index is taken directly from the vertical coordinate, so whether the art appears upside down depends on the panel's coordinate convention; flip the vertical lookup if needed.
