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
// bench and CI cannot drift. It digests with `crypto.subtle` where that
// exists and with its own SHA-256 where it does not — a device's own console
// is a plain-http origin, which is not a secure context, so WebCrypto is
// simply absent there (Gitea #794, found on the Athom).

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

/** SHA-256 round constants. */
// prettier-ignore
const SHA_K = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);

/**
 * SHA-256 in plain TypeScript.
 *
 * `crypto.subtle` does not exist outside a **secure context**, and a device's
 * own console is served from `http://<lan-ip>/` — plain http, not
 * `localhost` — so on the one host that matters most there is no WebCrypto to
 * call. Every harness runs on `127.0.0.1` (a secure context) and so never saw
 * it: the flow died on the Athom with `Cannot read properties of undefined
 * (reading 'digest')` the first time it met real hardware (Gitea #794).
 *
 * Dropping the verification there is not an option — what follows these
 * hashes is written into an OTA slot — so the codec carries its own digest.
 * ~2 MB (a whole package) takes a few tens of milliseconds, which is nothing
 * beside the upload that follows.
 */
export function sha256Js(b: Uint8Array): Uint8Array {
  const len = b.length;
  // one 0x80 byte, then zeros, then a 64-bit big-endian bit count
  const total = len + 9 + ((64 - ((len + 9) % 64)) % 64);
  const msg = new Uint8Array(total);
  msg.set(b);
  msg[len] = 0x80;
  const dv = new DataView(msg.buffer);
  // len * 8 stays exact as a double well past any package size; >>> 0 takes
  // the low word and the divide takes the high one.
  dv.setUint32(total - 8, Math.floor(len / 0x20000000), false);
  dv.setUint32(total - 4, (len * 8) >>> 0, false);

  const h = new Uint32Array([
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
  ]);
  const w = new Uint32Array(64);
  const rotr = (x: number, n: number) => ((x >>> n) | (x << (32 - n))) >>> 0;

  for (let off = 0; off < total; off += 64) {
    for (let i = 0; i < 16; i++) w[i] = dv.getUint32(off + i * 4, false);
    for (let i = 16; i < 64; i++) {
      const x = w[i - 15]!;
      const y = w[i - 2]!;
      const s0 = (rotr(x, 7) ^ rotr(x, 18) ^ (x >>> 3)) >>> 0;
      const s1 = (rotr(y, 17) ^ rotr(y, 19) ^ (y >>> 10)) >>> 0;
      w[i] = (w[i - 16]! + s0 + w[i - 7]! + s1) >>> 0;
    }
    let a = h[0]!;
    let b2 = h[1]!;
    let c = h[2]!;
    let d = h[3]!;
    let e = h[4]!;
    let f = h[5]!;
    let g = h[6]!;
    let hh = h[7]!;
    for (let i = 0; i < 64; i++) {
      const S1 = (rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25)) >>> 0;
      const ch = ((e & f) ^ (~e & g)) >>> 0;
      const t1 = (hh + S1 + ch + SHA_K[i]! + w[i]!) >>> 0;
      const S0 = (rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22)) >>> 0;
      const maj = ((a & b2) ^ (a & c) ^ (b2 & c)) >>> 0;
      const t2 = (S0 + maj) >>> 0;
      hh = g;
      g = f;
      f = e;
      e = (d + t1) >>> 0;
      d = c;
      c = b2;
      b2 = a;
      a = (t1 + t2) >>> 0;
    }
    h[0] = (h[0]! + a) >>> 0;
    h[1] = (h[1]! + b2) >>> 0;
    h[2] = (h[2]! + c) >>> 0;
    h[3] = (h[3]! + d) >>> 0;
    h[4] = (h[4]! + e) >>> 0;
    h[5] = (h[5]! + f) >>> 0;
    h[6] = (h[6]! + g) >>> 0;
    h[7] = (h[7]! + hh) >>> 0;
  }

  const out = new Uint8Array(32);
  const odv = new DataView(out.buffer);
  for (let i = 0; i < 8; i++) odv.setUint32(i * 4, h[i]!, false);
  return out;
}

async function sha256(b: Uint8Array): Promise<Uint8Array> {
  // WebCrypto where it exists (node, and any secure-context browser); the
  // in-house digest on a device's own plain-http console, where
  // `crypto.subtle` is undefined (Gitea #794). Same bytes either way —
  // tests/luxr.test.mjs pins the two against each other.
  const subtle = globalThis.crypto?.subtle;
  if (!subtle) return sha256Js(b);
  // `b.buffer` may be a view into a larger buffer (a subarray of the file),
  // so slice to exactly these bytes before digesting.
  const d = await subtle.digest("SHA-256", b.slice().buffer);
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
