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
  import SpriteImport from "../components/sprite/SpriteImport.svelte";
  import { confirm } from "../stores/dialog";
  import { isPlayground } from "../stores/device";
  import { layout, setPreviewAs } from "../stores/geometry";
  import { notes } from "../stores/notify";
  import {
    deleteSprite,
    duplicateSprite,
    freshSprite,
    saveSprite,
    spriteMaxBytes,
    spriteRev,
    sprites,
    stageSprite,
    startSpritePoll,
    usedBy,
    warmSprites,
  } from "../stores/sprites";
  import type { Sprite } from "../lib/sprite";

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
   *  so the tiles have something to draw. `warmSprites` is the store's, and
   *  the tiles' repaint signal is its `spriteRev` — one owner for both, so a
   *  record this tab downloads is one the scene editor already has (#740
   *  follow-up; the three page-local copies are what left the 2026-09-26
   *  panel's scene stage empty). */
  $: if (active) void warmSprites();

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

  // ---- Import image… (Gitea #784) ----
  //
  // The drop target is the WHOLE panel: dragging a PNG anywhere onto the
  // Sprites tab is the gesture, not hitting a 120px well. `importer` is the
  // live `SpriteImport` — only one of the two branches below is ever mounted,
  // so the binding always names the one on screen.

  let importer: SpriteImport | undefined;
  let dropping = false;

  /** An imported record goes to the EDITOR unsaved, never to the store —
   *  `openSprite("")` is the route for "a new sprite with no id yet". */
  function imported(s: Sprite): void {
    stageSprite(s);
    dispatch("open", "");
  }

  function onDrop(e: DragEvent): void {
    dropping = false;
    void importer?.offerFiles(e.dataTransfer?.files ?? null);
  }

  /** Only a FILE drag lights the panel up — dragging a tile around does not. */
  function hasFiles(e: DragEvent): boolean {
    return [...(e.dataTransfer?.types ?? [])].includes("Files");
  }
</script>

<!-- svelte-ignore a11y-no-static-element-interactions -->
<section
  class="scenes sprites panel"
  class:playground={$isPlayground}
  class:dropping
  data-role="sprites-panel"
  hidden={!active}
  on:dragover|preventDefault={(e) => {
    // `ready` too: with no 2D fixture there is no importer mounted, and a
    // highlight that accepts a drop and does nothing is worse than none
    if (ready && hasFiles(e)) dropping = true;
  }}
  on:dragleave={() => (dropping = false)}
  on:drop|preventDefault={onDrop}
>
  <!-- The `sprite` note channel, on screen at last: the palette-cap refusal
       (#741 item 14) and an unreadable import (#784) both speak through it and
       neither was rendered anywhere until now. -->
  {#if $notes.sprite}
    <p class="snote" class:bad={$notes.sprite.startsWith("sprite: ")} data-role="sprite-note">
      {$notes.sprite}
    </p>
  {/if}
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
      <div class="erow">
        <button class="btn primary" data-role="new-sprite" on:click={() => void create()}
          >+ New sprite</button
        >
        <SpriteImport
          bind:this={importer}
          panel={{ w: $layout.w, h: $layout.h }}
          maxBytes={$spriteMaxBytes}
          on:import={(e) => imported(e.detail)}
        />
      </div>
      <div class="hint">…or drop an image anywhere on this page.</div>
    </div>
  {:else}
    <div class="pagebar">
      <div class="hint" data-role="sprites-lede">{LEDE}</div>
      <div class="spacer"></div>
      <SpriteImport
        bind:this={importer}
        panel={{ w: $layout.w, h: $layout.h }}
        maxBytes={$spriteMaxBytes}
        on:import={(e) => imported(e.detail)}
      />
      <!-- ONE flex item, not two: `.btn`'s 6px gap would otherwise land
           between the `+` and the label and widen the button. The phone drops
           the label, as the Scenes primary does. -->
      <button class="btn primary" data-role="new-sprite" on:click={() => void create()}
        ><span>+ <span class="newlabel">New sprite</span></span></button
      >
    </div>
    <SpriteGrid
      items={$sprites}
      rev={$spriteRev}
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

  /* a file drag over the page: the whole surface is the drop target, so the
     whole surface says so (an inset ring rather than a moved border, which
     would shift every tile by a pixel) */
  .panel.dropping {
    box-shadow: inset 0 0 0 2px var(--accent);
  }

  /* the empty state's two actions on one row */
  .erow {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: center;
  }

  .snote {
    margin: 10px 12px 0;
    color: var(--text-dim);
    font-size: 12px;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }

  .snote.bad {
    color: var(--error);
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
