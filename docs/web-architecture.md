# Web app architecture (`web/src`)

How the playground / device console is put together. Written for the web UI v2
work (Gitea epic #461); `docs/webui.md` is the historical record of how the
pre-v2 UI got there and is not a spec.

Terminology (`.claude/rules/web.md`): the hardware-bound UI is the **device
console**, the hardware-free one the **playground**. Same bundle, same
components — `stores/device.ts` decides which one you are looking at.

## Layout

```
web/src/
  App.svelte        the shell: mode, tab, editing, boot cover, header, blocked banner
  main.ts           mounts App
  app.css           global tokens + element resets (dark only)
  stores/           plain-TS Svelte stores — all app state lives here
    device.ts       session lifecycle, hardware facts, settings state, ONE poll scheduler
    geometry.ts     the preview rig + the #372 derive-once latch  (placeholder for #463)
    pattern.ts      the pattern document, the wasm host, local library, .epe + share codecs
    notify.ts       transient notes + the banner list
  pages/            one component per surface
    Library.svelte        Patterns Library and PixelBlaze Library (two variants, one file)
    DevicePatterns.svelte the device's stored library
    Playlist.svelte       transport, defaults, rows
    Settings.svelte       the card list + the visible-tab refresh
    Editor.svelte         toolbar, code pane, playback bar, right-rail inspector
    MapEditor.svelte      the map program: its engine, debugger and code pane
  settings/         one card per concern, each owning its form and its endpoint
    DeviceCard, NetworkInputCard, BrightnessCard, WifiCard,
    OutputCard, ClockCard, SyncCard, MqttCard, cards.css
  components/       reusable widgets (CodeMirror wrapper, Preview, Controls,
                    PinPanel, VarWatcher, Debugger, Gallery, PatternThumb, PlaylistRow)
  lib/              non-UI logic: device HTTP client, fetchgate, LNA classifier,
                    wasm bindings, control hints, playlist transport, audio, builtins
  flash/            a SECOND rollup entry (flash.html) — the WLED takeover installer;
                    it shares only app.css and lib/lna.ts
```

The dependency direction between stores is one-way and enforced by hand:

```
device.ts  ←  geometry.ts  ←  pattern.ts        (notify.ts depends on nothing)
```

`device.ts` therefore never writes the pattern document. `connectDevice()`
*returns* the running pattern it pulled and the caller (`pages/Editor.svelte`)
installs it. Keep it that way — it is what lets #463 replace `geometry.ts`
wholesale.

## The shell

`App.svelte` owns exactly four things: `mode` (playground vs device),
`tab` (which home surface is open), `editing` (the full-screen editor sits over
the home tab — it is not a tab), and the boot cover. It also renders the header,
the fps readout and the LNA blocked banner, and it wires page events to
`Editor` methods (`newPattern`, `loadSaved`, `loadGalleryPick`,
`openDevicePattern`, `importEpeFile`, `bootDevice`, `bootPlayground`).

Every page stays **mounted and `hidden`** when it is not the active tab, so its
state (compiled gallery tiles, CodeMirror documents, scroll position) survives
tab switching. Each page takes an `active` prop: it drives `hidden` and gates
that page's poll subscription and lazy mounts.

## The poll scheduler (`stores/device.ts`)

One `setInterval` for the whole app, ticking at 500 ms. It replaces three
ad-hoc reactive blocks that each cleared and re-created their own interval with
their gating conditions duplicated inline.

```ts
const stop = pollSubscribe(id, everyMs, fn);  // re-registering the same id replaces it
stop();                                       // unsubscribe
```

Contract:

- A subscriber's `fn` runs when at least `everyMs` has elapsed since its last
  run. Cadences need not divide the tick; they are lower bounds.
- The ticker runs **only while a device session is live**. It starts when the
  first subscriber registers with `device !== null` and stops when the last one
  leaves or the session drops, so a playground tab has no timers at all.
- `pollStopAll()` tears everything down (app teardown).

Registered cadences today:

| id | cadence | who subscribes | what it refreshes |
|---|---|---|---|
| `status` | 1 Hz | `startSessionPoll()`, once from the shell | `/api/status` → fps readout, free heap, vmerr, per-board pixel cap |
| `playlist` | 1 Hz | `pages/Playlist.svelte` while its tab is open | `/api/playlist`, reconciled against the optimistic transport intent (#431) |
| `settings` | 0.5 Hz | `pages/Settings.svelte` while its tab is open | `/api/status` live field, `/api/mqtt`, `/api/sync`, `/api/clock` |

`/api/output` is deliberately **not** polled: it is a form, and re-reading it
under the user's fingers would fight their edits. It is read once when the
Settings tab becomes visible, and re-read after every write.

Do not add a bare `setInterval` to a component. Subscribe here instead — the
device serves from a tiny connection pool and every extra poll competes with
the UI's own fetches (docs/tools.md, `panel-load-bench`).

## The notify primitive (`stores/notify.ts`)

Nine independent `*Note` strings, each with its own `setTimeout`, became one
thing:

```ts
note("save", "saved to device", 3000);  // ttl 0 = sticky until something replaces it
clearNote("mic");
$notes.save                              // read in markup
```

Channels: `save`, `share`, `map`, `mic`, `wifi`, `mqtt`, `datapin`, `ap`,
`palette`. A channel is a *surface*, not a message — the component that renders
`$notes.<channel>` owns where it appears and what `data-role` it carries.

`banners` is the longer-lived list for conditions rather than events
(`setBanner(id, {level, text, role} | null)`, keyed upsert, insertion-ordered).
The editor's compile/runtime/capacity banners are still derived state with
bespoke markup and stay where they are.

## Store reference

`stores/device.ts` — `device`, `deviceBase`, `isPlayground`, `mode`,
`deviceError`, `deviceBlocked`, `devicePixels`, `pixelMax`, `deviceHeapFree`,
`deviceEngineHeap`, `deviceVmerr`, `deviceFps`, `deviceOutFps`,
`deviceRescanHz`, `deviceMap`, `devicePatterns`, `brightness`,
`brightnessMax`, `deviceProtocol`, `protocolOptions`, `dataPin*`, `wifi*`,
`mqtt*`, `outputStatus`, `palette*`, `clockStatus`, `syncStatus`, `netLive`,
`playlist`; functions `connectDevice`, `detectDeviceBase`, `refresh*`,
`queuePlaylistSave`, `markTransport`, `installDeviceMapCoords`,
`installDeviceGridMap`, `clearDeviceMap`, `pollSubscribe`, `pollStopAll`,
`startSessionPoll`.

`stores/geometry.ts` — `layout`, `pixelTotal`, `pixelCount()`, `deriveRig()`,
`cubeLattice()`, `markPatternLoaded()`, `markSourcePasted()`,
`markRigChosen()`, `takeRigDerivePending()`. **Placeholder.** It is today's
strip→grid-only rig derivation moved verbatim so #463 can replace this one
file with the real Layout reconciler without touching a consumer.

`stores/pattern.ts` — `luxel`, `loadLuxel()`, `source`, `dirty`,
`patternName`, `exampleName`, `devicePatternId`, `controlValues`, `hints`,
`mapSrc`, `NEW_PATTERN`, `previewFps`, `runtimeError`, `saved`,
`saveToLocalLibrary`, `deleteFromLocalLibrary`, `findSaved`, `startAutosave`,
`stopAutosave`, `loadWorkingCopy`, `compileToBytecode`, `parseEpe`,
`exportEpe`, `encodeShare`, `decodeShare`.

`stores/notify.ts` — `notes`, `note()`, `clearNote()`, `noteStore()`,
`banners`, `setBanner()`, `clearBanners()`.

## Invariants worth not breaking

- **The preview is local; the device is a sink.** The editor compiles with the
  local wasm engine and pushes source + LXBC to `/api/code`. There is no pixel
  stream back (removed at Jeremy's request); only the connect handshake reads
  the device's running pattern.
- **`data-role` attributes are the e2e contract.** `web/tools/{e2e,device-e2e,
  maxpixels-e2e,sync-e2e,flash-e2e}.mjs` drive them. Moving markup between
  components is fine; renaming or dropping a role is not, without updating the
  harness in the same commit.
- **Bundle shape is load-bearing** (Gitea #92): `dist/index.html` must emit one
  `<script>`, one stylesheet and no `modulepreload`. `cssCodeSplit: false` is
  what lets a page component or a plain `.css` import (e.g.
  `settings/cards.css`) land in that single stylesheet.
- **`$:` only tracks what appears in its own syntax.** Name every dependency in
  the block, not just inside the function it calls.
- `window.prompt` / `window.confirm` are still the naming and confirmation
  affordances (8 sites). #472 replaces them with in-app dialogs.
