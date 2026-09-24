<script lang="ts">
  // The device playlist (proposal §5.4, mockups S4/S4b).
  //
  // Transport is ONE group at the left — the four controls, then the
  // now-playing readout, so the controls sit beside the thing they control.
  // All four are PERSISTENT: the same buttons in the same places whether the
  // queue is playing, paused or empty, because a transport that reshapes
  // itself under the pointer is what made Clear read as a flicker (#538 §A).
  // (This is the one place §5.7's absent-never-disabled rule is deliberately
  // set aside: S4 is a fixed four-button group, and a Play button that
  // disappears when the queue empties moves every control beside it.)
  //
  // The two settings that are not transport (default duration, crossfade)
  // live at the right, and the destructive one (Clear) is in ⋯.
  //
  // Everything below is rows: `components/PlaylistRow.svelte`. `+ Add` at the
  // foot opens `components/PatternPicker.svelte` — the ONE picker, which now
  // offers the LIBRARY as well as the device (#538 §F) and which Phase B
  // extends with a Scenes section rather than replacing.
  import { createEventDispatcher, onDestroy, tick } from "svelte";
  import PatternPicker from "../components/PatternPicker.svelte";
  import Popover from "../components/Popover.svelte";
  import PlaylistRow from "../components/PlaylistRow.svelte";
  import {
    addToPlaylist,
    device,
    deviceError,
    devicePatterns,
    playlist,
    playlistParked,
    playlistPreemptedBy,
    playlistPause,
    playlistResume,
    playlistStep,
    playlistStop,
    pollSubscribe,
    queuePlaylistSave,
    refreshPlaylist,
    saveAndAddToPlaylist,
    savePlaylistNow,
  } from "../stores/device";
  import { confirm } from "../stores/dialog";
  import { compileToBytecode, luxel } from "../stores/pattern";
  import { playgroundPatternId } from "../lib/sceneRender";
  import type { Scene } from "../lib/scene";
  import { listPatterns } from "../lib/store";
  import { refreshScenes, scenes } from "../stores/scenes";

  /** The tab is the visible one — gates the 1 Hz follow poll. */
  export let active = false;

  /** `Edit scene ›` on a scene row — the shell opens the scene editor, the
   *  same way the Scenes page's tiles do (App's `openScene`). */
  const dispatch = createEventDispatcher<{ openscene: string }>();

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
      unsubscribe = pollSubscribe("playlist", 1000, pollPlaylist);
      unsubscribeTick = pollSubscribe("playlist-progress", 500, () => {
        now = Date.now();
      });
    }
  }
  onDestroy(() => {
    unsubscribe?.();
    unsubscribeTick?.();
  });

  /** The 1 Hz follow poll, except while a reorder is in progress: the drag
   *  measured the rows once, and a poll that replaced `items` under it would
   *  animate a hole for a row that is no longer where it was measured. A
   *  plain function, not an inline arrow in the subscribing `$:` block —
   *  naming `dragFrom` there would re-subscribe on every grab. */
  function pollPlaylist(): Promise<void> {
    return dragFrom < 0 ? refreshPlaylist() : Promise.resolve();
  }

  /** Total auto-advance run time (manual items — effective 0s — are excluded);
   *  also whether any item is manual. */
  $: playlistTotalSec = $playlist.items.reduce(
    (s, it) => s + Math.max(0, it.sec ?? $playlist.defaultSec),
    0,
  );
  $: playlistHasManual = $playlist.items.some((it) => (it.sec ?? $playlist.defaultSec) <= 0);

  /** `39 s`, `1m 5s` — the mock's footer form (`loop ≈ 39 s`). */
  const fmtDuration = (sec: number): string => {
    if (sec <= 0) return "0 s";
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return m > 0 ? `${m}m ${s}s` : `${s} s`;
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
  // `elapsedSec` on the wire would make it exact: Gitea #509 — when that
  // lands, feed it into `itemStart` here and everything downstream is right.
  let now = Date.now();
  let itemStart = Date.now();
  let lastMark = "";
  /** Seconds a just-issued seek asked for; consumed by the mark handler so
   *  the re-entry the seek causes does not reset the clock back to 0. */
  let seekSec: number | null = null;
  $: {
    const mark = `${$playlist.playing}:${$playlist.index}`;
    if (mark !== lastMark) {
      lastMark = mark;
      itemStart = Date.now() - (seekSec ?? 0) * 1000;
      seekSec = null;
    }
  }
  /** The block stays MOUNTED when stopped (S4 `.nowplaying` is always there,
   *  min-width 230px) — it just goes dim and reads the item it is parked on,
   *  so Pause/Play does not move every control beside it. While stopped the
   *  device's own `index` is frozen where it left off, so the PARK is what
   *  the readout follows (`stores/device.ts`). */
  $: nowIndex = Math.min(
    $playlist.playing ? $playlist.index : $playlistParked,
    Math.max(0, $playlist.items.length - 1),
  );
  $: nowItem = $playlist.items[nowIndex];
  $: nowSec = nowItem === undefined ? 0 : (nowItem.sec ?? $playlist.defaultSec);
  $: elapsed = !$playlist.playing
    ? 0
    : Math.min((now - itemStart) / 1000, nowSec > 0 ? nowSec : Infinity);
  $: progress = nowSec > 0 ? Math.max(0, Math.min(1, elapsed / nowSec)) : 0;
  /** Where the thumb sits: the drag position while scrubbing, else the clock. */
  $: shown = scrub === null ? progress : scrub;

  /** A playlist item whose pattern — or scene (#478) — was deleted. */
  const itemMissing = (
    it: { id: string; kind?: string },
    pats: typeof $devicePatterns,
    scs: readonly Scene[],
  ): boolean =>
    it.kind === "scene" ? !scs.some((s) => s.id === it.id) : !pats.some((p) => p.id === it.id);
  /** Source for a playlist item's thumbnail/params, from the device library. */
  const itemSource = (id: string, pats: typeof $devicePatterns): string | undefined =>
    pats.find((p) => p.id === id)?.source;
  /** The scene record a scene row composites (#482). */
  const itemScene = (it: { id: string; kind?: string }, scs: readonly Scene[]): Scene | null =>
    it.kind === "scene" ? (scs.find((s) => s.id === it.id) ?? null) : null;

  // ---- scenes (#478) ----
  //
  // The library is `stores/scenes.ts` — the ONE scene store, shared with the
  // Scenes page and the scene editor, so a scene edited there is the record
  // this page's rows composite.
  //
  // READ ON DEMAND, deliberately: this page already owns two of the four poll
  // subscriptions the app is allowed (docs/web-architecture.md), the device
  // serves ~2 connections, and scenes change when somebody edits one — not
  // while a playlist advances. So it reads when the tab comes forward, when
  // the picker opens, and when a row names a scene the library has not got.

  /** The scene ids the queue names. A row that names a scene the library does
   *  not hold asks for ONE refresh (the picker adding a scene is the usual
   *  reason), guarded so a genuinely missing record cannot re-fetch on every
   *  render. */
  $: sceneSig = $playlist.items
    .filter((it) => it.kind === "scene")
    .map((it) => it.id)
    .sort()
    .join(",");
  let sceneSigSeen: string | null = null;
  $: if (active && $device && sceneSig !== sceneSigSeen) {
    sceneSigSeen = sceneSig;
    void refreshScenes();
  }
  // a fresh visit re-reads even when the queue has not changed
  $: if (!active) sceneSigSeen = null;

  /** Layer sources for the composite thumbnails — the device's library on a
   *  console, this browser's in the playground. The same resolver the Scenes
   *  page gives its tiles. */
  function sceneLookup(id: string): string | null {
    const dev = $devicePatterns.find((p) => p.id === id);
    if (dev?.source !== undefined) return dev.source;
    for (const p of listPatterns()) if (playgroundPatternId(p.name) === id) return p.source;
    return null;
  }

  // ---- the ⋯ menu (Clear lives here) ----
  let menuOpen = false;
  /** The ⋯ button the menu hangs off (components/Popover.svelte). */
  let moreBtn: HTMLElement;
  let pickerOpen = false;
  /** The `+ Add` button the picker hangs under (S4c `.addwrap`). */
  let addBtn: HTMLElement;
  /** The picker's inline status line while a library pattern is being saved. */
  let pickerBusy = "";
  let pickerError = "";

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
    // ONE render: the rows go in the same flush the dialog closes in, and the
    // write leaves immediately rather than 400 ms later (#538 §A). The store's
    // save flag suppresses the follow poll until the POST lands, so nothing
    // comes back to repaint the list it just emptied.
    playlist.update((pl) => ({ ...pl, items: [] }));
    void savePlaylistNow();
  }

  /** Keyboard reorder — the drag handle's arrow keys (the ↑/↓ buttons the
   *  mock does not have are gone; this is the a11y path that replaces them). */
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

  // ---- drag to reorder, LIVE (#538 round 2) ----
  //
  // The list reorders VISUALLY while the pointer is down and only really
  // reorders on release. HTML5 drag-and-drop could not do that: its `drop` is
  // the first moment anything is known, the drag image is the browser's, and
  // it does not exist on touch at all. So this is pointer events plus one
  // FLIP-ish rule:
  //
  //   * the grabbed row lifts (`lifted`) and gets `translateY(pointer travel)`,
  //   * every row between its origin and the slot it is over gets
  //     `translateY(∓ the grabbed row's outer height)` — the hole,
  //   * `transform` only, so nothing reflows and nothing is measured twice,
  //   * the STORE is untouched until pointerup.
  //
  // Geometry is measured ONCE, at grab time, off the row components' own
  // elements (`els()` — a row is TWO elements whenever its values band is
  // open). `outer[i]` is the distance to the next row's top, so the 8px
  // margin between cards is in the number without being re-derived.
  //
  // Escape cancels: the target snaps back to the origin and the lifted row
  // animates home before the drag state is dropped.
  /** The row components, index-aligned with `$playlist.items`. */
  let rowComps: (PlaylistRow | undefined)[] = [];
  /** Index being dragged, or -1. */
  let dragFrom = -1;
  /** Slot it would drop into. */
  let dragTo = -1;
  /** Pointer travel in px since the grab. */
  let dragDy = 0;
  /** Per-index outer heights, measured at grab time. */
  let dragOuter: number[] = [];
  /** Per-index original top offsets, measured at grab time. */
  let dragTops: number[] = [];
  /** Transitions off for the one flush that re-renders the reordered list. */
  let dragSettling = false;
  /** The lifted row animates only while it is returning home (Escape/no-op). */
  let dragReturning = false;

  /** The offset row `i` is drawn at, given a drag state. Pure, so the
   *  reactive statement below can name every dependency — a `$:` only tracks
   *  what appears in its OWN syntax, never what the function it calls reads
   *  (.claude/rules/web.md). */
  function dragShiftAt(i: number, from: number, to: number, dy: number, outer: number[]): number {
    if (from < 0) return 0;
    if (i === from) return dy;
    const h = outer[from] ?? 0;
    if (to > from && i > from && i <= to) return -h;
    if (to < from && i >= to && i < from) return h;
    return 0;
  }
  /** One offset per row, recomputed on every pointer move. */
  $: dragShifts = $playlist.items.map((_, i) =>
    dragShiftAt(i, dragFrom, dragTo, dragDy, dragOuter),
  );

  function onGrab(i: number, e: PointerEvent): void {
    if (e.button !== 0 && e.pointerType === "mouse") return;
    if ($playlist.items.length < 2) return;
    e.preventDefault();
    const tops: number[] = [];
    const bottoms: number[] = [];
    for (let j = 0; j < $playlist.items.length; j++) {
      const els = rowComps[j]?.els() ?? [];
      const first = els[0];
      const last = els[els.length - 1];
      if (!first || !last) return; // a row that has not mounted — do not guess
      tops.push(first.getBoundingClientRect().top);
      bottoms.push(last.getBoundingClientRect().bottom);
    }
    dragTops = tops;
    dragOuter = tops.map((t, j) =>
      j + 1 < tops.length ? (tops[j + 1] as number) - t : (bottoms[j] as number) - t,
    );
    dragFrom = i;
    dragTo = i;
    dragDy = 0;
    dragReturning = false;
    dragStartY = e.clientY;
    window.addEventListener("pointermove", onDragMove);
    window.addEventListener("pointerup", onDragUp);
    window.addEventListener("pointercancel", onDragCancel);
    window.addEventListener("keydown", onDragKey, true);
  }
  let dragStartY = 0;

  function onDragMove(e: PointerEvent): void {
    if (dragFrom < 0) return;
    e.preventDefault();
    dragDy = e.clientY - dragStartY;
    // which ORIGINAL slot the grabbed row's middle is over
    const mid = (dragTops[dragFrom] as number) + dragDy + (dragOuter[dragFrom] as number) / 2;
    let to = dragFrom;
    for (let j = 0; j < dragTops.length; j++) {
      const top = dragTops[j] as number;
      if (mid >= top && mid < top + (dragOuter[j] as number)) {
        to = j;
        break;
      }
      if (j === 0 && mid < top) to = 0;
      if (j === dragTops.length - 1 && mid >= top + (dragOuter[j] as number))
        to = dragTops.length - 1;
    }
    dragTo = to;
  }

  function endDrag(): void {
    window.removeEventListener("pointermove", onDragMove);
    window.removeEventListener("pointerup", onDragUp);
    window.removeEventListener("pointercancel", onDragCancel);
    window.removeEventListener("keydown", onDragKey, true);
    dragFrom = -1;
    dragTo = -1;
    dragDy = 0;
    dragReturning = false;
  }

  /** Escape (or a cancelled pointer): animate the lifted row home, then let go. */
  function abortDrag(): void {
    if (dragFrom < 0) return;
    dragTo = dragFrom;
    dragDy = 0;
    dragReturning = true; // the lifted row eases back rather than snapping
    window.removeEventListener("pointermove", onDragMove);
    window.removeEventListener("pointerup", onDragUp);
    setTimeout(endDrag, 130);
  }

  function onDragKey(e: KeyboardEvent): void {
    if (e.key !== "Escape" || dragFrom < 0) return;
    e.preventDefault();
    e.stopPropagation();
    abortDrag();
  }

  function onDragCancel(): void {
    abortDrag();
  }

  async function onDragUp(): Promise<void> {
    const from = dragFrom;
    const to = dragTo;
    if (from < 0) return;
    if (to === from) {
      abortDrag();
      return;
    }
    // The reorder and the transform reset land in the SAME flush, so the row
    // that moved renders at its new place with no offset and nothing slides.
    dragSettling = true;
    endDrag();
    const items = [...$playlist.items];
    const [moved] = items.splice(from, 1);
    if (moved !== undefined) {
      items.splice(to, 0, moved);
      playlist.update((pl) => ({ ...pl, items }));
      queuePlaylistSave(); // ONE debounced POST, on release only
    }
    await tick();
    requestAnimationFrame(() => (dragSettling = false));
  }

  /** A value edited on the row that is PLAYING goes to the device at once as
   *  well as into the saved playlist — otherwise you would be tuning a slider
   *  while the fixture ignores you until the next advance. */
  function pushLive(i: number, detail: { name: string; values: number[] }): void {
    if (!$playlist.playing || $playlist.index !== i) return;
    void $device?.setControl(detail.name, detail.values);
  }

  async function onPick(
    e: CustomEvent<{ id: string; kind: "pattern" | "scene" | "library"; name: string; source?: string }>,
  ): Promise<void> {
    pickerError = "";
    if (e.detail.kind === "scene") {
      // A scene item names a RECORD (#478): no values of its own, so nothing
      // to seed — the layers inside it carry theirs.
      pickerOpen = false;
      playlist.update((pl) => ({
        ...pl,
        items: [
          ...pl.items,
          { id: e.detail.id, name: e.detail.name, kind: "scene", sec: null, controls: {} },
        ],
      }));
      queuePlaylistSave();
      return;
    }
    if (e.detail.kind !== "library") {
      pickerOpen = false;
      // no values: a freshly added item runs the pattern's own defaults until
      // you open its chip and tune it (D6 — values live on the item)
      addToPlaylist(e.detail.id);
      return;
    }
    // A library pattern is not on the device yet — save it there first, then
    // queue it. The picker stays open with a saving line so a failure has
    // somewhere to be said (#538 §F).
    const source = e.detail.source;
    if (source === undefined) return;
    pickerBusy = e.detail.name;
    const bc = compileToBytecode(source);
    if (!bc) {
      pickerBusy = "";
      pickerError = `“${e.detail.name}” does not compile — not added.`;
      return;
    }
    const r = await saveAndAddToPlaylist(e.detail.name, source, bc);
    pickerBusy = "";
    if (!r.ok) {
      pickerError = `“${e.detail.name}” could not be saved to the device (${r.error}) — not added.`;
      return;
    }
    pickerOpen = false;
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

  // ---- transport ----
  //
  // The primary is a PAUSE verb (S4). The wire has no pause — `stop` halts
  // the auto-advance and keeps the item loaded, so Pause is stop-with-
  // remembered-index and Play resumes at that index
  // (`stores/device.ts` → playlistPause/playlistResume, docs/api.md).

  $: canPlay = $playlist.items.length > 0;

  async function togglePlay(): Promise<void> {
    if (!canPlay) return;
    await ($playlist.playing ? playlistPause() : playlistResume());
  }

  // ---- seek ----
  //
  // The bar is a music-style slider over the CURRENT item. What the device
  // can do is re-enter an item (`play <index>`), not start it part-way in, so
  // a seek restarts the item and the readout jumps to where you dropped it;
  // the device's own advance still arrives a full duration later, so the bar
  // parks at the end for the seconds you skipped. #509 (elapsed on the wire)
  // plus a firmware seek would make it exact — `seekSec` above is the hook.

  /** Drag position while scrubbing (0..1); null = not scrubbing. */
  let scrub: number | null = null;
  let seekEl: HTMLElement;
  /** Seeking needs something to seek WITHIN: a running, timed item. While
   *  stopped the bar is a readout, not a control — a stray touch on a dim,
   *  empty bar should not turn the fixture on. Its BOX is unconditional all
   *  the same, so the transport's height never changes under the pointer. */
  $: seekable = $playlist.playing && nowSec > 0;

  const fracFromEvent = (e: { clientX: number }): number => {
    const r = seekEl.getBoundingClientRect();
    if (r.width <= 0) return 0;
    return Math.max(0, Math.min(1, (e.clientX - r.left) / r.width));
  };

  function onSeekDown(e: PointerEvent): void {
    if (!seekable) return;
    seekEl.setPointerCapture(e.pointerId);
    scrub = fracFromEvent(e);
  }

  function onSeekMove(e: PointerEvent): void {
    if (scrub === null) return;
    scrub = fracFromEvent(e);
  }

  async function onSeekUp(e: PointerEvent): Promise<void> {
    if (scrub === null) return;
    const f = fracFromEvent(e);
    scrub = null;
    if (seekEl.hasPointerCapture(e.pointerId)) seekEl.releasePointerCapture(e.pointerId);
    await commitSeek(f);
  }

  async function commitSeek(f: number): Promise<void> {
    if (nowSec <= 0) return;
    const i = nowIndex;
    seekSec = f * nowSec;
    itemStart = Date.now() - seekSec * 1000;
    now = Date.now();
    await playlistResume(i);
    await tick();
    seekSec = null;
  }

  /** ←/→ nudge by a second, Home/End jump to the ends (the slider's a11y). */
  function onSeekKey(e: KeyboardEvent): void {
    if (!seekable) return;
    const step = 1 / nowSec;
    let f: number | null = null;
    if (e.key === "ArrowRight" || e.key === "ArrowUp") f = progress + step;
    else if (e.key === "ArrowLeft" || e.key === "ArrowDown") f = progress - step;
    else if (e.key === "Home") f = 0;
    else if (e.key === "End") f = 1;
    if (f === null) return;
    e.preventDefault();
    void commitSeek(Math.max(0, Math.min(1, f)));
  }
</script>

<div class="playlist-tab" data-role="playlist-panel" hidden={!active}>
  <div class="transport">
    <!-- four persistent controls, S4's `.group`: primary toggle · stop ·
         prev · next. With an empty queue they are disabled rather than
         absent — see the header comment. -->
    <span class="group">
      <button
        class="btn primary"
        data-role={$playlist.playing ? "pl-pause" : "pl-play"}
        disabled={!canPlay}
        data-reason={canPlay ? undefined : "the playlist is empty"}
        title={$playlist.playing ? "pause the playlist (holds this item)" : "play the playlist"}
        on:click={() => void togglePlay()}
      >
        {#if $playlist.playing}‖ Pause{:else}▶ Play{/if}
      </button>
      <button
        class="btn icon"
        data-role="pl-stop"
        disabled={!canPlay}
        data-reason={canPlay ? undefined : "the playlist is empty"}
        title="stop"
        aria-label="stop"
        on:click={() => void playlistStop()}
      >
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
          <rect x="1" y="1" width="8" height="8" rx="1" fill="currentColor" />
        </svg>
      </button>
      <button
        class="btn icon"
        data-role="pl-prev"
        disabled={!canPlay}
        data-reason={canPlay ? undefined : "the playlist is empty"}
        title="previous"
        aria-label="previous"
        on:click={() => void playlistStep(-1)}
      >
        <svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden="true">
          <rect x="2" y="2" width="1.6" height="8" />
          <path d="M10 2v8L4.6 6z" />
        </svg>
      </button>
      <button
        class="btn icon"
        data-role="pl-next"
        disabled={!canPlay}
        data-reason={canPlay ? undefined : "the playlist is empty"}
        title="next"
        aria-label="next"
        on:click={() => void playlistStep(1)}
      >
        <svg width="12" height="12" viewBox="0 0 12 12" fill="currentColor" aria-hidden="true">
          <rect x="8.4" y="2" width="1.6" height="8" />
          <path d="M2 2v8l5.4-4z" />
        </svg>
      </button>
    </span>

    <span class="nowplaying" class:idle={!$playlist.playing} data-role="pl-now">
      <span class="np1">
        <b data-role="pl-now-name">{nowItem ? nowItem.name || nowItem.id : "Nothing queued"}</b>
        {#if nowSec > 0}<span class="t">{clock(elapsed)} / {clock(nowSec)}</span>{/if}
      </span>
      <!-- Why the transport says `stopped` when nobody pressed stop: a tile's
           `Play` (or the editor's `▶ Play on device`) took the fixture over,
           and the playlist parked rather than advancing on top of it (#538
           round 2). Absent in every state the mocks draw, which all play. -->
      {#if !$playlist.playing && $playlistPreemptedBy}
        <span class="preempt" data-role="pl-preempted">
          stopped — playing <b>{$playlistPreemptedBy}</b> directly
        </span>
      {/if}
      <!-- the progress bar IS the seek control (S4's `.prog`: a 3px track with
           an `--ok` fill inside it). The bar PAINTS at the mock's 3px; the
           grab area is the wrapper's ::after overlay, 20px tall, so the
           transport keeps the height S4 draws. -->
      <span
        class="progwrap"
        class:seekable
        on:pointerdown={onSeekDown}
        on:pointermove={onSeekMove}
        on:pointerup={(e) => void onSeekUp(e)}
        on:pointercancel={(e) => void onSeekUp(e)}
        role="presentation"
      >
        <span
          class="prog"
          class:seekable
          bind:this={seekEl}
          data-role="pl-progress"
          role="slider"
          tabindex={seekable ? 0 : -1}
          aria-label="seek within this item"
          aria-valuemin={0}
          aria-valuemax={Math.round(nowSec)}
          aria-valuenow={Math.round(elapsed)}
          aria-valuetext="{clock(elapsed)} of {clock(nowSec)}"
          title="drag to a time in this item — the device restarts the item there"
          on:keydown={onSeekKey}
        >
          <i class="fil" style="width:{shown * 100}%"></i>
        </span>
      </span>
    </span>

    <span class="spacer"></span>

    <span class="group">
      <!-- S4b shortens it to "Default" on a phone -->
      <label class="tiny dim" for="pl-default-sec">Default<span class="wide">&nbsp;duration</span></label>
      <input
        class="inp num xs"
        id="pl-default-sec"
        data-role="pl-default-sec"
        type="number"
        min="0"
        title="seconds each item plays unless it overrides this (0 = wait for next)"
        value={$playlist.defaultSec}
        on:change={onDefaultSecChange}
      />
      <span class="tiny dim">s</span>
    </span>

    <span class="group cf">
      <label class="tiny dim" for="pl-crossfade">Crossfade</label>
      <input
        class="inp num xs"
        id="pl-crossfade"
        data-role="pl-crossfade"
        type="number"
        min="0"
        step="0.1"
        title="seconds to blend between items (0 = hard cut)"
        value={$playlist.crossfadeMs ? $playlist.crossfadeMs / 1000 : 0}
        on:change={onCrossfadeChange}
      />
      <span class="tiny dim">s</span>
    </span>

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
  </div>

  {#if !$device}
    <p class="dim hint">device unreachable — {$deviceError || "reload to retry"}.</p>
  {:else}
    <div class="pl-list">
      {#if $playlist.items.length === 0}
        <p class="dim hint" data-role="pl-empty">
          Nothing queued yet. <strong>+ Add</strong> picks from the patterns saved on this device or
          from the library — add the same pattern more than once for different looks.
        </p>
      {:else}
        <ul class="rows">
          {#each $playlist.items as item, i (i)}
            {#if $luxel}
              <PlaylistRow
                bind:this={rowComps[i]}
                luxel={$luxel}
                source={itemSource(item.id, $devicePatterns)}
                {item}
                lifted={dragFrom === i}
                shift={dragShifts[i] ?? 0}
                anim={!dragSettling && (dragFrom !== i || dragReturning)}
                scene={itemScene(item, $scenes)}
                {sceneLookup}
                defaultSec={$playlist.defaultSec}
                missing={itemMissing(item, $devicePatterns, $scenes)}
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
                on:editscene={(e) => dispatch("openscene", e.detail)}
                on:grab={(e) => onGrab(i, e.detail)}
              />
            {/if}
          {/each}
        </ul>
      {/if}
      <!-- S4c `.addwrap`: the picker is a dropdown UNDER `+ Add`, in the list
           it adds to — 360 px on a console, the width of the list on a phone
           (S4d). The wrapper is what positions it. -->
      <div class="addwrap">
        <button
          class="btn quiet add"
          bind:this={addBtn}
          data-role="pl-add"
          on:click={() => {
            pickerOpen = !pickerOpen;
            if (pickerOpen) void refreshScenes();
          }}>+ Add</button
        >
        <PatternPicker
          luxel={$luxel}
          open={pickerOpen}
          anchor={addBtn}
          patterns={$devicePatterns}
          scenes={$scenes}
          {sceneLookup}
          busy={pickerBusy}
          error={pickerError}
          on:pick={(e) => void onPick(e)}
          on:close={() => {
            pickerOpen = false;
            pickerError = "";
          }}
        />
      </div>
    </div>
    <!-- S4 `.plfoot` — a sibling of the list, with its own page padding -->
    {#if $playlist.items.length > 0}
      <p class="foot" data-role="pl-total">
        {$playlist.items.length} item{$playlist.items.length === 1 ? "" : "s"} · loop ≈ {fmtDuration(
          playlistTotalSec,
        )}{playlistHasManual ? " + manual stops" : ""}
      </p>
    {/if}
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

  .tiny {
    font-size: 12px;
  }

  .spacer {
    flex: 1;
  }

  .hint {
    font-size: 12px;
    margin: 2px 0 10px;
  }

  /* S4 `.transport` */
  .transport {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border);
  }

  /* S4 `.group` */
  .group {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  /* the two settings groups space their own label/field/unit */
  .transport .group:not(:first-child) {
    gap: 8px;
  }

  .cf {
    margin-left: 8px;
  }

  /* S4 `.nowplaying` — a plain block (it is a flex ITEM of the transport, so
     it is blockified), always mounted, so Pause/Play never moves anything */
  .nowplaying {
    min-width: 230px;
    margin-left: 8px;
  }

  .nowplaying.idle {
    opacity: 0.55;
  }

  /* S4 `.nowplaying .np1` — one line: a bold name and a mono clock beside it */
  .np1 {
    display: block;
    font-size: 13px;
  }

  .np1 b {
    font-weight: 600;
  }

  /* the preemption line — the mocks have no stopped-by-a-direct-play state,
     so this is a line of its own rather than a variant of one of theirs */
  .preempt {
    display: block;
    margin-top: 2px;
    font-size: 11.5px;
    color: var(--text-dim);
  }

  .preempt b {
    font-weight: 600;
    color: var(--text);
  }

  .np1 .t {
    margin-left: 8px;
    font-family: var(--mono);
    font-size: 11.5px;
    color: var(--text-dim);
  }

  /* a grab area bigger than the 3px the mock paints, without changing what is
     painted or how tall the transport is: the overlay is the target */
  .progwrap {
    display: block;
    position: relative;
  }

  .progwrap::after {
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    top: -6px;
    bottom: -11px;
  }

  /* S4 `.prog` + `.prog i` */
  .prog {
    display: block;
    margin-top: 5px;
    height: 3px;
    border-radius: 2px;
    background: #272c35;
    overflow: hidden;
  }

  .fil {
    display: block;
    height: 100%;
    background: var(--ok);
    transition: width 0.4s linear;
  }

  .prog.seekable {
    touch-action: none;
  }

  .progwrap.seekable {
    cursor: pointer;
  }

  .prog.seekable:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: 2px;
  }

  .overflow {
    position: relative;
    display: inline-flex;
  }

  /* S4 draws the ⋯ with `margin-left:4px` on the BUTTON */
  .overflow :global(.btn) {
    margin-left: 4px;
  }

  /* S4 `.pllist` */
  .pl-list {
    padding: 20px;
    max-width: 820px;
  }

  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  /* S4c `.addwrap{position:relative;width:360px;margin-top:4px}` — the box the
     picker drops out of. The width is the PICKER's, not the button's. */
  .addwrap {
    position: relative;
    width: 360px;
    margin-top: 4px;
  }

  .add {
    margin-top: 0;
  }

  /* S4 `.plfoot` */
  .foot {
    margin: 0;
    padding: 0 20px 22px;
    font: 12px/1 var(--mono);
    color: var(--text-dim);
  }

  /* ---- phone (S4b, D9) ---- */
  @media (max-width: 600px) {
    .transport {
      padding: 12px;
      gap: 10px;
    }

    /* S4b: the four buttons and ⋯ share the first row, ⋯ pushed to the end */
    .transport .spacer {
      display: none;
    }

    .transport .group:first-child {
      flex: 1;
    }

    .overflow {
      margin-left: auto;
      order: 1;
    }

    .nowplaying {
      order: 2;
      width: 100%;
      min-width: 0;
      margin-left: 0;
    }

    /* S4b: "Default" + "Crossfade" share the third row */
    .transport .group:not(:first-child) {
      order: 3;
    }

    .cf {
      margin-left: 8px;
    }

    .wide {
      display: none;
    }

    .pl-list {
      padding: 12px;
    }

    /* S4d: the picker is the full width of the list, not a 360px popover —
       D9 responsive stacking, not a second flow */
    .addwrap {
      width: auto;
    }

    .foot {
      padding: 0 12px 16px;
    }
  }
</style>
