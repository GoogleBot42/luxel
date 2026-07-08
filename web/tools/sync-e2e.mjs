// Luxel-to-Luxel sync e2e: two native mirrors over loopback UDP.
// A leads, B follows: B's engine clock converges on A's, and a sensor
// frame posted to A relays to B through the beacon.
//
// Usage (from web/): node tools/sync-e2e.mjs

import { execSync, spawn } from "node:child_process";
import { lxpBody } from "./lxp.mjs";

const A_PORT = 8731;
const B_PORT = 8732;
const SYNC_PORT = 14049;
const A = `http://127.0.0.1:${A_PORT}`;
const B = `http://127.0.0.1:${B_PORT}`;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

execSync("cargo build -q -p luxel-cli", { stdio: "inherit", cwd: ".." });
const mirror = (port, extra) =>
  spawn(
    "../target/debug/luxel",
    ["serve", "--port", String(port), "--pixels", "60", ...extra],
    { stdio: ["ignore", "pipe", "inherit"] },
  );
const started = (p) =>
  new Promise((resolve, reject) => {
    p.stdout.on("data", (d) => String(d).includes("luxel serve:") && resolve());
    p.on("exit", () => reject(new Error("mirror died")));
    setTimeout(() => reject(new Error("mirror start timeout")), 30000);
  });

const a = mirror(A_PORT, ["--sync-target", "127.0.0.1", "--sync-port", String(SYNC_PORT)]);
const b = mirror(B_PORT, [
  "--sync-port",
  String(SYNC_PORT),
  "--sync-http-port",
  String(A_PORT), // pattern pulls hit the leader's HTTP port (device = :80)
]);
await Promise.all([started(a), started(b)]);

const fails = [];
const check = (name, cond, detail = "") => {
  console.log(`${cond ? " ok " : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`);
  if (!cond) fails.push(name);
};
const getSync = async (base) => (await fetch(`${base}/api/sync`)).json();

try {
  // same pattern on both (a time-driven one, so clocks matter). A code
  // push rebuilds the engine (clock restarts at 0), so pushing to A first
  // and to B 2.5s later leaves them genuinely desynced — the follower has
  // to take the hard-jump path before fine slewing.
  const src =
    "export var energyAverage\n" +
    "export function render(index) { hsv(time(0.05), 1, 1) }";
  const srcBody = await lxpBody("", src);
  await fetch(`${A}/api/code`, { method: "POST", body: srcBody });
  await sleep(2500);
  await fetch(`${B}/api/code`, { method: "POST", body: srcBody });
  await sleep(400);
  const t0a = (await getSync(A)).timeMs;
  const t0b = (await getSync(B)).timeMs;
  check(
    "mirrors start genuinely desynced (>1.5s apart)",
    t0a > 0 && t0b > 0 && t0a - t0b > 1500,
    `${t0a} vs ${t0b}`,
  );

  // roles: A leads, B follows
  check("sync: default mode off", (await getSync(A)).mode === "off");
  const ra = await (await fetch(`${A}/api/sync`, { method: "POST", body: "leader" })).json();
  const rb = await (await fetch(`${B}/api/sync`, { method: "POST", body: "follower" })).json();
  check("sync: roles accepted", ra.ok === true && rb.ok === true);
  const badRole = await (await fetch(`${B}/api/sync`, { method: "POST", body: "boss" })).json();
  check("sync: bad role rejected", badRole.ok === false);

  // convergence: B hears beacons and pulls its clock onto A's
  let offset = null;
  for (let i = 0; i < 30; i++) {
    await sleep(300);
    const s = await getSync(B);
    if (s.leader) {
      offset = s.leader.offsetMs;
      if (Math.abs(offset) < 40) break;
    }
  }
  check("sync: follower heard the leader", offset !== null);
  check("sync: clocks converged (<40ms)", offset !== null && Math.abs(offset) < 40, `offset ${offset}ms`);
  const ta = (await getSync(A)).timeMs;
  const tb = (await getSync(B)).timeMs;
  check("sync: absolute clocks agree (<250ms)", Math.abs(ta - tb) < 250, `${ta} vs ${tb}`);

  // sensor relay: a frame posted to the LEADER shows up in the FOLLOWER
  const sb = Buffer.alloc(98);
  sb.write("SB1.0\0", 0, "latin1");
  sb.writeUInt16LE(0x3000, 70); // energyAverage = 0.1875
  sb.write("END\0", 94, "latin1");
  await fetch(`${A}/api/sensors`, { method: "POST", body: sb });
  let relayed = false;
  for (let i = 0; i < 10 && !relayed; i++) {
    await sleep(300);
    const vars = await (await fetch(`${B}/api/vars`)).json();
    if (vars.energyAverage === 0x3000) relayed = true;
  }
  check("sync: sensor frame relays leader → follower", relayed);

  // pattern distribution: pushing NEW code to the leader propagates to the
  // follower (hash in the beacon → follower pulls /api/pattern and adopts)
  const marker = `export var syncMarker = 42\n${src}`;
  await fetch(`${A}/api/code`, { method: "POST", body: await lxpBody("", marker) });
  let adopted = false;
  for (let i = 0; i < 20 && !adopted; i++) {
    await sleep(400);
    const bSrc = await (await fetch(`${B}/api/pattern`)).text();
    adopted = bSrc.includes("syncMarker");
  }
  check("sync: leader's new pattern propagates to the follower", adopted);
  // adoption rebuilds the follower engine (clock restarts) — it re-converges
  let offset2 = null;
  for (let i = 0; i < 20; i++) {
    await sleep(300);
    const s2 = await getSync(B);
    if (s2.leader) {
      offset2 = s2.leader.offsetMs;
      if (Math.abs(offset2) < 40) break;
    }
  }
  check(
    "sync: re-converges after adoption (<40ms)",
    offset2 !== null && Math.abs(offset2) < 40,
    `offset ${offset2}ms`,
  );

  // follower back to off clears the leader lock
  await fetch(`${B}/api/sync`, { method: "POST", body: "off" });
  const cleared = await getSync(B);
  check("sync: off clears leader state", cleared.mode === "off" && cleared.leader === null);
} finally {
  a.kill();
  b.kill();
}

console.log(fails.length ? `\n${fails.length} FAILURES: ${fails.join(", ")}` : "\nsync e2e: all checks pass");
process.exit(fails.length ? 1 : 0);
