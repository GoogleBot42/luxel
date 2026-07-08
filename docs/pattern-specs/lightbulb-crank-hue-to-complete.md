# Lightbulb - Crank Hue to Complete
kind: 1D (uniform — every pixel shows the same color, so it works on any geometry)
sensors: yes (a digital GPIO input from a mechanical crank switch; NOT sound)

## Context

An interactive museum/exhibit-style piece: a giant lightbulb prop with two LED "filament coils" (upper and lower), each driven by its own controller running this same pattern. The controller's node-ID setting (one device set to the first ID, the other to the second) selects which row of the color tables it uses. Kids turn a hand crank; a switch closes once per full crank rotation, and cranking drives the bulb through the rainbow to a celebratory finish.

## What it looks like

- **Resting / attract state:** the upper coil glows golden yellow, the lower coil a lighter, partially desaturated warm white. Both slowly pulse in brightness — a gentle few-second breathing oscillation between half and full — as an invitation to interact.
- **Cranking:** on the first crank pulse, both coils snap to solid full-brightness red. Every several crank turns (five, per the design brief embedded in the pattern) the color steps to the next rainbow stop: red, orange, yellow, green, blue, purple. Colors are steady (no pulsing) while counting cranks.
- **Completion:** after the required turns on the final color (purple), both coils flash in their original resting colors a handful of times — each flash is a sharp pop that decays over a fraction of a second (a downward ramp, squared, so it fades fast then trails) — then the piece returns to the resting state.
- **Give-up handling:** if a participant walks away mid-sequence, roughly ten seconds of inactivity in any non-resting state resets everything to the resting state.

## Algorithm

**Rendering is trivial.** The per-pixel function ignores the pixel index entirely: it looks up hue and saturation from two small tables indexed by (node ID, current state) and gets brightness by calling a per-state brightness function from a parallel table (slow pulse for resting, constant-full for the color states, decaying flash for success). All the interesting behavior is in the per-frame state machine.

**State kept between frames:** current state index; time spent in the current state (accumulated from per-frame elapsed milliseconds, converted to seconds, wrapped every hour as a numeric-range guard); a count of sensor activations within the current state; and the previous frame's sensor reading (for edge detection).

**States:** a resting state, six rainbow color states, and a success state — the tables' length defines the count, and a load-time assertion deliberately crashes the pattern if the hue/saturation/brightness-function tables ever disagree in length.

**Per frame:**

1. Accumulate state time.
2. Configure the sensor pin as a digital input (with an internal pull resistor) and read it; the switch counts as "triggered" when the pin reads the designated active logic level. If the debug simulation toggle is on, the simulated-sensor toggle substitutes for the real pin.
3. On a rising edge (not-triggered last frame, triggered now): increment the activation counter and zero the state timer (an activation counts as activity for the inactivity timeout).
4. If in the resting state and any activation has occurred: advance to the first color state, carrying that first crank as one unit of progress toward it.
5. If the activation counter reaches the per-state requirement (a handful of turns): advance to the next state, zeroing the counter and timer. Advancing past the last color state lands in the success state.
6. If in the success state and its total duration has elapsed (flash count times flash length — a second or two overall): reset to resting.
7. If in any non-resting state and the state timer exceeds the inactivity timeout (on the order of ten seconds): reset to resting.
8. Store this frame's sensor reading for next frame's edge detection.

**Randomness:** none. **Layout assumptions:** none — output is uniform. The two-device split is purely the node-ID table row; on a single-device reimplementation you could map the two rows to two halves of one strip instead.

## Colors (qualitative)

- Resting: golden yellow (upper) / soft warm white with reduced saturation (lower).
- Sequence: fully saturated red → orange → yellow → green → blue → purple, all at full brightness.
- Success flashes: the resting colors again, strobing.

## Controls

Two debug-only toggles (the comments say to remove them for production):

- **Toggle — sensor simulation mode:** when on, the real pin is ignored.
- **Toggle — simulated sensor active:** stands in for the crank switch; flipping it on produces one activation edge.

## Sensor input

One digital GPIO pin, read every frame, edge-detected in software. Conceptually: "a switch that closes once per crank revolution." No analog, sound, or accelerometer input.

## Timing

Resting pulse: a few seconds per breath. Success: a handful of sub-second flashes totaling a second or two. Inactivity reset: about ten seconds. Everything else is event-driven by crank turns.
