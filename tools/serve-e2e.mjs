// Smoke test of `luxel serve` (the native mirror of the firmware server):
// its HTTP API plus page routing — `/` serves the built playground when
// present (else the minimal fallback), `/min` always the minimal page. The
// full playground UI is driven in a real browser by web/tools/device-e2e.mjs;
// this is the fast, dependency-light check. Run from the repo root:
//   node tools/serve-e2e.mjs

import { execSync, spawn } from "node:child_process";
import { PORT as E2E } from "../web/tools/e2e-common.mjs";
import { lxpBody } from "../web/tools/lxp.mjs"; // needs web/public/luxel.wasm (npm run wasm)

// `E2E_PORT + 70` (and +71 for the panel impersonation), so a concurrent
// session's run of this harness does not fight this one for the port —
// same plan every browser harness uses (web/tools/e2e-common.mjs).
const PORT = E2E.serve;
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

// ---- geom.compatible (Gitea #538) ----
// A Layout shows its own dimensionality and lower, never higher. The strip
// mirror above is still on the render2D pattern's FABRICATED grid, so this
// is the case that matters: dims says 2, the rig is a strip, compatible is
// false — and the frame still renders, so nothing goes dark.
check("geom: 1D pattern on a strip is compatible", status.geom?.compatible === true);
check(
  "geom: 2D pattern on a strip is NOT compatible (the grid is fabricated)",
  g2d.compatible === false,
  JSON.stringify(g2d),
);
const pxIncompat = new Uint8Array(await (await fetch(`${base}/api/pixels`)).arrayBuffer());
check("geom: an incompatible pattern still renders", pxIncompat.some((b) => b > 0));

// ---- device name (GET/POST /api/name, Gitea #538) ----
const name0 = await (await fetch(`${base}/api/name`)).json();
check("name: defaults to the mirror's own", name0.name === "luxel-serve" && name0.source === "default", JSON.stringify(name0));
const nameSet = await (await fetch(`${base}/api/name`, { method: "POST", body: "Kitchen Strip" })).json();
check(
  "name: POST stores it and asks for a reboot",
  nameSet.ok === true && nameSet.name === "Kitchen Strip" && nameSet.source === "stored" && nameSet.reboot_required === true,
  JSON.stringify(nameSet),
);
check(
  "name: /api/status reports it",
  (await (await fetch(`${base}/api/status`)).json()).name === "Kitchen Strip",
);
const nameBad = await (await fetch(`${base}/api/name`, { method: "POST", body: "x".repeat(33) })).json();
check("name: over 32 bytes is rejected", nameBad.ok === false, JSON.stringify(nameBad));
const nameQuote = await (await fetch(`${base}/api/name`, { method: "POST", body: 'a"b' })).json();
check("name: a JSON metacharacter is rejected", nameQuote.ok === false, JSON.stringify(nameQuote));
check(
  "name: the rejected name did not stick",
  (await (await fetch(`${base}/api/name`)).json()).name === "Kitchen Strip",
);
const nameClear = await (await fetch(`${base}/api/name`, { method: "POST", body: "" })).json();
check("name: empty body restores the default", nameClear.name === "luxel-serve" && nameClear.source === "default", JSON.stringify(nameClear));

// ---- clock sync now (POST /api/clock/sync, Gitea #538) ----
await fetch(`${base}/api/clock`, { method: "POST", body: "-360" });
const syncNow = await (await fetch(`${base}/api/clock/sync`, { method: "POST" })).json();
const clockNow = await (await fetch(`${base}/api/clock`)).json();
check(
  "clock/sync: mirror answers ok + already synced",
  syncNow.ok === true && syncNow.synced === true && Math.abs(syncNow.local - clockNow.local) < 5,
  JSON.stringify({ syncNow, clockNow }),
);
await fetch(`${base}/api/clock`, { method: "POST", body: "0" });

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

// ---- playlist wire format (Gitea #470) ----
// The line codec is the contract between the firmware, the mirror and the
// web client, and its whole point is that it only ever GROWS: a playlist a
// pre-#470 device wrote has no `P` line and must parse exactly as it did.
const savePat = async (name, src) =>
  (await (await fetch(`${base}/api/patterns`, { method: "POST", body: await lxpBody(name, src) })).json()).id;
const plA = await savePat("PL one", "export function sliderHue(h) { g = h }\nexport function render(index) { hsv(g + index / pixelCount, 1, 1) }");
const plB = await savePat("PL two", "export function render2D(index, x, y) { hsv(x, 1, y) }");

