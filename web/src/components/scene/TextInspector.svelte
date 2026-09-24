<script lang="ts">
  // The TEXT layer's inspector — mockup S7, field for field.
  //
  // First cut (#480): name · source (fixed | clock | text slot) · font ·
  // colour · align · scroll (+ speed only when scroll ≠ none) · box · blend ·
  // opacity · Delete layer. WEB-C extends it against S7h/S7i (#486): the
  // clock's "not synced" state line, the slot picker's echoed value, and the
  // font picker with its 1:1 glyph samples. Keep the file here so that work
  // is an edit rather than a second component.
  //
  // Speed is ABSENT at scroll = none, not disabled (§5.7 / mockups :1289-91),
  // and visibility is not here at all — it lives on the eye in the layer list.
  import { createEventDispatcher } from "svelte";
  import BoxRow from "./BoxRow.svelte";
  import SceneSwatch from "./SceneSwatch.svelte";
  import StyleTail from "./StyleTail.svelte";
  import {
    ALIGNS,
    CLOCK_FMTS,
    FONTS,
    SCROLLS,
    MAX_LAYER_NAME,
    TEXT_SLOTS,
    truncateUtf8,
    type Align,
    type ClockFmt,
    type Layer,
    type SceneFont,
    type Scroll,
    type TextLayer,
  } from "../../lib/scene";

  export let layer: Layer;

  const dispatch = createEventDispatcher<{ change: Layer }>();

  $: text = layer.body.kind === "text" ? layer.body.text : null;

  /** The mock's own labels (S7 `5×7 regular`, S7i the three built-ins). */
  const FONT_LABEL: Record<SceneFont, string> = {
    tiny: "4×6 tiny",
    regular: "5×7 regular",
    large: "5×8 large",
  };

  const ALIGN_LABEL: Record<Align, string> = { l: "left", c: "center", r: "right" };

  function patch(next: Partial<TextLayer>): void {
    if (!text) return;
    dispatch("change", { ...layer, body: { kind: "text", text: { ...text, ...next } } });
  }

  function patchLayer(next: Partial<Layer>): void {
    dispatch("change", { ...layer, ...next });
  }

  /** The device refuses a layer name over 32 BYTES, and a refusal the console
   *  could have prevented is the console's bug — clamp on the way in and put
   *  the clamped value back in the field so what you see is what is stored
   *  (`MAX_LAYER_NAME`, docs/spec/scenes.md §1). */
  function setName(el: HTMLInputElement): void {
    const name = truncateUtf8(el.value, MAX_LAYER_NAME);
    if (name !== el.value) el.value = name;
    patchLayer({ name });
  }

  function setSource(kind: "lit" | "clock" | "slot"): void {
    if (!text) return;
    if (kind === "lit") patch({ source: { kind: "lit", text: litDraft } });
    else if (kind === "clock") patch({ source: { kind: "clock", fmt: fmtDraft } });
    else patch({ source: { kind: "slot", slot: slotDraft } });
  }

  // The two inactive rows keep what you last typed in them, so switching
  // source and switching back does not lose it.
  let litDraft = "";
  let fmtDraft: ClockFmt = "HH:MM";
  let slotDraft = 0;
  $: if (text?.source.kind === "lit") litDraft = text.source.text;
  $: if (text?.source.kind === "clock") fmtDraft = text.source.fmt;
  $: if (text?.source.kind === "slot") slotDraft = text.source.slot;

  // The narrowing lives in the script, not in the markup: a TS assertion
  // inside a template expression is not something svelte-check parses.
  function setFmt(v: string): void {
    fmtDraft = v as ClockFmt;
    patch({ source: { kind: "clock", fmt: fmtDraft } });
  }

  function setFont(v: string): void {
    patch({ font: v as SceneFont });
  }

  function setScroll(v: string): void {
    patch({ scroll: v as Scroll });
  }
</script>

