// End-to-end check of the MQTT bridge against a REAL local broker:
// mosquitto (from the dev shell) + `luxel serve` (the firmware's native
// mirror — same hamqtt wire contract). Covers connect/availability and
// the event topic (`luxel/<id>/event` → readEvent() patterns), which is
// exactly the path an HA automation exercises. Run from the repo root:
//   node tools/mqtt-e2e.mjs
// The firmware runs the same shared luxel_core::hamqtt code; on-device
// verification is a deploy-time check, not this script's job.

import { execSync, spawn } from "node:child_process";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { lxpBody } from "../web/tools/lxp.mjs"; // needs web/public/luxel.wasm (npm run wasm)

const HTTP_PORT = 8722;
const MQTT_PORT = 18830;
const MQTT_ID = "luxel-native"; // the mirror's fixed device id
const base = `http://127.0.0.1:${HTTP_PORT}`;

let failures = 0;
function check(name, ok, extra = "") {
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${extra ? `  (${extra})` : ""}`);
  if (!ok) failures++;
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
async function until(what, fn, ms = 8000) {
  const t0 = Date.now();
  for (;;) {
    const v = await fn().catch(() => null);
    if (v) return v;
    if (Date.now() - t0 > ms) throw new Error(`timeout waiting for ${what}`);
    await sleep(200);
  }
}
const pub = (topic, msg) =>
  execSync(
    `mosquitto_pub -p ${MQTT_PORT} -t '${topic}' -m '${msg.replace(/'/g, "'\\''")}'`,
  );

// ---- broker (mosquitto 2.x denies anonymous by default — allow it) ----
const dir = mkdtempSync(join(tmpdir(), "luxel-mqtt-e2e-"));
writeFileSync(join(dir, "mosq.conf"), `listener ${MQTT_PORT} 127.0.0.1\nallow_anonymous true\n`);
const broker = spawn("mosquitto", ["-c", join(dir, "mosq.conf")], { stdio: "ignore" });

// ---- the mirror ----
execSync("cargo build -q -p luxel-cli", { stdio: "inherit" });
const server = spawn("target/debug/luxel", ["serve", "--port", String(HTTP_PORT), "--pixels", "120"], {
  stdio: ["ignore", "pipe", "inherit"],
});
process.on("exit", () => {
  server.kill();
  broker.kill();
  rmSync(dir, { recursive: true, force: true });
});

try {
  await new Promise((resolve, reject) => {
    server.stdout.on("data", (d) => { if (String(d).includes("luxel serve:")) resolve(); });
    server.on("exit", () => reject(new Error("server died")));
    setTimeout(() => reject(new Error("server start timeout")), 30000);
  });

  // point the mirror at the broker and wait for the session
  const cfg = await (await fetch(`${base}/api/mqtt`, {
    method: "POST",
    body: `127.0.0.1\n${MQTT_PORT}\n\n`,
  })).json();
  check("mqtt config accepted", cfg.ok === true, JSON.stringify(cfg));
  const st = await until("broker connect", async () => {
    const s = await (await fetch(`${base}/api/mqtt`)).json();
    return s.connected ? s : null;
  });
  check("mirror connects to mosquitto", st.connected === true);

  // availability is retained — a late subscriber must still see "online"
  const avail = execSync(
    `mosquitto_sub -p ${MQTT_PORT} -t luxel/${MQTT_ID}/status -C 1 -W 5`,
  ).toString().trim();
  check("availability retained as online", avail === "online", avail);

  // a readEvent pattern: any event paints the strip red at the event's value
  const evSrc =
    "var ev = array(4)\nvar hit = 0\n" +
    "export function beforeRender(delta) { while (readEvent(ev)) hit = ev[3] }\n" +
    "export function render(index) { rgb(hit, 0, 0) }";
  const up = await (await fetch(`${base}/api/code`, { method: "POST", body: await lxpBody("", evSrc) })).json();
  check("readEvent pattern accepted", up.ok === true, JSON.stringify(up));
  await sleep(300);
  const dark = new Uint8Array(await (await fetch(`${base}/api/pixels`)).arrayBuffer());
  check("dark before any event", dark.every((b) => b === 0));

  // the mapping under test: publish text event lines → pixels react
  pub(`luxel/${MQTT_ID}/event`, "1 0.5 0.25 1");
  const lit = await until("event pixels", async () => {
    const px = new Uint8Array(await (await fetch(`${base}/api/pixels`)).arrayBuffer());
    return px[0] === 255 && px[1] === 0 ? px : null;
  });
  check("MQTT event drives pixels red", lit[0] === 255, `first px = ${lit[0]},${lit[1]},${lit[2]}`);

  // defaults: a bare "type" line carries value 1 — publish value 0.5 first
  // to prove the change, then a junk payload must not kill the session
  pub(`luxel/${MQTT_ID}/event`, "1 0 0 0.5");
  const half = await until("half-value pixels", async () => {
    const px = new Uint8Array(await (await fetch(`${base}/api/pixels`)).arrayBuffer());
    return Math.abs(px[0] - 128) < 4 ? px : null;
  });
  check("event value field lands (0.5 → ~128)", Math.abs(half[0] - 128) < 4, `px=${half[0]}`);
  pub(`luxel/${MQTT_ID}/event`, '{"not":"numbers"}');
  pub(`luxel/${MQTT_ID}/event`, "1");
  const full = await until("bare-type pixels", async () => {
    const px = new Uint8Array(await (await fetch(`${base}/api/pixels`)).arrayBuffer());
    return px[0] === 255 ? px : null;
  });
  check("junk ignored; bare-type line defaults to value 1", full[0] === 255, `px=${full[0]}`);
} catch (e) {
  check(`aborted: ${e.message}`, false);
}

console.log(failures === 0 ? "\nall checks passed" : `\n${failures} FAILURES`);
process.exit(failures === 0 ? 0 : 1);