// (1) the OLD format, byte-for-byte what a device wrote before `P` existed
await fetch(`${base}/api/playlist`, {
  method: "POST",
  body: `D 7\nX 250\nI ${plA} -1\nC sliderHue 32768\nI ${plB} 3\n`,
});
const plOld = await (await fetch(`${base}/api/playlist`)).json();
check(
  "playlist: an old-format body parses unchanged",
  plOld.defaultSec === 7 &&
    plOld.crossfadeMs === 250 &&
    plOld.items.length === 2 &&
    plOld.items[0].sec === null &&
    Math.abs(plOld.items[0].controls.sliderHue[0] - 0.5) < 0.001 &&
    plOld.items[1].sec === 3,
  JSON.stringify(plOld),
);
check(
  "playlist: an old-format item carries no projection",
  plOld.items.every((i) => i.proj === undefined),
  JSON.stringify(plOld.items.map((i) => i.proj ?? null)),
);

// (2) `P` after an item is that item's projection override, echoed back
await fetch(`${base}/api/playlist`, {
  method: "POST",
  body: `D 7\nX 250\nI ${plA} -1\nC sliderHue 32768\nP x\nI ${plB} 3\n`,
});
const plNew = await (await fetch(`${base}/api/playlist`)).json();
check(
  "playlist: a `P` line rides on its own item and round-trips",
  plNew.items[0].proj === "x" &&
    plNew.items[1].proj === undefined &&
    Math.abs(plNew.items[0].controls.sliderHue[0] - 0.5) < 0.001,
  JSON.stringify(plNew.items),
);

// (3) an unknown mode (or an unknown line) leaves the item on the default —
// forward compatibility runs the same way backward compatibility does
await fetch(`${base}/api/playlist`, {
  method: "POST",
  body: `D 7\nI ${plA} -1\nP sideways\nQ 1 2 3\nI ${plB} -1\nP yz\n`,
});
const plJunk = await (await fetch(`${base}/api/playlist`)).json();
check(
  "playlist: an unknown projection token and an unknown line are ignored",
  plJunk.items.length === 2 && plJunk.items[0].proj === undefined && plJunk.items[1].proj === "yz",
  JSON.stringify(plJunk.items.map((i) => i.proj ?? null)),
);

// (4) a `P` before any item has nothing to attach to and is dropped
await fetch(`${base}/api/playlist`, { method: "POST", body: `P x\nD 7\nI ${plA} -1\n` });
const plStray = await (await fetch(`${base}/api/playlist`)).json();
check(
  "playlist: a `P` with no item ahead of it is dropped",
  plStray.items.length === 1 && plStray.items[0].proj === undefined,
  JSON.stringify(plStray.items),
);

// clean up so the routing checks below see the mirror as they expect
await fetch(`${base}/api/playlist`, { method: "POST", body: "D 0" });
for (const id of [plA, plB]) await fetch(`${base}/api/patterns/${id}`, { method: "DELETE" });

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
// A panel mirror's Layout IS the panel: kind matrix, 64x64, one panel-shaped
// output, and `strip`/`out` refused on it (Gitea #465).
const panelBase = `http://127.0.0.1:${PORT + 1}`;
const pLayout = await (await fetch(`${panelBase}/api/layout`)).json();
check(
  "--board panel: Layout is the 64x64 matrix",
  pLayout.kind === "matrix" &&
    pLayout.dims === 2 &&
    pLayout.w === 64 &&
    pLayout.h === 64 &&
    pLayout.pixels === 4096 &&
    pLayout.matrix.pw === 64 &&
    pLayout.matrix.cols === 1 &&
    pLayout.outputs.length === 1 &&
    pLayout.outputs[0].count === 1,
  JSON.stringify(pLayout),
);
check(
  "layout: a panel board refuses `strip`",
  (await postLayout(panelBase, "strip 60")).line === 1,
);
panel.kill();

// ---- GET/POST /api/layout (Gitea #465) ----
// The one geometry object: kind, arrangement, outputs, projection defaults.
// Everything below runs against the 120 px strip mirror started at the top.
async function postLayout(b, body) {
  return await (await fetch(`${b}/api/layout`, { method: "POST", body })).json();
}

