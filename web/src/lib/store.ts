// Local persistence for the playground: an autosaved working copy (never
// lose edits to a closed tab) and a named pattern library, both in
// localStorage. This UI is the prototype for the device's pattern CRUD —
// keep the shapes simple and serializable.

import type { Layout } from "./examples";

export interface SavedPattern {
  name: string;
  source: string;
  savedAt: number;
}

const LIB_KEY = "luxel.patterns";
const CUR_KEY = "luxel.current";

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

export function listPatterns(): SavedPattern[] {
  const list = read<SavedPattern[]>(LIB_KEY) ?? [];
  return Array.isArray(list) ? list.filter((p) => p && typeof p.name === "string") : [];
}

/** Save (or overwrite, by name) a pattern in the library. */
export function savePattern(name: string, source: string): SavedPattern[] {
  const list = listPatterns().filter((p) => p.name !== name);
  list.push({ name, source, savedAt: Date.now() });
  list.sort((a, b) => a.name.localeCompare(b.name));
  write(LIB_KEY, list);
  return list;
}

export function deletePattern(name: string): SavedPattern[] {
  const list = listPatterns().filter((p) => p.name !== name);
  write(LIB_KEY, list);
  return list;
}

export interface WorkingCopy {
  source: string;
  layout: Layout;
  /** name context so the picker label survives a reload */
  patternName: string;
  exampleName: string;
}

export function saveWorkingCopy(wc: WorkingCopy): void {
  write(CUR_KEY, wc);
}

export function loadWorkingCopy(): WorkingCopy | null {
  const wc = read<WorkingCopy>(CUR_KEY);
  if (!wc || typeof wc.source !== "string" || !wc.layout?.kind) return null;
  return wc;
}
