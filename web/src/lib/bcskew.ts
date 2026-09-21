// Bytecode-format skew between THIS web bundle and the device it is talking
// to, and the detection half of the stored-store self-heal (Gitea #643).
//
// Two independent version numbers meet at a device:
//
//   - the format this bundle's compiler EMITS — `Luxel.bcFormat()`, from
//     `luxel_core::bytecode::FORMAT_VERSION` compiled into luxel.wasm;
//   - the format the firmware READS — `/api/status`'s `bc_format`.
//
// They are shipped together in a release and normally agree. They come apart
// whenever a device takes a firmware OTA without the matching web assets (the
// Athom, 2026-09-20: firmware on v6, the console it served compiling v5 —
// every stored pattern unreadable, every save refused, the strip dark). Which
// way they are apart decides what the console may do:
//
//   bundle < device   this console cannot produce anything the device will
//                     run. It must NOT heal — recompiling the store with it
//                     would replace readable-but-old blobs with blobs that
//                     are equally unreadable. Banner: install the matching
//                     web assets (the banner carries the upload).
//   bundle > device   the firmware is behind this console. Patterns saved
//                     from here would not run. Banner: update the firmware.
//   equal             the console is authoritative and may recompile any
//                     stored pattern the device reports as `stale`.
//
// Pure functions only, so the rule is unit-testable (tests/bcskew.test.mjs)
// and stated exactly once.

import type { DevicePatternRow } from "./device";

export type BcSkew = "unknown" | "match" | "bundle-older" | "bundle-newer";

/** Compare this bundle's compiler with a device's reader. `device` is
 *  `undefined` on firmware older than the field, and `bundle` is 0 on a
 *  luxel.wasm older than `lx_bc_format` — either way the answer is
 *  `"unknown"`, which means say nothing and change nothing. */
export function bcSkew(bundle: number, device: number | undefined): BcSkew {
  if (!bundle || !device) return "unknown";
  if (bundle === device) return "match";
  return bundle < device ? "bundle-older" : "bundle-newer";
}

export interface SkewBanner {
  /** Banner id / `data-role` stem. */
  id: string;
  text: string;
  /** True when the banner should offer the web-asset upload inline — the
   *  only case where the fix is a file the user already has. */
  offerAssets: boolean;
}

/** What to say about a skew, or null when there is nothing to say. */
export function skewBanner(
  skew: BcSkew,
  bundle: number,
  device: number | undefined,
): SkewBanner | null {
  if (skew === "bundle-older")
    return {
      id: "bc-bundle-older",
      text:
        `This web bundle is older than the firmware (compiles v${bundle}, device reads v${device}) ` +
        `— install the matching web assets. Until then, patterns saved from here will not run, ` +
        `and stored patterns cannot be repaired from this console.`,
      offerAssets: true,
    };
  if (skew === "bundle-newer")
    return {
      id: "bc-bundle-newer",
      text:
        `The firmware is older than this web bundle (device reads v${device}, this console compiles v${bundle}) ` +
        `— update the firmware. Patterns saved from this console would not run on it.`,
      offerAssets: false,
    };
  return null;
}

/** The firmware's own wording for an unreadable blob
 *  (`luxel_core::bytecode::BcError::Version`). Matched rather than
 *  reconstructed: it is what lands in `/api/status`'s `vmerr` and in a
 *  playlist item's `invalid` on firmware that predates the structured
 *  `stale` flag. */
const STALE_TEXT = /bytecode format v(\d+) \(this build reads v(\d+)\)/;

/** `{found, reads}` when `msg` is the device's stale-bytecode complaint,
 *  else null. Works on `/api/status.vmerr`, on a playlist item's `invalid`
 *  and on a `RunResult.error` carrying `code: "bc-version"`. */
export function parseStaleError(msg: string | null | undefined): { found: number; reads: number } | null {
  if (!msg) return null;
  const m = STALE_TEXT.exec(msg);
  return m ? { found: Number(m[1]), reads: Number(m[2]) } : null;
}

/** Ids of the stored patterns that need recompiling.
 *
 *  The structured `stale` flag is the answer when the firmware reports it.
 *  The free-text fallback covers firmware that predates it: `vmerr` names
 *  the RUNNING pattern and each playlist item carries its own `invalid`, so
 *  between them every pattern the device has actually tried to load is
 *  accounted for. A pattern nothing has tried to load and nothing flags is
 *  left alone — this never recompiles on suspicion. */
export function stalePatternIds(
  rows: readonly DevicePatternRow[],
  runningId: string,
  vmerr: string | null | undefined,
  playlistInvalid: ReadonlyMap<string, string>,
): string[] {
  const flagged = rows.filter((r) => r.stale === true).map((r) => r.id);
  if (flagged.length > 0) return flagged;
  const ids = new Set<string>();
  const known = new Set(rows.map((r) => r.id));
  if (runningId && known.has(runningId) && parseStaleError(vmerr)) ids.add(runningId);
  for (const [id, why] of playlistInvalid)
    if (known.has(id) && parseStaleError(why)) ids.add(id);
  return [...ids];
}
