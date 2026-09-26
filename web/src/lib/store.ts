// Local persistence for the playground: an autosaved working copy (never
// lose edits to a closed tab), a named pattern library, and the "Preview as"
// Layout choice — all in localStorage. This UI is the prototype for the
// device's pattern CRUD — keep the shapes simple and serializable.

import { parsePreviewAs, type PreviewAs } from "./geometry";
import { isTaggedSpriteSource } from "./sprite";

export interface SavedPattern {
  name: string;
  source: string;
  savedAt: number;
}

const LIB_KEY = "luxel.patterns";
const CUR_KEY = "luxel.current";
const PREVIEW_AS_KEY = "luxel.previewAs";
const MAP_SRC_KEY = "luxel.mapSrc";

function read<T>(key: string): T | null {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : null;
  } catch {
    return null;
  }
}

function write(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* storage full / disabled — persistence is best-effort */
  }
}

/**
 * The playground's pattern library — SPRITES EXCLUDED.
 *
 * Before Gitea #740 a sprite WAS a pattern (`// @sprite …` on line 1), so
 * every list and picker in the app offered sprites as if they were playable
 * patterns. Sprites are their own records now with their own tab, and the one
 * place local lists are built is here, so this is where they leave: a stored
 * pattern that still carries the old tag is a sprite waiting for
 * `migrateTaggedSprites()` and is not offered as a pattern in the meantime.
 * [`listAllPatterns`] is the unfiltered read the migration itself needs.
 */
export function listPatterns(): SavedPattern[] {
  return listAllPatterns().filter((p) => !isTaggedSpriteSource(p.source ?? ""));
}

/** Every row, tagged sprites included — the migration's reader, and nothing
 *  else's. */
export function listAllPatterns(): SavedPattern[] {
  const list = read<SavedPattern[]>(LIB_KEY) ?? [];
  return Array.isArray(list) ? list.filter((p) => p && typeof p.name === "string") : [];
}

/** Save (or overwrite, by name) a pattern in the library.
 *
 *  The WRITERS read `listAllPatterns`, never the filtered view: writing back
 *  what `listPatterns` returned would silently delete every not-yet-migrated
 *  sprite from the library the first time anything saved a pattern. */
export function savePattern(name: string, source: string): SavedPattern[] {
  const list = listAllPatterns().filter((p) => p.name !== name);
  list.push({ name, source, savedAt: Date.now() });
  list.sort((a, b) => a.name.localeCompare(b.name));
  write(LIB_KEY, list);
  return listPatterns();
}

export function deletePattern(name: string): SavedPattern[] {
  write(
    LIB_KEY,
    listAllPatterns().filter((p) => p.name !== name),
  );
  return listPatterns();
}

export interface WorkingCopy {
  source: string;
  /** name context so the picker label survives a reload */
  patternName: string;
  exampleName: string;
  /** true once the source was edited away from the pattern it was loaded from
   *  (or saved as) — i.e. the reload has genuinely unsaved changes. Drives the
   *  device resume decision (resume a dirty edit vs. show what's running). */
  dirty: boolean;
  /** The device pattern this copy is an edit OF, or "" (a library pick, a new
   *  pattern, an ad-hoc program). With `dirty` it is the whole input to the
   *  boot resume decision — only an edit of the pattern the device is RUNNING
   *  may resume live push (`lib/resume.ts`, Gitea #585). */
  devicePatternId: string;
}

export function saveWorkingCopy(wc: WorkingCopy): void {
  write(CUR_KEY, wc);
}

/** The autosaved working copy. Pre-#463 copies also carried the preview rig
 *  (`layout`); it is migrated by `loadPreviewAs` and ignored here — geometry
 *  is no longer part of the pattern document. */
export function loadWorkingCopy(): WorkingCopy | null {
  const wc = read<WorkingCopy>(CUR_KEY);
  if (!wc || typeof wc.source !== "string") return null;
  return {
    ...wc,
    dirty: wc.dirty === true, // default legacy copies to clean
    // a copy written before #585 carries no id: treat it as an edit of nothing,
    // which is the conservative answer (local preview, no boot push)
    devicePatternId: typeof wc.devicePatternId === "string" ? wc.devicePatternId : "",
  };
}

/** The playground's Layout choice (the "Preview as" chip). Falls back to the
 *  rig a pre-#463 working copy persisted, so an existing tab keeps previewing
 *  on the geometry it was left on. */
export function loadPreviewAs(): PreviewAs | null {
  return (
    parsePreviewAs(read<unknown>(PREVIEW_AS_KEY)) ??
    parsePreviewAs((read<{ layout?: unknown }>(CUR_KEY) ?? {}).layout)
  );
}

export function savePreviewAs(choice: PreviewAs): void {
  write(PREVIEW_AS_KEY, choice);
}

/** The map program's source (A10, #471). It is geometry, not a pattern, so it
 *  is persisted on its own key rather than inside the working copy — the
 *  editor screen is reached from the Layout picker and its document must
 *  survive a reload and every pattern load. Older copies that carried it in a
 *  share link are handled by the boot path, not here.
 *
 *  There is no device-side counterpart: `GET /api/map` reports a count and
 *  dims, never the program that produced them (Gitea #517), so a console
 *  restores the program from this browser too. */
export function loadMapSrc(): string | null {
  const s = read<string>(MAP_SRC_KEY);
  return typeof s === "string" && s !== "" ? s : null;
}

export function saveMapSrc(src: string): void {
  write(MAP_SRC_KEY, src);
}
