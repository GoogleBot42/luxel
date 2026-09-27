// The SHIPPED sprite and scene libraries (Gitea #785) — the third backing, and
// the only read-only one.
//
// `stores/sprites.ts` and `stores/scenes.ts` each have two backings, both of
// them LIVE: the device and this browser's `localStorage`. This store is the
// `Library` source beside them — `public/sprites.json` and
// `public/scenes.json`, built from `library/sprites/` and `library/scenes/` by
// `web/tools/gen-sprite-scene-gallery.mjs`, shipped in the bundle exactly as
// `gallery.json` ships the pattern library. Nothing here is writable; the one
// verb is CLONE, which copies an entry into whichever live backing is in front
// of the user.
//
// Cloning a SCENE is the interesting half, and it is not this file's algorithm:
// a scene names its patterns and sprites by store id, so a shipped one carries
// library references instead and `lib/sceneRefs.ts` reconciles them (create
// what is missing, match what is already there BY CONTENT, rewrite the ids).
// What lives here is the two `RefTarget`s that algorithm writes through — the
// device's routes, and the playground's `localStorage` — and the `RefSource`
// that reads out of the shipped files.
//
// A `pat` reference carries its pattern's NAME, not its source: the sources are
// already in the bundle once, as `gallery.json`'s 354 KB, and shipping three of
// them a second time inside `scenes.json` would buy nothing but bytes against
// the assets partition (docs/boards.md). So `gallery.json` is fetched lazily,
// the first time something actually needs a pattern — the same rule
// `components/PatternPicker.svelte` uses for the same file.

import { get, writable, type Writable } from "svelte/store";
import { gatedFetch } from "../lib/fetchgate";
import { libSceneId } from "../lib/librarySource";
import { parseScene, type Scene } from "../lib/scene";
import { playgroundPatternId } from "../lib/sceneRender";
import { decodeSprite, encodeSprite, type Sprite } from "../lib/sprite";
import {
  freeName,
  resolveSceneRefs,
  tallyLine,
  type RefSource,
  type RefTarget,
  type SceneRef,
} from "../lib/sceneRefs";
import { listPatterns, savePattern as savePatternLocally } from "../lib/store";
import { device, devicePatterns, refreshDevicePatterns } from "./device";
import { note, reportApiError } from "./notify";
import { compileToBytecode, saved } from "./pattern";
import { refreshScenes, saveScene, scenes } from "./scenes";
import { cachedSprite, saveSprite, sprites, warmSprites } from "./sprites";

/** One row of the shipped sprite library — a `SpriteMeta`'s peer, keyed by the
 *  library SLUG instead of a store id, plus the decoded record so a tile can
 *  draw it without going anywhere. */
export interface LibrarySprite {
  slug: string;
  name: string;
  w: number;
  h: number;
  frames: number;
  fps: number;
  colors: number;
  bytes: number;
  sprite: Sprite;
}

/** One row of the shipped scene library — the parsed record plus its
 *  references, which are what a clone has to reconcile and what a THUMBNAIL
 *  has to resolve to draw the pattern layers at all. */
export interface LibraryScene {
  slug: string;
  name: string;
  kind: string;
  layers: number;
  refs: SceneRef[];
  scene: Scene;
}

export const librarySprites: Writable<LibrarySprite[]> = writable([]);
export const libraryScenes: Writable<LibraryScene[]> = writable([]);
/** True until the first load settles — the pages' `loading…` line. */
export const librarySpritesLoading = writable(true);
export const libraryScenesLoading = writable(true);
/**
 * Bumped whenever a lookup below would answer differently — the shipped files
 * landing, or `gallery.json` landing behind them. It is `stores/sprites.ts`'s
 * `spriteRev` for this backing: `libraryLookup` and `librarySpriteBytesOf` are
 * plain functions over Maps, so nothing about them invalidates a `$:`
 * (.claude/rules/web.md) and a tile grid needs a named signal to re-bind on.
 */
export const libraryRev: Writable<number> = writable(0);

// ---- reading the shipped files ----
//
// Once per session, and lazily: a playground that never opens the Sprites tab
// never pays for either file. Both are small (a few KB), so there is no
// streaming here the way `gallery.json`'s 354 KB gets one.

let spritesJob: Promise<void> | null = null;
let scenesJob: Promise<void> | null = null;
let patternsJob: Promise<void> | null = null;

/** Synthetic ref id → what it refers to, over every shipped scene. Built when
 *  `scenes.json` lands; the ids are derived, so two scenes naming the same
 *  library entry agree on one (`lib/librarySource.ts` `libRefId`). */
const refIndex = new Map<string, SceneRef>();
/** Display name → source, from `gallery.json`. */
const patternSources = new Map<string, string>();
/** `encodeSprite` memoised per slug — the compositor copies these bytes in on
 *  every rebuild and a tile grid rebuilds a lot. */
