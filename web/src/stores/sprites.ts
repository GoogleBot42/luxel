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

/**
 * Bumped whenever the BYTES behind some sprite id change — a record landing,
 * a save, a delete, or a refresh that finds the host holding a different
 * record under an id the browser already had.
 *
 * It is the re-bind signal for every surface that COMPOSITES a sprite layer:
 * a record arriving is not a change to any scene's wire, so nothing in
 * `lib/sceneRender.ts` would rebuild on its own (`SceneRenderer.setScene`
 * no-ops an unchanged wire). The scene editor's stage, the Scenes grid and
 * `lib/sceneThumb.ts` all watch this.
 *
 * It used to be a `let spriteRev = 0` in each of THREE pages, each with its
 * own `encoded` Map beside it — which is how the 2026-09-26 panel ended up
 * never drawing a sprite layer on a console. Two holes, both per-page: a
 * record that failed to READ was never asked for again (the counter only
 * moved when something landed, and nothing retried), and a record that was
 * re-drawn in the sprite editor was never re-encoded (the page's
 * `encoded.has(id)` said it already had it). One owner, one signal.
 */
export const spriteRev: Writable<number> = writable(0);

/** Decoded records by id, so opening the editor on a sprite the tab already
 *  drew costs nothing. Invalidated on every save, delete and refresh. */
const cache = new Map<string, Sprite>();

/** `encodeSprite` memoised per id: `SceneRenderer.setScene` copies these
 *  bytes into wasm on every rebuild and a 64×64 record is 4 KB. Dropped
 *  exactly when `cache` is. */
const encoded = new Map<string, Uint8Array>();

/** What the record behind an id LOOKED like when it was cached — the library
 *  row's own numbers, so a refresh can tell "still the same drawing" from
 *  "somebody re-drew it" without downloading anything. */
const sig = new Map<string, string>();

/** Loads in flight, so five surfaces asking for the same record at once is
 *  ONE request on a two-socket board. */
const inflight = new Map<string, Promise<Sprite | null>>();

/** Records that would not READ, and how many times. Not a negative cache: it
 *  is the retry BUDGET. A host that answers "no such sprite" is exhausted at
 *  once; a bad read — a refused connection, or the truncated body the firmware
 *  documents as the expected failure of an unpinned record streamed while a
 *  save lands (`firmware/src/server.rs` `SpriteRecord`: "a retryable bad
 *  download is the better failure") — is worth asking again. Nothing used to
 *  ask again. */
const misses = new Map<string, number>();

/** Attempts inside ONE `loadSprite`, and how long between them. */
const LOAD_TRIES = 3;
const LOAD_BACKOFF_MS = 250;
/** How many `ensureSprites`/`warmSprites` passes an id that never reads gets
 *  before it is left alone until something invalidates it (a save, a delete,
 *  or a library row whose numbers changed). */
const MISS_BUDGET = 4;

/** The six numbers `GET /api/sprites` prints for a row — and the same six a
 *  decoded record yields, because the host computes them FROM the record
 *  (`firmware/src/sprites.rs` `list_json`). Equal signatures mean the bytes in
 *  hand are still the bytes the host holds. */
function rowSig(m: SpriteMeta): string {
  return `${m.w}x${m.h}x${m.frames}x${m.fps}x${m.colors}x${m.bytes}`;
}

function recordSig(sp: Sprite): string {
  return `${sp.w}x${sp.h}x${sp.frames}x${sp.fps}x${sp.palette.length}x${spriteBytes(sp)}`;
}

/** Take `sp` as the record behind `id`. Bumps nothing: the caller decides
 *  whether this is news, because a refresh adopting twenty unchanged rows must
 *  not repaint every scene tile twenty times. */
function hold(id: string, sp: Sprite): void {
  cache.set(id, sp);
  encoded.delete(id);
  sig.set(id, recordSig(sp));
  misses.delete(id);
}

/** Forget everything about `id` — the record, its bytes, its retry budget. */
function drop(id: string): void {
  cache.delete(id);
  encoded.delete(id);
  sig.delete(id);
  misses.delete(id);
}

