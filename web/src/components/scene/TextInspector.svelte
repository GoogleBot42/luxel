<script lang="ts">
  // The TEXT layer's inspector — mockups S7 (the whole column) and S7h (the
  // three source states side by side), field for field.
  //
  // One inspector, three sources, and the rows that come WITH them (S7h's
  // note): Clock adds a format select and, when the device has no time yet,
  // one dim state line saying exactly what will be drawn instead — no toast,
  // no banner, and the row stays usable. Text slot adds the slot number, says
  // in one line who writes it, and echoes the current value under `Now` so
  // the layer is not a black box when the panel is in another room. Scroll is
  // the conditional-visibility rule again: Speed does not EXIST while
  // Scroll = none and appears directly under it the moment a direction is
  // chosen (§5.7 / mockups :1289-91).
  import { createEventDispatcher } from "svelte";
  import BoxRow from "./BoxRow.svelte";
  import FontPicker from "./FontPicker.svelte";
  import SceneSwatch from "./SceneSwatch.svelte";
  import StyleTail from "./StyleTail.svelte";
  import {
    ALIGNS,
    CLOCK_FMTS,
    DEFAULT_SCROLL_SPEED,
    SCROLLS,
    MAX_LAYER_NAME,
    truncateUtf8,
    type Align,
    type ClockFmt,
    type Layer,
    type SceneFont,
    type Scroll,
    type TextLayer,
  } from "../../lib/scene";
  import { clockStatus, isPlayground } from "../../stores/device";
  import { setTextSlot, textSlotCount, textSlots } from "../../stores/textSlots";

  export let layer: Layer;
  /** The layout's width, for the font picker's "~N chars wide" line. */
  export let gridW = 64;

  const dispatch = createEventDispatcher<{ change: Layer }>();

  $: text = layer.body.kind === "text" ? layer.body.text : null;

  const ALIGN_LABEL: Record<Align, string> = { l: "left", c: "center", r: "right" };

  /**
   * Does the align anchor still mean anything at this scroll setting?
   *
   * §5.7's absent-not-disabled rule, answered from the compositor rather than
   * from taste (`crates/luxel-core/src/compose.rs`). `draw_text_layer` puts
   * the string at `bx + align_off(align, bw, tw)` and then adds
   * `scroll_offset` — on the X axis for the horizontal modes, on the Y axis
   * for the vertical ones. Every HORIZONTAL running arm subtracts
   * `align_off` straight back out (`Left`: `bw - a - px…`; `Right`:
   * `px… - tw - a`; `Bounce`: `… - a`), which is deliberate — phase 0 has to
   * be the edge the text enters from, identically for all three alignments
   * (Gitea #733). So under left / right / bounce the anchor cancels and the
   * three buttons are three names for the same picture.
   *
   * `Up` and `Down` carry no `a` term and move the text vertically, so the
   * horizontal anchor is live and the row stays.
   *
   * The one seam: `scroll_offset` returns 0 at `speed == 0`, so a direction
   * parked at zero is static text, and static text IS aligned. The row comes
   * back there rather than lying about it — which also makes the slider's
   * left stop legible as "stopped".
   */
  function alignApplies(scroll: Scroll, speed: number): boolean {
    if (scroll === "none" || speed === 0) return true;
    return scroll === "up" || scroll === "down";
  }

  // Every dependency NAMED in the reactive statement's own syntax, and passed
  // as an ARGUMENT — a value the helper only reaches through a closure is not
  // a dependency as far as Svelte is concerned (.claude/rules/web.md).
  $: showAlign = text ? alignApplies(text.scroll, text.speed) : true;

  /** The device has a clock only once SNTP has answered; the playground never
   *  does — the browser's own clock is the one the layer draws from, and it
   *  is always right, so the state line is a console thing (S7h). */
  $: clockSynced = $isPlayground || ($clockStatus?.synced ?? false);

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

  $: slotValue = $textSlots[slotDraft] ?? "";
  $: slots = Array.from({ length: $textSlotCount }, (_, i) => i);

  // The narrowing lives in the script, not in the markup: a TS assertion
  // inside a template expression is not something svelte-check parses.
  function setFmt(v: string): void {
    fmtDraft = v as ClockFmt;
    patch({ source: { kind: "clock", fmt: fmtDraft } });
  }

  function setSlot(v: string): void {
    slotDraft = Number(v);
    patch({ source: { kind: "slot", slot: slotDraft } });
  }

  function setFont(f: SceneFont): void {
    patch({ font: f });
  }

  function setScroll(v: string): void {
    const scroll = v as Scroll;
    // Choosing a direction must MOVE the text. The wire default speed is 0
    // (it has to be — `serializeScene` omits the `F` line against it and
    // `TextLayer::default()` parses an absent one back as 0), so a layer that
    // had never been given a speed would say "scroll left" and sit perfectly
    // still. That, not the bounce kernel alone, is what "bounce mode does
    // nothing" was (Gitea #733). Bump only a ZERO speed, so switching
    // direction never overwrites a speed the user chose — and do it here
    // rather than in `newLayer`, so a scene loaded off a device with speed 0
    // is cured the moment its scroll is touched.
    if (!text) return;
    patch(scroll === "none" ? { scroll } : { scroll, speed: text.speed || DEFAULT_SCROLL_SPEED });
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

      <!-- Absent, not greyed, on a host with no slots: `caps.text_slots = 0`
           is firmware that predates the endpoint (docs/api.md). -->
      {#if $textSlotCount > 0}
        <div class="rrow" class:on={text.source.kind === "slot"}>
          <button class="pick" data-role="scene-text-slot" on:click={() => setSource("slot")}>
            <span class="radio"></span>Text slot{text.source.kind === "slot" ? "" : ` ${slotDraft}`}
          </button>
          {#if text.source.kind === "slot"}
            <select
              class="sel xs"
              style="flex:1"
              data-role="scene-text-slot-n"
              value={slotDraft}
              on:change={(e) => setSlot(e.currentTarget.value)}
            >
              {#each slots as n (n)}<option value={n}>{n}</option>{/each}
            </select>
          {/if}
        </div>
        <div class="hint" style="padding-left:21px" data-role="scene-text-slot-hint">
          set from the API or Home Assistant
        </div>
        {#if text.source.kind === "slot"}
          <div class="hint" style="padding-left:21px;margin-top:4px" data-role="scene-text-slot-how">
            slots 0–{$textSlotCount - 1} · <span class="mono" style="white-space:nowrap"
              >POST /api/text</span
            > · one HA text entity each
          </div>
        {/if}
      {/if}
    </div>
  </div>

  <!-- Clock, without a clock: one dim state line saying exactly what WILL be
       drawn, and the settings group that fixes it. The row above stays
       usable (S7h). -->
  {#if text.source.kind === "clock" && !clockSynced}
    <div class="irow start">
      <div class="ilab" style="padding-top:1px">Clock</div>
      <div>
        <div class="hint" data-role="scene-clock-state">
          not synced — the layer stays blank until the device gets the time
        </div>
        <div class="hint" style="margin-top:5px">
          <a class="lnk" href="#/settings" data-role="scene-clock-settings"
            >Settings › Clock &amp; time zone</a
          >
        </div>
      </div>
    </div>
  {/if}

  <!-- The slot's current value, echoed: "so the layer is not a black box when
       the panel is in another room" (S7h). -->
  {#if text.source.kind === "slot"}
    <div class="irow start">
      <div class="ilab" style="padding-top:1px">Now</div>
      <div>
        {#if $isPlayground}
          <!-- The playground has no API and no Home Assistant to be written
               FROM, so the row that echoes the slot is where you type it —
               the same trade the `Preview as` chip makes for a fixture. On a
               console it is the read-only echo S7h draws. -->
          <input
            class="inp xs"
            style="width:100%"
            data-role="scene-text-slot-value"
            placeholder="type what the API would write"
            value={slotValue}
            on:input={(e) => void setTextSlot(slotDraft, e.currentTarget.value)}
          />
        {:else}
          <div class="mono tiny" style="color:var(--text)" data-role="scene-text-slot-value">
            {slotValue}
          </div>
        {/if}
      </div>
    </div>
  {/if}

  <div class="irow">
    <div class="ilab">Font</div>
    <FontPicker value={text.font} {gridW} on:input={(e) => setFont(e.detail)} />
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

  <!-- ABSENT under a horizontal scroll, never greyed: the compositor cancels
       the align anchor there, so the three buttons would be three names for
       the same picture (`alignApplies`, §5.7). -->
  {#if showAlign}
    <div class="irow">
      <div class="ilab">Align</div>
      <div class="seg sm" data-role="scene-text-align">
        {#each ALIGNS as a (a)}
          <button
            class:on={text.align === a}
            data-role={`scene-align-${a}`}
            aria-pressed={text.align === a}
            on:click={() => patch({ align: a })}>{ALIGN_LABEL[a]}</button
          >
        {/each}
      </div>
    </div>
  {/if}

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
       control (S7h's third column, mockups.html :1289-91). In px/s, the unit
       the firmware takes. -->
  {#if text.scroll !== "none"}
    <div class="irow">
      <div class="ilab">Speed</div>
      <div class="speed">
        <input
          type="range"
          min="0"
          max="120"
          data-role="scene-text-speed"
          value={text.speed}
          on:input={(e) => patch({ speed: Number(e.currentTarget.value) })}
        />
        <span class="mono tiny dim" data-role="scene-text-speed-value">{text.speed} px/s</span>
      </div>
    </div>
  {/if}

  <BoxRow rect={layer.style.rect} on:input={(e) => patchLayer({ style: { ...layer.style, rect: e.detail } })} />

  {#if text.scroll !== "none"}
    <div class="hint" style="margin-top:12px" data-role="scene-scroll-window">
      The box is the scroll window: text longer than <i>w</i> is what scrolling is for, and it is
      clipped to the box in every direction.
    </div>
  {/if}

  <div class="irule"></div>

  <!-- No Transparent row: text is ALWAYS black-keyed (the glyphs are the
       layer, the space around them is not — `compose::draw_text_layer`), so
       there is nothing to offer. S7 draws Blend · Opacity · Delete and
       nothing between them. -->
  <StyleTail style={layer.style} on:change={(e) => patchLayer({ style: e.detail })} on:delete />
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
