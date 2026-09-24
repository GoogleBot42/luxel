// The scene library — one store, two backings (Gitea #480).
//
//   * CONSOLE: `/api/scenes` on the device. The whole library is one 3840-byte
//     blob there, which is why `used`/`max` are part of the read and why a
//     save can be refused with `scenes: store full (N of 3840 B)`.
//   * PLAYGROUND: `localStorage`, same shapes, so every screen below this can
//     be written once. The wire block IS the storage format (like the
//     playlist), so a scene made in the playground can be pasted at a device
//     and vice versa.
//
// What lives here: the list, which scene is active, and the verbs. What does
// NOT: any geometry (that is `stores/geometry.ts`, and on a console it is
// `deviceGeometry()` alone), and any rendering (`components/scene/`).
//
// The #563 live-push discipline applies to the ACTIVE scene only: editing a
// scene the device is not showing must never touch the device until Save.

import { get, writable, type Readable, type Writable } from "svelte/store";
import {
  emptyScene,
  parseScenes,
  sceneFromJson,
  serializeScene,
  serializeScenes,
  validSceneId,
  type Scene,
  type SceneJson,
} from "../lib/scene";
import { device, deviceCaps, pollSubscribe, playlist, playlistPause } from "./device";
import { note, reportApiError } from "./notify";

/** Where the playground keeps its library (a 5th `lib/store.ts`-style key —
 *  kept here rather than there so the scene format has ONE owner). */
const LS_SCENES = "luxel.scenes";
const LS_ACTIVE = "luxel.sceneActive";

/** Every scene, bottom-of-the-list first — the order the store returns. */
export const scenes: Writable<Scene[]> = writable([]);

/** The scene that is PLAYING (console: the device says so; playground: what
 *  the tiles ring). `""` = none. */
export const activeSceneId = writable("");

/** Bytes of the shared scene blob in use, and the ceiling. On the playground
 *  the same budget is simulated so the page teaches the same limit. */
export const sceneBytes = writable({ used: 0, max: 3840 });

/** The device's pattern-layer cap (`caps.layers`). NEVER a constant: a 300 px
 *  strip reads 3, the 4096 px panel reads 2 (#479). */
export const layerCap: Readable<number> = {
  subscribe: (run) =>
    deviceCaps.subscribe((c) => run(Math.max(1, c?.layers ?? FALLBACK_LAYER_CAP))),
};

/** What the playground (and a device too old to say) assumes. Matches
 *  `lib/settingsCaps.ts` FALLBACK_CAPS' spirit: one is always safe, but a
 *  playground has no engine budget at all, so it gets the panel's two. */
export const FALLBACK_LAYER_CAP = 2;

/** True while a save is in flight — the header's `saving…` state. */
export const sceneSaving = writable(false);

// ---- reading ----

/** Load the library. Console: `/api/scenes`. Playground: localStorage. */
export async function refreshScenes(): Promise<void> {
  const d = get(device);
  if (!d) {
    loadLocal();
    return;
  }
  try {
    const r = await d.scenes();
    scenes.set((r.scenes ?? []).map((s) => sceneFromJson(s as unknown as SceneJson)));
    activeSceneId.set(r.active ?? "");
    sceneBytes.set({ used: r.used ?? 0, max: r.max ?? 3840 });
  } catch {
    // Firmware without /api/scenes — the tab shows an empty library rather
    // than an error, exactly as `refreshDevicePatterns` does.
    scenes.set([]);
    activeSceneId.set("");
  }
}

function loadLocal(): void {
  let blob = "";
  try {
    blob = localStorage.getItem(LS_SCENES) ?? "";
  } catch {
    blob = "";
  }
  const r = parseScenes(blob);
  const list = r.ok ? r.scenes : [];
  scenes.set(list);
  sceneBytes.set({ used: new TextEncoder().encode(blob).length, max: 3840 });
  try {
    activeSceneId.set(localStorage.getItem(LS_ACTIVE) ?? "");
  } catch {
    activeSceneId.set("");
  }
}

function saveLocal(list: readonly Scene[]): void {
  const blob = serializeScenes(list);
  try {
    localStorage.setItem(LS_SCENES, blob);
  } catch {
    /* private mode: the library is session-only */
  }
  scenes.set([...list]);
  sceneBytes.set({ used: new TextEncoder().encode(blob).length, max: 3840 });
}

/** One scene by id, from whatever the store last read. */
export function sceneById(id: string): Scene | null {
  return get(scenes).find((s) => s.id === id) ?? null;
}

// ---- writing ----

/** 8 lowercase hex, the id shape both hosts assign. The playground assigns
 *  its own (the device assigns the real one on a console). */
export function randomSceneId(): string {
  const b = new Uint8Array(4);
  crypto.getRandomValues(b);
  return Array.from(b, (v) => v.toString(16).padStart(2, "0")).join("");
}

export interface SceneSave {
  ok: boolean;
  id?: string;
  error?: string;
}

/**
 * Create or replace a scene. `scene.id` empty = create (the device assigns
 * the id; the playground makes one).
 *
 * Every refusal goes through `reportApiError` with scope `scene`, so a
 * `scenes: store full` lands in the ONE error strip with a sentence rather
 * than in a page-local string nobody reads.
 */
