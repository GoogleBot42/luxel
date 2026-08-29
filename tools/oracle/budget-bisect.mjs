// Pin PB's array element cap: init-time array(N) — an aborted init leaves
// the sentinel unset (oracle rule: aborted init ⇒ exported vars read 0).
// Also: is the cap per-array or cumulative across arrays?
// Usage: node tools/oracle/budget-bisect.mjs <ip>
import { buildCompiler, packBytecode } from "./compiler.mjs";
import { PB, sleep } from "./pb.mjs";

const ip = process.argv[2] ?? "192.168.0.140";
const SENTINEL = 42;

async function initOk(pb, compile, source) {
  const compiled = compile(source);
  if (!compiled.ok) return { ok: false, compileError: compiled.error };
  await pb.setCode(packBytecode(compiled));
  for (let attempt = 0; attempt < 6; attempt++) {
    await sleep(250);
    const v = await pb.getVars();
    if (v.sent === SENTINEL) return { ok: true };
    if (v.sent !== undefined && attempt >= 3) return { ok: false };
  }
  return { ok: false };
}

async function main() {
  const webUI = await (await fetch(`http://${ip}/`)).text();
  const compile = buildCompiler(webUI);
  const pb = await PB.connect(ip);
  const { seq } = await pb.getConfig();
  const restoreId = seq.activeProgram?.activeProgramId;

  const single = (n) =>
    `export var sent\nexport function render(i){hsv(0,0,0)}\na = array(${n})\nsent = ${SENTINEL}`;
  const dual = (n, m) =>
    `export var sent\nexport function render(i){hsv(0,0,0)}\na = array(${n})\nb = array(${m})\nsent = ${SENTINEL}`;

  try {
    console.log("=== single-array bisect ===");
    let lo = 1, hi = 65536; // find the largest OK n
    // establish bounds first
    for (const n of [1000, 10000, 20000, 40000, 65536]) {
      const r = await initOk(pb, compile, single(n));
      console.log(`  array(${n}): ${r.ok ? "ok" : "ABORT"}${r.compileError ? " compile: " + r.compileError : ""}`);
      if (r.ok) lo = n; else { hi = n; break; }
    }
    while (hi - lo > 1) {
      const mid = Math.floor((lo + hi) / 2);
      const r = await initOk(pb, compile, single(mid));
      console.log(`  array(${mid}): ${r.ok ? "ok" : "ABORT"}`);
      if (r.ok) lo = mid; else hi = mid;
    }
    console.log(`  largest single array: ${lo}`);

    console.log("=== cumulative? two arrays of cap/2+ ===");
    const half = Math.floor(lo / 2);
    for (const [n, m] of [[half, half], [half + 200, half + 200], [lo, 1]]) {
      const r = await initOk(pb, compile, dual(n, m));
      console.log(`  array(${n}) + array(${m}) = ${n + m}: ${r.ok ? "ok" : "ABORT"}`);
    }
  } finally {
    if (restoreId) await pb.setActivePattern(restoreId).catch(() => {});
    await pb.close();
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
