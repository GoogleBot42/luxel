// Scene reference reconciliation (Gitea #785) — the one algorithm for "this
// scene names records that are not in this store yet; put them there and
// rewrite the ids".
//
// A scene is a stack of layers that name their pattern and their sprite by
// STORE id (docs/spec/scenes.md §1, the `I` line), which is the right design
// for a device and the wrong one for anything portable: 8 hex digits minted by
// somebody else's flash mean nothing here. So a scene that arrives from
// OUTSIDE the target store — the shipped library today (#785), an exported
// scene file tomorrow (#746) — carries its dependencies by a stable key
// instead, and something has to walk them, create what is missing, and swap
// the ids. That walk is here, ONCE:
//
//   * pure — no stores, no fetch, no device; the two sides are injected as
//     `RefSource` (where the records come from) and `RefTarget` (where they
//     go), which is what lets the console write to `/api/patterns` and
//     `/api/sprites` while the playground writes to `localStorage` with no
//     second copy of the logic;
//   * MATCHING IS BY CONTENT, not by name. A name is what a user renames; the
//     bytes are what make two records the same record. So a library pattern
//     whose source is already in the store binds that store id however it is
//     named, and only a genuinely new record is created. The library slug is
//     the stable key on the LIBRARY side (`refs[].slug`, the file stem — see
//     `librarySource.ts`); content identity is the stable key on the STORE
//     side, where there is no field to put a slug in.
//   * a name collision on create RENAMES rather than overwrites. Both hosts
//     treat a save under an existing name as "replace that one" (the pattern
//     store's rule, kept by the sprite routes), so creating "Heart" when the
//     user already has a different "Heart" would silently eat their drawing.
//
// #746 (scene export / import) should import this rather than grow its own:
// an exported scene's dependency list is the same `SceneRef[]`, and its
// importer is the same `RefTarget`.

import type { Scene } from "./scene.ts";
import { encodeSprite, type Sprite } from "./sprite.ts";
import type { LibRefKind } from "./librarySource.ts";

/** One dependency of a portable scene. `id` is the placeholder the scene's
 *  layers currently bind; `slug` is the stable library key; `name` is what the
 *  created record should be called. */
export interface SceneRef {
  id: string;
  kind: LibRefKind;
  slug: string;
  name: string;
}

/** Where the records come from — the shipped library, or an export file. */
export interface RefSource {
  /** The pattern source behind a `pat` ref, or null when it cannot be found. */
  patternSource(ref: SceneRef): string | null;
  /** The decoded record behind a `spr` ref, or null. */
  sprite(ref: SceneRef): Sprite | null;
}

export type PutResult = { id: string } | { error: string };

/** Where they go — a device, or this browser. */
export interface RefTarget {
  /** What the store already holds. `source` may be absent while a console's
   *  library is still streaming in; an absent source is UNKNOWN, never a
   *  match (.claude/rules/web.md — unknown is not a "no", but it is not a
   *  "yes" either, and guessing here would bind the wrong pattern). */
  patterns(): Promise<readonly { id: string; name: string; source?: string }[]>;
  putPattern(name: string, source: string): Promise<PutResult>;
  /** What the store already holds, with the decoded record where the host has
   *  one in hand. */
  sprites(): Promise<readonly { id: string; name: string; sprite: Sprite | null }[]>;
  putSprite(sprite: Sprite): Promise<PutResult>;
}

/** What one reconciliation did — the confirmation line a clone shows. */
export interface RefTally {
  patternsCreated: number;
  spritesCreated: number;
  reused: number;
}

export type ResolveResult =
  | { ok: true; scene: Scene; tally: RefTally }
  | { ok: false; error: string };

/**
 * `want`, or the first free `want 2`, `want 3`, … — the same shape
 * `stores/scenes.ts` `nextCopyName` produces, spelled here because this module
 * must not import a store.
 */
export function freeName(want: string, taken: readonly string[]): string {
  if (!taken.includes(want)) return want;
  const base = want.replace(/ \d+$/, "");
  for (let n = 2; n < 999; n++) {
    const candidate = `${base} ${n}`;
    if (!taken.includes(candidate)) return candidate;
  }
  return `${base} copy`;
}

/** Every layer's `I` id rewritten through `map` — a layer whose id is not in
 *  it keeps what it had (an unbound layer's `""` included). */
