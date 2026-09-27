// Decoding an image FILE into frames of RGBA — the browser half of the sprite
// importer (Gitea #784). The conversion itself is `lib/imageImport.ts`, which
// is pure and knows nothing about files or canvases; this module exists so
// that split can hold.
//
// Two paths, and the difference is animation:
//
//   * `ImageDecoder` (WebCodecs) gives EVERY frame of an animated GIF or WebP
//     with its delay. Chromium has it, which is the browser the console is
//     verified in; Firefox and Safari do not, as of 2026.
//   * `createImageBitmap` is everywhere and gives ONE frame — the first. That
//     is the documented fallback (#784), not a failure: a still import of an
//     animation is a usable sprite.
//
// Everything the browser can decode is accepted (PNG, GIF, JPEG, WebP, BMP);
// the type is SNIFFED from the bytes rather than trusted from `file.type`,
// because a drag-and-drop from some file managers arrives with an empty type
// and a `.png` renamed to `.bin` arrives with a wrong one.

import type { SourceFrame, SourceImage } from "./imageImport.ts";

/** What the file input offers. `image/*` is last so a browser that knows a
 *  type we did not list still shows it. */
export const IMPORT_ACCEPT = "image/png,image/gif,image/jpeg,image/webp,image/bmp,image/*";

/** The one refusal sentence, so the dialog and the drop target agree. */
export const NOT_AN_IMAGE =
  "that is not an image this browser can decode — PNG, GIF, JPEG, WebP or BMP";

/** Source frames read at most. 255 is the record's ceiling and `keepEvery`
 *  works off the source count, so a 2,000-frame GIF only needs enough frames
 *  to sample from — and decoding all of them costs seconds. */
export const MAX_SOURCE_FRAMES = 512;

/** A decoded file: the frames, plus what the UI says about where they came
 *  from. */
export interface DecodedImage extends SourceImage {
  /** The file's own name, extension and all — what the dialog shows. The
   *  RECORD's name is `spriteName(name)`, which strips it. */
  name: string;
  /** The sniffed media type, e.g. `image/gif`. */
  type: string;
  /** More than one frame came back. */
  animated: boolean;
  /** Frames past [`MAX_SOURCE_FRAMES`] that were not read. */
  sourceTruncated: number;
  /** True when the file is animated but only its first frame could be read
   *  (no `ImageDecoder` in this browser). */
  firstFrameOnly: boolean;
}

/** The media types whose files can hold more than one frame. */
const MULTIFRAME = new Set(["image/gif", "image/webp", "image/apng", "image/png"]);

/**
 * `file` as frames of RGBA. Throws an `Error` carrying [`NOT_AN_IMAGE`] for
 * anything that will not decode — including a text file dropped on the
 * Sprites page, which is the case that must not look like a crash.
 */
export async function decodeImageFile(file: File | Blob, fileName = ""): Promise<DecodedImage> {
  const name = fileName !== "" ? fileName : file instanceof File ? file.name : "";
  const bytes = new Uint8Array(await file.arrayBuffer());
  const type = sniffType(bytes) ?? (file.type === "" ? "" : file.type);
  if (type === "" || !type.startsWith("image/")) throw new Error(NOT_AN_IMAGE);

  if (MULTIFRAME.has(type)) {
    const got = await decodeAnimated(bytes, type);
    if (got !== null && got.frames.length > 0) {
      return { ...got, name, type, animated: got.frames.length > 1, firstFrameOnly: false };
    }
  }
  const still = await decodeStill(file);
  return {
    ...still,
    name,
    type,
    animated: false,
    sourceTruncated: 0,
    // An animation we could only read one frame of — the dialog says so.
    firstFrameOnly: MULTIFRAME.has(type) && !hasImageDecoder(),
  };
}

/** The media type the first bytes say this is, or null. */
export function sniffType(b: Uint8Array): string | null {
  const at = (i: number): number => b[i] ?? 0;
  if (b.length >= 8 && at(0) === 0x89 && at(1) === 0x50 && at(2) === 0x4e && at(3) === 0x47) {
    return "image/png";
  }
  if (b.length >= 6 && at(0) === 0x47 && at(1) === 0x49 && at(2) === 0x46) return "image/gif";
  if (b.length >= 3 && at(0) === 0xff && at(1) === 0xd8 && at(2) === 0xff) return "image/jpeg";
  if (
    b.length >= 12 &&
    at(0) === 0x52 &&
    at(1) === 0x49 &&
    at(2) === 0x46 &&
    at(3) === 0x46 &&
    at(8) === 0x57 &&
    at(9) === 0x45 &&
    at(10) === 0x42 &&
    at(11) === 0x50
  ) {
    return "image/webp";
  }
  if (b.length >= 2 && at(0) === 0x42 && at(1) === 0x4d) return "image/bmp";
  return null;
}

