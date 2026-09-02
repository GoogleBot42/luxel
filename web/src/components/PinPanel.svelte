<script lang="ts">
  import { createEventDispatcher } from "svelte";

  /** Digital pins the running pattern actually touches, ascending. The parent
   *  hides the whole section when this and `analogPins` are both empty — a
   *  pattern that never calls `pinMode`/`digitalRead` gets no pin panel at all
   *  (Gitea #205). */
  export let pins: number[] = [];
  /** Analog pins the pattern samples with `analogRead`/`touchRead`, ascending
   *  (Gitea #206). They get a 0..1 slider rather than a press/latch pair:
   *  there is no "pressed" state for a pot, only a position. */
  export let analogPins: number[] = [];
  /** Slider position per analog pin, 0..1 — what `analogRead`/`touchRead`
   *  report. Bound, like `latched`: the parent clears it on recompile, since a
   *  fresh engine reads 0 on every pin. */
  export let analogValues: Record<number, number> = {};
  /** What `digitalRead(pin)` reports right now, per pin. */
  export let levels: Record<number, boolean> = {};
  /** Pins whose IDLE level is HIGH (`pinMode` asked for a pull-up). Pressing
   *  such a pin pulls it LOW — the button-to-ground wiring. */
  export let idleHigh: Record<number, boolean> = {};
  /** Pins latched down (bound: the parent clears it on recompile, since a new
   *  engine starts with nothing driven). */
  export let latched: Record<number, boolean> = {};

  const dispatch = createEventDispatcher<{
    drive: { pin: number; level: boolean | null };
    analog: { pin: number; value: number };
  }>();

  /** Pointer/key currently held on the momentary button, per pin. */
  let held: Record<number, boolean> = {};

  /** The level a press drives the pin to: the OPPOSITE of its idle level, so
   *  "press" always changes what the pattern reads. A pulled-up pin goes LOW
   *  (button to ground); anything else goes HIGH. */
  function pressedLevel(pin: number): boolean {
    return !(idleHigh[pin] ?? false);
  }

  /** Momentary press and the latch are independent holds on the same pin —
   *  either one keeps it driven, neither leaves it released (idle). */
  function apply(pin: number): void {
    const on = (held[pin] ?? false) || (latched[pin] ?? false);
    dispatch("drive", { pin, level: on ? pressedLevel(pin) : null });
  }

  function press(pin: number): void {
    if (held[pin]) return;
    held = { ...held, [pin]: true };
    apply(pin);
  }

  function release(pin: number): void {
    if (!held[pin]) return;
    held = { ...held, [pin]: false };
    apply(pin);
  }

  function toggleLatch(pin: number): void {
    latched = { ...latched, [pin]: !latched[pin] };
    apply(pin);
  }

  /** Drag an analog pin's slider: the value goes straight into the engine, no
   *  press/release model — a pot sits where it is left. */
  function setAnalog(pin: number, e: Event): void {
    const value = Number((e.currentTarget as HTMLInputElement).value);
    analogValues = { ...analogValues, [pin]: value };
    dispatch("analog", { pin, value });
  }

  /** Space/Enter on the momentary button behaves like the pointer: down =
   *  driven, up = released. Buttons fire `click` on those keys, so without
   *  this the momentary control would be unusable from the keyboard. */
  function keyPress(pin: number, e: KeyboardEvent): void {
    if (e.key !== " " && e.key !== "Enter") return;
    e.preventDefault();
    if (!e.repeat) press(pin);
  }

  function keyRelease(pin: number, e: KeyboardEvent): void {
    if (e.key !== " " && e.key !== "Enter") return;
    e.preventDefault();
    release(pin);
  }
</script>

<div class="panel">
  {#each pins as pin (pin)}
    <div class="pin" data-role="pin-row" data-pin={pin}>
      <span class="label mono">GPIO {pin}</span>
      <button
        class="press"
        class:down={held[pin]}
        data-role="pin-press"
        data-pin={pin}
        title="hold to drive this pin {pressedLevel(pin) ? 'HIGH' : 'LOW'}"
        on:pointerdown|preventDefault={() => press(pin)}
        on:pointerup={() => release(pin)}
        on:pointerleave={() => release(pin)}
        on:pointercancel={() => release(pin)}
        on:keydown={(e) => keyPress(pin, e)}
        on:keyup={(e) => keyRelease(pin, e)}
        on:click|preventDefault
      >
        press
      </button>
      <button
        class="latch"
        class:on={latched[pin]}
        data-role="pin-latch"
        data-pin={pin}
        aria-pressed={latched[pin] ? "true" : "false"}
        title="keep this pin driven after the press is released"
        on:click={() => toggleLatch(pin)}
      >
        hold
      </button>
      <span class="level mono" class:high={levels[pin]} data-role="pin-level" data-pin={pin}>
        {levels[pin] ? "HIGH" : "LOW"}
      </span>
      <span class="dim idle">idle {idleHigh[pin] ? "HIGH (pull-up)" : "LOW"}</span>
    </div>
  {/each}
  {#each analogPins as pin (pin)}
    <div class="pin" data-role="analog-row" data-pin={pin}>
      <span class="label mono">GPIO {pin}</span>
      <input
        class="slider"
        type="range"
        min="0"
        max="1"
        step="0.01"
        data-role="analog-slider"
        data-pin={pin}
        title="what analogRead({pin}) / touchRead({pin}) reads"
        value={analogValues[pin] ?? 0}
        on:input={(e) => setAnalog(pin, e)}
      />
      <span class="level mono" data-role="analog-value" data-pin={pin}>
        {(analogValues[pin] ?? 0).toFixed(2)}
      </span>
      <span class="dim idle">analog in (0..1)</span>
    </div>
  {/each}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .pin {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .label {
    min-width: 72px;
    color: var(--text-dim);
  }

  .mono {
    font-family: ui-monospace, Menlo, Consolas, monospace;
    font-size: 12px;
  }

  .press {
    min-width: 62px;
    touch-action: none; /* a press-and-hold must not start a scroll gesture */
    user-select: none;
  }

  .press.down,
  .latch.on {
    background: var(--accent);
    color: var(--bg);
    border-color: var(--accent);
  }

  .latch {
    min-width: 48px;
  }

  .slider {
    flex: 1;
    min-width: 120px;
    max-width: 240px;
    accent-color: var(--accent);
  }

  .level {
    min-width: 42px;
    text-align: center;
    color: var(--text-dim);
  }

  .level.high {
    color: var(--accent);
  }

  .dim {
    color: var(--text-dim);
    font-size: 12px;
  }

  .idle {
    margin-left: auto;
    white-space: nowrap;
  }
</style>
