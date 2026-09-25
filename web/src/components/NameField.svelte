<script lang="ts">
  // THE click-to-rename title control, shared by every full-screen editor
  // header (Gitea #736 item 30, the cleanest instance of #737's reuse audit).
  //
  // The pattern editor grew this first and got every detail right; the scene
  // editor hand-copied the markup and lost three of them — the focus, the
  // select-all, the inline refusal of an empty name — so clicking the scene's
  // title put a cursor nowhere and you had to click again. One component now,
  // so there is one place for those details to be right.
  //
  // It owns the INTERACTION only. What a name MEANS — which store it lands
  // in, whether it makes the document dirty, whether it is pushed — is the
  // page's, and arrives as `on:commit`. That split is why the pattern
  // editor can keep its "unchanged name is not an edit" guard while the
  // scene editor clamps to 64 wire bytes, with no flags in here.
  //
  // The box is `.nameedit` / `.nametext` from components/editor-frame.css —
  // the shared chrome, never a local fork (.claude/rules/web.md).
  import { createEventDispatcher, tick } from "svelte";
  import { truncateUtf8 } from "../lib/scene";

  /** What the resting BUTTON shows. May be a display fallback that is not a
   *  real name ("untitled pattern"). */
  export let value = "";
  /** What the FIELD is seeded with when it opens — the raw name, which is
   *  `""` for a document that has none.
   *
   *  These are two different strings and conflating them is a real bug, not
   *  a tidiness point: seeding the field from `value` pre-fills it with the
   *  display fallback, so Enter COMMITS "untitled pattern" as if the user had
   *  typed it and the empty-name refusal below can never fire. Caught by
   *  e2e's "an empty name is rejected inline" the first time this component
   *  was adopted (Gitea #736/#737). Defaults to `value` for the callers
   *  where the two genuinely are the same. */
  export let seed: string | null = null;
  /** The field's accessible name — "pattern name", "scene name". */
  export let label = "name";
  /** `data-role` on the resting button, the open field and the inline error.
   *  Three props rather than one prefix: both existing harnesses already
   *  name these elements and neither spelling is derivable from the other
   *  (`pattern-name` / `name-input` / `name-error`). */
  export let dataRole = "name";
  export let inputRole = "name-input";
  export let errorRole = "name-error";
  export let title = "click to rename";
  /** Clamp the committed name to this many UTF-8 BYTES; 0 = no clamp. A
   *  refusal the console could have prevented is the console's bug, so the
   *  scene editor passes `MAX_SCENE_NAME` here rather than letting the
   *  device say no (docs/spec/scenes.md §1). */
  export let maxBytes = 0;

  const dispatch = createEventDispatcher<{ commit: string; cancel: void }>();

  let editing = false;
  let draft = "";
  let error = "";
  let input: HTMLInputElement | undefined;

  /**
   * Open the field, focused, with its text selected — the whole of item 30.
   * `reason` seeds the inline rejection for the case where the rename is
   * forced by something else (Save on an unnamed pattern).
   */
  export function start(reason = ""): void {
    draft = seed ?? value;
    error = reason;
    editing = true;
    void tick().then(() => {
      input?.focus();
      input?.select();
    });
  }

  /** Close without committing — for a page that has just replaced its
   *  document underneath the header (`resetDocumentState`). */
  export function reset(): void {
    editing = false;
    error = "";
  }

  /** Enter or blur commits; an empty name is refused IN PLACE, so nothing is
   *  ever disabled and the field stays open with the reason (§5.7). */
  function commit(): void {
    if (!editing) return;
    const next = draft.trim();
    if (next === "") {
      error = "a name is required";
      void tick().then(() => input?.focus());
      return;
    }
    editing = false;
    error = "";
    dispatch("commit", maxBytes > 0 ? truncateUtf8(next, maxBytes) : next);
  }

  function cancel(): void {
    editing = false;
    error = "";
    dispatch("cancel");
  }

  function onKey(e: KeyboardEvent): void {
    if (e.key === "Enter") {
      e.preventDefault();
      commit();
    } else if (e.key === "Escape") {
      e.preventDefault();
      cancel();
    }
  }
</script>

{#if editing}
  <input
    class="nameedit"
    data-role={inputRole}
    bind:this={input}
    bind:value={draft}
    aria-label={label}
    on:keydown={onKey}
    on:blur={commit}
    on:click|stopPropagation
  />
{:else}
  <button class="nameedit" data-role={dataRole} {title} on:click|stopPropagation={() => start()}>
    <!-- the clamp is on an INNER span: mockup `.nameedit` neither wraps nor
         clips, and that element is what mockdiff measures -->
    <span class="nametext">{value}</span>
  </button>
{/if}
{#if error}<span class="name-error" data-role={errorRole}>{error}</span>{/if}

<style>
  .name-error {
    color: var(--error);
    font-size: 12px;
  }
</style>
