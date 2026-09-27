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
   test of `luxel serve` (builds `luxel-cli`, starts it on `E2E_PORT + 70`) — HTTP API
   (`/api/status`, `/api/pixels`, `/api/code`) plus page routing (`/` serves the built
   playground when `web/dist` exists, else a minimal fallback; `/min` always the minimal
   page). No browser involved; catches API/build regressions before spending time on
   puppeteer. Ports are fixed (not `E2E_PORT`-configurable).
2. For anything that touches rendered UI, drive it for real: `cd web && npm run build`
   (runs `npm run wasm` → `gen-gallery.mjs` → `svelte-check` → `vite build`), then one of
   the harnesses below **from `web/` — that cwd is load-bearing**. `e2e.mjs` spawns
   `vite preview` with no `cwd` of its own, so `node web/tools/e2e.mjs` from the repo
   root serves the REPO ROOT instead of `web/dist`: the page shell loads, the gallery
   does not, and you get `0 tiles` / `failed to find element matching selector
   "[data-role="library-panel"] .tiles"` on a perfectly healthy tree. It reads exactly
   like a real regression (cost three runs and a stash-and-rebuild on 2026-09-08 before
   the cwd was the answer). Run `cd web && node tools/e2e.mjs [screenshot-dir]`:
   - `node tools/e2e.mjs [screenshot-dir]` — playground-only, no device. Starts its own
     `vite preview` on `E2E_PORT + 0` (default 4179). Covers the pattern library, editor,
     compile-error surfacing, tile spinners. Screenshots land in `screenshot-dir`
     (default `/tmp`) as `e2e-N-*.png`. **`mkdir -p` it first** — a
     screenshot-dir that does not exist is not created, and the ENOENT kills the
     run mid-way (device-e2e died ~200 checks in on 2026-09-26, reading like a
     harness bug).
   - `node tools/device-e2e.mjs` — device-mode. Builds `luxel-cli`, starts `luxel serve`
     (the native mirror of the firmware API, on `E2E_PORT + 20`) as a stand-in device,
     then drives the playground on `E2E_PORT + 2` pointed at it via `?device=`.
     Covers connect, editor sync from device, live-code push, preview streaming,
     controls/vars, compile errors, disconnect.
   - `node tools/sync-e2e.mjs` — two native mirrors over loopback UDP
     (`E2E_PORT + 40/41`), no browser: leader/follower clock convergence and sensor
     relay. Use this only when the change touches multi-device sync, not general UI work.
   Since #496 every port any harness binds is `E2E_PORT + <fixed offset>` from the table
   in `web/tools/e2e-common.mjs` (also in docs/tools.md) — one 100-port block per run.
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
   `ERR_MODULE_NOT_FOUND` — or, worse, SILENTLY resolves into **another session's
   worktree** (2026-09-27: a scratchpad script run with `cd <my worktree>/web` loaded
   `/home/googlebot/workspace/wt-786/web/node_modules/puppeteer-core`, and the only
   reason it was noticed is that the stack trace named the path). Either keep the
   script under your own `web/`, or import by ABSOLUTE path
   (`import puppeteer from "/home/googlebot/workspace/<wt>/web/node_modules/puppeteer-core/lib/esm/puppeteer/puppeteer-core.js"`)
   — same hazard as worktree-setup §4b, and the same rule: absolute paths
   everywhere. `NODE_PATH` does not rescue ESM; resolve the browser
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

### Driving a device's OWN console ad hoc

