# Automap
kind: 1D
sensors: no

Trivial utility pattern, not a decorative effect. It exists to support automated pixel-mapping: an external client (over the device's variable-setting API/websocket) writes an exported integer variable naming a single pixel index; the renderer lights exactly that one pixel fully bright in a pure saturated red-family color and leaves every other pixel dark. The variable defaults to an impossible index (negative one) so nothing is lit until a client sets it.

No per-frame state, no animation, no controls in the UI (the exported variable is the whole interface). The per-frame hook is present but empty.

Implementation is one line of logic: brightness equals "this pixel's index equals the requested index".
