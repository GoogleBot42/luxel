// The sprite library — one store, two backings (Gitea #740), modelled line
// for line on `stores/scenes.ts`.
//
//   * CONSOLE: `/api/sprites` on the device. Each sprite is its OWN record
//     there (its own id namespace, its own routes), capped at 16 KiB — which
//     is why `maxBytes` is part of the read and why a save can be refused
//     with `sprite: over the 16 KiB cap`.
//   * PLAYGROUND: `localStorage` under `luxel.sprites`, holding `{id, b64}`
//     records. The BYTES are the same `LXSP` record the device stores, so a
//     sprite drawn in the playground can be POSTed at a device unchanged.
//
// What lives here: the metadata list, the decoded-record cache and the verbs.
// What does NOT: the codec (`lib/sprite.ts`) and any rendering
// (`components/sprite/`).
//
// THE MIGRATION lives here too. Before #740 a sprite was a `// @sprite`
// PATTERN; `migrateTaggedSprites()` converts every one it finds, re-points
// the scenes that named it and deletes the pattern, once per session, on
// both backings. That is the "keep the old tag readable for one release"
// half of #740 step 3 — and it is why sprites leave the Patterns library.

import { get, writable, type Writable } from "svelte/store";
import {
  compactPalette,
  decodeSprite,
  encodeSprite,
  isTaggedSpriteSource,
  newSprite,
  SPRITE_MAX_BYTES,
  spriteBytes,
  spriteFromTaggedPattern,
  type Sprite,
} from "../lib/sprite";
import { playgroundPatternId } from "../lib/sceneRender";
import { deletePattern, listAllPatterns } from "../lib/store";
import { device, devicePatterns, pollSubscribe, refreshDevicePatterns } from "./device";
import { note, reportApiError } from "./notify";
import { nextCopyName, refreshScenes, saveScene, scenes } from "./scenes";
import type { Scene } from "../lib/scene";

/** Where the playground keeps its library — the 6th `lib/store.ts`-style key,
 *  kept here rather than there so the sprite format has ONE owner. */
const LS_SPRITES = "luxel.sprites";

/** One row of the library: what a tile and a picker row need, and never the
 *  pixels. `GET /api/sprites` answers exactly this shape. */
export interface SpriteMeta {
  id: string;
  name: string;
  w: number;
  h: number;
  frames: number;
  fps: number;
  colors: number;
  bytes: number;
}

/** Every sprite, in store order. */
export const sprites: Writable<SpriteMeta[]> = writable([]);

/** The per-record byte ceiling the device enforces. The playground teaches
 *  the same one so a sprite drawn there always fits a device. */
export const spriteMaxBytes = writable(SPRITE_MAX_BYTES);

/** True while a save is in flight — the editor header's `saving…`. */
export const spriteSaving = writable(false);

/** Decoded records by id, so opening the editor on a sprite the tab already
 *  drew costs nothing. Invalidated on every save, delete and refresh. */
const cache = new Map<string, Sprite>();

// ---- reading ----

/** Load the library. Console: `/api/sprites`. Playground: localStorage. */
export async function refreshSprites(): Promise<void> {
  const d = get(device);
  if (!d) {
    loadLocal();
    void migrateTaggedSprites();
    return;
  }
  try {
    const r = await d.sprites();
    sprites.set((r.sprites ?? []).map(rowMeta));
    spriteMaxBytes.set(r.max_bytes ?? SPRITE_MAX_BYTES);
  } catch {
    // Firmware without /api/sprites — the tab shows an empty library rather
    // than an error, exactly as `refreshScenes` does for /api/scenes.
    sprites.set([]);
  }
  void migrateTaggedSprites();
}

function rowMeta(r: SpriteMeta): SpriteMeta {
  return {
    id: String(r.id ?? ""),
    name: String(r.name ?? ""),
    w: Number(r.w ?? 0),
    h: Number(r.h ?? 0),
    frames: Number(r.frames ?? 1),
    fps: Number(r.fps ?? 0),
    colors: Number(r.colors ?? 0),
    bytes: Number(r.bytes ?? 0),
  };
}

/** One row by id, from whatever the store last read. */
export function spriteMeta(id: string): SpriteMeta | null {
  return get(sprites).find((s) => s.id === id) ?? null;
}

