# neutronorbit
kind: 1D
sensors: no

## What it looks like
An "atom" on a strip. Three warm-colored comets — one orange, one magenta-pink, one coral/salmon — sweep back and forth along the strip in smooth sinusoidal motion (slowing at the ends, fast through the middle), each leaving a short glowing tail that fades in a fraction of a second. The three share the same travel period of several seconds but are staggered by exactly a third of a cycle each, so they perpetually weave past and through one another like orbiting particles. Meanwhile a white blob sits at the strip's center — the "nucleus" — throbbing rapidly in brightness (a shallow pulse, roughly one-to-two beats per second) and breathing slightly in width, while its position quivers just a hair around the exact midpoint. Where layers overlap, the brighter wins per color channel, so crossings flash toward white.

(This file is machine-generated output of a node-graph pattern compiler; the graph description is embedded as a comment. Implement the behavior, not the generated code structure.)

## Algorithm
Everything happens per-frame in normalized strip position (fraction of pixel count), so it adapts to any strip length. State kept between frames: a running seconds clock, one birth-time per traveling pulse, and three per-pixel "trail" buffers (one per comet).

Four layers are computed each frame into per-pixel intensity buffers:

1–3. Three traveling pulses, identical except for phase and tint:
- Position: a raised-cosine (sinusoidal) oscillation of the pulse center between roughly one-tenth and nine-tenths of the strip, period of several seconds (about six), with the three pulses time-shifted by one-third of the period each (shifts of zero, one-third, two-thirds of a cycle).
- Shape: a fixed width of about a tenth of the strip; intensity across the pulse follows a half-sine hump (zero at both edges, peak in the middle). Flat in time (the pulse itself doesn't pulse).
- Trail: each pulse layer feeds a per-pixel decay buffer updated as: new value = max(previous value exponentially decayed with a half-life of roughly a tenth of a second, current pulse value). This "peak-hold with exponential release" is what draws the comet tail behind the moving hump.

4. The nucleus:
- Position: pinned essentially at the strip midpoint, with a tiny sinusoidal wobble (a couple percent of strip length) at the same several-second period as the comets.
- Width: about a tenth of the strip, itself modulated by a small sinusoidal breathing (width varies by a small fraction) with a period of about half a second.
- Shape: a triangle profile (linear ramp up then down) rather than the comets' sine hump.
- Brightness: multiplied by a fast sine oscillation that swings between roughly seven-tenths and nine-tenths of full, period well under a second — the rapid throb.
- No decay/trail on this layer.

Combining and color: each of the three comet trail buffers is tinted with its own fixed RGB weighting —
- comet A: strong red, moderate green, no blue (orange);
- comet B: strong red, no green, medium blue (magenta/pink);
- comet C: strong red, light-and-equal green and blue (coral / warm pinkish red);
- nucleus: equal full weight on all three channels (white).

The final per-pixel color takes, independently for each of red, green, and blue, the maximum across all four tinted layers (not the sum). Rendering squares each channel (gamma-style shaping) before output.

## Colors (qualitative)
Three warm comets — orange, hot pink/magenta, and coral — against black, with a white center blob. Overlaps brighten toward white because of the per-channel max.

## Controls
None.

## Timing feel
Comet round trip: several seconds. Trails: vanish in a fraction of a second. Nucleus throb: quick, around one-to-two beats per second, shallow (never dropping near black). Nucleus width breathing: about twice per second, subtle.

## Notes / quirks
- The generated code contains a pulse-spawning framework (birth times, capacity-of-one pools, respawn scheduling) with a bookkeeping bug: the live-count that would limit respawns is never actually incremented (it writes a different, unused global). Net effect: each generator spawns exactly one pulse shortly after startup and that pulse lives forever. The correct reimplementation is simply four perpetual oscillators as described above — no spawn/despawn machinery needed.
- The sinusoidal position oscillators are phrased as raised cosines of pulse age, so all comets start near the low end of their arc at power-on; only their relative one-third-cycle stagger matters visually.
