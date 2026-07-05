// Fetch the community pattern corpus from patterns.electromage.com into
// corpus/ (gitignored — the patterns are community-owned; we fetch them
// locally for compatibility testing only, never redistribute).
//
// The list API caps limit at 100, but the search API paginates via nextUrl.
// We union several broad searches with both list orderings and fetch every
// pattern's full record.
//
// Usage: node tools/corpus/fetch.mjs

import fs from "node:fs";
import path from "node:path";

const BASE = "https://patterns.electromage.com";
const OUT = "corpus";
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function getJson(url) {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${res.status} for ${url}`);
  return res.json();
}

async function collectIds() {
  const ids = new Map(); // id → summary
  const add = (items) => {
    for (const it of items ?? []) if (!ids.has(it.id)) ids.set(it.id, it);
  };
  for (const order of ["newest", "popular"]) {
    add((await getJson(`${BASE}/api/v1/patterns?order=${order}&limit=100`)).items);
    await sleep(300);
  }
  // broad full-text terms (search covers name/description/source)
  for (const q of ["export", "render", "hsv", "function", "pixelCount", "var", "e"]) {
    let url = `${BASE}/api/v1/search?q=${encodeURIComponent(q)}&limit=100`;
    for (let page = 0; url && page < 10; page++) {
      const d = await getJson(url);
      add(d.items);
      url = d.nextUrl ? `${BASE}${d.nextUrl}` : null;
      await sleep(300);
    }
    console.error(`after q=${q}: ${ids.size} unique`);
  }
  return ids;
}

const ids = await collectIds();
fs.mkdirSync(OUT, { recursive: true });

let n = 0;
const index = [];
for (const [id, summary] of ids) {
  let rec = summary;
  if (!rec.file?.sources?.main) {
    try {
      rec = await getJson(`${BASE}/api/v1/patterns/${id}`);
      await sleep(200);
    } catch (e) {
      console.error(`skip ${id}: ${e.message}`);
      continue;
    }
  }
  const src = rec.file?.sources?.main ?? rec.sources?.main;
  if (!src) {
    console.error(`no source for ${id} (${rec.name})`);
    continue;
  }
  fs.writeFileSync(
    path.join(OUT, `${id}.epe`),
    JSON.stringify({ name: rec.name, id, sources: { main: src } }, null, 1),
  );
  index.push({
    id,
    name: rec.name,
    votes: rec.votes ?? 0,
    downloads: rec.downloads ?? 0,
  });
  n++;
}
fs.writeFileSync(path.join(OUT, "index.json"), JSON.stringify(index, null, 1));
console.error(`wrote ${n} patterns to ${OUT}/`);
