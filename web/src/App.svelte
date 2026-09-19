<script lang="ts">
  // The shell: mode (playground / device console), which home tab is open,
  // whether the full-screen editor is over it, the boot cover, the header and
  // the browser-blocked banner. Everything else lives in ./stores (state) and
  // ./pages (surfaces) — see docs/web-architecture.md.
  import { onDestroy, onMount } from "svelte";
  import { get } from "svelte/store";
  import Dialog from "./components/Dialog.svelte";
  import PreviewAsChip from "./components/PreviewAsChip.svelte";
  import { gatedFetch } from "./lib/fetchgate";
  import Editor from "./pages/Editor.svelte";
  import Patterns from "./pages/Patterns.svelte";
  import Playlist from "./pages/Playlist.svelte";
  import Settings from "./pages/Settings.svelte";
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
  import { layoutName, pixelTotal, setPreviewAs } from "./stores/geometry";
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
  /** First-load cover: hides the app until we've decided playground vs device
   *  (and, on a device, loaded its running pattern) so nothing flashes first. */
  let booting = true;
  let bootLabel = "loading…";
  /** The "PixelBlaze Library" tab browses the scraped corpus (original
   *  Pixelblaze community patterns). It's a local-only convenience: the tab
   *  only exists when tools/gen-corpus-gallery.mjs found a populated corpus/
   *  and wrote public/pixelblaze-library.json (see onMount probe). */
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
    try {
      await loadLuxel();
    } catch (e) {
      setBanner("load-failure", { level: "error", text: `failed to load luxel.wasm: ${String(e)}` });
      booting = false;
      return;
    }
    // Probe for a local corpus gallery; its tab appears only when present.
    void gatedFetch(`${import.meta.env.BASE_URL}pixelblaze-library.json`)
      .then((r) => (r.ok ? r.json() : null))
      .then((list) => {
        hasPixelblazeLibrary = Array.isArray(list) && list.length > 0;
      })
      .catch(() => {});
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
        // still work: the map becomes this playground's Layout choice.
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

    if (base !== null) {
      // Device mode: keep the boot cover up (with device-aware text) through
      // the whole handshake so the running pattern is loaded before anything
      // shows.
      bootLabel = "opening the pattern running on the device…";
      deviceBase.set(base);
      tab = "patterns"; // the editor's back button lands on Patterns
      editing = true;
      await editor.bootDevice((pull) => connectDevice(base, pull), wipDirty);
    } else {
      editor.bootPlayground(sharedMap);
      editing = hadWip; // resume in the editor if there was work in progress
    }
    booting = false;
  });

  onDestroy(() => {
    pollStopAll();
    stopAutosave();
  });
</script>

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
  <header>
    {#if !editing}
      <span class="wordmark">
        luxel <span class="dim">{$isPlayground ? "playground" : ($device?.base ?? $deviceBase) || "device"}</span>
      </span>
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
    {/if}

    <span class="spacer"></span>

    <!-- What this app is rendering through (#463). The console states the
         device's own Layout; the playground offers the chip that chooses one. -->
    {#if $isPlayground}
      <PreviewAsChip />
    {:else}
      <span class="layout-chip" data-role="layout-chip" title="the device's LED layout">
        <span class="dot" class:live={$device !== null}></span>
        <span class="dev-name">{($device?.base ?? $deviceBase) || "device"}</span>
        <span class="dim">·</span>
        <span class="mono" data-role="layout-label">{$layoutName}</span>
      </span>
    {/if}

    <span class="mono dim" data-role="fps" title={fpsReadout.title}>{fpsReadout.text}</span>
  </header>

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
    active={editing}
    {backLabel}
    on:open={() => (editing = true)}
    on:back={() => (editing = false)}
  />

  <Patterns
    active={!editing && tab === "patterns"}
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
      // Play WITHOUT opening the editor: the same call, minus `editing`. It
      // activates the pattern on the device and adopts it as the editor's
      // document, so the running marker and the editor agree.
      void editor.openDevicePattern(e.detail);
    }}
  />

  {#if !$isPlayground}
    <Playlist active={!editing && tab === "playlist"} />
  {/if}

  {#if $device}
    <Settings
      active={!editing && tab === "settings"}
      on:navigate={(e) => (tab = e.detail)}
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

  header {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border);
    background: var(--bg-panel);
    flex-wrap: wrap;
  }

  .wordmark {
    font-weight: 700;
    letter-spacing: 0.04em;
    color: var(--accent);
  }

  .dim {
    color: var(--text-dim);
  }

  .tabs {
    display: flex;
    gap: 2px;
  }

  .tab {
    background: transparent;
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    padding: 6px 12px;
    color: var(--text-dim);
    font-size: 13px;
    cursor: pointer;
  }

  .tab:hover {
    color: var(--text);
  }

  .tab.active {
    color: var(--accent);
    border-bottom-color: var(--accent);
  }

  .spacer {
    flex: 1;
  }

  /* the console's "● luxel-f6b0a8 · 64×64 matrix" (proposal §4) */
  .layout-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-dim);
  }

  .layout-chip .dev-name {
    color: var(--text);
  }

  .layout-chip .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--text-dim);
  }

  .layout-chip .dot.live {
    background: #4caf50;
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
