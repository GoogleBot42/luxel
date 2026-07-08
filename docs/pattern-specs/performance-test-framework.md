# Performance test framework
kind: 1D (nominal — the renderer is intentionally empty)
sensors: no

## Not a visual pattern
This is a developer utility, not an LED effect. The LEDs stay dark ("no blinkenlights"); all output is via exported variables meant to be inspected in the editor's live variable watcher. A reimplementation only makes sense if the target platform has an equivalent live-variable inspection facility.

## What it does
It benchmarks two alternative code implementations against each other, correcting for shared overhead:

- Three user-editable functions are defined: an **overhead** function (the shared loop/setup cost that should not count), a **control** function (baseline implementation), and an **experiment** function (the candidate being compared). The shipped example runs a loop of about a thousand iterations, comparing squaring a number via the general power builtin (control) versus multiplying it by itself (experiment); the overhead function runs the same loop doing only a bare assignment.
- State kept between frames: which of the three phases is active, accumulated elapsed milliseconds, and a frame counter.
- Per frame (in the pre-render hook): add the frame delta to the accumulator, bump the frame count, then call whichever of the three functions is currently active — this is the workload being timed.
- When the accumulator passes about one second: record elapsed-time ÷ frames as that phase's milliseconds-per-execution; for the control and experiment phases, subtract the overhead phase's result first (floored at zero). After the experiment phase completes, publish a **speedup** ratio = control time ÷ experiment time (1 means equal, 2 means the experiment is twice as fast). Then advance to the next phase (cycling through all three forever) and reset the accumulator and frame count.
- Exported/watchable outputs: a three-element results array (overhead, control, experiment, in milliseconds per execution) and the speedup ratio. A full cycle takes about three seconds and results refresh every cycle.

No randomness, no per-pixel work, no layout assumptions, no controls, no sensors.
