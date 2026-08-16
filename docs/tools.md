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
| `tools/soak.mjs` | The host-side twin: same pattern-churn against `luxel serve` (the native mirror) — liveness/fps/memory without hardware. |
| `tools/stack-check.sh [budget]` | Builds the Xtensa firmware with `-Z emit-stack-sizes` and fails if any function's stack frame exceeds the budget (default 12 KB). Sees EVERY linked function incl. deps — catches what clippy's lints can't. `stack-check.py` is its ELF parser. |
| `tools/size-report.py` | Flash-size breakdown of the firmware ELF (see docs/size-report.md for the methodology + history). |
| `tools/image-check.sh <elf-or-bin>` | Asserts load-bearing features (WLED takeover, AP provisioning, boot guard) are actually linked into a built image via their serial-string markers — the //SIZETEST regression guard. Runs automatically in build-esp32.sh and the release workflow. |

## Compatibility: corpus & oracle

| tool | what it does |
|---|---|
| `tools/corpus/fetch.mjs` | Fetches the community pattern corpus (~293 `.epe`) from patterns.electromage.com into `corpus/` (gitignored, never redistributed). **Clean-room rule: never copy corpus code into Luxel — see the describer-firewall policy.** |
| `tools/corpus/report.mjs` | `cargo build --release -p luxel-cli && node tools/corpus/report.mjs` — runs `luxel check` (compile + LXBC round-trip + smoke frames) over the whole corpus; aggregates pass rate, error buckets, missing-builtin usage into `tools/corpus/last-report.json`. Baseline: 291/293. |
| `tools/oracle/*` | The **differential oracle** against Jeremy's real Pixel Blaze (192.168.0.140, black-box via its public websocket; live-code only, nothing saved to it). `run.mjs` drives the `vectors.mjs` battery (exact 16.16 raws both sides); `pixels.mjs` diffs rendered RGB via previewFrame; `sweep.mjs`/`compare-sweeps.mjs` sample builtins over input grids into `sweeps/` for algorithm fitting; `mapdump.mjs` dumps/restores the device's installed pixel map losslessly (reconstructed from inside the pattern language); `probe.mjs` is a quick connectivity check; `pb.mjs`/`compiler.mjs` are the client + PB's own compiler extracted for encoding parity. |

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

## Heap & memory analysis

| tool | what it does |
|---|---|
| `cargo test -p luxel-cli --release --test heapstat -- --nocapture` | Counting-allocator model of the device pattern lifecycle (decode → engine → frames) over the whole gallery; prints per-pattern blob/program/engine/peak and a device-model column. How the OOM hunt and the const-array win were measured. |
| `/api/status` `heap_free` | Live heap margin on the device; `docs/bench-report.md` records the soak's lowest observed value. |