function bumpRev(): void {
  spriteRev.update((n) => n + 1);
}

/**
 * The `SpriteLookup` the wasm compositor takes (`lib/sceneRender.ts`): the
 * `LXSP` bytes behind an id, or null when the browser does not hold that
 * record yet.
 *
 * Synchronous on purpose — a render loop calls it — so it is a CACHE reader
 * and never a fetch. `warmSprites`/`ensureSprites` fill the cache, and
 * `spriteRev` is what tells a surface to ask again.
 */
export function spriteBytesOf(id: string): Uint8Array | null {
  if (id === "") return null;
  const held = encoded.get(id);
  if (held) return held;
  const sp = cache.get(id);
  if (!sp) return null;
  const bytes = encodeSprite(sp);
  encoded.set(id, bytes);
  return bytes;
}

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
    const list = (r.sprites ?? []).map(rowMeta);
    sprites.set(list);
    spriteMaxBytes.set(r.max_bytes ?? SPRITE_MAX_BYTES);
    reconcile(list);
  } catch {
    // Firmware without /api/sprites — the tab shows an empty library rather
    // than an error, exactly as `refreshScenes` does for /api/scenes.
    sprites.set([]);
  }
  void migrateTaggedSprites();
}

/**
 * Line the cached records up against the library that just landed: a row
 * whose numbers no longer match what we hold was re-drawn somewhere else (the
 * sprite editor in another tab, or on another console), and an id the host no
 * longer lists is gone. Both drop, and ONE `spriteRev` bump re-binds every
 * scene surface.
 */
function reconcile(list: readonly SpriteMeta[]): void {
  let changed = 0;
  const live = new Set<string>();
  for (const row of list) {
    live.add(row.id);
    const was = sig.get(row.id);
    if (was !== undefined && was !== rowSig(row)) {
      drop(row.id);
      changed++;
    }
  }
  for (const id of [...cache.keys()]) {
    if (live.has(id)) continue;
    drop(id);
    changed++;
  }
  if (changed > 0) bumpRev();
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
  // Deduped: the scene editor, the Scenes grid and a thumbnail all ask for the
  // same record within a frame of each other, and a two-socket board answers
  // one of those three at a time.
  const busy = inflight.get(id);
  if (busy) return busy;
  const job = fetchRecord(id).finally(() => inflight.delete(id));
  inflight.set(id, job);
  return job;
}

/**
 * One record, with a retry ladder — and the ladder is the point.
 *
 * `lib/fetchgate.ts` retries a refused CONNECTION, so what reaches here is a
 * body: a truncated record (the firmware streams an unpinned record and says
 * so), or `{"ok":false,…}` under a 200 for an id the host does not have
 * (`lib/device.ts` `spriteRecord` turns that one into `null`). Neither was
 * retried before, and neither left a trace — the page's `spriteRev` only moved
 * when a record LANDED, so one bad read meant a scene layer that drew nothing
 * for as long as the tab stayed open.
 */
async function fetchRecord(id: string): Promise<Sprite | null> {
  for (let attempt = 0; attempt < LOAD_TRIES; attempt++) {
    if (attempt > 0) await new Promise((r) => setTimeout(r, LOAD_BACKOFF_MS * attempt));
    const d = get(device);
    let bytes: Uint8Array | null = null;
    if (!d) {
      bytes = localRecords().find((r) => r.id === id)?.bytes ?? null;
      // The playground's backing cannot fail transiently: what localStorage
      // holds is the whole truth, so one look is the answer.
      const sp = bytes ? decodeSprite(bytes) : null;
      if (sp) {
        hold(id, sp);
        bumpRev();
        return sp;
      }
      misses.set(id, MISS_BUDGET);
      return null;
    }
    try {
      bytes = await d.spriteRecord(id);
    } catch {
      continue; // a bad read: ask again
    }
    if (bytes === null) {
      // The host says it has no such sprite. That is an answer, not a failure.
      misses.set(id, MISS_BUDGET);
      return null;
    }
    const sp = decodeSprite(bytes);
    if (sp) {
      hold(id, sp);
      bumpRev();
      return sp;
    }
    // Bytes that are not a record: a truncated stream. Worth one more ask.
  }
  misses.set(id, (misses.get(id) ?? 0) + 1);
  return null;
}

