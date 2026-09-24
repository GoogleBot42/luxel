// Drive the built playground in real chromium (nix dev shell) via
// puppeteer-core. The app opens on the Patterns page; the editor is a
// full-screen view entered via "New pattern" or by picking a library tile.
//
// Usage (from web/): npm run build && node tools/e2e.mjs [screenshot-dir]

import { execSync, spawn } from "node:child_process";
import fs from "node:fs";
import puppeteer from "puppeteer-core";
import {
  acceptDialog,
  cancelDialog,
  dialogTitle,
  disabledSweep,
  menuClick,
  menuHas,
  PORT as E2E,
  renameTo,
  saveState,
  waitDialog,
} from "./e2e-common.mjs";

const CHROMIUM =
  process.env.CHROMIUM ?? execSync("command -v chromium", { encoding: "utf8" }).trim();

const shotDir = process.argv[2] ?? "/tmp";
// A non-existent shot dir used to surface as a bare ENOENT on the first
// screenshot write, mid-suite, looking like a puppeteer failure (#224).
fs.mkdirSync(shotDir, { recursive: true });
const PORT = E2E.web.e2e; // E2E_PORT + 0 (see tools/e2e-common.mjs)

const server = spawn("npx", ["vite", "preview", "--port", String(PORT), "--strictPort"], {
  stdio: "ignore",
});
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
await sleep(1500);

const browser = await puppeteer.launch({
  executablePath: CHROMIUM,
  headless: true,
  args: [
    "--no-sandbox",
    "--disable-gpu",
    "--window-size=1400,900",
    // fake mic (a generated tone) + auto-grant, for the sound-reactive check
    "--use-fake-device-for-media-stream",
    "--use-fake-ui-for-media-stream",
  ],
});

const fails = [];
const check = (name, cond, detail = "") => {
  console.log(`${cond ? " ok " : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`);
  if (!cond) fails.push(name);
};

/** Replace the (visible) pattern editor's contents by typing. */
/** The code pane of whichever editor screen is on top: the pattern editor, or
 *  the map program's own screen (A10, #471). Both are mounted at all times;
 *  the hidden one's `<main>` carries `hidden`. */
const VISIBLE_CODE = "main.editor-frame:not([hidden]) .editor-slot";

async function setEditor(page, text) {
  await page.click(`${VISIBLE_CODE} .cm-content`);
  await page.keyboard.down("Control");
  await page.keyboard.press("a");
  await page.keyboard.up("Control");
  await page.keyboard.press("Backspace");
  await page.keyboard.type(text, { delay: 0 });
  await sleep(300);
}

/** Replace the editor's contents with a real PASTE, which the rig derivation
 *  treats differently from typing (Gitea #372). Chromium refuses
 *  `navigator.clipboard.writeText` even with the permission overridden, and
 *  headless has no system clipboard to prime, so the paste is delivered as a
 *  ClipboardEvent carrying a DataTransfer — CodeMirror's own paste handler
 *  runs, and the transaction is a genuine `input.paste` user event, which is
 *  what the app keys off. */
async function pasteEditor(page, text) {
  await page.click(`${VISIBLE_CODE} .cm-content`);
  await page.keyboard.down("Control");
  await page.keyboard.press("a");
  await page.keyboard.up("Control");
  await page.keyboard.press("Backspace");
  await page.$eval(
    `${VISIBLE_CODE} .cm-content`,
    (el, t) => {
      const dt = new DataTransfer();
      dt.setData("text/plain", t);
      el.dispatchEvent(
        new ClipboardEvent("paste", { clipboardData: dt, bubbles: true, cancelable: true }),
      );
    },
    text,
  );
  await sleep(500);
}

// The playground's Layout (Gitea #463): the header chip chooses it, the
// preview's `data-shape` and the chip's label report it. The old rig
// dropdown / px / W×H fields in the playback bar are gone — they were the
// rig config, and geometry is not per-pattern any more.
const rig = async (page) => ({
  shape: await page.$eval('[data-role="editor-view"] [data-role="preview"]', (el) => el.dataset.shape ?? ""),
  label: await page
    .$eval('[data-role="preview-as-label"]', (el) => (el.textContent ?? "").trim())
    .catch(() => ""),
});

/** Pick a "Preview as" option, optionally typing its numbers first. */
async function previewAs(page, choice, nums = {}) {
  await page.click('[data-role="preview-as"]');
  await page.waitForSelector('[data-role="preview-as-menu"]', { timeout: 3000 });
  for (const [role, value] of Object.entries(nums)) {
    await page.$eval(
      `[data-role="preview-as-${role}"]`,
      (el, v) => {
        el.value = String(v);
        el.dispatchEvent(new Event("change", { bubbles: true }));
      },
      value,
    );
    await sleep(150);
  }
  await page.click(`[data-role="preview-as-${choice}"]`);
  await sleep(150);
  await page.keyboard.press("Escape"); // the popover dismisses on Escape (#538)
  await sleep(400);
}

// The Patterns page (#467) keeps ONE grid per source mounted and hides the
// inactive ones, so an unscoped `.tile` would also match tiles the user
// cannot see. Everything that counts or clicks tiles goes through these.
const GRID = '[data-role="patterns-grid"]:not([hidden])';
const TILE = `${GRID} .tile`;
const MINE = '[data-role="patterns-grid"][data-source="mine"]';

/** Switch the Patterns page's source control and let the grid settle. */
async function pickSource(page, id) {
  await page.click(`[data-role="patterns-source-${id}"]`);
  await sleep(350);
}

