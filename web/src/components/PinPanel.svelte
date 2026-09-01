<script lang="ts">
  import { createEventDispatcher } from "svelte";

  /** Digital pins the running pattern actually touches, ascending. The parent
   *  hides the whole section when this is empty — a pattern that never calls
   *  `pinMode`/`digitalRead` gets no pin panel at all (Gitea #205). */
  export let pins: number[] = [];
  /** What `digitalRead(pin)` reports right now, per pin. */
  export let levels: Record<number, boolean> = {};
  /** Pins whose IDLE level is HIGH (`pinMode` asked for a pull-up). Pressing
   *  such a pin pulls it LOW — the button-to-ground wiring. */
  export let idleHigh: Record<number, boolean> = {};
  /** Pins latched down (bound: the parent clears it on recompile, since a new
   *  engine starts with nothing driven). */
  export let latched: Record<number, boolean> = {};

  const dispatch = createEventDispatcher<{ drive: { pin: number; level: boolean | null } }>();

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
