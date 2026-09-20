<script lang="ts">
  // The device playlist (proposal §5.4, mockups S4/S4b).
  //
  // Transport is ONE group at the left — the primary action, the step
  // buttons and the now-playing readout together, so the controls sit beside
  // the thing they control. The two settings that are not transport (default
  // duration, crossfade) live at the right, and the destructive one (Clear)
  // is in ⋯ where a destructive action belongs.
  //
  // Everything below is rows: `components/PlaylistRow.svelte`. `+ Add` at the
  // foot opens `components/PatternPicker.svelte` — the ONE picker, which
  // Phase B extends with a Scenes section rather than replacing.
  import { onDestroy } from "svelte";
  import PatternPicker from "../components/PatternPicker.svelte";
  import Popover from "../components/Popover.svelte";
  import PlaylistRow from "../components/PlaylistRow.svelte";
  import {
    addToPlaylist,
    device,
    deviceError,
    devicePatterns,
    markTransport,
    playlist,
    pollSubscribe,
    queuePlaylistSave,
    refreshPlaylist,
  } from "../stores/device";
  import { confirm } from "../stores/dialog";
  import { luxel } from "../stores/pattern";

  /** The tab is the visible one — gates the 1 Hz follow poll. */
  export let active = false;

  // Follow the device while the Playlist tab is open (light status poll, not
  // pixel streaming) so the current entry highlights as it advances — and so
  // the transport recovers from any state it read wrong. Deliberately NOT
  // gated on `playlist.playing`: that gate made one early `playing: false`
  // read permanent, since the only thing that could correct it was the poll
  // it had just disabled (Gitea #431).
  //
  // A second subscription ticks the now-playing progress bar. It is a
  // subscription rather than a `setInterval` of its own for the reason in
  // docs/web-architecture.md: the device serves from a tiny connection pool
  // and the ONE scheduler is what keeps the app's timers countable.
  let unsubscribe: (() => void) | undefined;
  let unsubscribeTick: (() => void) | undefined;
  $: {
    unsubscribe?.();
    unsubscribeTick?.();
    unsubscribe = undefined;
    unsubscribeTick = undefined;
    if (active && $device) {
      unsubscribe = pollSubscribe("playlist", 1000, refreshPlaylist);
      unsubscribeTick = pollSubscribe("playlist-progress", 500, () => {
        now = Date.now();
      });
    }
  }
  onDestroy(() => {
    unsubscribe?.();
    unsubscribeTick?.();
  });

  /** Total auto-advance run time (manual items — effective 0s — are excluded);
   *  also whether any item is manual. */
  $: playlistTotalSec = $playlist.items.reduce(
    (s, it) => s + Math.max(0, it.sec ?? $playlist.defaultSec),
    0,
  );
  $: playlistHasManual = $playlist.items.some((it) => (it.sec ?? $playlist.defaultSec) <= 0);

  const fmtDuration = (sec: number): string => {
    if (sec <= 0) return "0s";
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return m > 0 ? `${m}m ${s}s` : `${s}s`;
  };

  /** `0:05` — the now-playing readout's clock form. */
  const clock = (sec: number): string => {
    const s = Math.max(0, Math.floor(sec));
    return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  };

  // ---- now playing ----
  //
  // The device reports WHICH item is playing, not how far into it it is, and
  // an elapsed field would be a wire change for a progress bar. So the bar is
  // timed locally from the moment the index (or the playing flag) last
  // changed, and re-zeroes on every advance the poll reports — a fresh page
  // opened mid-item under-reads until the next advance, and says nothing
  // false in the meantime because the item's own duration bounds it. An
  // `elapsedSec` on the wire would make it exact: Gitea #509.
  let now = Date.now();
  let itemStart = Date.now();
  let lastMark = "";
  $: {
    const mark = `${$playlist.playing}:${$playlist.index}`;
    if (mark !== lastMark) {
      lastMark = mark;
      itemStart = Date.now();
    }
  }
  $: nowItem = $playlist.playing ? $playlist.items[$playlist.index] : undefined;
  $: nowSec = nowItem === undefined ? 0 : (nowItem.sec ?? $playlist.defaultSec);
  $: elapsed = Math.min((now - itemStart) / 1000, nowSec > 0 ? nowSec : Infinity);
  $: progress = nowSec > 0 ? Math.max(0, Math.min(1, elapsed / nowSec)) : 0;

  /** A playlist item whose pattern was deleted from the device. */
  const itemMissing = (id: string, pats: typeof $devicePatterns): boolean =>
    !pats.some((p) => p.id === id);
  /** Source for a playlist item's thumbnail/params, from the device library. */
  const itemSource = (id: string, pats: typeof $devicePatterns): string | undefined =>
    pats.find((p) => p.id === id)?.source;

  // ---- the ⋯ menu (Clear lives here) ----
  let menuOpen = false;
  /** The ⋯ button the menu hangs off (components/Popover.svelte). */
  let moreBtn: HTMLElement;
  let pickerOpen = false;

  function removePlaylistItem(i: number): void {
    playlist.update((pl) => ({ ...pl, items: pl.items.filter((_, j) => j !== i) }));
    queuePlaylistSave();
  }

  async function clearPlaylist(): Promise<void> {
    menuOpen = false;
    const n = $playlist.items.length;
    if (n === 0) return;
    const ok = await confirm({
      title: "Clear the playlist?",
      body: `All ${n} item${n === 1 ? "" : "s"} are removed from the device's playlist. The patterns themselves are kept.`,
      confirmLabel: "Clear",
      danger: true,
    });
    if (!ok) return;
    playlist.update((pl) => ({ ...pl, items: [] }));
    queuePlaylistSave();
  }

  function movePlaylistItem(i: number, dir: number): void {
    const j = i + dir;
    const items = [...$playlist.items];
    const a = items[i];
    const b = items[j];
    if (a === undefined || b === undefined) return;
    items[i] = b;
    items[j] = a;
    playlist.update((pl) => ({ ...pl, items }));
    queuePlaylistSave();
  }

  /** drag-to-reorder: index the grip drag started from. */
  let playlistDragFrom = -1;
  function dropPlaylistItem(to: number): void {
    const from = playlistDragFrom;
    playlistDragFrom = -1;
    if (from < 0 || from === to) return;
    const items = [...$playlist.items];
    const [moved] = items.splice(from, 1);
    if (moved === undefined) return;
    items.splice(to, 0, moved);
    playlist.update((pl) => ({ ...pl, items }));
    queuePlaylistSave();
  }

  /** A value edited on the row that is PLAYING goes to the device at once as
   *  well as into the saved playlist — otherwise you would be tuning a slider
   *  while the fixture ignores you until the next advance. */
  function pushLive(i: number, detail: { name: string; values: number[] }): void {
    if (!$playlist.playing || $playlist.index !== i) return;
    void $device?.setControl(detail.name, detail.values);
  }

  function onPick(e: CustomEvent<{ id: string; kind: "pattern" | "scene" }>): void {
    pickerOpen = false;
    // no values: a freshly added item runs the pattern's own defaults until
    // you open its chip and tune it (D6 — values live on the item)
    addToPlaylist(e.detail.id);
  }

  function onDefaultSecChange(e: Event): void {
    const v = (e.target as HTMLInputElement).value.trim();
    playlist.update((pl) => ({
      ...pl,
      defaultSec: v === "" ? 0 : Math.max(0, Math.round(Number(v) || 0)),
    }));
    queuePlaylistSave();
  }

  function onCrossfadeChange(e: Event): void {
    const v = (e.target as HTMLInputElement).value.trim();
    // field is in seconds; store ms
    const ms = v === "" ? 0 : Math.max(0, Math.round((Number(v) || 0) * 1000));
    playlist.update((pl) => ({ ...pl, crossfadeMs: ms }));
    queuePlaylistSave();
  }

  async function playlistPlay(): Promise<void> {
    markTransport(true);
    await $device?.playlistPlay(0);
    void refreshPlaylist();
  }
  async function playlistStop(): Promise<void> {
    markTransport(false);
    await $device?.playlistStop();
    void refreshPlaylist();
  }
  async function playlistNext(): Promise<void> {
    await $device?.playlistNext();
    void refreshPlaylist();
  }
  async function playlistPrev(): Promise<void> {
    await $device?.playlistPrev();
    void refreshPlaylist();
  }
