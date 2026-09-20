<script lang="ts">
  // The shell: mode (playground / device console), which home tab is open,
  // whether the full-screen editor is over it, the boot cover, the header and
  // the browser-blocked banner. Everything else lives in ./stores (state) and
  // ./pages (surfaces) — see docs/web-architecture.md.
  import { onDestroy, onMount } from "svelte";
  import { get } from "svelte/store";
  import DeviceChip from "./components/DeviceChip.svelte";
  import Dialog from "./components/Dialog.svelte";
  import HeaderBrightness from "./components/HeaderBrightness.svelte";
  import PreviewAsChip from "./components/PreviewAsChip.svelte";
  import type { Luxel } from "./lib/luxel";
  import { parseRoute, pushRoute, replaceRoute, type Page, type Route } from "./lib/router";
  import Editor from "./pages/Editor.svelte";
  import MapEditor from "./pages/MapEditor.svelte";
  import Patterns from "./pages/Patterns.svelte";
  import Playlist from "./pages/Playlist.svelte";
  import Settings from "./pages/Settings.svelte";
  import RebootBar from "./settings/RebootBar.svelte";
  import {
    connectDevice,
    detectDeviceBase,
    device,
    deviceBase,
    deviceBlocked,
    deviceFps,
    deviceOutFps,
    deviceRescanHz,
    isPlayground,
    mode,
    pollStopAll,
    refreshPlaylist,
    startSessionPoll,
  } from "./stores/device";
  import { pixelTotal, runMapProgram, setPreviewAs } from "./stores/geometry";
  import { setBanner } from "./stores/notify";
  import {
    decodeShare,
    dirty,
    exampleName,
    loadLuxel,
    loadWorkingCopy,
    mapSrc,
    patternName,
    previewFps,
    source,
    startAutosave,
    stopAutosave,
  } from "./stores/pattern";

  // ---- navigation ----
  // Home tabs (proposal §4): `Patterns` always, `Playlist` + `Settings` on a
  // console. `Scenes` is Phase B — Gitea #480 adds ONE entry to `tabs` below.
  // The editor is NOT a tab: it opens full-screen over the home tab when you
  // pick a pattern or create one, with a back button. `tab` is the home you
  // return to.
  type Tab = "patterns" | "playlist" | "settings";
  let tab: Tab = "patterns";
  /** Full-screen editor open (over the home tab). */
  let editing = false;
  /** The map program's screen, open over everything else (A10, Gitea #471).
   *  It is not a tab either: it is reached from the Layout picker — the
   *  playground's "Preview as" chip, or Settings → LED layout on a console. */
  let mapEditing = false;
  /** What the map screen's back button says; the opener sets it. */
  let mapBackLabel = "back";
  /** First-load cover: hides the app until we've decided playground vs device
   *  (and, on a device, loaded its running pattern) so nothing flashes first. */
  let booting = true;
  let bootLabel = "loading…";
  /** The "PixelBlaze Library" tab browses the scraped corpus (original
   *  Pixelblaze community patterns). It's a local-only convenience: the tab
   *  only exists when tools/gen-gallery.mjs found a populated corpus/ and
   *  wrote public/pixelblaze-library.json — which is a BUILD-time fact
   *  (`__HAS_PIXELBLAZE_LIBRARY__`, vite.config.ts), not something to probe
   *  for over the wire. It is also a playground affordance: a device bundle
   *  never carries the file, so a console does not ask for it at all
   *  (Gitea #564). Decided once the device probe has answered, below. */
  let hasPixelblazeLibrary = false;

  let editor: Editor;

  /** The tab strip, data-driven so Scenes (#480) is one more entry. */
  $: tabs = [
    { id: "patterns" as const, label: "Patterns", show: true },
    { id: "playlist" as const, label: "Playlist", show: $device !== null },
    { id: "settings" as const, label: "Settings", show: $device !== null },
  ].filter((t) => t.show);

  /** The label/target the editor's back button returns to. */
  $: backLabel = tabs.find((t) => t.id === tab)?.label ?? "Patterns";

  // ---- the URL ----
  // One fragment per screen (lib/router.ts) so a reload reopens what was
  // open — Jeremy's ask for Settings, 2026-09-19. The shell owns it because
  // the shell owns `tab`/`editing`/`mapEditing`; nothing else writes it
  // except the share link, which is also a fragment and is left alone.

  /** Which page the shell's state IS, in route terms. */
  $: currentPage = (mapEditing ? "map" : editing ? "editor" : tab) as Page;

  /** The page the URL last named, so nothing is pushed twice. */
  let urlPage: Page | "" = "";
  /** Nothing writes the URL until boot has settled on a screen. */
  let routing = false;

  $: if (routing) syncUrl(currentPage);
  function syncUrl(page: Page): void {
    if (page === urlPage) return;
    urlPage = page;
    pushRoute({ page });
  }

  /** Put the shell on the screen a route names. Tabs that need hardware are
   *  ignored without it (a `#/settings` link opened in the playground lands
   *  on Patterns rather than on a page that cannot exist). */
  function applyRoute(r: Route): void {
    urlPage = r.page;
    if (r.page === "map") {
      mapEditing = true;
      return;
    }
    mapEditing = false;
    if (r.page === "editor") {
      editing = true;
      return;
    }
    editing = false;
    tab = r.page === "patterns" || $device !== null ? r.page : "patterns";
    if (tab === "playlist") void refreshPlaylist();
  }

  function onPopState(): void {
    const r = parseRoute(location.hash);
    if (r) applyRoute(r);
  }

  /** What the status-bar counter says, and what it is allowed to claim.
   *  Connected: the DEVICE's own rate — `out_fps` on a pipelined HUB75 board
   *  (frames the panel displayed), else `fps` (a strip renders and writes in
   *  the same loop, so there render == wire). `out_fps` has been bounded by
   *  `rescan_hz` since #394, so it is shown as measured rather than clamped
   *  to the ceiling; the ceiling goes in the tooltip. Not connected: the
   *  browser preview loop, which is the only frame rate a playground has. */
  $: fpsReadout = !$device
    ? {
        text: `${$previewFps.toFixed(0)} fps`,
        title: "local preview loop in this browser tab",
      }
    : $deviceOutFps > 0
      ? {
          text: `device ${$deviceOutFps} fps (panel)`,
          title:
            `${$deviceOutFps} fps displayed by the panel (out_fps)` +
            ($deviceRescanHz ? `, panel rescan ${$deviceRescanHz} Hz` : "") +
            ` — device render loop ${$deviceFps} fps, local preview ${$previewFps.toFixed(0)} fps`,
        }
      : {
          text: `device ${$deviceFps} fps`,
          title: `${$deviceFps} fps rendered by the device — local preview ${$previewFps.toFixed(0)} fps`,
        };

  /** Open the editor full-screen over the Patterns page (where every pattern
   *  is opened from since #467). */
  function openEditor(): void {
    tab = "patterns";
    editing = true;
  }

  /**
   * THE route to the map program's screen (A10, Gitea #471) — the shell owns
   * it because every entry point is somewhere else: the playground's "Preview
   * as" chip picking "Custom map program", the console's Settings → LED layout
   * link, and the pattern editor's "Map program ›" while the Layout is custom.
   * A8 (#469) calls this from the real LED-layout card.
   */
  export function openMapEditor(from = backLabel): void {
    mapBackLabel = from;
    mapEditing = true;
  }

  function onDrop(e: DragEvent): void {
    e.preventDefault();
    const f = e.dataTransfer?.files?.[0];
    if (f && /\.(epe|json)$/i.test(f.name)) void editor.importEpeFile(f);
  }

  onMount(async () => {
    // Probe for a device in parallel with the wasm load, so we can go straight
    // into device mode without ever flashing the playground first.
    const deviceProbe = detectDeviceBase();
    startSessionPoll(); // 1 Hz while a session is live; the scheduler idles otherwise
    let lx: Luxel;
    try {
      lx = await loadLuxel();
    } catch (e) {
      setBanner("load-failure", { level: "error", text: `failed to load luxel.wasm: ${String(e)}` });
      booting = false;
      return;
    }
    // In-progress work wins: a share link's pattern, else the autosaved
    // working copy. The editor opens on it (resume — never lose edits). A
    // device, though, only resumes it when it has *unsaved changes*; a clean
    // copy defers to whatever pattern is actually running on the device.
    const shared = await decodeShare(location.hash);
    let sharedMap = false;
    if (shared) {
      source.set(shared.source);
      if (shared.mapSrc) {
        // A pre-#463 link that carried a map program. Links no longer ship
        // one (a map is the Layout's, not the pattern's), but the old ones
        // still work: the map becomes this playground's Layout choice. The
        // program is run headlessly here — since A10 (#471) the map editor is
        // a screen, and a share link must not have to open one.
        mapSrc.set(shared.mapSrc);
        setPreviewAs({ mode: "map", pixels: get(pixelTotal) });
        sharedMap = true;
      }
      exampleName.set("");
      patternName.set("shared pattern");
    }
    let hadWip = shared !== null;
    let wipDirty = shared !== null; // a shared link is itself an unsaved edit to resume
    if (!shared) {
      const wc = loadWorkingCopy();
      if (wc) {
        source.set(wc.source);
        patternName.set(wc.patternName);
        exampleName.set(wc.exampleName);
        dirty.set(wc.dirty);
        hadWip = true; // the playground always resumes the last working copy
        wipDirty = wc.dirty; // the device only resumes it if it's genuinely dirty
      }
    }
    startAutosave();
    // a share link never auto-connects to a device
    const base = shared ? null : await deviceProbe;
    // The corpus tab, decided without a request (#564). Gated on the mode as
    // well as on the build: an asset bundle packed from a dev tree that HAD a
    // corpus would otherwise still ask the device for a file it never ships.
    hasPixelblazeLibrary = __HAS_PIXELBLAZE_LIBRARY__ && base === null;

    if (base !== null) {
      // Device mode: keep the boot cover up (with device-aware text) through
      // the whole handshake so the running pattern is loaded before anything
      // shows.
      bootLabel = "opening the pattern running on the device…";
      deviceBase.set(base);
      // The console OPENS ON PATTERNS → On device, with the running tile lit
      // (Jeremy, 2026-09-19 — it used to drop straight into the editor). The
      // handshake below still pulls the running pattern into the working
      // copy, so opening the editor afterwards is instant and already on it.
      tab = "patterns";
      editing = false;
      await editor.bootDevice((pull) => connectDevice(base, pull), wipDirty);
    } else {
      if (sharedMap) runMapProgram(lx, get(mapSrc), get(pixelTotal));
      editor.bootPlayground();
      editing = hadWip; // resume in the editor if there was work in progress
    }
    // The URL wins over the boot default: a reload reopens the screen that
    // was open. A share link is a fragment too, and it is NOT a route — it
    // has already been decoded above, so leave its fragment in the bar.
    const route = shared ? null : parseRoute(location.hash);
    if (route) {
      applyRoute(route);
    } else if (!shared) {
      // `currentPage` is reactive and has not been recomputed yet inside this
      // handler — read the state directly.
      const bootPage: Page = mapEditing ? "map" : editing ? "editor" : tab;
      replaceRoute({ page: bootPage });
      urlPage = bootPage;
    }
    routing = true;
    booting = false;
  });

  onDestroy(() => {
    pollStopAll();
    stopAutosave();
  });
