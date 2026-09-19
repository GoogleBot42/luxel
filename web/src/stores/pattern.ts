// The pattern document: the working copy the editor holds, its identity, its
// control values, the local library it can be saved into, and the two
// interchange codecs (.epe files and share links).
//
// It also owns `luxel` — the wasm module every surface compiles through — and
// the two readouts the local preview loop produces (`previewFps`,
// `runtimeError`), because those belong to "the pattern as previewed" and are
// read from the header and the Settings status line as well as the editor.

import { derived, get, writable, type Readable, type Writable } from "svelte/store";
import { DEFAULT_PATTERN } from "../lib/examples";
import { parseControlHints, type ControlHint } from "../lib/hints";
import { Engine, Luxel, type ProjectionMode, type RuntimeError } from "../lib/luxel";
import {
  deletePattern,
  listPatterns,
  loadMapSrc,
  loadWorkingCopy,
  savePattern,
  saveMapSrc,
  saveWorkingCopy,
  type SavedPattern,
} from "../lib/store";
import { pixelCount, type Dims } from "./geometry";

// ---- the wasm engine host ----

/** The loaded wasm module, or undefined until `loadLuxel()` resolves. Every
 *  compile in the app (editor, gallery tiles, thumbnails, playlist rows) goes
 *  through this one instance. */
export const luxel: Writable<Luxel | undefined> = writable(undefined);

export async function loadLuxel(): Promise<Luxel> {
  const lx = await Luxel.load(`${import.meta.env.BASE_URL}luxel.wasm`);
  luxel.set(lx);
  return lx;
}

// ---- the document ----

/** The pattern text in the editor (single working copy, app-wide). */
export const source = writable(DEFAULT_PATTERN.source);
/** The editor's source differs from the pattern it was loaded from / saved
 *  as — genuinely unsaved changes. Persisted in the working copy so a reload
 *  can tell "resume my edit" from "show what's running". */
export const dirty = writable(false);
/** Name from a library/device/.epe pattern; used for the export filename. */
export const patternName = writable("");
/** Name of the built-in example the document came from. */
export const exampleName = writable(DEFAULT_PATTERN.name);
/** Set while the editor holds a device-stored pattern. */
export const devicePatternId = writable("");
/** Slider positions, keyed by control name. */
export const controlValues = writable<Record<string, number[]>>({});
/**
 * The working copy's projection override (proposal §5.4d, Gitea #468): how
 * THIS pattern is shown on a Layout whose dimensionality differs from its
 * own, when the user has overridden the device's default. `null` = inherit.
 *
 * It is a VALUE, not source: a projection never touches the pattern text (the
 * map's mistake — §5.4d), and it is keyed off the pattern's dims, so one mode
 * is all it needs to carry. Durable storage belongs to whatever *used* the
 * pattern — a playlist item's values (A9, #470) or a scene layer's (Phase B);
 * until those exist it lives here, beside the slider values, and every
 * pattern load clears it exactly as `controlValues` is cleared.
 */
export const projectionOverride: Writable<ProjectionMode | null> = writable(null);
/** `//# min=…` annotations parsed out of the source. */
export const hints: Readable<Map<string, ControlHint>> = derived(source, parseControlHints);

/** The map program (a Luxel program: plot() one point per pixel). It is
 *  GEOMETRY, not part of the pattern — since #463 it no longer rides in share
 *  links, and since A10 (#471) its editor is a screen of its own reached from
 *  the Layout picker. It lives here only because the wasm host does, and it is
 *  persisted on its own localStorage key (`luxel.mapSrc`), NOT in the
 *  pattern's working copy: a map outlives every pattern load. */
export const MAP_PROGRAM_TEMPLATE = `// Map program — runs once per pixel on the Luxel VM, so it's
// debuggable: set a gutter breakpoint and step through it.
// plot() one point per pixel (units are arbitrary; they normalize).
// This lays the strip out as a ring:
export function render(index) {
  a = index / pixelCount * PI2
  plot(cos(a), sin(a))
}`;

