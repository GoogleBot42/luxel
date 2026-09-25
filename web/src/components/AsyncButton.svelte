<script lang="ts">
  /**
   * The button for a verb that goes somewhere and comes back (Gitea #738).
   *
   * Jeremy: *"All save buttons should replace the 'Save' text with a spinner
   * when pressed. The spin should stop and say 'Saved' when the save is done
   * for one second. Then say 'Save' again. If possible, if we track if the
   * state is dirty, just have the 'Save' button say 'Saved' if there are no
   * dirty changes."* — and the same for every other button whose action is a
   * device round trip.
   *
   * So the lifecycle is four states, not two:
   *
   *   idle      `label`, or `doneLabel` when `settled` says there is nothing
   *             left to do (a clean document's Save already reads `Saved`)
   *   in flight the label is REPLACED by the spinner, and the button is
   *             `disabled` — a second click cannot start a second round trip
   *   done      `doneLabel` for `doneMs` (1 s), then back to idle
   *   refused   straight back to idle with no `doneLabel` flash — a failed
   *             save must never claim it saved
   *
   * `action` says which of the last two happened by RETURNING: `false` (or
   * throwing) is a refusal, anything else — `undefined` included — is
   * success. The reason for a refusal is not this component's business; the
   * caller has already put it on the one error strip or in a `note`.
   *
   * Nothing here is Save-specific: `label`/`doneLabel` are any verb and its
   * past tense (`Install`/`Installed`, `Reboot`/`Rebooting…`), and the
   * optional `icon` slot carries a mark in front of the word.
   *
   * Layout note: the box is FROZEN at its idle width for the whole flight, so
   * swapping a word for a 12px ring (and then for a longer word) never makes
   * the row around it jump.
   */
  import { onDestroy } from "svelte";

  /** What the verb is called while there is work to do. */
  export let label: string;
  /** What it is called just after it succeeded — and at rest when `settled`.
   *  Empty means "same as `label`", for a verb with no meaningful past tense. */
  export let doneLabel = "";
  /** "there is nothing to do": the document is not dirty, the update is
   *  already installed. Purely cosmetic — the button still works. */
  export let settled = false;
  /** The round trip. `false`/throw = refused, anything else = done. */
  export let action: () => unknown;
  /** The full class list of the rendered element — `btn primary`, `btn sm`,
   *  `mi` for a menu item. Everything visual is the shared chrome's (app.css),
   *  never a local copy (.claude/rules/web.md). */
  export let cls = "btn";
  /** Harness/contract handle. Omitted rather than empty when unset. */
  export let dataRole = "";
  export let title = "";
  /** Only when the word alone is not the accessible name (an icon-only verb). */
  export let ariaLabel = "";
  /** Disabled for a reason the CALLER owns. §5.7: a disabled control must say
   *  which budget it is up against, so `reason` is required with it. */
  export let disabled = false;
  export let reason = "";
  /** How long `doneLabel` holds after a success. */
  export let doneMs = 1000;
  /** For a button inside a clickable row (a gallery tile's action strip): the
   *  verb is not also "open this tile". A prop rather than the caller wrapping
   *  us in a `<span on:click|stopPropagation>`, because that wrapper becomes
   *  the flex item and takes the row's per-`.btn` `pointer-events` with it. */
  export let stopPropagation = false;

  type Phase = "idle" | "busy" | "done";
  let phase: Phase = "idle";
  let el: HTMLButtonElement | null = null;
  let frozen = 0;
  let timer: ReturnType<typeof setTimeout> | null = null;

  /** The word on the button right now. Every dependency is an ARGUMENT
   *  because this is called from the markup: a markup expression is re-patched
   *  with the dirty bits of what its own syntax names, and `{labelNow()}`
   *  naming nothing would freeze at its first render (.claude/rules/web.md,
   *  #469). */
  function labelNow(p: Phase, done: string, idle: string, rested: boolean): string {
    if (p === "done") return done || idle;
    return rested ? done || idle : idle;
  }

  /** Why the button is dark, in the two ways it can be. The in-flight one is
   *  still a `data-reason`, because the §5.7 sweep asserts that EVERY
   *  disabled element in the live DOM carries one (`disabledSweep`). */
  function reasonNow(p: Phase, off: boolean, why: string): string | undefined {
    if (p === "busy") return "the action is still running";
    return off ? why || undefined : undefined;
  }

  function stopTimer(): void {
    if (timer !== null) clearTimeout(timer);
    timer = null;
  }

  /** Run the verb. Exported so a keyboard shortcut drives the SAME button
   *  (⌘S in the pattern editor) instead of calling the action behind its
   *  back — which would save with no visible feedback, and could start a
   *  second save during the first. */
  export async function trigger(): Promise<void> {
    if (phase === "busy" || disabled) return;
    stopTimer();
    frozen = el?.offsetWidth ?? 0;
    phase = "busy";
    let ok = true;
    try {
      ok = (await action()) !== false;
    } catch {
      ok = false;
    }
    if (!ok) {
      phase = "idle";
      frozen = 0;
      return;
    }
    phase = "done";
    timer = setTimeout(() => {
      timer = null;
      phase = "idle";
      frozen = 0;
    }, doneMs);
  }

  onDestroy(stopTimer);
</script>

<button
  bind:this={el}
  class={cls}
  data-role={dataRole || undefined}
  data-reason={reasonNow(phase, disabled, reason)}
  title={title || undefined}
  aria-label={ariaLabel || undefined}
  aria-busy={phase === "busy"}
  disabled={disabled || phase === "busy"}
  style:min-width={frozen ? `${frozen}px` : ""}
  on:click={(e) => {
    if (stopPropagation) e.stopPropagation();
    void trigger();
  }}
>
  {#if phase === "busy"}
    <span class="spinner" data-role="btn-spinner" aria-hidden="true"></span>
  {:else}
    <slot name="icon" />
    <span class="lbl">{labelNow(phase, doneLabel, label, settled)}</span>
  {/if}
</button>