// ---- WebCodecs: every frame ----
//
// `ImageDecoder` is not in TypeScript's DOM library, so the shape it is used
// through is declared here rather than cast away at each call.

interface DecodedFrame {
  displayWidth: number;
  displayHeight: number;
  /** Frame duration in MICROseconds, or null when the container omits it. */
  duration: number | null;
  close(): void;
}

interface DecoderTrack {
  frameCount: number;
  animated: boolean;
}

interface DecoderLike {
  tracks: { ready: Promise<void>; selectedTrack: DecoderTrack | null };
  completed: Promise<void>;
  decode(opts: { frameIndex: number }): Promise<{ image: DecodedFrame }>;
  close(): void;
}

type DecoderCtor = new (opts: { data: Uint8Array; type: string }) => DecoderLike;

function decoderCtor(): DecoderCtor | null {
  const g = globalThis as unknown as { ImageDecoder?: DecoderCtor };
  return typeof g.ImageDecoder === "function" ? g.ImageDecoder : null;
}

/** Does this browser have the every-frame path at all? */
export function hasImageDecoder(): boolean {
  return decoderCtor() !== null;
}

async function decodeAnimated(
  bytes: Uint8Array,
  type: string,
): Promise<{ w: number; h: number; frames: SourceFrame[]; sourceTruncated: number } | null> {
  const Ctor = decoderCtor();
  if (Ctor === null) return null;
  let dec: DecoderLike | null = null;
  try {
    dec = new Ctor({ data: bytes, type });
    await dec.tracks.ready;
    // `frameCount` only settles once the whole buffer has been read — and it
    // has, because we handed over the entire file rather than a stream.
    await dec.completed;
    const track = dec.tracks.selectedTrack;
    const total = Math.max(1, track === null ? 1 : track.frameCount);
    const take = Math.min(total, MAX_SOURCE_FRAMES);
    const frames: SourceFrame[] = [];
    let w = 0;
    let h = 0;
    let canvas: Plate | null = null;
    for (let i = 0; i < take; i++) {
      const { image } = await dec.decode({ frameIndex: i });
      try {
        if (canvas === null) {
          w = image.displayWidth;
          h = image.displayHeight;
          canvas = plate(w, h);
        }
        canvas.ctx.clearRect(0, 0, w, h);
        canvas.ctx.drawImage(image as unknown as CanvasImageSource, 0, 0);
        frames.push({
          rgba: new Uint8Array(canvas.ctx.getImageData(0, 0, w, h).data),
          // microseconds on the wire, milliseconds in the record
          delayMs: image.duration === null ? 0 : Math.round(image.duration / 1000),
        });
      } finally {
        image.close();
      }
    }
    if (frames.length === 0) return null;
    return { w, h, frames, sourceTruncated: total - take };
  } catch {
    // A still PNG goes down this path too (it is a MULTIFRAME type), and a
    // browser whose decoder dislikes the file still has `createImageBitmap`.
    return null;
  } finally {
    dec?.close();
  }
}

// ---- the everywhere path: one frame ----

async function decodeStill(file: File | Blob): Promise<SourceImage> {
  let bmp: ImageBitmap;
  try {
    bmp = await createImageBitmap(file);
  } catch {
    throw new Error(NOT_AN_IMAGE);
  }
  try {
    const w = bmp.width;
    const h = bmp.height;
    if (w < 1 || h < 1) throw new Error(NOT_AN_IMAGE);
    const p = plate(w, h);
    p.ctx.drawImage(bmp, 0, 0);
    return { w, h, frames: [{ rgba: new Uint8Array(p.ctx.getImageData(0, 0, w, h).data), delayMs: 0 }] };
  } finally {
    bmp.close();
  }
}

interface Plate {
  ctx: CanvasRenderingContext2D;
}

/** A read-back canvas. `willReadFrequently` is the point: every frame is
 *  `getImageData`'d immediately, and without it Chromium keeps the surface on
 *  the GPU and pays a readback per frame. */
function plate(w: number, h: number): Plate {
  const el = document.createElement("canvas");
  el.width = w;
  el.height = h;
  const ctx = el.getContext("2d", { willReadFrequently: true });
  if (ctx === null) throw new Error("this browser gave no 2D canvas to decode into");
  return { ctx };
}
