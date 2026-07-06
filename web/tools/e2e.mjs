// Drive the built playground in real chromium (nix dev shell) via
// puppeteer-core: verify typing, live recompile, control readouts, //#
// hints, layout editing, and take screenshots for human review.
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

try {
  const page = await browser.newPage();
  await page.setViewport({ width: 1400, height: 900 });
  const pageErrors = [];
  // dialogs: accept the library save-name prompt and delete confirm;
  // dismiss anything else (e.g. the clipboard-fallback prompt)
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
  await page.waitForSelector(".cm-content");
  await sleep(800); // wasm load + first frames

  // 1. rendering: fps counter alive, waterfall canvas non-black
  const fpsText = await page.$eval(String.raw`[data-role="fps"]`, (el) => el.textContent ?? "");
  check("engine renders (fps > 0)", parseInt(fpsText) > 10, fpsText.trim());
  const lit = await page.$eval(".waterfall", (c) => {
    const d = c.getContext("2d").getImageData(0, 0, c.width, 3).data;
    return d.some((v, i) => i % 4 !== 3 && v > 0);
  });
  check("waterfall shows pixels", lit);
  await page.screenshot({ path: `${shotDir}/e2e-1-rainbow.png` });

  // 2. typing works (THE bug): click editor, type garbage, expect the text
  //    to land and a compile-error banner to appear while pixels keep going.
  //    `~` is an unterminated token (no bracket auto-close, no completion
  //    popup to interfere with the later fix).
  await page.click(".cm-content");
  await page.keyboard.press("End");
  await page.keyboard.type(" @@@");
  await sleep(300);
  const doc = await page.$eval(".cm-content", (el) => el.textContent ?? "");
  check("editor accepts typing", doc.includes("@@@"));
  await page.waitForSelector(".banner.error", { timeout: 3000 }).catch(() => null);
  const banner = await page.$(".banner.error");
  check("compile error banner appears", banner !== null);
  await page.waitForSelector(".cm-lintRange-error", { timeout: 2000 }).catch(() => null);
  check("error squiggle rendered", (await page.$(".cm-lintRange-error")) !== null);
  // fix it by undoing the typed garbage — restores the exact original
  // 3-line rainbow (the debugger test below needs line 3 = hsv). Robust
  // against auto-closed brackets / completion state.
  await page.keyboard.press("Escape"); // dismiss any completion popup
  await page.keyboard.down("Control");
  await page.keyboard.press("z");
  await page.keyboard.press("z");
  await page.keyboard.up("Control");
  await sleep(500);
  const fixedDoc = await page.$eval(".cm-content", (el) => el.textContent ?? "");
  check("banner clears after fix", (await page.$(".banner.error")) === null && !fixedDoc.includes("@@@"));
  await page.screenshot({ path: `${shotDir}/e2e-2-typing.png` });

  // 2.5 debugger: gutter breakpoint on the hsv line pauses per pixel
  const lineRect = await page.$$eval(".cm-line", (els) => {
    const r = els[2].getBoundingClientRect(); // line 3 = hsv(...)
    return { x: r.x, y: r.y, h: r.height };
  });
  const gutterRect = await page.$eval(".cm-bp-gutter", (el) => {
    const r = el.getBoundingClientRect();
    return { x: r.x, w: r.width };
  });
  await page.mouse.click(gutterRect.x + gutterRect.w / 2, lineRect.y + lineRect.h / 2);
  await page.waitForSelector('.debugger[data-paused="true"]', { timeout: 3000 }).catch(() => null);
  const paused = await page.$('.debugger[data-paused="true"]');
  check("breakpoint pauses execution", paused !== null);
  const status = await page.$eval(".debug-status", (el) => el.textContent ?? "").catch(() => "");
  check("paused at hsv line, pixel 0", status.includes("line 3") && status.includes("pixel 0"), status.trim());
  const stackTxt = await page.$eval(".stack", (el) => el.textContent ?? "").catch(() => "");
  check("stack shows render + index local", stackTxt.includes("render") && stackTxt.includes("index"), "");
  check("current line highlighted", (await page.$(".cm-debug-line")) !== null);
  await page.screenshot({ path: `${shotDir}/e2e-debugger.png` });

  // hover an identifier → value tooltip
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
  check("found hover target", wordRect !== null);
  if (wordRect) {
    await page.mouse.move(wordRect.x, wordRect.y);
    await sleep(500);
    const tip = await page.$eval(".cm-hover-value", (el) => el.textContent ?? "").catch(() => "");
    check("hover shows variable value", tip.includes("index = 0"), tip);
    await page.mouse.move(5, 5); // dismiss
    await sleep(200);
  }

  // step lands on the next pixel's first line
  await page.click(".db-over");
  await sleep(150);
  const status2 = await page.$eval(".debug-status", (el) => el.textContent ?? "").catch(() => "");
  check("step flows to next pixel", status2.includes("pixel 1"), status2.trim());
  // continue hits the breakpoint again on pixel 2
  await page.click(".db-continue");
  await sleep(150);
  const status3 = await page.$eval(".debug-status", (el) => el.textContent ?? "").catch(() => "");
  check("continue re-arms breakpoint", status3.includes("pixel 2"), status3.trim());

  // KITT: implicit globals (v, leds, pos) appear in the globals pane —
  // the exact scenario from review feedback
  await page.select('[data-role="pattern-picker"]', "KITT");
  await sleep(700);
  const kittLine = await page.$$eval(".cm-line", (els) => {
    const el = els[11]; // line 12: rgb(...) — right after v = leds[index]
    const r = el.getBoundingClientRect();
    return { y: r.y, h: r.height };
  });
  await page.mouse.click(gutterRect.x + gutterRect.w / 2, kittLine.y + kittLine.h / 2);
  await page.waitForSelector('.debugger[data-paused="true"]', { timeout: 3000 }).catch(() => null);
  const kittGlobals = await page
    .$eval('[data-role="globals"]', (el) => el.textContent ?? "")
    .catch(() => "");
  check(
    "globals pane shows implicit globals (v, leds, pos)",
    kittGlobals.includes("v") && kittGlobals.includes("leds") && kittGlobals.includes("pos"),
    kittGlobals.slice(0, 60),
  );
  await page.screenshot({ path: `${shotDir}/e2e-debugger-globals.png` });

  // clean up: disable debug (example switch already cleared old breakpoints)
  await page.click(".debug-toggle");
  await sleep(400);
  check("debug off resumes rendering", (await page.$(".debugger")) === null);

  // 3. Blink Fade: slider + number entry + readout reactivity
  await page.select('[data-role="pattern-picker"]', "Blink Fade");
  await sleep(600);
  await page.waitForSelector('input[type="range"]');
  const before = await page.$eval(".control .num", (el) => el.value);
  await page.$eval('input[type="range"]', (el) => {
    el.value = "0.9";
    el.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await sleep(200);
  const after = await page.$eval(".control .num", (el) => el.value);
  check("slider moves the numeric readout", before !== after && Number(after) === 0.9, `${before} → ${after}`);
  // manual number entry drives the engine too
  await page.$eval(".control .num", (el) => {
    el.value = "0.25";
    el.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await sleep(200);
  const rangeNow = await page.$eval('input[type="range"]', (el) => el.value);
  check("number entry moves the slider", Number(rangeNow) === 0.25, rangeNow);

  // 4. hints: Spinning Plasma slider bounded by //# min/max
  await page.select('[data-role="pattern-picker"]', "Spinning Plasma 2D");
  await sleep(700);
  const [mn, mx, val] = await page.$eval('input[type="range"]', (el) => [el.min, el.max, el.value]);
  check("//# hint bounds the slider", mn === "0.1" && mx === "1.5", `min=${mn} max=${mx}`);
  check("//# default applied", Number(val) === 0.45, val);
  const gridLit = await page.$eval(".grid", (c) => {
    const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
    return d.some((v, i) => i % 4 !== 3 && v > 0);
  });
  check("2D grid preview renders", gridLit);
  await page.screenshot({ path: `${shotDir}/e2e-3-plasma.png` });

  // 5. layout editing: bump grid to 24×24 and keep rendering
  await page.$eval(String.raw`[data-role="layout-w"]`, (el) => {
    el.value = "24";
    el.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await sleep(500);
  const gridW = await page.$eval(".grid", (c) => c.width);
  check("layout edit resizes the render", gridW === 24, String(gridW));

  // 6. pause stops the fps counter from advancing frames
  await page.click(String.raw`[data-role="pause"]`);
  await sleep(300);
  const litBefore = await page.$eval(".grid", (c) => c.getContext("2d").getImageData(0, 0, 8, 8).data.join());
  await sleep(400);
  const litAfter = await page.$eval(".grid", (c) => c.getContext("2d").getImageData(0, 0, 8, 8).data.join());
  check("pause freezes the preview", litBefore === litAfter);
  await page.click(String.raw`[data-role="pause"]`);

  // 7. vars watcher shows exported values
  const varText = await page.$eval("table", (el) => el.textContent ?? "").catch(() => "");
  check("var watcher lists zoom", varText.includes("zoom"));

  // 8. .epe import: upload a real corpus export → source swaps in and
  //    compiles (or at worst reports a compile error — not an import error)
  const { mkdtempSync, writeFileSync, readFileSync, readdirSync } = await import("node:fs");
  const { tmpdir } = await import("node:os");
  const { join } = await import("node:path");
  const epeDir = mkdtempSync(join(tmpdir(), "luxel-epe-"));
  const epePath = join(epeDir, "KITT.epe");
  const epeMain =
    "leader = 0\nexport function beforeRender(delta) { leader = time(0.05) }\nexport function render(index) { hsv(0, 1, saturate(1 - abs(index / pixelCount - leader) * 4)) }\n";
  writeFileSync(
    epePath,
    JSON.stringify({ name: "KITT e2e", id: "e2eTestPattern0001", sources: { main: epeMain } }),
  );
  const fileInput = await page.$('input[type="file"]');
  check("import file input present", fileInput !== null);
  await fileInput.uploadFile(epePath);
  await sleep(700);
  const importedDoc = await page.$eval(".cm-content", (el) => el.textContent ?? "");
  check("epe import replaces the source", importedDoc.includes("beforeRender"));
  check("epe import compiles (no banners)", (await page.$(".banner.error")) === null);
  const pickerLabel = await page.$eval('[data-role="pattern-picker"]', (el) => el.selectedOptions[0]?.textContent ?? "");
  check("picker shows the imported name", pickerLabel.includes("KITT e2e"), pickerLabel);

  // a broken file surfaces the import banner (and doesn't clobber source)
  const badPath = join(epeDir, "broken.epe");
  writeFileSync(badPath, "{ not json");
  await fileInput.uploadFile(badPath);
  await sleep(400);
  const importBanner = await page.$('[data-role="import-error"]');
  check("broken epe shows import error", importBanner !== null);
  const docAfterBad = await page.$eval(".cm-content", (el) => el.textContent ?? "");
  check("broken epe keeps the pattern", docAfterBad.includes("beforeRender"));
  await page.click('[data-role="import-error"] .dismiss');

  // 9. .epe export: download lands and round-trips sources.main
  const dlDir = mkdtempSync(join(tmpdir(), "luxel-dl-"));
  const cdp = await page.createCDPSession();
  await cdp.send("Browser.setDownloadBehavior", {
    behavior: "allow",
    downloadPath: dlDir,
    eventsEnabled: true,
  });
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
      round.name === "KITT e2e" &&
        typeof round.id === "string" &&
        round.id.length === 17 &&
        round.sources?.main === epeMain,
      dl,
    );
  }

  // 10. pattern browser: tab opens, examples show immediately, corpus streams
  //     in (the header shows a loading spinner until it lands), tiles animate.
  await page.click('[data-role="tab-patterns"]');
  await page.waitForSelector(".tile", { timeout: 5000 }).catch(() => null);
  // examples render right away; wait for the corpus to finish loading in
  await page
    .waitForFunction(() => document.querySelectorAll(".tile").length > 150, { timeout: 8000 })
    .catch(() => null);
  const tileCount = await page.$$eval(".tile", (els) => els.length);
  check("gallery shows examples + corpus", tileCount > 150, `${tileCount} tiles`);
  await sleep(2500); // let visible tiles compile + render a few frames
  const litTiles = await page.$$eval(".tile canvas", (cs) =>
    cs.slice(0, 12).filter((c) => {
      const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
      return d.some((v, i) => i % 4 !== 3 && v > 8);
    }).length,
  );
  check("gallery tiles animate (≥4 of first 12 lit)", litTiles >= 4, `${litTiles} lit`);
  await page.screenshot({ path: `${shotDir}/e2e-5-gallery.png` });
  const pickName = await page.$$eval(".tile", (els) => {
    const t = els.find((el) => !el.classList.contains("dead") && el.querySelector(".tname"));
    t?.scrollIntoView();
    return t?.querySelector(".tname")?.textContent ?? "";
  });
  await page.click(".tile:not(.dead)");
  await sleep(700);
  check("gallery pick returns to editor", (await page.$("main.editor-tab:not([hidden])")) !== null);
  const pickedLabel = await page.$eval(
    '[data-role="pattern-picker"]',
    (el) => el.selectedOptions[0]?.textContent ?? "",
  );
  check("gallery pick loads the pattern", pickedLabel.trim() === pickName.trim(), pickedLabel);
  check("picked pattern compiles", (await page.$(".banner.error")) === null);

  // 11. builtin hover docs: hovering `hsv` shows its signature + doc
  const hsvRect = await page.evaluate(() => {
    for (const lineEl of document.querySelectorAll(".cm-line")) {
      const walker = document.createTreeWalker(lineEl, NodeFilter.SHOW_TEXT);
      let node;
      while ((node = walker.nextNode())) {
        const i = node.textContent.indexOf("hsv");
        if (i >= 0) {
          const r = document.createRange();
          r.setStart(node, i);
          r.setEnd(node, i + 3);
          const b = r.getBoundingClientRect();
          return { x: b.x + b.width / 2, y: b.y + b.height / 2 };
        }
      }
    }
    return null;
  });
  check("found hsv hover target", hsvRect !== null);
  if (hsvRect) {
    await page.mouse.move(hsvRect.x, hsvRect.y);
    await sleep(500);
    const docTip = await page.$eval(".cm-hover-doc", (el) => el.textContent ?? "").catch(() => "");
    check("hover shows builtin signature + doc", docTip.includes("hsv(h, s, v)") && docTip.includes("pixel"), docTip.slice(0, 50));
    await page.mouse.move(5, 5);
    await sleep(200);
  }

  // 12. shareable URL: share → open the produced URL in a NEW page → the
  //     pattern rides in and compiles
  await page.click('[data-role="share"]');
  await sleep(400);
  const shareUrl = await page.url();
  check("share writes a #p= fragment", /#p(s)?=/.test(shareUrl), shareUrl.slice(-30));
  const page2 = await browser.newPage();
  await page2.goto(shareUrl, { waitUntil: "networkidle0" });
  await page2.waitForSelector(".cm-content");
  await sleep(800);
  // the current pattern at this point is the gallery-picked Rainbow
  const sharedDoc = await page2.$eval(".cm-content", (el) => el.textContent ?? "");
  check("share link restores the pattern", sharedDoc.includes("hsv(time(.1)"), sharedDoc.slice(0, 40));
  check("shared pattern compiles", (await page2.$(".banner.error")) === null);
  await page2.close();

  // 13. mapper: the map is now a debuggable Luxel program in its own editor
  //     sub-tab. Switch to it, run the default ring → scatter preview renders.
  await page.click('[data-role="subtab-map"]');
  await page.waitForSelector('[data-role="map-editor"] .cm-content', { timeout: 3000 });
  await sleep(200);
  const mapDoc = await page.$eval('[data-role="map-editor"] .cm-content', (el) => el.textContent ?? "");
  check("map editor holds a Luxel map program", mapDoc.includes("plot("), mapDoc.slice(0, 40));
  await page.click('[data-role="map-run"]');
  await sleep(700);
  const mapErr =
    (await page.$('[data-role="map-error"]')) || (await page.$('[data-role="map-compile-error"]'));
  check("map runs without error", mapErr === null);
  const mapBadge = await page
    .$eval(String.raw`[data-role="map-badge"]`, (el) => el.textContent ?? "")
    .catch(() => "");
  check("map installs (px mapped)", mapBadge.includes("60 px mapped"), mapBadge);
  const mapLit = await page.$eval(".map", (c) => {
    const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
    let lit = 0;
    for (let i = 0; i < d.length; i += 4) if (d[i] + d[i + 1] + d[i + 2] > 20) lit++;
    return lit;
  });
  check("map scatter renders lit dots", mapLit > 200, `${mapLit} lit px`);
  await page.screenshot({ path: `${shotDir}/e2e-6-mapper.png` });

  // 13b. the map program is debuggable exactly like a pattern: a gutter
  //      breakpoint on the plot() line pauses the per-pixel map run.
  const mapPlot = await page.$$eval('[data-role="map-editor"] .cm-line', (els) => {
    const i = els.findIndex((el) => el.textContent?.includes("plot("));
    if (i < 0) return null;
    const r = els[i].getBoundingClientRect();
    return { y: r.y, h: r.height };
  });
  check("found map plot() line", mapPlot !== null);
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
    const mapStatus = await page.$eval(".debug-status", (el) => el.textContent ?? "").catch(() => "");
    check("map paused at pixel 0", mapStatus.includes("pixel 0"), mapStatus.trim());
    await page.screenshot({ path: `${shotDir}/e2e-6b-map-debug.png` });
    await page.click('[data-role="map-debug"]'); // debug off → resume + install
    await sleep(400);
  }

  // 13c. a broken map surfaces a compile error (keeps the last good map)
  await page.click('[data-role="map-editor"] .cm-content');
  await page.keyboard.down("Control");
  await page.keyboard.press("a");
  await page.keyboard.up("Control");
  await page.keyboard.press("Backspace");
  await page.keyboard.type("export function render(index) { plot( }");
  await sleep(500);
  check("broken map shows compile error", (await page.$('[data-role="map-compile-error"]')) !== null);

  // back to strip restores the 1D preview; return to the pattern sub-tab
  await page.click('[data-role="map-back"]');
  await sleep(400);
  check("back to strip restores waterfall", (await page.$(".waterfall")) !== null);
  await page.click('[data-role="subtab-pattern"]');
  await sleep(200);

  // 14. pattern library: save under a name, reload restores the working
  //     copy (autosave), the saved entry loads from the picker, delete works
  await page.click('[data-role="save"]');
  await sleep(400);
  const savedOpt = await page.$$eval('[data-role="pattern-picker"] option', (els) =>
    els.some((o) => o.value === "saved:e2e saved"),
  );
  check("save adds a library entry", savedOpt);
  await sleep(1200); // let the autosave debounce flush
  // drop the #p= fragment left by the share test — a share hash rightly
  // outranks the autosave on load, and here we want the autosave path
  await page.evaluate(() => history.replaceState(null, "", location.pathname));
  await page.reload({ waitUntil: "networkidle0" });
  await page.waitForSelector(".cm-content");
  await sleep(800);
  const restoredLabel = await page.$eval(
    '[data-role="pattern-picker"]',
    (el) => el.selectedOptions[0]?.textContent ?? "",
  );
  check("reload restores the working copy", restoredLabel.trim() === "e2e saved", restoredLabel);
  // switch away, then load the saved entry back from the picker
  await page.select('[data-role="pattern-picker"]', "Rainbow");
  await sleep(400);
  await page.select('[data-role="pattern-picker"]', "saved:e2e saved");
  await sleep(500);
  const savedDoc = await page.$eval(".cm-content", (el) => el.textContent ?? "");
  check("saved entry loads from the picker", savedDoc.includes("hsv"), savedDoc.slice(0, 30));
  await page.click('[data-role="delete"]');
  await sleep(400);
  const goneOpt = await page.$$eval('[data-role="pattern-picker"] option', (els) =>
    els.some((o) => o.value === "saved:e2e saved"),
  );
  check("delete removes the library entry", !goneOpt);

  await page.screenshot({ path: `${shotDir}/e2e-4-final.png` });

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
