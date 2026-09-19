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
  change. The mirror-image trap (2026-09-19, #463): `$: shape = f(rig)` where
  `rig` is assigned *inside a function* another reactive block calls renders a
  cycle behind — the markup showed `bar` while the component held a grid rig,
  in e2e only (it reproduced under a second page in the same browser, never in
  isolation). Assign the derived values in that same function.
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
- Set `E2E_PORT` when running e2e concurrently with another session, and set
  it to a **multiple of 100** (4200, 4300, …). Since #496 it is the base of a
  100-port block that the whole run owns: every web-preview port, every
  `luxel serve` mirror, the fake-WLED fixture and the DDP/sACN listeners are
  `E2E_PORT + <fixed offset>`, listed in `web/tools/e2e-common.mjs` and
  docs/tools.md. Nothing else may bind a literal port. Default base 4179.
  Before the block existed, `E2E_PORT` moved only the `vite preview` server
  and the mirror ports were literals — so a second session either died on
  `Address already in use` or, worse, drove the browser against the other
  session's mirror and passed.
- Naming and confirmations are in-app dialogs (`components/Dialog.svelte` +
  `stores/dialog.ts`, #472), never `window.prompt`/`confirm`. Harnesses drive
  them with `acceptDialog`/`cancelDialog` from `web/tools/e2e-common.mjs`;
  never add a `page.on("dialog")` handler — a native dialog reaching the
  browser is the regression, and it hangs the run.
- In e2e scripts, write injected pattern bodies on one line — CodeMirror
  auto-closes `{`, so a trailing `}` on its own line doubles up and the
  compile silently breaks.
- The playground previews a `render2D` pattern on a **16x16 grid at targetFps
  60** by default (`stores/geometry.ts`, "Preview as" = Auto), while the bench
  panel is 64x64 at 100+ fps. So "wrong in the browser, fine on device" for a
  2D pattern is almost always a resolution or frame-rate assumption in the
  PATTERN, not in the harness (#285 was three of them, 2026-09-07). Since #463
  the chip can be set to `Matrix 64×64` to check that without hardware.
- Terminology Jeremy set: the hardware-bound UI is the "device console"; the
  hardware-free UI is the "playground." The playground must not offer device
  affordances (connect/disconnect controls, device badges, etc.).
- Live device pixel-stream readback in the UI was explicitly removed at
  Jeremy's request ("isn't helpful") — device mode previews via the local
  WASM engine and pushes to the device; don't reintroduce a pixel-stream
  socket. The connect handshake on page load stays, though: the device
  reports its running pattern/status before the editor opens.
- `web/src` is shell (`App.svelte`) + stores (`stores/*.ts`) + pages
  (`pages/*.svelte`) + reusable components — see
  [docs/web-architecture.md](../../docs/web-architecture.md) for the store
  reference, the one-way `device → geometry → pattern` dependency rule, the
  single poll scheduler (`pollSubscribe`, never a bare `setInterval`) and the
  `notify` primitive. New app state goes in a store, not a component.
- Strict TypeScript only.
- **The web UI v2 spec is `docs/design/webui-v2/proposal.md`** (approved
  2026-09-18, epic #461). Any v2 work follows it: geometry comes from the one
  `Layout` store, and a control is *absent* unless the device's advertised
  `caps` / the Layout / the pattern's exports say the thing it acts on exists —
  never disabled, never inferred from a board name (the one exception is a
  budget the user must learn, e.g. Add layer → Pattern at the layer cap; see
  proposal §5.7 for the full visibility audit). `docs/webui.md` is historical.
- In a `page.evaluate` / `page.waitForFunction` callback, a template literal
  in the function BODY is not interpolated by node — the function is shipped
  as source text and evaluated in the browser, where your harness constant is
  undefined. It fails as a `ReferenceError` in the page (caught by the
  "no page errors" check, not by the assertion it belongs to). Pass the
  selector as a trailing argument instead (2026-09-19, #467).
- A responsive tile/card grid needs `repeat(N, minmax(0, 1fr))`, not
  `repeat(N, 1fr)`: `1fr`'s implicit minimum is the item's **min-content**, so
  one long `white-space: nowrap` label widens the track past its share and the
  grid overflows its container. A desktop track written `minmax(150px, 1fr)`
  hides this, so it only shows up in the mobile media query (2026-09-19, #467).
