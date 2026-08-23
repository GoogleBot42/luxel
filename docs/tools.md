# Tools directory

One-page index of every script and harness in the repo. Each file carries
its full usage in a header comment — this page exists so you know what to
open. Everything runs inside `nix develop` from the repo root unless noted.

## Device operations

| tool | what it does |
|---|---|
| `tools/deploy.sh <ip> [--fw-only\|--assets-only]` | The one-shot deploy: builds firmware + web app, OTAs the image (waits for the reboot), streams the asset archive. Serial flashing does NOT update assets — use this after any serial recovery. |
| `tools/ota-push.sh <ip>` | Just the firmware OTA + comes-back-on-the-new-slot verification (deploy.sh calls it). Refuses images built without WiFi creds. |
| `tools/decode-backtrace.sh` | Symbolicate ESP32 panic backtraces from a serial log against the current ELF. |
| `tools/release.sh [X.Y.Z] [--dry-run]` | Cut a release tag (validates tag == firmware/Cargo.toml on origin/master, then tags via the Gitea API). The GitHub mirror's release workflow builds + publishes the assets — see docs/releases.md for the whole pipeline. |
| `firmware/serial.log` | Live serial feed from the dev unit (fed from outside the container — there is no in-container serial). Watch for `PANIC` / `panic: rebooting in 3s` / `pattern rejected`; the HTTP-side view of a soak hides reboots (the device is back in ~5 s and retries succeed). |

## Hardware verification

