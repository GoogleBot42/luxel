// Recompiling a device's stored patterns after a firmware format bump
// (Gitea #643) — the console doing, automatically, what a checkout full of
// tooling had to do by hand on the Athom.
//
// The situation: the firmware reads LXBC vN, every blob in its store is
// vN-1, and the strip is dark because nothing in the playlist decodes. The
// SOURCES are intact — only the compiled blobs are unreadable — and the
// console has the compiler the device does not. So the repair is mechanical:
// fetch each stale pattern's source, compile it here, save it back under the
// SAME NAME. `patterns::save` upserts by name, so the id survives and every
// playlist reference with it (this is the bit that makes the repair safe
// rather than a rebuild: nothing the user arranged is lost).
//
// Rules this module keeps:
//   - it never runs when the bundle is older than the device (lib/bcskew.ts);
//     recompiling with an older compiler would replace unreadable blobs with
//     equally unreadable ones. That case is a banner, not a repair.
//   - it never touches a pattern that is not flagged stale.
//   - it is idempotent: a second run finds nothing to do.
//   - it is resumable: each pattern is its own save, so an interrupted run
//     leaves the rest still flagged and a re-run picks them up.
//   - a pattern whose source no longer compiles is LEFT AS IT IS and listed.
//     Silently dropping it would destroy the only surviving copy.

import { stalePatternIds } from "./bcskew.ts";
import type { DevicePatternRow, Playlist, RunResult } from "./device";

/** The device-side calls the repair needs (a `DeviceSession` satisfies it);
 *  narrowed so a test can drive the whole thing in-process. */
export interface HealTarget {
  patterns(): Promise<DevicePatternRow[]>;
  patternSource(id: string): Promise<{ id: string; name: string; source: string }>;
  savePattern(
    name: string,
    source: string,
    bytecode: Uint8Array,
  ): Promise<RunResult & { id?: string }>;
  activatePattern(id: string): Promise<RunResult>;
  playlist(): Promise<Playlist>;
  status(): Promise<{ vmerr: string | null }>;
}

export interface HealFailure {
  id: string;
  name: string;
  why: string;
}

export interface HealReport {
  /** Patterns found stale. 0 = nothing to do (the normal outcome). */
  found: number;
  /** Recompiled and saved back. */
  repaired: number;
  /** Left exactly as they were, with the reason. */
  failed: HealFailure[];
  /** The stored pattern re-activated afterwards, if any. */
  reactivated: string | null;
}

/** Compile a pattern source to LXBC with THIS bundle's engine. Injected
 *  rather than imported: the compiler lives in `stores/pattern.ts`, which
 *  the device store may not import (its own layering rule), and injecting
 *  keeps this module testable without wasm. */
export type CompileFn = (source: string) => Uint8Array | null;

export interface HealOptions {
  /** Which stored pattern the device is running (`""` if unknown) — used to
   *  decide whether the strip needs a kick once the blob is readable. */
  runningId?: string;
  /** Called before each pattern, for a "recompiling 3 of 11…" line. */
  onProgress?: (done: number, total: number, name: string) => void;
}

/**
 * Find the stale stored patterns and recompile them in place.
 *
 * The caller is responsible for the skew guard — call `bcSkew()` first and
 * do NOT call this unless it says `"match"`. That check needs the wasm
 * engine's format and the device's, both of which the caller already holds.
 */
export async function healStaleStore(
  dev: HealTarget,
  compile: CompileFn,
  opts: HealOptions = {},
): Promise<HealReport> {
  const report: HealReport = { found: 0, repaired: 0, failed: [], reactivated: null };

  const rows = await dev.patterns();
  // The playlist's per-item `invalid` and the status `vmerr` are the
  // fallback evidence on firmware that does not flag rows (docs/api.md).
  const invalid = new Map<string, string>();
  let vmerr: string | null = null;
  try {
    const pl = await dev.playlist();
    for (const it of pl.items) if (it.invalid) invalid.set(it.id, it.invalid);
  } catch {
    // no playlist route / no playlist — the row flags still stand
  }
  try {
    vmerr = (await dev.status()).vmerr;
  } catch {
    // unreadable status is not a reason to skip the row flags either
  }

  const ids = stalePatternIds(rows, opts.runningId ?? "", vmerr, invalid);
  report.found = ids.length;
  if (ids.length === 0) return report;

  const nameOf = new Map(rows.map((r) => [r.id, r.name]));
  let done = 0;
  for (const id of ids) {
    const name = nameOf.get(id) ?? id;
    opts.onProgress?.(done, ids.length, name);
    done++;
    let src: { name: string; source: string };
    try {
      src = await dev.patternSource(id);
    } catch (e) {
      report.failed.push({ id, name, why: `its source could not be read (${String(e)})` });
      continue;
    }
    if (!src.source) {
      report.failed.push({ id, name, why: "the device holds no source for it" });
      continue;
    }
    const bc = compile(src.source);
    if (!bc) {
      report.failed.push({ id, name: src.name || name, why: "its source no longer compiles" });
      continue;
    }
    // Save by NAME, not by id: that is what makes the id — and every
    // playlist entry pointing at it — survive the repair.
    let r: RunResult;
    try {
      r = await dev.savePattern(src.name || name, src.source, bc);
    } catch (e) {
      report.failed.push({ id, name: src.name || name, why: `the save failed (${String(e)})` });
      continue;
    }
    if (!r.ok) {
      report.failed.push({ id, name: src.name || name, why: r.error });
      continue;
    }
    report.repaired++;
  }

  // The running pattern's engine is still the one that failed to load, so
  // the fixture stays dark until something re-enters it. Re-activating by id
  // is the smallest possible kick and does not disturb a playing playlist
  // (the firmware does not treat an id activation as a takeover).
  const running = opts.runningId ?? "";
  if (running && ids.includes(running) && !report.failed.some((f) => f.id === running)) {
    try {
      if ((await dev.activatePattern(running)).ok) report.reactivated = running;
    } catch {
      // the next playlist advance will do it
    }
  }
  return report;
}

/** One line for the user, written from a finished report. Empty string when
 *  there is nothing worth saying (the normal case). */
export function healSummary(r: HealReport): string {
  if (r.found === 0) return "";
  const parts: string[] = [];
  if (r.repaired > 0)
    parts.push(
      `recompiled ${r.repaired} stored pattern${r.repaired === 1 ? "" : "s"} for this firmware`,
    );
  if (r.failed.length > 0)
    parts.push(
      `${r.failed.length} could not be repaired and ${r.failed.length === 1 ? "was" : "were"} left as ${r.failed.length === 1 ? "it is" : "they are"}: ` +
        r.failed.map((f) => `${f.name} (${f.why})`).join("; "),
    );
  return parts.join(" — ");
}
