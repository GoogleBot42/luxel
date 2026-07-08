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

import { createHash } from "node:crypto";
import { gzipSync } from "node:zlib";
import fs from "node:fs";
import path from "node:path";

const OUT = process.argv[2] ?? "dist.luxa";
const DIST = "dist";

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "application/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".wasm": "application/wasm",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".ico": "image/x-icon",
};

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
  const gz = gzipSync(raw, { level: 9 });
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
