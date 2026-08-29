// One-shot probe battery: does PB's general ADD/SUBTRACT overflow wrap or
// saturate at the 16.16 range ends? (pow/exp2 are known to saturate; hypot
// and products are known to wrap — plain +/-/+= is the open question, and
// it decides whether the fire-blue/fire-red/spring-colors 32.768 s freeze
// family is authentic PB behavior or a Luxel engine gap. Gitea #106.)
// Also: does PB tolerate per-frame array() allocation indefinitely, i.e.
// are arrays freed on rebind? (Gitea #109.)
//
// Usage (repo root, `nix develop`):  node tools/oracle/overflow-probes.mjs <ip>
import { buildCompiler, packBytecode } from "./compiler.mjs";
import { PB, sleep } from "./pb.mjs";

const ip = process.argv[2] ?? "192.168.0.140";
const SENTINEL = 42;

async function vars(pb, compile, source, keys, settleMs = 300) {
  const compiled = compile(source);
  if (!compiled.ok) return { ok: false, error: compiled.error };
  await pb.setCode(packBytecode(compiled));
  for (let attempt = 0; attempt < 10; attempt++) {
    await sleep(settleMs);
    const v = await pb.getVars();
    if (keys.some((k) => v[k] !== undefined) && v.sent === SENTINEL) {
      return { ok: true, vars: v };
    }
    if (attempt >= 5 && keys.some((k) => v[k] !== undefined)) {
      return { ok: true, aborted: true, vars: v };
    }
  }
  return { ok: false, error: "vars never appeared" };
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
    `device: ${settings.name} fw ${settings.ver}, active="${seq.activeProgram?.name}"`,
  );

  try {
    // Operands live in vars so the PB compiler cannot constant-fold the op.
    console.log("=== add/sub overflow (wrap ⇒ 33000→−32536; saturate ⇒ 32767.9998) ===");
    const r = await vars(
      pb,
      compile,
      `export var sent, addBig, addEps, addAssign, subBig, cmpAfter
       export function render(i) { hsv(0,0,0) }
       x = 32000
       y = 1000
       addBig = x + y            // 33000: wrap → −32536
       e = 0.001
       m = 32767.999
       addEps = m + e            // just past MAX: wrap → ≈−32768
       z = 32000
       z += y
       addAssign = z             // += path, same question
       n = 0 - 32000
       subBig = n - y            // −33000: wrap → +32536
       cmpAfter = (addBig >= 20) ? 1 : 0   // the fire-idiom gate after overflow
       sent = ${SENTINEL}`,
      ["addBig"],
    );
    if (!r.ok) console.log(`  FAILED: ${r.error}`);
    else if (r.aborted) console.log(`  init aborted; vars: ${JSON.stringify(r.vars)}`);
    else {
      const v = r.vars;
      console.log(`  addBig    = ${v.addBig}`);
      console.log(`  addEps    = ${v.addEps}`);
      console.log(`  addAssign = ${v.addAssign}`);
      console.log(`  subBig    = ${v.subBig}`);
      console.log(`  cmpAfter  = ${v.cmpAfter}`);
      const wraps = v.addBig < 0;
      console.log(`  verdict: add ${wraps ? "WRAPS" : "SATURATES"}`);
    }

    console.log("=== per-frame array() churn (frees-on-rebind ⇒ frames keeps climbing) ===");
    const churnSrc = `export var sent, frames
       export function render(i) { hsv(0,0,0) }
       c = 0
       export function beforeRender(delta) {
         t = array(100)
         t[0] = c
         c = c + 1
         frames = c
       }
       sent = ${SENTINEL}`;
    const c1 = await vars(pb, compile, churnSrc, ["frames"]);
    if (!c1.ok) console.log(`  FAILED: ${c1.error}`);
    else {
      const f1 = c1.vars.frames;
      await sleep(5000);
      const f2 = (await pb.getVars()).frames;
      await sleep(5000);
      const f3 = (await pb.getVars()).frames;
      console.log(`  frames over ~10 s: ${f1} → ${f2} → ${f3}`);
      const alive = f3 > f2 && f2 > f1;
      console.log(
        `  verdict: per-frame array() ${alive ? `TOLERATED (~${Math.round((f3 - f2) / 5)} fps steady — arrays are freed)` : "KILLS the pattern (allocation cap hit)"}`,
      );
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
