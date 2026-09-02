---
name: verify-webui
description: Verifying a web playground (web/) change in a real browser before calling it done — use after any UI-affecting change, not just build/typecheck.
---

Jeremy's standing expectation: a web UI change is not "done" until it's been driven in a
real browser and screenshotted. Build and typecheck passing is not verification — a
duplicate `@codemirror/state` dependency once made the editor silently read-only while
`npm run build` stayed green the whole time. If you're in a fresh worktree, run the
`worktree-setup` skill first: this repo's worktrees are missing `corpus/`, `web/public/`,
and `node_modules`, and the harnesses below assume a real `npm run build` succeeded.

## Procedure

1. Cheapest first check: `node tools/serve-e2e.mjs` from the repo root. Fetch-only smoke
   test of `luxel serve` (builds `luxel-cli`, starts it on port 8721) — HTTP API
   (`/api/status`, `/api/pixels`, `/api/code`) plus page routing (`/` serves the built
   playground when `web/dist` exists, else a minimal fallback; `/min` always the minimal
   page). No browser involved; catches API/build regressions before spending time on
   puppeteer. Ports are fixed (not `E2E_PORT`-configurable).
2. For anything that touches rendered UI, drive it for real: `cd web && npm run build`
   (runs `npm run wasm` → `gen-gallery.mjs` → `svelte-check` → `vite build`), then one of:
   - `node tools/e2e.mjs [screenshot-dir]` — playground-only, no device. Starts its own
     `vite preview` on `E2E_PORT` (default 4179). Covers the pattern library, editor,
     compile-error surfacing, tile spinners. Screenshots land in `screenshot-dir`
     (default `/tmp`) as `e2e-N-*.png`.
   - `node tools/device-e2e.mjs` — device-mode. Builds `luxel-cli`, starts `luxel serve`
     (the native mirror of the firmware API, fixed port 8723) as a stand-in device, then
     drives the playground on `E2E_PORT` (default 4181) pointed at it via `?device=`.
     Covers connect, editor sync from device, live-code push, preview streaming,
     controls/vars, compile errors, disconnect.
   - `node tools/sync-e2e.mjs` — two native mirrors over loopback UDP (fixed ports
     8731/8732), no browser: leader/follower clock convergence and sensor relay. Use
     this only when the change touches multi-device sync, not general UI work.
   All three build `luxel-cli` (`cargo build -q -p luxel-cli`) and/or the wasm engine
   themselves — no separate build step needed beyond `npm run build` for the wasm/gallery
   assets `e2e.mjs`/`device-e2e.mjs` serve.
3. Set `E2E_PORT` explicitly whenever another session might be running `vite
   preview`/`vite dev` — a concurrent process already holding the default port makes
   `--strictPort` fail loudly for the harness's own server, but if it's puppeteer's
   *target* URL that collides with someone else's server, puppeteer silently drives the
   wrong app (wrong tile count, unexpected UI) with no error at all. Pick an unused port
   per session. This applies to EVERY locally served page, not just vite —
   `tools/verify/review.mjs` too: on 2026-08-30 a session's chromium silently drove
   another session's review server on the default port and a working change looked
   broken (404s on a file only the new server had).

   Address the preview server as **`localhost`, never `127.0.0.1`**. `vite preview`
   binds the name, which resolves to `::1` here, so a `127.0.0.1:<port>` target gets
   `ERR_CONNECTION_REFUSED` while the server is plainly up and logging
   `➜ Local: http://localhost:<port>/` (cost two debug cycles on 2026-08-29 —
   it reads exactly like "the server didn't start"). The mirror (`luxel serve`)
   is the opposite: it binds `127.0.0.1`, which is what `?device=` should say.
