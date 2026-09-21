// The `.luxr` release package: one file carrying the app image AND the web
// assets archive that belong with it (Gitea #643).
//
// Why it exists: firmware and the on-device console are versioned together
// but shipped separately, so an OTA can leave a device running an engine its
// own bundled console cannot compile for. On the Athom (2026-09-20) that
// meant every stored pattern reading "bytecode format v5 (this build reads
// v6)" and a dark strip, with no way out from the device's own UI. A package
// makes "install the firmware" and "install the console that matches it" one
// action that cannot be half-done.
//
// Layout (little-endian). Header is fixed up to the two length-prefixed
// strings, so a reader knows the whole shape from the first 80 bytes:
//
//   off  len  field
//     0    4  magic "LUXR"
//     4    2  container format (u16) — FORMAT below
//     6    1  board-name length in bytes (1..255)
//     7    1  firmware-version length in bytes (1..255)
//     8    4  app image length (u32)
//    12    4  assets archive length (u32; 0 = firmware-only package)
//    16   32  sha256 of the app image
//    48   32  sha256 of the assets archive (zeros when there are none)
//    80    n  board name, UTF-8 — `board::NAME`, the same string
//              tools/ota-push.sh greps an image for and `/api/status.board`
//              reports, so a mismatch is catchable before the OTA slot
//    ..    m  firmware version, UTF-8 ("0.1.46")
//    ..    .  app image bytes
//    ..    .  assets archive bytes (a LUXA/LUX2 archive, web/tools/pack-assets.mjs)
//
// Both hashes are verified on parse: a truncated download is the failure
// mode this format exists to catch, since the payload after it is written
// into an OTA slot.
//
// Shared by the browser (Settings → Firmware & recovery), the packer
// (web/tools/pack-luxr.mjs, used by tools/deploy.sh and the release
// workflow) and the unit test (web/tests/luxr.test.mjs) — one codec, so the
// bench and CI cannot drift. `crypto.subtle` is the common denominator: it
// is in every browser and in node ≥ 18.

/** Magic at byte 0. */
export const LUXR_MAGIC = "LUXR";
/** Container format this build writes and reads. */
export const LUXR_FORMAT = 1;
/** Bytes before the board name. */
const HEADER_FIXED = 80;

export interface LuxrPackage {
  /** `board::NAME`, e.g. `"Athom music-reactive WLED controller"`. */
  board: string;
  /** Firmware version the images were built at, e.g. `"0.1.46"`. */
  version: string;
  /** App-only OTA image — the body of `POST /api/ota`. */
  app: Uint8Array;
  /** LUXA/LUX2 web-asset archive — the body of `POST /api/assets`. Empty
   *  for a firmware-only package (a hosted-UI board serves no assets). */
  assets: Uint8Array;
}

/** A package that is not a package, or one that did not survive the trip. */
export class LuxrError extends Error {}

const enc = new TextEncoder();
const dec = new TextDecoder();

/** True when `buf` starts with the LUXR magic — the cheap "is this a package
 *  or a bare app image?" test a file picker needs before committing to a
 *  parse. */
export function isLuxr(buf: Uint8Array): boolean {
  if (buf.length < 4) return false;
  return dec.decode(buf.subarray(0, 4)) === LUXR_MAGIC;
}

async function sha256(b: Uint8Array): Promise<Uint8Array> {
  // `b.buffer` may be a view into a larger buffer (a subarray of the file),
  // so slice to exactly these bytes before digesting.
  const d = await crypto.subtle.digest("SHA-256", b.slice().buffer);
  return new Uint8Array(d);
}

function sameBytes(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false;
  return true;
}

/** Build a `.luxr`. Throws on a board/version that does not fit its length
 *  byte — both are short identifiers, so that is a programming error. */
export async function packLuxr(p: LuxrPackage): Promise<Uint8Array> {
  const board = enc.encode(p.board);
  const version = enc.encode(p.version);
  if (board.length < 1 || board.length > 255)
    throw new LuxrError(`board name must be 1..255 bytes, got ${board.length}`);
  if (version.length < 1 || version.length > 255)
    throw new LuxrError(`version must be 1..255 bytes, got ${version.length}`);
  if (p.app.length < 1) throw new LuxrError("a package needs an app image");

  const total = HEADER_FIXED + board.length + version.length + p.app.length + p.assets.length;
  const out = new Uint8Array(total);
  const dv = new DataView(out.buffer);
  out.set(enc.encode(LUXR_MAGIC), 0);
  dv.setUint16(4, LUXR_FORMAT, true);
  out[6] = board.length;
  out[7] = version.length;
  dv.setUint32(8, p.app.length, true);
  dv.setUint32(12, p.assets.length, true);
  out.set(await sha256(p.app), 16);
  if (p.assets.length > 0) out.set(await sha256(p.assets), 48);
  let at = HEADER_FIXED;
  out.set(board, at);
  at += board.length;
  out.set(version, at);
  at += version.length;
  out.set(p.app, at);
  at += p.app.length;
  out.set(p.assets, at);
  return out;
}

/** Read a `.luxr`, verifying both payload hashes. Throws [LuxrError] with a
 *  message written for the person holding the file. */
export async function parseLuxr(buf: Uint8Array): Promise<LuxrPackage> {
  if (!isLuxr(buf))
    throw new LuxrError("not a Luxel release package (no LUXR header)");
  if (buf.length < HEADER_FIXED) throw new LuxrError("package header is truncated");
  const dv = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  const format = dv.getUint16(4, true);
  if (format !== LUXR_FORMAT)
    throw new LuxrError(
      `package format v${format}, this console reads v${LUXR_FORMAT} — use a matching release`,
    );
  const boardLen = dv.getUint8(6);
  const versionLen = dv.getUint8(7);
  const appLen = dv.getUint32(8, true);
  const assetsLen = dv.getUint32(12, true);
  if (boardLen < 1 || versionLen < 1) throw new LuxrError("package names no board or version");
  const want = HEADER_FIXED + boardLen + versionLen + appLen + assetsLen;
  if (buf.length !== want)
    throw new LuxrError(
      `package is ${buf.length} bytes, its header describes ${want} — truncated or corrupt`,
    );
  let at = HEADER_FIXED;
  const board = dec.decode(buf.subarray(at, at + boardLen));
  at += boardLen;
  const version = dec.decode(buf.subarray(at, at + versionLen));
  at += versionLen;
  const app = buf.subarray(at, at + appLen);
  at += appLen;
  const assets = buf.subarray(at, at + assetsLen);
  if (!sameBytes(await sha256(app), buf.subarray(16, 48)))
    throw new LuxrError("the firmware image in this package failed its checksum");
  if (assetsLen > 0 && !sameBytes(await sha256(assets), buf.subarray(48, 80)))
    throw new LuxrError("the web assets in this package failed their checksum");
  return { board, version, app, assets };
}