const l0 = await (await fetch(`${base}/api/layout`)).json();
check(
  "layout: a bare strip mirror reports kind strip, one implicit output",
  l0.kind === "strip" &&
    l0.source === "regular" &&
    l0.dims === 1 &&
    l0.regular === true &&
    l0.w === 120 &&
    l0.h === 1 &&
    l0.pixels === 120 &&
    l0.max === 2048 &&
    l0.matrix === undefined &&
    l0.outputs.length === 1 &&
    l0.outputs[0].n === 0 &&
    l0.outputs[0].count === 120 &&
    l0.proj.proj1d === "index" &&
    l0.map.installed === false,
  JSON.stringify(l0),
);

const lStrip = await postLayout(base, "strip 240\nproj1d x\nproj3d yz");
check(
  "layout: `strip N` resizes live and echoes the whole object",
  lStrip.ok === true &&
    lStrip.reboot_required === false &&
    lStrip.pixels === 240 &&
    lStrip.w === 240 &&
    lStrip.proj.proj1d === "x" &&
    lStrip.proj.proj3d === "yz",
  JSON.stringify(lStrip),
);
await sleep(400);
check(
  "layout: /api/status geom follows a POST without a reboot",
  (await (await fetch(`${base}/api/status`)).json()).geom.w === 240,
);
check(
  "layout: /api/config is an alias of the same pixel count",
  (await (await fetch(`${base}/api/config`)).json()).pixels === 240,
);

const lMatrix = await postLayout(base, "matrix 32 16 2 1 tl row 1 0 16");
check(
  "layout: `matrix` derives the grid, keeps the arrangement, needs a reboot",
  lMatrix.ok === true &&
    lMatrix.reboot_required === true &&
    lMatrix.kind === "matrix" &&
    lMatrix.dims === 2 &&
    lMatrix.w === 64 &&
    lMatrix.h === 16 &&
    lMatrix.pixels === 1024 &&
    lMatrix.matrix.cols === 2 &&
    lMatrix.matrix.snake === 1 &&
    lMatrix.matrix.scan === 16 &&
    lMatrix.outputs[0].count === 2 && // panels, not pixels
    lMatrix.map.kind === "grid" &&
    lMatrix.map.w === 64,
  JSON.stringify(lMatrix),
);
await sleep(400);
check(
  "layout: the matrix grid reached /api/map too",
  (await (await fetch(`${base}/api/map`)).json()).w === 64,
);

const lMap = await postLayout(base, "map grid 8 8");
check(
  "layout: `map grid W H` is the /api/map wire, source flips to map, pixels follow",
  lMap.kind === "map" && lMap.source === "map" && lMap.w === 8 && lMap.h === 8 && lMap.pixels === 64,
  JSON.stringify(lMap),
);
await sleep(400);
check(
  "layout: the map grid resized the pixel space through /api/config too",
  (await (await fetch(`${base}/api/config`)).json()).pixels === 64,
);
// the alias keeps its own contract: `POST /api/map` never resized a strip
await fetch(`${base}/api/map`, { method: "POST", body: "grid 16 4" });
const lAfterMapAlias = await (await fetch(`${base}/api/layout`)).json();
check(
  "layout: a POST /api/map alias keeps kind map and reports the new shape",
  lAfterMapAlias.kind === "map" &&
    lAfterMapAlias.w === 16 &&
    lAfterMapAlias.h === 4 &&
    lAfterMapAlias.pixels === 64,
  JSON.stringify(lAfterMapAlias),
);

// a one-output mirror must refuse a second output outright (#474): caps is
// what makes the Settings Outputs table appear, so the two have to agree
const caps1 = (await (await fetch(`${base}/api/status`)).json()).caps;
check("caps.outputs is 1 on a default mirror", caps1.outputs === 1, JSON.stringify(caps1));
const refused = await postLayout(base, "strip 120\nout 1 18 ws2812 grb 120");
check(
  "layout: a board with one output refuses `out 1`",
  refused.ok === false && refused.line === 2,
  JSON.stringify(refused),
);