/**
 * A sprite's PIXELS by id, decoded — the editor's document and the
 * compositor's input. Cached, because a tile grid and the scene stage both
 * ask for the same records repeatedly; the cache is dropped on any write.
 */
export async function loadSprite(id: string): Promise<Sprite | null> {
  if (id === "") return null;
  const held = cache.get(id);
  if (held) return held;
  const d = get(device);
  let sp: Sprite | null = null;
  if (!d) {
    const rec = localRecords().find((r) => r.id === id);
    sp = rec ? decodeSprite(rec.bytes) : null;
  } else {
    try {
      const bytes = await d.spriteRecord(id);
      sp = bytes ? decodeSprite(bytes) : null;
    } catch {
      sp = null;
    }
  }
  if (sp) cache.set(id, sp);
  return sp;
}

/** The decoded record if it is already in hand, without a fetch — what a
 *  render loop can afford to call. */
export function cachedSprite(id: string): Sprite | null {
  return cache.get(id) ?? null;
}

// ---- writing ----

/** 8 lowercase hex, the id shape both hosts assign (`randomSceneId`'s twin).
 *  A device assigns the real one on a console. */
export function randomSpriteId(): string {
  const b = new Uint8Array(4);
  crypto.getRandomValues(b);
  return Array.from(b, (v) => v.toString(16).padStart(2, "0")).join("");
}

export interface SpriteSave {
  ok: boolean;
  id?: string;
  error?: string;
}

/**
 * Create or replace a sprite. `id` empty/omitted = create (the device assigns
 * the id; the playground makes one), and — the pattern store's rule, which
 * the sprite routes keep — a bare save whose NAME matches an existing sprite
 * replaces that one instead of making a second.
 *
 * The palette is COMPACTED first: entries nothing paints with are dropped, so
 * the 255-colour cap is about colours in USE rather than about every colour
 * the session touched (#741 item 14).
 *
 * Every refusal goes through `reportApiError` with scope `sprite`, so an
 * over-cap record lands in the ONE error strip with a sentence rather than in
 * a page-local string nobody reads.
 */
export async function saveSprite(sprite: Sprite, id = ""): Promise<SpriteSave> {
  const tidy = compactPalette(sprite);
  const record = encodeSprite(tidy);
  const max = get(spriteMaxBytes);
  if (record.length > max) {
    const msg = `sprite: ${record.length} B is over the ${Math.round(max / 1024)} KiB cap`;
    reportApiError(msg, { scope: "sprite", subject: tidy.name });
    return { ok: false, error: msg };
  }
  const d = get(device);
  spriteSaving.set(true);
  try {
    if (!d) {
      const list = localRecords();
      const at =
        id !== ""
          ? list.findIndex((r) => r.id === id)
          : list.findIndex((r) => decodeSprite(r.bytes)?.name === tidy.name);
      const assigned = at >= 0 ? (list[at]?.id ?? randomSpriteId()) : id || randomSpriteId();
      const row = { id: assigned, bytes: record };
      if (at >= 0) list[at] = row;
      else list.push(row);
      saveLocal(list);
      cache.set(assigned, tidy);
      return { ok: true, id: assigned };
    }
    const r = await d.saveSprite(id, record);
    if (!r.ok) {
      reportApiError(r.error, { scope: "sprite", subject: tidy.name });
      return { ok: false, error: r.error };
    }
    cache.delete(id);
    if (r.id) cache.set(r.id, tidy);
    await refreshSprites();
    return { ok: true, id: r.id };
  } catch (e) {
    const msg = String(e);
    reportApiError(msg, { scope: "sprite", subject: tidy.name });
    return { ok: false, error: msg };
  } finally {
    spriteSaving.set(false);
  }
}

/** Delete a sprite. The scenes that used it keep the layer and say the sprite
 *  is missing — the same thing a deleted pattern does to a pattern layer. */
export async function deleteSprite(id: string): Promise<boolean> {
  cache.delete(id);
  const d = get(device);
  if (!d) {
    saveLocal(localRecords().filter((r) => r.id !== id));
    return true;
  }
  try {
    const r = await d.deleteSprite(id);
    if (!r.ok) {
      reportApiError(r.error ?? "rejected", { scope: "sprite" });
      return false;
    }
  } catch (e) {
    reportApiError(String(e), { scope: "sprite" });
    return false;
  }
  await refreshSprites();
  return true;
}

