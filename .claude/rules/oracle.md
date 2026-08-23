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
- A live-coded program does not START RUNNING until a `setControls` message
  arrives — even an empty one. `tools/oracle/pb.mjs`'s `setCode()` sends a
  trailing `send({setControls: {}})` for exactly this reason; it is
  load-bearing, not cleanup. A hand-rolled client that omits it reads an empty
  `getVars` forever and looks like a hung pattern. (2026-08-22, fw 3.67)
- Never read a control's default out of `activeProgram.controls` for an
  UNSAVED (live-coded) program: never-set controls echo uninitialized garbage
  that varies run to run. `{getControls: true}` reports SAVED controls only, so
  it comes back empty for live code. Neither is a source of defaults.
  (2026-08-22, fw 3.67)
- Control initialization, as measured on fw 3.67 (2026-08-22): on live-code
  load PB calls NO control function of any kind — control-backed globals keep
  whatever top-level code assigned. Values arrive ONLY via `setControls`, which
  calls exactly the named functions once each with 16.16-truncated values
  (1 arg for slider/toggle, 3 for hsvPicker/rgbPicker, 0 for trigger). Nothing
  is replayed on a source re-send: explicitly-set values do not survive it, the
  program re-inits fresh. Whether PB replays saved values when switching to a
  SAVED pattern is UNVERIFIED (we cannot save to the oracle).
- Check docs/research/04-oracle-findings.md before re-deriving builtin
  behavior against the oracle — it may already be documented there.