| tool | what it does |
|---|---|
| `tools/hw-bench.mjs <ip> [report.md]` | **The soak**: pushes all ~195 gallery patterns to a real device one by one (~15 min), samples fps/heap/vmerr per pattern, then measures fps vs pixel count (60→2048), restores the device, and writes `docs/bench-report.md`. The definitive "does the firmware survive real patterns" gate — run it with a serial-log monitor for panics. |
| `tools/event-soak.mjs <ip>` | Focused soak of external event injection (`POST /api/events` → `readEvent`): live-codes a counter pattern, injects ~30k events (steady 1–32 batches, malformed frames, 32-batch overflow bursts), verifies delivery/rejection/heap/fps over HTTP only (`/api/vars` is raw 16.16). First run 2026-08-22 on the Athom: all green. |
| `tools/soak.mjs` | The host-side twin: same pattern-churn against `luxel serve` (the native mirror) — liveness/fps/memory without hardware. |
| `[BOARD=…] [EXTRA_FEATURES=…] tools/stack-check.sh [budget]` | Builds the firmware with `-Z emit-stack-sizes` and fails if any function's stack frame exceeds the budget (default 12 KB), or if the leftover `.stack` drops below 24 KB. Sees EVERY linked function incl. deps — catches what clippy's lints can't. `stack-check.py` is its ELF parser. Works for any board (default `board-pixelblaze-v3`); chip/target/toolchain come from `firmware/board-target.sh`, the shared board map that `firmware/build-esp32.sh` also sources. `EXTRA_FEATURES=small-chip` checks the RAM-constrained profile, which shifts the statics/stack split and so needs its own run (same env knob as `build-esp32.sh`). |
| `tools/size-report.py` | Flash-size breakdown of the firmware ELF (see docs/size-report.md for the methodology + history). |
| `tools/qemu/qemu-espressif.nix` (`nix build .#qemu-espressif`) | Flake-output Nix derivation for Espressif's QEMU fork (esp32 machine, flash-file backed), carrying the patches that make esp-rs firmware run: CPENABLE reset, DPORT `INTR_STATUS`, TIMG level-int gating (`tools/qemu/patches/`). Stock images boot to the WiFi task — see docs/research/qemu-emulation-spike.md for the CLI and the divergence notes. |
| `tools/qemu/takeover-test.py --stock <stock.bin> --fs <fs.bin> [--slot app0\|app1]` | **The WLED→Luxel takeover test, hardware-free.** Composes a 4 MiB flash in the exact state a real Athom is in when WLED's OTA updater has just accepted a Luxel upload (stock dump + configured WLED littlefs at 0x310000 + `result/luxel-fw-ota.bin` in an app slot + an otadata entry selecting it), boots it under the patched QEMU, and asserts the takeover's serial narration **and** the resulting flash bytes (Luxel table at 0x8000, `LXCF` inherited-creds record, the copied image, untouched littlefs, otadata/boot-guard state). `--slot app1` (default) exercises the full 920 KiB self-copy; `--slot app0` the skip-the-copy path; `--inject-fault` drops boot 1's program ops at sector 0x10000 via the emulator's m25p80 fault-injection patch (`LUXEL_FLAKY_WRITE`) and asserts the takeover's bounded reboot-to-retry recovers (issue #35). ~12 s / ~2 s / ~13 s. Needs `nix build .#luxel-fw-athom-music` first and the two **gitignored** dumps `athom-wled-stock.bin` / `athom-wled-fs-configured.bin` from the main checkout — local/agent harness, not CI. |
| `tools/qemu/run-all.py` (`nix develop -c python3 tools/qemu/run-all.py`) | **The QEMU test harness — one command runs every emulator-backed test.** Builds the athom firmware (`.#luxel-fw-athom-music` → `./result`) and Espressif's QEMU (`.#qemu-espressif` → `./result-qemu`, a separate out-link so the two builds don't clobber each other's symlink), autodetects the two gitignored Athom dumps (or `--stock`/`--fs`/`LUXEL_ATHOM_STOCK`/`LUXEL_ATHOM_FS`), then runs takeover (app1/app0/fault) + heap-regions (selfheal/rollback) and prints a pass/fail summary. `-k <substr>` filters; `--no-build` reuses existing out-links. Tests needing the dumps are **skipped**, not failed, when the dumps are absent. ~36 s warm. Add new QEMU tests to its `suite` list. |
| `tools/qemu/heap-regions-test.py --stock <stock.bin> --fs <fs.bin> [--mode selfheal\|rollback]` | **Regression test for the pre-guard `esp-alloc: Exceeded the maximum of 3 heap memory regions` panic** (root cause + fix: UPDATES.md 2026-08-16). Composes the post-upload takeover flash like takeover-test.py. `--mode selfheal` (default) stands in for the flash-read flake by poking esp-alloc's HEAP slot array to `Some` over the gdbstub at the first `add_region` (no firmware bytes changed — harness stays isolated), then asserts the panic fires **before** the boot println/ota::init and self-heals. `--mode rollback` pre-seeds the LXBG failed-boot counter at its threshold and asserts `ota::preboot_guard` rolls back to WLED **before** the allocators run (no panic), proving a deterministic pre-guard panic can no longer loop forever. ~8 s / ~1 s. Same inputs as takeover-test.py. |
| `tools/qemu/gdbrsp.py` | Dependency-free GDB Remote Serial Protocol client (software breakpoints, memory read/write, continue/step) for driving QEMU's gdbstub from the Python harnesses — no `gdb` binary, no pygdbmi. Used by heap-regions-test.py to inject the flake. Note: QEMU's xtensa gdbstub does not auto-step past a software breakpoint on `c`, so re-inject-every-boot loops must clear the breakpoint or single-step; heap-regions-test.py sidesteps this by pre-seeding flash for the deterministic case. |
| `tools/qemu/make-efuse.py [-o efuse.bin]` | Generates the eFuse backing image that presents chip revision v3.0 to the guest, so *unmodified release* firmware clears esp-hal's min-revision gate under QEMU. Attach with `-drive ...,id=efuse -global driver=nvram.esp32.efuse,property=drive,value=efuse` (add `snapshot=on` in CI — the model writes back on a fuse burn). |
| `tools/image-check.sh <elf-or-bin>` | Asserts load-bearing features (WLED takeover, AP provisioning, boot guard) are actually linked into a built image via their serial-string markers — the //SIZETEST regression guard. Feature-gated markers (e.g. `hub75`) are asserted only when `EXPECT_FEATURES` lists the cargo feature. Runs automatically in build-esp32.sh and the release workflow. |

## Compatibility: corpus & oracle

| tool | what it does |
|---|---|
| `tools/corpus/fetch.mjs` | Fetches the community pattern corpus (~293 `.epe`) from patterns.electromage.com into `corpus/` (gitignored, never redistributed). **Clean-room rule: never copy corpus code into Luxel — see the describer-firewall policy.** |
| `tools/corpus/report.mjs` | `cargo build --release -p luxel-cli && node tools/corpus/report.mjs` — runs `luxel check` (compile + LXBC round-trip + smoke frames) over the whole corpus; aggregates pass rate, error buckets, missing-builtin usage into `tools/corpus/last-report.json`. Baseline: 291/293. |
| `tools/oracle/*` | The **differential oracle** against Jeremy's real Pixel Blaze (192.168.0.140, black-box via its public websocket; live-code only, nothing saved to it). `run.mjs` drives the `vectors.mjs` battery (exact 16.16 raws both sides); `pixels.mjs` diffs rendered RGB via previewFrame; `sweep.mjs`/`compare-sweeps.mjs` sample builtins over input grids into `sweeps/` for algorithm fitting; `mapdump.mjs` dumps/restores the device's installed pixel map losslessly (reconstructed from inside the pattern language); `probe.mjs` is a quick connectivity check; `todo-probes.mjs` is the self-judging battery that settled the last TODO(oracle) markers 2026-08-22 (palette edges, transform cap, rotate conventions, null/undefined, arity recon via the local PB compiler); `pb.mjs`/`compiler.mjs` are the client + PB's own compiler extracted for encoding parity. |

