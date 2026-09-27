// The SOURCE forms of the shipped sprite and scene library (Gitea #785), and
// nothing else: two pure parsers plus the one id derivation the whole feature
// hangs off. `web/tools/gen-sprite-scene-gallery.mjs` reads `library/sprites/`
// and `library/scenes/` through these, the gate re-reads them, and
// `web/tests/librarySource.test.mjs` pins them — so a malformed library file
// fails the build with a line number instead of shipping a broken tile.
//
// WHY A TEXT PIXEL FORMAT RATHER THAN PNGs. `library/` is a reviewable tree:
// every pattern in it is source a person can read in a diff, and that is the
// property the clean-room policy is built on. A committed PNG is a blob — a
// reviewer cannot see that a sprite changed, only that some bytes did — and it
// would put an image decoder in the web build for art that is authored here
// anyway. So the shipped art is TEXT: one character per texel, the palette
// spelled out above it. The image path is not lost — `luxel sprite import` and
// the console's `Import image…` (#784) are how a USER's PNG becomes a sprite,
// and Gitea #873 tracks teaching the generator to read one too.
//
// THE REFERENCE PROBLEM, and its answer. A scene names its patterns and
// sprites by STORE id (docs/spec/scenes.md §1, the `I` line), and 8 hex digits
// of somebody else's device mean nothing here. A library scene's SOURCE
// therefore writes `I @pat/aurora-2d` / `I @spr/heart` — the library slug,
// which is the file name and so is stable across renames of the display name
// — and the generator rewrites each one to a DERIVED id (`libRefId`) before
// the record is serialized. That keeps the shipped wire a perfectly ordinary
// scene record: the real `luxel_core::scene::parse` validates it, the real
// compositor previews it, and `lib/sceneRefs.ts` only has to swap ids at clone
// time. No parser anywhere grows a special case.

import {
  SPRITE_MAX_BYTES,
  SPRITE_MAX_COLORS,
  SPRITE_MAX_EDGE,
  SPRITE_MAX_FPS,
  SPRITE_MAX_FRAMES,
  SPRITE_MAX_NAME,
  hexToRgb8,
  spriteBytes,
  type Rgb8,
  type Sprite,
} from "./sprite.ts";
import { utf8Len } from "./scene.ts";

/** What a library scene's `I` line refers to. */
export type LibRefKind = "pat" | "spr";

export const LIB_REF_KINDS: readonly LibRefKind[] = ["pat", "spr"];

/**
 * The 8-hex id a library reference is serialized as — FNV-1a over
 * `<kind>/<slug>`, which is the same arithmetic in every language and needs no
 * table to reproduce.
 *
 * It is a PLACEHOLDER, never a store id: the shipped wire carries it so the
 * record parses like any other, and a clone rewrites it to whatever the target
 * store assigns. Two different slugs hashing to the same id would make two
 * references indistinguishable, so the generator asserts the whole shipped set
 * is collision-free rather than hoping.
 */
export function libRefId(kind: LibRefKind, slug: string): string {
  return fnv8(`${kind}/${slug}`);
}

/**
 * The 8-hex id a shipped SCENE wears while it is only a library row.
 *
 * It needs one at all because the record's `S -` line leaves the id empty and
 * every scene surface keys on it — one keyed `{#each}` over five id-less scenes
 * is one tile. It is discarded on clone (the host mints the real one), and it
 * has to be 8 hex because `luxel_core::scene::parse` validates the `S` line
 * like any other: a slug there is `scene: line 1: bad scene id`.
 */
export function libSceneId(slug: string): string {
  return fnv8(`scn/${slug}`);
}