export function rewriteSceneIds(scene: Scene, map: ReadonlyMap<string, string>): Scene {
  return {
    ...scene,
    layers: scene.layers.map((l) => {
      if (l.body.kind === "pat") {
        const to = map.get(l.body.pat.id);
        return to === undefined ? l : { ...l, body: { kind: "pat" as const, pat: { ...l.body.pat, id: to } } };
      }
      if (l.body.kind === "sprite") {
        const to = map.get(l.body.id);
        return to === undefined ? l : { ...l, body: { ...l.body, id: to } };
      }
      return l;
    }),
  };
}

/**
 * Make `scene`'s dependencies exist in `target`, then return the scene with its
 * ids rewritten. NOT saved — the caller owns that, because the scene store is
 * where a refusal has to be reported from (`stores/scenes.ts` reports one
 * through the ONE error strip with the device's own words).
 *
 * The first failure stops the walk and is returned verbatim: a device's
 * `sprite: 17408 B is over the 16 KiB cap` or `scenes: store full (…)` is the
 * sentence the user needs, and paraphrasing it would lose the numbers.
 * Anything already created stays created — a half-resolved clone leaves the
 * store with a usable record rather than rolling back a save that succeeded.
 */
export async function resolveSceneRefs(
  scene: Scene,
  refs: readonly SceneRef[],
  lib: RefSource,
  target: RefTarget,
): Promise<ResolveResult> {
  const map = new Map<string, string>();
  const tally: RefTally = { patternsCreated: 0, spritesCreated: 0, reused: 0 };

  const patRefs = refs.filter((r) => r.kind === "pat");
  const sprRefs = refs.filter((r) => r.kind === "spr");

  if (patRefs.length > 0) {
    const have = await target.patterns();
    const names = have.map((p) => p.name);
    for (const ref of patRefs) {
      const source = lib.patternSource(ref);
      if (source === null) {
        return { ok: false, error: `the library has no pattern “${ref.name}” (${ref.slug})` };
      }
      const hit = have.find((p) => p.source === source);
      if (hit) {
        map.set(ref.id, hit.id);
        tally.reused++;
        continue;
      }
      const name = freeName(ref.name, names);
      const r = await target.putPattern(name, source);
      if ("error" in r) return { ok: false, error: r.error };
      names.push(name);
      map.set(ref.id, r.id);
      tally.patternsCreated++;
    }
  }

  if (sprRefs.length > 0) {
    const have = await target.sprites();
    const names = have.map((s) => s.name);
    // Compared as BYTES: two records are the same sprite when they encode the
    // same, which is a cheap total order on an object that has no id yet.
    const held = have.map((s) => ({
      id: s.id,
      bytes: s.sprite ? bytesKey(encodeSprite(s.sprite)) : null,
    }));
    for (const ref of sprRefs) {
      const sprite = lib.sprite(ref);
      if (sprite === null) {
        return { ok: false, error: `the library has no sprite “${ref.name}” (${ref.slug})` };
      }
      const key = bytesKey(encodeSprite(sprite));
      const hit = held.find((s) => s.bytes === key);
      if (hit) {
        map.set(ref.id, hit.id);
        tally.reused++;
        continue;
      }
      const name = freeName(sprite.name, names);
      const r = await target.putSprite(name === sprite.name ? sprite : { ...sprite, name });
      if ("error" in r) return { ok: false, error: r.error };
      names.push(name);
      map.set(ref.id, r.id);
      tally.spritesCreated++;
    }
  }

  return { ok: true, scene: rewriteSceneIds(scene, map), tally };
}

/** A record's bytes as a comparable string — `Uint8Array` has no value
 *  equality and the records are kilobytes, so this is cheaper than a loop per
 *  candidate pair. */
function bytesKey(bytes: Uint8Array): string {
  let s = "";
  for (let i = 0; i < bytes.length; i += 4096) {
    s += String.fromCharCode(...bytes.subarray(i, i + 4096));
  }
  return s;
}

/** The one-line confirmation a clone reports. */
export function tallyLine(tally: RefTally): string {
  const made: string[] = [];
  if (tally.patternsCreated > 0) {
    made.push(`${tally.patternsCreated} pattern${tally.patternsCreated === 1 ? "" : "s"}`);
  }
  if (tally.spritesCreated > 0) {
    made.push(`${tally.spritesCreated} sprite${tally.spritesCreated === 1 ? "" : "s"}`);
  }
  if (made.length === 0 && tally.reused === 0) return "cloned";
  if (made.length === 0) return `cloned — reused ${tally.reused}`;
  return `cloned — added ${made.join(" and ")}${tally.reused > 0 ? `, reused ${tally.reused}` : ""}`;
}
