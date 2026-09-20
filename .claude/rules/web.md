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
  It is NOT only a cycle and NOT only flaky: when the assigning function is
  called *from* a reactive block, the derived `$:` never re-runs at all —
  Svelte resets `$$.dirty` only after `$$.update()` has finished, so an
  invalidation raised during it is folded into the fragment patch and
  schedules no second flush. #471 hit this deterministically (a map screen
  that ran its program on open showed "not run yet" every single time, while
  clicking Run — an event handler, i.e. its own flush — worked). Applies to a
  plain `let`, not just to stores (next bullet).
- The device serves the UI from a tiny connection pool (3 sockets default,
  2 small-chip) and browser-NATIVE requests (script/stylesheet/preload
  tags) can't go through fetchgate — vite is deliberately configured with
  `cssCodeSplit: false` + `modulePreload: false`, and the `inlineBoot()`
  plugin in `web/vite.config.ts` then (a) folds that single stylesheet INTO
  each entry HTML as a `<style>` and deletes the asset, and (b) replaces
  the module `<script src>` TAG with a loader that appends the script after
  `DOMContentLoaded` and re-appends it up to 3 times (2/4/6 s) if it is
  refused (Gitea #592). **The emitted shape of `dist/*.html` is now: NO
  browser-native subresource request at all — no `<script src>`, no
  `<link rel=stylesheet>`, no `modulepreload`; one inline `<script>`
  loader, one `<style>`, and a `data:` favicon (not a request).**
  Two things made that necessary. A refused stylesheet is never retried, so
  a busy pool gave a silently UNSTYLED console; and a tag is fetched by the
  preload scanner while the document is still arriving, so it needs a
  SECOND socket — with the panel's pool at `"web":[1,1,1]` even the lone
  bundle request was refused on 5/5 cold loads (blank page), while the
  post-parse loader rides the document's own keep-alive socket and landed
  on 5/5. Any change to `web/vite.config.ts`, an entry HTML, or
  anything else that alters that set must re-run `web/tools/coldload.mjs`
  against a real device before merging — the installer page's second
  rollup entry silently grew the burst to 4 and every device cold load ate
  a TCP RST for two weeks (Gitea #92).
  Sharing a NEW module between the two entries does not trigger this on its
  own: `index.js` already statically imports a shared `app` chunk, and a
  module both entries import lands inside it, adding no tag. The free half
  of the check is `web/tests/bundleShape.test.mjs` (in `npm test`, which CI
  runs after a build): it fails if either entry HTML asks the browser for
  anything. `coldload.mjs` is the paid half and asserts the page is styled
  (body `background-color` and the `--bg` token) as well as booted, because
  "booted but unstyled" was a state every other assertion passed;
  `web/tools/bootretry-check.mjs` exercises the retry on the mirror.
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
  Not every multiple of 100 works: Chromium refuses to navigate to its
  blocked ports, so `E2E_PORT=6000` dies with `net::ERR_UNSAFE_PORT` before a
  single check runs (6000 is X11). Move to the next block.
  **Borrowing a block that is not yours fails SILENTLY, and the failure looks
  like your own regression.** The harnesses spawn `vite preview --strictPort`
  with `stdio: "ignore"`, so when a sibling worktree already owns the port the
  new preview exits unheard and the browser happily loads THEIR `dist/` — a
  screenshot run then shows the old UI on a tree that builds the new one. Stay
  in the block your task assigned you, and if a run's output disagrees with a
  green e2e, check `ss -ltnp | grep <port>` and whose worktree that `vite
  preview` argv names before debugging the code (2026-09-19, #538).
- **Harnesses run BACK TO BACK flake on the concurrency assertions — re-run
  the one that failed, alone, before debugging it.** A mirror that was killed
  a second ago still holds its listening socket for a moment, and the checks
  that count sockets rather than outcomes are the ones that notice:
  `device-e2e`'s "never more than the gate's two requests in flight" / "the
  tab's reads never overlap" (#540) and `maxpixels-e2e`'s status rewrite both
  failed once in a `for h in …; do` loop and passed immediately on their own,
  with no code between (2026-09-20, #538). This is the opposite of the
  stale-selector rule below: a harness failure is usually real, but these
  particular counters are not. One re-run decides it.
- **The route is in the URL fragment** since #538 (`web/src/lib/router.ts`),
  which changes what "reload" means in a harness: `page.goto(url)` where the
  page is ALREADY on that URL is a same-document navigation — the app never
  re-boots, and a test that thought it was re-running the connect handshake is
  silently asserting the old page's state. Use `page.reload()`, or
  `reloadInto(page, "#/editor")` in `web/tools/device-e2e.mjs` to reload INTO
  a named screen (a plain reload reopens whichever screen the URL last named,
  which is the feature, not what a test that wants the editor means).
- Puppeteer pages from the same `browser` share an ORIGIN, and therefore share
  `localStorage` — which holds the autosaved working copy. A second page
  opened against a second mirror will resume whatever the first page last
  autosaved, so a "fresh console" fixture needs its own
  `browser.createBrowserContext()`. Two device-e2e sections were passing for
  the wrong reason before that (2026-09-19). Since #585 that resume no longer
  PUSHES — which flips the trap: a section that needs the editor in live push
  must arrive with no working copy at all (`gotoConsole` in `device-e2e.mjs`
  clears `luxel.current`), or it silently tests a preview-only editor that
  writes nothing (2026-09-20).
- Naming and confirmations are in-app dialogs (`components/Dialog.svelte` +
  `stores/dialog.ts`, #472), never `window.prompt`/`confirm`. Harnesses drive
  them with `acceptDialog`/`cancelDialog` from `web/tools/e2e-common.mjs`;
  never add a `page.on("dialog")` handler — a native dialog reaching the
  browser is the regression, and it hangs the run.
- Svelte compiles SLOTTED markup in the PARENT's scope, so a wrapper
  component cannot style what it was handed. That is why the shared chrome
  lives in `app.css` as global classes (`.btn`, `.inp`, `.menu`, `.pop`,
  `.slabel`) and `components/Popover.svelte` owns only geometry and
  dismissal — and why `components/editor-frame.css` is a plain `.css` import
  rather than a frame component. A new button wears `.btn` + a modifier and a
  new dropdown mounts `Popover`; a local copy of either is how four different
  `.primary` blocks and three divergent menu stylesheets happened (#538).
- **A harness that changes the device's LAYOUT must do it through the UI, not
  a raw `fetch`.** `deviceLayoutWire` is what the geometry reconciler reads
  and only `applyLayout()` writes it, so a `POST /api/layout` made behind the
  page's back changes the device and nothing else: every tile, thumbnail and
  Settings field keeps drawing the old shape, and the check that follows fails
  for a reason that looks nothing like the cause. Drive
  `[data-role="layout-kind"]` / the fields instead. (Cost two debug cycles in
  #538, 2026-09-19 — a 3D lattice torn down by `fetch` left every thumbnail a
  cloud, and a projection rig set by `fetch` left the row absent.)
- **Chromium's `Intl.supportedValuesOf("timeZone")` carries the LEGACY zone
  spellings** — `Asia/Calcutta`, `Asia/Katmandu`, not `Asia/Kolkata` /
  `Asia/Kathmandu` — while node's ICU resolves either through
  `DateTimeFormat`. So a zone name that a unit test accepts can be missing
  from a `<select>` built from that list, and `page.select()` answers `[]`
  and changes NOTHING rather than throwing. Pick harness zones from the
  list the browser actually returns (#538, 2026-09-19).

- **An e2e harness ABORTS on a stale selector; it does not just fail.** The
  `check()` helper collects failures, but a bare `page.click` / `page.$eval` /
  `elementHandle.click()` on something the UI no longer renders throws, and
  the run dies there — so `node tools/device-e2e.mjs | grep FAIL` can print two
  failures and hide that two thirds of the suite never executed. A run is only
  a verdict when its TAIL says `all device-mode checks passed` or `N FAILURES`;
  anything else is an abort, and the stack trace names the line. This bites
  hardest after an engine/firmware change removes an affordance the web
  harness still reaches for (#545 → #547 did exactly this: two stale
  projection assertions threw and took the playlist, settings and capacity
  phases with them, on master, for everyone).
  The other way a *later* section aborts is UI STATE an earlier one left
  behind. The Patterns page's search box and source chip belong to the PAGE,
  not to a grid, so a section that switched to `Library` and typed a needle
  leaves every `[data-source="device"] .tile` hidden — and the next
  `tileAction(DTILE, …)` dies on "Node is either not clickable or not an
  Element", 400 lines away from the cause. A section that drives a page-level
  control puts it back in its own `finally` (2026-09-19, #563).
- **Inject pattern source by PASTING it, not by typing it.** CodeMirror
  auto-closes `{`, so a typed program's own `}` doubles up and the compile
  silently breaks. "Write it on one line" only works for a body that IS one
  statement: the language needs a newline or `;` between statements, so
  `var f = index / pixelCount hsv(f, 1, 1)` is a parse error, and anything
  with two statements cannot be a one-liner. `e2e.mjs`'s `pasteEditor`
  delivers a real `ClipboardEvent` with a `DataTransfer`, which CodeMirror's
  own paste handler drops in verbatim; `mockdiff.mjs`'s `code` step does the
  same. Typing cost a whole run in #538's mockdiff work — the editor frames
  measured a compile-error state instead of the Controls rail they exist for.
- **A harness that walks SEVERAL named app states in one chromium must clear
  `localStorage`/`sessionStorage` between them**
  (`page.evaluateOnNewDocument(() => localStorage.clear())`, which `mockdiff.mjs`
  now does per frame). Every state shares one `vite preview` origin and the app
  persists real state there (`luxel.previewAs`, `luxel.current`,
  `luxel.patterns`, `luxel.mapSrc`), so state N silently re-shapes state N+1 —
  and the symptom is a PRODUCT-shaped lie, not an obvious harness error: the
  mockdiff editor frames left a dirty 2D working copy behind, the console
  resumed it, and a 300 px strip console twenty frames later measured as an
  `18×17 matrix` with its `Not for this layout` group legitimately empty
  (2026-09-20; that was a real product bug, Gitea #573 — see the console-Layout
  bullet below — but the instrument must not depend on frame order either way).
  Reproduce any such bug against BOTH an empty profile and a seeded one before
  blaming either side.
- **…and the shared MIRROR carries state between them the same way.** One
  `luxel serve` serves every frame of a mockdiff run, so a recipe that WRITES
  leaves the next frames measuring a device nobody ships: an editor frame's
  live push is a playlist takeover on the mirror exactly as on the firmware
  (`pl_playing = false`, `crates/luxel-cli/src/serve.rs`), so by the time the
  playlist frames ran, `S4` had no playing row and a `▶ Play` primary — which
  the round-1 report duly filed as two app deltas that were nothing of the
  kind. The fix in the harness is `"replay": true` on the frame (and on the
  `sweep` screen) that needs the queue playing; the fix in your head is that a
  frame passing ALONE and failing in a full run is a shared-fixture question,
  not a flake (2026-09-20, #538).
- **A flex container blockifies its children's `display`**, so an `inline-flex`
  `.btn` inside a `display:inline-flex` wrapper computes as `flex`. A mock-vs-app
  `display` delta is usually the WRAPPER's fault, not the element the report
  names (#538, `PreviewAsChip`'s `.wrap`). A GRID container does the same, which
  is why a single-control cell in Settings is a plain block div — the mockups
  draw those inputs `inline-block` (#538, `settings/cards.css` `.fctl`).
- **Chrome ignores `line-height` on a `<select>`** — it computes `normal` however
  you set it, `font: 13px/1` included — until the control is
  `appearance: none`. With it the select takes the mockups' `.sel` metrics
  exactly, and the native chevron comes back as a background image (2026-09-20,
  #538). Drawing `<div class="sel">` instead is not the answer: the real control
  is what the keyboard and a phone know how to open.
- Since #471 **two full-screen editors are mounted at once** — the pattern
  editor (`[data-role="editor-view"]`) and the map program's screen
  (`map-editor-view`) — each with its own CodeMirror pane and its own
  `[data-role="preview"]`. An unscoped `.cm-content` / `.editor-slot` /
  `[data-role="preview"]` selector silently resolves to the pattern editor's,
  whichever screen is on top. Scope to the visible one
  (`main.editor-frame:not([hidden]) …`, the `VISIBLE_CODE` constant in
  `web/tools/e2e.mjs`) or to the screen you mean by its `data-role`. Same rule
  when adding a third screen: give its `<main>` a `data-role` and scope.
- The playground previews a `render2D` pattern on a **16x16 grid at targetFps
  60** by default (`stores/geometry.ts`, "Preview as" = Auto), while the bench
  panel is 64x64 at 100+ fps. So "wrong in the browser, fine on device" for a
  2D pattern is almost always a resolution or frame-rate assumption in the
  PATTERN, not in the harness (#285 was three of them, 2026-09-07). Since #463
  the chip can be set to `Matrix 64×64` to check that without hardware.
- **A console's Layout is `deviceGeometry()`'s, and its parameters are DEVICE
  readings only** (`web/src/lib/geometry.ts`; `stores/device.ts`'s
  `deviceLayout` is wire parsing over it and decides nothing). The same
  invariant has leaked twice through two different inputs — a persisted
  `luxel.previewAs` (#539) and `/api/status`'s `geom.source:"default"`, the
  grid the device's ENGINE fabricates for the program it was handed (#573,
  which made a 300 px strip report `18×17 matrix` and took the chip, the tile
  shapes, Settings' Projection section and the #538 filter with it). So the
  function's SIGNATURE is the guard: if you are about to give it a parameter
  the browser holds, or to read one of the browser's stores next to its call,
  that is the bug. A running program never reshapes the fixture — it may only
  caption ITSELF (`captionFor` / `effectiveFor`), or appear in the Patterns
  page's `Not for this layout` group in the playground's Auto style. Add a
  unit case per new input in `web/tests/geometry.test.mjs` (2026-09-20).
- `vite preview` binds **`localhost`**, not `0.0.0.0` — and on this box that
  resolves to `::1` first, so an ad-hoc puppeteer script that navigates to
  `http://127.0.0.1:<port>` dies with `net::ERR_CONNECTION_REFUSED` while the
  server is up and the harnesses are green. Use `http://localhost:<port>` for
  the PAGE (the mirrors, reached by `fetch`, are fine on `127.0.0.1`)
  (2026-09-20).
- Terminology Jeremy set: the hardware-bound UI is the "device console"; the
  hardware-free UI is the "playground." The playground must not offer device
  affordances (connect/disconnect controls, device badges, etc.).
- Live device pixel-stream readback in the UI was explicitly removed at
  Jeremy's request ("isn't helpful") — device mode previews via the local
  WASM engine and pushes to the device; don't reintroduce a pixel-stream
  socket. The connect handshake on page load stays, though: the device
  reports its running pattern/status before the editor opens.
- **The editor pushes only while its document IS the device's running
  program** (`livePush`, #563 — it gates `/api/code`, `/api/control`,
  `/api/events`, `/api/sensors`). Opening a pattern is BROWSING: a Library
  tile, `+ New pattern`, or `Edit` on a stored pattern that is not playing all
  open in local preview and write nothing, because a push is a playlist
  takeover on the firmware (`playlist::stop()`) and one click on a browsing
  page must not cost the user their installation. `▶ Play on device` /
  a tile's `Play` is the explicit activation. **A console's BOOT obeys the
  same rule** (#585): the autosaved working copy is always resumed in the
  editor, but it is only pushed when it is an unsaved edit of the pattern the
  device is already RUNNING — one pure function, `lib/resume.ts`. Two empty
  ids are not a match; a page load never replaces the program on the LEDs.
  Two ids, not one:
  `deviceRunningId` (device) vs `devicePatternId` (editor document). The full
  rule and the `save-state` string contract are in docs/web-architecture.md
  ("Opening a pattern is not a device action").
- `web/src` is shell (`App.svelte`) + stores (`stores/*.ts`) + pages
  (`pages/*.svelte`) + reusable components — see
  [docs/web-architecture.md](../../docs/web-architecture.md) for the store
  reference, the one-way `device → geometry → pattern` dependency rule, the
  single poll scheduler (`pollSubscribe`, never a bare `setInterval`) and the
  `notify` primitive. New app state goes in a store, not a component.
- A verb that MUTATES device state (add to playlist, activate, delete, install)
  belongs in the store as a named function, not inlined per page. Three
  surfaces wanted "add to playlist" within a day of each other and two of them
  wrote their own `playlist.update(...)` item literal — which then drifts the
  moment the item model grows a field (#470 added `kind` and `proj`). Pages own
  layout and intent; `stores/device.ts` owns what an edit MEANS.
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
- A `$:` must not derive from a **store that another reactive statement in the
  same component writes**. Svelte runs a component's reactive statements once
  per flush, in order; a store `.set()` from inside statement N ORs its dirty
  bit into the batch the fragment patch then uses, but does NOT re-run
  statement M < N, and does not schedule a second flush — so the derived value
  is stale **forever**, not for a cycle. It bit A7 (#468) the moment the
  editor's header moved into the same component as
  `matchRunningToLibrary()`, which sets `patternName` from a `$:`: the header
  read "untitled pattern" for a pattern it had just adopted a name for, and
  every harness check around it passed. Compute such values in a **function
  called from the markup** with every dependency passed as an argument — a
  markup expression is patched with those dirty bits and is always current
  (2026-09-19, #468).
- **"With every dependency passed as an argument" is the load-bearing half of
  the rule above.** A markup expression is re-patched with the dirty bits of
  what its own syntax NAMES, so `{summaryHint()}` names only the function and
  re-runs **never** — the text freezes at whatever it computed on first render
  and nothing anywhere errors. Same trap between two `$:` blocks: Svelte orders
  them by syntactic dependency, so `$: hint = costHint()` (which names nothing)
  is free to run BEFORE `$: eff = …` that `costHint` reads, and it throws on
  the first pass. Both bit A8 (#469); both are invisible to a build and a
  typecheck (2026-09-19, #469).
- **An exception thrown while a component mounts surfaces as a MODE
  regression, not a render error.** `connectDevice()` wraps its handshake in a
  try/catch, and `{#if $device}<Settings/>` mounts inside the `device.set()`
  that the catch then undoes — so one bad `$:` in a Settings card made the
  whole console fall back to playground: no Settings panel, `Save` instead of
  `Save to device`, and "connect: editor synced to the device's running
  pattern" still passing. When a console e2e suddenly looks like a playground,
  read the page's `pageerror`s before suspecting the session code
  (2026-09-19, #469).
- **`Popover.svelte` closes on ANY window click that is not inside itself or
  inside its `anchor`** — so a second opener (a status line beside the anchored
  trigger, a swatch that is not the bound element) sets `open = true` and the
  same click immediately dispatches `close`. Add `on:click|stopPropagation` to
  every opener that is not the `anchor`. The symptom is a handler that visibly
  runs while the popover never appears, and it reads like a reactivity bug
  (2026-09-19, #538).
- **Svelte orders `$:` statements by the assignments it can SEE, and a value
  assigned inside a called function is not one of them.** `$: if (x !== null)
  fill(x)` where `fill` assigns `rows`, followed *earlier in the file* by
  `$: view = rows.filter(…)`, runs `view` FIRST on the initial pass: the
  derived value is computed from the empty array and stays empty until
  something else invalidates `rows`. A grid whose items are fetched
  asynchronously hides this (the fetch invalidates later); one whose items are
  handed to it as a prop renders empty. Put the derived statement after the
  one that fills it, and say why in a comment (2026-09-19, #538,
  `components/Gallery.svelte`).
- **The app opens on the Patterns page now (#538), not in the editor.** An
  ad-hoc puppeteer script must click `[data-role="new-pattern"]` (or a tile)
  before it can touch `.cm-content` — and gate that click on
  `offsetParent !== null`, because a RELOAD restores the last-opened pattern
  into the editor and the Patterns panel is then present-but-boxless
  (2026-09-19, #538).
- **A `<select>` cannot wear the mockups' `.sel` box without
  `appearance:none`** — Chrome pins a native select's line-height to
  `normal` and ignores the `font` shorthand's — and once it is bare, the
  chevron has to come back as background art. It still sizes to its WIDEST
  option, not to the current value, which is why the editor's fps chooser
  carries an explicit `width` (`max fps` is 3px wider than the `60 fps` the
  mock draws). `.settings select` and `.editor-frame .grp .sel` are the two
  copies; keep the chevron identical (2026-09-20, #538).
- **A flex GAP is not a word space.** Splitting a run of text into spans so
  each can carry a `data-role` puts the container's `gap` between them where
  the mock has a rendered space — 6px vs 3.66px per separator, which is how
  the device chip measured 4.7px wide. Keep the text one inline run and scope
  the roles inside it (2026-09-20, #538, `components/DeviceChip.svelte`).
