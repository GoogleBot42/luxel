# Luxel (repo "pixler")

FOSS Pixel Blaze-inspired LED controller: Rust engine (`crates/`), Svelte playground
(`web/`), ESP32 firmware (`firmware/`). Deep context: README.md, docs/PLAN.md.
Every script and harness is indexed in **docs/tools.md** — check there before writing
a one-off tool. `UPDATES.md` is the worklog: append a dated entry for substantial work.

## Toolchain
- Everything runs inside `nix develop` — cargo, node, and chromium are only on PATH there.
- New toolchains/deps must be Nix flake derivations, never imperative run-once setup scripts.
- Third-party crate fixes are carried as PATCH FILES (`firmware/patches/` + a flake
  derivation materializing the patched source), never vendored source trees
  (Jeremy's preference, 2026-08-22; see firmware/patches/README.md for the mechanism).
- Rust over C unless genuinely blocked; strict TypeScript in `web/`; prefer established
  crates over hand-rolling.

## Environment boundaries
- Serial flashing of the DEV UNIT is Jeremy-only (needs a physical button hold), and its
  `firmware/serial.log` is fed from outside the container and can go stale — check its
  mtime before trusting soak/panic claims from it. The ATHOM RIG's serial appears
  in-container at `/dev/ttyUSB0` (single-reader rule — see the athom-rig skill), but
  the hotplugged node CAN be absent entirely (it was gone all of 2026-08-22; worked
  2026-08-15) — check it exists before planning around it, and fall back to polling
  `/api/status` for panic/reboot detection. Only the replug is a Jeremy action —
  `doas chmod 666 /dev/ttyUSB0` works passwordless in-container (verified
  2026-08-22), so fix the perms yourself once the node exists.
- The GitHub repo (github.com/GoogleBot42/luxel) is a READ-ONLY downstream mirror for
  releases/CI only — never push, PR, or file issues there; everything happens on Gitea.
  See docs/releases.md.
- OTA deploys and real-chromium browser testing DO work from inside the container.
- There is ONE Luxel dev device and ONE Pixel Blaze oracle — never parallelize
  device-touching work across subagents.
- Other Claude instances may be working in this checkout concurrently. ALWAYS do your
  work in a fresh git worktree (.claude/skills/worktree-setup), never in the main
  checkout directly. Treat uncommitted changes in the main checkout as another live
  session's work-in-progress: don't commit, revert, build on, or "finish" them, and
  don't assume the tree state you see is yours alone.

## Subagents
- ALWAYS prefer Opus-model subagents for any task where Opus works (searches,
  mechanical edits, well-specified implementation, routine verification) — this
  saves Fable token usage, which is the scarce resource. Sonnet is fine for the
  simplest lookups. Reserve Fable for the main loop and subagent tasks that
  genuinely need top-tier reasoning (Jeremy's standing instruction, 2026-08-16).

## Autonomy
- Be decisive on routine engineering safeguards (tests, lints, guards, docs): add them
  and report, don't ask.
- Work to be done goes in a Gitea issue — ALWAYS. Whenever something is deferred,
  left for Jeremy to verify on hardware, or otherwise "to do later," file it as a
  ticket on Gitea (`tea issues create --repo zuckerberg/luxel …`, see the git-forges
  skill), not as a docs entry, a memory note, or a session task. Docs like
  docs/UNTESTED.md may still describe status, but the actionable to-do lives in the
  tracker (Jeremy's instruction, 2026-08-16).
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
- The QEMU harness is strictly isolated: emulation fixes live in `tools/qemu/` only
  (derivation patches, eFuse images) — never guest-side workarounds, QEMU-conditional
  firmware code, or build flags. The image under test must be byte-identical to what
  ships. See docs/research/qemu-emulation-spike.md.
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
- `compile failed: unknown identifier` from `web/tools/lxp.mjs` = stale
  build artifacts: the main checkout's `web/public/luxel.wasm` and
  `target/*/luxel` lag master (nobody rebuilds them). `npm run wasm` /
  rebuild in your worktree; don't debug the pattern.
