# Rainbow Flag
kind: 1D
sensors: no

Trivial static pattern: the strip is divided into six equal stripes displaying a rainbow-flag palette. From the start of the strip to the end: violet/purple, medium blue, green, yellow, orange, red. (Note this is the reverse of the usual flag order, which runs red first — reproduce as-is.) Each stripe is a fixed, fully saturated, bright color; nothing animates and no state is kept.

Stripe length is the total pixel count divided by six, so it adapts to any strip length (no hardcoding).

Known quirk worth preserving or fixing: the stripe membership tests use strict inequalities on both ends, so a pixel whose index lands exactly on a stripe boundary matches no stripe and is never painted (it stays black, or holds stale color if the framework doesn't clear). Likewise any pixels past the sixth stripe (when the count isn't divisible by six) are unpainted. The obvious fix is to compute the stripe as floor(index / stripe-length), clamped to the last stripe, and index a six-entry color table.

No UI controls. Runs at full brightness constantly.
