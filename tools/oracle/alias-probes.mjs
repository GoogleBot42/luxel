// One-shot probes for two late-binding questions (Gitea #108):
//  1. Does setPalette(arr) hold a LIVE reference — do later writes through
//     arr change what paint() renders, with no second setPalette call?
//     (fast-palette-blending's palette manager depends on it.)
//  2. Is a render function assigned to an `export var render` dispatched?
//     (slime-mold-palette switches render2D between functions at runtime.)
// Verdicts come from preview frames: paint colors are chosen so the two
// answers are unambiguous channel-wise.
// Usage (repo root, `nix develop`):  node tools/oracle/alias-probes.mjs <ip>
import { buildCompiler, packBytecode } from "./compiler.mjs";
import { PB, sleep } from "./pb.mjs";

const ip = process.argv[2] ?? "192.168.0.140";
const SENTINEL = 42;

function rgbOfFrame(frame) {
  // previewFrame: [r,g,b] per pixel; average the first 10 pixels
  const n = Math.min(30, frame.length - (frame.length % 3));
  let r = 0, g = 0, b = 0, c = 0;
  for (let i = 0; i + 2 < n; i += 3) {
    r += frame[i]; g += frame[i + 1]; b += frame[i + 2]; c++;
  }
  return [Math.round(r / c), Math.round(g / c), Math.round(b / c)];
}

async function load(pb, compile, source) {
  const compiled = compile(source);
  if (!compiled.ok) throw new Error("compile: " + compiled.error);
  await pb.setCode(packBytecode(compiled));
  for (let a = 0; a < 10; a++) {
    await sleep(250);
    const v = await pb.getVars();
    if (v.sent === SENTINEL) return v;
  }
  throw new Error("sentinel never landed (init aborted?)");
}

async function main() {
  const webUI = await (await fetch(`http://${ip}/`)).text();
  const compile = buildCompiler(webUI);
  const pb = await PB.connect(ip);
  const { seq } = await pb.getConfig();
  const restoreId = seq.activeProgram?.activeProgramId;

  try {
    console.log("=== 1. setPalette live alias ===");
    // Palette starts red; after ~2 s of frames beforeRender rewrites the
    // array IN PLACE to blue. No second setPalette call. Live alias ⇒ the
    // strip turns blue; snapshot ⇒ stays red.
    await load(
      pb,
      compile,
      `export var sent, f
       p = array(8)
       p[0] = 0
       p[1] = 1
       p[2] = 0
       p[3] = 0
       p[4] = 1
       p[5] = 1
       p[6] = 0
       p[7] = 0
       setPalette(p)
       f = 0
       export function beforeRender(delta) {
         f = f + 1
         if (f > 100) {
           p[1] = 0
           p[2] = 0
           p[3] = 1
           p[5] = 0
           p[6] = 0
           p[7] = 1
         }
       }
       export function render(index) { paint(0.5, 1) }
       sent = ${SENTINEL}`,
    );
    const early = rgbOfFrame(await pb.getPreviewFrame());
    await sleep(4000);
    const late = rgbOfFrame(await pb.getPreviewFrame());
    console.log(`  early rgb=${early}  late rgb=${late}`);
    const live = late[2] > 128 && late[0] < 64 && early[0] > 128;
    console.log(`  verdict: setPalette ${live ? "LIVE-ALIASES the array" : early[0] > 128 ? "SNAPSHOTS (stayed red)" : "UNCLEAR"}`);

    console.log("=== 2. render via export var ===");
    // No static render function at all: an exported var is assigned one of
    // two painters, swapped every 100 frames. Dispatch-through-var ⇒ the
    // strip alternates red/green; ignored ⇒ black (or an error).
    await load(
      pb,
      compile,
      `export var sent, f, render
       function paintRed(index) { rgb(1, 0, 0) }
       function paintGreen(index) { rgb(0, 1, 0) }
       render = paintRed
       f = 0
       export function beforeRender(delta) {
         f = f + 1
         if (f % 200 < 100) { render = paintRed } else { render = paintGreen }
       }
       sent = ${SENTINEL}`,
    );
    const a = rgbOfFrame(await pb.getPreviewFrame());
    let flipped = false;
    let b = a;
    for (let i = 0; i < 12 && !flipped; i++) {
      await sleep(700);
      b = rgbOfFrame(await pb.getPreviewFrame());
      if ((a[0] > 128) !== (b[0] > 128)) flipped = true;
    }
    console.log(`  first rgb=${a}  later rgb=${b}`);
    const lit = a[0] > 128 || a[1] > 128 || b[0] > 128 || b[1] > 128;
    console.log(
      `  verdict: export-var render ${lit ? (flipped ? "DISPATCHED + LIVE-SWAPPED" : "DISPATCHED (no swap seen)") : "NOT DISPATCHED (dark)"}`,
    );
  } finally {
    if (restoreId) await pb.setActivePattern(restoreId).catch(() => {});
    await pb.close();
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