export const mapSrc = writable(loadMapSrc() ?? MAP_PROGRAM_TEMPLATE);
mapSrc.subscribe((s) => saveMapSrc(s));

/** The template "+ New pattern" starts from — on a strip. */
export const NEW_PATTERN = `export function render(index) {
  hsv(index / pixelCount, 1, 1)
}`;

/** The template for a new pattern on THIS Layout (Gitea #463): a matrix
 *  console starts you in `render2D`, a 3D rig in `render3D`, a strip in
 *  `render`. Starting a 64×64 panel user on a 1D ramp was the mode-blind
 *  default the audit called out (research/ui-audit.md §3). */
export function newPatternSource(dims: Dims): string {
  if (dims === 3) {
    return `export function render3D(index, x, y, z) {
  hsv((x + y + z) / 3, 1, 1)
}`;
  }
  if (dims === 2) {
    return `export function render2D(index, x, y) {
  hsv(x, 1, y)
}`;
  }
  return NEW_PATTERN;
}

// ---- local preview readouts ----

/** The browser preview loop's frame rate (the only rate a playground has). */
export const previewFps = writable(0);
export const runtimeError = writable<RuntimeError | null>(null);

// ---- local library (localStorage) ----

export const saved = writable<SavedPattern[]>(listPatterns());

export function saveToLocalLibrary(name: string, src: string): void {
  saved.set(savePattern(name, src));
}

export function deleteFromLocalLibrary(name: string): void {
  saved.set(deletePattern(name));
}

export function findSaved(name: string): SavedPattern | undefined {
  return get(saved).find((s) => s.name === name);
}

// ---- working-copy autosave ----

let autosave: ReturnType<typeof setTimeout> | undefined;
let autosaveStarted = false;

/** Debounced: the working copy survives closed tabs and reloads. */
function queueAutosave(): void {
  clearTimeout(autosave);
  autosave = setTimeout(() => {
    saveWorkingCopy({
      source: get(source),
      patternName: get(patternName),
      exampleName: get(exampleName),
      dirty: get(dirty),
    });
  }, 800);
}

/**
 * Begin persisting the working copy. Called once, AFTER boot has decided what
 * the document is — starting it earlier would race the restore and could
 * overwrite a saved working copy with the default pattern.
 *
 * Re-persists on any change to the fields the working copy stores — including
 * `dirty`, which flips to false on save WITHOUT a source change (so a reload
 * then correctly defers to the device instead of resuming a saved pattern).
 */
export function startAutosave(): void {
  if (autosaveStarted) return;
  autosaveStarted = true;
  let first = true;
  derived([source, dirty], (v) => v).subscribe(() => {
    if (first) {
      first = false; // the subscribe-time callback is the current state, not a change
      return;
    }
    queueAutosave();
  });
}

export function stopAutosave(): void {
  clearTimeout(autosave);
}

export { loadWorkingCopy, type SavedPattern };

// ---- compile helper ----

/** Compile source with the local wasm engine and return its LXBC bytecode
 *  (null if it doesn't compile). Fresh compile so the blob always matches the
 *  given source, not a stale preview engine. */
export function compileToBytecode(src: string): Uint8Array | null {
  const lx = get(luxel);
  if (!lx) return null;
  const eng = lx.compile(src, pixelCount());
  if (!(eng instanceof Engine)) return null;
  try {
    return eng.bytecode();
  } finally {
    eng.free();
  }
}

// ---- .epe import / export (Pixel Blaze pattern interchange) ----
// An .epe is JSON: { name, id, sources: { main } } — we recompile from
// sources.main, the only portable part (PB byte/blob keys are its own
// compiled artifacts).

