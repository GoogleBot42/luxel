// Gallery tags (`src/lib/tags.ts`) — the library's `// tags:` line, the shape
// control and the theme chips on the Patterns page.
//
// The last tests read the real shipped files: every `gallery.json` entry must
// carry a shape and at least one theme (the generator writes what the lint
// passed), and the three copies of the vocabulary — lib/tags.ts,
// web/tools/gen-gallery.mjs and tools/check-library.sh — must agree.
//
// Run: `npm test` from web/.
import test from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import {
  defaultShape,
  isShape,
  isTheme,
  parseShapeChoice,
  shapeMatches,
  shapeOf,
  SHAPES,
  tagShowsOn,
  themeCounts,
  themeMatches,
  THEMES,
} from "../src/lib/tags.ts";

const repo = fileURLToPath(new URL("../..", import.meta.url));

test("shape control: Strip offers strip + both, Panel offers panel + both", () => {
  const strip = ["strip", "ambient"];
  const panel = ["panel", "game"];
  const both = ["both", "nature"];
  for (const t of [strip, panel, both]) assert.equal(shapeMatches(t, "all"), true);
  assert.deepEqual(
    [strip, panel, both].map((t) => shapeMatches(t, "strip")),
    [true, false, true],
  );
  assert.deepEqual(
    [strip, panel, both].map((t) => shapeMatches(t, "panel")),
    [false, true, true],
  );
  // an untagged entry is never hidden by the shape control
  assert.equal(shapeMatches(undefined, "panel"), true);
  assert.equal(shapeOf(["ambient", "panel"]), "panel");
  assert.equal(shapeOf(undefined), null);
});

test("a `both` tag puts a strip + panel file on a strip Layout, and nothing else moves", () => {
  assert.equal(tagShowsOn(["both", "ambient"], 1), true);
  assert.equal(tagShowsOn(["both", "ambient"], 2), false);
  assert.equal(tagShowsOn(["panel", "ambient"], 1), false);
  assert.equal(tagShowsOn(["strip", "ambient"], 1), false);
  assert.equal(tagShowsOn(undefined, 1), false);
});

test("theme chips: OR over the picked themes, none picked = all", () => {
  const t = ["panel", "game", "retro"];
  assert.equal(themeMatches(t, []), true);
  assert.equal(themeMatches(t, ["retro"]), true);
  assert.equal(themeMatches(t, ["clock", "game"]), true);
  assert.equal(themeMatches(t, ["clock", "sound"]), false);
  assert.equal(themeMatches(undefined, ["clock"]), false);
  assert.equal(themeMatches(undefined, []), true);
});

test("theme badges count under the current shape only", () => {
  const pool = [
    ["strip", "ambient"],
    ["panel", "ambient", "nature"],
    ["both", "nature"],
    undefined,
  ];
  const all = themeCounts(pool, "all");
  assert.equal(all.ambient, 2);
  assert.equal(all.nature, 2);
  assert.equal(all.game, 0);
  const strip = themeCounts(pool, "strip");
  assert.equal(strip.ambient, 1);
  assert.equal(strip.nature, 1);
  const panel = themeCounts(pool, "panel");
  assert.equal(panel.ambient, 1);
  assert.equal(panel.nature, 2);
});

test("default shape follows the fixture; a stored pick parses strictly", () => {
  assert.equal(defaultShape(1), "strip");
  assert.equal(defaultShape(2), "panel");
  assert.equal(defaultShape(3), "all");
  assert.equal(defaultShape(null), "all");
  assert.equal(parseShapeChoice("panel"), "panel");
  assert.equal(parseShapeChoice("both"), null);
  assert.equal(parseShapeChoice(null), null);
});

test("every library pattern's `// tags:` line follows `// name:` with one shape and a theme", () => {
  const dir = `${repo}/library`;
  const bad = [];
  for (const f of readdirSync(dir).filter((f) => f.endsWith(".js"))) {
    const m = readFileSync(`${dir}/${f}`, "utf8").match(/^\/\/ name:.*\r?\n\/\/ tags:(.*)$/m);
    if (!m) {
      bad.push(`${f}: no tags line after // name:`);
      continue;
    }
    const words = m[1].split(",").map((w) => w.trim());
    const unknown = words.filter((w) => !isShape(w) && !isTheme(w));
    const shapes = words.filter(isShape).length;
    const themes = words.filter(isTheme).length;
    if (unknown.length || shapes !== 1 || themes === 0) {
      bad.push(`${f}: ${m[1].trim()}`);
    }
  }
  assert.deepEqual(bad, []);
});

test("every gallery.json entry carries a shape tag (first) and at least one theme", (t) => {
  const p = `${repo}/web/public/gallery.json`;
  if (!existsSync(p)) {
    t.skip("public/gallery.json not built — `npm run build` (or node tools/gen-gallery.mjs) first");
    return;
  }
  const entries = JSON.parse(readFileSync(p, "utf8"));
  assert.ok(entries.length > 0);
  const bad = entries
    .filter((e) => {
      const tags = e.tags;
      return (
        !Array.isArray(tags) ||
        !isShape(tags[0]) ||
        tags.filter(isShape).length !== 1 ||
        !tags.slice(1).some(isTheme) ||
        !tags.every((x) => isShape(x) || isTheme(x))
      );
    })
    .map((e) => `${e.name}: ${JSON.stringify(e.tags)}`);
  assert.deepEqual(bad, []);
});

test("the three copies of the vocabulary agree", () => {
  const sorted = (a) => [...a].sort();
  const gen = readFileSync(`${repo}/web/tools/gen-gallery.mjs`, "utf8");
  const arr = (name) => {
    const m = gen.match(new RegExp(`const ${name} = \\[([^\\]]*)\\]`));
    assert.ok(m, `gen-gallery.mjs has no ${name}`);
    return [...m[1].matchAll(/"([a-z]+)"/g)].map((x) => x[1]);
  };
  assert.deepEqual(sorted(arr("SHAPES")), sorted(SHAPES));
  assert.deepEqual(sorted(arr("THEMES")), sorted(THEMES));
  const sh = readFileSync(`${repo}/tools/check-library.sh`, "utf8");
  const words = (name) => {
    const m = sh.match(new RegExp(`^${name}="([^"]*)"`, "m"));
    assert.ok(m, `check-library.sh has no ${name}`);
    return m[1].split(/\s+/).filter(Boolean);
  };
  assert.deepEqual(sorted(words("TAGS_SHAPES")), sorted(SHAPES));
  assert.deepEqual(sorted(words("TAGS_THEMES")), sorted(THEMES));
});
