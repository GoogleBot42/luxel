// Smoke test of `luxel serve` (the native mirror of the firmware server):
// its HTTP API plus page routing — `/` serves the built playground when
// present (else the minimal fallback), `/min` always the minimal page. The
// full playground UI is driven in a real browser by web/tools/device-e2e.mjs;
// this is the fast, dependency-light check. Run from the repo root:
//   node tools/serve-e2e.mjs

import { execSync, spawn } from "node:child_process";
import { rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { PORT as E2E } from "../web/tools/e2e-common.mjs";
import { lxpBody } from "../web/tools/lxp.mjs"; // needs web/public/luxel.wasm (npm run wasm)

// `E2E_PORT + 70` (and +71..+74 for the extra mirrors it spawns), so a concurrent
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
    status.caps.text_slots === 8 &&
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


// ---- scenes: /api/scenes + playlist scene items (Gitea #478) ----
// The mirror is the reference implementation the web app develops against,
// so every route the firmware grows is driven here first: create, list, get,
// replace, activate, the playlist `I S<id>` item, delete, and both refusals
// (a bad block, and a store that would not fit in the device's 3840 B).
const scRed = await savePat("SC red", "export function render(index) { hsv(0, 1, 1) }");
const scGreen = await savePat("SC green", "export function render(index) { hsv(1/3, 1, 1) }");

const emptyScenes = await (await fetch(`${base}/api/scenes`)).json();
check(
  "scenes: an empty store reports the budget and the layer cap",
  emptyScenes.active === null &&
    emptyScenes.layers_max === 3 &&
    emptyScenes.used === 0 &&
    emptyScenes.max === 3840 &&
    Array.isArray(emptyScenes.scenes) &&
    emptyScenes.scenes.length === 0,
  JSON.stringify(emptyScenes),
);

const postScene = async (body, id = "") =>
  await (
    await fetch(`${base}/api/scenes${id ? `/${id}` : ""}`, { method: "POST", body })
  ).json();

// (1) create — `S -` means "assign me an id"
const made = await postScene(
  `S - wash\nL color 0 0 0 0 normal 100 none fill 1\nK ff8800\n`,
);
check(
  "scenes: POST /api/scenes assigns an 8-hex id",
  made.ok === true && /^[0-9a-f]{8}$/.test(made.id ?? ""),
  JSON.stringify(made),
);
const washId = made.id;

// (2) list + (3) get one — the same JSON object either way
const listed = await (await fetch(`${base}/api/scenes`)).json();
const one = await (await fetch(`${base}/api/scenes/${washId}`)).json();
check(
  "scenes: GET lists the record and GET /<id> returns the same object",
  listed.scenes.length === 1 &&
    listed.used > 0 &&
    JSON.stringify(listed.scenes[0]) === JSON.stringify(one) &&
    one.name === "wash" &&
    one.layers.length === 1 &&
    one.layers[0].type === "color" &&
    one.layers[0].color === "ff8800",
  JSON.stringify(one),
);
check(
  "scenes: GET /<id> of a scene that isn't there",
  (await (await fetch(`${base}/api/scenes/deadbeef`)).json()).error === "no such scene",
);

// (4) replace in place — the route's id wins over the block's `S -`
const replaced = await postScene(
  `S - wash 2\nL color 0 0 0 0 normal 100 none fill 1\nK 0000ff\n`,
  washId,
);
const afterReplace = await (await fetch(`${base}/api/scenes`)).json();
check(
  "scenes: POST /<id> replaces in place, keeping the id",
  replaced.ok === true &&
    replaced.id === washId &&
    afterReplace.scenes.length === 1 &&
    afterReplace.scenes[0].name === "wash 2" &&
    afterReplace.scenes[0].layers[0].color === "0000ff",
  JSON.stringify(afterReplace.scenes),
);
check(
  "scenes: POST /<id> of a scene that isn't there",
  (await postScene(`S - nope\nL color 0 0 0 0 normal 100 none fill 1\n`, "deadbeef")).error ===
    "no such scene",
);

// (5) activate — the scene goes on screen and `active` names it
await fetch(`${base}/api/scenes/${washId}/activate`, { method: "POST", body: "" });
await sleep(400);
const washPx = new Uint8Array(await (await fetch(`${base}/api/pixels`)).arrayBuffer());
const activeAfter = await (await fetch(`${base}/api/scenes`)).json();
check(
  "scenes: activate puts the colour layer on screen",
  activeAfter.active === washId && washPx[0] === 0 && washPx[1] === 0 && washPx[2] === 255,
  `active=${activeAfter.active} px=${[...washPx.slice(0, 3)]}`,
);

// (6) a TWO-pattern scene composites through the Compositor: red under
// green at `add` 50% is ff7f00, and the render loop keeps its frame rate
const two = await postScene(
  `S - stack\nL pat 0 0 0 0 normal 100 none fill 1\nI ${scRed}\n` +
    `L pat 0 0 0 0 add 50 none fill 1\nI ${scGreen}\n`,
);
await fetch(`${base}/api/scenes/${two.id}/activate`, { method: "POST", body: "" });
await sleep(1200);
const stackPx = new Uint8Array(await (await fetch(`${base}/api/pixels`)).arrayBuffer());
const stackStatus = await (await fetch(`${base}/api/status`)).json();
check(
  "scenes: two pattern layers composite (red + green at add 50%)",
  stackPx[0] === 0xff && stackPx[1] === 0x7f && stackPx[2] === 0x00,
  [...stackPx.slice(0, 3)].join(","),
);
check(
  "scenes: a two-layer scene still renders at a frame rate",
  stackStatus.fps > 0 && stackStatus.vmerr === null,
  `fps=${stackStatus.fps} vmerr=${stackStatus.vmerr}`,
);

// (7) playlist scene item — `I S<id> <sec>`; `C`/`P` under it are ignored
await fetch(`${base}/api/playlist`, {
  method: "POST",
  body: `D 0\nX 0\nI S${two.id} 0\nC nope 1\nP x\nI ${scRed} 0\n`,
});
const plScenes = await (await fetch(`${base}/api/playlist`)).json();
check(
  "playlist: `I S<id>` is a scene item, and pattern items say so too",
  plScenes.items.length === 2 &&
    plScenes.items[0].kind === "scene" &&
    plScenes.items[0].id === two.id &&
    plScenes.items[0].name === "stack" &&
    plScenes.items[0].layers === 2 &&
    plScenes.items[0].controls === undefined &&
    plScenes.items[0].proj === undefined &&
    plScenes.items[1].kind === "pattern" &&
    plScenes.items[1].id === scRed,
  JSON.stringify(plScenes.items),
);
// an id that is not 8 hex after the S is a PATTERN id, so no playlist a
// pre-#478 device wrote changes meaning
await fetch(`${base}/api/playlist`, { method: "POST", body: `D 0\nI Sabc 0\n` });
const plNotScene = await (await fetch(`${base}/api/playlist`)).json();
check(
  "playlist: a bare `S` prefix that isn't an id stays a pattern item",
  plNotScene.items[0].kind === "pattern" && plNotScene.items[0].id === "Sabc",
  JSON.stringify(plNotScene.items),
);

// entering a scene item activates the scene
await fetch(`${base}/api/playlist`, {
  method: "POST",
  body: `D 0\nX 0\nI ${scRed} 0\nI S${two.id} 0\n`,
});
await fetch(`${base}/api/playlist/play`, { method: "POST", body: "1" });
await sleep(500);
const plPx = new Uint8Array(await (await fetch(`${base}/api/pixels`)).arrayBuffer());
check(
  "playlist: entering a scene item activates the scene",
  plPx[0] === 0xff && plPx[1] === 0x7f && plPx[2] === 0x00,
  [...plPx.slice(0, 3)].join(","),
);

// (8) delete drops the record AND every playlist item that named it
const del = await (await fetch(`${base}/api/scenes/${two.id}`, { method: "DELETE" })).json();
await sleep(200);
const plAfterDel = await (await fetch(`${base}/api/playlist`)).json();
const scAfterDel = await (await fetch(`${base}/api/scenes`)).json();
check(
  "scenes: DELETE removes the record and its playlist items",
  del.ok === true &&
    scAfterDel.scenes.every((s) => s.id !== two.id) &&
    plAfterDel.items.length === 1 &&
    plAfterDel.items[0].id === scRed,
  JSON.stringify({ del, items: plAfterDel.items }),
);
check(
  "scenes: DELETE of a scene that isn't there",
  (await (await fetch(`${base}/api/scenes/${two.id}`, { method: "DELETE" })).json()).error ===
    "no such scene",
);

// (9) a bad block is refused with the core parser's own message
const badBlend = await postScene(`S - bad\nL pat 0 0 0 0 nosuch 100 none fill 1\n`);
check(
  "scenes: a parse error names the line and the token",
  badBlend.ok === false && badBlend.error === 'scene: line 2: unknown blend "nosuch"',
  JSON.stringify(badBlend),
);
check(
  "scenes: more pat layers than caps.layers is refused",
  (
    await postScene(
      `S - toomany\n` + `L pat 0 0 0 0 normal 100 none fill 1\n`.repeat(4),
    )
  ).error === "scene: layer 4 does not fit",
);

// (10) the store is one 3840 B blob on a device, and the mirror enforces it
// against the same serializer — a write that would not fit is REFUSED, with
// the size it would have taken
let storeFull = null;
for (let i = 0; i < 80 && !storeFull; i++) {
  const r = await postScene(
    `S - filler ${i}\n` + `L color 0 0 0 0 normal 100 none fill 1\nK 112233\n`.repeat(6),
  );
  if (r.ok === false) storeFull = r;
}
check(
  "scenes: a write past the 3840 B blob is refused, not truncated",
  storeFull !== null && /^scenes: store full \(\d+ of 3840 B\)$/.test(storeFull?.error ?? ""),
  JSON.stringify(storeFull),
);
const full = await (await fetch(`${base}/api/scenes`)).json();
check("scenes: `used` stays inside the budget", full.used <= 3840, `used=${full.used}`);

// clean up: the routing checks below want a quiet mirror
for (const s of full.scenes) await fetch(`${base}/api/scenes/${s.id}`, { method: "DELETE" });
await fetch(`${base}/api/playlist`, { method: "POST", body: "D 0" });
for (const id of [scRed, scGreen]) await fetch(`${base}/api/patterns/${id}`, { method: "DELETE" });

// ---- text slots: GET/POST /api/text (Gitea #485) ----
const text0 = await (await fetch(`${base}/api/text`)).json();
check(
  "text: eight empty slots, matching caps.text_slots",
  Array.isArray(text0.slots) && text0.slots.length === 8 && text0.slots.every((s) => s === ""),
  JSON.stringify(text0),
);
await fetch(`${base}/api/text`, { method: "POST", body: "3 hello there" });
const text1 = await (await fetch(`${base}/api/text`)).json();
check(
  "text: POST `<slot> <text>` round-trips, rest-of-line and all",
  text1.slots[3] === "hello there" && text1.slots[0] === "",
  JSON.stringify(text1.slots),
);
await fetch(`${base}/api/text`, { method: "POST", body: "3 " });
check(
  "text: an empty body clears the slot",
  (await (await fetch(`${base}/api/text`)).json()).slots[3] === "",
);
// 64 B, truncated on a char boundary: 63 ASCII + a 2-byte é would be 65, so
// the é is dropped whole rather than split
await fetch(`${base}/api/text`, { method: "POST", body: `0 ${"x".repeat(70)}` });
const longSlot = (await (await fetch(`${base}/api/text`)).json()).slots[0];
await fetch(`${base}/api/text`, { method: "POST", body: `1 ${"y".repeat(63)}é` });
const utf8Slot = (await (await fetch(`${base}/api/text`)).json()).slots[1];
check(
  "text: a slot holds 64 bytes, truncated on a char boundary",
  longSlot === "x".repeat(64) &&
    utf8Slot === "y".repeat(63) &&
    new TextEncoder().encode(utf8Slot).length === 63,
  `${longSlot.length} ${utf8Slot.length}`,
);
check(
  "text: a slot number past the table is refused",
  (await (await fetch(`${base}/api/text`, { method: "POST", body: "8 nope" })).json()).error ===
    "text: slot 8 out of range (0..7)",
);
for (const n of [0, 1]) await fetch(`${base}/api/text`, { method: "POST", body: `${n} ` });
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
// fallback page — either is a valid mirror state, so accept both. Since
// Gitea #592 the built page INLINES its script and stylesheet, so an
// `/assets/` reference no longer identifies it; the fallback names itself,
// so "not the fallback, and it is the app's page" is the honest probe.
const minimalFallback = /isn['’]t installed/.test(rootBody);
const builtUi = !minimalFallback && /<title>Luxel<\/title>/.test(rootBody);
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

// ---- a panel mirror enforces the PANEL's array ledger (Gitea #253/#420) ----
// PB's 10,236-element ledger is a memory budget in disguise; a board with an
// external array arena has a real byte budget and raises the count out of the
// way. A mirror that reports `psram_total` and still enforced PB's number
// answered `/api/pixels` all-zeroes for patterns the panel shows —
// `library/fairies.js` (15,104 elements at 4096 px) is the one that found it.
const CHANNELS_4096 = [
  "var a = array(pixelCount)",
  "var b = array(pixelCount)",
  "var c = array(pixelCount)",
  "export function render(i) { a[i] = 1; b[i] = 1; c[i] = 1; rgb(1, 1, 1) }",
].join("\n");
await fetch(`${panelBase}/api/code`, { method: "POST", body: await lxpBody("", CHANNELS_4096) });
await sleep(600);
const ledger = await (await fetch(`${panelBase}/api/status`)).json();
const litPx = new Uint8Array(
  await (await fetch(`${panelBase}/api/pixels`)).arrayBuffer(),
).reduce((n, v) => n + (v ? 1 : 0), 0);
check(
  "--board panel: three array(pixelCount) channels load and RENDER at 4096 px",
  ledger.vmerr === null && litPx === 4096 * 3,
  `vmerr=${ledger.vmerr} lit=${litPx}`,
);

// ---- projection applies LIVE, on all three paths (Gitea #538/#598) ----
//
// The assertion is the FRAME, not the echo: a 1D pattern whose pixel is a
// pure function of `index` draws a hue ramp, so on a 64x64 panel
//   along x  → every row identical (the strip runs along x, replicated down y)
//   along y  → every column identical
//   by index → neither.
// Nothing here reboots or re-activates anything: each POST has to land on
// the engine that is already running.
async function projShape(b) {
  const buf = Buffer.from(await (await fetch(`${b}/api/pixels`)).arrayBuffer());
  const px = [];
  for (let i = 0; i < buf.length / 3; i++) px.push(`${buf[i * 3]},${buf[i * 3 + 1]},${buf[i * 3 + 2]}`);
  const row = (y) => px.slice(y * 64, (y + 1) * 64).join("|");
  const col = (x) => Array.from({ length: 64 }, (_, y) => px[y * 64 + x]).join("|");
  const rows = row(0) === row(1) && row(0) === row(63);
  const cols = col(0) === col(1) && col(0) === col(63);
  return rows && !cols ? "along-x" : cols && !rows ? "along-y" : "index";
}
const RAMP_1D = "export function render(index) { hsv(index / pixelCount, 1, 1) }";
await fetch(`${panelBase}/api/code`, { method: "POST", body: await lxpBody("", RAMP_1D) });
await sleep(400);
check("projection: a 1D pattern starts on the default (by index)", (await projShape(panelBase)) === "index");

// (a) the DEVICE DEFAULT — `POST /api/layout proj1d …`, the Settings cards
for (const [mode, want] of [["x", "along-x"], ["y", "along-y"], ["index", "index"]]) {
  const r = await postLayout(panelBase, `proj1d ${mode}`);
  await sleep(400);
  const got = await projShape(panelBase);
  check(`projection: \`proj1d ${mode}\` applies live, no reboot`, r.ok === true && r.reboot_required === false && got === want, got);
}

// (b) the PER-PATTERN OVERRIDE — a `proj` line, the editor's row.
// It outranks the device default and is NOT persisted into the Layout.
await postLayout(panelBase, "proj1d x");
await sleep(400);
const pj = await postLayout(panelBase, "proj y");
await sleep(400);
const ovr = await projShape(panelBase);
const layoutAfter = await (await fetch(`${panelBase}/api/layout`)).json();
check(
  "projection: an override outranks the device default, live",
  pj.ok === true && pj.reboot_required === false && ovr === "along-y" && layoutAfter.proj.proj1d === "x",
  `${ovr} / layout ${layoutAfter.proj.proj1d}`,
);
const pjd = await postLayout(panelBase, "proj default");
await sleep(400);
check(
  "projection: `proj default` clears the override back to the device's",
  pjd.ok === true && (await projShape(panelBase)) === "along-x",
);
const pjBad = await postLayout(panelBase, "proj sideways");
check("projection: an unknown token is refused and changes nothing", pjBad.ok === false && (await projShape(panelBase)) === "along-x", JSON.stringify(pjBad));

// (c) the PLAYLIST ITEM's `P` — applied when the item activates, on top of
// the device default the fresh engine starts from.
const pid = (
  await (await fetch(`${panelBase}/api/patterns`, { method: "POST", body: await lxpBody("Ramp 1D", RAMP_1D) })).json()
).id;
await fetch(`${panelBase}/api/playlist`, { method: "POST", body: `D 0\nI ${pid} -1\nP y\n` });
await fetch(`${panelBase}/api/playlist/play`, { method: "POST", body: "0" });
await sleep(600);
check("projection: a playlist item's `P` is applied on activation", (await projShape(panelBase)) === "along-y");
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
// …and writing the ONE implicit output down explicitly rebuilds nothing, so
// a Settings page that always POSTs the whole table gets no reboot prompt
// for it (#550). The mirror's implicit output is on pin 0 (it has no pad).
const lImplicit = await postLayout(base, "strip 120\nout 0 0 ws2812 grb 120");
check(
  "layout: spelling out the implicit output is not a reboot",
  lImplicit.ok === true && lImplicit.reboot_required === false,
  JSON.stringify(lImplicit),
);
check(
  "layout: `out none` back to the same implicit output is not a reboot",
  (await postLayout(base, "out none")).reboot_required === false,
);
const lImplicitPin = await postLayout(base, "strip 120\nout 0 4 ws2812 grb 120");
check(
  "layout: moving the implicit output's pad IS a reboot",
  lImplicitPin.ok === true && lImplicitPin.reboot_required === true,
  JSON.stringify(lImplicitPin.outputs),
);
check(
  "layout: …and so is dropping back off it",
  (await postLayout(base, "out none")).reboot_required === true,
);
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
// Which parts of an `out` line a reboot actually builds (#550). Each POST
// below starts from the table the previous one left, so the ONE field named
// is the only thing that moved.
const reboots = [
  // re-partition, keeping every wiring field
  ["out 0 18 ws2812 grb 90\nout 1 19 sk9822 rgb 30 rev", false, "a re-split is live"],
  // rev, on each output in turn
  ["out 0 18 ws2812 grb 90 rev\nout 1 19 sk9822 rgb 30", false, "reversing a run is live"],
  // output 0's protocol and colour order ARE the strip's
  ["out 0 18 sk9822 bgr 90 rev\nout 1 19 sk9822 rgb 30", false, "output 0's proto/order are live"],
  // a further output's are captured when its peripheral is built
  ["out 0 18 sk9822 bgr 90 rev\nout 1 19 ws2812 rgb 30", true, "output 1's protocol needs a boot"],
  ["out 0 18 sk9822 bgr 90 rev\nout 1 19 ws2812 bgr 30", true, "output 1's order needs a boot"],
  // a pad binds once, on either output
  ["out 0 17 sk9822 bgr 90 rev\nout 1 19 ws2812 bgr 30", true, "moving output 0's pad needs a boot"],
  ["out 0 17 sk9822 bgr 90 rev\nout 1 21 ws2812 bgr 30", true, "moving output 1's pad needs a boot"],
];
for (const [body, want, why] of reboots) {
  const r = await postLayout(outsBase, body);
  check(`layout: ${why}`, r.ok === true && r.reboot_required === want, JSON.stringify(r));
}
check(
  "layout: output 0's protocol reached /api/protocol without a reboot",
  (await (await fetch(`${outsBase}/api/protocol`)).json()).protocol === "sk9822",
);
check(
  "layout: output 0's colour order reached /api/output without a reboot",
  (await (await fetch(`${outsBase}/api/output`)).json()).order === "bgr",
);
const lRev = await postLayout(outsBase, "strip 120\nout 0 18 ws2812 grb 120 rev");
check(
  "layout: losing output 1's driver instance needs a reboot",
  lRev.reboot_required === true,
  JSON.stringify(lRev),
);
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
  "layout: `out none` goes back to the one implicit output, and rebuilds output 1's driver away",
  lCleared.ok === true &&
    lCleared.outputs.length === 1 &&
    lCleared.outputs[0].count === 120 &&
    lCleared.reboot_required === true,
  JSON.stringify(lCleared),
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

// ---- a panel mirror's scene budget + `--scenes` seeding (Gitea #478) ----
// `caps.layers` steps down past 512 px, so the S3 panel affords TWO pattern
// layers, not three — and the mirror must refuse a third at the door rather
// than let a scene into the store that the board could not show. `--scenes
// <file>` brings the store up pre-filled, which is how a harness (and the
// mockdiff seed) gets scenes onto a mirror without a POST per record.
const seedPath = `${tmpdir()}/luxel-serve-e2e-scenes-${process.pid}.txt`;
writeFileSync(
  seedPath,
  `S 5ceed001 seeded\nL color 0 0 0 0 normal 100 none fill 1\nK 00ff00\n`,
);
const panelSc = spawn(
  "target/debug/luxel",
  ["serve", "--port", String(PORT + 4), "--board", "panel", "--scenes", seedPath],
  { stdio: ["ignore", "pipe", "inherit"] },
);
process.on("exit", () => {
  panelSc.kill();
  try {
    rmSync(seedPath, { force: true });
  } catch {}
});
await new Promise((resolve, reject) => {
  panelSc.stdout.on("data", (d) => { if (String(d).includes("luxel serve:")) resolve(); });
  panelSc.on("exit", () => reject(new Error("panel scene mirror died")));
  setTimeout(() => reject(new Error("panel scene mirror start timeout")), 30000);
});
const pscBase = `http://127.0.0.1:${PORT + 4}`;
await sleep(600);
const pscList = await (await fetch(`${pscBase}/api/scenes`)).json();
check(
  "--scenes <file>: the store comes up seeded, at the panel's layer cap",
  pscList.layers_max === 2 &&
    pscList.scenes.length === 1 &&
    pscList.scenes[0].id === "5ceed001" &&
    pscList.scenes[0].name === "seeded",
  JSON.stringify(pscList),
);
check(
  "scenes: a panel refuses a THIRD pattern layer (caps.layers = 2)",
  (
    await (
      await fetch(`${pscBase}/api/scenes`, {
        method: "POST",
        body: `S - three\n` + `L pat 0 0 0 0 normal 100 none fill 1\n`.repeat(3),
      })
    ).json()
  ).error === "scene: layer 3 does not fit",
);
const pscRed = (await (await fetch(`${pscBase}/api/patterns`, {
  method: "POST",
  body: await lxpBody("PSC red", "export function render2D(index, x, y) { hsv(0, 1, 1) }"),
})).json()).id;
const pscGreen = (await (await fetch(`${pscBase}/api/patterns`, {
  method: "POST",
  body: await lxpBody("PSC green", "export function render2D(index, x, y) { hsv(1/3, 1, 1) }"),
})).json()).id;
const pscTwo = await (await fetch(`${pscBase}/api/scenes`, {
  method: "POST",
  body:
    `S - stack\nL pat 0 0 0 0 normal 100 none fill 1\nI ${pscRed}\n` +
    `L pat 0 0 0 0 add 50 none fill 1\nI ${pscGreen}\n`,
})).json();
await fetch(`${pscBase}/api/scenes/${pscTwo.id}/activate`, { method: "POST", body: "" });
await sleep(1500);
const pscPx = new Uint8Array(await (await fetch(`${pscBase}/api/pixels`)).arrayBuffer());
const pscStatus = await (await fetch(`${pscBase}/api/status`)).json();
check(
  "scenes: a 64x64 panel composites two pattern layers and keeps rendering",
  pscPx.length === 4096 * 3 &&
    pscPx[0] === 0xff &&
    pscPx[1] === 0x7f &&
    pscPx[2] === 0x00 &&
    pscStatus.fps > 0 &&
    pscStatus.vmerr === null,
  `px=${[...pscPx.slice(0, 3)]} fps=${pscStatus.fps} vmerr=${pscStatus.vmerr}`,
);
// A `text` layer draws NATIVELY — no engine, no layer slot — and its `slot`
// source is resolved by the host every frame, so `POST /api/text` changes
// what is on the panel with no other call (Gitea #484/#485).
const litScene = await (await fetch(`${pscBase}/api/scenes`, {
  method: "POST",
  body: `S - words\nL text 0 0 0 0 normal 100 none fill 1\nT lit HI\nF regular ffffff l none 0\n`,
})).json();
await fetch(`${pscBase}/api/scenes/${litScene.id}/activate`, { method: "POST", body: "" });
await sleep(600);
const textLitPx = new Uint8Array(await (await fetch(`${pscBase}/api/pixels`)).arrayBuffer());
check(
  "scenes: a `lit` text layer draws glyphs with no engine",
  textLitPx.some((b) => b > 0),
  `lit px on: ${textLitPx.filter((b) => b > 0).length}`,
);

const slotScene = await (await fetch(`${pscBase}/api/scenes`, {
  method: "POST",
  body: `S - slotted\nL text 0 0 0 0 normal 100 none fill 1\nT slot 0\nF regular ffffff l none 0\n`,
})).json();
await fetch(`${pscBase}/api/text`, { method: "POST", body: "0 " });
await fetch(`${pscBase}/api/scenes/${slotScene.id}/activate`, { method: "POST", body: "" });
await sleep(600);
const slotEmpty = new Uint8Array(await (await fetch(`${pscBase}/api/pixels`)).arrayBuffer());
await fetch(`${pscBase}/api/text`, { method: "POST", body: "0 LUXEL" });
await sleep(600);
const slotFilled = new Uint8Array(await (await fetch(`${pscBase}/api/pixels`)).arrayBuffer());
check(
  "scenes: a `slot` text layer follows POST /api/text with no other call",
  slotEmpty.every((b) => b === 0) && slotFilled.some((b) => b > 0),
  `empty=${slotEmpty.filter((b) => b > 0).length} filled=${slotFilled.filter((b) => b > 0).length}`,
);

panelSc.kill();
rmSync(seedPath, { force: true });

server.kill();
console.log(failures === 0 ? "\nall checks passed" : `\n${failures} FAILURES`);
process.exit(failures === 0 ? 0 : 1);
