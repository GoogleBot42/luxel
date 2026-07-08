# Tunnel of Squares 2D
kind: 2D
sensors: no

## What it looks like

An endless "tunnel" of concentric squares centered on the middle of the display, rushing continuously toward (or away from) the viewer as if flying down a square-cross-section corridor. The squares are not perfectly axis-aligned rings — they carry a slight spiral twist, so the whole tunnel appears to corkscrew as it flows. Rings near the center are packed tightly together and spread out exponentially toward the edges, which is what sells the perspective illusion. Colors cycle slowly through the whole rainbow, and hue also varies with distance from center so each ring band sits at a different point on the color wheel. The original author notes that staring at it produces a motion-aftereffect optical illusion. Motion is brisk at default speed; a full hue drift cycle takes several seconds.

## Coordinate frame

Pixel coordinates are shifted so the origin sits at the center of the mapped area (i.e. the standard unit-square map is translated by half in each axis before rendering). Everything below is in these centered coordinates.

## Algorithm

State between frames: a running elapsed-time accumulator (seconds), advanced by the frame delta and wrapped after roughly an hour to avoid losing floating-point precision. Two per-frame time values are derived:

- a fast animation phase = the accumulator multiplied by the user speed setting (drives ring motion),
- a slow sawtooth phase from the engine's global time utility, cycling over several seconds (drives the overall hue drift).

Per pixel, each frame:

1. Compute a **square-ish radial metric**. Conceptually this is the "diamond norm" |x| + |y|, but built as a dot product of the pixel position with the *sign vector* of the position (a vector whose components are +1, −1, or 0 matching the signs of x and y). Before taking the dot product, that sign vector is rotated by a small fixed angle (on the order of a tenth of a radian — precomputed once at startup as a cosine/sine pair). This rotation is what warps the rings from clean diamonds/squares into subtly twisted, spiral-looking squares, with the twist differing per quadrant.
2. Take the **logarithm** of that metric. Log-spacing makes equal ring counts occupy exponentially growing radii — the tunnel-perspective trick (adapted, per the source comments, from a public shadertoy "squared spiral" idea).
3. Form a phase = (ring-count setting) × (log metric) + (the pixel's polar angle about the center) − (the fast animation phase). Adding the polar angle turns the concentric rings into a continuous spiral; subtracting time makes it flow.
4. Brightness = the absolute value of the sine of that phase, then **cubed**. Cubing narrows the bright bands and deepens the dark gaps, giving crisp ring edges instead of soft sinusoidal blur.
5. Hue = the slow drifting phase plus the (un-logged) square-radial metric, at full saturation. So hue sweeps radially outward and the whole palette rotates slowly.

No randomness. No layout hardcoding: it is resolution-independent and works on any 2D map (pixel index is unused). Pixels exactly at the center (metric ~0) hit the log's singularity; the sine of a huge negative number just yields noise-like flicker at the very center pixel, which reads fine visually, but an implementer may clamp the metric to a small floor if their math library objects.

## Colors

Full rainbow, fully saturated. Hue is a function of radial distance (square-norm) plus a slow global drift, so at any instant the display shows a radial rainbow sweep; over several seconds the whole thing rotates through the color wheel. Brightness bands run from black gaps to full-brightness rings.

## Controls

- **Speed** (slider): scales the ring-flow rate roughly tenfold from the slider's bottom to top, never fully stopping (minimum is a slow crawl, maximum is a fast rush). Default sits mid-high.
- **"Squarocity" / ring density** (slider): sets the integer number of square rings per log-octave, from one up to about seven. Low values give a few broad rings; high values give many tight rings. Default is a handful.

## Timing

Ring motion speed is user-controlled; at defaults, rings visibly stream past several times per second. Hue drift completes a cycle in a handful of seconds. The time accumulator wraps after about an hour (invisible in practice).