export async function saveScene(scene: Scene): Promise<SceneSave> {
  const d = get(device);
  const block = serializeScene(scene);
  sceneSaving.set(true);
  try {
    if (!d) {
      const list = [...get(scenes)];
      const id = scene.id === "" ? randomSceneId() : scene.id;
      const next = { ...scene, id };
      const at = list.findIndex((s) => s.id === id);
      if (at === -1) list.push(next);
      else list[at] = next;
      // The playground teaches the same ceiling the device enforces.
      const blob = serializeScenes(list);
      const used = new TextEncoder().encode(blob).length;
      if (used > get(sceneBytes).max) {
        const msg = `scenes: store full (${used} of ${get(sceneBytes).max} B)`;
        reportApiError(msg, { scope: "scene", subject: scene.name });
        return { ok: false, error: msg };
      }
      saveLocal(list);
      return { ok: true, id };
    }
    const r = await d.saveScene(scene.id, block);
    if (!r.ok) {
      reportApiError(r.error, { scope: "scene", subject: scene.name });
      return { ok: false, error: r.error };
    }
    await refreshScenes();
    return { ok: true, id: r.id };
  } catch (e) {
    const msg = String(e);
    reportApiError(msg, { scope: "scene", subject: scene.name });
    return { ok: false, error: msg };
  } finally {
    sceneSaving.set(false);
  }
}

/** Delete a scene. */
export async function deleteScene(id: string): Promise<boolean> {
  const d = get(device);
  if (!d) {
    saveLocal(get(scenes).filter((s) => s.id !== id));
    if (get(activeSceneId) === id) setLocalActive("");
    return true;
  }
  try {
    const r = await d.deleteScene(id);
    if (!r.ok) {
      reportApiError(r.error ?? "rejected", { scope: "scene" });
      return false;
    }
  } catch (e) {
    reportApiError(String(e), { scope: "scene" });
    return false;
  }
  await refreshScenes();
  return true;
}

/** Duplicate a scene under a new name; returns the new id. */
export async function duplicateScene(id: string): Promise<string> {
  const src = sceneById(id);
  if (!src) return "";
  const copy: Scene = {
    ...structuredClone(src),
    id: "",
    name: nextCopyName(src.name, get(scenes).map((s) => s.name)),
  };
  const r = await saveScene(copy);
  return r.ok ? (r.id ?? "") : "";
}

/** `Clock overlay` → `Clock overlay 2` → `Clock overlay 3`. */
export function nextCopyName(name: string, taken: readonly string[]): string {
  const base = name.replace(/ \d+$/, "");
  for (let n = 2; n < 999; n++) {
    const candidate = `${base} ${n}`;
    if (!taken.includes(candidate)) return candidate;
  }
  return `${base} copy`;
}

/**
 * Play a scene — the activation verb, and the exact peer of
 * `activateDevicePattern`: it parks a playing playlist first, because a
 * direct play is a takeover of the fixture and an auto-advance a few seconds
 * later would replace it with nothing on screen saying why (#538 r2).
 */
export async function activateScene(id: string): Promise<boolean> {
  const d = get(device);
  if (!d) {
    setLocalActive(id);
    return true;
  }
  if (get(playlist).playing) await playlistPause();
  try {
    const r = await d.activateScene(id);
    if (!r.ok) {
      reportApiError(r.error ?? "rejected", { scope: "scene", subject: sceneById(id)?.name });
      return false;
    }
  } catch (e) {
    reportApiError(String(e), { scope: "scene" });
    return false;
  }
  activeSceneId.set(id);
  return true;
}

function setLocalActive(id: string): void {
  activeSceneId.set(id);
  try {
    localStorage.setItem(LS_ACTIVE, id);
  } catch {
    /* ignore */
  }
}

// ---- the live push (#563 / #585) ----
//
// The rule, in one sentence: an edit reaches the device ONLY while the scene
// being edited is the one the device is showing. Opening a scene, or editing
// any other one, touches nothing until Save. That is the pattern editor's
// `livePush` gate applied to a document that has no `dirty` flag of its own.
//
// Coalesced to 10 Hz because dragging the marquee produces a pointermove
// storm and each push is a whole-record POST.

const LIVE_PUSH_MS = 100;
let liveTimer: ReturnType<typeof setTimeout> | undefined;
let livePending: Scene | null = null;

/** True when editing `id` should reach the device as it happens. */
export function shouldLivePush(id: string): boolean {
  return get(device) !== null && id !== "" && get(activeSceneId) === id;
}

/** Push an edit to the device if — and only if — this scene is the running
 *  one. Safe to call on every keystroke: it coalesces. */
export function livePushScene(scene: Scene): void {
  if (!shouldLivePush(scene.id)) return;
  livePending = scene;
  if (liveTimer !== undefined) return;
  liveTimer = setTimeout(() => {
    liveTimer = undefined;
    const s = livePending;
    livePending = null;
    if (!s || !shouldLivePush(s.id)) return;
    const d = get(device);
    if (!d) return;
    void d
      .saveScene(s.id, serializeScene(s))
      .then((r) => {
        if (!r.ok) reportApiError(r.error, { scope: "scene", subject: s.name });
      })
      .catch(() => note("scene", "the device did not take that edit", 4000));
  }, LIVE_PUSH_MS);
}

/** Drop anything queued (leaving the editor, disconnecting). */
export function cancelLivePush(): void {
  clearTimeout(liveTimer);
  liveTimer = undefined;
  livePending = null;
}

// ---- polling ----
//
// The 5th cadence on the ONE scheduler (docs/web-architecture.md): which
// scene is playing can change from Home Assistant, the playlist or another
// browser tab, so the page reads it while it is open — and only while.

export function startScenePoll(): () => void {
  return pollSubscribe("scenes", 2000, refreshScenes);
}

/** A blank scene with the base layer the editor opens on. Exported so the
 *  Scenes page and the playlist's `New scene…` agree on what "new" means. */
export function newScene(name = "New scene"): Scene {
  return emptyScene(name);
}

/** Guard for an id that came out of a URL fragment. */
export function isSceneId(s: string): boolean {
  return validSceneId(s);
}
