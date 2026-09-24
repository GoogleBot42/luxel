// Rendering a scene in the browser: the wasm `Compositor` plus one engine per
// pattern or sprite layer, plus the host's half of a text layer (Gitea #480).
//
// WHY A CLASS and not a painter: a scene is stateful in a way a pattern frame
// is not. It owns N engines that must be freed together, a compositor handle,
// the scroll/sprite clocks inside it, and the per-frame text the compositor
// refuses to resolve itself (`text_source(i)` says what it wants,
// `set_text(i, s)` supplies it — docs/spec/scenes.md §2). One owner for all of
// that is what keeps the Scenes grid from leaking wasm handles.
//
// The composite comes out as a plain `Uint8Array` of the layout's pixels, so
// it goes into the SAME painters (`lib/draw.ts`) and the same `lx_outpipe`
// stage a pattern frame does. Nothing downstream knows it was a stack.
//
// Reused by: the Scenes grid, the scene editor's stage, and (#482) the
// playlist row's thumbnail — `lib/sceneThumb.ts` is the one-shot form.

import { Engine, type Luxel } from "./luxel";
import { serializeScene, type ClockFmt, type Scene } from "./scene";
import { compileForLayout, type Layout } from "../stores/geometry";
import { Compositor } from "./luxel";

/** Hands back the SOURCE of a stored pattern or sprite by id, or null when
 *  the browser does not have it (yet). Console: `stores/device.ts`
 *  `devicePatterns`. Playground: the local library, keyed by
 *  `playgroundPatternId`. */
export type SourceLookup = (id: string) => string | null;

/** The text a `slot` source shows. Until `/api/text` lands (#485) there are
 *  no slots, so every slot reads empty and the layer draws nothing — which is
 *  what a device without the endpoint does too. */
export type SlotLookup = (n: number) => string;

export class SceneRenderer {
  private comp: Compositor | null = null;
  private engines: (Engine | null)[] = [];
  /** What each layer wants drawn, recomputed per frame for clock/slot. */
  private textOf: ((slots: SlotLookup) => string)[] = [];
  private wire = "";
  private freed = false;

  constructor(
    private lx: Luxel,
    /** The rig every layer renders through — the device's shape, or a
     *  thumbnail-sized copy of it. MUST be a regular 2D Layout. */
    readonly rig: Layout,
  ) {}

  get width(): number {
    return this.rig.w;
  }
  get height(): number {
    return this.rig.h;
  }

  /** How many pattern layers this scene is actually running (what the frame
   *  cost line counts). */
  get patternEngines(): number {
    return this.engines.filter((e) => e !== null).length;
  }

  /**
   * Point the renderer at `scene`. Returns null, or the compositor's parse
   * error — the SAME `scene: line N: …` string the device would answer with,
   * which is how the editor catches a bad record before any device does.
   *
   * Rebuilding is cheap-ish but not free (one wasm compile per pattern
   * layer), so an unchanged wire block is a no-op: the editor calls this on
   * every keystroke.
   */
  setScene(scene: Scene, lookup: SourceLookup, force = false): string | null {
    const wire = serializeScene(scene);
    if (!force && wire === this.wire && this.comp) return null;
    this.wire = wire;
    this.dropEngines();
    this.comp?.free();
    this.comp = this.lx.compositor(this.rig.w, this.rig.h);
    if (!this.comp) return "this build has no compositor";
    const err = this.comp.setScene(wire);
    if (err) return err;

    this.engines = [];
    this.textOf = [];
    for (const l of scene.layers) {
      let engine: Engine | null = null;
      if (l.body.kind === "pat" || l.body.kind === "sprite") {
        const id = l.body.kind === "pat" ? l.body.pat.id : l.body.id;
        const src = id === "" ? null : lookup(id);
        if (src !== null) {
          // A sprite's engine is BUILT and then only read — the compositor
          // never steps it (docs/spec/scenes.md §4) — so both kinds go
          // through the one compile path.
          const proj = l.body.kind === "pat" ? l.body.pat.proj : null;
          const built = compileForLayout(this.lx, src, 0, proj, this.rig);
          if ("engine" in built) {
            const e = built.engine;
            engine = e;
            if (l.body.kind === "pat") {
              for (const [name, vals] of Object.entries(l.body.pat.controls)) {
                e.setControl(name, vals);
              }
            }
          }
        }
      }
      this.engines.push(engine);
      this.textOf.push(textResolver(l.body.kind === "text" ? l.body.text.source : null));
    }
    this.engines.forEach((e, i) => this.comp?.bind(i, e));
    return null;
  }

  /** One composite frame, or null when there is no scene installed. */
  frame(deltaMs: number, slots: SlotLookup = () => ""): Uint8Array | null {
    if (!this.comp) return null;
    this.textOf.forEach((f, i) => this.comp?.setText(i, f(slots)));
    return this.comp.frame(deltaMs);
  }

  /** The engine behind layer `i`, for the sprite tools (#481) and for
   *  reading a pattern's `controls()` in the inspector. */
  engineAt(i: number): Engine | null {
    return this.engines[i] ?? null;
  }

  private dropEngines(): void {
    for (const e of this.engines) e?.free();
    this.engines = [];
  }

  free(): void {
    if (this.freed) return;
    this.freed = true;
    this.dropEngines();
    this.comp?.free();
    this.comp = null;
  }
}

/** What a text layer draws, as a function of the clock and the slot table. */
function textResolver(
  source: { kind: "lit"; text: string } | { kind: "clock"; fmt: ClockFmt } | { kind: "slot"; slot: number } | null,
): (slots: SlotLookup) => string {
  if (!source) return () => "";
  if (source.kind === "lit") return () => source.text;
  if (source.kind === "slot") return (slots) => slots(source.slot);
  return () => formatClock(source.fmt, new Date());
}

/** `luxel_core::text::format_clock`, browser side. The compositor reads no
 *  wall clock — this is the HOST's half, and the device's own clock does the
 *  same job over there. */
export function formatClock(fmt: ClockFmt, at: Date): string {
  const p2 = (n: number): string => String(n).padStart(2, "0");
  const h24 = at.getHours();
  const h12 = h24 % 12 === 0 ? 12 : h24 % 12;
  switch (fmt) {
    case "HH:MM":
      return `${p2(h24)}:${p2(at.getMinutes())}`;
    case "HH:MM:SS":
      return `${p2(h24)}:${p2(at.getMinutes())}:${p2(at.getSeconds())}`;
    case "hh:MM":
      return `${h12}:${p2(at.getMinutes())}`;
    case "hh:MM:SS":
      return `${h12}:${p2(at.getMinutes())}:${p2(at.getSeconds())}`;
    case "MM-DD":
      return `${p2(at.getMonth() + 1)}-${p2(at.getDate())}`;
    default:
      return `${at.getFullYear()}-${p2(at.getMonth() + 1)}-${p2(at.getDate())}`;
  }
}

/**
 * The playground's pattern ids. A device assigns 8-hex ids; the browser's own
 * library is keyed by NAME, so a scene made in the playground needs a stable
 * id it can put on the `I` line and resolve again after a reload. FNV-1a over
 * the name gives one, and it is the same 8-hex shape the wire validates.
 */
export function playgroundPatternId(name: string): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < name.length; i++) {
    h ^= name.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, "0");
}
