// One modal-dialog primitive for the whole app.
//
// Before this (Gitea #472) naming and every confirmation went through
// `window.prompt` / `window.confirm` at eight call sites: unstyleable, not
// keyboard- or mobile-shaped, and untestable without a puppeteer
// `page.on("dialog")` handler that matched on the *message text*.
//
// The shape here is deliberately promise-returning so a call site keeps
// reading top-to-bottom:
//
//     if (!(await confirm({ title: "Delete pattern?", danger: true }))) return;
//     const name = await promptText({ title: "Save pattern", initial: suggestion });
//     if (name === null) return;   // cancelled
//
// `components/Dialog.svelte` is the single renderer; exactly one instance is
// mounted per app entry (the shell, and `flash/Flash.svelte` for the
// installer). It is the only thing that calls `submitDialog`/`cancelDialog`.

import { writable, type Readable } from "svelte/store";

/** Validation hook: return an error to show, or null when the value is fine. */
export type Validate = (value: string) => string | null;

/** The live request the renderer draws. `null` = no dialog on screen. */
export interface DialogState {
  kind: "confirm" | "prompt";
  title: string;
  /** Optional explanatory line under the title. */
  body: string;
  /** Renders the "this reboots the device" line (proposal §5.3). */
  reboot: boolean;
  /** Destructive: the primary button reads as a danger action. */
  danger: boolean;
  confirmLabel: string;
  cancelLabel: string;
  // prompt only
  label: string;
  initial: string;
  placeholder: string;
  validate: Validate;
}

export interface ConfirmOptions {
  title: string;
  body?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
  /** The action reboots the device — say so in the dialog. */
  reboot?: boolean;
}

export interface PromptOptions {
  title: string;
  body?: string;
  /** Field label; defaults to "Name" — this is the naming affordance. */
  label?: string;
  initial?: string;
  placeholder?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  /** Defaults to "a value is required" on empty. */
  validate?: Validate;
}

const inner = writable<DialogState | null>(null);

/** The dialog on screen, or null. Read by `components/Dialog.svelte` only. */
export const dialog: Readable<DialogState | null> = { subscribe: inner.subscribe };

/** Resolver for the request currently on screen. */
let pending: ((value: string | null) => void) | null = null;

const requireValue: Validate = (v) => (v.trim() === "" ? "a value is required" : null);

/** Open `state` and resolve with the entered text, or null when cancelled. */
function open(state: DialogState): Promise<string | null> {
  // Never stack: a second request while one is up (a stray Ctrl+S behind the
  // modal, a double click) resolves as "cancelled" rather than replacing the
  // dialog the user is looking at.
  if (pending !== null) return Promise.resolve(null);
  return new Promise<string | null>((resolve) => {
    pending = resolve;
    inner.set(state);
  });
}

function settle(value: string | null): void {
  const resolve = pending;
  pending = null;
  inner.set(null);
  resolve?.(value);
}

/** The renderer accepted: `value` is the (trimmed) prompt text, "" for a confirm. */
export function submitDialog(value: string): void {
  settle(value);
}

/** The renderer cancelled (Cancel, Escape, backdrop). */
export function cancelDialog(): void {
  settle(null);
}

/** Yes/no. Resolves false on cancel, Escape or a backdrop click. */
export function confirm(o: ConfirmOptions): Promise<boolean> {
  return open({
    kind: "confirm",
    title: o.title,
    body: o.body ?? "",
    reboot: o.reboot ?? false,
    danger: o.danger ?? false,
    confirmLabel: o.confirmLabel ?? "OK",
    cancelLabel: o.cancelLabel ?? "Cancel",
    label: "",
    initial: "",
    placeholder: "",
    validate: () => null,
  }).then((v) => v !== null);
}

/** Ask for a line of text. Resolves null on cancel; never resolves "" unless
 *  a custom `validate` allows it. */
export function promptText(o: PromptOptions): Promise<string | null> {
  return open({
    kind: "prompt",
    title: o.title,
    body: o.body ?? "",
    reboot: false,
    danger: false,
    confirmLabel: o.confirmLabel ?? "OK",
    cancelLabel: o.cancelLabel ?? "Cancel",
    label: o.label ?? "Name",
    initial: o.initial ?? "",
    placeholder: o.placeholder ?? "",
    validate: o.validate ?? requireValue,
  });
}
