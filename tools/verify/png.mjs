// Minimal dependency-free PNG encoder for the render-verification harness.
// 8-bit RGB (color type 2), one zlib IDAT, filter type 0 on every scanline —
// deliberately dumb so the bytes are deterministic across runs and hosts
// (the judge harness compares images byte-for-byte to prove determinism).
//
// Usage: import { encodePNG } from "./png.mjs";
//        fs.writeFileSync("x.png", encodePNG(w, h, rgbBuffer));
//        `rgbBuffer` is w*h*3 bytes, row-major, R,G,B per pixel.
//
// Also carries a 3x5 bitmap font (digits, '.', 's') and `drawText`, used to
// stamp per-cell timestamps into contact sheets AFTER upscale so the glyphs
// stay 1:1 with output pixels and stay legible.

import zlib from "node:zlib";

const CRC_TABLE = (() => {
  const t = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c;
  }
  return t;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const out = Buffer.alloc(12 + data.length);
  out.writeUInt32BE(data.length, 0);
  out.write(type, 4, "ascii");
  data.copy(out, 8);
  out.writeUInt32BE(crc32(out.subarray(4, 8 + data.length)), 8 + data.length);
  return out;
}

/** Encode w×h RGB bytes as a PNG buffer. */
export function encodePNG(width, height, rgb) {
  if (rgb.length < width * height * 3) {
    throw new Error(`encodePNG: need ${width * height * 3} bytes, got ${rgb.length}`);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 2; // color type: truecolor RGB
  ihdr[10] = 0; // deflate
  ihdr[11] = 0; // adaptive filtering
  ihdr[12] = 0; // no interlace

  const src = Buffer.isBuffer(rgb) ? rgb : Buffer.from(rgb.buffer, rgb.byteOffset, rgb.length);
  const stride = width * 3;
  const raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y++) {
    raw[y * (stride + 1)] = 0; // filter: none
    src.copy(raw, y * (stride + 1) + 1, y * stride, y * stride + stride);
  }

  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", zlib.deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

/** Nearest-neighbour integer upscale of a w×h RGB buffer. */
export function upscale(width, height, rgb, sx, sy) {
  const ow = width * sx;
  const oh = height * sy;
  const out = Buffer.alloc(ow * oh * 3);
  for (let y = 0; y < oh; y++) {
    const srcRow = ((y / sy) | 0) * width * 3;
    const dstRow = y * ow * 3;
    for (let x = 0; x < ow; x++) {
      const s = srcRow + ((x / sx) | 0) * 3;
      const d = dstRow + x * 3;
      out[d] = rgb[s];
      out[d + 1] = rgb[s + 1];
      out[d + 2] = rgb[s + 2];
    }
  }
  return { width: ow, height: oh, rgb: out };
}

// ---- 3x5 bitmap font -------------------------------------------------------

const GLYPH_W = 3;
const GLYPH_H = 5;
const GLYPH_GAP = 1;

/** Rows of "1"/"0", top to bottom. Only what timestamps need, plus a fallback. */
const FONT3X5 = {
  0: ["111", "101", "101", "101", "111"],
  1: ["010", "110", "010", "010", "111"],
  2: ["111", "001", "111", "100", "111"],
  3: ["111", "001", "111", "001", "111"],
  4: ["101", "101", "111", "001", "001"],
  5: ["111", "100", "111", "001", "111"],
  6: ["111", "100", "111", "101", "111"],
  7: ["111", "001", "001", "001", "001"],
  8: ["111", "101", "111", "101", "111"],
  9: ["111", "101", "111", "001", "111"],
  ".": ["000", "000", "000", "000", "010"],
  // lowercase: one row shorter than the digits, so "3.0s" can't read as "3.05"
  s: ["000", "111", "110", "011", "111"],
  "?": ["111", "001", "010", "000", "010"],
};

/** Pixel footprint of `text` in the 3x5 font (excludes the backing box). */
export function textSize(text) {
  const n = String(text).length;
  return { width: n ? n * GLYPH_W + (n - 1) * GLYPH_GAP : 0, height: GLYPH_H };
}

function putPixel(rgb, width, height, x, y, c) {
  if (x < 0 || y < 0 || x >= width || y >= height) return;
  const i = (y * width + x) * 3;
  rgb[i] = c[0];
  rgb[i + 1] = c[1];
  rgb[i + 2] = c[2];
}

/** Stamp `text` into a w×h RGB buffer with its top-left glyph pixel at (x, y).
 *  `bg` (default black) fills a 1px box behind the text so it survives any
 *  background; pass null to draw the glyphs alone. Clipped, never throws. */
export function drawText(rgb, width, height, x, y, text, color = [128, 128, 128], bg = [0, 0, 0]) {
  const str = String(text);
  const { width: tw, height: th } = textSize(str);
  if (!tw) return;
  if (bg) {
    for (let yy = y - 1; yy <= y + th; yy++) {
      for (let xx = x - 1; xx <= x + tw; xx++) putPixel(rgb, width, height, xx, yy, bg);
    }
  }
  let cx = x;
  for (const ch of str) {
    const g = FONT3X5[ch] ?? FONT3X5["?"];
    for (let r = 0; r < GLYPH_H; r++) {
      for (let c = 0; c < GLYPH_W; c++) {
        if (g[r][c] === "1") putPixel(rgb, width, height, cx + c, y + r, color);
      }
    }
    cx += GLYPH_W + GLYPH_GAP;
  }
}