{#if text}
  <div class="rhead" style="margin-bottom:12px"><div class="slabel">Text layer</div></div>

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

  <div class="irow start">
    <div class="ilab">Text</div>
    <div>
      <!-- The mock draws `.rrow` as one line: the radio, its word, and the
           row's own control. A control inside a `<button>` is not legal HTML,
           so the radio and its word are the button and the control is its
           sibling — the flex line, its gap and its metrics are unchanged. -->
      <div class="rrow" class:on={text.source.kind === "lit"}>
        <button class="pick" data-role="scene-text-fixed" on:click={() => setSource("lit")}>
          <span class="radio"></span>Fixed
        </button>
        <input
          class="inp xs"
          style="flex:1;min-width:0"
          data-role="scene-text-lit"
          value={litDraft}
          on:input={(e) => {
            litDraft = e.currentTarget.value;
            patch({ source: { kind: "lit", text: litDraft } });
          }}
        />
      </div>
      <div class="rrow" class:on={text.source.kind === "clock"}>
        <button class="pick" data-role="scene-text-clock" on:click={() => setSource("clock")}>
          <span class="radio"></span>Clock
        </button>
        <select
          class="sel xs"
          style="flex:1"
          data-role="scene-text-fmt"
          value={fmtDraft}
          on:change={(e) => setFmt(e.currentTarget.value)}
        >
          {#each CLOCK_FMTS as f (f)}<option value={f}>{f}</option>{/each}
        </select>
      </div>
      <div class="rrow" class:on={text.source.kind === "slot"}>
        <button class="pick" data-role="scene-text-slot" on:click={() => setSource("slot")}>
          <span class="radio"></span>Text slot {slotDraft + 1}
        </button>
      </div>
      <div class="hint" style="padding-left:21px">set from the API or Home Assistant</div>
    </div>
  </div>

  <div class="irow">
    <div class="ilab">Font</div>
    <select
      class="sel wide"
      data-role="scene-text-font"
      value={text.font}
      on:change={(e) => setFont(e.currentTarget.value)}
    >
      {#each FONTS as f (f)}<option value={f}>{FONT_LABEL[f]}</option>{/each}
    </select>
  </div>

  <div class="irow">
    <div class="ilab">Color</div>
    <div>
      <SceneSwatch
        value={text.color}
        label="text colour"
        dataRole="scene-text-color"
        on:input={(e) => patch({ color: e.detail })}
      />
    </div>
  </div>

  <div class="irow">
    <div class="ilab">Align</div>
    <div class="seg sm" data-role="scene-text-align">
      {#each ALIGNS as a (a)}
        <button class:on={text.align === a} data-role={`scene-align-${a}`} on:click={() => patch({ align: a })}
          >{ALIGN_LABEL[a]}</button
        >
      {/each}
    </div>
  </div>

  <div class="irow">
    <div class="ilab">Scroll</div>
    <select
      class="sel wide"
      data-role="scene-text-scroll"
      value={text.scroll}
      on:change={(e) => setScroll(e.currentTarget.value)}
    >
      {#each SCROLLS as s (s)}<option value={s}>{s}</option>{/each}
    </select>
  </div>

  <!-- ABSENT at `none`, never greyed: a speed with nothing to move is not a
       control (mockups.html :1289-91). -->
  {#if text.scroll !== "none"}
    <div class="irow">
      <div class="ilab">Speed</div>
      <div class="speed">
        <input
          class="inp num"
          type="number"
          min="0"
          max="65535"
          data-role="scene-text-speed"
          value={text.speed}
          on:change={(e) => patch({ speed: Number(e.currentTarget.value) })}
        />
        <span class="hint">px/s</span>
      </div>
    </div>
  {/if}

  <BoxRow rect={layer.style.rect} on:input={(e) => patchLayer({ style: { ...layer.style, rect: e.detail } })} />

  <div class="irule"></div>

  <StyleTail
    style={layer.style}
    on:change={(e) => patchLayer({ style: e.detail })}
    on:delete
  />
{/if}

<style>
  /* the radio + its word, transparent inside the `.rrow` flex line */
  .pick {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0;
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
    white-space: nowrap;
  }

  .pick:hover {
    border-color: transparent;
  }

  .speed {
    display: flex;
    align-items: center;
    gap: 10px;
  }
</style>
