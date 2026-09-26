<script lang="ts">
  // The SPRITE layer's inspector, after the #740/#741 redesign and the
  // 2026-09-26 panel review:
  // Name · Sprite · Box · Size · Fit · FPS · the blend tail. And nothing else.
  //
  // What LEFT, and why:
  //   * Size / Frames / Palette as EDITABLE fields — those describe the
  //     SPRITE, not the layer, and a sprite is its own record with its own
  //     editor now. The Frames field in particular was the one Jeremy caught
  //     doing nothing ("I set frames to 2 and 3 and saw nothing change"); it
  //     lives in the sprite editor's frame strip, which is a real control.
  //   * "Black pixels · sprites are always keyed" — a dead line stating a fact
  //     about the format. Transparency is index 0 in the record now; there is
  //     nothing to choose.
  //   * The natural/explicit SPLIT. The box used to be "natural" while w/h
  //     were 0 — the fields showed 0, the inspector said `12×12 · natural
  //     size`, `Fit` was hidden, and then one handle drag or one typed number
  //     flipped the layer into a different mode. Jeremy: "the stretch fit
  //     should force the UI to see the w + h the same always (ex when the
  //     person is dragging the box or editing the bbox values directly)."
  //     So there is ONE model here now: a sprite layer's box always carries
  //     explicit w/h, the fields always show the real numbers, the marquee and
  //     the fields write the same two, `Fit` is always offered, and `natural`
  //     is a BUTTON that puts the box back to the sprite's own size. The wire
  //     still allows 0 (the firmware reads it as natural) — the editor
  //     normalises it on load, `pages/SceneEditor.svelte`.
  //
  // What ARRIVED: a real sprite PICKER over the sprite store, which PREVIEWS
  // each sprite animated at its own fps ("it doesn't actually preview the
  // sprite + its animation"), `Edit ↗` into the sprite editor with a return
  // route, and the layer's own FPS override ("it should also be possible to
  // override the FPS of the sprite in the layer settings" — the wire's `A`
  // line, `crates/luxel-core/src/scene.rs`).
  import { createEventDispatcher } from "svelte";
  import BoxRow from "./BoxRow.svelte";
  import StyleTail from "./StyleTail.svelte";
  import RichSelect from "../RichSelect.svelte";
  import SpriteThumb from "../sprite/SpriteThumb.svelte";
  import { FIT_OPTIONS, fitValue, type RichOption } from "../../lib/blendMeta";
  import {
    MAX_LAYER_NAME,
    MAX_SPRITE_FPS,
    truncateUtf8,
    type Fit,
    type Layer,
    type Rect,
  } from "../../lib/scene";
  import { spriteMetaLine, type Sprite } from "../../lib/sprite";
  import { cachedSprite, spriteRev, type SpriteMeta } from "../../stores/sprites";

  export let layer: Layer;
  /** The sprite library — the picker's rows. */
  export let library: SpriteMeta[] = [];
  /** True while the sprite store has a write in flight. */
  export let saving = false;

  const dispatch = createEventDispatcher<{
    change: Layer;
    /** Open the sprite editor on this layer's sprite, returning here. */
    edit: void;
    /** Make a blank sprite, bind it, and open it. */
    fresh: void;
  }>();

  $: id = layer.body.kind === "sprite" ? layer.body.id : "";
  $: current = library.find((s) => s.id === id) ?? null;
  /** The bound sprite's OWN size — what `natural` restores, and what the box
   *  is set to the moment a sprite is picked. Null while none is bound. */
  $: own = current ? { w: current.w, h: current.h } : null;
  $: atNatural = own !== null && layer.style.rect.w === own.w && layer.style.rect.h === own.h;

  /** The decoded records, so the picker can DRAW each sprite. A cache read
   *  with `$spriteRev` named as an argument, because `cachedSprite` is a plain
   *  Map and nothing about it invalidates a `$:` (.claude/rules/web.md). */
  $: records = recordsOf(library, $spriteRev);

  function recordsOf(lib: readonly SpriteMeta[], _rev: unknown): Map<string, Sprite> {
    const out = new Map<string, Sprite>();
    for (const s of lib) {
      const sp = cachedSprite(s.id);
      if (sp) out.set(s.id, sp);
    }
    return out;
  }

  /** One row per stored sprite, with its size and frame count as the row's
   *  sentence — the picker explains itself like every other `RichSelect`, and
   *  the drawing beside it is the sprite itself rather than a diagram. */
  $: options = library.map(
    (s): RichOption => ({
      value: s.id,
      label: s.name,
      desc: spriteMetaLine(s.w, s.h, s.frames),
      icon: "sprite",
    }),
  );

  // ---- the FPS override (the wire's `A` line) ----
  //
  // THREE states, and the row has to be honest about which one it is in:
  // `undefined` is the record's own rate (nothing on the wire), `0` is "hold
  // frame 0" and 1..30 is that rate. So the field is EMPTY for "the sprite's
  // own" — a 0 in it would be a real override — and `use sprite's` is how you
  // get back once you have typed one.
  $: override = layer.body.kind === "sprite" ? layer.body.fps : undefined;
  $: ownFps = current?.fps ?? 0;
  $: fpsLine =
    override === undefined
      ? `sprite's own (${ownFps === 0 ? "still" : `${ownFps} fps`})`
      : override === 0
        ? "0 = still"
        : `${override} fps`;

  function patchLayer(next: Partial<Layer>): void {
    dispatch("change", { ...layer, ...next });
  }

  function patchRect(rect: Rect): void {
    patchLayer({ style: { ...layer.style, rect } });
  }

  /** Binding a sprite SETS THE BOX to that sprite's own size. A layer whose
   *  box still said 0×0 was the old "natural" mode, and carrying it forward is
   *  what made the fields and the marquee disagree. */
  function setSprite(nextId: string): void {
    if (layer.body.kind !== "sprite" || nextId === layer.body.id) return;
    const picked = library.find((s) => s.id === nextId);
    const rect = picked
      ? { ...layer.style.rect, w: picked.w, h: picked.h }
      : { ...layer.style.rect };
    dispatch("change", {
      ...layer,
      style: { ...layer.style, rect },
      body: { ...layer.body, id: nextId },
    });
  }

  /** Fit is the ONE place `style.fit` means anything (`compose::blit_sprite`).
   *  The narrowing lives here rather than in the markup: svelte-check does not
   *  parse a TS assertion inside a template expression. */
  function setFit(v: string): void {
    patchLayer({ style: { ...layer.style, fit: v as Fit } });
  }

  /** The device refuses a layer name over 32 BYTES, and a refusal the console
   *  could have prevented is the console's bug — clamp on the way in and put
   *  the clamped value back in the field (`MAX_LAYER_NAME`). */
  function setName(el: HTMLInputElement): void {
    const name = truncateUtf8(el.value, MAX_LAYER_NAME);
    if (name !== el.value) el.value = name;
    patchLayer({ name });
  }

  /** Back to the sprite's own size — EXPLICITLY, not by zeroing the box. */
  function goNatural(): void {
    if (!own) return;
    patchRect({ ...layer.style.rect, w: own.w, h: own.h });
  }

  /** An empty field means "the sprite's own", which is the absent `A` line. */
  function setFps(el: HTMLInputElement): void {
    if (layer.body.kind !== "sprite") return;
    const raw = el.value.trim();
    if (raw === "") {
      useOwnFps();
      return;
    }
    const v = Number.parseInt(raw, 10);
    if (!Number.isFinite(v)) return;
    const fps = Math.max(0, Math.min(MAX_SPRITE_FPS, v));
    if (String(fps) !== raw) el.value = String(fps);
    dispatch("change", { ...layer, body: { ...layer.body, fps } });
  }

  function useOwnFps(): void {
    if (layer.body.kind !== "sprite") return;
    dispatch("change", { ...layer, body: { kind: "sprite", id: layer.body.id } });
  }
