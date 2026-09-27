<script lang="ts">
  // The Scenes page (mockups S6 · S6b · S6c · S6d · S9; Gitea #480).
  //
  // "Scenes are new, so the job here is making them feel like patterns, not
  // like a second app" (S6's note): same tab bar, same tile grid, same square
  // device-shaped thumbnail, same playing ring. One quiet sentence says what
  // a scene is; the only loud thing on the page is `+ New scene`.
  //
  // SINCE #785 IT HAS SOURCES, the way the Patterns page does (§5.1, D3):
  //
  //   console:     On device (N) | Library (N)
  //   playground:  Mine (N)      | Library (N)
  //
  // `Library` is the shipped set (`library/scenes/` →
  // `web/tools/gen-sprite-scene-gallery.mjs` → `stores/library.ts`). It is
  // read-only, and its one verb is `+ Add`, which is where the interesting work
  // is: a shipped scene names its patterns and sprites by LIBRARY reference,
  // not by store id, so adding it creates whatever the target store is missing
  // and rewrites the ids (`lib/sceneRefs.ts`). Nothing is played — a tile must
  // not change what the LEDs are doing (#563) — the scene is stored and the
  // editor opens on it.
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
  import SourceSeg, { type Source } from "../components/SourceSeg.svelte";
  import { playgroundPatternId } from "../lib/sceneRender";
  import { listPatterns } from "../lib/store";
  import { confirm } from "../stores/dialog";
  import { device, devicePatterns, isPlayground } from "../stores/device";
  import { layout, setPreviewAs } from "../stores/geometry";
  import {
    cloneLibraryScene,
    libraryLookup,
    libraryRev,
    libraryScenes,
    libraryScenesLoading,
    librarySpriteBytesOf,
    warmSceneLibrary,
  } from "../stores/library";
  import { luxel } from "../stores/pattern";
  import { spriteBytesOf, spriteRev, warmSprites } from "../stores/sprites";
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

  // ---- the source control (#785) ----

  type SourceId = "live" | "library";
  /** The page opens on the LIVE source in both shells — see
   *  `pages/Sprites.svelte` for why this control defaults differently from the
   *  Patterns page's (a workspace opens on your own work; the shipped set is one
   *  chip away, counted on the chip and named in the empty state). */
  let sourceId: SourceId = "live";
  $: liveSource = {
    id: "live" as const,
    label: $isPlayground ? "Mine" : "On device",
    count: $scenes.length,
  };
  $: librarySource = {
    id: "library" as const,
    label: "Library",
    count: $libraryScenes.length,
  };
  $: sources = [liveSource, librarySource] as Source[];

  /** The narrowing lives in a FUNCTION: svelte-check does not parse a TS
   *  assertion inside a template expression (the same reason
   *  `SpriteInspector`'s `setFit` exists). */
  function selectSource(id: string): void {
    sourceId = id as SourceId;
  }

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

  /** Re-read whenever the page comes forward — the scene library, the sprite
   *  library the tiles need to draw a sprite layer at all (#740), and the
   *  shipped set behind the `Library` chip (its tiles composite real pattern
   *  layers, so `warmSceneLibrary` pulls `gallery.json` too, once). */
  $: if (active) {
    void refreshScenes();
    void warmSprites();
    void warmSceneLibrary();
  }

  function lookup(id: string): string | null {
    const dev = $devicePatterns.find((p) => p.id === id);
    if (dev?.source !== undefined) return dev.source;
    for (const p of listPatterns()) if (playgroundPatternId(p.name) === id) return p.source;
    return null;
  }

  // ---- sprite layers (#740) ----
  //
  // A sprite layer's pixels are a RECORD, not a pattern source, and the
  // compositor takes it by value — so `stores/sprites.ts` pre-loads the
  // library's records and the grid reads the encoded bytes out of its cache
  // (`spriteBytesOf`), re-binding on `$spriteRev`. Both used to be page-local
  // here, in a `Map` that was never invalidated beside a counter that only
  // moved on success; see the store for what that cost on 2026-09-26.

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

  /**
   * `+ Add` on a shipped tile. The scene is copied into the live library
   * together with the patterns and sprites its layers name — created only where
   * the store does not already hold them (matched by CONTENT, not by name:
   * `lib/sceneRefs.ts`) — and the editor opens on the result so the layer stack
   * is the next thing on screen.
   *
   * Nothing is activated. Every refusal along the way is the HOST's own
   * sentence: `sprite: … over the 16 KiB cap`, `scenes: store full (N of 3840
   * B)`, a pattern the device would not take.
   */
  async function add(id: string): Promise<void> {
    const row = $libraryScenes.find((s) => s.scene.id === id);
    if (!row) return;
    const made = await cloneLibraryScene(row.slug);
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
  {:else}
    <div class="pagebar">
      <SourceSeg
        {sources}
        value={sourceId}
        dataRole="scenes-sources"
        on:select={(e) => selectSource(e.detail)}
      />
      <div class="hint lede" data-role="scenes-lede">{LEDE}<span class="second">{LEDE_2}</span></div>
      <div class="spacer"></div>
      <!-- ONE flex item, not two: `.btn`'s 6px gap would otherwise land
           between the `+` and the label and widen the button past the mock's
           (S6 `.pagebar .btn.primary`). The phone drops the label (S6d). -->
      <button class="btn primary" data-role="new-scene" on:click={() => void create()}
        ><span>+ <span class="newlabel">New scene</span></span></button
      >
    </div>
    {#if sourceId === "library"}
      {#if $libraryScenesLoading}
        <p class="hint pad" data-role="scenes-library-loading">loading the scene library…</p>
      {:else if $libraryScenes.length === 0}
        <p class="hint pad" data-role="scenes-library-note">
          no shipped scenes in this build (scenes.json is missing)
        </p>
      {:else}
        <!-- The shipped tiles composite the real thing: `libraryLookup` resolves
             a `pat` reference to its clean-room source and `librarySpriteBytesOf`
             a `spr` one to its `LXSP` record, so what you see on the tile is
             what `+ Add` lands. `$libraryRev` is their re-bind signal. -->
        <SceneGrid
          luxel={$luxel}
          mode="library"
          items={$libraryScenes.map((s) => s.scene)}
          canPlay={false}
          rig={$layout}
          lookup={libraryLookup}
          sprites={librarySpriteBytesOf}
          spriteRev={$libraryRev}
          on:add={(e) => void add(e.detail)}
        />
      {/if}
    {:else if $scenes.length === 0}
      <!-- S6c: the tab has to say something useful with nothing in it — the
           same definition the populated page carries, and the page's one
           primary, centred. No empty grid, no illustration. -->
      <div class="empty" data-role="scenes-empty">
        <div class="dim" style="font-size:13px">{LEDE}{LEDE_2}</div>
        <button class="btn primary" data-role="new-scene-empty" on:click={() => void create()}
          >+ New scene</button
        >
        <div class="hint">…or start from the Library.</div>
      </div>
    {:else}
      <SceneGrid
        luxel={$luxel}
        items={$scenes}
        playingId={$activeSceneId}
        canPlay={!$isPlayground}
        rig={$layout}
        {lookup}
        sprites={spriteBytesOf}
        spriteRev={$spriteRev}
        on:play={(e) => void activateScene(e.detail)}
        on:edit={(e) => dispatch("open", e.detail)}
        on:duplicate={(e) => void duplicate(e.detail)}
        on:remove={(e) => void remove(e.detail)}
      />
    {/if}
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

  /* the library source's own "loading" / "nothing here" line */
  .pad {
    margin: 12px 20px 0;
  }

  /* The source chips took the left of the bar, so the sentence yields first:
     it is the one thing on the strip that is pure explanation. */
  @media (max-width: 900px) {
    .lede {
      display: none;
    }
  }

  /* S6d: the primary shrinks to an icon beside the source chips */
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