/** Duplicate a sprite under a new name; returns the new id. */
export async function duplicateSprite(id: string): Promise<string> {
  const src = await loadSprite(id);
  if (!src) return "";
  const copy: Sprite = {
    ...src,
    name: nextCopyName(
      src.name,
      get(sprites).map((s) => s.name),
    ),
    palette: src.palette.map((c) => [...c] as [number, number, number]),
    index: new Uint8Array(src.index),
  };
  const r = await saveSprite(copy);
  return r.ok ? (r.id ?? "") : "";
}

/** A blank sprite under the next free `Sprite N` name — `+ New sprite` and
 *  the scene inspector's `New…` both mean this. */
export function freshSprite(w = 8, h = 8): Sprite {
  const taken = new Set(get(sprites).map((s) => s.name));
  let name = "Sprite 1";
  for (let n = 1; n < 1000; n++) {
    name = `Sprite ${n}`;
    if (!taken.has(name)) break;
  }
  return newSprite(name, w, h);
}

/** The names of the scenes whose sprite layers reference `id` — the editor's
 *  "Used in" line, and the warning a delete deserves. */
export function usedBy(id: string): string[] {
  if (id === "") return [];
  return get(scenes)
    .filter((s) => s.layers.some((l) => l.body.kind === "sprite" && l.body.id === id))
    .map((s) => s.name);
}

/** Guard for an id that came out of a URL fragment. */
export function isSpriteId(s: string): boolean {
  return /^[0-9a-f]{8}$/.test(s);
}

// ---- polling ----
//
// The 6th cadence on the ONE scheduler (docs/web-architecture.md): the sprite
// library can change from another browser tab, so the page reads it while it
// is open — and only while.

export function startSpritePoll(): () => void {
  return pollSubscribe("sprites", 2000, refreshSprites);
}

// ---- the playground's backing ----

interface LocalRecord {
  id: string;
  bytes: Uint8Array;
}

function localRecords(): LocalRecord[] {
  let raw = "";
  try {
    raw = localStorage.getItem(LS_SPRITES) ?? "";
  } catch {
    raw = "";
  }
  if (raw === "") return [];
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];
  const out: LocalRecord[] = [];
  for (const row of parsed) {
    if (!row || typeof row !== "object") continue;
    const { id, b64 } = row as { id?: unknown; b64?: unknown };
    if (typeof id !== "string" || typeof b64 !== "string") continue;
    const bytes = fromBase64(b64);
    if (!bytes) continue;
    out.push({ id, bytes });
  }
  return out;
}

function saveLocal(list: readonly LocalRecord[]): void {
  try {
    localStorage.setItem(
      LS_SPRITES,
      JSON.stringify(list.map((r) => ({ id: r.id, b64: toBase64(r.bytes) }))),
    );
  } catch {
    /* private mode: the library is session-only */
  }
  setLocalList(list);
}

function loadLocal(): void {
  setLocalList(localRecords());
  spriteMaxBytes.set(SPRITE_MAX_BYTES);
}

/** Publish the metadata list (and re-fill the cache) from local records. */
function setLocalList(list: readonly LocalRecord[]): void {
  const metas: SpriteMeta[] = [];
  for (const r of list) {
    const sp = decodeSprite(r.bytes);
    if (!sp) continue;
    cache.set(r.id, sp);
    metas.push({
      id: r.id,
      name: sp.name,
      w: sp.w,
      h: sp.h,
      frames: sp.frames,
      fps: sp.fps,
      colors: sp.palette.length,
      bytes: spriteBytes(sp),
    });
  }
  sprites.set(metas);
}

/** Chunked on purpose: `String.fromCharCode(...bytes)` on a 16 KiB record is
 *  a 16 384-argument call, which some engines refuse outright. */
function toBase64(bytes: Uint8Array): string {
  let s = "";
  for (let i = 0; i < bytes.length; i += 4096) {
    s += String.fromCharCode(...bytes.subarray(i, i + 4096));
  }
  return btoa(s);
}

function fromBase64(b64: string): Uint8Array | null {
  try {
    const s = atob(b64);
    const out = new Uint8Array(s.length);
    for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
    return out;
  } catch {
    return null;
  }
}

// ---- the one-release migration (#740 step 3) ----

/** Once per session, per backing. Not persisted: a run that found nothing
 *  costs one list read, and a run that failed should be retried on the next
 *  reload rather than remembered as done. */