/**
 * Make sure the browser holds the RECORD behind every `id` — the bytes the
 * compositor blits. Safe to call as often as you like: an id already in hand
 * costs nothing, an id being loaded joins that load, and an id that has failed
 * `MISS_BUDGET` passes is left alone until something invalidates it.
 *
 * ONE AT A TIME on a console. A parallel burst across a two-socket board
 * starves the status poll, and the gate would queue them anyway.
 */
export async function ensureSprites(ids: Iterable<string>): Promise<void> {
  const want = new Set<string>();
  for (const id of ids) {
    if (id === "" || cache.has(id)) continue;
    if ((misses.get(id) ?? 0) >= MISS_BUDGET) continue;
    want.add(id);
  }
  for (const id of want) await loadSprite(id);
}

/** Re-read the library, then pull every row's record — what a surface that
 *  composites sprites calls when it comes forward. */
export async function warmSprites(): Promise<void> {
  await refreshSprites();
  await ensureSprites(get(sprites).map((s) => s.id));
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
      hold(assigned, tidy);
      bumpRev();
      return { ok: true, id: assigned };
    }
    const r = await d.saveSprite(id, record);
    if (!r.ok) {
      reportApiError(r.error, { scope: "sprite", subject: tidy.name });
      return { ok: false, error: r.error };
    }
    // The drawing that was just saved IS the record now, so put it in hand
    // rather than waiting for a download to bring it back — and BUMP, because
    // every scene that draws this sprite is showing the old pixels until
    // something re-binds (the half of the 2026-09-26 panel bug that survived a
    // healthy device: a per-page `encoded` map that was never invalidated).
    drop(id);
    if (r.id) hold(r.id, tidy);
    bumpRev();
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
  drop(id);
  bumpRev();
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

/**
 * A sprite waiting for the editor to adopt it as an UNSAVED document (Gitea
 * #784: an imported image "lands in the editor, not straight into the store,
 * so the result can be touched up before Save").
 *
 * It is a store rather than a route segment or a prop because the hand-off
 * crosses two screens — `Import image…` can be pressed on the Sprites TAB,
 * which then routes to the editor — and a record is far too big to put in a
 * URL. One slot: a second import replaces the first, which is what pressing
 * the button twice means.
 */
export const pendingSprite: Writable<Sprite | null> = writable(null);

/** Hand a sprite to the editor. The caller then routes there (`#/sprites/`,
 *  i.e. `openSprite("")`), and the editor takes it with
 *  [`takePendingSprite`]. */
export function stageSprite(s: Sprite): void {
  pendingSprite.set(s);
}

/** The staged sprite, ONCE — reading it clears the slot, so a later
 *  re-render of the editor does not re-adopt it over your edits. */
export function takePendingSprite(): Sprite | null {
  const s = get(pendingSprite);
  if (s !== null) pendingSprite.set(null);
  return s;
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
  // `warmSprites`, not `refreshSprites`: the pass that pulls the RECORDS is
  // also the retry for one that would not read, so the surface with the poll
  // on it heals the cache every two seconds (Gitea #740 follow-up).
  return pollSubscribe("sprites", 2000, warmSprites);
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
  let changed = 0;
  for (const r of list) {
    const sp = decodeSprite(r.bytes);
    if (!sp) continue;
    if (sig.get(r.id) !== recordSig(sp)) changed++;
    hold(r.id, sp);
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
  const live = new Set(metas.map((m) => m.id));
  for (const id of [...cache.keys()]) {
    if (live.has(id)) continue;
    drop(id);
    changed++;
  }
  sprites.set(metas);
  if (changed > 0) bumpRev();
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
