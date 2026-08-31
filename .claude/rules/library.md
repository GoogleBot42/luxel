---
paths:
  - "library/**"
---

# Library pattern conventions (house style set by the 2026-08-31 review pass, PR #176)

- Controls carry real-unit `//#` bounds (seconds, degrees, pixels, percent;
  integer counts with step=1; mode selectors min=0 max=n-1 step=1 so every
  mode is reachable). A bare 0..1 slider with meaningless values is the
  single most common review complaint.
- With a `//#` directive the UI sends REAL units — the handler must not
  rescale as if it got 0..1 (this exact bug indexed arrays at 381 in
  glittering-jewels). floor()/clamp() defensively; a stock PB still sends
  0..1, so degrade safely.
- A directive parses ONLY trailing on the export line or on the line
  immediately above it; anywhere else it is silently ignored (web/src/lib/
  hints.ts). `tools/check-library.sh` lints this and fails the sweep on a
  detached directive (Gitea #179).
- Attaching a directive makes it LIVE: the playground seeds each control with
  its `default=` on load (App.svelte), so the effective default becomes
  "handler applied at `default=`", not the top-level initializer. Check the
  two agree before/after, and check the handler consumes the declared units.
- Shape control math so the declared `default=` reproduces the shipped
  constant, and give the variable that value at top level — then verify the
  untouched render is byte-identical (snap.mjs before/after, compare the
  port.png md5s). "Add controls" must not change the approved look.
- NEVER add a pattern-level brightness/master-dim control — Luxel has a
  global brightness setting (Jeremy removes these on sight).
- Fix-pass verification gotchas: snap's meta.json nests per-side data under
  `sides.port`; `--controls-port` matches the display label (export name
  minus prefix, no spaces) or the full export name; `--probe-controls`
  ignores `//#` bounds (Gitea #180) — use directed `--controls-port` runs
  at min/mid/max for real evidence.
- When a fix pass acts on a review decision, stamp `addressedAt` on its
  tools/verify/decisions.json entry — the review UI's "addressed" chip is
  how Jeremy finds patterns awaiting re-review. Re-deciding clears the
  stamp automatically.
- New curated originals: `// name:` header first line, provenance comment
  ("Curated original …", not a port), controls from day one, no
  docs/pattern-specs entry. Long-running exported counters wrap negative
  past 32767 (16.16) — roll them explicitly.