</script>

<div class="rhead" style="margin-bottom:12px">
  <div class="slabel">Sprite layer</div>
  {#if saving}<div class="rdim" style="margin-left:auto" data-role="scene-sprite-saving">saving…</div>{/if}
</div>

<div class="irow">
  <div class="ilab">Name</div>
  <input
    class="inp"
    style="width:100%"
    data-role="scene-layer-name"
    value={layer.name}
    on:change={(e) => setName(e.currentTarget)}
  />
</div>

<!-- The picker is ALWAYS here, not only while the layer is unbound: changing
     which sprite a layer draws is an ordinary edit, and the old inspector had
     no row for it once one was chosen. Each row DRAWS its sprite, animated at
     the record's own fps — a name and a size were never enough to tell two
     8×8 drawings apart (Jeremy, 2026-09-26). -->
<div class="irow start">
  <div class="ilab" style="padding-top:7px">Sprite</div>
  <div class="spritepick">
    {#if library.length === 0}
      <div class="hint" data-role="scene-sprite-state">no sprites yet</div>
    {:else}
      <RichSelect
        value={id}
        {options}
        iconSize={30}
        dataRole="scene-sprite-pick"
        menuRole="scene-sprite-menu"
        ariaLabel="which sprite this layer draws"
        on:input={(e) => setSprite(e.detail)}
      >
        <svelte:fragment slot="icon" let:option>
          <SpriteThumb
            sprite={records.get(option?.value ?? "") ?? null}
            size={30}
            playing
            dataRole="scene-sprite-thumb"
          />
        </svelte:fragment>
        <svelte:fragment slot="value" let:option>
          {#if option}
            <span class="tnm">{option.label}</span>
            <span class="tds" data-role="scene-sprite-pick-meta">{option.desc}</span>
          {:else}
            <span class="tnm">choose a sprite</span>
          {/if}
        </svelte:fragment>
      </RichSelect>
    {/if}
    <div class="pickrow">
      <button
        class="btn sm"
        data-role="scene-sprite-edit"
        disabled={id === ""}
        data-reason={id === "" ? "choose a sprite first, or make a new one" : null}
        title="open this sprite in the sprite editor"
        on:click={() => dispatch("edit")}>Edit ↗</button
      >
      <button class="btn sm" data-role="scene-sprite-new" on:click={() => dispatch("fresh")}
        >New…</button
      >
    </div>
  </div>
</div>

<div class="irule"></div>

<!-- ONE box model: w/h are always the real numbers, whatever set them — a
     typed value, a marquee handle, or `natural` below. `Fit` decides how the
     sprite fills them. -->
<BoxRow rect={layer.style.rect} on:input={(e) => patchRect(e.detail)} />

<div class="irow">
  <div class="ilab">Size</div>
  <div class="pickrow">
    <span class="hint" data-role="scene-sprite-natural">
      {own === null
        ? "no sprite chosen"
        : atNatural
          ? `${own.w}×${own.h} · the sprite's own size`
          : `the sprite is ${own.w}×${own.h}`}
    </span>
    <button
      class="btn sm"
      data-role="scene-sprite-natural-btn"
      disabled={own === null || atNatural}
      data-reason={own === null
        ? "choose a sprite first"
        : atNatural
          ? "the box is already the sprite's own size"
          : null}
      title="put the box back to the sprite's own size"
      on:click={goNatural}>natural</button
    >
  </div>
</div>

<!-- Always offered, because the box always has a size to fit into now. -->
<div class="irow">
  <div class="ilab">Fit</div>
  <RichSelect
    value={fitValue(layer.style.fit)}
    options={FIT_OPTIONS}
    dataRole="scene-sprite-fit"
    menuRole="scene-fit-menu"
    ariaLabel="how the sprite fills its box"
    on:input={(e) => setFit(e.detail)}
  />
</div>

<!-- The LAYER's frame rate, over the record's own (the `A` line). A blank
     field is "the sprite's own"; 0 holds frame 0. -->
<div class="irow">
  <div class="ilab">FPS</div>
  <div class="pickrow">
    <input
      class="inp fps"
      type="number"
      min="0"
      max={MAX_SPRITE_FPS}
      data-role="scene-sprite-fps"
      placeholder={String(ownFps)}
      aria-label="frames per second for this layer"
      title={`0 = still, up to ${MAX_SPRITE_FPS}; empty = the sprite's own rate`}
      value={override === undefined ? "" : String(override)}
      on:change={(e) => setFps(e.currentTarget)}
    />
    <span class="hint" data-role="scene-sprite-fps-state">{fpsLine}</span>
    {#if override !== undefined}
      <button
        class="btn sm"
        data-role="scene-sprite-fps-own"
        title="drop the override and use the sprite's own rate"
        on:click={useOwnFps}>use sprite's</button
      >
    {/if}
  </div>
</div>

<div class="irule"></div>

<StyleTail
  style={layer.style}
  kind="sprite"
  on:change={(e) => patchLayer({ style: e.detail })}
  on:delete
/>

<style>
  .spritepick {
    display: flex;
    flex-direction: column;
    gap: 7px;
    min-width: 0;
  }

  .pickrow {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  /* the trigger's two-part value: the name, then the size in the dim tone the
     menu rows use for the same words */
  .tnm {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tds {
    margin-left: 5px;
    font-size: 10.5px;
    color: var(--text-dim);
    white-space: nowrap;
  }

  /* the same 46px number field BoxRow uses, spinners and all */
  .fps {
    width: 46px;
    appearance: textfield;
    -moz-appearance: textfield;
  }

  .fps::-webkit-outer-spin-button,
  .fps::-webkit-inner-spin-button {
    appearance: none;
    margin: 0;
  }
</style>
