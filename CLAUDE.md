# Luxel (repo "pixler")

FOSS Pixel Blaze-inspired LED controller: Rust engine (`crates/`), Svelte playground
(`web/`), ESP32 firmware (`firmware/`). Deep context: README.md, docs/PLAN.md.
Every script and harness is indexed in **docs/tools.md** — check there before writing
a one-off tool. `UPDATES.md` is the worklog: append a dated entry for substantial work.

## Toolchain
- Everything runs inside `nix develop` — cargo, node, and chromium are only on PATH there.
- New toolchains/deps must be Nix flake derivations, never imperative run-once setup scripts.
- Rust over C unless genuinely blocked; strict TypeScript in `web/`; prefer established
  crates over hand-rolling.

## Environment boundaries
- This session runs in a container with NO serial access. Serial flashing is Jeremy-only
  (needs a physical button hold). `firmware/serial.log` is fed from outside the container
  and can go stale — check its mtime before trusting soak/panic claims from it.
- OTA deploys and real-chromium browser testing DO work from inside the container.
- There is ONE Luxel dev device and ONE Pixel Blaze oracle — never parallelize
  device-touching work across subagents.

## Subagents
- Prefer Opus or Sonnet models for subagent work where they're up to the task
  (searches, mechanical edits, well-specified implementation); reserve the
  top-tier model for the main loop and genuinely hard reasoning.

## Autonomy
- Be decisive on routine engineering safeguards (tests, lints, guards, docs): add them
  and report, don't ask.
- Commit and push completed, verified work when it's the right time — don't leave
  finished work sitting dirty in the tree waiting for Jeremy to ask. Open the PR
  AND merge it yourself; don't wait for Jeremy to merge (his standing
  instructions, 2026-08-15).
- OTA / live-coding / soak testing on the dev device and the Athom rig is pre-authorized
  (see .claude/skills/deploy-device and athom-rig).
- Ask first for: irreversible hardware actions, outward-facing actions, and design
  tradeoffs that genuinely depend on Jeremy's judgment.

## Hard rules
- NEVER copy code from the scraped pattern corpus (`corpus/`) into any deliverable.
  Reuse goes through the describer-firewall procedure — .claude/skills/cleanroom-port.
  (Deliberate duplication with .claude/rules/corpus-cleanroom.md — keep both.)
- The real Pixel Blaze is a black-box oracle: public websocket API only; never reverse
  or disassemble its firmware.
- No secrets in tracked files, ever. WiFi creds live in `firmware/creds.env` (gitignored);
  HA/MQTT broker details live only in agent memory. Device flash dumps (`*.bin`) are
  gitignored — keep it that way.
- License split: Apache-2.0 (crates, web, library) vs GPL-3.0-or-later (firmware).
  Match the split when adding files.

## Session shape
- After finishing any substantial piece of work (and before the final summary), run the
  `reflect` skill — it patches stale guidance and prunes memory. "No changes needed" is
  the normal outcome; small/routine tasks skip it.

## Verification norms
- Web UI change → drive it in real chromium (puppeteer-core) and screenshot before
  calling it done — .claude/skills/verify-webui. Build/typecheck alone is not verification.
- Firmware change touching statics or buffers → run `tools/stack-check.sh`.
  Measure, don't estimate.
- "Works on device" claims → `tools/hw-bench.mjs` soak (docs/tools.md).

## Tripwires
- Firmware app must fit the 1 MiB OTA slot — docs/boards.md tracks per-board margin.
- Boot-time multi-KB loads wait for `wait_config_up()` — WiFi mallocs don't null-check.
- `BUILTINS` in `crates/luxel-core/src/vm.rs` is append-only; never reorder.
- A serial flash leaves the assets partition stale → follow with
  `tools/deploy.sh <ip> --assets-only`.