## Web / e2e (from `web/`, `npm run build` first)

| tool | what it does |
|---|---|
| `web/tools/e2e.mjs` | Playground-only e2e in real chromium (puppeteer-core, nix-provided): gallery, editor, compile. |
| `web/tools/device-e2e.mjs` | Device-mode e2e: builds `luxel-cli`, starts the mirror, points the UI at it via `?device=` — connect, live push, library CRUD, playlist, map install, controls. The pattern-flow gate. |
| `web/tools/sync-e2e.mjs` | Two mirrors over loopback UDP: leader/follower clock convergence, sensor relay, pattern adoption via `/api/pattern.lxp`. |
| `web/tools/coldload.mjs` | Cold-load soak against a REAL device: `node tools/coldload.mjs <url> [N]` launches N fresh-profile chromiums (cache off) and requires full device-mode boot with zero failed requests — the acceptance check for the 2-socket web pool. `TRACE=1` prints per-request timelines on clean loads too. |
| `web/tools/lxp.mjs` | Node-side pattern compiler for scripts: loads the built `luxel.wasm`, exports `compile()` / `envelope()` / `lxpBody()` — how anything outside a browser produces the LXP1 envelopes the device API takes. |
| `web/tools/gen-gallery.mjs` | Builds `public/gallery.json` from the corpus, filtered to patterns that compile clean per the last corpus report. Runs as part of `npm run build`. |
| `web/tools/pack-assets.mjs` | Packs `dist/` into the LUXA flash archive `deploy.sh` uploads. |
| `web/tools/flash-e2e.mjs` | Installer-page (flash.html) e2e in real chromium against `fake-wled.mjs`: bundled + github firmware-source modes, CORS-less and CORS-full WLED, esp8266 stop, full flash→reboot→assets flow. No hardware. |
| `web/tools/fake-wled.mjs` | A fake WLED device over HTTP that "reboots into Luxel" after an `/update` upload — the fixture flash-e2e drives. Arch/CORS/reboot-time via env. |
| `web/tools/gen-flash-manifest.mjs` | Writes `firmware/manifest.json` for the installer page from a directory of release artifacts (release workflow + flash-e2e fixture both use it). |
| `tools/serve-e2e.mjs` | Fast fetch-only smoke test of the mirror: HTTP API + page routing (`/` serves the built playground or the minimal fallback; `/min` the minimal page). Full-UI browser coverage is `web/tools/device-e2e.mjs`. |
| `tools/mqtt-e2e.mjs` | MQTT bridge against a REAL local mosquitto (dev-shell dep): mirror connects, retained availability, and the `luxel/<id>/event` topic driving a `readEvent()` pattern (text lines → pixels). Needs `web/public/luxel.wasm` (`npm run wasm`). |

## CLI (`cargo run -p luxel-cli --` or `target/release/luxel`)

`run` (PPM frame strip) · `bench` (VM throughput) · `parse` (AST dump) ·
`check` (compile + bytecode round-trip + smoke, JSON line — the corpus
report's engine) · `compile` (source → `.lxbc`) · `pixels`/`vars` (oracle
halves) · `serve` (the native device-API mirror the web e2e runs against).

`serve --heap-free BYTES` makes the mirror report that much free heap from
`/api/status` instead of the default 0 ("this host has no meaningful number")
— how the editor's capacity warning (docs/webui.md) is exercised against a
starved device without hardware.

## Heap & memory analysis

| tool | what it does |
|---|---|
| `cargo test -p luxel-cli --release --test heapstat -- --nocapture` | Counting-allocator model of the device pattern lifecycle (decode → engine → frames) over the whole gallery; prints per-pattern blob/program/engine/peak and a device-model column. How the OOM hunt and the const-array win were measured. |
| `/api/status` `heap_free` | Live heap margin on the device; `docs/bench-report.md` records the soak's lowest observed value. |
