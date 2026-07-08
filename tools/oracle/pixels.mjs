// Pixel-level differential oracle: render deterministic per-pixel test
// cases on the real Pixel Blaze (captured via its previewFrame stream) and
// on the Luxel engine (`luxel pixels`), and diff the RGB bytes. One pixel
// per case, so a whole battery ships as a single live-coded pattern.
//
// Usage (repo root, `nix develop`):
//   cargo build --release -p luxel-cli
//   node tools/oracle/pixels.mjs <ip>
//
// Only time-independent code belongs in a case (frames are sampled at an
// arbitrary moment / rendered with delta 0).

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { buildCompiler, packBytecode } from "./compiler.mjs";
import { PB } from "./pb.mjs";

const ip = process.argv[2] ?? "192.168.0.140";
const LUXEL = "target/release/luxel";

// ---- battery 1: rgb / hsv rounding, clamping, wrapping ----
const CASES = [
  { name: "rgb_red", code: "rgb(1, 0, 0)" },
  { name: "rgb_half", code: "rgb(0.5, 0.5, 0.5)" }, // 127.5 rounds…?
  { name: "rgb_quarters", code: "rgb(0.25, 0.75, 0.125)" },
  { name: "rgb_clamp_hi", code: "rgb(1.5, 2, 100)" },
  { name: "rgb_clamp_neg", code: "rgb(-0.5, -1, 0)" },
  { name: "rgb_epsilon", code: "rgb(0.0000152587890625, 0, 0)" }, // 1 raw
  { name: "rgb_under1", code: "rgb(0.9999847412109375, 0, 0)" }, // 1-ε
  { name: "rgb_out_third", code: "rgb(1/3, 2/3, 1/6)" },
  { name: "hsv_red", code: "hsv(0, 1, 1)" },
  { name: "hsv_third", code: "hsv(1/3, 1, 1)" },
  { name: "hsv_sixth", code: "hsv(1/6, 1, 1)" },
  { name: "hsv_wrap", code: "hsv(1.25, 1, 1)" },
  { name: "hsv_negwrap", code: "hsv(-0.25, 1, 1)" },
  { name: "hsv_sat_clamp", code: "hsv(0, 1.5, 1)" },
  { name: "hsv_val_clamp", code: "hsv(0, 1, 2)" },
  { name: "hsv_val_neg", code: "hsv(0, 1, -1)" },
  { name: "hsv_gray", code: "hsv(0.5, 0, 0.5)" },
  { name: "hsv_dim", code: "hsv(0.1, 0.5, 0.25)" },
  { name: "hsv_precise", code: "hsv(0.123456, 0.789, 0.456)" },
  { name: "hsv24_precise", code: "hsv24(0.123456, 0.789, 0.456)" },
  { name: "untouched", code: "" }, // never set → what does a pixel default to?
];

// ---- battery 2: palette semantics (paint index clamp vs wrap) ----
const PAINT_VALS = [
  0, 0.25, 0.5, 0.75, 1, 1.25, -0.25, 2, -1, 0.999,
  // wrap-vs-clamp fine print: just-past-1, whole numbers, deep negatives
  1.0000152587890625, 1.5, 1.75, 1.999, 2.25, 2.5, 3, -0.5, -1.25, -2,
];
const PALETTE_SRC = `
var pal = [0, 0,0,1,  0.5, 0,1,0,  1, 1,0,0]
var vals = [${PAINT_VALS.join(", ")}]
export function beforeRender(delta) { setPalette(pal) }
export function render(index) {
  if (index < ${PAINT_VALS.length}) paint(vals[index])
  else rgb(0, 0, 0)
}
`;

function batterySource(cases) {
  let src = "export function render(index) {\n";
  cases.forEach((c, i) => {
    if (!c.code) return;
    src += `  ${i === 0 ? "if" : "else if"} (index == ${i}) { ${c.code} }\n`;
  });
  src += "}\n";
  return src;
}

function luxelPixels(source, pixels) {
  const tmp = path.join(os.tmpdir(), `oracle-px-${process.pid}.js`);
  fs.writeFileSync(tmp, source);
  try {
    const out = execFileSync(LUXEL, ["pixels", tmp, "--pixels", String(pixels)], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });
    return JSON.parse(out);
  } finally {
    fs.unlinkSync(tmp);
  }
}

let diffs = 0;
function comparePixels(labels, pbPx, luxPx, results) {
  for (let i = 0; i < labels.length; i++) {
    const pb = [pbPx[i * 3], pbPx[i * 3 + 1], pbPx[i * 3 + 2]];
    const lux = [luxPx[i * 3], luxPx[i * 3 + 1], luxPx[i * 3 + 2]];
    const delta = Math.max(...pb.map((v, k) => Math.abs(v - lux[k])));
    const match = delta === 0;
    if (!match) diffs++;
    results.push({ name: labels[i], pb, luxel: lux, delta });
    console.log(
      `${match ? " ok " : delta <= 1 ? "±1  " : "DIFF"}  ${labels[i].padEnd(16)} pb=${pb.join(",").padEnd(12)} luxel=${lux.join(",").padEnd(12)}${match ? "" : ` Δ${delta}`}`,
    );
  }
}

async function main() {
  console.error(`fetching web UI from ${ip}…`);
  const webUI = await (await fetch(`http://${ip}/`)).text();
  const compile = buildCompiler(webUI);

  const pb = await PB.connect(ip);
  const { settings, seq } = await pb.getConfig();
  const restoreId = seq.activeProgram?.activeProgramId;
  const px = settings.pixelCount;
  console.error(`device: fw ${settings.ver}, ${px} px, brightness ${settings.brightness}`);
  if (settings.brightness !== 1) {
    console.error(
      "NOTE: device brightness < 1 — previewFrames may be scaled; interpret with care",
    );
  }

  const results = [];
  try {
    // battery 1
    const src1 = batterySource(CASES);
    const c1 = compile(src1);
    if (!c1.ok) throw new Error(`PB compile failed: ${c1.error}`);
    await pb.setCode(packBytecode(c1));
    const pbPx1 = await pb.getPreviewFrame(600);
    const luxPx1 = luxelPixels(src1, px);
    console.log("-- rgb/hsv rounding, clamping, wrapping --");
    comparePixels(
      CASES.map((c) => c.name),
      pbPx1,
      luxPx1,
      results,
    );

    // battery 2
    const c2 = compile(PALETTE_SRC);
    if (!c2.ok) {
      console.log(`-- palette battery skipped: PB compile failed (${c2.error}) --`);
    } else {
      await pb.setCode(packBytecode(c2));
      const pbPx2 = await pb.getPreviewFrame(600);
      const luxPx2 = luxelPixels(PALETTE_SRC, px);
      console.log("-- palette paint() index semantics --");
      comparePixels(
        PAINT_VALS.map((v) => `paint(${v})`),
        pbPx2,
        luxPx2,
        results,
      );
    }
  } finally {
    if (restoreId) {
      console.error(`restoring active pattern ${restoreId}…`);
      await pb.setActivePattern(restoreId).catch(() => {});
    }
    await pb.close();
  }

  fs.writeFileSync(
    "tools/oracle/last-pixels.json",
    JSON.stringify({ ip, fw: settings.ver, when: new Date().toISOString(), results }, null, 1),
  );
  const exact = results.filter((r) => r.delta === 0).length;
  const close = results.filter((r) => r.delta === 1).length;
  console.log(
    `\n${exact}/${results.length} exact, ${close} within ±1, ${results.length - exact - close} diverge`,
  );
  console.log("wrote tools/oracle/last-pixels.json");
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
