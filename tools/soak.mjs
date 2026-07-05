// Hot-swap soak test: cycle the whole community corpus through a running
// `luxel serve` instance via POST /api/code — the same path the firmware
// takes — and watch liveness, fps, and server memory. Rehearses the M2 exit
// criterion ("evening-long soak without a crash") on the host.
//
// Usage: node tools/soak.mjs [rounds] [dwell-ms]

import { execSync, spawn } from "node:child_process";
import fs from "node:fs";

const ROUNDS = Number(process.argv[2] ?? 2);
const DWELL = Number(process.argv[3] ?? 250);
const PORT = 8722;
const base = `http://127.0.0.1:${PORT}`;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

execSync("cargo build -q -p luxel-cli", { stdio: "inherit" });
const server = spawn("target/debug/luxel", ["serve", "--port", String(PORT), "--pixels", "300"], {
  stdio: ["ignore", "pipe", "inherit"],
});
let died = false;
server.on("exit", () => { died = true; });
process.on("exit", () => server.kill());
await new Promise((resolve, reject) => {
  server.stdout.on("data", (d) => { if (String(d).includes("luxel serve:")) resolve(); });
  setTimeout(() => reject(new Error("server start timeout")), 30000);
});

const rssKb = () =>
  Number(/VmRSS:\s+(\d+)/.exec(fs.readFileSync(`/proc/${server.pid}/status`, "utf8"))[1]);

const files = fs.readdirSync("corpus").filter((f) => f.endsWith(".epe"));
console.log(`soak: ${files.length} patterns × ${ROUNDS} rounds, ${DWELL}ms dwell, RSS start ${(rssKb() / 1024).toFixed(1)} MB`);

let uploads = 0, accepted = 0, rejected = 0, vmerrs = 0;
const rejectedNames = new Set(), vmerrNames = new Set();

for (let round = 0; round < ROUNDS; round++) {
  for (const f of files) {
    if (died) break;
    const epe = JSON.parse(fs.readFileSync(`corpus/${f}`, "utf8"));
    const src = epe.sources?.main;
    if (typeof src !== "string") continue;
    uploads++;
    const r = await (await fetch(`${base}/api/code`, { method: "POST", body: src })).json();
    if (r.ok) accepted++;
    else { rejected++; rejectedNames.add(epe.name); continue; }
    await sleep(DWELL);
    const st = await (await fetch(`${base}/api/status`)).json();
    if (st.vmerr) { vmerrs++; vmerrNames.add(`${epe.name}: ${st.vmerr}`); }
    if (st.fps === 0 && uploads > 5) console.log(`  WARN fps=0 after ${epe.name}`);
  }
  console.log(`round ${round + 1}/${ROUNDS} done, RSS ${(rssKb() / 1024).toFixed(1)} MB`);
}

if (died) {
  console.log("FAIL: server process died during soak");
  process.exit(1);
}
const st = await (await fetch(`${base}/api/status`)).json();
console.log(`\nsoak done: ${uploads} uploads, ${accepted} accepted, ${rejected} compile-rejected, ${vmerrs} runtime errors; final fps ${st.fps}, RSS ${(rssKb() / 1024).toFixed(1)} MB`);
if (rejectedNames.size) console.log("compile-rejected:", [...rejectedNames].join(" | "));
if (vmerrNames.size) console.log("runtime errors:\n  " + [...vmerrNames].join("\n  "));
server.kill();
process.exit(0);
