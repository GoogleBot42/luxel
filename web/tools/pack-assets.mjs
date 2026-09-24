// Pack the built playground (dist/) into a LUXA archive for the device's
// assets flash region. Usage (from web/): npm run build && node
// tools/pack-assets.mjs [outfile]
//
// Layout (little-endian), format v2 ("LUX2"):
//   "LUX2" u32 count { u8 path_len, path, u8 ctype_len, ctype,
//                      u8 gzip, u32 len, u32 offset, u8[8] etag } … blobs
// Offsets are relative to the archive start. All text/wasm is gzipped
// (served with Content-Encoding: gzip). `etag` is the first 8 bytes of
// SHA-256 over the *raw* (pre-gzip) file — the device serves it as a strong
// ETag so browsers can revalidate with If-None-Match and get 304s.
// The firmware still reads legacy "LUXA" archives (no etag field).
//
// The gzip streams come from ZOPFLI when it is on PATH (the devshell has
// it): a plain DEFLATE encoder that searches harder than zlib, so it is the
// same format, the same `Content-Encoding: gzip` and no firmware change, for
// ~4.5 % fewer bytes in a bundle gated at the 983,040 B assets partition
// (Gitea #683). Without it we fall back to zlib level 9 with a warning — a
// bare checkout still packs a valid archive, and the fallback is the LOOSE
// direction, so a bundle that fits when packed here fits when CI packs it.
// Brotli and zstd are NOT options however much they would help: a browser
// only advertises `Accept-Encoding: br`/`zstd` on a secure origin, and the
// device is plain http on a LAN IP.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import os from "node:os";
import { gunzipSync, gzipSync } from "node:zlib";
import fs from "node:fs";
import path from "node:path";

const OUT = process.argv[2] ?? "dist.luxa";
const DIST = "dist";

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "application/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".wasm": "application/wasm",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".ico": "image/x-icon",
};

// zopfli has no stdin mode, so each blob goes through a temp file.
const haveZopfli = (() => {
  try {
    execFileSync("zopfli", ["-h"], { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
})();
if (!haveZopfli) {
  console.warn(
    "pack-assets: zopfli not on PATH — falling back to zlib level 9 " +
      "(~4.5 % larger). Run inside `nix develop` for the shipping bytes.",
  );
}

const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), "luxa-"));
process.on("exit", () => fs.rmSync(tmpDir, { recursive: true, force: true }));
function deflate(raw, name) {
  if (!haveZopfli) return gzipSync(raw, { level: 9 });
  // --i15 is the default iteration count; --i50 was measured worth 2 bytes
  // on the 1.2 MB gallery for 3x the wall clock.
  const src = path.join(tmpDir, name.replaceAll("/", "_"));
  fs.writeFileSync(src, raw);
  try {
    const gz = execFileSync("zopfli", ["-c", src], { maxBuffer: 1 << 28 });
    // An external compressor is a new way for this archive to be silently
    // wrong, and the device is where that would be discovered. Round-trip
    // every blob here instead: ~0.1 s for the whole bundle.
    if (!gunzipSync(gz).equals(raw)) {
      throw new Error(`zopfli round-trip mismatch for ${name}`);
    }
    return gz;
  } finally {
    fs.rmSync(src, { force: true });
  }
}

function* walk(dir) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) yield* walk(p);
    else yield p;
  }
}

const files = [];
for (const f of walk(DIST)) {
  const rel = "/" + path.relative(DIST, f).replaceAll("\\", "/");
  const ext = path.extname(f);
  const ctype = TYPES[ext] ?? "application/octet-stream";
  const raw = fs.readFileSync(f);
  const gz = deflate(raw, rel);
  const useGz = gz.length < raw.length;
  const etag = createHash("sha256").update(raw).digest().subarray(0, 8); // 8-byte content id
  files.push({ path: rel, ctype, gzip: useGz, etag, data: useGz ? gz : raw });
}

// header size first so blob offsets are known (meta = 9 + 8-byte etag)
let headerSize = 8;
for (const f of files) {
  headerSize += 1 + Buffer.byteLength(f.path) + 1 + Buffer.byteLength(f.ctype) + 17;
}
let offset = headerSize;
const parts = [Buffer.from("LUX2"), u32(files.length)];
function u32(n) {
  const b = Buffer.alloc(4);
  b.writeUInt32LE(n);
  return b;
}
for (const f of files) {
  parts.push(Buffer.from([Buffer.byteLength(f.path)]), Buffer.from(f.path));
  parts.push(Buffer.from([Buffer.byteLength(f.ctype)]), Buffer.from(f.ctype));
  parts.push(Buffer.from([f.gzip ? 1 : 0]), u32(f.data.length), u32(offset), f.etag);
  offset += f.data.length;
}
for (const f of files) parts.push(f.data);

const out = Buffer.concat(parts);
fs.writeFileSync(OUT, out);
console.log(
  `${OUT}: ${files.length} files, ${out.length} bytes\n` +
    files.map((f) => `  ${f.path} ${f.data.length}${f.gzip ? " gz" : ""} ${f.ctype}`).join("\n"),
);
