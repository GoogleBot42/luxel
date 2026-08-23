// Differential report: luxel-core vs captured Pixel Blaze sweep data.
// Regenerates each sweep's probe pattern (identical inputs), runs it through
// luxel-cli (host build of the same core the firmware runs), and diffs the
// raw 16.16 outputs against tools/oracle/sweeps/<name>.json.
//
// Usage: node tools/oracle/compare-sweeps.mjs [sweep-name ...]

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { buildBatchSource, SWEEPS } from "./sweep.mjs";

const only = process.argv.slice(2);
const CLI = ["run", "-q", "-p", "luxel-cli", "--"];

function luxelVars(src) {
  const tmp = path.join(os.tmpdir(), `luxel-sweep-${process.pid}.js`);
  fs.writeFileSync(tmp, src);
  try {
    const out = execFileSync("cargo", [...CLI, "vars", tmp], { encoding: "utf8" });
    return JSON.parse(out);
  } finally {
    fs.unlinkSync(tmp);
  }
}

console.log("sweep   n     exact    ±1      ±4      max(raw)  max(value)  at x");
for (const [name, def] of Object.entries(SWEEPS)) {
  if (only.length > 0 && !only.includes(name)) continue;
  const file = `tools/oracle/sweeps/${name}.json`;
  if (!fs.existsSync(file)) continue;
  const pb = new Map(JSON.parse(fs.readFileSync(file, "utf8")));

  // luxel outputs over the same inputs, batched like the device probe
  const BATCH = 200;
  const ours = [];
  for (let off = 0; off < def.inputs.length; off += BATCH) {
    const batch = def.inputs.slice(off, off + BATCH);
    // `def.setup` (e.g. setPerlinWrap) is part of the probe — dropping it
    // silently compares a different function than the device ran.
    const vars = luxelVars(buildBatchSource(def.expr, batch, def.setup));
    if (!Array.isArray(vars.ys) || vars.ys.length !== batch.length) {
      throw new Error(`${name}: luxel ys missing`);
    }
    batch.forEach((x, i) => ours.push([x, vars.ys[i]]));
  }

  let n = 0, exact = 0, w1 = 0, w4 = 0, maxErr = 0, maxAt = 0;
  for (const [x, y] of ours) {
    if (!pb.has(x)) continue;
    const ref = pb.get(x);
    n++;
    const e = Math.abs(y - ref);
    if (e === 0) exact++;
    if (e <= 1) w1++;
    if (e <= 4) w4++;
    if (e > maxErr) { maxErr = e; maxAt = x; }
  }
  const pct = (k) => ((100 * k) / n).toFixed(1).padStart(5) + "%";
  console.log(
    name.padEnd(7),
    String(n).padEnd(5),
    pct(exact), pct(w1), pct(w4),
    String(maxErr).padStart(9),
    (maxErr / 65536).toFixed(5).padStart(11),
    "x=" + (maxAt / 65536).toFixed(3),
  );
}