Anything gated on the device's `caps` (`/api/ota`, `/api/assets`, reboot
routes) can only be driven on the real page at `http://<device-ip>/`, because
the mirror advertises those caps off. Three things bite there and nowhere
else (all three, 2026-09-26, Gitea #526/#794):

- **That origin is not a secure context** — no `crypto.subtle`, no Clipboard,
  no `navigator.mediaDevices`. See `.claude/rules/web.md`; the way to test a
  fix without pushing assets to the device first is `vite preview --host
  0.0.0.0` and loading it by the container's LAN IP, which is insecure the
  same way while serving the code you just built.
- **The first `page.goto` can die on `net::ERR_CONNECTION_REFUSED`** with the
  device perfectly healthy: chromium preconnects and the 2-3 socket pool
  refuses the extra. Launch with
  `--disable-features=NetworkPrediction,PreconnectToOrigin,LoadingPredictorPrefetch`
  (coldload.mjs's `NO_PRECONNECT` flag) AND retry the navigation a few times.
- **Your own `fetch("/api/status")` probes compete with the page under test**
  for those same sockets. A single failed probe is not "the device
  rebooted" — retry 3-8 times before believing it, or an unlucky null
  crashes the harness mid-upload (it did; the device survived it fine).

Two more that bite on a console served by the MIRROR too (`luxel serve`,
2026-09-26, #781): the Patterns page opens on the (empty) *device* source —
click `[data-role="patterns-source-library"]` before looking for library
tiles; and start the mirror `--board panel` when the pattern under test is
2D — a strip console's library grid is `only="compatible"`
(`pages/Patterns.svelte`) and hides every 2D pattern, which looks exactly
like "the tile isn't there".

### Cold loads against a REAL device

For changes touching startup/connection behavior (fetch gating, device
probe, boot cover), the mirror is not enough: browser connection-pool
behavior vs the device's tiny socket pool only shows on hardware. Use
`web/tools/coldload.mjs <device-url> [N]` — N fresh-profile chromium
launches, cache off, per-request tracing (`TRACE=1`), clean/dirty verdict
per load. Since #592 a load is only `ok` if the page is also STYLED (body
`background-color` = `rgb(20, 22, 26)` and the `--bg` token resolving),
because the failure that ticket found — a stylesheet refused by a busy
socket pool, never retried, console rendered as raw HTML — passed every
other assertion in this tool. `inlineBoot()` in `web/vite.config.ts` now
inlines the stylesheet AND replaces the module tag with a post-parse
loader, so the shape to expect in `dist/*.html` is: no `<script src>`, no
stylesheet link, no `modulepreload` — one inline loader `<script>` and one
`<style>`. `npm test` fails if that drifts; `tools/bootretry-check.mjs`
(mirror, no device) checks the loader's retry and its bound.
A pool that is FULL (`"web":[1,1,1]` for minutes at a time, as the
Seengreat panel was on 2026-09-20) refuses the app's own `/api/*` and
`luxel.wasm` fetches too — those ARE retried, so the run still boots; read
coldload's per-load `boot ok`/`styled` before treating `N failed reqs` as
a regression in the page.
Watch `/api/status`'s `"web"` slot stages and (if wired)
serial while it runs; check `slot` afterwards — a crash-looping build
rolls back silently to the same version string.

**Driving the console against a real device CHANGES that device.** The v2
console is not a viewer: opening a **Library** tile (its `Edit`, or the tile
face) pushes that pattern to the device as live code — `POST /api/code`, which
the firmware treats as a takeover, so a running **playlist stops** and the LEDs
switch to a pattern that was never saved; opening an **On device** tile
`activate`s it, which also changes what is lit (the playlist keeps running and
swaps back at the next item). Neither says so in the UI. So a browser pass over
someone's live rig costs device state: read `/api/brightness`, `/api/layout`,
`/api/name`, `/api/pattern`, `/api/playlist` first, leave the library/editor
clicks until LAST, and restore afterwards (`POST /api/playlist/play <index>`,
`POST /api/patterns/<id>/activate`). Cost three silent playlist stops on the
Athom rig on 2026-09-19 before the cause was isolated; `coldload.mjs` itself is
safe (it never clicks a tile).

**Load the device's OWN bundle once per run, then navigate by fragment.** A
harness that points chromium at `http://<device>/` for each case — rather than
at `vite preview` with `?device=` — cold-loads index.html + the JS + the CSS +
the wasm every time, and a browser opens several parallel connections per load.
After a handful the device's web pool (3 sockets, 2 small-chip) is out and the
next `goto` comes back `net::ERR_CONNECTION_REFUSED` *while `/api/status` keeps
answering in 50ms* — so it reads as a page full of missing elements, not as the
load failure it is. Keep one page alive and set `location.hash`, which is what
the fragment router is for (Seengreat, 2026-09-20, Gitea #568).

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

- **A fresh-profile repro that looks perfect when Jeremy says it's broken.**
  The app persists real state in `localStorage` (`luxel.previewAs`,
  `luxel.current`, `luxel.patterns`, `luxel.mapSrc`) and those keys outlive
  UI versions — a value written by an older build, or by the playground on
  the same origin, is still there and still read. Every chromium harness here
  launches with a throwaway `userDataDir`, so it reproduces an empty profile,
  not his. When a bug won't reproduce, SEED the keys and reload before
  concluding the device is at fault: navigate to a cheap same-origin URL
  (`http://<ip>/api/status`), `page.evaluate` the key in, then `goto` the app.
  That is what finally reproduced #539 — two clean fresh-profile loads of a
  panel console had already said the reported bug wasn't there.
- Reporting success off `npm run build` alone — it doesn't execute the app; a
  runtime-only regression (like the read-only editor) won't show up there.
- A harness dying on `ENOENT … open '<shotdir>/….png'` partway through a long
  suite — the screenshot directory is YOURS to create, the harnesses do not
  `mkdir` it. `device-e2e.mjs` runs ~400 checks and takes its first screenshot
  around check 20, so a missing dir throws away the whole run before it says
  anything useful (2026-09-20). `mkdir -p` it before you start.
- `e2e.mjs` dying instantly on `net::ERR_CONNECTION_REFUSED at http://localhost:<port>/`
  when nothing else is on the port — you ran it from the repo ROOT. It spawns
  `npx vite preview` with no `cwd`, so outside `web/` vite finds no config and never
  serves; the symptom is identical to the port-collision case below, which sends you
  hunting the wrong thing. Always `cd web && node tools/e2e.mjs …` (the script's own
  header says "from web/"; the failure does not).
- Two sessions' `vite preview`/`e2e.mjs` runs colliding on the default port — always a
  silent wrong-app pass, never a loud error. If tile counts or UI look "off" for no
  reason, check for a stale process on the port before debugging the change itself.
  The squatter can be a day-old `vite preview` whose worktree was DELETED (a 404 on
  `/` and zero tiles, 2026-09-07): `ss -tlnp | grep <port>` then
  `ls -l /proc/<pid>/cwd` names the owner (`… (deleted)`); pick another port rather
  than killing a process that might be another live session's.
- `e2e.mjs`/`device-e2e.mjs` do NOT rebuild the app — a check failing after an
  `App.svelte` edit without `npm run build` reads exactly like a real bug (two false
  failures on 2026-09-07). Build first, every time.
- `device-e2e.mjs` dying mid-suite with `ECONNREFUSED` on its mirror — since #496 the
  mirror is `E2E_PORT + 20`, so this now means your OWN orphan from an aborted run (two
  sessions with different `E2E_PORT` can no longer collide). `ss -tlnp | grep <port>`
  then `ls -l /proc/<pid>/cwd` tells you whose it is; kill your own and re-run before
  suspecting the change.
- A `page.click` that hangs forever, or checks failing right after a save/delete/WiFi
  action — naming and confirmations are in-app dialogs (#472). Drive them with
  `acceptDialog`/`cancelDialog` from `web/tools/e2e-common.mjs`; never install a
  `page.on("dialog")` handler (a native prompt/confirm reaching the browser is itself
  the regression).
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
  exists, and gate tile picks on visibility, not on `hidden`
  (the corpus-tab tiles live in a hidden panel and are unhidden too).
  **The honest visibility test is `el.getClientRects().length > 0`, not
  `offsetParent !== null`**: inside the v2 console shell every element has
  a fixed-position ancestor, so `offsetParent` is null for ALL of them,
  visible or not, and an `offsetParent` gate finds no tiles on a console
  that is showing them (four aborted soak runs, 2026-09-26, #781).
  Device mode (`?device=`) boots straight into the editor too, so the tabs
  (`tab-settings` etc.) don't exist until that click — and the click no-ops
  if fired the instant the button appears (still loading the running
  pattern): wait ~3 s after `editor-back` shows, then click (2026-09-02, two
  timeouts on `tab-settings` before the settle delay).
- A puppeteer click on an `<input type=range>` at exactly `box.x + box.width`
  doing nothing (value stays at min, no error) — the right edge is outside the
  control's hit box. To reach max, press on the thumb and *drag past* the end:
  `mouse.move(centre)` → `down()` → `mouse.move(box.x + box.width * 1.1, y)` →
  `up()`. Range inputs clamp during a drag, so overshooting is the reliable way
  to land on the endpoint (cost a cycle on the #206 analog-pin sliders).
- **Typing a whole pattern into the editor leaves a stray `}` and the app keeps
  running the PREVIOUS program.** CodeMirror auto-closes `{`, so
  `keyboard.type(src)` with the closing brace in `src` produces one extra `}`
  at the end, the source no longer compiles, and `recompile()` deliberately
  keeps the old engine running while you type. Headless there is almost no
  signal: the preview animates, `fps` is healthy, no page error fires, and the
  compile error is a small banner off in the right-hand column. On 2026-09-07
  this made a debugger step-through look like a product bug (one "into" click
  appeared to resume the whole run) and nearly earned a bogus ticket — the
  screenshot is what exposed it. After typing, delete the tail
  (`Control+Shift+End` then `Backspace`) and ASSERT on what actually landed —
  but **never on `.cm-content`'s `textContent`**: CodeMirror virtualizes it, so
  it holds only the rendered viewport and a long paste always reads back
  truncated. Assert on the model's OUTPUT instead (compile-error banner empty,
  the capacity banner, the preview) — and note the capacity banner silently
  describes the PREVIOUS pattern when a stray `}` broke the paste (2026-09-07).
  Reusing one puppeteer page across several `setEditor` probes drops
  keystrokes; do a fresh `page.goto` per probe. `e2e.mjs`'s `setEditor` has the
  same exposure; it gets away with it because its sources end mid-line.
- A preview that looks non-square in a screenshot — `canvas.grid` in
  `Preview.svelte` is a fixed 396x320 box with `object-fit: contain`, so that
  is the letterbox, not the pattern (2026-09-07).
- Forgetting to rebuild after ANY source edit — `e2e.mjs`/`device-e2e.mjs` serve
  whatever `web/dist` currently holds. A `library/` or corpus change needs
  `gen-gallery.mjs`; a `web/src/` change needs `vite build`; `npm run build` is all of
  it. 2026-09-27: one `web/src/` edit made between a build and an `e2e.mjs` run
  produced EIGHT failures that read like real regressions in features the edit had
  nothing to do with — the harness was driving the previous bundle. If a run fails in
  a section you did not touch, suspect the build before the change.
- A harness script you just patched with a node one-liner dying on
  `e.map is not a function`, or a `$$eval` that behaves like `$eval` — **`String.replace`
  ate it**. `$$`, `$&` and dollar-backtick are all substitution syntax in a replacement
  STRING, so `s.replace(anchor, block + anchor)` silently rewrote every `page.$$eval` in
  the inserted block to `page.$eval` (2026-09-27, a full e2e run), and a later attempt
  to document that in this file pasted the whole file into itself. Splice with
  `s.slice(0, at) + block + s.slice(at)`, or pass a FUNCTION as the replacement.
- **"Confirmed pre-existing on master" means nothing without a rebuild.** The same
  `web/dist` staleness makes a *checkout* lie, not just a `library/` edit: checking
  out `origin/master` (detached or otherwise) in a worktree and re-running
  `e2e.mjs`/`device-e2e.mjs` re-tests the bundle that is already on disk, so a
  branch's own bug and a genuine master regression look identical, and so does a
  harness assertion the UI moved past hours ago. That is how Gitea #819 was filed:
  an `Onion` label reported against a master where the same commit had already
  renamed it `Ghost prev` and written the matching check. `npm run build` on the
  tree you are accusing, THEN run the harness — and say in the ticket that you did.
- **Harness fixtures pinned to a firmware constant are a standing bug, not an
  accident.** #547, #809 and #810 are all the same failure: a number the harness
  spelled out (a pixel cap, a scan depth) moved in the firmware and the case either
  stopped testing anything or went red. Derive it instead — the device serves its own
  ceiling at `/api/status`'s `max_pixels` / `/api/layout`'s `max`, and a mirror-only
  constant can be read out of the Rust source — then assert the RELATION (this chain
  exceeds that cap; the suggested arrangement fits under it) rather than the literal
  sentence.
- **Never let one missing element abort a 600-check suite.** `page.$eval` throws when
  the selector matches nothing, so an unguarded read turns a single wrong expectation
  into a run that reports nothing after it (#810 died at check 553 of 594; #819 took
  the §5.7 disabled sweep with it). Both browser harnesses carry an
  `evalOr(page, sel, fn, fallback)` next to `check` — use it for any read where the
  element's own presence is part of what is under test, and `check` the fallback.
- **`uploadFile` must be handed a RESOLVED path.** Chromium keeps the string it is
  given, so `elementHandle.uploadFile(`${import.meta.dirname}/../tests/fixtures/x.png`)`
  is accepted, sets a file named `x.png` on the input, fires `change` — and then the
  page's `file.arrayBuffer()` throws `NotReadableError: The requested file could not
  be read, typically due to permission problems that have occurred after a reference
  to a file was acquired`. It reads exactly like a decoder or a permissions bug and is
  neither (#784, an hour). `fs.realpathSync(...)` the directory once and build paths
  from that. The same upload from a `waitForFileChooser()` + `fc.accept([...])` with an
  absolute path works, which is what makes the two look like different bugs.
