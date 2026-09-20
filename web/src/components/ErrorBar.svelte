<script lang="ts">
  // THE error surface (Gitea #538 round 2). One strip at the TOP of every
  // screen, in the `--error` palette, in the same pinned family as
  // `settings/RebootBar.svelte` — which is at the bottom, in `--warn`,
  // because it reports something STORED. This one reports something that did
  // not happen.
  //
  // It replaces the 12px dim line at the foot of whichever form was refused:
  // "an /api/layout reply must surface as a prominent error banner at the top
  // of the page, never a tiny line at the bottom" (Jeremy, 2026-09-20).
  //
  // Two conditions share it, because to a user they are one question — "did
  // that work?":
  //
  //   * the device is not answering at all (`deviceDown`, driven by the
  //     status poll through `lib/fetchgate.ts`). A condition: it clears
  //     itself, counts up from the last answer, and has no ✕.
  //   * a request was REFUSED (`apiError`, translated by `lib/apiErrors.ts`).
  //     An event: it is dismissable, and it names the field it is about.
  //
  // The connectivity one wins when both are true — a refusal explained by a
  // dead board is one fact, not two.
  import { onDestroy } from "svelte";
  import { apiError, clearApiError } from "../stores/notify";
  import { deviceDown, deviceLastSeen, pollSubscribe } from "../stores/device";

  /** Ticks the "last seen 12 s ago" clock while the device is down. */
  let now = Date.now();
  let stopTick: (() => void) | undefined;
  $: {
    stopTick?.();
    stopTick = undefined;
    if ($deviceDown)
      stopTick = pollSubscribe("liveness-clock", 1000, () => {
        now = Date.now();
      });
  }
  onDestroy(() => stopTick?.());

  /** `12 s`, `3 min` — how long since anything answered. */
  function ago(ms: number): string {
    const s = Math.max(0, Math.round(ms / 1000));
    if (s < 90) return `${s} s`;
    return `${Math.round(s / 60)} min`;
  }

  $: lastSeen = $deviceLastSeen > 0 ? ago(now - $deviceLastSeen) : "";

  /** The table writes a wire line as `` `matrix 32 64 2 1 tr row 0 0` ``, so
   *  split on backticks and let the odd segments be real `<code>`. Nothing
   *  else in the sentence is markup — this is not a markdown renderer. */
  function segments(text: string): { code: boolean; s: string }[] {
    return text.split("`").map((s, i) => ({ code: i % 2 === 1, s }));
  }

  // ---- the field highlight ----
  //
  // The banner marks the control the device was talking about. It is done
  // here, by `data-role`, rather than threaded as a prop through nine
  // Settings cards: the attribute is the app's existing contract for "this
  // element is that thing", and one global rule in app.css paints it. A
  // resting page carries no `data-field-error` anywhere, so nothing the
  // mockups measure is touched.
  let marked: Element | null = null;
  function mark(role: string | undefined): void {
    if (typeof document === "undefined") return;
    if (marked) marked.removeAttribute("data-field-error");
    marked = role ? document.querySelector(`[data-role="${role}"]`) : null;
    marked?.setAttribute("data-field-error", "");
  }
  $: mark($apiError?.field);
  onDestroy(() => mark(undefined));
</script>

{#if $deviceDown}
  <div class="errbar down" data-role="device-down-bar" role="alert">
    <span class="msg">
      <b>Device unreachable</b> — retrying…{lastSeen ? ` last seen ${lastSeen} ago` : ""}
    </span>
  </div>
{:else if $apiError}
  {#key $apiError.at}
    <div class="errbar" data-role="api-error-bar" role="alert">
      <span class="msg">
        <span class="lead" data-role="api-error-text"
          >{#each segments($apiError.text) as seg}{#if seg.code}<code>{seg.s}</code
            >{:else}{seg.s}{/if}{/each}</span
        >
        <span class="det" data-role="api-error-details">the device said: {$apiError.details}</span>
      </span>
      <button
        class="btn sm eb"
        data-role="api-error-dismiss"
        aria-label="dismiss"
        on:click={clearApiError}>✕</button
      >
    </div>
  {/key}
{/if}

<style>
  /* the `.capstrip` family RebootBar wears, in the error palette and pinned
     to the TOP of the flow instead of the bottom of the viewport — it is the
     first thing on the page, above the tab's own content */
  .errbar {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 9px 12px;
    border-bottom: 1px solid rgba(224, 85, 85, 0.42);
    background: rgba(224, 85, 85, 0.13);
    color: #f2a3a3;
    font-size: 12.5px;
    line-height: 1.45;
  }

  .msg {
    flex: 1;
    min-width: 0;
  }

  .lead {
    display: block;
    color: #f5b9b9;
  }

  .lead code {
    padding: 0 3px;
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.28);
    font: 11.5px/1.4 var(--mono);
  }

  /* the device's own words, kept verbatim and quiet */
  .det {
    display: block;
    margin-top: 3px;
    font: 11px/1.4 var(--mono);
    color: rgba(242, 163, 163, 0.72);
    overflow-wrap: anywhere;
  }

  /* a condition, not an event: one line, no dismissal */
  .errbar.down .msg {
    align-self: center;
  }

  .errbar .eb {
    flex: none;
    border-color: rgba(224, 85, 85, 0.5);
    background: rgba(224, 85, 85, 0.14);
    color: #f2a3a3;
  }

  .errbar .eb:hover {
    border-color: var(--error);
    color: #ffd3d3;
  }
</style>
