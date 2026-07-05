// Differential oracle driver: run the test battery on a real Pixel Blaze
// (black-box, via its public websocket; live-code only, nothing saved) and
// on the Luxel engine, and diff the exact 16.16 raws.
//
// Usage (from repo root, inside `nix develop`):
//   cargo build --release -p luxel-cli
//   node tools/oracle/run.mjs <ip> [--filter substr]

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { buildCompiler, packBytecode } from "./compiler.mjs";
import { PB, sleep } from "./pb.mjs";
import { SPECIALS, VECTORS } from "./vectors.mjs";

const ip = process.argv[2] ?? "192.168.0.140";
const filterIx = process.argv.indexOf("--filter");
const filter = filterIx >= 0 ? process.argv[filterIx + 1] : null;
const LUXEL = "target/release/luxel";
const BATCH = 18;

const toRaw = (v) => Math.round(v * 65536);

function vectorSource(vectors) {
  let src = "export function render(index) { hsv(0, 0, 0) }\n";
  for (const v of vectors) {
    if (v.setup) src += v.setup.replaceAll("$", "_" + v.name) + "\n";
    src += `export var o_${v.name} = ${v.code.replaceAll("$", "_" + v.name)}\n`;
  }
  return src;
}

function luxelVars(source) {
  const tmp = path.join(os.tmpdir(), `oracle-${process.pid}.js`);
  fs.writeFileSync(tmp, source);
  try {
    const out = execFileSync(LUXEL, ["vars", tmp, "--pixels", "420"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    });
    return { ok: true, vars: JSON.parse(out) };
  } catch (e) {
    return { ok: false, error: String(e.stderr || e.message).trim() };
  }
}

async function pbVars(pb, compile, source, expectKeys = null) {
  const compiled = compile(source);
  if (!compiled.ok) return { ok: false, error: compiled.error };
  await pb.setCode(packBytecode(compiled));
  // vars snapshot happens after a rendered frame; give it a moment
  const arrived = (vars) =>
    expectKeys
      ? expectKeys.some((k) => vars[k] !== undefined)
      : Object.keys(vars).some((k) => k.startsWith("o_"));
  for (let attempt = 0; attempt < 6; attempt++) {
    await sleep(250);
    const vars = await pb.getVars();
    if (arrived(vars)) return { ok: true, vars };
  }
  return { ok: false, error: "expected vars never appeared" };
}

function compare(name, pbVal, luxVal, results) {
  const pbRaw = pbVal === undefined ? null : toRaw(pbVal);
  const luxRaw = luxVal === undefined || luxVal === null ? null : luxVal;
  const match = pbRaw !== null && luxRaw !== null && pbRaw === luxRaw;
  results.push({ name, pb: pbRaw, luxel: luxRaw, match });
  const fmt = (r) => (r === null ? "∅" : `${r} (${(r / 65536).toPrecision(8)})`);
  console.log(
    `${match ? " ok " : "DIFF"}  ${name.padEnd(16)} pb=${fmt(pbRaw).padEnd(24)} luxel=${fmt(luxRaw)}`,
  );
}

async function main() {
  console.error(`fetching web UI from ${ip}…`);
  const webUI = await (await fetch(`http://${ip}/`)).text();
  const compile = buildCompiler(webUI);
  console.error("compiler extracted OK");

  const pb = await PB.connect(ip);
  const { settings, seq } = await pb.getConfig();
  const restoreId = seq.activeProgram?.activeProgramId;
  console.error(
    `device: ${settings.name} fw ${settings.ver}, ${settings.pixelCount} px, active="${seq.activeProgram?.name}" (${restoreId})`,
  );

  const vectors = filter ? VECTORS.filter((v) => v.name.includes(filter)) : VECTORS;
  const results = [];

  try {
    for (let i = 0; i < vectors.length; i += BATCH) {
      let batch = vectors.slice(i, i + BATCH);
      let src = vectorSource(batch);

      // PB compile; if the whole batch fails, drop failing vectors one by one
      let compiled = compile(src);
      if (!compiled.ok) {
        const bad = [];
        for (const v of batch) {
          const solo = compile(vectorSource([v]));
          if (!solo.ok) {
            bad.push(v.name);
            results.push({ name: v.name, pbCompileError: solo.error, match: false });
            console.log(`DIFF  ${v.name.padEnd(16)} PB compile error: ${solo.error}`);
          }
        }
        batch = batch.filter((v) => !bad.includes(v.name));
        if (batch.length === 0) continue;
        src = vectorSource(batch);
      }

      const pbSide = await pbVars(pb, compile, src);
      const luxSide = luxelVars(src);
      if (!pbSide.ok || !luxSide.ok) {
        console.log(`batch ${i / BATCH}: pb=${pbSide.error ?? "ok"} luxel=${luxSide.error ?? "ok"}`);
        if (pbSide.ok) {
          for (const v of batch) {
            const val = pbSide.vars[`o_${v.name}`];
            if (val !== undefined) console.log(`  pb-only ${v.name} = ${toRaw(val)} (${val})`);
          }
        }
        for (const v of batch) results.push({ name: v.name, batchError: true, match: false });
        continue;
      }
      for (const v of batch) {
        compare(v.name, pbSide.vars[`o_${v.name}`], luxSide.vars[`o_${v.name}`], results);
      }
    }

    for (const sp of SPECIALS) {
      if (sp.skip || (filter && !sp.title.includes(filter))) continue;
      console.log(`--- ${sp.title}`);
      const pbSide = await pbVars(pb, compile, sp.source, sp.keys);
      const luxSide = luxelVars(sp.source);
      for (const k of sp.keys) {
        compare(
          `${sp.title}.${k}`,
          pbSide.ok ? pbSide.vars[k] : undefined,
          luxSide.ok ? luxSide.vars[k] : undefined,
          results,
        );
      }
    }
  } finally {
    if (restoreId) {
      console.error(`restoring active pattern ${restoreId}…`);
      await pb.setActivePattern(restoreId).catch(() => {});
    }
    await pb.close();
  }

  const diffs = results.filter((r) => !r.match);
  console.log(`\n${results.length - diffs.length}/${results.length} match, ${diffs.length} diffs`);
  fs.writeFileSync(
    "tools/oracle/last-results.json",
    JSON.stringify({ ip, fw: settings.ver, when: new Date().toISOString(), results }, null, 2),
  );
  console.error("wrote tools/oracle/last-results.json");
}

main().then(
  () => process.exit(0),
  (e) => {
    console.error("fatal:", e.message ?? e);
    process.exit(1);
  },
);
