// Smoke test of `luxel serve` (the native mirror of the firmware server):
// its HTTP API plus page routing — `/` serves the built playground when
// present (else the minimal fallback), `/min` always the minimal page. The
// full playground UI is driven in a real browser by web/tools/device-e2e.mjs;
// this is the fast, dependency-light check. Run from the repo root:
//   node tools/serve-e2e.mjs

import { execSync, spawn } from "node:child_process";
import { lxpBody } from "../web/tools/lxp.mjs"; // needs web/public/luxel.wasm (npm run wasm)

const PORT = 8721;
let failures = 0;
function check(name, ok, extra = "") {
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${extra ? `  (${extra})` : ""}`);
  if (!ok) failures++;
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// ---- start the server ----
execSync("cargo build -q -p luxel-cli", { stdio: "inherit" });
const server = spawn("target/debug/luxel", ["serve", "--port", String(PORT), "--pixels", "120"], {
  stdio: ["ignore", "pipe", "inherit"],
});
process.on("exit", () => server.kill());
await new Promise((resolve, reject) => {
  server.stdout.on("data", (d) => { if (String(d).includes("luxel serve:")) resolve(); });
  server.on("exit", () => reject(new Error("server died")));
  setTimeout(() => reject(new Error("server start timeout")), 30000);
});

// ---- API-level checks (mirror what the page does) ----
const base = `http://127.0.0.1:${PORT}`;
await sleep(1200); // let fps settle
const status = await (await fetch(`${base}/api/status`)).json();
check("status: pixels", status.pixels === 120, JSON.stringify(status));
check("status: fps > 0", status.fps > 0, `fps=${status.fps}`);
check("status: no vmerr", status.vmerr === null);

// ---- geom + caps (Gitea #464) ----
// The two blocks the v2 UI gates every screen on. A bare strip mirror with
// the 1D default pattern: 1D, regular, the whole pixel count in one row, and
// the board's own shape (no map installed, no engine fallback).
check(
  "status: geom = a bare strip",
  status.geom?.dims === 1 &&
    status.geom.regular === true &&
    status.geom.w === 120 &&
    status.geom.h === 1 &&
    status.geom.source === "board" &&
    status.geom.pattern_dims === 1,
  JSON.stringify(status.geom),
);
check(
  "status: caps = what the mirror implements",
  status.caps?.strip_driver === true &&
    status.caps.panel === false &&
    status.caps.outputs === 1 &&
    status.caps.power_cap === true &&
    status.caps.blur_glow === true &&
    status.caps.layers === 3 &&
    status.caps.text_slots === 0 &&
    status.caps.reboot === false &&
    status.caps.ota === false &&
    status.caps.psram === false &&
    status.caps.assets === false,
  JSON.stringify(status.caps),
);

// A render2D-only pattern installs the engine's fabricated ceil(sqrt(n))
// grid — the geometry `/api/map` cannot see, which is the whole reason
// `geom` reports the ENGINE's view and carries `source`.
await fetch(`${base}/api/code`, {
  method: "POST",
  body: await lxpBody("", "export function render2D(index, x, y) { hsv(x, 1, y) }"),
});
await sleep(400);
const g2d = (await (await fetch(`${base}/api/status`)).json()).geom;
check(
  "geom: render2D-only pattern shows the fabricated grid as source=default",
  g2d.dims === 2 && g2d.regular === true && g2d.w === 11 && g2d.h === 11 && g2d.source === "default" && g2d.pattern_dims === 2,
  JSON.stringify(g2d),
);

// A user map takes over, and an irregular one hides blur/glow (no neighbours).
await fetch(`${base}/api/map`, { method: "POST", body: "grid 12 10" });
await sleep(400);
const gUser = await (await fetch(`${base}/api/status`)).json();
check(
  "geom: installed grid map reads as source=user",
  gUser.geom.dims === 2 && gUser.geom.w === 12 && gUser.geom.h === 10 && gUser.geom.source === "user" && gUser.caps.blur_glow === true,
  JSON.stringify(gUser.geom),
);
const irregular = ["3"].concat(
  Array.from({ length: 120 }, (_, i) => `${i * 7919} ${i * 104729} ${i * 1299709}`),
).join(" ");
await fetch(`${base}/api/map`, { method: "POST", body: irregular });
await sleep(400);
const gIrr = await (await fetch(`${base}/api/status`)).json();
check(
  "geom: irregular 3D map is not regular, and hides blur/glow",
  gIrr.geom.dims === 3 && gIrr.geom.regular === false && gIrr.geom.w === 0 && gIrr.geom.h === 0 && gIrr.caps.blur_glow === false,
  JSON.stringify({ geom: gIrr.geom, blur_glow: gIrr.caps.blur_glow }),
);
// back to the bare strip for everything below
await fetch(`${base}/api/map`, { method: "POST", body: "" });
await sleep(400);

const px1 = new Uint8Array(await (await fetch(`${base}/api/pixels`)).arrayBuffer());
check("pixels: 3 bytes per pixel", px1.length === 360, `len=${px1.length}`);
check("pixels: not all black", px1.some((b) => b > 0));

// Patterns compile to LXBC client-side and upload as an LXP1 envelope
// (devices carry no compiler), so a syntax error throws at compile time and
// never reaches the wire.
let compileThrew = false;
try {
  await lxpBody("", "export function render(index) { hsv(");
} catch {
  compileThrew = true;
}
check("code: syntax error rejected at compile", compileThrew);

const good = await (await fetch(`${base}/api/code`, {
  method: "POST",
  body: await lxpBody("", "export function render(index) { rgb(0, 0, 1) }"),
})).json();
check("code: upload accepted", good.ok === true, JSON.stringify(good));
await sleep(300);
const px2 = new Uint8Array(await (await fetch(`${base}/api/pixels`)).arrayBuffer());
check("code: pattern actually swapped (all blue)", px2.length === 360 && px2[0] === 0 && px2[1] === 0 && px2[2] === 255, `first px = ${px2[0]},${px2[1]},${px2[2]}`);

// runtime out-of-bounds compiles clean but faults at render → surfaces on /api/status
const vmerrSrc = "export var arr = array(4)\nexport function render(index) { arr[9] = 1\nhsv(0,0,0) }";
const ve = await (await fetch(`${base}/api/code`, { method: "POST", body: await lxpBody("", vmerrSrc) })).json();
check("code: vmerr pattern accepted (compiles)", ve.ok === true, JSON.stringify(ve));
await sleep(400);
const st2 = await (await fetch(`${base}/api/status`)).json();
check("status: vmerr surfaced with location", typeof st2.vmerr === "string" && st2.vmerr.includes("line 2"), String(st2.vmerr));

// ---- external event injection (POST /api/events → readEvent builtin) ----
const evSrc =
  "var ev = array(4)\nvar hit = 0\n" +
  "export function beforeRender(delta) { while (readEvent(ev)) hit = ev[3] }\n" +
  "export function render(index) { rgb(hit, 0, 0) }";
const evUp = await (await fetch(`${base}/api/code`, { method: "POST", body: await lxpBody("", evSrc) })).json();
check("events: readEvent pattern accepted", evUp.ok === true, JSON.stringify(evUp));
await sleep(300);
const pxDark = new Uint8Array(await (await fetch(`${base}/api/pixels`)).arrayBuffer());
check("events: dark before injection", pxDark.every((b) => b === 0));
// "EV1\0" + count + [type=1, x=0.5, y=0.25, value=1] as raw 16.16 LE
const evFrame = new Uint8Array(5 + 16);
evFrame.set([0x45, 0x56, 0x31, 0, 1]);
const dv = new DataView(evFrame.buffer);
[1, 0.5, 0.25, 1].forEach((v, i) => dv.setInt32(5 + i * 4, Math.round(v * 65536), true));
const evRes = await (await fetch(`${base}/api/events`, { method: "POST", body: evFrame })).json();
check("events: EV1 frame accepted", evRes.ok === true, JSON.stringify(evRes));
await sleep(300);
const pxLit = new Uint8Array(await (await fetch(`${base}/api/pixels`)).arrayBuffer());
check("events: event drives pixels red", pxLit[0] === 255 && pxLit[1] === 0, `first px = ${pxLit[0]},${pxLit[1]},${pxLit[2]}`);
const evBad = await (await fetch(`${base}/api/events`, { method: "POST", body: "junk" })).json();
check("events: junk body rejected", evBad.ok === false, JSON.stringify(evBad));

// ---- digital pin injection (POST /api/pins → digitalRead builtin, #177) ----
const pinSrc =
  "pinMode(4, INPUT_PULLUP)\n" +
  "export function render(index) { hsv(0, 0, digitalRead(4) == LOW) }";
const pinUp = await (await fetch(`${base}/api/code`, { method: "POST", body: await lxpBody("", pinSrc) })).json();
check("pins: digitalRead pattern accepted", pinUp.ok === true, JSON.stringify(pinUp));
const pins = (body) => fetch(`${base}/api/pins`, { method: "POST", body }).then((r) => r.json());
const firstPx = async () => [...new Uint8Array(await (await fetch(`${base}/api/pixels`)).arrayBuffer()).slice(0, 3)];
await sleep(300);
check("pins: dark at idle (pulled-up pin reads not-pressed)", (await firstPx())[0] === 0);
const pinRes = await pins("4 0\n");
check("pins: level accepted", pinRes.ok === true && pinRes.pins === 1, JSON.stringify(pinRes));
await sleep(300);
const pxHeld = await firstPx();
check("pins: driving pin 4 LOW lights the strip", pxHeld[0] === 255, `first px = ${pxHeld}`);
await sleep(300);
check("pins: the level is HELD, not a one-frame pulse", (await firstPx())[0] === 255);
await pins("4 x\n");
await sleep(300);
check("pins: release returns the pin to its idle level", (await firstPx())[0] === 0);
const pinBad = await pins("nonsense\n");
check("pins: unparseable body rejected", pinBad.ok === false, JSON.stringify(pinBad));

// ---- analog pin injection (POST /api/pins "a <pin> <0..1>" → analogRead, #206)
const potSrc = "export function render(index) { hsv(0, 0, analogRead(33)) }";
const potUp = await (await fetch(`${base}/api/code`, { method: "POST", body: await lxpBody("", potSrc) })).json();
check("pins: analogRead pattern accepted", potUp.ok === true, JSON.stringify(potUp));
await sleep(300);
check("pins: an undriven analog pin reads 0 (dark)", (await firstPx())[0] === 0);
const potRes = await pins("a 33 0.5\n");
check("pins: analog value accepted", potRes.ok === true && potRes.pins === 1, JSON.stringify(potRes));
await sleep(300);
const pxPot = await firstPx();
check("pins: driving analog pin 33 to 0.5 half-lights the strip", pxPot[0] > 100 && pxPot[0] < 160, `first px = ${pxPot}`);
await sleep(300);
check("pins: the analog value is HELD too", (await firstPx())[0] === pxPot[0]);
await pins("analog 33 1\n");
await sleep(300);
check("pins: the spelled-out kind word works and drives full scale", (await firstPx())[0] === 255);
await pins("a 33 x\n");
await sleep(300);
check("pins: releasing an analog pin returns it to 0", (await firstPx())[0] === 0);

// restore a lit pattern (the vmerr pattern renders black: render aborts
// before hsv) so the preview check sees light
await fetch(`${base}/api/code`, {
  method: "POST",
  body: await lxpBody("", "export function render(index) { hsv(index / pixelCount, 1, 1) }"),
});
await sleep(300);

// ---- browser-level checks ----
// ---- page routing (the mirror stands in for a device serving its assets) ----
const rootRes = await fetch(base);
const rootBody = await rootRes.text();
check(
  "GET /: 200 html",
  rootRes.status === 200 && (rootRes.headers.get("content-type") || "").includes("text/html"),
  `status=${rootRes.status}`,
);
// `/` serves the built playground when web/dist exists, else the minimal
// fallback page — either is a valid mirror state, so accept both.
const builtUi = /(?:src|href)=["']\.?\/assets\//.test(rootBody);
const minimalFallback = /isn['’]t installed/.test(rootBody);
check(
  "GET /: playground when built, else minimal fallback",
  builtUi || minimalFallback,
  builtUi ? "playground" : minimalFallback ? "minimal fallback (web/dist not built)" : rootBody.slice(0, 80),
);
if (builtUi) {
  // a hashed asset the page references must resolve through the mirror
  const asset = rootBody.match(/["']\.?(\/assets\/[^"']+?\.js)["']/)?.[1];
  const ar = await fetch(base + asset);
  check(
    "GET playground asset: 200 js",
    ar.status === 200 && (ar.headers.get("content-type") || "").includes("javascript"),
    `${asset} -> ${ar.status}`,
  );
}
const minBody = await (await fetch(`${base}/min`)).text();
check("GET /min: minimal fallback page", /isn['’]t installed/.test(minBody), minBody.slice(0, 80));
// firmware/src/index.html carries build-mode blocks that firmware/build.rs
// resolves at compile time and the mirror resolves at startup (serve.rs
// index_html). If that ever stops happening the raw markers — and the
// hosted-ui paragraph, which contradicts the one above it — ship verbatim.
check("GET /min: build-mode blocks resolved", !/#if |#endif/.test(minBody));
check(
  "GET /min: normal-build half only",
  /tools\/deploy\.sh/.test(minBody) && !/hosted-UI build/.test(minBody),
);
check("GET /nope.js: 404 for a missing asset", (await fetch(`${base}/nope.js`)).status === 404);

// ---- --board panel: impersonate a HUB75 board (Gitea #464) ----
// What lets the Settings page's capability gating be driven without the
// panel on the bench (docs/tools.md).
const panel = spawn(
  "target/debug/luxel",
  ["serve", "--port", String(PORT + 1), "--board", "panel", "--pixels", "4096", "--outputs", "2"],
  { stdio: ["ignore", "pipe", "inherit"] },
);
process.on("exit", () => panel.kill());
await new Promise((resolve, reject) => {
  panel.stdout.on("data", (d) => { if (String(d).includes("luxel serve:")) resolve(); });
  panel.on("exit", () => reject(new Error("panel mirror died")));
  setTimeout(() => reject(new Error("panel mirror start timeout")), 30000);
});
await sleep(800);
const pst = await (await fetch(`http://127.0.0.1:${PORT + 1}/api/status`)).json();
check(
  "--board panel: 4096 px ceiling and the board's own 64x64 grid",
  pst.max_pixels === 4096 &&
    pst.geom.dims === 2 &&
    pst.geom.regular === true &&
    pst.geom.w === 64 &&
    pst.geom.h === 64 &&
    pst.geom.source === "board",
  JSON.stringify({ max_pixels: pst.max_pixels, geom: pst.geom }),
);
check(
  "--board panel: panel caps (no strip driver, no power cap, no blur/glow, 2 layers)",
  pst.caps.panel === true &&
    pst.caps.strip_driver === false &&
    pst.caps.power_cap === false &&
    pst.caps.blur_glow === false &&
    pst.caps.layers === 2 &&
    pst.caps.outputs === 2,
  JSON.stringify(pst.caps),
);
panel.kill();

server.kill();
console.log(failures === 0 ? "\nall checks passed" : `\n${failures} FAILURES`);
process.exit(failures === 0 ? 0 : 1);
