// Dump (and optionally restore) a Pixel Blaze's installed pixel map by
// reconstructing it from INSIDE the pattern language: live-code a pattern
// that exports every pixel's render2D/render3D coordinates, read them via
// getVars. Lossless — PB map coords are u16-quantized, and re-saving the
// dumped JSON round-trips bit-exactly (verified against the oracle).
//
//   node tools/oracle/mapdump.mjs <ip>                 dump → stdout (JSON)
//   node tools/oracle/mapdump.mjs <ip> --restore m.json  save a dumped map back
//
// Restore path: POST /edit (form-file /pixelmap.txt) + ws {savePixelMap}.
// Nothing else is modified; the active pattern is restored; the websocket
// closes cleanly (the device wedges otherwise — see 04-oracle-findings.md).
import fs from "node:fs";
import { buildCompiler, packBytecode } from "./compiler.mjs";
import { PB, sleep } from "./pb.mjs";

const ip = process.argv[2];
if (!ip) {
  console.error("usage: mapdump.mjs <ip> [--restore map.json]");
  process.exit(2);
}
const restoreArg = process.argv[3] === "--restore" ? process.argv[4] : null;

const DUMP = `
export var xs = array(pixelCount)
export var ys = array(pixelCount)
export var zs = array(pixelCount)
export var mode = 0
export function render2D(i, x, y) { mode = 2; xs[i] = x; ys[i] = y; hsv(0,0,0) }
export function render3D(i, x, y, z) { mode = 3; xs[i] = x; ys[i] = y; zs[i] = z; hsv(0,0,0) }
`;

const webUI = await (await fetch(`http://${ip}/`)).text();
const compile = buildCompiler(webUI);
const pb = await PB.connect(ip);
try {
  const { settings, seq } = await pb.getConfig();
  const restoreId = seq?.activeProgram?.activeProgramId;
  const n = settings?.pixelCount;

  if (restoreArg) {
    const saved = JSON.parse(fs.readFileSync(restoreArg, "utf8"));
    const form = new FormData();
    form.append("data", new Blob([JSON.stringify(saved.coords)]), "/pixelmap.txt");
    const r = await fetch(`http://${ip}/edit`, { method: "POST", body: form });
    if (!r.ok) throw new Error(`POST /edit failed: ${r.status}`);
    pb.send({ savePixelMap: true });
    await sleep(1500);
    console.error(`restored ${saved.coords.length}-pixel ${saved.mode}D map`);
  } else {
    const c = compile(DUMP);
    if (!c.ok) throw new Error("PB compile failed: " + c.error);
    await pb.setCode(packBytecode(c));
    let v = {};
    for (let i = 0; i < 10 && v.mode === undefined; i++) {
      await sleep(350);
      v = await pb.getVars();
    }
    const { xs, ys, zs, mode } = v;
    if (!(mode === 2 || mode === 3) || xs?.length !== n) {
      console.error("no 2D/3D map coordinates arrived (mapless device, or render never ran)");
      process.exit(1);
    }
    const coords = xs.map((x, i) => (mode === 3 ? [x, ys[i], zs[i]] : [x, ys[i]]));
    console.log(JSON.stringify({ ip, pixelCount: n, mode, coords }));
  }

  if (restoreId) await pb.setActivePattern(restoreId);
} finally {
  await pb.close();
}
