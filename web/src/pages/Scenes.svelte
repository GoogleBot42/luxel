<script lang="ts">
  // The Scenes page (mockups S6 · S6b · S6c · S6d · S9; Gitea #480).
  //
  // "Scenes are new, so the job here is making them feel like patterns, not
  // like a second app" (S6's note): same tab bar, same tile grid, same square
  // device-shaped thumbnail, same playing ring. One quiet sentence says what
  // a scene is; the only loud thing on the page is `+ New scene`.
  //
  // WHEN THE PAGE HAS CONTENT (D10, §5.4c):
  //   * console — strictly by Layout kind. A matrix console always has the
  //     tab, empty or not (S6c). A strip/3D/map console never does, and the
  //     shell's tab list is what enforces that.
  //   * playground — the tab is always there (hiding it would make the
  //     feature undiscoverable), and when the `Preview as` chip is not a
  //     matrix the page is the S6b empty state whose one action sets it.
  //
  // PLAYING, in the playground (#742 item 40): there is no device, so the
  // tiles wear `Open` and the gesture opens the scene editor — whose stage
  // composites the scene live — exactly as a pattern tile there wears `Open`
  // and opens the pattern editor (§5.7, `pages/Patterns.svelte`'s
  // `openVerb`). `canPlay` below is the whole of that rule; it also takes the
  // `▶ playing` ring and pill with it, which mock S9 does not draw either.
  import { createEventDispatcher } from "svelte";
  import "../components/scene/scene.css";
  import SceneGrid from "../components/scene/SceneGrid.svelte";
  import { playgroundPatternId } from "../lib/sceneRender";
  import { listPatterns } from "../lib/store";
  import { confirm } from "../stores/dialog";
  import { device, devicePatterns, isPlayground } from "../stores/device";
  import { layout, setPreviewAs } from "../stores/geometry";
  import { luxel } from "../stores/pattern";
  import { cachedSprite, loadSprite, refreshSprites, sprites } from "../stores/sprites";
  import { encodeSprite } from "../lib/sprite";
  import {
    activateScene,
    activeSceneId,
    deleteScene,
    duplicateScene,
    newScene,
    refreshScenes,
    saveScene,
    scenes,
    startScenePoll,
  } from "../stores/scenes";
  import { onDestroy } from "svelte";

  export let active = false;

  const dispatch = createEventDispatcher<{ open: string }>();

  /** The one sentence, identical on the populated page and the empty one, so
   *  it does not change meaning when the first scene appears (S6c's note). */
  const LEDE = "Layer patterns, text and images on top of each other.";
  const LEDE_2 = " Scenes play like patterns.";

  /** A matrix is what a scene needs. On a console this is the DEVICE's
   *  Layout and nothing else (`deviceGeometry()`, #539/#573); in the
   *  playground it is the `Preview as` choice. */
  $: ready = $layout.dims === 2 && $layout.regular && $layout.w > 0 && $layout.h > 0;

  // The 5th cadence on the ONE poll scheduler — and ONLY while this page is
  // the one on screen. Every page stays mounted when it is hidden, so a poll
  // registered in `onMount` would keep asking a device for its scenes from
  // behind the Playlist forever, through the same two sockets the transport
  // needs (docs/web-architecture.md, the poll scheduler).
  //
  // The assignment lives in a FUNCTION: a `$:` that both reads and assigns
  // the same variable is its own dependency and re-runs forever
  // (.claude/rules/web.md — it froze this page the first time round).
  let stopPoll: (() => void) | undefined;

  function syncPoll(on: boolean): void {
    if (on && !stopPoll) stopPoll = startScenePoll();
    else if (!on && stopPoll) {
      stopPoll();
      stopPoll = undefined;
    }
  }

  $: syncPoll(active);
  onDestroy(() => syncPoll(false));

  /** Re-read whenever the page comes forward — the scene library, and the
   *  sprite library the tiles need to draw a sprite layer at all (#740). */
  $: if (active) {
    void refreshScenes();
    void warmSprites();
  }

  function lookup(id: string): string | null {
    const dev = $devicePatterns.find((p) => p.id === id);
    if (dev?.source !== undefined) return dev.source;
    for (const p of listPatterns()) if (playgroundPatternId(p.name) === id) return p.source;
    return null;
  }

  // ---- sprite layers (#740) ----
  //
  // A sprite layer's pixels are a RECORD now, not a pattern source, and the
  // compositor takes it by value — so the page pre-loads the library and the
  // grid reads the encoded bytes out of a cache. The encode is memoised per id
  // because `SceneRenderer.setScene` copies the bytes into wasm on every
  // rebuild and a 64×64 record is 4 KB.

  const encoded = new Map<string, Uint8Array>();
  /** Bumped when a record lands — the grid's re-bind signal. */
  let spriteRev = 0;

  async function warmSprites(): Promise<void> {
    await refreshSprites();
    let landed = 0;
    for (const s of $sprites) {
      if (encoded.has(s.id)) continue;
      const sp = await loadSprite(s.id);
      if (!sp) continue;
      encoded.set(s.id, encodeSprite(sp));
      landed++;
    }
    if (landed > 0) spriteRev += landed;
  }

  function spriteBytesOf(id: string): Uint8Array | null {
    const held = encoded.get(id);
    if (held) return held;
    const sp = cachedSprite(id);
    if (!sp) return null;
    const bytes = encodeSprite(sp);
    encoded.set(id, bytes);
    return bytes;
  }

  async function create(): Promise<void> {
    const r = await saveScene(newScene());
    if (r.ok && r.id) dispatch("open", r.id);
  }

  async function remove(id: string): Promise<void> {
    const s = $scenes.find((x) => x.id === id);
    const ok = await confirm({
      title: `Delete “${s?.name ?? "this scene"}”?`,
      body: "The scene is removed. The patterns it used are untouched.",
      confirmLabel: "Delete scene",
      danger: true,
    });
    if (ok) await deleteScene(id);
  }

  async function duplicate(id: string): Promise<void> {
    const made = await duplicateScene(id);
    if (made) dispatch("open", made);
  }