try {
  const page = await browser.newPage();
  await page.setViewport({ width: 1400, height: 900 });
  const pageErrors = [];
  // No `page.on("dialog")` handler on purpose: naming and confirmations are
  // in-app dialogs (Gitea #472). A native prompt/confirm reaching the browser
  // would hang the run, which is exactly the regression we want.
  page.on("pageerror", (e) => pageErrors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") pageErrors.push(m.text());
  });

  await page.goto(`http://localhost:${PORT}/`, { waitUntil: "networkidle0" });
  await sleep(900); // wasm load

  // ── 1. lands on the Patterns page (not the editor) ──
  check("opens on the Patterns page", (await page.$('[data-role="patterns-panel"]:not([hidden])')) !== null);
  check("has a New pattern button", (await page.$('[data-role="new-pattern"]')) !== null);
  check("the examples dropdown is gone", (await page.$('[data-role="pattern-picker"]')) === null);
  // One Patterns tab (#467): the library / corpus / saved split is a source
  // control on the page, not a tab set. Scenes joined it in the playground
  // (D10, #480) — it needs a 2D fixture, not a device, and hiding it here
  // would make the feature undiscoverable. Playlist and Settings still need
  // hardware and are still absent.
  const tabs = await page.$$eval('[data-role="tabs"] .tab', (e) => e.map((x) => x.textContent.trim()));
  check(
    "the playground's tabs are Patterns and Scenes",
    tabs.join(",") === "Patterns,Scenes",
    tabs.join(","),
  );
  const segs = await page.$$eval('[data-role="patterns-sources"] button', (e) =>
    e.map((x) => x.textContent.replace(/\s+/g, " ").trim()),
  );
  check(
    "playground source control is Library | Mine",
    /^Library /.test(segs[0] ?? "") && /^Mine /.test(segs[1] ?? ""),
    segs.join(" | "),
  );
  check(
    "no 'On device' source in the playground",
    (await page.$('[data-role="patterns-source-device"]')) === null,
  );
  await page
    .waitForFunction((sel) => document.querySelectorAll(sel).length > 150, { timeout: 8000 }, TILE)
    .catch(() => null);
  const tileCount = await page.$$eval(TILE, (els) => els.length);
  check("library shows examples + corpus", tileCount > 150, `${tileCount} tiles`);
  await page.screenshot({ path: `${shotDir}/e2e-1-library.png` });

  // ── 1b. tiles show a spinner until their preview draws its first frame ──
  // Tiles only start compiling once scrolled into view, so throttle the CPU
  // and jump to the (never-yet-visible) bottom of the list to catch the
  // transient loading state, then confirm it clears once frames land.
  const cdpTiles = await page.createCDPSession();
  await cdpTiles.send("Emulation.setCPUThrottlingRate", { rate: 6 });
  await page.$eval(GRID, (el) => {
    el.scrollTop = el.scrollHeight;
  });
  const sawSpinner = await page
    .waitForSelector('[data-role="tile-spinner"]', { timeout: 4000 })
    .then(() => true)
    .catch(() => false);
  check("tiles spin while their preview compiles", sawSpinner);
  await cdpTiles.send("Emulation.setCPUThrottlingRate", { rate: 1 });
  await page
    .waitForFunction(
      () => document.querySelectorAll('[data-role="tile-spinner"]').length === 0,
      { timeout: 10000 },
    )
    .catch(() => null);
  const spinLeft = await page.$$eval('[data-role="tile-spinner"]', (els) => els.length);
  check("spinners clear after the first frame", spinLeft === 0, `${spinLeft} left`);
  await cdpTiles.detach();
  await page.$eval(GRID, (el) => {
    el.scrollTop = 0;
  });
  await sleep(300);

  // gallery search filters the tiles by name
  await page.type('[data-role="gallery-search"]', "rainbow");
  await sleep(250);
  const visible = await page.$$eval(TILE, (els) => els.filter((e) => !e.hidden).length);
  const allMatch = await page.$$eval(TILE, (els) =>
    els.filter((e) => !e.hidden).every((e) => /rainbow/i.test(e.textContent ?? "")),
  );
  check("gallery search filters tiles", visible > 0 && visible < tileCount && allMatch, `${visible} shown`);
  await page.$eval('[data-role="gallery-search"]', (el) => {
    el.value = "";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await sleep(200);
  const backToAll = await page.$$eval(TILE, (els) => els.filter((e) => !e.hidden).length);
  check("gallery search clears", backToAll === tileCount, `${backToAll}`);

  // ── 1c. the tile IS the mock card (S1, docs/design/webui-v2/mockups.html) ──
  // Numbers quoted from `.tile` / `.tiles` / `.seg` there; Jeremy's review
  // called the shipped tile out by name, so they are asserted, not eyeballed.
  {
    const tile = await page.$eval(`${TILE}:not([hidden])`, (el) => {
      const c = getComputedStyle(el);
      const meta = el.querySelector(".meta");
      const nm = el.querySelector('[data-role="tile-name"]');
      const cv = el.querySelector("canvas");
      return {
        radius: c.borderTopLeftRadius,
        bg: c.backgroundColor,
        border: c.borderTopWidth,
        meta: meta ? getComputedStyle(meta).padding : "",
        nmSize: nm ? getComputedStyle(nm).fontSize : "",
        nmColor: nm ? getComputedStyle(nm).color : "",
        // full-bleed: the canvas spans the card's content box edge to edge
        bleed: cv ? Math.abs(cv.getBoundingClientRect().width - el.clientWidth) < 1 : false,
      };
    });
    check(
      "tile: mock card metrics (8px radius, 1px border, panel bg, 8/10px meta)",
      tile.radius === "8px" &&
        tile.border === "1px" &&
        tile.bg === "rgb(28, 31, 38)" &&
        tile.meta === "8px 10px 10px",
      JSON.stringify(tile),
    );
    check(
      "tile: 13px name in --text, over a full-bleed canvas",
      tile.nmSize === "13px" && tile.nmColor === "rgb(215, 218, 224)" && tile.bleed,
      JSON.stringify(tile),
    );
    const grid = await page.$eval(`${GRID} .tiles`, (el) => {
      const c = getComputedStyle(el);
      return { cols: c.gridTemplateColumns.split(" ").length, gap: c.gap, pad: c.padding };
    });
    check(
      "tiles: fixed columns, 16px gap, 20px padding (mock `.tiles`)",
      grid.cols === 6 && grid.gap === "16px" && grid.pad === "20px",
      JSON.stringify(grid),
    );
    // the segmented control: active = accent-soft ground, --text label, an
    // inset 2px amber underline, and the count in the accent (`.seg` in the
    // mock). Jeremy: "the colors don't match the mocks when activated".
    const seg = await page.$eval('[data-role="patterns-sources"] .segbtn.on', (el) => {
      const c = getComputedStyle(el);
      const ct = el.querySelector(".ct");
      return {
        bg: c.backgroundColor,
        color: c.color,
        shadow: c.boxShadow,
        h: c.height,
        ct: ct ? getComputedStyle(ct).color : "",
        ctFont: ct ? getComputedStyle(ct).fontSize : "",
      };
    });
    check(
      "seg: the active segment is accent-soft + --text + an inset amber rule",
      seg.bg === "rgba(232, 163, 61, 0.15)" &&
        seg.color === "rgb(215, 218, 224)" &&
        seg.shadow === "rgb(232, 163, 61) 0px -2px 0px 0px inset" &&
        seg.h === "32px",
      JSON.stringify(seg),
    );
    check(
      "seg: the active count is 11px mono in the accent",
      seg.ct === "rgb(232, 163, 61)" && seg.ctFont === "11px",
      JSON.stringify(seg),
    );
    // the page bar holds the segment, the search and the ONE primary — the
    // free-standing "N patterns" span is gone (the chip carries the count)
    check(
      "pagebar: no standalone pattern count (it lives in the chip)",
      (await page.$('[data-role="gallery-count"]')) === null,
    );
    const prim = await page.$eval('[data-role="new-pattern"]', (el) => {
      const c = getComputedStyle(el);
      return { bg: c.backgroundColor, color: c.color, h: c.height, w: c.fontWeight };
    });
    check(
      "pagebar: + New pattern is the filled primary",
      prim.bg === "rgb(232, 163, 61)" && prim.color === "rgb(22, 17, 10)" && prim.h === "32px",
      JSON.stringify(prim),
    );
  }

  // ── 1d. a fixture never offers a pattern it cannot show (#538) ──
  // The playground's Layout is a fixture only when the user picked one:
  // under Auto it follows the editor's pattern, so nothing is filtered.
  {
    // both numbers in ONE evaluate: a tile reclassifies the moment it
    // compiles (the regex guess yields to `preferredDims`), so two separate
    // reads can straddle that and disagree for no good reason
    const shownAndChip = (sel) =>
      page.evaluate((tileSel) => {
        const tiles = [...document.querySelectorAll(tileSel)];
        const ct = document.querySelector('[data-role="patterns-source-library"] .ct');
        return {
          shown: tiles.filter((e) => !e.hidden).length,
          chip: Number((ct?.textContent ?? "").trim()),
        };
      }, sel);
    const auto = await shownAndChip(TILE);
    check(
      "filter: Auto is not a fixture — nothing is hidden",
      auto.shown === tileCount,
      `${auto.shown}/${tileCount}`,
    );
    await previewAs(page, "strip", { px: 300 });
    await sleep(1500);
    const strip = await shownAndChip(TILE);
    check(
      "filter: a 300 px strip hides the 2D/3D library patterns",
      strip.shown > 0 && strip.shown < tileCount && strip.chip === strip.shown,
      `${strip.shown} of ${tileCount}, chip says ${strip.chip}`,
    );
    await previewAs(page, "lattice", { n: 5 });
    await sleep(1500);
    const lattice = await shownAndChip(TILE);
    check(
      "filter: a 3D lattice shows every pattern again",
      lattice.shown === tileCount,
      `${lattice.shown}/${tileCount}`,
    );
    await previewAs(page, "auto");
    await sleep(800);
  }

  // ── 2. New pattern opens the editor full-screen ──
  await page.click('[data-role="new-pattern"]');
  await page.waitForSelector('[data-role="editor-back"]', { timeout: 3000 });
  await page.waitForSelector(".cm-content");
  await sleep(400);
  check("New opens the editor (back button)", (await page.$('[data-role="editor-back"]')) !== null);
  // The shell header (and with it the fps readout) does not exist over an
  // editor screen since #538 — the editor states its own rate in the preview
  // section's header instead (mockup S2).
  const fpsText = await page.$eval('[data-role="preview-dims"]', (el) => el.textContent ?? "");
  check(
    "engine renders (fps > 0)",
    parseInt(/^\s*(\d+)\s*fps/.exec(fpsText)?.[1] ?? "0") > 10,
    fpsText.trim(),
  );

  // The header owns the document (A7, #468): the name edits INLINE — there is
  // no naming dialog any more, and an unnamed pattern is refused in place.
  await page.keyboard.down("Control");
  await page.keyboard.press("s");
  await page.keyboard.up("Control");
  await sleep(300);
  check(
    "Ctrl+S on an unnamed pattern opens the inline name editor",
    (await page.$('[data-role="name-input"]')) !== null,
  );
  check("Ctrl+S opens no naming dialog", (await page.$('[data-role="dialog"]')) === null);
  check(
    "the inline editor says why, in place",
    (await page.$eval('[data-role="name-error"]', (el) => el.textContent.trim())) !== "",
  );
  await page.keyboard.press("Enter"); // still empty
  await sleep(200);
  check(
    "an empty name is rejected inline (field stays open)",
    (await page.$('[data-role="name-input"]')) !== null &&
      (await page.$eval('[data-role="name-error"]', (el) => el.textContent.trim())) ===
        "a name is required",
  );
  await page.type('[data-role="name-input"]', "e2e saved");
  await page.keyboard.press("Enter");
  await sleep(300);
  check(
    "Enter commits the inline rename",
    (await page.$eval('[data-role="pattern-name"]', (el) => el.textContent.trim())) === "e2e saved",
  );
  check("a renamed document reads unsaved", (await saveState(page)) === "unsaved");
  await page.click('[data-role="save"]');
  await sleep(400);
  check("Save stores it under the header's name", (await saveState(page)) === "saved · in browser");
  const lit = await page.$eval(".waterfall", (c) => {
    const d = c.getContext("2d").getImageData(0, 0, c.width, 3).data;
    return d.some((v, i) => i % 4 !== 3 && v > 0);
  });
  check("waterfall shows pixels", lit);

  // The document verbs live in the EDITOR's own header (A7, #468), not in the
  // shell header next to the device chip, and not in a bar under the code.
  const saveInEditorHeader = await page.$(
    'main.editor-view [data-role="editor-header"] [data-role="save"]',
  );
  check("Save is the editor header's primary action", saveInEditorHeader !== null);
  check("no file actions in the shell header", (await page.$('.shell > header [data-role="save"]')) === null);
  check("the editor header states the save state", (await page.$('[data-role="save-state"]')) !== null);
  check("Duplicate is in the ⋯ menu", await menuHas(page, "duplicate"));
  check("Import .epe… is in the ⋯ menu", await menuHas(page, "epe-import"));

  // ── the editor header, against mockup S2 (audit E1–E5, #538) ──
  check("the primary action reads Save, everywhere", (await page.$eval('[data-role="save"]', (el) => el.textContent.trim())) === "Save");
  const nameBox = await page.$eval('[data-role="pattern-name"]', (el) => {
    const cs = getComputedStyle(el);
    return {
      h: Math.round(el.getBoundingClientRect().height),
      bw: cs.borderTopWidth,
      bc: cs.borderTopColor,
      weight: cs.fontWeight,
    };
  });
  check(
    "E1: the name is a persistent bordered field (30px .nameedit)",
    nameBox.h === 30 && nameBox.bw === "1px" && nameBox.bc !== "rgba(0, 0, 0, 0)" && nameBox.weight === "600",
    JSON.stringify(nameBox),
  );
  // E4: the header's rail segment is the RAIL COLUMN, so Save + ⋯ end where
  // the code does rather than at the page's right edge.
  const split = await page.evaluate(() => {
    const view = document.querySelector('[data-role="editor-view"]');
    const rail = view?.querySelector(".right");
    const hdrRail = view?.querySelector(".edhdr-rail");
    const save = view?.querySelector('[data-role="save"]');
    const menu = view?.querySelector('[data-role="overflow"]');
    return {
      rail: Math.round((rail?.getBoundingClientRect().left ?? -1)),
      hdrRail: Math.round(hdrRail?.getBoundingClientRect().left ?? -2),
      saveRight: Math.round(save?.getBoundingClientRect().right ?? 0),
      menuRight: Math.round(menu?.getBoundingClientRect().right ?? 0),
    };
  });
  check(
    "E4: the header splits on the rail column's own edge",
    Math.abs(split.rail - split.hdrRail) <= 1,
    JSON.stringify(split),
  );
  check(
    "E4: Save and ⋯ end inside the code column",
    split.menuRight < split.rail && split.saveRight < split.rail,
    JSON.stringify(split),
  );
  // E5: the mock's order — playlist group, rule, document verbs, rule, delete
  await page.click('[data-role="overflow"]');
  await page.waitForSelector('[data-role="editor-menu"]', { timeout: 3000 });
  const menuOrder = await page.$$eval('[data-role="editor-menu"] > *', (els) =>
    els.map((e) => (e.classList.contains("sepr") ? "—" : (e.dataset.role ?? "?"))),
  );
  check(
    "E5: menu order is the mock's, delete last and error-tinted",
    menuOrder.join(",") === "duplicate,epe-export,epe-import,share,—,delete",
    menuOrder.join(","),
  );
  // `Add to scene ▸` is a CONSOLE verb on a regular 2D fixture (§5.4b/§5.4c):
  // the playground has no `/api/scenes` to put a pattern into, so the row is
  // absent rather than disabled — the same rule as `Add to playlist` above it
  // (Gitea #478). device-e2e covers the console halves, both of them.
  check(
    "E5: no `Add to scene` in the playground — there is no device to hold one",
    !menuOrder.includes("add-to-scene"),
    menuOrder.join(","),
  );
  await page.keyboard.press("Escape");
  await sleep(150);

  // ── the preview header's transport (audit E7/E8) ──
  const pauseBox = await page.$eval('[data-role="pause"]', (el) => {
    const r = el.getBoundingClientRect();
    return { w: Math.round(r.width), h: Math.round(r.height), fs: getComputedStyle(el).fontSize };
  });
  check(
    "E7: pause is the 26×26 .btn.sm.icon with a 12px glyph",
    pauseBox.w === 26 && pauseBox.h === 26 && pauseBox.fs === "12px",
    JSON.stringify(pauseBox),
  );
  const transport = await page.$$eval('[data-role="editor-view"] .grp > *', (els) =>
    els.map((e) => e.dataset.role ?? e.tagName.toLowerCase()),
  );
  check(
    "E8: Debug sits immediately after pause, with the rate last",
    transport[0] === "pause" && transport[1] === "debug" && transport[transport.length - 1] === "target-fps",
    transport.join(","),
  );
  check(
    "E8: the debug button is labelled, not icon-only",
    (await page.$eval('[data-role="debug"]', (el) => el.textContent.trim())) === "Debug",
  );
  // The readout leads with the RATE(S) and puts the layout after them, so a
  // 360px rail ellipsises the layout rather than the number (mockup S2's
  // `.rdim`). The playground has one loop, so one rate and no slash.
  check(
    "E9: the playground states the LOCAL rate",
    /^\d+ fps · /.test(
      (await page.$eval('[data-role="preview-dims"]', (el) => el.textContent ?? "")).trim(),
    ) && !/\//.test(await page.$eval('[data-role="preview-dims"]', (el) => el.textContent ?? "")),
    await page.$eval('[data-role="preview-dims"]', (el) => (el.textContent ?? "").trim()),
  );
  // the old playback bar is gone: geometry and transport left the code column
  check("no layout select in the playground editor", (await page.$('[data-role="layout-kind"]')) === null);
  check("no sub-tabs above the code", (await page.$('[data-role="editor-subtabs"]')) === null);
  check(
    "the transport is the preview panel's own header",
    (await page.$('.rsec .rhead [data-role="pause"]')) !== null,
  );

  // ── 3. a clean rainbow, then typing + compile error. Single-line bodies
  //      throughout: CodeMirror auto-closes `{`, so a trailing `}` on its own
  //      line would double up. ──
  await setEditor(page, "export function render(index) { hsv(index / pixelCount, 1, 1) }");
  await page.click(".cm-content");
  await page.keyboard.type(" @@@");
  await sleep(300);
  check("editor accepts typing", (await page.$eval(".cm-content", (el) => el.textContent ?? "")).includes("@@@"));
  // The code pane owns its errors (A7, #468): a strip pinned to its bottom,
  // a squiggle on the span and a dot in the gutter — no banner in the rail.
  await page.waitForSelector('[data-role="compile-error"]', { timeout: 3000 }).catch(() => null);
  const errStrip = await page
    .$eval('[data-role="compile-error"]', (el) => (el.textContent ?? "").trim())
    .catch(() => "");
  check("compile error strip appears under the code", errStrip !== "");
  check("compile error strip names the line", /^✗ line \d+ · /.test(errStrip), errStrip.slice(0, 48));
  check("no compile-error banner in the rail", (await page.$(".right .banner.error")) === null);
  await page.waitForSelector(".cm-lintRange-error", { timeout: 2000 }).catch(() => null);
  check("error squiggle rendered", (await page.$(".cm-lintRange-error")) !== null);
  check("error gutter dot rendered", (await page.$(".cm-err-dot")) !== null);
  await page.screenshot({ path: `${shotDir}/e2e-compile-error.png` });
  await page.keyboard.press("Escape");
  await page.keyboard.down("Control");
  await page.keyboard.press("z");
  await page.keyboard.press("z");
  await page.keyboard.up("Control");
  await sleep(500);
  const fixedDoc = await page.$eval(".cm-content", (el) => el.textContent ?? "");
  check(
    "error strip and gutter dot clear after a fix",
    (await page.$('[data-role="compile-error"]')) === null &&
      (await page.$(".cm-err-dot")) === null &&
      !fixedDoc.includes("@@@"),
  );
  await page.screenshot({ path: `${shotDir}/e2e-2-typing.png` });

  // ── 3b. event injection: a preview click feeds readEvent() ──
  await setEditor(
    page,
    "var ev = array(4)\nvar hit = 0\n" +
      "export function beforeRender(delta) { while (readEvent(ev)) hit = 1 }\n" +
      "export function render(index) { rgb(hit, 0, 0) }",
  );
  await sleep(300);
  const stripPx = () =>
    page.$eval(".strip", (c) => {
      const d = c.getContext("2d").getImageData(0, 0, c.width, 1).data;
      return Array.from(d).some((v, i) => i % 4 !== 3 && v > 0);
    });
  const darkBefore = !(await stripPx());
  await page.click(".strip"); // pointerdown → inject event at the click
  await sleep(300);
  const litAfter = await stripPx();
  check("preview click injects a readEvent event", darkBefore && litAfter, `dark=${darkBefore} lit=${litAfter}`);
  await page.screenshot({ path: `${shotDir}/e2e-2b-events.png` });

  // ── 3c. pin panel: shown only for patterns that name a pin, and a press
  // drives digitalRead (Gitea #205) ──
  check("no pin panel for a pattern with no GPIO", (await page.$('[data-role="pin-row"]')) === null);
  await setEditor(
    page,
    "pinMode(26, INPUT_PULLUP)\n" +
      "export function render(index) { rgb(digitalRead(26) == LOW, 0, 0) }",
  );
  await page.waitForSelector('[data-role="pin-row"][data-pin="26"]', { timeout: 5000 });
  const pinLevel = () =>
    page.$eval('[data-role="pin-level"][data-pin="26"]', (e) => e.textContent.trim());
  check("pull-up pin idles HIGH", (await pinLevel()) === "HIGH");
  check("undriven pull-up pattern renders dark", !(await stripPx()));
  const pressBtn = await page.$('[data-role="pin-press"][data-pin="26"]');
  const pressBox = await pressBtn.boundingBox();
  await page.mouse.move(pressBox.x + pressBox.width / 2, pressBox.y + pressBox.height / 2);
  await page.mouse.down();
  await sleep(300);
  check("press pulls the pin LOW and lights the strip", (await pinLevel()) === "LOW" && (await stripPx()));
  await page.screenshot({ path: `${shotDir}/e2e-2c-pins.png` });
  await page.mouse.up();
  await sleep(300);
  check("releasing the press returns the pin to idle", (await pinLevel()) === "HIGH");
  // the latch holds the same state with no pointer down
  await page.click('[data-role="pin-latch"][data-pin="26"]');
  await sleep(300);
  check("latch holds the pin driven", (await pinLevel()) === "LOW" && (await stripPx()));
  await page.click('[data-role="pin-latch"][data-pin="26"]');
  await sleep(300);
  check("un-latching releases the pin", (await pinLevel()) === "HIGH");

  // ── 3d. analog pins: a pattern that samples analogRead/touchRead gets a
  // 0..1 slider instead of press/latch, and dragging it moves the render
  // (Gitea #206) ──
  check(
    "no analog row for a pattern with no analog reads",
    (await page.$('[data-role="analog-row"]')) === null,
  );
  await setEditor(
    page,
    "export function render(index) { rgb(analogRead(33), 0, touchRead(4)) }",
  );
  await page.waitForSelector('[data-role="analog-row"][data-pin="33"]', { timeout: 5000 });
  check(
    "touchRead pins get a row too",
    (await page.$('[data-role="analog-row"][data-pin="4"]')) !== null,
  );
  check(
    "an analog-only pattern gets no digital press/latch row",
    (await page.$('[data-role="pin-row"]')) === null,
  );
  const analogText = (pin) =>
    page.$eval(`[data-role="analog-value"][data-pin="${pin}"]`, (e) => e.textContent.trim());
  // max red channel of the strip's first row — the value analogRead(33) fed
  // into rgb(), read back off the real canvas
  const stripRed = () =>
    page.$eval(".strip", (c) => {
      const d = c.getContext("2d").getImageData(0, 0, c.width, 1).data;
      let m = 0;
      for (let i = 0; i < d.length; i += 4) m = Math.max(m, d[i]);
      return m;
    });
  check("undriven analog pin reads 0", (await analogText(33)) === "0.00" && !(await stripPx()));
  // drag the slider to full scale with a real pointer, not a synthetic event
  const slider = await page.$('[data-role="analog-slider"][data-pin="33"]');
  const sBox = await slider.boundingBox();
  // press on the thumb and drag: a click at exactly the right edge lands
  // outside the control and does nothing, so past-the-end is how you reach max
  const dragTo = async (frac) => {
    const y = sBox.y + sBox.height / 2;
    await page.mouse.move(sBox.x + sBox.width / 2, y);
    await page.mouse.down();
    await page.mouse.move(sBox.x + sBox.width * frac, y, { steps: 5 });
    await page.mouse.up();
    await sleep(300);
  };
  await dragTo(1.1);
  const full = await stripRed();
  check("slider at full scale drives analogRead to 1", (await analogText(33)) === "1.00" && full > 250, `red=${full}`);
  await page.screenshot({ path: `${shotDir}/e2e-2d-analog.png` });
  await dragTo(0.25);
  const quarter = Number(await analogText(33));
  const dim = await stripRed();
  check(
    "dragging the slider down dims the render proportionally",
    quarter > 0 && quarter < 1 && dim > 0 && dim < full,
    `value=${quarter} red=${dim} full=${full}`,
  );

  // restore the rainbow the debugger section expects (render on line 1)
  await setEditor(page, "export function render(index) { hsv(index / pixelCount, 1, 1) }");
  await sleep(300);
  check("pin panel disappears with the pin", (await page.$('[data-role="pin-row"]')) === null);
  check(
    "analog rows disappear with the pattern",
    (await page.$('[data-role="analog-row"]')) === null,
  );

  // ── 4. debugger: gutter breakpoint on the render line (line 1) pauses ──
  const lineRect = await page.$$eval(".cm-line", (els) => {
    const r = els[0].getBoundingClientRect(); // line 1 = render body
    return { y: r.y, h: r.height };
  });
  const gutterRect = await page.$eval(".cm-bp-gutter", (el) => {
    const r = el.getBoundingClientRect();
    return { x: r.x, w: r.width };
  });
  await page.mouse.click(gutterRect.x + gutterRect.w / 2, lineRect.y + lineRect.h / 2);
  await page.waitForSelector('.debugger[data-paused="true"]', { timeout: 3000 }).catch(() => null);
  check("breakpoint pauses execution", (await page.$('.debugger[data-paused="true"]')) !== null);
  const status = await page.$eval(".debug-status", (el) => el.textContent ?? "").catch(() => "");
  check("paused in render at pixel 0", /line \d+/.test(status) && status.includes("pixel 0"), status.trim());
  const stackTxt = await page.$eval(".stack", (el) => el.textContent ?? "").catch(() => "");
  check("stack shows render + index local", stackTxt.includes("render") && stackTxt.includes("index"));
  check("current line highlighted", (await page.$(".cm-debug-line")) !== null);
  // hover the `index` identifier → value tooltip
  const wordRect = await page.evaluate(() => {
    for (const lineEl of document.querySelectorAll(".cm-line")) {
      const walker = document.createTreeWalker(lineEl, NodeFilter.SHOW_TEXT);
      let node;
      while ((node = walker.nextNode())) {
        const i = node.textContent.indexOf("index");
        if (i >= 0) {
          const r = document.createRange();
          r.setStart(node, i);
          r.setEnd(node, i + 5);
          const b = r.getBoundingClientRect();
          return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
        }
      }
    }
    return null;
  });
  if (wordRect) {
    await page.mouse.move(wordRect.x, wordRect.y);
    await sleep(500);
    const tip = await page.$eval(".cm-hover-value", (el) => el.textContent ?? "").catch(() => "");
    check("hover shows variable value", tip.includes("index = 0"), tip);
    await page.mouse.move(5, 5);
    await sleep(200);
  }
  await page.click(".db-over");
  await sleep(150);
  const status2 = await page.$eval(".debug-status", (el) => el.textContent ?? "").catch(() => "");
  check("step flows to next pixel", status2.includes("pixel 1"), status2.trim());
  await page.click(".db-continue");
  await sleep(150);
  const status3 = await page.$eval(".debug-status", (el) => el.textContent ?? "").catch(() => "");
  check("continue re-arms breakpoint", status3.includes("pixel 2"), status3.trim());
  await page.click('[data-role="debug"]'); // debug off (also clears breakpoints on next swap)
  await sleep(300);
  check("debug off resumes rendering", (await page.$(".debugger")) === null);
  await page.screenshot({ path: `${shotDir}/e2e-debugger.png` });

  // ── 4b. implicit globals appear in the globals pane (globals set in
  //       beforeRender; breakpoint on the render line, line 2) ──
  await setEditor(
    page,
    "export function beforeRender(delta) { phase = time(.1); bright = 1 }\nexport function render(index) { hsv(phase, 1, bright) }",
  );
  await page.mouse.move(5, 5); // clear any hover tooltip that could eat the click
  await sleep(100);
  const gline = await page.$$eval(`${VISIBLE_CODE} .cm-line`, (els) => {
    const r = els[1].getBoundingClientRect(); // line 2 = render body
    return { y: r.y, h: r.height };
  });
  await page.mouse.click(gutterRect.x + gutterRect.w / 2, gline.y + gline.h / 2);
  const gpaused = await page.waitForSelector('.debugger[data-paused="true"]', { timeout: 3000 }).then(() => true).catch(() => false);
  check("globals test paused", gpaused);
  const globals = await page.$eval('[data-role="globals"]', (el) => el.textContent ?? "").catch(() => "");
  check("globals pane shows implicit globals (phase, bright)", globals.includes("phase") && globals.includes("bright"), globals.slice(0, 40));
  await page.click('[data-role="debug"]');
  await sleep(300);

  // ── 5. controls: slider + numeric entry are two-way ──
  await setEditor(
    page,
    "export var level = 0.5\nexport function sliderSpeed(v) { level = v }\nexport function render(index) { hsv(0, 0, level) }",
  );
  await page.waitForSelector('input[type="range"]');
  const before = await page.$eval(".control .num", (el) => el.value);
  await page.$eval('input[type="range"]', (el) => {
    el.value = "0.9";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await sleep(200);
  const after = await page.$eval(".control .num", (el) => el.value);
  check("slider moves the numeric readout", before !== after && Number(after) === 0.9, `${before} → ${after}`);
  await page.$eval(".control .num", (el) => {
    el.value = "0.25";
    el.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await sleep(200);
  const rangeNow = await page.$eval('input[type="range"]', (el) => el.value);
  check("number entry moves the slider", Number(rangeNow) === 0.25, rangeNow);

  // ── 5b. a colour control is a SWATCH that opens a real picker (#538) ──
  // Jeremy: "should not be asking the user to input raw hsv numbers. Use a
  // real color picker (that lets putting in values directly too!) Do not use
  // the browser color picker."
  await setEditor(
    page,
    "export var h = 0, s = 1, v = 1\nexport function hsvPickerPrimary(a, b, c) { h = a; s = b; v = c }\nexport function render(index) { hsv(h, s, v) }",
  );
  await sleep(400);
  check(
    "a colour control is a swatch, not three raw channels",
    (await page.$('.control [data-role="color-swatch"]')) !== null &&
      (await page.$$eval(".control .ch-row", (els) => els.length)) === 0,
  );
  check(
    "no browser colour input anywhere in the editor",
    (await page.$('[data-role="editor-view"] input[type="color"]')) === null,
  );
  const swBox = await page.$eval('[data-role="color-swatch"]', (el) => {
    const r = el.getBoundingClientRect();
    return { w: Math.round(r.width), h: Math.round(r.height) };
  });
  check("the swatch is the mock's 26×22 (S2 .swatches i)", swBox.w === 26 && swBox.h === 22, JSON.stringify(swBox));
  await page.click('[data-role="color-swatch"]');
  await page.waitForSelector('[data-role="color-pop"]', { timeout: 3000 });
  check(
    "the picker offers a field, a hue strip and direct entry",
    (await page.$('[data-role="color-field"]')) !== null &&
      (await page.$('[data-role="color-hue"]')) !== null &&
      (await page.$('[data-role="color-hex"]')) !== null &&
      (await page.$('[data-role="color-hsv"]')) !== null &&
      (await page.$('[data-role="color-rgb"]')) !== null,
  );
  // typing a hex must move the CONTROL, not just the preview
  await page.$eval('[data-role="color-hex"]', (el) => {
    el.value = "#00ff00";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await sleep(300);
  const hexed = await page.$eval('[data-role="color-swatch"]', (el) => el.dataset.value);
  const hsvNow = await page.$$eval('[data-role="color-hsv"] input', (els) => els.map((e) => Number(e.value)));
  check("hex entry sets the colour", hexed === "#00ff00", `${hexed}`);
  check(
    "and it lands on the control's own hsv channels",
    Math.abs((hsvNow[0] ?? 0) - 1 / 3) < 0.005 && hsvNow[1] === 1 && hsvNow[2] === 1,
    JSON.stringify(hsvNow),
  );
  await page.keyboard.press("Escape");
  await sleep(200);
  check("Escape closes the picker", (await page.$('[data-role="color-pop"]')) === null);
  const overflow = await page.$eval(".right", (el) => el.scrollWidth - el.clientWidth);
  check("picker does not overflow the rail", overflow === 0, `${overflow}px`);

  // ── 6. //# hints bound a slider; grid layout renders ──
  await setEditor(
    page,
    "export var zoom = 0.45\nexport function sliderZoom(v) { zoom = v }  //# min=0.1 max=1.5 default=0.45\nexport function render2D(index, x, y) { hsv(x * zoom, 1, 1) }",
  );
  await previewAs(page, "matrix", { w: 16, h: 16 });
  const [mn, mx, val] = await page.$eval('input[type="range"]', (el) => [el.min, el.max, el.value]);
  check("//# hint bounds the slider", mn === "0.1" && mx === "1.5", `min=${mn} max=${mx}`);
  check("//# default applied", Number(val) === 0.45, val);
  const gridLit = await page.$eval(".grid", (c) => {
    const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
    return d.some((v, i) => i % 4 !== 3 && v > 0);
  });
  check("2D grid preview renders", gridLit);
  check(
    "the Preview-as chip states the layout",
    (await rig(page)).label === "16×16 matrix",
    (await rig(page)).label,
  );
  // bump the matrix width from the chip
  await previewAs(page, "matrix", { w: 24 });
  check("layout edit resizes the render", (await page.$eval(".grid", (c) => c.width)) === 24);
  await previewAs(page, "auto");
  check("back to Auto follows the pattern again", (await rig(page)).label === "16×16 matrix");
  await setEditor(page, "export function render(index) { hsv(index / pixelCount, 1, 1) }");
  await sleep(400);
  check("Auto: a 1D pattern is a bar again", (await rig(page)).shape === "bar");
  await page.screenshot({ path: `${shotDir}/e2e-3-controls.png` });

  // ── 7. pause freezes the preview; vars watcher lists exports ──
  await setEditor(page, "export var zoom = 2\nexport function render(index) { hsv(index / pixelCount, 1, 1) }");
  await page.click('[data-role="pause"]');
  await sleep(300);
  const a1 = await page.$eval(".waterfall", (c) => c.getContext("2d").getImageData(0, 0, 8, 8).data.join());
  await sleep(400);
  const a2 = await page.$eval(".waterfall", (c) => c.getContext("2d").getImageData(0, 0, 8, 8).data.join());
  check("pause freezes the preview", a1 === a2);
  await page.click('[data-role="pause"]');
  const varText = await page.$eval("table", (el) => el.textContent ?? "").catch(() => "");
  check("var watcher lists zoom", varText.includes("zoom"));
  check("VARS is present for a pattern that exports one", (await page.$('[data-role="vars-section"]')) !== null);

  // ── 7a2. absent, not disabled (proposal §5.7): a pattern that exports no
  //        vars has no VARS section, and one that reads no sensors has no mic ──
  await setEditor(page, "export function render(index) { hsv(index / pixelCount, 1, 1) }");
  await sleep(400);
  check("VARS is absent for a pattern that exports none", (await page.$('[data-role="vars-section"]')) === null);
  check("mic is absent for a pattern that reads no sensors", (await page.$('[data-role="mic-toggle"]')) === null);

  // ── 7b. sound: mic toggle feeds sensor vars (fake chromium mic tone) ──
  await setEditor(
    page,
    "export var energyAverage\nexport var frequencyData\n" +
      "export function render(index) { hsv(0, 1, energyAverage) }",
  );
  await sleep(400);
  check(
    "mic appears for a pattern that reads frequencyData",
    (await page.$('[data-role="mic-toggle"]')) !== null,
  );
  await page.click('[data-role="mic-toggle"]');
  let heard = false;
  for (let i = 0; i < 12 && !heard; i++) {
    await sleep(250);
    // the var watcher polls every 250ms; fake mic emits a beeping tone
    const t = await page.$eval("table", (el) => el.textContent ?? "").catch(() => "");
    const m = t.match(/energyAverage\s*([0-9.]+)/);
    if (m && Number(m[1]) > 0.001) heard = true;
  }
  check("sound: mic drives energyAverage", heard);
  await page.click('[data-role="mic-toggle"]'); // off again for the rest

  // ── 8. .epe import / export ──
  const { mkdtempSync, writeFileSync, readFileSync, readdirSync } = await import("node:fs");
  const { tmpdir } = await import("node:os");
  const { join } = await import("node:path");
  const epeDir = mkdtempSync(join(tmpdir(), "luxel-epe-"));
  const epeMain =
    "leader = 0\nexport function beforeRender(delta) { leader = time(0.05) }\nexport function render(index) { hsv(0, 1, saturate(1 - abs(index / pixelCount - leader) * 4)) }\n";
  const epePath = join(epeDir, "KITT.epe");
  writeFileSync(epePath, JSON.stringify({ name: "KITT e2e", id: "e2eTestPattern0001", sources: { main: epeMain } }));
  const fileInput = await page.$('input[type="file"]');
  check("import file input present", fileInput !== null);
  await fileInput.uploadFile(epePath);
  await sleep(700);
  check("epe import replaces the source", (await page.$eval(".cm-content", (el) => el.textContent ?? "")).includes("beforeRender"));
  check("epe import compiles (no error strip)", (await page.$('[data-role="compile-error"]')) === null);
  check("editor shows the imported name", (await page.$eval('[data-role="pattern-name"]', (el) => el.textContent ?? "")).includes("KITT e2e"));
  const badPath = join(epeDir, "broken.epe");
  writeFileSync(badPath, "{ not json");
  await fileInput.uploadFile(badPath);
  await sleep(400);
  check("broken epe shows import error", (await page.$('[data-role="import-error"]')) !== null);
  check("broken epe keeps the pattern", (await page.$eval(".cm-content", (el) => el.textContent ?? "")).includes("beforeRender"));
  await page.click('[data-role="import-error"] .dismiss');
  const dlDir = mkdtempSync(join(tmpdir(), "luxel-dl-"));
  const cdp = await page.createCDPSession();
  await cdp.send("Browser.setDownloadBehavior", { behavior: "allow", downloadPath: dlDir, eventsEnabled: true });
  await menuClick(page, "epe-export");
  await sleep(800);
  const dl = readdirSync(dlDir).find((f) => f.endsWith(".epe"));
  check("export downloads an .epe", dl !== undefined, dl ?? "no file");
  if (dl) {
    const round = JSON.parse(readFileSync(join(dlDir, dl), "utf8"));
    check(
      "export round-trips source + name",
      round.name === "KITT e2e" && round.id?.length === 17 && round.sources?.main === epeMain,
      dl,
    );
  }

  // ── 9. shareable URL (playground only, in the ⋯ menu) ──
  await setEditor(page, "export function render(index) { hsv(time(.1) + index / pixelCount, 1, 1) }");
  await menuClick(page, "share");
  await sleep(400);
  const shareUrl = await page.url();
  check("share writes a #p= fragment", /#p(s)?=/.test(shareUrl), shareUrl.slice(-24));
  const page2 = await browser.newPage();
  await page2.goto(shareUrl, { waitUntil: "networkidle0" });
  await page2.waitForSelector(".cm-content");
  await sleep(800);
  check("share link opens the editor on the pattern", (await page2.$('[data-role="editor-back"]')) !== null);
  check("share link restores the pattern", (await page2.$eval(".cm-content", (el) => el.textContent ?? "")).includes("hsv(time(.1)"));
  check("shared pattern compiles", (await page2.$('[data-role="compile-error"]')) === null);
  await page2.close();

  // ── 10. the map program's OWN SCREEN (A10, #471) ──
  // It is reached from the Layout picker, never from inside a pattern: in the
  // playground the "Preview as" chip's `Custom map program` opens it. The
  // screen is the pattern editor's chrome with the map's document in it —
  // code left, the plotted points + the debugger right, and ONE primary
  // action ("Use in preview" here, "Install on device" on a console).
  await previewAs(page, "map"); // picking it opens the screen
  await page.waitForSelector('[data-role="map-editor-view"]:not([hidden])', { timeout: 3000 });
  await sleep(700);
  check(
    "the chip's Custom map program opens the map screen",
    (await page.$('[data-role="map-editor-view"]:not([hidden])')) !== null,
  );
  check("the pattern editor is not the visible screen", (await page.$('[data-role="editor-view"]:not([hidden])')) === null);
  check("the map screen has the editor chrome (back, primary, ⋯)", (await page.$('[data-role="map-editor-back"]')) !== null
    && (await page.$('[data-role="map-use"]')) !== null
    && (await page.$('[data-role="map-overflow"]')) !== null);
  check("the playground's primary is 'Use in preview', not an install", (await page.$('[data-role="map-install"]')) === null);
  check("the map program is not a pattern (no Save/name in this header)",
    (await page.$('[data-role="map-editor-view"] [data-role="save"]')) === null);
  check("the map screen shows the program's source", (await page.$('[data-role="map-editor"] .cm-line')) !== null);

  // it compiles and runs on arrival, so the points are on screen immediately
  check("map runs without error", (await page.$('[data-role="map-compile-error"]')) === null
    && (await page.$('[data-role="map-error"]')) === null);
  const mapBadge = await page.$eval('[data-role="map-badge"]', (el) => (el.textContent ?? "").trim());
  check("the map badge states the count and the detected dims", /^\d+ points · 2D$/.test(mapBadge), mapBadge);
  const mapLit = await page.$eval('[data-role="map-editor-view"] .map', (c) => {
    const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
    let n = 0;
    for (let i = 0; i < d.length; i += 4) if (d[i] + d[i + 1] + d[i + 2] > 20) n++;
    return n;
  });
  check("the map screen scatters the plotted points", mapLit > 200, `${mapLit} lit`);
  await page.screenshot({ path: `${shotDir}/e2e-4-map.png` });

  // the primary action is what makes these points the page's Layout
  await page.click('[data-role="map-use"]');
  await sleep(400);
  await page.click('[data-role="map-editor-back"]');
  await sleep(500);
  check("back returns to the pattern editor", (await page.$('[data-role="editor-view"]:not([hidden])')) !== null);
  check(
    "Use in preview makes the map the Layout",
    (await rig(page)).label.includes("custom map"),
    (await rig(page)).label,
  );
  check("map layout is a scatter", (await rig(page)).shape === "scatter");
  check("the editor's rail links to the map screen while the Layout is custom",
    (await page.$('[data-role="subtab-map"]')) === null); // playground: the chip is the way in

  // debuggable: a breakpoint on plot() pauses the per-pixel map run
  await previewAs(page, "map"); // re-open the screen from the chip
  await page.waitForSelector('[data-role="map-editor-view"]:not([hidden])', { timeout: 3000 });
  await sleep(400);
  const mapPlot = await page.$$eval('[data-role="map-editor"] .cm-line', (els) => {
    const i = els.findIndex((el) => el.textContent?.includes("plot("));
    if (i < 0) return null;
    const r = els[i].getBoundingClientRect();
    return { y: r.y, h: r.height };
  });
  check("the map pane shows the map program's source", mapPlot !== null);
  if (mapPlot) {
    const mg = await page.$eval('[data-role="map-editor"] .cm-bp-gutter', (el) => {
      const r = el.getBoundingClientRect();
      return { x: r.x, w: r.width };
    });
    await page.mouse.click(mg.x + mg.w / 2, mapPlot.y + mapPlot.h / 2);
    await sleep(200);
    await page.click('[data-role="map-run"]');
    await page.waitForSelector('.debugger[data-paused="true"]', { timeout: 3000 }).catch(() => null);
    check("map breakpoint pauses the run", (await page.$('.debugger[data-paused="true"]')) !== null);
    await page.click('[data-role="map-debug"]');
    await sleep(300);
  }

  // ── 10b. a 3D map (z spirals) renders as an auto-rotating point cloud ──
  // The map is already in use, so editing the program re-publishes its points:
  // the pattern editor's preview is a cloud when we come back.
  await setEditor(
    page,
    "export function render(index) { plot(cos(index/pixelCount*PI2*3), sin(index/pixelCount*PI2*3), index/pixelCount - 0.5) }",
  );
  await page.click('[data-role="map-run"]');
  await sleep(500);
  const badge3d = await page.$eval('[data-role="map-badge"]', (el) => (el.textContent ?? "").trim());
  check("plot(x, y, z) is detected as 3D", badge3d.endsWith("3D"), badge3d);
  await page.screenshot({ path: `${shotDir}/e2e-4b-map3d.png` });
  await page.click('[data-role="map-editor-back"]');
  await sleep(400);
  const is3d = await page
    .$eval('[data-role="editor-view"] .map', (c) => c.dataset["3d"])
    .catch(() => "");
  check(
    "3D map detected (badge shown)",
    is3d === "true" && (await page.$('[data-role="editor-view"] [data-role="map-3d"]')) !== null,
    `data-3d=${is3d}`,
  );
  const m3a = await page.$eval('[data-role="editor-view"] .map', (c) => c.toDataURL());
  await sleep(500);
  const m3b = await page.$eval('[data-role="editor-view"] .map', (c) => c.toDataURL());
  check("3D map auto-rotates", m3a !== m3b);

  // ── 10c. share links carry the PATTERN only; old map links still open ──
  // A map is the Layout's, not the pattern's (#463), so a link made today is
  // `#p=` even with a custom map installed. Links already out there carry one
  // (`#pj=`) and must keep working — built by hand here, uncompressed, since
  // nothing writes that form any more.
  await menuClick(page, "share");
  await sleep(400);
  const shareMapUrl = await page.url();
  check("share with a map still writes #p= (no map inside)", /#p(s)?=/.test(shareMapUrl), shareMapUrl.slice(-24));
  check("share no longer writes #pj=", !/#pj/.test(shareMapUrl));
  const legacy = await page.evaluate(() => {
    const payload = JSON.stringify({
      s: "export function render(index) { hsv(index / pixelCount, 1, 1) }",
      m: "export function render(index) { plot(cos(index/pixelCount*PI2), sin(index/pixelCount*PI2), index/pixelCount - 0.5) }",
    });
    const bytes = new TextEncoder().encode(payload);
    let s = "";
    for (const v of bytes) s += String.fromCharCode(v);
    return `#pjs=${btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "")}`;
  });
  const page3 = await browser.newPage();
  await page3.goto(`http://localhost:${PORT}/${legacy}`, { waitUntil: "networkidle0" });
  await page3.waitForSelector(".cm-content");
  await sleep(1500);
  const sharedLabel = await page3
    .$eval('[data-role="preview-as-label"]', (el) => (el.textContent ?? "").trim())
    .catch(() => "");
  check("a pre-#463 share link still restores its map", sharedLabel.includes("custom map"), sharedLabel);
  check(
    "its 3D map is 3D again (badge)",
    (await page3.$('[data-role="editor-view"] [data-role="map-3d"]')) !== null,
  );
  await page3.close();

  // leaving the custom Layout puts the map program out of reach again (§5.7)
  await previewAs(page, "auto");
  await sleep(300);
  check("leaving the map layout closes mapping", !(await rig(page)).label.includes("custom map"));
  check("the map screen is not on top", (await page.$('[data-role="map-editor-view"]:not([hidden])')) === null);

  // ── 11. library: the inline name IS the saved name; back; reload resumes ──
  await setEditor(page, "export function render(index) { hsv(index / pixelCount, 1, 0.5) }");

  // 11a. the cancel path: emptying the name is refused in place, and cancelling
  //      the rename saves nothing (there is no naming dialog since #468).
  //      This browser's saved patterns are the `Mine` source of the Patterns
  //      page since #467 — they used to be a row of chips above the grid.
  const savedCount = () => page.$$eval(`${MINE} .tile`, (els) => els.length);
  const mineNames = () =>
    page.$$eval(`${MINE} .tile [data-role="tile-name"]`, (els) => els.map((e) => (e.textContent ?? "").trim()));
  const savedBefore = await savedCount();
  await page.click('[data-role="pattern-name"]');
  await page.waitForSelector('[data-role="name-input"]', { timeout: 2000 });
  await page.$eval('[data-role="name-input"]', (el) => {
    el.value = "";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await page.focus('[data-role="name-input"]');
  await page.keyboard.press("Enter");
  await sleep(200);
  check(
    "an emptied name keeps the inline editor open with a reason",
    (await page.$('[data-role="name-input"]')) !== null &&
      (await page.$eval('[data-role="name-error"]', (el) => el.textContent.trim())) !== "",
  );
  await page.keyboard.press("Escape");
  await sleep(250);
  check("Escape cancels the rename", (await page.$('[data-role="name-input"]')) === null);
  check("a cancelled rename adds nothing to the library", (await savedCount()) === savedBefore);
  check("no native or in-app dialog for naming", (await page.$('[data-role="dialog"]')) === null);

  // 11b. the accept path: the header's name is the saved name
  await renameTo(page, "e2e saved");
  await page.click('[data-role="save"]');
  await sleep(400);
  check(
    "the inline name becomes the pattern name",
    (await page.$eval('[data-role="pattern-name"]', (el) => el.textContent.trim())) === "e2e saved",
  );
  check("saving clears the unsaved state", (await saveState(page)) === "saved · in browser");
  await page.screenshot({ path: `${shotDir}/e2e-editor-playground.png` });
  await page.setViewport({ width: 390, height: 780 });
  await sleep(400);
  const stacked = (role) =>
    page.evaluate((r) => {
      const rail = document.querySelector(`[data-role="${r}"] .right`);
      const code = document.querySelector(`[data-role="${r}"] .left`);
      return rail !== null && code !== null && rail.getBoundingClientRect().top < code.getBoundingClientRect().top;
    }, role);
  check("mobile: the rail stacks above the code", await stacked("editor-view"));
  check(
    "mobile: the editor does not scroll sideways",
    await page.evaluate(() => document.documentElement.scrollWidth <= 390),
  );
  await page.screenshot({ path: `${shotDir}/e2e-editor-390.png` });
  // the map program's screen is the same frame, so it stacks the same way
  await previewAs(page, "map");
  await page.waitForSelector('[data-role="map-editor-view"]:not([hidden])', { timeout: 3000 });
  await sleep(600);
  check("mobile: the map screen stacks its rail above the code too", await stacked("map-editor-view"));
  check(
    "mobile: the map screen does not scroll sideways",
    await page.evaluate(() => document.documentElement.scrollWidth <= 390),
  );
  await page.screenshot({ path: `${shotDir}/e2e-map-390.png` });
  await page.click('[data-role="map-editor-back"]');
  await sleep(300);
  await previewAs(page, "auto"); // leave the Layout as the rest of the suite expects
  await page.setViewport({ width: 1400, height: 900 });
  await sleep(300);
  await page.click('[data-role="editor-back"]');
  await sleep(300);
  check("back returns to the Patterns page", (await page.$('[data-role="patterns-panel"]:not([hidden])')) !== null);
  await pickSource(page, "mine");
  const listedMine = await mineNames();
  check("saved pattern appears in the Mine source", listedMine.includes("e2e saved"), listedMine.join(","));
  check(
    "switching source shows only that source's grid",
    (await page.$$eval('[data-role="patterns-grid"]:not([hidden])', (els) =>
      els.map((e) => e.dataset.source),
    )).join(",") === "mine",
  );
  await page.screenshot({ path: `${shotDir}/e2e-patterns-mine.png` });
  // ⋯ → Import .epe… on a Mine tile puts the FILE in this browser's library
  // (Gitea #572, mockup S2's menu) — the editor's import verb replaces the
  // open document, which is not a thing a tile can mean.
  {
    const epePath = `${shotDir}/e2e-import-mine.epe`;
    fs.writeFileSync(
      epePath,
      JSON.stringify({
        name: "Imported Mine",
        id: "e2eimportmineabcd",
        sources: { main: "export function render(index) { hsv(0.1, 1, 1) }" },
      }),
    );
    await page.hover(`${MINE} .tile`);
    await sleep(150);
    await page.click(`${MINE} .tile [data-role="tile-menu"]`);
    await page.waitForSelector('[data-role="tile-menu-popup"]', { timeout: 3000 });
    const items = await page.$$eval('[data-role="tile-menu-popup"] .mi', (els) =>
      els.map((b) => (b.textContent ?? "").trim()),
    );
    check(
      "#572: a Mine tile's ⋯ menu carries Import .epe…",
      items.includes("Import .epe…"),
      items.join("|"),
    );
    await page.keyboard.press("Escape");
    await sleep(200);
    const picker = await page.$('[data-role="tile-menu-import-file"]');
    await picker.uploadFile(epePath);
    await sleep(900);
    const afterImport = await mineNames();
    check(
      "#572: the imported .epe lands in Mine, without opening the editor",
      afterImport.includes("Imported Mine") &&
        (await page.$('[data-role="editor-view"]:not([hidden])')) === null,
      afterImport.join(","),
    );
    fs.unlinkSync(epePath);
  }
  // reload → resumes the editor on the working copy
  await page.evaluate(() => history.replaceState(null, "", location.pathname));
  await page.reload({ waitUntil: "networkidle0" });
  await page.waitForSelector(".cm-content");
  await sleep(700);
  check("reload resumes the editor (working copy)", (await page.$('[data-role="editor-back"]')) !== null);
  check("working copy restored", (await page.$eval(".cm-content", (el) => el.textContent ?? "")).includes("0.5"));
  // open the saved pattern from its Mine tile
  await page.click('[data-role="editor-back"]');
  await sleep(300);
  await pickSource(page, "mine");
  await page.click(`${MINE} .tile [data-role="tile-face"]`);
  await sleep(400);
  check("a Mine tile opens the editor", (await page.$('[data-role="editor-back"]')) !== null);
  check("a stored pattern reads as saved", (await saveState(page)) === "saved · in browser");
  // delete: in the ⋯ menu, a danger confirmation, cancel first (entry survives)
  await menuClick(page, "delete");
  await waitDialog(page);
  check(
    "delete opens a danger confirmation",
    (await dialogTitle(page)) === "Delete pattern from the library?" &&
      (await page.$eval('[data-role="dialog"]', (el) => el.hasAttribute("data-danger"))),
  );
  await page.screenshot({ path: `${shotDir}/e2e-dialog-delete.png` });
  await page.setViewport({ width: 390, height: 780 });
  await sleep(200);
  await page.screenshot({ path: `${shotDir}/e2e-dialog-delete-390.png` });
  await page.setViewport({ width: 1400, height: 900 });
  await sleep(200);
  await cancelDialog(page);
  await page.click('[data-role="editor-back"]');
  await sleep(300);
  check("cancelled delete keeps the saved entry", (await mineNames()).includes("e2e saved"));
  await pickSource(page, "mine");
  await page.click(`${MINE} .tile [data-role="tile-face"]`);
  await sleep(400);
  await menuClick(page, "delete");
  await acceptDialog(page);
  await sleep(300);
  await page.click('[data-role="editor-back"]');
  await sleep(300);
  check("saved entry gone after delete", !(await mineNames()).includes("e2e saved"));

  // ---- mobile: two columns, and `Edit` under the name instead of the hover
  // strip a finger cannot reach (D9, mockup S1c) ----
  await pickSource(page, "library");
  await page.setViewport({ width: 390, height: 780 });
  await sleep(600);
  const cols = await page.$eval(
    `${GRID} .tiles`,
    (el) => getComputedStyle(el).gridTemplateColumns.split(" ").length,
  );
  check("mobile 390 px: the tile grid is 2 columns", cols === 2, `${cols} columns`);
  const editLinkShown = await page.$eval(
    `${TILE} [data-role="tile-edit-link"]`,
    (el) => getComputedStyle(el).display !== "none",
  );
  check("mobile 390 px: an Edit link sits under the name", editLinkShown);
  // a tile must FIT its column: `1fr`'s implicit min is min-content, and a
  // long nowrap name used to widen the track past half the screen
  const tileOverflow = await page.$$eval(
    `${TILE}:not([hidden])`,
    (els) => els.filter((e) => e.getBoundingClientRect().right > 391).length,
  );
  check("mobile 390 px: no tile overflows the column", tileOverflow === 0, `${tileOverflow} wide`);
  await page.screenshot({ path: `${shotDir}/e2e-patterns-mobile-390.png` });
  await page.setViewport({ width: 1400, height: 900 });
  await sleep(500);

  // ── 11b. render3D patterns show as rotating point-cloud tiles ──
  await page.type('[data-role="gallery-search"]', "3D Rotation");
  await sleep(300);
  const cloudTile = await page
    .waitForSelector(`${TILE}[data-kind="cloud"]:not([hidden])`, { timeout: 5000 })
    .catch(() => null);
  check("gallery has a cloud (render3D) tile", cloudTile !== null);
  if (cloudTile) {
    await cloudTile.scrollIntoView();
    await page
      .waitForFunction(
        (sel) => {
          const c = document.querySelector(sel);
          if (!c) return false;
          const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
          for (let i = 0; i < d.length; i += 4) if (d[i] + d[i + 1] + d[i + 2] > 20) return true;
          return false;
        },
        { timeout: 8000 },
        `${TILE}[data-kind="cloud"]:not([hidden]) canvas`,
      )
      .catch(() => null);
    const litCloud = await page.$eval(`${TILE}[data-kind="cloud"]:not([hidden]) canvas`, (c) => {
      const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
      let n = 0;
      for (let i = 0; i < d.length; i += 4) if (d[i] + d[i + 1] + d[i + 2] > 20) n++;
      return n;
    });
    check("cloud tile renders projected dots", litCloud > 30, `${litCloud} lit px`);
  }
  await page.$eval('[data-role="gallery-search"]', (el) => {
    el.value = "";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await sleep(250);

  // ── 12. gallery pick opens the editor on that pattern ──
  const pickName = await page.$$eval(TILE, (els) => {
    const t = els.find(
      (el) => !el.classList.contains("dead") && el.querySelector('[data-role="tile-name"]'),
    );
    t?.scrollIntoView();
    return t?.querySelector('[data-role="tile-name"]')?.textContent ?? "";
  });
  await page.click(`${TILE}:not(.dead) [data-role="tile-face"]`);
  await sleep(500);
  check("gallery pick opens the editor", (await page.$('[data-role="editor-back"]')) !== null);
  check("gallery pick loads the pattern name", (await page.$eval('[data-role="pattern-name"]', (el) => el.textContent ?? "")).trim() === pickName.trim(), pickName);
  check("picked pattern compiles", (await page.$(".banner.error")) === null);
  await page.screenshot({ path: `${shotDir}/e2e-5-final.png` });

  // The playground has no device, so it has no capacity budget to judge
  // against — and it must not sprout a device affordance to say so. A
  // deliberately array-heavy pattern still gets nothing (Gitea #15).
  await setEditor(
    page,
    "var a = array(9000)\nexport function render(index) { hsv(a[index % 9000] + index / pixelCount, 1, 1) }",
  );
  await sleep(600);
  check(
    "playground never shows a device capacity warning",
    (await page.$('[data-role="capacity-warning"]')) === null,
  );

  // ---- the Layout: Auto follows the COMPILED pattern, a choice outranks it ----
  // (Gitea #463, D7. The rig used to be derived once per pattern load and
  // latched (#372); now Auto tracks the compiled pattern continuously and an
  // explicit "Preview as" choice is the user's until they change it.)
  await page.click('[data-role="editor-back"]');
  await page.click('[data-role="new-pattern"]');
  await page.waitForSelector('[data-role="editor-back"]');
  await sleep(400);
  check("layout: a new (render) pattern starts on a strip", (await rig(page)).shape === "bar");

  await pasteEditor(page, "export function render(index) { hsv(index / pixelCount, 1, 1) }");
  check("layout: a render() pattern stays a bar", (await rig(page)).shape === "bar");

  // A renderFrame pattern that draws in coordinate space is 2D too, even
  // though it never mentions render2D (the engine's own uses_coordinate_bulk_op).
  await pasteEditor(
    page,
    "export function renderFrame() {\n  clear()\n  hsv(0.3, 1, 1)\n  fillCircle(0.5, 0.5, 0.3)\n}",
  );
  check(
    "layout: a renderFrame pattern drawing in coordinate space gets the grid",
    (await rig(page)).shape === "grid",
  );

  // An explicit choice outranks the pattern — and now also survives opening
  // another pattern, because it is the PAGE's layout, not the pattern's.
  await previewAs(page, "strip", { px: 120 });
  await pasteEditor(page, "export function render2D(index, x, y) { hsv(y, 1, x) }");
  {
    const r = await rig(page);
    check(
      "layout: an explicit Strip choice is not overridden by a 2D pattern",
      r.shape === "bar" && r.label === "120 px strip",
      JSON.stringify(r),
    );
  }
  await page.screenshot({ path: `${shotDir}/e2e-rig-manual-strip.png` });

  await page.click('[data-role="editor-back"]');
  await page.click('[data-role="new-pattern"]');
  await page.waitForSelector('[data-role="editor-back"]');
  await sleep(400);
  await pasteEditor(page, "export function render2D(index, x, y) { hsv(x, 1, y) }");
  check(
    "layout: an explicit choice survives opening another pattern",
    (await rig(page)).label === "120 px strip",
    (await rig(page)).label,
  );

  await previewAs(page, "auto");
  {
    const r = await rig(page);
    check(
      "layout: back on Auto, a render2D pattern gets the 16×16 grid",
      r.shape === "grid" && r.label === "16×16 matrix",
      JSON.stringify(r),
    );
    await page.screenshot({ path: `${shotDir}/e2e-rig-render2d-grid.png` });
  }

  // ---- device-shaped tiles + captions (the #463 payoff) ----
  // Under Auto the Library is a mix of bars and squares; under an explicit
  // Matrix every tile is square and the 1D ones say how they are projected.
  await page.click('[data-role="editor-back"]');
  await sleep(600);
  const autoKinds = await page.$$eval(`${TILE}:not([hidden])`, (els) =>
    els.map((e) => e.dataset.kind),
  );
  check(
    "tiles: Auto gives a mix of bar and grid tiles",
    autoKinds.includes("bar") && autoKinds.includes("grid"),
    [...new Set(autoKinds)].join(","),
  );
  await page.screenshot({ path: `${shotDir}/e2e-tiles-auto.png` });

  await previewAs(page, "matrix", { w: 64, h: 64 });
  await page
    .waitForFunction(
      (sel) =>
        [...document.querySelectorAll(sel)].length > 0 &&
        [...document.querySelectorAll(sel)]
          .slice(0, 12)
          .every((e) => e.dataset.kind === "grid" || e.dataset.kind === ""),
      { timeout: 10000 },
      `${TILE}:not([hidden])`,
    )
    .catch(() => null);
  const matrixKinds = await page.$$eval(`${TILE}:not([hidden])`, (els) =>
    els.slice(0, 12).map((e) => e.dataset.kind),
  );
  check(
    "tiles: Preview as 64×64 matrix makes every tile square",
    matrixKinds.every((k) => k === "grid" || k === ""),
    [...new Set(matrixKinds)].join(","),
  );
  const captioned = await page.$$eval('[data-role="tile-caption"]', (els) =>
    els.map((e) => (e.textContent ?? "").trim()),
  );
  check(
    "tiles: a 1D pattern on a matrix is captioned",
    captioned.some((c) => /^1D · /.test(c)),
    captioned.slice(0, 3).join(" | "),
  );
  await page.screenshot({ path: `${shotDir}/e2e-tiles-matrix.png` });

  await previewAs(page, "lattice", { n: 5 });
  await sleep(1500);
  const cloudKinds = await page.$$eval(`${TILE}:not([hidden])`, (els) =>
    els.slice(0, 12).map((e) => e.dataset.kind),
  );
  check(
    "tiles: Preview as 3D lattice makes every tile a cloud",
    cloudKinds.every((k) => k === "cloud" || k === ""),
    [...new Set(cloudKinds)].join(","),
  );
  await page.screenshot({ path: `${shotDir}/e2e-tiles-lattice.png` });
  await previewAs(page, "auto");

  // ── Scenes, the playground half (Gitea #480; mockups S6b · S6c · S7 · S9) ──
  //
  // D10: the tab is ALWAYS here, because hiding it in the playground would
  // make the feature undiscoverable — the page carries the empty state that
  // sets the fixture instead. The library is a localStorage blob in the same
  // WIRE FORMAT the device stores, so what survives a reload here is exactly
  // what a device would have taken.
  {
    await page.click('[data-role="editor-back"]').catch(() => {});
    await sleep(400);
    check(
      "scenes: the tab is present in the playground",
      (await page.$('[data-role="tab-scenes"]')) !== null,
    );

    // A named state: under Auto the Layout follows whatever pattern the
    // editor happens to hold, so SAY what the fixture is rather than inherit
    // the previous section's — the empty state below is about not having a
    // 2D one.
    await previewAs(page, "strip", { px: 60 });
    await page.click('[data-role="tab-scenes"]');
    await sleep(700);
    check(
      "scenes: no 2D fixture → the empty state that sets one (S6b)",
      (await page.$('[data-role="scenes-empty-fixture"]')) !== null,
    );

    await page.click('[data-role="scenes-preview-as-matrix"]');
    await sleep(900);
    check(
      "scenes: the empty state's one action makes the fixture a matrix (S6c)",
      (await page.$('[data-role="scenes-empty"]')) !== null,
    );
    await page.screenshot({ path: `${shotDir}/e2e-scenes-empty.png` });

    // `+ New scene` creates one and opens the editor ON it — the route
    // carries the id, because opening a scene does not run it (lib/router.ts)
    await page.$eval('[data-role="new-scene"]', (el) => el.click());
    await sleep(1000);
    check(
      "scenes: + New scene opens the editor on #/scenes/<id>",
      /#\/scenes\/[0-9a-f]{8}$/.test(page.url()),
      page.url(),
    );

    // Add layer ▾ — text then colour. TOP = FRONT, so the last one added is
    // the FIRST row and the wire index the row carries counts the other way.
    for (const role of ["scene-add-text", "scene-add-color"]) {
      await page.click('[data-role="scene-add-layer"]');
      await sleep(300);
      await page.click(`[data-role="${role}"]`);
      await sleep(500);
    }
    // The TYPE badges, not `data-layer`: the badge says which layer a row IS,
    // while `data-layer` is its wire index — and with two layers the indices
    // read `1,0` whichever way round the stack is.
    const kinds = () =>
      page.$$eval('[data-role="scene-editor-view"] [data-role="scene-layer"] .ty', (els) =>
        els.map((e) => e.textContent.trim()).join(","),
      );
    check("scenes: the layer list is top = front", (await kinds()) === "▭,T", await kinds());

    // the eye hides a layer; the row keeps its place and its metadata says so
    await page.click('[data-role="scene-layer"][data-layer="0"] [data-role="scene-layer-eye"]');
    await sleep(400);
    const hidden = await page.$eval(
      '[data-role="scene-layer"][data-layer="0"] .meta2',
      (el) => (el.textContent ?? "").trim(),
    );
    check("scenes: the eye hides a layer and the row reads `hidden`", hidden === "hidden", hidden);
    await page.click('[data-role="scene-layer"][data-layer="0"] [data-role="scene-layer-eye"]');
    await sleep(300);

    // selection links the list, the marquee and the inspector, and nothing
    // else (mockups.html :1285-87)
    await page.click('[data-role="scene-layer"][data-layer="0"] [data-role="scene-layer-pick"]');
    await sleep(400);
    check(
      "scenes: selecting a text layer mounts the text inspector",
      (await page.$('[data-role="scene-text-fixed"]')) !== null,
    );
    check(
      "scenes: the selected layer's box is the marquee on the stage",
      (await page.$('[data-role="scene-marquee"]')) !== null,
    );

    // drag the bottom row to the top: the destination is a 2px accent rule
    // BETWEEN rows, and the list must not reflow under the pointer (S7e)
    await page.evaluate(() => {
      const v = document.querySelector('[data-role="scene-editor-view"]');
      const rows = [...v.querySelectorAll('[data-role="scene-layer"]')];
      const g = rows[1].querySelector('[data-role="scene-layer-grip"]');
      const top = rows[0].getBoundingClientRect();
      const o = { bubbles: true, pointerId: 1, pointerType: "mouse", isPrimary: true, buttons: 1 };
      g.dispatchEvent(
        new PointerEvent("pointerdown", { ...o, clientX: top.left + 6, clientY: top.bottom + 4 }),
      );
      g.dispatchEvent(
        new PointerEvent("pointermove", { ...o, clientX: top.left + 6, clientY: top.top + 2 }),
      );
    });
    await sleep(250);
    check(
      "scenes: a drag lifts the row and shows the drop rule",
      (await page.$('[data-role="scene-layer"].drag')) !== null &&
        (await page.$('[data-role="scene-drop"]')) !== null,
    );
    await page.screenshot({ path: `${shotDir}/e2e-scenes-drag.png` });
    await page.evaluate(() => {
      const v = document.querySelector('[data-role="scene-editor-view"]');
      const g = v.querySelector('[data-role="scene-layer"].drag [data-role="scene-layer-grip"]');
      g.dispatchEvent(
        new PointerEvent("pointerup", { bubbles: true, pointerId: 1, pointerType: "mouse" }),
      );
    });
    await sleep(400);
    const reordered = await kinds();
    check("scenes: the drag reordered the stack", reordered === "T,▭", reordered);

    // Save, reload, and see the record come back — through the same wire
    // block a device would have stored
    await page.click('[data-role="scene-save"]');
    await sleep(700);
    const state = await page.$eval('[data-role="scene-save-state"]', (el) =>
      (el.textContent ?? "").trim(),
    );
    check("scenes: saving settles the save state", state === "saved", state);
    const wire = await page.evaluate(() => localStorage.getItem("luxel.scenes") ?? "");
    check(
      "scenes: the playground stores the WIRE block, not JSON",
      /^S [0-9a-f]{8} .*\nL (text|color) /m.test(wire),
      wire.slice(0, 60),
    );

    await page.reload({ waitUntil: "networkidle2" });
    await sleep(1800);
    const back = await page.$$eval(
      '[data-role="scene-editor-view"] [data-role="scene-layer"]',
      (els) => els.length,
    );
    check("scenes: a reload reopens the scene the route named", back === 2, String(back));
    await page.screenshot({ path: `${shotDir}/e2e-scenes-editor.png` });

    // ── Sprite drawing + the text inspector (Gitea #481 / #486) ───────────
    //
    // Still inside the scene the block above opened. Three things that have
    // no other home: that drawing on the preview rewrites the sprite's
    // PATTERN (and the composite shows it on the same frame), that the text
    // inspector's three source states carry the rows S7h draws, and that the
    // font picker lists the three built-ins with samples drawn through the
    // real font blobs.
    {
      // Add layer › Sprite with nothing sprite-shaped in the store MAKES one
      // (S1 and S7 draw no `New sprite…` anywhere, so this is the path).
      await page.click('[data-role="scene-add-layer"]');
      await sleep(300);
      await page.click('[data-role="scene-add-sprite"]');
      await sleep(1200);
      check(
        "sprite: Add layer › Sprite on an empty store creates a blank one",
        (await page.$('[data-role="sprite-tools"]')) !== null,
      );
      const made = await page.evaluate(() =>
        JSON.parse(localStorage.getItem("luxel.patterns") ?? "[]").some(
          (p) => p.name === "Sprite 1" && p.source.startsWith("// @sprite w=16 h=16"),
        ),
      );
      check("sprite: the new sprite is a sprite-tagged PATTERN in the store", made);

      // paint three cells, pixel-snapped, through the stage's own pointer path
      const paint = async (col, row) => {
        await page.$eval(
          '[data-role="scene-stage"]',
          (c, cx, cy, w, h) => {
            const r = c.getBoundingClientRect();
            c.dispatchEvent(
              new PointerEvent("pointerdown", {
                clientX: r.left + ((cx + 0.5) / w) * r.width,
                clientY: r.top + ((cy + 0.5) / h) * r.height,
                bubbles: true,
              }),
            );
          },
          col,
          row,
          await page.$eval('[data-role="scene-stage"]', (c) => c.width),
          await page.$eval('[data-role="scene-stage"]', (c) => c.height),
        );
        await sleep(120);
      };
      await paint(2, 3);
      await paint(3, 3);
      await paint(4, 3);
      await sleep(1200); // the store write is debounced

      // the SOURCE is what round-trips: the three texels are opaque, the rest
      // transparent, and the arrays still match the tag
      const shape = await page.evaluate(() => {
        const p = JSON.parse(localStorage.getItem("luxel.patterns") ?? "[]").find(
          (x) => x.name === "Sprite 1",
        );
        if (!p) return null;
        const arr = (n) =>
          JSON.parse(
            (new RegExp(`var ${n} = (\\[[^\\]]*\\])`).exec(p.source) ?? [])[1] ?? "null",
          );
        const v = arr("sprV");
        const h = arr("sprH");
        return v && h
          ? {
              len: v.length,
              lit: v.filter((x) => x > 0).length,
              row3: [v[3 * 16 + 2], v[3 * 16 + 3], v[3 * 16 + 4]],
              hue: h[3 * 16 + 2],
              tag: p.source.split("\n")[0],
            }
          : null;
      });
      check(
        "sprite: three painted cells round-trip through the pattern source",
        shape !== null &&
          shape.len === 256 &&
          shape.lit === 3 &&
          shape.row3.every((x) => x === 1) &&
          shape.tag === "// @sprite w=16 h=16 frames=1 fps=0",
        JSON.stringify(shape),
      );

      // …and the composite shows it. The brush starts at pure red on a blank
      // sprite, so the cell is (255, 0, 0) once the engine is rebound.
      const px = await page.$eval('[data-role="scene-stage"]', (c) => {
        const d = c.getContext("2d").getImageData(3, 3, 1, 1).data;
        return [d[0], d[1], d[2]];
      });
      check(
        "sprite: the painted pixel is in the composite",
        px[0] > 200 && px[1] < 60 && px[2] < 60,
        JSON.stringify(px),
      );

      // the palette counts what is used, and the cap is 16 cells
      const pal = await page.$eval('[data-role="scene-sprite-palette"]', (el) => ({
        used: el.getAttribute("data-used"),
        cells: el.children.length,
      }));
      check(
        "sprite: the palette is 16 cells and says how many are used",
        pal.used === "1" && pal.cells === 16,
        JSON.stringify(pal),
      );
      await page.screenshot({ path: `${shotDir}/e2e-sprite-tools.png` });

      // ---- the text inspector's three source states (S7h) ----
      // the text layer is the one the block above added first; select it by
      // its type badge
      const textRow = await page.$$eval('[data-role="scene-layer"]', (els) =>
        els.findIndex((el) => (el.querySelector(".ty")?.textContent ?? "").trim() === "T"),
      );
      await page.$$eval(
        '[data-role="scene-layer"] [data-role="scene-layer-pick"]',
        (els, i) => els[i].click(),
        textRow,
      );
      await sleep(500);

      // scroll = none has NO speed row; picking a direction brings one
      check(
        "text: Speed does not exist at Scroll = none (§5.7)",
        (await page.$('[data-role="scene-text-speed"]')) === null,
      );
      await page.select('[data-role="scene-text-scroll"]', "left");
      await sleep(400);
      check(
        "text: choosing a direction brings the Speed row (px/s)",
        (await page.$('[data-role="scene-text-speed"]')) !== null &&
          /px\/s$/.test(
            await page.$eval('[data-role="scene-text-speed-value"]', (el) =>
              (el.textContent ?? "").trim(),
            ),
          ),
      );
      await page.select('[data-role="scene-text-scroll"]', "none");
      await sleep(300);

      // the slot source: the picker, the two hint lines, and the echo
      await page.click('[data-role="scene-text-slot"]');
      await sleep(400);
      const slotRows = await page.evaluate(() => ({
        picker: !!document.querySelector('[data-role="scene-text-slot-n"]'),
        opts: document.querySelectorAll('[data-role="scene-text-slot-n"] option').length,
        hint: (
          document.querySelector('[data-role="scene-text-slot-hint"]')?.textContent ?? ""
        ).trim(),
        how: (document.querySelector('[data-role="scene-text-slot-how"]')?.textContent ?? "")
          .replace(/\s+/g, " ")
          .trim(),
      }));
      check(
        "text: the slot source adds a 0–7 picker and says who writes it (S7h)",
        slotRows.picker &&
          slotRows.opts === 8 &&
          slotRows.hint === "set from the API or Home Assistant" &&
          slotRows.how === "slots 0–7 · POST /api/text · one HA text entity each",
        JSON.stringify(slotRows),
      );

      // the playground has no API to be written from, so `Now` is where you
      // type what the API would have said — and the layer draws it
      await page.type('[data-role="scene-text-slot-value"]', "HI");
      await sleep(600);
      const drew = await page.$eval('[data-role="scene-stage"]', (c) => {
        const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
        let lit = 0;
        for (let i = 0; i < d.length; i += 4) if (d[i] + d[i + 1] + d[i + 2] > 60) lit++;
        return lit;
      });
      check("text: a slot's text reaches the composite", drew > 0, String(drew));

      // the clock source's format list is the wire's own
      await page.click('[data-role="scene-text-clock"]');
      await sleep(400);
      const fmts = await page.$$eval('[data-role="scene-text-fmt"] option', (els) =>
        els.map((e) => e.value).join(","),
      );
      check(
        "text: the clock formats are the wire's six",
        fmts === "HH:MM,HH:MM:SS,hh:MM,hh:MM:SS,MM-DD,YYYY-MM-DD",
        fmts,
      );

      // ---- the font picker (S7i) ----
      await page.click('[data-role="scene-text-font"]');
      await page.waitForSelector('[data-role="scene-font-menu"]', { timeout: 5000 });
      await sleep(400);
      const fonts = await page.evaluate(() =>
        ["tiny", "regular", "large"].map((f) => {
          const row = document.querySelector(`[data-role="scene-font-${f}"]`);
          const c = row?.querySelector("canvas.glyph");
          let ink = 0;
          if (c) {
            const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
            for (let i = 3; i < d.length; i += 4) if (d[i] > 0) ink++;
          }
          return {
            name: (row?.querySelector(".fnm")?.textContent ?? "").trim(),
            w: c?.width ?? 0,
            h: c?.height ?? 0,
            ink,
          };
        }),
      );
      check(
        "fonts: three built-ins, each sampled through its REAL blob (S7i)",
        fonts.length === 3 &&
          fonts.map((f) => f.name).join(",") === "4×6 tiny,5×7 regular,5×8 large" &&
          fonts.every((f) => f.ink > 10) &&
          // the samples differ in size, which is the thing S7i exists to show
          new Set(fonts.map((f) => `${f.w}x${f.h}`)).size === 3,
        JSON.stringify(fonts),
      );
      check(
        "fonts: there is no Upload… row (user fonts are filed, not planned)",
        !/upload/i.test(
          await page.$eval('[data-role="scene-font-menu"]', (el) => el.textContent ?? ""),
        ),
      );
      await page.screenshot({ path: `${shotDir}/e2e-font-picker.png` });
      await page.click('[data-role="scene-font-tiny"]');
      await sleep(400);
      const picked = await page.$eval('[data-role="scene-text-font"]', (el) =>
        (el.textContent ?? "").trim(),
      );
      check("fonts: picking one closes the menu and shows it", picked === "4×6 tiny", picked);
    }

    // Everything above this line is UNSAVED (the scene was saved with two
    // layers before it). Reload back onto the stored record so the tile check
    // below still counts what the store holds.
    await page.reload({ waitUntil: "networkidle2" });
    await sleep(1800);

    // back to the grid: the tile grid is the composite, and `3 layers` is the
    // one piece of scene-specific metadata it carries (S6)
    await page.click('[data-role="scene-editor-back"]');
    await sleep(900);
    const sub = await page.$eval('[data-role="scene-tile-layers"]', (el) =>
      (el.textContent ?? "").trim(),
    );
    check("scenes: the tile says how many layers", sub === "2 layers", sub);
    await page.screenshot({ path: `${shotDir}/e2e-scenes-grid.png` });

    // leave the playground the way the later sections expect it
    await page.click('[data-role="tab-patterns"]');
    await sleep(500);
    await previewAs(page, "auto");
  }
  // ── Layout-gated text completions and docs (Gitea #486, mockup S2f) ──────
  //
  // "The editor never offers a builtin that would silently do nothing on the
  // device you are connected to" (S2f's note): the five text builtins are in
  // the completion list and the docs index only on a regular 2D matrix — the
  // same Layout gate that hides the Scenes tab. In the playground that gate
  // is the `Preview as` chip, which is why this runs here and not only on a
  // console.
  {
    const TEXT_BUILTINS = ["drawText", "drawNumber", "textWidth", "font", "textSlot"];
    const CM = '[data-role="editor-view"]:not([hidden]) .cm-content';
    /** Type `prefix` on a FRESH line and let the popup settle.
     *
     *  A new line, not a cleared document: `completeFromList` hands CodeMirror
     *  a result with a `validFor`, and CM keeps filtering that result while the
     *  word at the same offset still matches it — so a second prefix typed at
     *  offset 0 is filtered against the FIRST prefix's options and comes back
     *  empty, which reads exactly like "the builtin is gated". */
    const typePrefix = async (prefix) => {
      await page.$eval(CM, (el) => el.focus());
      await page.keyboard.down("Control");
      await page.keyboard.press("End");
      await page.keyboard.up("Control");
      await page.keyboard.press("Enter");
      for (const ch of prefix) await page.keyboard.press(ch);
      await sleep(900);
    };
    const read = () =>
      page.evaluate(() =>
        [...document.querySelectorAll(".cm-tooltip-autocomplete li .cm-completionLabel")].map((e) =>
          (e.textContent ?? "").trim(),
        ),
      );
    /** The completion labels a FRESH editor offers for `prefix`.
     *
     *  Fresh, because CodeMirror's completion state is sticky within one
     *  editor: `completeFromList` hands it a result with a `validFor`, and a
     *  second prefix in the same session is filtered against the FIRST one's
     *  options and comes back empty — which reads exactly like "the builtin
     *  is gated", and cost this harness three runs to see. */
    const offered = async (prefix) => {
      await page.click('[data-role="editor-back"]').catch(() => {});
      await sleep(400);
      await page.click('[data-role="new-pattern"]');
      await page.waitForSelector(CM, { timeout: 8000 });
      await sleep(900);
      await typePrefix(prefix);
      let labels = await read();
      for (let i = 0; i < 6 && labels.length === 0; i++) {
        await sleep(400);
        labels = await read();
      }
      return labels;
    };

    await page.click('[data-role="tab-patterns"]');
    await sleep(400);
    await previewAs(page, "matrix", { w: 32, h: 32 });

    const onMatrix = [
      ...(await offered("text")),
      ...(await offered("draw")),
      // `font` matches neither prefix — a popup only ever shows what matches
      // what you typed
      ...(await offered("fon")),
    ];
    check(
      "completions: the five text builtins are offered on a matrix (S2f)",
      TEXT_BUILTINS.every((b) => onMatrix.includes(b)),
      JSON.stringify([...new Set(onMatrix)]),
    );

    // the docs card carries the signature, the grid rule and the example
    await offered("drawT");
    const card = await page.evaluate(() => {
      const el = document.querySelector('[data-role="editor-view"]:not([hidden]) .cm-completionInfo');
      return el
        ? {
            sig: (el.querySelector(".sigl")?.textContent ?? "").trim(),
            ps: [...el.querySelectorAll("p")].map((p) => (p.textContent ?? "").trim()),
            ex: (el.querySelector(".ex")?.textContent ?? "").trim(),
          }
        : null;
    });
    check(
      "docs: the card is signature · what it does · the grid rule · an example (S2f)",
      card !== null &&
        card.sig === "drawText(text, x, y[, align])" &&
        card.ps.length === 2 &&
        /Needs a real 2D grid/.test(card.ps[1] ?? "") &&
        card.ex === "drawText(textSlot(0), x, 28)",
      JSON.stringify(card),
    );
    await page.screenshot({ path: `${shotDir}/e2e-text-completions.png` });
    await page.keyboard.press("Escape");
    await sleep(200);

    // …and absent on a strip, in both the list and the docs index
    await page.click('[data-role="editor-back"]');
    await sleep(400);
    await previewAs(page, "strip", { px: 60 });
    const onStrip = [
      ...(await offered("text")),
      ...(await offered("draw")),
      ...(await offered("fon")),
    ];
    check(
      "completions: the text builtins are ABSENT on a strip (S2f's note)",
      onStrip.length > 0 && TEXT_BUILTINS.every((b) => !onStrip.includes(b)),
      JSON.stringify(onStrip),
    );
    // the other builtins are still there — the gate is per-entry, not a kill
    check(
      "completions: a strip still offers everything that works on one",
      onStrip.includes("drawLine"),
      JSON.stringify(onStrip.slice(0, 8)),
    );
    await page.click('[data-role="editor-back"]');
    await sleep(400);
    await previewAs(page, "auto");
  }


  // ── §5.7 sweep on the playground's surfaces (Gitea #529) ──
  // Same invariant device-e2e asserts on the console: a control is ABSENT
  // unless the thing it acts on exists, and the only element allowed to be
  // `[disabled]` is one gating a BUDGET, which must carry `data-reason`.
  // The playground's own case is the DEAD tile — a library pattern that does
  // not compile. It used to render a dimmed dead `Open`; now it has no verb
  // at all and says why.
  {
    // A dead tile on demand: the shipped library all compiles, so plant one
    // in the local library (the playground's `Mine` source) rather than hope
    // the corpus is checked out.
    await page.click('[data-role="editor-back"]').catch(() => {});
    await sleep(400);
    await page.evaluate(() => {
      const list = JSON.parse(localStorage.getItem("luxel.patterns") ?? "[]");
      list.push({ name: "zz broken", source: "export function render(i) { this is not luxel }", savedAt: Date.now() });
      localStorage.setItem("luxel.patterns", JSON.stringify(list));
    });
    await page.reload({ waitUntil: "networkidle0" });
    await sleep(1500);
    // the reload resumes the working copy, which lands in the editor
    await page.click('[data-role="editor-back"]').catch(() => {});
    await page.waitForSelector('[data-role="patterns-source-mine"]', { timeout: 8000 });
    await sleep(400);
    await pickSource(page, "mine");
    await page.waitForSelector(`${MINE} .tile.dead`, { timeout: 15000 });
    await page.$(`${MINE} .tile.dead`).then((el) => el.hover());
    await sleep(250);
    const shape = await page.$eval(`${MINE} .tile.dead`, (el) => ({
      dimmed: el.querySelectorAll("[disabled]").length,
      face: el.querySelector('[data-role="tile-face"]')?.tagName.toLowerCase() ?? "",
      why: (el.querySelector('[data-role="tile-dead"]')?.textContent ?? "").trim(),
      // a broken pattern of your OWN must stay fixable and deletable
      fixable: el.querySelector('.actions [data-role="tile-edit"]') !== null,
    }));
    check(
      "§5.7: a tile that does not compile is inert and SAYS why, never dimmed",
      shape.dimmed === 0 &&
        shape.face === "span" &&
        /does not compile/.test(shape.why) &&
        shape.fixable,
      JSON.stringify(shape),
    );
    await page.screenshot({ path: `${shotDir}/e2e-dead-tile.png` });
    const sweepPage = async (what) => {
      const bad = await disabledSweep(page);
      check(`§5.7: nothing is disabled-without-a-reason (${what})`, bad.length === 0, JSON.stringify(bad));
    };
    await sweepPage("playground · Patterns, mine (with a dead tile)");
    await pickSource(page, "library");
    await sleep(900);
    await sweepPage("playground · Patterns, library");
    await page.click('[data-role="new-pattern"]');
    await page.waitForSelector('[data-role="editor-back"]', { timeout: 8000 });
    await sleep(600);
    await sweepPage("playground · editor");
    await page.click('[data-role="editor-back"]');
    await sleep(300);
    await page.evaluate(() => {
      const list = JSON.parse(localStorage.getItem("luxel.patterns") ?? "[]");
      localStorage.setItem(
        "luxel.patterns",
        JSON.stringify(list.filter((p) => p.name !== "zz broken")),
      );
    });
  }

  check("no page errors", pageErrors.length === 0, pageErrors.slice(0, 3).join(" | "));
} finally {
  await browser.close();
  server.kill();
}

if (fails.length > 0) {
  console.error(`\n${fails.length} FAILURES: ${fails.join(", ")}`);
  process.exit(1);
}
console.log("\ne2e: all checks pass");
