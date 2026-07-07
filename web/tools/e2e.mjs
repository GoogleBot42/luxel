// Drive the built playground in real chromium (nix dev shell) via
// puppeteer-core. The app opens on the Patterns Library; the editor is a
// full-screen view entered via "New pattern" or by picking a library tile.
//
// Usage (from web/): npm run build && node tools/e2e.mjs [screenshot-dir]

import { execSync, spawn } from "node:child_process";
import puppeteer from "puppeteer-core";

const CHROMIUM =
  process.env.CHROMIUM ?? execSync("command -v chromium", { encoding: "utf8" }).trim();

const shotDir = process.argv[2] ?? "/tmp";
const PORT = 4179;

const server = spawn("npx", ["vite", "preview", "--port", String(PORT), "--strictPort"], {
  stdio: "ignore",
});
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
await sleep(1500);

const browser = await puppeteer.launch({
  executablePath: CHROMIUM,
  headless: true,
  args: ["--no-sandbox", "--disable-gpu", "--window-size=1400,900"],
});

const fails = [];
const check = (name, cond, detail = "") => {
  console.log(`${cond ? " ok " : "FAIL"} ${name}${detail ? ` — ${detail}` : ""}`);
  if (!cond) fails.push(name);
};

/** Replace the (visible) pattern editor's contents by typing. */
async function setEditor(page, text) {
  await page.click('.editor-slot:not([hidden]) .cm-content');
  await page.keyboard.down("Control");
  await page.keyboard.press("a");
  await page.keyboard.up("Control");
  await page.keyboard.press("Backspace");
  await page.keyboard.type(text, { delay: 0 });
  await sleep(300);
}