</script>

<section
  class="scenes panel"
  class:playground={$isPlayground}
  data-role="scenes-panel"
  hidden={!active}
>
  {#if !ready}
    <!-- S6b: no 2D fixture. One sentence, the single action that unblocks the
         page, and the follow-up step as dim text rather than a second button.
         Reachable only in the playground — a console's tab is gated on its
         Layout and never appears here. -->
    <div class="empty" data-role="scenes-empty-fixture">
      <div class="dim" style="font-size:13px">
        Scenes layer patterns, text and images on a matrix.
      </div>
      <button
        class="btn primary"
        data-role="scenes-preview-as-matrix"
        on:click={() => setPreviewAs({ mode: "matrix", w: 64, h: 64 })}
        >Preview as a 64×64 matrix</button
      >
      <div class="hint">Then + New scene.</div>
    </div>
  {:else if $scenes.length === 0}
    <!-- S6c: the tab has to say something useful with nothing in it — the
         same definition the populated page carries, and the page's one
         primary, centred. No page bar, no empty grid, no illustration. -->
    <div class="empty" data-role="scenes-empty">
      <div class="dim" style="font-size:13px">{LEDE}{LEDE_2}</div>
      <button class="btn primary" data-role="new-scene" on:click={() => void create()}
        >+ New scene</button
      >
    </div>
  {:else}
    <div class="pagebar">
      <div class="hint" data-role="scenes-lede">{LEDE}<span class="second">{LEDE_2}</span></div>
      <div class="spacer"></div>
      <!-- ONE flex item, not two: `.btn`'s 6px gap would otherwise land
           between the `+` and the label and widen the button past the mock's
           (S6 `.pagebar .btn.primary`). The phone drops the label (S6d). -->
      <button class="btn primary" data-role="new-scene" on:click={() => void create()}
        ><span>+ <span class="newlabel">New scene</span></span></button
      >
    </div>
    <SceneGrid
      luxel={$luxel}
      items={$scenes}
      playingId={$activeSceneId}
      canPlay={!$isPlayground}
      rig={$layout}
      {lookup}
      sprites={spriteBytesOf}
      {spriteRev}
      on:play={(e) => void activateScene(e.detail)}
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
    /* `flex: 1` and the panel colour are the pair that make this a tab SURFACE
       like `.patterns-tab` / `.playlist-tab` / `.settings-tab`, which both set
       (Gitea #739). Without the background it fell through to the body's `--bg`
       and read visibly darker than the tab beside it; without the grow, the
       background it now paints would stop at the empty state's 212px and the
       shell's darker ground would show under it. */
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    background: var(--bg-panel);
  }

  .panel[hidden] {
    display: none;
  }

  /* S6d: the primary shrinks to an icon beside the one-line explanation */
  @media (max-width: 600px) {
    .newlabel {
      display: none;
    }

    [data-role="new-scene"] {
      width: 32px;
      padding: 0;
    }
  }
</style>