4. For manual/ad-hoc checks beyond the scripted harnesses, real chromium is on `PATH`
   inside `nix develop` (`command -v chromium`) and `puppeteer-core` is available in
   `web/` (`optionalDependencies`) — drive it over CDP the same way `e2e.mjs` does:
   launch with `--no-sandbox --disable-gpu`, script real clicks/typing (not
   `page.evaluate` shortcuts that bypass the actual UI), and take `page.screenshot()`
   at each key state. The Read tool renders PNG screenshots directly — read the file
   back to actually look at it before reporting success.
   Ad-hoc script mechanics that each cost a debug cycle on 2026-09-01: the script
   must live under `web/` (e.g. `web/tools/_scratch.mjs`, deleted before commit) —
   an ESM `import puppeteer from "puppeteer-core"` from the scratchpad fails with
   `ERR_MODULE_NOT_FOUND` and `NODE_PATH` does not rescue ESM; resolve the browser
   with `execSync("command -v chromium")` like e2e.mjs (a bare `"chromium"`
   executablePath is rejected); and spawn `vite preview` with `stdio: "ignore"` plus
   a fixed sleep — waiting for a "Local:" line on its stdout hangs until timeout.
   Clean up with `pkill -f 'vite preview --port 419[3]'` (bracket one character):
   a plain `pkill -f '<text>'` matches your own `bash -c` command line and kills the
   shell mid-command.

   Two gotchas when the harness script lives OUTSIDE `web/` (e.g. scratch verification
   for a tool that serves its own UI, like `tools/verify/review.mjs`):
   - `NODE_PATH=…/web/node_modules` does **not** work — ESM ignores it. Symlink
     instead: `ln -s /home/googlebot/workspace/pixler/web/node_modules <scriptdir>/node_modules`.
   - `executablePath: "chromium"` fails with "Browser was not found at the configured
     executablePath" — puppeteer wants an absolute path, and PATH lookup is not done.
     Resolve it inside the shell:
     `nix develop -c bash -c 'CHROMIUM=$(command -v chromium) node harness.mjs'`.

   Also: the nix chromium ships **no emoji font**, so 🗑 ✅ 🔀 🔧 and even ⏸ render as
   tofu boxes in screenshots — and in any UI you build for it. ✕ ✓ ✗ ⟳ ▶ ‖ ⋔ ⚙ ⚠ ★ »
   are all covered. Boxes in a screenshot are the font, not your markup; prefer the
   covered glyphs so the UI reads correctly in this browser too.
5. Watch `PIPESTATUS` when piping build output: `npm run build | tail` masks a failing
   `wasm`/`gen-gallery`/`svelte-check`/`vite build` step behind `tail`'s own success exit
   code. Either don't pipe, or check `${PIPESTATUS[0]}` explicitly.

### Cold loads against a REAL device

For changes touching startup/connection behavior (fetch gating, device
probe, boot cover), the mirror is not enough: browser connection-pool
behavior vs the device's tiny socket pool only shows on hardware. Use
`web/tools/coldload.mjs <device-url> [N]` — N fresh-profile chromium
launches, cache off, per-request tracing (`TRACE=1`), clean/dirty verdict
per load. Watch `/api/status`'s `"web"` slot stages and (if wired)
serial while it runs; check `slot` afterwards — a crash-looping build
rolls back silently to the same version string.

### The hosted https copy can NOT be driven headless