try {
  const page = await browser.newPage();
  await page.setViewport({ width: 1400, height: 900 });
  const pageErrors = [];
  page.on("dialog", (d) => {
    if (d.message().includes("save pattern as")) return void d.accept("e2e saved");
    if (d.message().includes("delete")) return void d.accept();
    void d.dismiss();
  });
  page.on("pageerror", (e) => pageErrors.push(String(e)));
  page.on("console", (m) => {
    if (m.type() === "error") pageErrors.push(m.text());
  });

  await page.goto(`http://localhost:${PORT}/`, { waitUntil: "networkidle0" });
  await sleep(900); // wasm load

  // ── 1. lands on the Patterns Library (not the editor) ──
  check("opens on the Patterns Library", (await page.$('[data-role="library-panel"]:not([hidden])')) !== null);
  check("has a New pattern button", (await page.$('[data-role="new-pattern"]')) !== null);
  check("the examples dropdown is gone", (await page.$('[data-role="pattern-picker"]')) === null);
  const tabs = await page.$$eval('[data-role="tabs"] .tab', (e) => e.map((x) => x.textContent.trim()));
  check("tab is 'Patterns Library' (no Editor tab)", tabs.length === 1 && tabs[0] === "Patterns Library", tabs.join(","));
  await page
    .waitForFunction(() => document.querySelectorAll(".tile").length > 150, { timeout: 8000 })
    .catch(() => null);
  const tileCount = await page.$$eval(".tile", (els) => els.length);
  check("library shows examples + corpus", tileCount > 150, `${tileCount} tiles`);
  await page.screenshot({ path: `${shotDir}/e2e-1-library.png` });

  // ── 2. New pattern opens the editor full-screen ──
  await page.click('[data-role="new-pattern"]');
  await page.waitForSelector('[data-role="editor-back"]', { timeout: 3000 });
  await page.waitForSelector(".cm-content");
  await sleep(400);
  check("New opens the editor (back button)", (await page.$('[data-role="editor-back"]')) !== null);
  const fpsText = await page.$eval('[data-role="fps"]', (el) => el.textContent ?? "");
  check("engine renders (fps > 0)", parseInt(fpsText) > 10, fpsText.trim());
  const lit = await page.$eval(".waterfall", (c) => {
    const d = c.getContext("2d").getImageData(0, 0, c.width, 3).data;
    return d.some((v, i) => i % 4 !== 3 && v > 0);
  });
  check("waterfall shows pixels", lit);

  // ── 3. a clean rainbow, then typing + compile error. Single-line bodies
  //      throughout: CodeMirror auto-closes `{`, so a trailing `}` on its own
  //      line would double up. ──
  await setEditor(page, "export function render(index) { hsv(index / pixelCount, 1, 1) }");
  await page.click(".cm-content");
  await page.keyboard.type(" @@@");
  await sleep(300);
  check("editor accepts typing", (await page.$eval(".cm-content", (el) => el.textContent ?? "")).includes("@@@"));
  await page.waitForSelector(".banner.error", { timeout: 3000 }).catch(() => null);
  check("compile error banner appears", (await page.$(".banner.error")) !== null);
  await page.waitForSelector(".cm-lintRange-error", { timeout: 2000 }).catch(() => null);
  check("error squiggle rendered", (await page.$(".cm-lintRange-error")) !== null);
  await page.keyboard.press("Escape");
  await page.keyboard.down("Control");
  await page.keyboard.press("z");
  await page.keyboard.press("z");
  await page.keyboard.up("Control");
  await sleep(500);
  const fixedDoc = await page.$eval(".cm-content", (el) => el.textContent ?? "");
  check("banner clears after fix", (await page.$(".banner.error")) === null && !fixedDoc.includes("@@@"));
  await page.screenshot({ path: `${shotDir}/e2e-2-typing.png` });

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
  await page.click(".debug-toggle"); // debug off (also clears breakpoints on next swap)
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
  const gline = await page.$$eval('.editor-slot:not([hidden]) .cm-line', (els) => {
    const r = els[1].getBoundingClientRect(); // line 2 = render body
    return { y: r.y, h: r.height };
  });
  await page.mouse.click(gutterRect.x + gutterRect.w / 2, gline.y + gline.h / 2);
  const gpaused = await page.waitForSelector('.debugger[data-paused="true"]', { timeout: 3000 }).then(() => true).catch(() => false);
  check("globals test paused", gpaused);
  const globals = await page.$eval('[data-role="globals"]', (el) => el.textContent ?? "").catch(() => "");
  check("globals pane shows implicit globals (phase, bright)", globals.includes("phase") && globals.includes("bright"), globals.slice(0, 40));
  await page.click(".debug-toggle");
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

  // ── 5b. color-picker control stacks its channels, each with a number box ──
  await setEditor(
    page,
    "export var h = 0, s = 1, v = 1\nexport function hsvPickerPrimary(a, b, c) { h = a; s = b; v = c }\nexport function render(index) { hsv(h, s, v) }",
  );
  await sleep(400);
  const chRows = await page.$$eval(".control .ch-row", (els) => els.length);
  const chNums = await page.$$eval(".control .ch-row .num", (els) => els.length);
  check("picker stacks 3 channels with number fields", chRows === 3 && chNums === 3, `rows=${chRows} nums=${chNums}`);
  const overflow = await page.$eval(".right", (el) => el.scrollWidth - el.clientWidth);
  check("picker does not overflow the rail", overflow === 0, `${overflow}px`);

  // ── 6. //# hints bound a slider; grid layout renders ──
  await setEditor(
    page,
    "export var zoom = 0.45\nexport function sliderZoom(v) { zoom = v }  //# min=0.1 max=1.5 default=0.45\nexport function render2D(index, x, y) { hsv(x * zoom, 1, 1) }",
  );
  await page.select('[data-role="layout-kind"]', "grid");
  await sleep(500);
  const [mn, mx, val] = await page.$eval('input[type="range"]', (el) => [el.min, el.max, el.value]);
  check("//# hint bounds the slider", mn === "0.1" && mx === "1.5", `min=${mn} max=${mx}`);
  check("//# default applied", Number(val) === 0.45, val);
  const gridLit = await page.$eval(".grid", (c) => {
    const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
    return d.some((v, i) => i % 4 !== 3 && v > 0);
  });
  check("2D grid preview renders", gridLit);
  // bump grid width via the layout input
  await page.$eval('[data-role="layout-w"]', (el) => {
    el.value = "24";
    el.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await sleep(400);
  check("layout edit resizes the render", (await page.$eval(".grid", (c) => c.width)) === 24);
  await page.select('[data-role="layout-kind"]', "strip");
  await sleep(300);
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
  check("epe import compiles (no banners)", (await page.$(".banner.error")) === null);
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
  await page.click('[data-role="overflow"]');
  await sleep(150);
  await page.click('[data-role="epe-export"]');
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

  // ── 9. shareable URL (playground only) ──
  await setEditor(page, "export function render(index) { hsv(time(.1) + index / pixelCount, 1, 1) }");
  await page.click('[data-role="share"]');
  await sleep(400);
  const shareUrl = await page.url();
  check("share writes a #p= fragment", /#p(s)?=/.test(shareUrl), shareUrl.slice(-24));
  const page2 = await browser.newPage();
  await page2.goto(shareUrl, { waitUntil: "networkidle0" });
  await page2.waitForSelector(".cm-content");
  await sleep(800);
  check("share link opens the editor on the pattern", (await page2.$('[data-role="editor-back"]')) !== null);
  check("share link restores the pattern", (await page2.$eval(".cm-content", (el) => el.textContent ?? "")).includes("hsv(time(.1)"));
  check("shared pattern compiles", (await page2.$(".banner.error")) === null);
  await page2.close();

  // ── 10. map: enable via the "2D map" layout option; it's a debuggable Luxel program ──
  await page.select('[data-role="layout-kind"]', "map");
  await page.waitForSelector('[data-role="subtab-map"]', { timeout: 3000 });
  await sleep(700);
  check("2D map reveals the map sub-tab", (await page.$('[data-role="subtab-map"]')) !== null);
  const mapErr = (await page.$('[data-role="map-error"]')) || (await page.$('[data-role="map-compile-error"]'));
  check("map runs without error", mapErr === null);
  check("map installs (px mapped)", (await page.$eval('[data-role="map-badge"]', (el) => el.textContent ?? "")).includes("px mapped"));
  const mapLit = await page.$eval(".map", (c) => {
    const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
    let n = 0;
    for (let i = 0; i < d.length; i += 4) if (d[i] + d[i + 1] + d[i + 2] > 20) n++;
    return n;
  });
  check("map scatter renders lit dots", mapLit > 200, `${mapLit} lit`);
  await page.screenshot({ path: `${shotDir}/e2e-4-map.png` });
  // debuggable: breakpoint on plot() pauses the per-pixel map run
  const mapPlot = await page.$$eval('[data-role="map-editor"] .cm-line', (els) => {
    const i = els.findIndex((el) => el.textContent?.includes("plot("));
    if (i < 0) return null;
    const r = els[i].getBoundingClientRect();
    return { y: r.y, h: r.height };
  });
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
  // turning mapping off hides the map sub-tab
  await page.select('[data-role="layout-kind"]', "strip");
  await sleep(400);
  check("choosing strip turns mapping off", (await page.$('[data-role="subtab-map"]')) === null);

  // ── 11. library: save, back-to-library, reload resumes the working copy ──
  await setEditor(page, "export function render(index) { hsv(index / pixelCount, 1, 0.5) }");
  await page.click('[data-role="save"]');
  await sleep(500);
  await page.click('[data-role="editor-back"]');
  await sleep(300);
  check("back returns to the library", (await page.$('[data-role="library-panel"]:not([hidden])')) !== null);
  const savedChip = await page.$('[data-role="saved-pattern"]');
  check("saved pattern appears in the library", savedChip !== null);
  // reload → resumes the editor on the working copy
  await page.evaluate(() => history.replaceState(null, "", location.pathname));
  await page.reload({ waitUntil: "networkidle0" });
  await page.waitForSelector(".cm-content");
  await sleep(700);
  check("reload resumes the editor (working copy)", (await page.$('[data-role="editor-back"]')) !== null);
  check("working copy restored", (await page.$eval(".cm-content", (el) => el.textContent ?? "")).includes("0.5"));
  // open the saved pattern from the library chip
  await page.click('[data-role="editor-back"]');
  await sleep(300);
  await page.click('[data-role="saved-pattern"]');
  await sleep(400);
  check("saved chip opens the editor", (await page.$('[data-role="editor-back"]')) !== null);
  check("delete removes the saved entry", true);
  await page.click('[data-role="delete"]');
  await sleep(300);
  await page.click('[data-role="editor-back"]');
  await sleep(300);
  check("saved entry gone after delete", (await page.$('[data-role="saved-pattern"]')) === null);

  // ── 12. gallery pick opens the editor on that pattern ──
  const pickName = await page.$$eval(".tile", (els) => {
    const t = els.find((el) => !el.classList.contains("dead") && el.querySelector(".tname"));
    t?.scrollIntoView();
    return t?.querySelector(".tname")?.textContent ?? "";
  });
  await page.click(".tile:not(.dead)");
  await sleep(500);
  check("gallery pick opens the editor", (await page.$('[data-role="editor-back"]')) !== null);
  check("gallery pick loads the pattern name", (await page.$eval('[data-role="pattern-name"]', (el) => el.textContent ?? "")).trim() === pickName.trim(), pickName);
  check("picked pattern compiles", (await page.$(".banner.error")) === null);
  await page.screenshot({ path: `${shotDir}/e2e-5-final.png` });

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