function fnv8(key: string): string {
  let h = 0x811c9dc5;
  for (const b of new TextEncoder().encode(key)) {
    h = Math.imul(h ^ b, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, "0");
}

/** `library/sprites/heart.sprite` → `heart`. The slug IS the file stem. */
export function libSlug(fileName: string): string {
  return fileName.replace(/\.[^.]*$/, "");
}

// ---- library/sprites/<slug>.sprite ----

export type SpriteSourceResult = { ok: true; sprite: Sprite } | { ok: false; error: string };

/**
 * One sprite, as text. The grammar, in full:
 *
 * ```
 * # any line starting with # is a comment, anywhere
 * name: Heart                  required, 1..=64 UTF-8 bytes
 * fps: 12                      optional, 0..=30, default 0 (a still sprite)
 * palette:
 *   r ff3355                   one <char> <rrggbb> per line, in record order
 *   h ff8fa3
 *
 * frame                        one `frame` line per frame, then its rows
 * .rr...rr.                    one character per texel; `.` is TRANSPARENT
 * rhrrrrrrr
 * ```
 *
 * Blank lines and comments are skipped everywhere, which is why a fully
 * transparent row has to be written as dots rather than left empty. Every
 * frame must be the same size, and a character that is not `.` must be in the
 * palette — the two mistakes that are easy to make by hand and impossible to
 * see in a rendered thumbnail.
 */
export function parseSpriteSource(text: string): SpriteSourceResult {
  let name = "";
  let fps = 0;
  const palette: Rgb8[] = [];
  /** character → index byte (1-based; 0 is transparent). */
  const codes = new Map<string, number>();
  const frames: string[][] = [];
  type Mode = "header" | "palette" | "frames";
  let mode: Mode = "header";
  const bad = (n: number, why: string): SpriteSourceResult => ({
    ok: false,
    error: `sprite: line ${n}: ${why}`,
  });

  const lines = text.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const raw = (lines[i] ?? "").replace(/\r+$/, "");
    const line = raw.trim();
    if (line === "" || line.startsWith("#")) continue;
    const n = i + 1;

    if (line === "frame") {
      mode = "frames";
      frames.push([]);
      continue;
    }
    if (mode === "frames") {
      const rows = frames[frames.length - 1];
      if (!rows) return bad(n, "a pixel row before the first `frame`");
      rows.push(line);
      continue;
    }
    if (line === "palette:") {
      if (mode === "palette") return bad(n, "a second `palette:`");
      mode = "palette";
      continue;
    }
    if (mode === "palette") {
      const parts = line.split(/\s+/);
      if (parts.length !== 2) return bad(n, "a palette line is `<char> <rrggbb>`");
      const [ch, hex] = parts as [string, string];
      if ([...ch].length !== 1) return bad(n, `palette key ${JSON.stringify(ch)} is not one character`);
      if (ch === "." || ch === "#") return bad(n, `${ch} cannot be a palette key`);
      if (codes.has(ch)) return bad(n, `palette key ${ch} appears twice`);
      const rgb = hexToRgb8(hex);
      if (!rgb) return bad(n, `${JSON.stringify(hex)} is not rrggbb`);
      if (palette.length >= SPRITE_MAX_COLORS) return bad(n, `over ${SPRITE_MAX_COLORS} colours`);
      palette.push(rgb);
      codes.set(ch, palette.length);
      continue;
    }
    // header
    const at = line.indexOf(":");
    if (at < 0) return bad(n, "expected `key: value`, `palette:` or `frame`");
    const key = line.slice(0, at).trim();
    const value = line.slice(at + 1).trim();
    if (key === "name") {
      if (value === "") return bad(n, "empty name");
      if (utf8Len(value) > SPRITE_MAX_NAME) return bad(n, `name is over ${SPRITE_MAX_NAME} bytes`);
      name = value;
    } else if (key === "fps") {
      const v = Number.parseInt(value, 10);
      if (!Number.isFinite(v) || v < 0 || v > SPRITE_MAX_FPS) {
        return bad(n, `fps must be 0..${SPRITE_MAX_FPS}`);
      }
      fps = v;
    } else {
      return bad(n, `unknown header ${JSON.stringify(key)}`);
    }
  }

  if (name === "") return { ok: false, error: "sprite: no `name:` header" };
  if (frames.length === 0) return { ok: false, error: "sprite: no frames" };
  if (frames.length > SPRITE_MAX_FRAMES) {
    return { ok: false, error: `sprite: ${frames.length} frames is over ${SPRITE_MAX_FRAMES}` };
  }
  const first = frames[0] ?? [];
  const h = first.length;
  const w = [...(first[0] ?? "")].length;
  if (h === 0 || w === 0) return { ok: false, error: "sprite: the first frame has no rows" };
  if (w > SPRITE_MAX_EDGE || h > SPRITE_MAX_EDGE) {
    return { ok: false, error: `sprite: ${w}×${h} is over ${SPRITE_MAX_EDGE}×${SPRITE_MAX_EDGE}` };
  }

  const index = new Uint8Array(w * h * frames.length);
  for (let f = 0; f < frames.length; f++) {
    const rows = frames[f] ?? [];
    if (rows.length !== h) {
      return { ok: false, error: `sprite: frame ${f + 1} has ${rows.length} rows, frame 1 has ${h}` };
    }
    for (let y = 0; y < h; y++) {
      const cells = [...(rows[y] ?? "")];
      if (cells.length !== w) {
        return {
          ok: false,
          error: `sprite: frame ${f + 1} row ${y + 1} is ${cells.length} wide, frame 1 is ${w}`,
        };
      }
      for (let x = 0; x < w; x++) {
        const ch = cells[x] ?? ".";
        if (ch === ".") continue;
        const k = codes.get(ch);
        if (k === undefined) {
          return {
            ok: false,
            error: `sprite: frame ${f + 1} row ${y + 1}: ${JSON.stringify(ch)} is not in the palette`,
          };
        }
        index[f * w * h + y * w + x] = k;
      }
    }
  }

  const sprite: Sprite = { name, w, h, frames: frames.length, fps, palette, index };
  const bytes = spriteBytes(sprite);
  if (bytes > SPRITE_MAX_BYTES) {
    return {
      ok: false,
      error: `sprite: ${bytes} B is over the ${SPRITE_MAX_BYTES / 1024} KiB cap`,
    };
  }
  // A colour declared and never painted with would ship three bytes and a
  // misleading `colors` count, and it is always an authoring slip.
  for (const [ch, k] of codes) {
    if (!index.includes(k)) {
      return { ok: false, error: `sprite: palette key ${ch} is never used` };
    }
  }
  return { ok: true, sprite };
}