Verifying `https://googlebot42.github.io/luxel/?device=http://<lan-ip>` — the
URL a `hosted-ui` device's fallback page hands the user — is **not possible
from this container**, and the failure looks like a product bug. Chromium 150
blocks every request from that https origin to a plain-http LAN device with
`blocked by CORS policy: Permission was denied for this request to access the
'local' address space`, and the page shows "cannot reach device: TypeError:
Failed to fetch". Three escapes were tried on 2026-08-31 and none work:
a CDP `Browser.grantPermissions(["localNetworkAccess"])` is accepted but
changes nothing; an explicit `targetAddressSpace: "local"`/`"private"` on the
fetch is blocked identically; and `--disable-features=LocalNetworkAccessChecks,…`
only swaps the LNA denial for a plain mixed-content block. Headless has no
permission prompt to answer, so this leg needs Jeremy in a headful browser
(Gitea #162).

What you CAN verify, and what actually exercises the device's CORS + fetch
gate: serve the same built app from a plain-**http** origin (`vite preview` on
localhost) and point it at `?device=http://<lan-ip>`. Cross-origin http→http
has neither gate, and it drives a real device fully.

You can also drive the app from a REAL https origin locally — `vite preview`
only speaks http, and `location.protocol` is `[Unforgeable]`, so anything
keyed on "am I https?" needs an actual TLS server. `web/tools/lna-e2e.mjs`
(added with #162) is the worked example: throwaway self-signed cert from
`openssl` (dev shell), a node https server over `web/dist`, chromium launched
with `--ignore-certificate-errors`. That covers the browser-blocked *UI*
state — but do not mistake it for the Pages case. **An https origin on
loopback is a different address space from a public one**: from
`https://localhost` Chromium 150 let requests to a LAN address straight
through to the socket (`net::ERR_CONNECTION_REFUSED`, no LNA denial, no
mixed-content block) with `targetAddressSpace` none/local/public alike, and
`--enable-features=LocalNetworkAccessChecks` plus
`--ip-address-space-overrides=127.0.0.1:<port>=public` did not make the
policy engage (measured 2026-08-31). The policy governs public→local, and
nothing in this container is public.

## Failure modes

- Reporting success off `npm run build` alone — it doesn't execute the app; a
  runtime-only regression (like the read-only editor) won't show up there.
- `e2e.mjs` dying instantly on `net::ERR_CONNECTION_REFUSED at http://localhost:<port>/`
  when nothing else is on the port — you ran it from the repo ROOT. It spawns
  `npx vite preview` with no `cwd`, so outside `web/` vite finds no config and never
  serves; the symptom is identical to the port-collision case below, which sends you
  hunting the wrong thing. Always `cd web && node tools/e2e.mjs …` (the script's own
  header says "from web/"; the failure does not).
- Two sessions' `vite preview`/`e2e.mjs` runs colliding on the default port — always a
  silent wrong-app pass, never a loud error. If tile counts or UI look "off" for no
  reason, check for a stale process on the port before debugging the change itself.
- `device-e2e.mjs` dying mid-suite with `ECONNREFUSED 127.0.0.1:8723` — its mirror port
  is hardcoded (no `E2E_PORT` for it), so a concurrent session's `luxel serve`, or your
  own orphan from a previous aborted run, takes it. `ss -tlnp | grep 8723` then
  `ls -l /proc/<pid>/cwd` tells you whose it is; kill your own and re-run before
  suspecting the change.
- A `page.click` on a Settings-tab field throwing "Node is either not clickable or not
  an Element" — the settings panel is rendered into the DOM even while the editor is
  open (`hidden={editing || tab !== "settings"}`), so the element *exists* and
  `page.$(...)` finds it, but it has no clickable box. That's why every Settings field
  in `device-e2e.mjs` is driven with `page.$eval` + `dispatchEvent("input")` and
  `"change"` rather than real clicks — the one place in this repo where the
  real-clicks rule doesn't apply. (Dispatch BOTH events: Svelte `bind:value` needs
  `input`, the `on:change` handler needs `change`.) To screenshot the panel you must
  first leave the editor — click the "← Device Patterns" button, then
  `[data-role="tab-settings"]`.
- An ad-hoc script that reloads the playground in a loop finding no tiles, or a
  `.tile` handle throwing "Node is either not clickable or not an Element" — a
  reload **restores the last-opened pattern into the editor**, so the gallery
  panel is hidden. Typing into `[data-role="gallery-search"]` then silently
  no-ops (no error, no filtering) and every tile reports `!el.hidden` while
  `el.offsetParent === null`. Click `[data-role="editor-back"]` first if it
  exists, and gate tile picks on `offsetParent !== null`, not on `hidden`
  (the corpus-tab tiles live in a hidden panel and are unhidden too).
- A puppeteer click on an `<input type=range>` at exactly `box.x + box.width`
  doing nothing (value stays at min, no error) — the right edge is outside the
  control's hit box. To reach max, press on the thumb and *drag past* the end:
  `mouse.move(centre)` → `down()` → `mouse.move(box.x + box.width * 1.1, y)` →
  `up()`. Range inputs clamp during a drag, so overshooting is the reliable way
  to land on the endpoint (cost a cycle on the #206 analog-pin sliders).
- Forgetting `npm run wasm`/`gen-gallery.mjs` reran after a `library/` or corpus change —
  `e2e.mjs`/`device-e2e.mjs` serve whatever `web/dist` currently holds, which is stale
  until you rebuild.
