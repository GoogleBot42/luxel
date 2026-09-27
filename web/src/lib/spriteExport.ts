// Sprite → PNG sprite sheet (Gitea #784, the export half): "so a sprite can
// round-trip through an external editor".
//
// The sheet is ONE ROW of frames, left to right — the strip layout Aseprite,
// Piskel and every tile editor read without being told anything. Transparent
// texels (index 0) are alpha 0, so the sheet re-imports with the same
// transparency it left with.
//
// `sheetPixels` is pure and unit-tested; `downloadSpriteSheet` is the browser
// half (a canvas and an `<a download>`) and lives here with it so the layout
// has exactly one definition.

import { renderFrame, type Sprite } from "./sprite.ts";

/** The sheet as RGBA bytes: `frames` frames side by side. */
export interface Sheet {
  w: number;
  h: number;
  rgba: Uint8ClampedArray;
}

/** The sheet for `sprite` — `w·frames` × `h`, frame `f` at x = `f·w`. */
export function sheetPixels(sprite: Sprite): Sheet {
  const sw = sprite.w * sprite.frames;
  const sh = sprite.h;
  const rgba = new Uint8ClampedArray(sw * sh * 4);
  for (let f = 0; f < sprite.frames; f++) {
    const one = renderFrame(sprite, f);
    for (let row = 0; row < sprite.h; row++) {
      for (let col = 0; col < sprite.w; col++) {
        const from = (row * sprite.w + col) * 4;
        const to = (row * sw + f * sprite.w + col) * 4;
        rgba[to] = one[from] ?? 0;
        rgba[to + 1] = one[from + 1] ?? 0;
        rgba[to + 2] = one[from + 2] ?? 0;
        rgba[to + 3] = one[from + 3] ?? 0;
      }
    }
  }
  return { w: sw, h: sh, rgba };
}

/** `Heart-9x8x2.png` — the shape says how to cut the sheet back up. */
export function sheetFileName(sprite: Sprite): string {
  const base = sprite.name.replace(/[^A-Za-z0-9 _-]+/g, "").trim().replace(/\s+/g, "-");
  const stem = base === "" ? "sprite" : base;
  return `${stem}-${sprite.w}x${sprite.h}x${sprite.frames}.png`;
}

/** Hand the sheet to the browser as a download. Returns the file name so the
 *  caller can say what it saved. */
export async function downloadSpriteSheet(sprite: Sprite): Promise<string> {
  const sheet = sheetPixels(sprite);
  const canvas = document.createElement("canvas");
  canvas.width = sheet.w;
  canvas.height = sheet.h;
  const ctx = canvas.getContext("2d");
  if (ctx === null) throw new Error("this browser gave no 2D canvas to draw into");
  const img = ctx.createImageData(sheet.w, sheet.h);
  img.data.set(sheet.rgba);
  ctx.putImageData(img, 0, 0);
  const blob = await new Promise<Blob | null>((done) => canvas.toBlob((b) => done(b), "image/png"));
  if (blob === null) throw new Error("this browser could not encode a PNG");
  const name = sheetFileName(sprite);
  const url = URL.createObjectURL(blob);
  try {
    const a = document.createElement("a");
    a.href = url;
    a.download = name;
    a.rel = "noopener";
    document.body.append(a);
    a.click();
    a.remove();
  } finally {
    // the click has already started the save; revoking on the next turn is
    // what every download helper does
    setTimeout(() => URL.revokeObjectURL(url), 10_000);
  }
  return name;
}
