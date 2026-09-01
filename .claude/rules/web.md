---
paths:
  - "web/**"
---

- Verify UI changes in real chromium via puppeteer-core with screenshots
  before declaring done (see .claude/skills/verify-webui) — build/typecheck
  alone has missed real bugs, e.g. a duplicate `@codemirror/state` dependency
  once made the editor silently read-only with no build or type error.
- Svelte reactive statements (`$: ...`) only track variables referenced
  directly in the block's own syntax, not variables only touched inside a
  called function's body. Write `$: { a; b; fn(); }`, not `$: fn()` if `fn`
  closes over `a`/`b` — the latter silently stops re-running when `a`/`b`
  change.
- The device serves the UI from a tiny connection pool (3 sockets default,
  2 small-chip) and browser-NATIVE requests (script/stylesheet/preload
  tags) can't go through fetchgate — vite is deliberately configured with
  `cssCodeSplit: false` + `modulePreload: false` so a cold load's native
  burst stays at 2 concurrent sockets. Any change to `web/vite.config.ts`,
  an entry HTML, or anything else that alters the emitted
  `<script>`/`<link>` set of `dist/*.html` must re-run
  `web/tools/coldload.mjs` against a real device before merging — the
  installer page's second rollup entry silently grew the burst to 4 and
  every device cold load ate a TCP RST for two weeks (Gitea #92).
  Sharing a NEW module between the two entries does not trigger this on its
  own: `index.js` already statically imports a shared `app` chunk, and a
  module both entries import lands inside it, adding no tag. Confirm by
  reading `dist/index.html` after the build (one `<script>`, one
  `<link rel=stylesheet>`, no `modulepreload`) rather than assuming either
  way — that check is free; a device coldload run is not.
- Set `E2E_PORT` when running e2e concurrently with another session — a
  concurrent `vite preview` can hold the default port and puppeteer will
  silently test the wrong app. Current defaults: `web/tools/e2e.mjs` uses
  4179, `web/tools/device-e2e.mjs` uses 4181 — check those files if you need
  the exact numbers, they can change.
- In e2e scripts, write injected pattern bodies on one line — CodeMirror
  auto-closes `{`, so a trailing `}` on its own line doubles up and the
  compile silently breaks.
- Terminology Jeremy set: the hardware-bound UI is the "device console"; the
  hardware-free UI is the "playground." The playground must not offer device
  affordances (connect/disconnect controls, device badges, etc.).
- Live device pixel-stream readback in the UI was explicitly removed at
  Jeremy's request ("isn't helpful") — device mode previews via the local
  WASM engine and pushes to the device; don't reintroduce a pixel-stream
  socket. The connect handshake on page load stays, though: the device
  reports its running pattern/status before the editor opens.
- Strict TypeScript only.
