<script lang="ts">
  // The one modal renderer. Exactly one instance is mounted per app entry
  // (the shell; `flash/Flash.svelte` for the installer) and it draws whatever
  // `stores/dialog.ts` has open — see that file for the call-site API.
  //
  // Keyboard contract: Escape cancels, Enter confirms (from the text field or
  // anywhere that isn't a button — buttons already act on Enter themselves),
  // Tab cycles inside the panel and nowhere else, and focus returns to
  // whatever had it when the dialog closes.
  import { tick } from "svelte";
  import { cancelDialog, dialog, submitDialog, type DialogState } from "../stores/dialog";

  let panel: HTMLDivElement | undefined;
  let field: HTMLInputElement | undefined;
  let okBtn: HTMLButtonElement | undefined;
  let value = "";
  let error = "";
  /** What had focus before we opened, to hand it back on close. */
  let restore: HTMLElement | null = null;

  $: onRequest($dialog);

  function onRequest(d: DialogState | null): void {
    if (d === null) {
      const back = restore;
      restore = null;
      if (back) void tick().then(() => back.focus());
      return;
    }
    value = d.initial;
    error = "";
    restore = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    void tick().then(() => {
      if (field) {
        field.focus();
        field.select(); // the suggested name is replaceable by typing
      } else {
        okBtn?.focus();
      }
    });
  }

  function submit(): void {
    const d = $dialog;
    if (!d) return;
    if (d.kind === "confirm") {
      submitDialog("");
      return;
    }
    const msg = d.validate(value);
    if (msg !== null) {
      error = msg; // stay open — nothing is disabled, the reason is shown
      field?.focus();
      return;
    }
    submitDialog(value.trim());
  }

  /** The focusable controls inside the panel, in tab order. */
  function focusables(): HTMLElement[] {
    if (!panel) return [];
    return [...panel.querySelectorAll<HTMLElement>("button, input, [href], select, textarea")].filter(
      (el) => !el.hasAttribute("disabled"),
    );
  }

  function onKeydown(e: KeyboardEvent): void {
    if (!$dialog) return;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      cancelDialog();
      return;
    }
    if (e.key === "Enter" && !(e.target instanceof HTMLButtonElement)) {
      e.preventDefault();
      e.stopPropagation();
      submit();
      return;
    }
    if (e.key !== "Tab") return;
    // focus trap: wrap at both ends, and pull focus back in if it escaped
    const items = focusables();
    if (items.length === 0) return;
    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement;
    if (!(active instanceof HTMLElement) || !panel?.contains(active)) {
      e.preventDefault();
      first?.focus();
      return;
    }
    if (e.shiftKey && active === first) {
      e.preventDefault();
      last?.focus();
    } else if (!e.shiftKey && active === last) {
      e.preventDefault();
      first?.focus();
    }
  }
</script>

<svelte:window on:keydown|capture={onKeydown} />

{#if $dialog}
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
  <div class="backdrop" data-role="dialog-backdrop" on:mousedown|self={cancelDialog}>
    <div
      class="panel"
      class:danger={$dialog.danger}
      data-role="dialog"
      bind:this={panel}
      role="dialog"
      aria-modal="true"
      aria-labelledby="lx-dialog-title"
    >
      <h2 id="lx-dialog-title" data-role="dialog-title">{$dialog.title}</h2>
      {#if $dialog.body}
        <p class="body" data-role="dialog-body">{$dialog.body}</p>
      {/if}
      {#if $dialog.kind === "prompt"}
        <label class="field">
          <span class="flabel">{$dialog.label}</span>
          <input
            data-role="dialog-input"
            bind:this={field}
            bind:value
            placeholder={$dialog.placeholder}
            spellcheck="false"
            autocomplete="off"
            on:input={() => (error = "")}
          />
        </label>
      {/if}
      {#if $dialog.reboot}
        <p class="reboot" data-role="dialog-reboot">
          The device <strong>reboots</strong> to apply this and is unreachable for a few seconds.
        </p>
      {/if}
      {#if error}
        <p class="error" data-role="dialog-error">{error}</p>
      {/if}
      <div class="buttons">
        <button data-role="dialog-cancel" on:click={cancelDialog}>{$dialog.cancelLabel}</button>
        <button
          class="primary"
          class:danger={$dialog.danger}
          data-role="dialog-confirm"
          bind:this={okBtn}
          on:click={submit}
        >
          {$dialog.confirmLabel}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  /* above the boot cover (z-index 100) — a dialog is never behind anything */
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 200;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
    background: rgba(0, 0, 0, 0.55);
  }

  .panel {
    width: min(440px, 100%);
    max-height: calc(100vh - 32px);
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 16px 18px;
    border: 1px solid var(--border);
    border-top: 2px solid var(--accent);
    border-radius: 8px;
    background: var(--bg-panel);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
  }

  .panel.danger {
    border-top-color: var(--error);
  }

  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: var(--text);
  }

  .body {
    margin: 0;
    color: var(--text-dim);
    font-size: 13px;
    line-height: 1.5;
    overflow-wrap: anywhere;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .flabel {
    color: var(--text-dim);
    font-size: 12px;
  }

  .field input {
    width: 100%;
    padding: 6px 8px;
  }

  .field input:focus {
    outline: none;
    border-color: var(--accent);
  }

  .reboot {
    margin: 0;
    padding: 6px 8px;
    border: 1px solid color-mix(in srgb, var(--warn) 45%, transparent);
    border-radius: 6px;
    background: color-mix(in srgb, var(--warn) 12%, transparent);
    color: var(--warn);
    font-size: 12px;
    line-height: 1.45;
  }

  .error {
    margin: 0;
    color: var(--error);
    font-size: 12px;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 2px;
  }

  .primary {
    background: var(--accent);
    border-color: var(--accent);
    color: #1a1207;
    font-weight: 600;
  }

  .primary:hover {
    filter: brightness(1.08);
  }

  .primary.danger {
    background: var(--error);
    border-color: var(--error);
    color: #1a0808;
  }

  /* D9: responsive stacking — on a phone the actions become full-width rows
     with the primary on top (column-reverse keeps DOM/tab order cancel→ok) */
  @media (max-width: 420px) {
    .backdrop {
      align-items: flex-end;
      padding: 8px;
    }

    .buttons {
      flex-direction: column-reverse;
      gap: 6px;
    }

    .buttons button {
      width: 100%;
      padding: 8px;
    }
  }
</style>
