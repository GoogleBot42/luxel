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
   per session.

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

## Failure modes

- Reporting success off `npm run build` alone — it doesn't execute the app; a
  runtime-only regression (like the read-only editor) won't show up there.
- Two sessions' `vite preview`/`e2e.mjs` runs colliding on the default port — always a
  silent wrong-app pass, never a loud error. If tile counts or UI look "off" for no
  reason, check for a stale process on the port before debugging the change itself.
- Forgetting `npm run wasm`/`gen-gallery.mjs` reran after a `library/` or corpus change —
  `e2e.mjs`/`device-e2e.mjs` serve whatever `web/dist` currently holds, which is stale
  until you rebuild.
