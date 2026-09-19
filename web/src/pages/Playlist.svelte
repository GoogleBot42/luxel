<script lang="ts">
  // The device playlist: transport, defaults, and the rows. Almost entirely
  // self-contained already — it talks to the device store and PlaylistRow.
  import { onDestroy } from "svelte";
  import PlaylistRow from "../components/PlaylistRow.svelte";
  import {
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
  let unsubscribe: (() => void) | undefined;
  $: {
    unsubscribe?.();
    unsubscribe = undefined;
    if (active && $device) unsubscribe = pollSubscribe("playlist", 1000, refreshPlaylist);
  }
  onDestroy(() => unsubscribe?.());

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

  /** A playlist item whose pattern was deleted from the device. */
  const itemMissing = (id: string, pats: typeof $devicePatterns): boolean =>
    !pats.some((p) => p.id === id);
  /** Source for a playlist item's thumbnail/params, from the device library. */
  const itemSource = (id: string, pats: typeof $devicePatterns): string | undefined =>
    pats.find((p) => p.id === id)?.source;

  function removePlaylistItem(i: number): void {
    playlist.update((pl) => ({ ...pl, items: pl.items.filter((_, j) => j !== i) }));
    queuePlaylistSave();
  }

  async function clearPlaylist(): Promise<void> {
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
  <div class="lib-head">
    <span class="lib-title">Playlist</span>
    <span class="dim">plays your saved patterns in order</span>
    <span class="spacer"></span>
    <span class="pl-transport">
      {#if $playlist.playing}
        <button data-role="pl-prev" title="previous" on:click={playlistPrev}>⏮</button>
        <button data-role="pl-stop" on:click={playlistStop}>■ stop</button>
        <button data-role="pl-next" title="next" on:click={playlistNext}>⏭</button>
      {:else}
        <button
          class="primary"
          data-role="pl-play"
          disabled={!$device || $playlist.items.length === 0}
          on:click={playlistPlay}
        >
          ▶ play
        </button>
      {/if}
      {#if $playlist.items.length > 0}
        <button data-role="pl-clear" title="remove all items" on:click={() => void clearPlaylist()}>
          clear
        </button>
      {/if}
    </span>
  </div>

  <div class="pl-default">
    <span class="dim">default duration</span>
    <input
      class="num"
      data-role="pl-default-sec"
      type="number"
      min="0"
      placeholder="manual"
      value={$playlist.defaultSec || ""}
      on:change={onDefaultSecChange}
    />
    <span class="dim">seconds (blank/0 = manual advance) · items can override</span>
    {#if $playlist.items.length > 0}
      <span class="spacer"></span>
      <span class="dim" data-role="pl-total">
        {$playlist.items.length} item{$playlist.items.length === 1 ? "" : "s"} · loop ≈ {fmtDuration(
          playlistTotalSec,
        )}{playlistHasManual ? " + manual stops" : ""}
      </span>
    {/if}
  </div>

  <div class="pl-default">
    <span class="dim">crossfade</span>
    <input
      class="num"
      data-role="pl-crossfade"
      type="number"
      min="0"
      step="0.1"
      placeholder="0"
      value={$playlist.crossfadeMs ? $playlist.crossfadeMs / 1000 : ""}
      on:change={onCrossfadeChange}
    />
    <span class="dim">seconds to blend between items (blank/0 = hard cut)</span>
  </div>

  {#if !$device}
    <p class="dim hint">device unreachable — {$deviceError || "reload to retry"}.</p>
  {:else if $playlist.items.length === 0}
    <p class="dim hint" data-role="pl-empty">
      Empty. Open a saved device pattern in the editor, set its parameters, and use
      <strong>“+ Add to playlist”</strong> — add the same pattern more than once for different
      looks.
    </p>
  {:else}
    <ul class="pl-list">
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
            on:remove={() => removePlaylistItem(i)}
            on:move={(e) => movePlaylistItem(i, e.detail)}
            on:dragstart={() => (playlistDragFrom = i)}
            on:drop={() => dropPlaylistItem(i)}
          />
        {/if}
      {/each}
    </ul>
  {/if}
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

  .spacer {
    flex: 1;
  }

  .hint {
    font-size: 12px;
    margin: 2px 0;
  }

  .num {
    width: 64px;
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
  }

  .lib-head {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--border);
  }

  .lib-title {
    font-weight: 700;
    letter-spacing: 0.03em;
    color: var(--accent);
    font-size: 14px;
  }

  .pl-transport {
    display: inline-flex;
    gap: 6px;
  }

  .pl-default {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 16px;
    border-bottom: 1px solid var(--border);
  }

  .pl-list {
    list-style: none;
    margin: 0;
    padding: 12px 16px;
    max-width: 680px;
  }
</style>