let migrating = false;
let migrated = false;

/**
 * Convert every `// @sprite` PATTERN into a sprite record, re-point the
 * scenes that named it, and delete the pattern.
 *
 * Order matters: the sprite is saved FIRST, the scenes are re-saved SECOND
 * and the pattern is deleted LAST, so an interruption anywhere leaves a
 * readable state (at worst a duplicate sprite and a pattern still in the
 * library, which the next run converts again under a copy name rather than
 * losing a drawing). Anything that fails to convert is left alone and
 * reported — a sprite nobody can read is not something to delete.
 */
export async function migrateTaggedSprites(): Promise<void> {
  if (migrated || migrating) return;
  migrating = true;
  try {
    const failed: string[] = [];
    const converted = (await (get(device) ? migrateDevice(failed) : migrateLocal(failed))) ?? 0;
    migrated = true;
    if (converted > 0) {
      note("sprite", `Moved ${converted} sprite${converted === 1 ? "" : "s"} out of Patterns.`, 6000);
      await refreshSprites();
      await refreshScenes();
    }
    if (failed.length > 0) {
      note("sprite", `Couldn't convert ${failed.join(", ")} — left in Patterns.`, 8000);
    }
  } finally {
    migrating = false;
  }
}

async function migrateDevice(failed: string[]): Promise<number> {
  const d = get(device);
  if (!d) return 0;
  let rows: { id: string; name: string }[];
  try {
    rows = await d.patterns();
  } catch {
    return 0;
  }
  // Reuse what the library ALREADY holds (the 2026-09-26 panel). This
  // migration used to re-download every pattern's source, one request each,
  // on top of the sweep `refreshDevicePatterns` runs for the same rows — so
  // opening the scene editor fired two full library downloads at a board with
  // two sockets, which is how the reads start refusing each other in the first
  // place. A row whose source is cached here is by definition NOT a tagged
  // sprite (the sweep drops those from the library), so skipping it is exactly
  // right.
  const held = new Map(get(devicePatterns).map((p) => [p.id, p.source]));
  let n = 0;
  for (const row of rows) {
    let source = held.get(row.id);
    if (source === undefined) {
      try {
        source = (await d.patternSource(row.id)).source;
      } catch {
        continue;
      }
    }
    if (!isTaggedSpriteSource(source)) continue;
    const made = await convert(source, row.name, row.id, failed);
    if (made) {
      try {
        await d.deletePattern(row.id);
      } catch {
        /* the sprite exists either way; the stale pattern is cosmetic */
      }
      n++;
    }
  }
  if (n > 0) await refreshDevicePatterns();
  return n;
}

async function migrateLocal(failed: string[]): Promise<number> {
  let n = 0;
  for (const p of listAllPatterns()) {
    if (!isTaggedSpriteSource(p.source)) continue;
    const made = await convert(p.source, p.name, playgroundPatternId(p.name), failed);
    if (made) {
      deletePattern(p.name);
      n++;
    }
  }
  return n;
}

/** Decode one tagged pattern, store it as a sprite and re-point the scenes
 *  that named the pattern. True when the pattern may now be deleted. */
async function convert(
  source: string,
  patternName: string,
  patternId: string,
  failed: string[],
): Promise<boolean> {
  const sp = spriteFromTaggedPattern(source);
  if (!sp) {
    failed.push(patternName);
    return false;
  }
  // The tag line carried no name of its own on a pattern saved by an early
  // build, and the PATTERN's name is the one the user actually chose.
  const named: Sprite = { ...sp, name: sp.name === "Sprite" ? patternName || "Sprite" : sp.name };
  const r = await saveSprite(named);
  if (!r.ok || !r.id) {
    failed.push(patternName);
    return false;
  }
  await repointScenes(patternId, r.id);
  return true;
}

/** Rewrite every sprite layer that named `from` to name `to`, and re-save the
 *  scenes that changed. */
async function repointScenes(from: string, to: string): Promise<void> {
  if (from === "" || from === to) return;
  for (const s of get(scenes)) {
    if (!s.layers.some((l) => l.body.kind === "sprite" && l.body.id === from)) continue;
    const next: Scene = {
      ...s,
      layers: s.layers.map((l) =>
        l.body.kind === "sprite" && l.body.id === from ? { ...l, body: { kind: "sprite", id: to } } : l,
      ),
    };
    await saveScene(next);
  }
}