/** Parse an .epe file; throws with a readable message when it isn't one. */
export async function parseEpe(file: File): Promise<{ name: string; source: string }> {
  const epe = JSON.parse(await file.text()) as { name?: unknown; sources?: { main?: unknown } };
  const main = epe.sources?.main;
  if (typeof main !== "string" || main.length === 0) {
    throw new Error("no sources.main — is this a Pixel Blaze .epe export?");
  }
  return {
    name:
      typeof epe.name === "string" && epe.name !== ""
        ? epe.name
        : file.name.replace(/\.(epe|json)$/i, ""),
    source: main,
  };
}

/** PB-style 17-char base-58 id, so exports round-trip into PB tooling. */
function epeId(): string {
  const chars = "ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
  let id = "";
  for (let i = 0; i < 17; i++) id += chars[Math.floor(Math.random() * chars.length)];
  return id;
}

export function exportEpe(name: string, src: string): void {
  const epe = { name, id: epeId(), sources: { main: src } };
  const blob = new Blob([JSON.stringify(epe, null, 1)], { type: "application/json" });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = `${name.replace(/[^\w\- ]+/g, "_")}.epe`;
  a.click();
  URL.revokeObjectURL(a.href);
}

// ---- shareable pattern URLs ----
// The source rides in the fragment (deflate + base64url), so links are
// self-contained: no server, works on the static playground and pasted
// between people. `#p=` is compressed, `#ps=` the plain fallback.

function b64url(bytes: Uint8Array): string {
  let s = "";
  for (const v of bytes) s += String.fromCharCode(v);
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function b64urlDecode(s: string): Uint8Array {
  const bin = atob(s.replace(/-/g, "+").replace(/_/g, "/"));
  return Uint8Array.from(bin, (c) => c.charCodeAt(0));
}

async function pipe(data: Uint8Array, stream: GenericTransformStream): Promise<Uint8Array> {
  // our arrays always own their whole buffer, so this cast is sound
  const blob = new Blob([data.buffer as ArrayBuffer]);
  const body = new Response(blob.stream().pipeThrough(stream));
  return new Uint8Array(await body.arrayBuffer());
}

/** Build the URL fragment carrying `src`.
 *
 *  The pattern ONLY (Gitea #463): a map is the Layout's, i.e. the device's or
 *  the playground's, never the pattern's — shipping it inside a link was one
 *  of the ways geometry leaked into the document (research/ui-audit.md §2.4).
 *  Links that already carry one (`#pj=`) still decode, so nothing anyone has
 *  shared breaks. */
export async function encodeShare(src: string): Promise<string> {
  const payload = src;
  const key = "p";
  const bytes = new TextEncoder().encode(payload);
  try {
    return `${key}=${b64url(await pipe(bytes, new CompressionStream("deflate-raw")))}`;
  } catch {
    return `${key}s=${b64url(bytes)}`;
  }
}

/** Decode a `#p=`/`#ps=`/`#pj=`/`#pjs=` fragment, or null if it isn't one.
 *  The `pj` forms are pre-#463 links that carried a map program; they are
 *  still honoured — the map becomes the playground's Layout choice, not part
 *  of the pattern. */
export async function decodeShare(
  hash: string,
): Promise<{ source: string; mapSrc?: string } | null> {
  const m = /^(pj|p)(s?)=([A-Za-z0-9_-]+)$/.exec(hash.replace(/^#/, ""));
  const kind = m?.[1];
  const plain = m?.[2] === "s";
  const payload = m?.[3];
  if (!kind || !payload) return null;
  try {
    const data = b64urlDecode(payload);
    const bytes = plain ? data : await pipe(data, new DecompressionStream("deflate-raw"));
    const text = new TextDecoder().decode(bytes);
    if (kind === "pj") {
      const j = JSON.parse(text) as { s: string; m?: string };
      return j.m ? { source: j.s, mapSrc: j.m } : { source: j.s };
    }
    return { source: text };
  } catch {
    return null;
  }
}
