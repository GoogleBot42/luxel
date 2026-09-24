// The eight device text slots (Gitea #485/#486, docs/api.md `/api/text`).
//
// A scene's `text slot n` source and a pattern's `textSlot(n)` read the same
// table, and this store is the browser's copy of it so both surfaces show the
// same words the panel does:
//
//   console    — `GET /api/text` on the shared poll scheduler, `POST` to write
//   playground — a local table, pushed into the wasm through
//                `Luxel.setTextSlot` so `textSlot(n)` in a pattern answers
//                with it exactly as the device's VM would
//
// A device that predates the endpoint advertises `caps.text_slots = 0`, and
// then there is nothing to poll and the slot source is not offered at all
// (docs/api.md's note on the field).

import { derived, get, writable, type Readable, type Writable } from "svelte/store";
import { TEXT_SLOTS } from "../lib/scene";
import { device, deviceCaps, pollSubscribe } from "./device";
import { luxel } from "./pattern";

const empty = (): string[] => new Array<string>(TEXT_SLOTS).fill("");

/** What each slot currently holds, index 0..7. */
export const textSlots: Writable<string[]> = writable(empty());

/** How many slots this host has. 0 hides the source entirely. The playground
 *  has no caps block and always has all eight (the wasm is the same core). */
export const textSlotCount: Readable<number> = derived([device, deviceCaps], ([d, c]) =>
  d === null ? TEXT_SLOTS : (c?.text_slots ?? 0),
);

/** Read one slot without subscribing — the inspector's echoed value. */
export function slotText(n: number): string {
  return get(textSlots)[n] ?? "";
}

/** Refresh the table from the device. A playground has no device and keeps
 *  whatever the UI last set. */
export async function refreshTextSlots(): Promise<void> {
  const d = get(device);
  if (!d) return;
  try {
    const slots = await d.text();
    const next = empty();
    for (let i = 0; i < next.length; i++) next[i] = slots[i] ?? "";
    textSlots.set(next);
    pushToWasm(next);
  } catch {
    /* older firmware without /api/text — the table stays empty */
  }
}

/** Register the poll. Called by the screens that show slot text, per the ONE
 *  scheduler rule (stores/device.ts) — never `setInterval`. */
export function startTextSlotPoll(): () => void {
  return pollSubscribe("text-slots", 2000, refreshTextSlots);
}

/**
 * Write a slot. On a console that is `POST /api/text` and then the echo; in
 * the playground it is the local table plus the wasm's own, so a pattern that
 * draws `textSlot(0)` previews what was typed.
 */
export async function setTextSlot(n: number, text: string): Promise<boolean> {
  const next = [...get(textSlots)];
  next[n] = text;
  textSlots.set(next);
  pushToWasm(next);
  const d = get(device);
  if (!d) return true;
  try {
    const r = await d.setText(n, text);
    await refreshTextSlots();
    return r.ok;
  } catch {
    return false;
  }
}

/** Mirror the table into the wasm so `textSlot(n)` answers with it. */
function pushToWasm(slots: string[]): void {
  const lx = get(luxel);
  if (!lx) return;
  for (let i = 0; i < slots.length; i++) lx.setTextSlot(i, slots[i] ?? "");
}
