---
paths:
  - "tools/oracle/**"
---

- The real Pixel Blaze (the oracle) wedges PERMANENTLY if a websocket client
  disconnects without a clean close handshake — recovery is a physical power
  cycle. Never regress the harness's clean-close behavior (see
  `tools/oracle/pb.mjs`'s `close()`, and `run.mjs` calling it before exit).
- An aborted pattern init leaves exported vars at 0, not at their would-be
  runtime value — a probe that doesn't pair its reads with a sentinel value
  can silently read a false 0 and report a wrong result.
- Live-coded probe patterns stay on the device's LEDs after disconnect.
  Every probe battery must snapshot `activeProgramId` at connect and
  re-activate it in its `finally` block (run.mjs, todo-probes.mjs, and
  oob-probes.mjs all do — copy that shape). A skipped restore leaves the
  last probe frame glowing on the bench and reads as a wedged oracle
  (happened 2026-08-22; Jeremy reported the device "broken").
- A Pixel Blaze that has ever saved a pixel map cannot be made mapless again
  through its public API. Before any map-touching probe, snapshot the
  current map with `tools/oracle/mapdump.mjs` (bit-exact dump/restore) so it
  can be restored afterward.
- Black-box only: use the public websocket API exclusively, never reverse or
  disassemble the oracle's firmware.
- Check docs/research/04-oracle-findings.md before re-deriving builtin
  behavior against the oracle — it may already be documented there.