// ---- library/scenes/<slug>.scene ----

/** One resolved reference out of a library scene. */
export interface LibRef {
  /** The derived 8-hex id it carries in the shipped wire. */
  id: string;
  kind: LibRefKind;
  /** The library slug: `library/<slug>.js` for `pat`, `library/sprites/<slug>.sprite` for `spr`. */
  slug: string;
}

export type SceneSourceResult =
  | { ok: true; wire: string; refs: LibRef[] }
  | { ok: false; error: string };

/** `@pat/<slug>` or `@spr/<slug>`; slugs are the file-name alphabet. */
const REF_RE = /^@(pat|spr)\/([a-z0-9][a-z0-9-]*)$/;

/**
 * One scene, as the wire record it already is (docs/spec/scenes.md §1), with
 * two liberties taken for reviewability and undone here:
 *
 *   * `#` comment lines, which the wire has no room for.
 *   * `I @pat/<slug>` / `I @spr/<slug>` in place of a store id.
 *
 * What comes back is the ORDINARY wire — `libRefId`'s 8 hex in every `I` line
 * — plus the reference table. Nothing downstream knows this file format
 * exists; the generator hands the wire to the real parser.
 */
export function parseSceneSource(text: string): SceneSourceResult {
  const out: string[] = [];
  const refs: LibRef[] = [];
  const seen = new Set<string>();
  const lines = text.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const raw = (lines[i] ?? "").replace(/\r+$/, "");
    if (raw.trim().startsWith("#")) continue;
    if (raw.trim() === "") continue;
    const n = i + 1;
    const tok = raw.trim().split(/\s+/);
    if (tok[0] === "I") {
      const target = tok[1] ?? "";
      if (target.startsWith("@")) {
        const m = REF_RE.exec(target);
        if (!m) {
          return { ok: false, error: `scene: line ${n}: ${JSON.stringify(target)} is not @pat/<slug> or @spr/<slug>` };
        }
        const kind = m[1] as LibRefKind;
        const slug = m[2] as string;
        const id = libRefId(kind, slug);
        if (!seen.has(id)) {
          seen.add(id);
          refs.push({ id, kind, slug });
        }
        out.push(`I ${id}`);
        continue;
      }
      return { ok: false, error: `scene: line ${n}: a library scene must reference @pat/… or @spr/…, not a store id` };
    }
    if (raw.includes("@pat/") || raw.includes("@spr/")) {
      return { ok: false, error: `scene: line ${n}: a library reference belongs on an I line` };
    }
    out.push(raw);
  }
  if (out.length === 0) return { ok: false, error: "scene: empty" };
  return { ok: true, wire: out.join("\n") + "\n", refs };
}
