<script lang="ts">
  // The Sprites tab (Gitea #740). Jeremy: "Sprites should be a first class
  // type. Make a new Sprite tab. That's where the sprite editor will be based
  // out of (which the scene edit page can directly invoke)."
  //
  // Same shell as `pages/Scenes.svelte`, on purpose and to the line: one
  // quiet sentence, `+ New sprite` as the page's only loud thing, a tile grid,
  // and an empty state carrying the same sentence plus the same button. It
  // wears the `scenes` class so it inherits that page's design system
  // (`components/scene/scene.css`) rather than growing a second one.
  //
  // WHEN THE PAGE HAS CONTENT — identical to Scenes, because a sprite is for
  // a scene layer and a scene needs a matrix:
  //   * console — strictly by Layout kind; the shell's tab list enforces it.
  //   * playground — the tab is always there, and with a non-matrix
  //     `Preview as` the page is the S6b-shaped empty state that sets one.
  import { createEventDispatcher, onDestroy } from "svelte";
  import "../components/scene/scene.css";
  import SpriteGrid from "../components/sprite/SpriteGrid.svelte";
  import { confirm } from "../stores/dialog";
  import { isPlayground } from "../stores/device";
  import { layout, setPreviewAs } from "../stores/geometry";
  import {
    cachedSprite,
    deleteSprite,
    duplicateSprite,
    freshSprite,
    loadSprite,
    refreshSprites,
    saveSprite,
    sprites,
    startSpritePoll,
    usedBy,
  } from "../stores/sprites";

  export let active = false;

  const dispatch = createEventDispatcher<{ open: string }>();

  /** The one sentence, identical on the populated page and the empty one. */
  const LEDE = "Sprites are small pixel images you draw, for scene layers.";

  /** A matrix is what a sprite is FOR. Same test as Scenes (`deviceGeometry()`
   *  on a console, `Preview as` in the playground). */
  $: ready = $layout.dims === 2 && $layout.regular && $layout.w > 0 && $layout.h > 0;

  // The 6th cadence on the ONE poll scheduler, and ONLY while this page is on
  // screen. The assignment lives in a FUNCTION: a `$:` that both reads and
  // assigns the same variable is its own dependency (.claude/rules/web.md).
  let stopPoll: (() => void) | undefined;

  function syncPoll(on: boolean): void {
    if (on && !stopPoll) stopPoll = startSpritePoll();
    else if (!on && stopPoll) {
      stopPoll();
      stopPoll = undefined;
    }
  }

  $: syncPoll(active);
  onDestroy(() => syncPoll(false));

  /** Re-read whenever the page comes forward, and pull every record's pixels
   *  so the tiles have something to draw. */
  $: if (active) void open();

  /** Bumped when a record lands in the store's cache — the tiles' repaint
   *  signal (the cache is a plain Map, so nothing invalidates on a write). */
  let rev = 0;

  async function open(): Promise<void> {
    await refreshSprites();
    await warm();
  }

  /** Decode every row's record once. One at a time on a console: the device
   *  serves ~2 connections and a parallel burst starves the status poll. Rows
   *  already in hand cost nothing, so the 2 Hz poll re-running this is free
   *  and `rev` only moves when a record actually arrived. */
  async function warm(): Promise<void> {
    let landed = 0;
    for (const s of $sprites) {
      if (cachedSprite(s.id)) continue;
      if (await loadSprite(s.id)) landed++;
    }
    if (landed > 0) rev += landed;
  }

  $: if (active && $sprites.length > 0) void warm();

  async function create(): Promise<void> {
    const r = await saveSprite(freshSprite(8, 8));
    if (r.ok && r.id) dispatch("open", r.id);
  }

  async function remove(id: string): Promise<void> {
    const s = $sprites.find((x) => x.id === id);
    const used = usedBy(id);
    const ok = await confirm({
      title: `Delete “${s?.name ?? "this sprite"}”?`,
      body:
        used.length > 0
          ? `The drawing is removed. ${used.length === 1 ? "The scene" : "The scenes"} ${used.join(", ")} will have a sprite layer with nothing to draw.`
          : "The drawing is removed. Nothing else changes.",
      confirmLabel: "Delete sprite",
      danger: true,
    });
    if (ok) await deleteSprite(id);
  }

  async function duplicate(id: string): Promise<void> {
    const made = await duplicateSprite(id);
    if (made) dispatch("open", made);
  }
</script>

<section
  class="scenes sprites panel"
  class:playground={$isPlayground}
  data-role="sprites-panel"
  hidden={!active}
>
  {#if !ready}
    <!-- No 2D fixture: one sentence, the single action that unblocks the page,
         and the follow-up as dim text rather than a second button (the shape
         Scenes' S6b empty state settled on). Reachable only in the playground
         — a console's tab is gated on its Layout and never appears here. -->
    <div class="empty" data-role="sprites-empty-fixture">
      <div class="dim" style="font-size:13px">
        Sprites are small pixel images you draw, for scene layers on a matrix.
      </div>
      <button
        class="btn primary"
        data-role="sprites-preview-as-matrix"
        on:click={() => setPreviewAs({ mode: "matrix", w: 64, h: 64 })}
        >Preview as a 64×64 matrix</button
      >
      <div class="hint">Then + New sprite.</div>
    </div>
  {:else if $sprites.length === 0}
    <div class="empty" data-role="sprites-empty">
      <div class="dim" style="font-size:13px">{LEDE}</div>
      <button class="btn primary" data-role="new-sprite" on:click={() => void create()}
        >+ New sprite</button
      >
    </div>
  {:else}
    <div class="pagebar">
      <div class="hint" data-role="sprites-lede">{LEDE}</div>
      <div class="spacer"></div>
      <!-- ONE flex item, not two: `.btn`'s 6px gap would otherwise land
           between the `+` and the label and widen the button. The phone drops
           the label, as the Scenes primary does. -->
      <button class="btn primary" data-role="new-sprite" on:click={() => void create()}
        ><span>+ <span class="newlabel">New sprite</span></span></button
      >
    </div>
    <SpriteGrid
      items={$sprites}
      {rev}
      on:edit={(e) => dispatch("open", e.detail)}
      on:duplicate={(e) => void duplicate(e.detail)}
      on:remove={(e) => void remove(e.detail)}
    />
  {/if}
</section>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    /* `flex: 1` + the panel colour are the pair that make this a tab SURFACE
       like `.scenes`/`.patterns-tab`/`.settings-tab` (Gitea #739). */
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    background: var(--bg-panel);
  }

  .panel[hidden] {
    display: none;
  }

  /* the primary shrinks to an icon beside the one-line explanation (S6d) */
  @media (max-width: 600px) {
    .newlabel {
      display: none;
    }

    [data-role="new-sprite"] {
      width: 32px;
      padding: 0;
    }
  }
</style>
