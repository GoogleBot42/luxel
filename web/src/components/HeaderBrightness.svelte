<script lang="ts">
  // Brightness, in the header bar (Jeremy, 2026-09-19: "brightness should be
  // a slider in the header bar"). No mockup frame has one — S3 only puts it
  // in Settings — so it is built to the header's scale: a 96px range left of
  // the fps readout, with the raw value beside it.
  //
  // The Settings control stays where it is; both drive the same store, so
  // they track each other. Console only: a playground has no brightness.
  //
  // ONE POST per drag: `input` moves the slider and the store (instant
  // feedback), `change` — which fires on pointer release and after a keyboard
  // adjustment — is what actually writes, debounced so a flurry of arrow keys
  // coalesces into a single request. The device applies and persists it.
  import { onDestroy } from "svelte";
  import { brightness, brightnessMax, device } from "../stores/device";

  /** Coalescing window for `change` events, in ms. */
  const SETTLE = 150;

  let timer: ReturnType<typeof setTimeout> | null = null;

  function onInput(e: Event): void {
    brightness.set(Number((e.target as HTMLInputElement).value));
  }

  function onChange(e: Event): void {
    const v = Number((e.target as HTMLInputElement).value);
    brightness.set(v);
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      void $device?.setBrightness(v);
    }, SETTLE);
  }

  onDestroy(() => {
    if (timer) clearTimeout(timer);
  });
</script>

<label class="br" title="device brightness — applied live and saved on the device">
  <!-- a glyph, not a word: the header is 44px of mostly-empty chrome in the
       mocks and "Brightness" would be the widest thing in it -->
  <svg class="ico" viewBox="0 0 16 16" aria-label="brightness" role="img">
    <circle cx="8" cy="8" r="3.1" fill="currentColor" />
    <g stroke="currentColor" stroke-width="1.3" stroke-linecap="round">
      <path d="M8 1v1.8M8 13.2V15M1 8h1.8M13.2 8H15" />
      <path d="M3.1 3.1l1.3 1.3M11.6 11.6l1.3 1.3M12.9 3.1l-1.3 1.3M4.4 11.6l-1.3 1.3" />
    </g>
  </svg>
  <input
    type="range"
    data-role="hdr-brightness"
    min="0"
    max={$brightnessMax}
    step="1"
    value={$brightness}
    on:input={onInput}
    on:change={onChange}
  />
  <span class="val" data-role="hdr-brightness-val">{$brightness}</span>
</label>

<style>
  .br {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }

  .ico {
    display: block;
    width: 13px;
    height: 13px;
    flex: none;
    color: var(--text-dim);
  }

  .br input[type="range"] {
    width: 96px;
    height: 16px;
  }

  .val {
    min-width: 16px;
    font: 11.5px/1 var(--mono);
    color: var(--text-dim);
    text-align: right;
  }
</style>