const spriteRecords = new Map<string, Uint8Array>();

const assetUrl = (name: string): string => `${import.meta.env.BASE_URL}${name}`;

const bump = (): void => libraryRev.update((n) => n + 1);

export function loadLibrarySprites(): Promise<void> {
  spritesJob ??= (async () => {
    try {
      const r = await gatedFetch(assetUrl("sprites.json"));
      if (r.ok) {
        const rows = (await r.json()) as (Omit<LibrarySprite, "sprite"> & { b64: string })[];
        const out: LibrarySprite[] = [];
        for (const row of rows) {
          const sp = decodeSprite(fromBase64(row.b64));
          if (sp) out.push({ ...row, sprite: sp });
        }
        librarySprites.set(out);
      }
    } catch {
      /* a build without the file (or an older device's bundle) — the source is
         simply empty, the way the PixelBlaze tab is when corpus/ is absent */
    }
    librarySpritesLoading.set(false);
    bump();
  })();
  return spritesJob;
}

export function loadLibraryScenes(): Promise<void> {
  scenesJob ??= (async () => {
    // A scene draws sprites, so its library needs the sprite library too.
    await loadLibrarySprites();
    try {
      const r = await gatedFetch(assetUrl("scenes.json"));
      if (r.ok) {
        const rows = (await r.json()) as (Omit<LibraryScene, "scene"> & { wire: string })[];
        const out: LibraryScene[] = [];
        for (const row of rows) {
          const p = parseScene(row.wire);
          // The generator already ran this exact parser (and CI runs
          // `luxel_core`'s over the same wire), so a failure here means a
          // truncated download, not a bad library entry: drop the row rather
          // than the whole source.
          if (!p.ok) continue;
          // The shipped record's `S -` leaves the id empty, and every scene
          // surface keys on it — five id-less scenes are ONE tile in a keyed
          // `{#each}`. A derived id stands in until a clone drops it again.
          out.push({ ...row, scene: { ...p.scene, id: libSceneId(row.slug) } });
          for (const ref of row.refs) refIndex.set(ref.id, ref);
        }
        libraryScenes.set(out);
      }
    } catch {
      /* as above */
    }
    libraryScenesLoading.set(false);
    bump();
  })();
  return scenesJob;
}

/**
 * The clean-room pattern library, by display NAME — the only key
 * `gallery.json` has, and what a scene's `pat` reference carries. Fetched at
 * most once, and only when something needs a pattern: the Library scene grid
 * coming forward (its tiles composite the pattern layers) or a clone.
 */
export function loadLibraryPatterns(): Promise<void> {
  patternsJob ??= (async () => {
    try {
      const r = await gatedFetch(assetUrl("gallery.json"));
      if (r.ok) {
        for (const p of (await r.json()) as { name?: unknown; source?: unknown }[]) {
          if (typeof p.name === "string" && typeof p.source === "string" && !patternSources.has(p.name)) {
            patternSources.set(p.name, p.source);
          }
        }
      }
    } catch {
      /* a clone then reports "the library has no pattern …" through sceneRefs,
         and a tile simply draws its other layers */
    }
    bump();
  })();
  return patternsJob;
}

/** Everything a shipped scene's tile needs, in one call — the scene library,
 *  the sprite library behind it and the pattern sources its layers name. */
export async function warmSceneLibrary(): Promise<void> {
  await loadLibraryScenes();
  if (get(libraryScenes).some((s) => s.refs.some((r) => r.kind === "pat"))) {
    await loadLibraryPatterns();
  }
}

/**
 * `SourceLookup` for a LIBRARY scene (`lib/sceneRender.ts`): a synthetic `pat`
 * ref id → that pattern's source. `undefined` is "not loaded yet" (the tile
 * spins), `null` is "the library does not have it" — the same three-state
 * contract `pages/SceneEditor.svelte`'s `lookup` keeps.
 */
export function libraryLookup(id: string): string | null | undefined {
  const ref = refIndex.get(id);
  if (!ref || ref.kind !== "pat") return null;
  if (patternSources.size === 0) return undefined;
  return patternSources.get(ref.name) ?? null;
}

/** `SpriteLookup` for a LIBRARY scene: a synthetic `spr` ref id → the `LXSP`
 *  record the compositor blits. */
export function librarySpriteBytesOf(id: string): Uint8Array | null {
  const ref = refIndex.get(id);
  if (!ref || ref.kind !== "spr") return null;
  const held = spriteRecords.get(ref.slug);
  if (held) return held;
  const row = get(librarySprites).find((s) => s.slug === ref.slug);
  if (!row) return null;
  const bytes = encodeSprite(row.sprite);
  spriteRecords.set(ref.slug, bytes);
  return bytes;
}

// ---- the clone verbs ----

