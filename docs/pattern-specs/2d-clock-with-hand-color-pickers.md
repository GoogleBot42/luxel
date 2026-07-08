# 2d Clock with Hand Color Pickers
kind: 2D
sensors: no (but requires the device's real-time wall clock to be set/synced)

## What it looks like
An analog clock face rendered on a 2D-mapped panel. Three "hands" — hour, minute, and second — appear as colored wedges/rays radiating from the center of the mapped area, each pointing at the correct clock position (12 o'clock is straight up). The background is black. Each hand has its own user-picked color; the defaults follow the classic scheme: second hand red, minute hand green, hour hand blue.

The minute and hour hands sweep smoothly (they include the fractional progress of the units below them, e.g. at half past the hour the hour hand sits halfway between hour marks). The second hand ticks in discrete one-second jumps.

Depending on a "radius mode" selector, the hands are drawn either as full rays from center to edge, as concentric ring/arc patterns (so a hand appears as a series of arcs at its angle rather than a solid ray), or as hard annular bands (giving a chunky, pixelated-ring look). A "breathe" option makes the hands' edge definition slowly pulse softer and sharper.

## Algorithm
Layout assumptions: needs a 2D pixel map with coordinates normalized to a unit square. All geometry is computed relative to the square's center (coordinates shifted by half in each axis). Works on matrices, rings, or any mapped shape; the clock is centered on the map's center.

Per frame:
- Read the real-time clock's hour, minute, and second.
- Maintain a sub-second accumulator: add the frame delta each frame; when the whole-second value changes, reset the accumulator to roughly half a frame's worth (a jitter guess). In this variant the accumulator is actually unused for display — the second hand deliberately uses the whole-second value so it jumps. (A commented-out smooth-seconds option existed in the original; the accumulator machinery supports it.)
- Compute fractional time values: minutes = whole minutes plus seconds as a fraction of a minute; hours = whole hours on a 12-hour dial plus minutes as a fraction of an hour.
- Compute the effective edge-sharpness for this frame: a base sharpness plus an optional slow oscillation (the "breathe" effect), whose rate is set by the speed slider.

Per pixel:
1. Center the coordinates, then compute the pixel's angle around the center — measured so that the zero/one seam of the normalized angle is at 12 o'clock — and its radius (distance from center). (The original had to hand-roll a quadrant-aware two-argument arctangent because the firmware builtin was buggy at the time; a correct atan2 builtin is fine.)
2. Run the selected radius mode, which produces one radial-intensity value per hand (how strongly a hand may appear at this radius):
   - Mode A ("equidistant rings"): each hand's radial intensity is a repeating triangle-wave gradient of radius, with a different phase offset per hand, slightly overdriven and clamped to full — soft repeating concentric rings, evenly spread.
   - Mode B ("clustered"): similar triangle-wave rings but at different spatial frequencies per hand so they cluster and blend more; the hour hand instead gets a solid center disc that fades out with radius.
   - Mode C ("bands"): hard on/off annuli — each hand is confined to a thin ring at a characteristic radius (hour innermost, minute in the middle, second outermost-ish). Combined with the thresholded hand drawing this gives a blocky, pixelated dial.
   - Mode D ("rays"): radial intensity is full everywhere — hands are solid rays to the edge.
   (All modes also compute a value for a vestigial "sub-seconds" hand inherited from the pattern this was adapted from; it is never displayed and can be dropped.)
3. For each hand, compute an angular intensity: take the triangle-wave distance between the pixel's angle and the hand's angle (hand angle = its time value as a fraction of a full revolution), subtract it from a strength factor slightly above one, multiply by the hand's radial intensity, then raise the result to the (large) sharpness power. High sharpness makes a narrow, hard-edged wedge; low sharpness makes broad soft lobes.
4. Threshold each hand's intensity at one-half to a hard on/off decision. Paint the pixel with the first hand that passes, in priority order hour, then minute, then second, using that hand's picked color. If no hand passes, the pixel is black.

State between frames: the last-seen whole second and the sub-second accumulator. No randomness. The distance/zoom factor scales radius before the ring modes use it, zooming the concentric structures in and out (in "rays" mode zoom has no visible effect).

## Colors
Three solid, user-chosen colors, one per hand, on a black background. Defaults: red seconds, green minutes, blue hours. Hands are hard-edged (thresholded), so there is no gradient within a hand — anti-aliasing comes only from how narrow the wedge is relative to pixel spacing.

## Controls
- Color picker "hour hand color" — solid color of the hour hand (default blue).
- Color picker "minute hand color" — solid color of the minute hand (default green).
- Color picker "second hand color" — solid color of the second hand (default red).
- Slider "radius mode" — selects one of the four radial modes above (quantized to four steps: equidistant rings / clustered / bands / rays).
- Slider "sharpness" — hand edge definition, from soft broad lobes to razor-thin wedges (response is squared so most of the travel is at the soft end; maps to a power from about unity up to several dozen).
- Slider "strength" — the overdrive factor in the angular gradient, effectively widening/intensifying the hands (from about one up to about two, squared response).
- Slider "breathe" — amplitude of the slow sharpness oscillation; fully left disables it.
- Slider "speed" — period of the breathing oscillation, from around a second at one end to around a minute at the other.
- Slider "distance" — zoom on the clock face, scaling the radial ring/band structures from natural size up to several times denser.

## Non-obvious notes
- The hard threshold plus priority ordering means hands occlude each other cleanly (hour over minute over second) instead of additively blending — that is what lets arbitrary picked colors stay pure where hands cross.
- The angle must be measured from straight up with the wrap seam at 12 o'clock, or every hand is rotated wrong; this is done by swapping the roles of the two axes in the arctangent and offsetting by half a turn before normalizing.
