#!/usr/bin/env node
// Dynamic opcode profile of the WHOLE library — the data that chooses
// superinstructions (Gitea #261).
//
// Runs `luxel bench <pattern> --profile --json` over every pattern in
// library/ (299 of them) on a 16x16 grid rig and sums the per-opcode,
// adjacent-pair, adjacent-triple and per-builtin dynamic counts. Every
// count is weighted by how often the instruction actually EXECUTES, so a
// pattern with a hot inner loop dominates a pattern with a long init —
// which is the right weighting for an interpreter change.
//
// "Adjacent" means statically adjacent: the second instruction is the
// first's fall-through successor in the same function, reached without a
// jump. That is exactly what a compiler-side peephole is allowed to fuse.
//
//   nix develop -c node tools/profile-library.mjs             # ranked tables
// (it builds target/profile/release/luxel with --features profile itself)
//   nix develop -c node tools/profile-library.mjs --md out.md # + markdown
//   nix develop -c node tools/profile-library.mjs --json out.json
//   PIXELS=256 FRAMES=20 GRID=16x16 node tools/profile-library.mjs
//
// A pattern whose render errors still contributes what it executed; a
// pattern that fails to compile is listed and skipped.
import { execFileSync } from 'node:child_process';
import { readdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const dir = process.env.DIR ?? join(root, 'library');
const pixels = Number(process.env.PIXELS ?? 256);
const frames = Number(process.env.FRAMES ?? 20);
const grid = process.env.GRID ?? '16x16';
// The counters are behind luxel-cli's non-default `profile` feature (they
// cost real time in the dispatch loop, so the plain `luxel bench` binary
// must not carry them). Its own target dir keeps the two builds from
// invalidating each other on every switch.
const targetDir = join(root, 'target/profile');
const bin = join(targetDir, 'release/luxel');

const argv = process.argv.slice(2);
const flag = (name) => {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : null;
};

execFileSync(
  'cargo',
  ['build', '--release', '-p', 'luxel-cli', '--features', 'profile',
    '--target-dir', targetDir],
  { cwd: root, stdio: ['ignore', 'ignore', 'inherit'] },
);

const files = readdirSync(dir).filter((f) => f.endsWith('.js')).sort();
const total = {
  files: 0,
  skipped: [],
  insns: 0,
  pixels: 0,
  ops: new Map(),
  bigrams: new Map(),
  trigrams: new Map(),
  builtins: new Map(),
};
/** Per-pattern instructions/pixel, for the outlier list. */
const perPattern = [];

const add = (map, k, v) => map.set(k, (map.get(k) ?? 0) + v);

for (const f of files) {
  const path = join(dir, f);
  let out;
  try {
    out = execFileSync(
      bin,
      ['bench', path, '--pixels', String(pixels), '--map-grid', grid,
        '--frames', String(frames), '--profile', '--json'],
      { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] },
    );
  } catch {
    total.skipped.push(f);
    continue;
  }
  const line = out.trim().split('\n').filter((l) => l.startsWith('{')).pop();
  if (!line) { total.skipped.push(f); continue; }
  const p = JSON.parse(line);
  total.files += 1;
  total.insns += p.insns;
  total.pixels += p.pixels;
  for (const [k, v] of p.ops) add(total.ops, k, v);
  for (const [k, v] of p.bigrams) add(total.bigrams, k, v);
  for (const [k, v] of p.trigrams) add(total.trigrams, k, v);
  for (const [k, v] of p.builtins) add(total.builtins, k, v);
  perPattern.push([f, p.insns_per_px]);
}

const ranked = (map, n) =>
  [...map.entries()].sort((a, b) => b[1] - a[1]).slice(0, n);
const pct = (v) => ((100 * v) / total.insns).toFixed(2);

const table = (title, rows, unit) => {
  const lines = [`\n### ${title}\n`, `| ${unit} | count | % of all insns |`,
    '|---|---:|---:|'];
  for (const [k, v] of rows) lines.push(`| \`${k}\` | ${v.toLocaleString()} | ${pct(v)} |`);
  return lines.join('\n');
};

const header = [
  `# Library-wide dynamic opcode profile`,
  '',
  `${total.files} patterns from \`${dir.replace(root + '/', '')}\`, ` +
  `${pixels} px on a ${grid} grid × ${frames} frames each.`,
  `**${total.insns.toLocaleString()} instructions** over ` +
  `${total.pixels.toLocaleString()} pixel renders — ` +
  `**${(total.insns / total.pixels).toFixed(1)} insns/px** on average.`,
  total.skipped.length ? `Skipped (did not run): ${total.skipped.join(', ')}` : '',
].join('\n');

const md = [
  header,
  table('Opcodes', ranked(total.ops, 40), 'opcode'),
  table('Adjacent pairs', ranked(total.bigrams, 40), 'pair'),
  table('Adjacent triples', ranked(total.trigrams, 40), 'triple'),
  table('Builtins', ranked(total.builtins, 30), 'builtin'),
  '',
  '### Heaviest patterns (insns per pixel)',
  '',
  '| pattern | insns/px |',
  '|---|---:|',
  ...perPattern.sort((a, b) => b[1] - a[1]).slice(0, 20)
    .map(([f, n]) => `| ${f} | ${n.toFixed(1)} |`),
  '',
].join('\n');

const mdOut = flag('--md');
if (mdOut) writeFileSync(mdOut, md);
const jsonOut = flag('--json');
if (jsonOut) {
  writeFileSync(jsonOut, JSON.stringify({
    files: total.files, skipped: total.skipped,
    insns: total.insns, pixels: total.pixels,
    ops: [...total.ops], bigrams: [...total.bigrams],
    trigrams: [...total.trigrams], builtins: [...total.builtins],
    per_pattern: perPattern,
  }, null, 1));
}
console.log(md);
