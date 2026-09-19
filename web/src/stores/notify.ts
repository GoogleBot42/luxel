// One transient-notification primitive for the whole app.
//
// Before the decomposition (Gitea #462) there were nine independent `*Note`
// strings — `saveNote`, `shareNote`, `mapError`, `micError`, `wifiNote`,
// `mqttNote`, `dataPinNote`, `apNote`, `paletteNote` — each with its own
// `setTimeout` to clear it. They are all the same thing: a short line of text
// attached to one surface, sometimes self-clearing. `note()` is that thing;
// the channel names below are the surfaces.
//
// `banners` is the promoted, longer-lived list: things that stay until the
// condition behind them goes away (the wasm failed to load, the device is
// unreachable). It is keyed so a repeated push replaces rather than stacks,
// and insertion-ordered so the render order is stable.

import { writable, type Readable } from "svelte/store";

/** The surfaces a transient note can be attached to. */
export type NoteChannel =
  | "save"
  | "share"
  | "map"
  | "mic"
  | "wifi"
  | "mqtt"
  | "datapin"
  | "ap"
  | "palette";

export type Notes = Partial<Record<NoteChannel, string>>;

const notesInner = writable<Notes>({});

/** Every live note, keyed by channel. Read as `$notes.save` in markup. */
export const notes: Readable<Notes> = { subscribe: notesInner.subscribe };

const timers = new Map<NoteChannel, ReturnType<typeof setTimeout>>();

/**
 * Show `text` on `channel`, replacing whatever was there. `ttlMs > 0` clears
 * it again after that long (the old `setTimeout(() => (xNote = ""), n)`);
 * `ttlMs = 0` leaves it up until something else changes it — the map/runtime
 * errors that must persist until the next run.
 */
export function note(channel: NoteChannel, text: string, ttlMs = 0): void {
  const existing = timers.get(channel);
  if (existing !== undefined) {
    clearTimeout(existing);
    timers.delete(channel);
  }
  notesInner.update((n) => ({ ...n, [channel]: text }));
  if (text !== "" && ttlMs > 0) {
    timers.set(
      channel,
      setTimeout(() => {
        timers.delete(channel);
        notesInner.update((n) => ({ ...n, [channel]: "" }));
      }, ttlMs),
    );
  }
}

/** Clear one channel now (and cancel any pending expiry). */
export function clearNote(channel: NoteChannel): void {
  note(channel, "");
}

// ---- banners ----

export interface Banner {
  /** Stable key: pushing the same id replaces the banner in place. */
  id: string;
  level: "error" | "warn";
  text: string;
  /** e2e hook, when the banner needs one. */
  role?: string;
}

const bannersInner = writable<Banner[]>([]);

/** The app-level banner stack, in insertion order. */
export const banners: Readable<Banner[]> = { subscribe: bannersInner.subscribe };

/**
 * Upsert a banner by id, or remove it when `banner` is null. Insertion order
 * is preserved across updates so a banner that comes and goes does not jump
 * around the stack.
 */
export function setBanner(id: string, banner: Omit<Banner, "id"> | null): void {
  bannersInner.update((list) => {
    const i = list.findIndex((b) => b.id === id);
    if (banner === null) return i < 0 ? list : [...list.slice(0, i), ...list.slice(i + 1)];
    const next = { id, ...banner };
    if (i < 0) return [...list, next];
    const copy = [...list];
    copy[i] = next;
    return copy;
  });
}