</script>

<div class="playlist-tab" data-role="playlist-panel" hidden={!active}>
  <div class="transport">
    <!-- §5.7: absent, never disabled. There is nothing to play until the queue
         has an item, so the transport carries the empty state as TEXT instead
         of a dimmed dead button (Gitea #529). -->
    {#if $device && $playlist.items.length > 0}
      <span class="group">
        {#if $playlist.playing}
          <button class="btn primary" data-role="pl-stop" on:click={playlistStop}>■ Stop</button>
          <button class="icon" data-role="pl-prev" title="previous" aria-label="previous"
            on:click={playlistPrev}>⏮</button
          >
          <button class="icon" data-role="pl-next" title="next" aria-label="next"
            on:click={playlistNext}>⏭</button
          >
        {:else}
          <button class="btn primary" data-role="pl-play" on:click={playlistPlay}>▶ Play</button>
        {/if}
      </span>
    {:else if $device}
      <span class="dim tiny" data-role="pl-transport-empty">playlist empty</span>
    {/if}

    {#if nowItem}
      <span class="nowplaying" data-role="pl-now">
        <span class="np1">
          <b>{nowItem.name || nowItem.id}</b>
          {#if nowSec > 0}<span class="dim t">{clock(elapsed)} / {clock(nowSec)}</span>{/if}
        </span>
        <span class="prog" data-role="pl-progress" style="--p:{progress * 100}%"></span>
      </span>
    {/if}

    <span class="spacer"></span>

    <span class="group defaults">
      <span class="dim tiny">Default</span>
      <input
        class="num wide"
        data-role="pl-default-sec"
        type="number"
        min="0"
        placeholder="manual"
        title="seconds per item (blank/0 = manual advance); items can override"
        value={$playlist.defaultSec || ""}
        on:change={onDefaultSecChange}
      />
      <span class="dim tiny">s</span>
      <span class="dim tiny cf">Crossfade</span>
      <input
        class="num"
        data-role="pl-crossfade"
        type="number"
        min="0"
        step="0.1"
        placeholder="0"
        title="seconds to blend between items (blank/0 = hard cut)"
        value={$playlist.crossfadeMs ? $playlist.crossfadeMs / 1000 : ""}
        on:change={onCrossfadeChange}
      />
      <span class="dim tiny">s</span>
      <!-- Clear acts on the items; with none, the menu would be empty, so the
           chip that OPENS it is absent too (§5.7, Gitea #529). -->
      {#if $playlist.items.length > 0}
        <span class="overflow">
          <button
            class="btn icon"
            bind:this={moreBtn}
            data-role="pl-more"
            title="more actions"
            aria-label="more actions"
            on:click={() => (menuOpen = !menuOpen)}>⋯</button
          >
          <Popover
            open={menuOpen}
            anchor={moreBtn}
            dataRole="pl-menu"
            on:close={() => (menuOpen = false)}
          >
            <button
              class="mi del"
              data-role="pl-clear"
              role="menuitem"
              on:click={() => void clearPlaylist()}
            >
              Clear playlist
            </button>
          </Popover>
        </span>
      {/if}
    </span>
  </div>

  {#if !$device}
    <p class="dim hint">device unreachable — {$deviceError || "reload to retry"}.</p>
  {:else}
    <div class="pl-list">
      {#if $playlist.items.length === 0}
        <p class="dim hint" data-role="pl-empty">
          Nothing queued yet. <strong>+ Add</strong> picks from the patterns saved on this device —
          add the same pattern more than once for different looks.
        </p>
      {:else}
        <ul class="rows">
          {#each $playlist.items as item, i (i)}
            {#if $luxel}
              <PlaylistRow
                luxel={$luxel}
                source={itemSource(item.id, $devicePatterns)}
                {item}
                defaultSec={$playlist.defaultSec}
                missing={itemMissing(item.id, $devicePatterns)}
                active={$playlist.playing && $playlist.index === i}
                first={i === 0}
                last={i === $playlist.items.length - 1}
                on:change={() => {
                  playlist.update((pl) => pl);
                  queuePlaylistSave();
                }}
                on:control={(e) => pushLive(i, e.detail)}
                on:remove={() => removePlaylistItem(i)}
                on:move={(e) => movePlaylistItem(i, e.detail)}
                on:dragstart={() => (playlistDragFrom = i)}
                on:drop={() => dropPlaylistItem(i)}
              />
            {/if}
          {/each}
        </ul>
      {/if}
      <button class="add" data-role="pl-add" on:click={() => (pickerOpen = true)}>+ Add</button>
      {#if $playlist.items.length > 0}
        <p class="foot dim" data-role="pl-total">
          {$playlist.items.length} item{$playlist.items.length === 1 ? "" : "s"} · loop ≈ {fmtDuration(
            playlistTotalSec,
          )}{playlistHasManual ? " + manual stops" : ""}
        </p>
      {/if}
    </div>
  {/if}

  <PatternPicker
    luxel={$luxel}
    open={pickerOpen}
    patterns={$devicePatterns}
    on:pick={onPick}
    on:close={() => (pickerOpen = false)}
  />
</div>

<style>
  /* one surface visible at a time; hidden ones stay mounted (state survives) */
  .playlist-tab[hidden] {
    display: none;
  }

  .playlist-tab {
    flex: 1;
    min-height: 0;
    background: var(--bg-panel);
    overflow-y: auto;
  }

  .dim {
    color: var(--text-dim);
  }

  .tiny {
    font-size: 11px;
  }

  .spacer {
    flex: 1;
  }

  .hint {
    font-size: 12px;
    margin: 2px 0 10px;
  }

  .num {
    width: 56px;
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
  }

  /* wide enough for the word "manual" — 56 px clipped the placeholder to
     "mar" at every width (Gitea #530) */
  .num.wide {
    width: 88px;
  }

  .transport {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }

  .group {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .defaults {
    gap: 6px;
  }

  .cf {
    margin-left: 8px;
  }

  .icon {
    padding: 4px 9px;
    line-height: 1;
  }

  .nowplaying {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 180px;
    max-width: 320px;
  }

  .np1 {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: 12px;
    min-width: 0;
  }

  .np1 b {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .t {
    font-family: ui-monospace, Menlo, Consolas, monospace;
    flex: none;
  }

  .prog {
    display: block;
    height: 3px;
    border-radius: 2px;
    background: var(--border);
    background-image: linear-gradient(var(--accent), var(--accent));
    background-repeat: no-repeat;
    background-size: var(--p) 100%;
    transition: background-size 0.4s linear;
  }

  .overflow {
    position: relative;
    display: inline-flex;
    margin-left: 4px;
  }

  .pl-list {
    padding: 12px 16px;
    max-width: 720px;
  }

  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .add {
    color: var(--text-dim);
    border-style: dashed;
    width: 100%;
    padding: 8px;
  }

  .add:hover {
    color: var(--text);
  }

  .foot {
    font-size: 12px;
    margin: 10px 2px 0;
  }

  /* ---- phone (D9) ---- */
  @media (max-width: 600px) {
    .transport {
      padding: 12px;
      gap: 8px;
    }

    .nowplaying {
      flex-basis: 100%;
      max-width: none;
    }

    .defaults {
      flex-basis: 100%;
      flex-wrap: wrap;
    }

    .spacer {
      display: none;
    }

    .pl-list {
      padding: 12px;
    }

    /* a touch target, on the shared button primitive (app.css) */
    .btn {
      min-height: 36px;
    }
  }
</style>