</script>

<!-- back/forward between screens (lib/router.ts) -->
<svelte:window on:popstate={onPopState} />

<div
  class="shell"
  data-mode={$mode}
  data-tab={tab}
  role="application"
  on:drop={onDrop}
  on:dragover|preventDefault
>
  {#if booting}
    <!-- first-load cover: nothing renders behind it, so the playground never
         flashes before we switch into device mode + load the running pattern -->
    <div class="boot" data-role="boot">
      <span class="spinner"></span>
      <span class="boot-label" data-role="boot-label">{bootLabel}</span>
    </div>
  {/if}
  <!-- The shell header exists only over a HOME tab. An editor screen carries
       its own header (mockup S2) — Jeremy, 2026-09-19: "when editing a file,
       'luxel' and other things in the header bar hide". The device chip that
       used to live here moves into the editor's header with it
       (components/DeviceChip.svelte). -->
  {#if !editing && !mapEditing}
    <header class="hdr">
      <div class="hdrtop">
        <!-- the word `luxel`, and on a console nothing after it: WHICH device
             is the chip's job (mockup S1), not a URL in the wordmark -->
        <span class="wordmark"
          >luxel{#if $isPlayground}&nbsp;<span>playground</span>{/if}</span
        >
        {#if !$isPlayground}
          <span class="slot chip"><DeviceChip /></span>
        {/if}

        <span class="spacer"></span>

        <!-- What this app is rendering through (#463). The console states the
             device's own Layout in the chip above; the playground offers the
             chip that chooses one. -->
        {#if $isPlayground}
          <span class="slot ctl">
            <PreviewAsChip on:openmap={() => openMapEditor(backLabel)} />
          </span>
        {:else}
          <span class="slot ctl bri"><HeaderBrightness /></span>
        {/if}

        <span class="fps mono" data-role="fps" title={fpsReadout.title}>{fpsReadout.text}</span>
      </div>

      <nav class="tabs" data-role="tabs">
        {#each tabs as t (t.id)}
          <button
            data-role={`tab-${t.id}`}
            class="tab"
            class:active={tab === t.id}
            on:click={() => {
              tab = t.id;
              if (t.id === "playlist") void refreshPlaylist();
            }}
          >
            {t.label}
          </button>
        {/each}
      </nav>
    </header>
  {/if}

  <!-- Browser-blocked device connection (#162). Not an error the app can
       retry: this page is https, the device is plain http, and Chromium's
       Local Network Access policy has to allow that combination. Say so, and
       name the routes around it — the device serves this same console over
       http itself. Spans every tab, so it's visible whether the user landed
       in the editor or on a device tab. -->
  {#if $deviceBlocked}
    <div class="blocked-bar" data-role="device-blocked" role="alert">
      <strong>Your browser blocked this page from reaching the device.</strong>
      This copy of the console is served over <code>https</code> and
      {#if $deviceBase}<code>{$deviceBase}</code>{:else}the device{/if} speaks plain
      <code>http</code>, so Chromium has to grant Local Network Access first. Allow it if you get
      the prompt and reload — otherwise:
      <ul>
        <li>
          {#if $deviceBase}
            <a href={$deviceBase} data-role="device-blocked-link">open the console from the device</a>
          {:else}
            open the console from the device
          {/if}
          — it serves this same UI over plain http, with no permission involved.
        </li>
        <li>or host this UI on a plain-http LAN address, which has none of these restrictions.</li>
      </ul>
    </div>
  {/if}

  <!-- the one modal host: confirmations and the share-link fallback
       (stores/dialog.ts; naming is inline in the editor header since #468) -->
  <Dialog />

  <!-- The editor owns its own header (A7, #468): back, the inline-editable
       name, the save state, Save and the ⋯ menu are the DOCUMENT's, and the
       document is the editor's. The shell only tells it where back goes. -->
  <Editor
    bind:this={editor}
    active={editing && !mapEditing}
    {backLabel}
    on:open={() => (editing = true)}
    on:back={() => (editing = false)}
    on:openmap={() => openMapEditor("Editor")}
  />

  <!-- The map program's screen (A10, #471): geometry, not a pattern, so it is
       a peer of the editor rather than something inside it. Mounted once and
       kept — its document, breakpoints and computed points survive leaving. -->
  <MapEditor
    active={mapEditing}
    backLabel={mapBackLabel}
    on:back={() => (mapEditing = false)}
  />

  <Patterns
    active={!editing && !mapEditing && tab === "patterns"}
    {hasPixelblazeLibrary}
    on:new={() => {
      openEditor();
      editor.newPattern();
    }}
    on:openSaved={(e) => {
      openEditor();
      editor.loadSaved(e.detail);
    }}
    on:pick={(e) => {
      openEditor();
      editor.loadGalleryPick(e.detail);
    }}
    on:openDevice={(e) => {
      openEditor();
      void editor.openDevicePattern(e.detail);
    }}
    on:playDevice={(e) => {
      // Play WITHOUT opening the editor: it activates the pattern on the
      // device and adopts it as the editor's document, so the running marker
      // and the editor agree. `openDevice` above is the other half of the
      // #563 split — it opens the same pattern and touches nothing.
      void editor.playDevicePattern(e.detail);
    }}
  />

  {#if !$isPlayground}
    <Playlist active={!editing && !mapEditing && tab === "playlist"} />
  {/if}

  <!-- "stored, but not running yet" — pinned to the bottom of the VIEWPORT on
       every screen until the device reboots (#538). Here rather than inside
       Settings: the user changes a data pin and walks off to Patterns. -->
  <RebootBar />

  {#if $device}
    <Settings
      active={!editing && !mapEditing && tab === "settings"}
      on:navigate={(e) => (tab = e.detail)}
      on:openmap={() => openMapEditor("Settings")}
      on:pixelchange={() => editor.clearPreview()}
    />
  {/if}
</div>

<style>
  .shell {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  /* mockup S1: 44px, `padding:0 16px`, one row —
     wordmark · device chip · tabs · spacer · controls · fps */
  .hdr {
    display: flex;
    align-items: center;
    gap: 16px;
    height: 44px;
    padding: 0 16px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-panel);
  }

  /* On a wide screen the header is ONE row, so this wrapper dissolves and its
     children become the header's own flex items (ordered below). At phone
     width it becomes the first of the two rows (mockup S1c) — which is the
     only reason it exists. */
  .hdrtop {
    display: contents;
  }

  .wordmark {
    order: 1;
    font-size: 14px;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--accent);
    white-space: nowrap;
  }

  /* "playground" after the wordmark (mockup S5); a console says which DEVICE
     in the chip instead, and puts nothing here */
  .wordmark span {
    color: var(--text);
  }

  .slot {
    display: inline-flex;
    align-items: center;
  }

  .chip {
    order: 2;
    min-width: 0;
  }

  .tabs {
    order: 3;
    display: flex;
    align-self: stretch;
    margin-left: 8px;
  }

  .spacer {
    order: 4;
    flex: 1;
  }

  .ctl {
    order: 5;
  }

  .fps {
    order: 6;
    font: 11.5px/1 var(--mono);
    color: var(--text-dim);
    white-space: nowrap;
  }

  .tab {
    display: flex;
    align-items: center;
    padding: 0 12px;
    background: transparent;
    border: none;
    /* flush with the header's own bottom border (mockup S1) */
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    border-radius: 0;
    color: var(--text-dim);
    font-size: 13px;
    cursor: pointer;
  }

  .tab:hover {
    color: var(--text);
  }

  /* the active tab is BRIGHTER, not amber — the amber is the underline
     (mockup S1; Jeremy, 2026-09-19) */
  .tab.active {
    color: var(--text);
    border-bottom-color: var(--accent);
  }

  /* mockup S1c: two rows — [wordmark · chip · spacer · fps] then a scrolling
     tab strip. */
  @media (max-width: 600px) {
    .hdr {
      flex-direction: column;
      align-items: stretch;
      height: auto;
      gap: 0;
      padding: 0;
    }

    .hdrtop {
      display: flex;
      align-items: center;
      gap: 10px;
      height: 40px;
      padding: 0 12px;
    }

    .tabs {
      height: 38px;
      margin-left: 0;
      padding: 0 8px;
      overflow-x: auto;
    }

    /* no room for it beside the chip; Settings keeps its own */
    .bri {
      display: none;
    }
  }

  /* first-load cover over the whole app (header included) */
  .boot {
    position: fixed;
    inset: 0;
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    background: var(--bg);
    color: var(--text-dim);
    font-size: 15px;
  }

  /* Full-width, above every tab: the device is unreachable for a reason the
     user has to act on outside the app (#162). Wider and wordier than the
     .banner family on purpose — it carries instructions, not a status. */
  .blocked-bar {
    margin: 8px 12px 0;
    padding: 10px 12px;
    border: 1px solid var(--error);
    border-radius: 6px;
    background: color-mix(in srgb, var(--error) 14%, transparent);
    color: #f2c4c4;
    font-size: 13px;
    line-height: 1.5;
  }

  .blocked-bar code {
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
  }

  .blocked-bar ul {
    margin: 6px 0 0;
    padding-left: 20px;
  }

  .spinner {
    display: inline-block;
    width: 12px;
    height: 12px;
    border: 2px solid color-mix(in srgb, var(--text-dim) 40%, transparent);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: conn-spin 0.7s linear infinite;
  }

  @keyframes conn-spin {
    to {
      transform: rotate(1turn);
    }
  }
</style>