// outputs: stored, validated, reported; the firmware drives one run per
// output (#474) — the mirror models the table they are driven from
await postLayout(base, "strip 120");
const outs2 = spawn(
  "target/debug/luxel",
  ["serve", "--port", String(PORT + 2), "--pixels", "120", "--outputs", "2"],
  { stdio: ["ignore", "pipe", "inherit"] },
);
process.on("exit", () => outs2.kill());
await new Promise((resolve, reject) => {
  outs2.stdout.on("data", (d) => { if (String(d).includes("luxel serve:")) resolve(); });
  outs2.on("exit", () => reject(new Error("2-output mirror died")));
  setTimeout(() => reject(new Error("2-output mirror start timeout")), 30000);
});
const outsBase = `http://127.0.0.1:${PORT + 2}`;
await sleep(500);
const lOuts = await postLayout(
  outsBase,
  "strip 120\nout 0 18 ws2812 grb 60\nout 1 19 sk9822 rgb 60 rev",
);
check(
  "layout: two outputs partition the pixel space and need a reboot",
  lOuts.ok === true &&
    lOuts.reboot_required === true &&
    lOuts.outputs.length === 2 &&
    lOuts.outputs[0].pin === 18 &&
    lOuts.outputs[0].proto === "ws2812" &&
    lOuts.outputs[1].order === "rgb" &&
    lOuts.outputs[1].rev === true,
  JSON.stringify(lOuts.outputs),
);
check(
  "layout: output 0 writes through to /api/protocol",
  (await (await fetch(`${outsBase}/api/protocol`)).json()).protocol === "ws2812",
);
const sOuts = await (await fetch(`${outsBase}/api/status`)).json();
check(
  "caps.outputs follows --outputs, so the Outputs table is offered",
  sOuts.caps.outputs === 2,
  JSON.stringify(sOuts.caps),
);
const lOutsGet = await (await fetch(`${outsBase}/api/layout`)).json();
check(
  "layout: GET reports the same table the POST echoed, field for field",
  JSON.stringify(lOutsGet.outputs) === JSON.stringify(lOuts.outputs),
  JSON.stringify(lOutsGet.outputs),
);
const lRev = await postLayout(outsBase, "strip 120\nout 0 18 ws2812 grb 120 rev");
check(
  "layout: one output can be the whole space, wired backwards",
  lRev.ok === true && lRev.outputs.length === 1 && lRev.outputs[0].rev === true,
  JSON.stringify(lRev.outputs),
);
await postLayout(outsBase, "strip 120\nout 0 18 ws2812 grb 60\nout 1 19 sk9822 rgb 60 rev");
const badCases = [
  ["strip 0", 1, "pixel count out of range"],
  ["strip 99999", 1, "past the board ceiling"],
  ["strip 60\nwibble 1", 2, "unknown line"],
  ["strip 60\nout 0 18 apa106 grb 60", 2, "unknown protocol"],
  ["strip 60\nout 5 18 ws2812 grb 60", 2, "output index past caps.outputs"],
  ["strip 60\nproj1d sideways", 2, "unknown projection token"],
  ["strip 120\nout 0 18 ws2812 grb 60", 0, "counts must add up"],
  ["strip 60\nmatrix 8 8 1 1 tl row 0 0", 2, "one kind line per body"],
  ["strip 120\nout 0 18 ws2812 grb 60\nout 1 18 ws2812 grb 60", 3, "two outputs on one pad"],
];
for (const [body, line, why] of badCases) {
  const r = await postLayout(outsBase, body);
  check(`layout: rejects ${why} at line ${line}`, r.ok === false && r.line === line, JSON.stringify(r));
}
const lUnchanged = await (await fetch(`${outsBase}/api/layout`)).json();
check(
  "layout: a rejected body changes nothing",
  lUnchanged.pixels === 120 && lUnchanged.outputs.length === 2,
  JSON.stringify(lUnchanged),
);
const lCleared = await postLayout(outsBase, "out none");
check(
  "layout: `out none` goes back to the one implicit output",
  lCleared.ok === true && lCleared.outputs.length === 1 && lCleared.outputs[0].count === 120,
  JSON.stringify(lCleared.outputs),
);
outs2.kill();

// ---- --pixels past the ceiling is an error, not a silent clamp (#495) ----
const over = spawn(
  "target/debug/luxel",
  ["serve", "--port", String(PORT + 3), "--pixels", "4096"],
  { stdio: ["ignore", "pipe", "pipe"] },
);
let overErr = "";
over.stderr.on("data", (d) => (overErr += String(d)));
const overCode = await new Promise((r) => over.on("exit", r));
check(
  "--pixels past the board ceiling exits with the reason named",
  overCode !== 0 && /--max-pixels/.test(overErr),
  `code=${overCode} ${overErr.trim()}`,
);

server.kill();
console.log(failures === 0 ? "\nall checks passed" : `\n${failures} FAILURES`);
process.exit(failures === 0 ? 0 : 1);