/**
 * Copy a shipped sprite into the live library. Returns the new store id, or
 * `""` when the host refused — the refusal itself is already in the ONE error
 * strip (`stores/sprites.ts` reports every `sprite: …` through
 * `reportApiError`), which is how the device's own words reach the screen.
 */
export async function cloneLibrarySprite(slug: string): Promise<string> {
  const row = get(librarySprites).find((s) => s.slug === slug);
  if (!row) return "";
  const name = freeName(
    row.sprite.name,
    get(sprites).map((s) => s.name),
  );
  const r = await saveSprite({
    ...row.sprite,
    name,
    palette: row.sprite.palette.map((c) => [...c] as [number, number, number]),
    index: new Uint8Array(row.sprite.index),
  });
  if (!r.ok) return "";
  note("sprite", name === row.sprite.name ? "added to your sprites" : `added as “${name}”`, 2500);
  return r.id ?? "";
}

/**
 * Copy a shipped scene into the live library, WITH its dependencies: the
 * patterns and sprites its layers name are created in the target store if they
 * are not already there (matched by content, `lib/sceneRefs.ts`) and the ids
 * are rewritten before the scene is saved.
 *
 * Returns the new scene id, or `""`. Every refusal — the sprite cap, the 3840 B
 * scene blob, a device that went away — is reported with the HOST's own text.
 */
export async function cloneLibraryScene(slug: string): Promise<string> {
  const row = get(libraryScenes).find((s) => s.slug === slug);
  if (!row) return "";
  if (row.refs.some((r) => r.kind === "pat")) await loadLibraryPatterns();
  const source: RefSource = {
    patternSource: (ref) => patternSources.get(ref.name) ?? null,
    sprite: (ref) => get(librarySprites).find((s) => s.slug === ref.slug)?.sprite ?? null,
  };
  const resolved = await resolveSceneRefs(row.scene, row.refs, source, refTarget());
  if (!resolved.ok) {
    // `putPattern`/`putSprite` already spoke through `reportApiError` for a
    // HOST refusal; what reaches here unreported is a library-side miss (a
    // renamed pattern), which is this store's to say.
    if (!resolved.error.startsWith("sprite:") && !resolved.error.startsWith("scenes:")) {
      reportApiError(resolved.error, { scope: "scene", subject: row.name });
    }
    return "";
  }
  const name = freeName(
    row.name,
    get(scenes).map((s) => s.name),
  );
  const r = await saveScene({ ...resolved.scene, id: "", name });
  if (!r.ok) return "";
  note("scene", tallyLine(resolved.tally), 3500);
  await refreshScenes();
  return r.id ?? "";
}

/** The live backing, as the reconciliation's write side. Two branches, one
 *  interface — which is the whole reason `lib/sceneRefs.ts` takes one. */
function refTarget(): RefTarget {
  const d = get(device);
  const spriteSide = {
    sprites: async () => {
      await warmSprites();
      return get(sprites).map((s) => ({ id: s.id, name: s.name, sprite: cachedSprite(s.id) }));
    },
    putSprite: async (sprite: Sprite) => {
      const r = await saveSprite(sprite);
      return r.ok && r.id ? { id: r.id } : { error: r.error ?? "the sprite was refused" };
    },
  };
  if (!d) {
    return {
      ...spriteSide,
      patterns: async () =>
        listPatterns().map((p) => ({
          id: playgroundPatternId(p.name),
          name: p.name,
          source: p.source,
        })),
      putPattern: async (name, source) => {
        // The playground's library is keyed by NAME and its id is a hash of
        // one (`playgroundPatternId`), so there is nothing to ask a host for.
        saved.set(savePatternLocally(name, source));
        return { id: playgroundPatternId(name) };
      },
    };
  }
  return {
    ...spriteSide,
    patterns: async () => {
      // A row whose source has not streamed in yet cannot be matched — it
      // reads as UNKNOWN, so the clone creates its own copy rather than
      // binding a pattern it has not read (`lib/sceneRefs.ts`).
      await refreshDevicePatterns();
      return get(devicePatterns).map((p) => ({ id: p.id, name: p.name, source: p.source }));
    },
    putPattern: async (name, source) => {
      const bc = compileToBytecode(source);
      if (!bc) return { error: `“${name}” does not compile — the scene was not cloned` };
      try {
        const r = await d.savePattern(name, source, bc);
        if (!r.ok || !r.id) {
          // `RunResult` is a union; only its failing arm has `error`.
          const why = ("error" in r && r.error) || "the device refused it";
          reportApiError(why, { scope: "scene", subject: name });
          return { error: why };
        }
        await refreshDevicePatterns();
        return { id: r.id };
      } catch (e) {
        reportApiError(String(e), { scope: "scene", subject: name });
        return { error: String(e) };
      }
    },
  };
}

function fromBase64(b64: string): Uint8Array {
  try {
    const s = atob(b64);
    const out = new Uint8Array(s.length);
    for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
    return out;
  } catch {
    return new Uint8Array(0);
  }
}
