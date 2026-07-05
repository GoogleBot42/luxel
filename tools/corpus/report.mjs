// Run `luxel check` over the fetched corpus and aggregate a compatibility
// report: pass rate, error buckets, which not-yet-implemented builtins real
// patterns actually use, and feature usage (2D/3D renderers, sensors).
//
// Usage (repo root): cargo build --release -p luxel-cli && node tools/corpus/report.mjs

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const LUXEL = "target/release/luxel";
const DIR = "corpus";

// not-yet-implemented builtins (mirror of the Todo list in vm.rs)
const TODO_BUILTINS = [
  "perlin", "perlinFbm", "perlinRidge", "perlinTurbulence", "setPerlinWrap",
  "resetTransform", "transform", "translate", "scale", "rotate",
  "translate3D", "scale3D", "rotateX", "rotateY", "rotateZ",
  "pixelMapDimensions", "has2DMap", "has3DMap", "mapPixels",
  "setPalette", "paint", "pinMode", "digitalWrite", "digitalRead",
  "analogRead", "touchRead", "clockYear", "clockMonth", "clockDay",
  "clockHour", "clockMinute", "clockSecond", "clockWeekday",
  "sequencerNext", "sequencerGetMode", "playlistGetPosition",
  "playlistSetPosition", "playlistGetLength", "nodeId",
];
const SENSOR_VARS = [
  "frequencyData", "energyAverage", "maxFrequency", "maxFrequencyMagnitude",
  "accelerometer", "light", "analogInputs",
];

const files = fs.readdirSync(DIR).filter((f) => f.endsWith(".epe")).sort();
const index = JSON.parse(fs.readFileSync(path.join(DIR, "index.json"), "utf8"));
const nameOf = Object.fromEntries(index.map((i) => [i.id, i.name]));

const results = [];
for (const f of files) {
  const p = path.join(DIR, f);
  let out;
  try {
    out = execFileSync(LUXEL, ["check", p], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] });
  } catch (e) {
    out = e.stdout || `{"file":"${p}","stage":"crash","error":${JSON.stringify(String(e.message))}}`;
  }
  const r = JSON.parse(out.trim().split("\n").pop());
  const src = JSON.parse(fs.readFileSync(p, "utf8")).sources.main;
  r.id = f.replace(".epe", "");
  r.name = nameOf[r.id] ?? r.id;
  r.uses = {
    todo: TODO_BUILTINS.filter((b) => new RegExp(`\\b${b}\\s*\\(`).test(src)),
    render2D: /render2D/.test(src),
    render3D: /render3D/.test(src),
    sensors: SENSOR_VARS.filter((v) => new RegExp(`\\b${v}\\b`).test(src)),
  };
  results.push(r);
}

const by = (stage) => results.filter((r) => r.stage === stage);
const ok = by("ok");
const compileErrs = by("compile");
const runtimeErrs = [...by("init"), ...by("frame")];
const todoErrs = runtimeErrs.filter((r) => r.error?.includes("not implemented"));
const realRuntimeErrs = runtimeErrs.filter((r) => !r.error?.includes("not implemented"));

const bucket = (errs, norm) => {
  const map = new Map();
  for (const r of errs) {
    const k = norm(r.error ?? "?");
    if (!map.has(k)) map.set(k, []);
    map.get(k).push(r.name);
  }
  return [...map.entries()].sort((a, b) => b[1].length - a[1].length);
};

console.log(`corpus: ${results.length} patterns`);
console.log(`  compiles + smoke-runs clean : ${ok.length} (${((100 * ok.length) / results.length).toFixed(1)}%)`);
console.log(`  compile errors              : ${compileErrs.length}`);
console.log(`  runtime: unimplemented      : ${todoErrs.length}`);
console.log(`  runtime: real errors        : ${realRuntimeErrs.length}`);
const compilesTotal = results.length - compileErrs.length - by("epe").length - by("crash").length;
console.log(`  → compile rate              : ${((100 * compilesTotal) / results.length).toFixed(1)}%`);

console.log(`\ncompile error buckets:`);
for (const [k, names] of bucket(compileErrs, (e) => e.replace(/^\d+:\d+: /, "").replace(/`[^`]*`/g, "`…`"))) {
  console.log(`  ${String(names.length).padStart(3)}× ${k}  [${names.slice(0, 4).join(", ")}${names.length > 4 ? ", …" : ""}]`);
}
console.log(`\nunimplemented-builtin runtime errors:`);
for (const [k, names] of bucket(todoErrs, (e) => e)) {
  console.log(`  ${String(names.length).padStart(3)}× ${k}  [${names.slice(0, 4).join(", ")}${names.length > 4 ? ", …" : ""}]`);
}
console.log(`\nreal runtime errors:`);
for (const r of realRuntimeErrs) {
  console.log(`  ${r.name} (${r.stage}): ${r.error}`);
}

// static usage counts across the whole corpus (what to build next)
const todoUse = new Map();
for (const r of results) for (const b of r.uses.todo) todoUse.set(b, (todoUse.get(b) ?? 0) + 1);
console.log(`\nTodo-builtin usage across corpus (patterns referencing):`);
for (const [b, n] of [...todoUse.entries()].sort((a, b) => b[1] - a[1])) {
  console.log(`  ${String(n).padStart(3)}× ${b}`);
}
console.log(`\nfeature usage: render2D=${results.filter((r) => r.uses.render2D).length}, render3D=${results.filter((r) => r.uses.render3D).length}, sensors=${results.filter((r) => r.uses.sensors.length).length}`);

fs.writeFileSync("tools/corpus/last-report.json", JSON.stringify(results, null, 1));
console.error("\nwrote tools/corpus/last-report.json");
