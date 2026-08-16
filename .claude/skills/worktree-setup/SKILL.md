---
name: worktree-setup
description: Making a fresh git worktree of pixler actually buildable — worktrees are missing untracked assets (corpus/, web/public/, node_modules, firmware/creds.env) that the main checkout has.
---

This repo keeps some build inputs gitignored, so a fresh `git worktree add` gives you
source without them. Fix
these before trusting any build/test failure as a real regression.

## Procedure

1. `corpus/` (scraped pattern exports, gitignored) — only needed for the local-only
   "PixelBlaze Library" tab and `pixelblaze-library.json` generation; the main
   `gallery.json` and its e2e tile-count check are built from the tracked `library/`
   directory alone (well over the e2e threshold) and do NOT require `corpus/` to pass.
   If you need the corpus tab or are doing
   corpus-related work (see `cleanroom-port`), symlink it from the main checkout:
   `ln -s /home/googlebot/workspace/pixler/corpus corpus`. **Remove the symlink before
   committing** — it shows up as untracked and you don't want it staged.
   Note: `tools/corpus/last-report.json` does NOT need copying — `gen-gallery.mjs`
   never reads it (it's written by `tools/corpus/report.mjs` and consumed only by
   docs/narrative, not by any build step).
2. `web/public/` (gitignored, holds build outputs `luxel.wasm`, `gallery.json`,
   `pixelblaze-library.json`) — absent in a fresh worktree, and `npm run wasm`'s `cp`
   step fails hard because the destination directory doesn't exist. Fix: `mkdir -p
   web/public` before the first `npm run wasm` / `npm run dev` / `npm run build`.
3. `web/node_modules` — run `npm ci` inside `nix develop` (bare shells have no `node`).
3b. `firmware/creds.env` (gitignored WiFi creds) — required by any firmware build
   (`build-esp32.sh` reads it). Copy it from the main checkout:
   `cp /home/googlebot/workspace/pixler/firmware/creds.env firmware/creds.env`.
4. `cargo` and `node` are only on `PATH` inside `nix develop`. If you need
   ImageMagick or similar one-off tools not in the flake, `nix-shell -p <pkg>` alongside
   it rather than assuming it's present.
5. Regenerate the gallery after the above: `node web/tools/gen-gallery.mjs` (or just run
   `npm run build`/`npm run dev`, which call it as a step). It writes
   `web/public/gallery.json` from `library/*.js` unconditionally, and
   `web/public/pixelblaze-library.json` from `corpus/*.epe` only if `corpus/` is present
   and non-empty (it removes stale output otherwise, so the tab disappears cleanly).
   `npm run build`'s final `vite build` step copies everything in `web/public/` into
   `web/dist/` automatically — do this *before* pointing `e2e.mjs`/`device-e2e.mjs` (which
   serve `web/dist` via `vite preview`) at the app, not after, or they'll serve a stale
   gallery.

## Failure modes

- `npm run wasm` failing with an ENOENT on the `cp` into `web/public/` — you skipped step 2.
- e2e gallery/tile-count assertions failing — almost always a stale `web/dist` (rerun
  `npm run build` after any `library/`, `corpus/`, or `gen-gallery.mjs` change), not a
  missing `corpus/` symlink.
- `cargo`/`node`: command not found — you're outside `nix develop`.
- A leftover `corpus` symlink showing up in `git status`/a diff — remove it before
  committing; it's a worktree convenience, never a tracked or intended artifact.

## Merging back (concurrent sessions)

Expect `origin/master` to have moved while you worked — other sessions
merge their own PRs continuously. Before opening the PR: `git fetch
origin master && git rebase origin/master`.

- An `UPDATES.md` conflict is the NORMAL case (every session prepends an
  entry under `# Update log`). Resolution is always: keep BOTH entries,
  yours on top (newest first), markers removed.
- After resolving, `grep -c '^<<<<<<<\|^=======$\|^>>>>>>>' UPDATES.md`
  must print 0 BEFORE `git rebase --continue` — `git add` happily stages
  a file that still contains conflict markers, and the rebase commits it
  without complaint (this has happened; the markers shipped into a
  commit and needed an amend).
- After any rebase over real upstream changes, re-run the verification
  that overlaps them (at minimum the affected test suite / e2e) before
  force-pushing — green-before-rebase proves nothing about the merged
  result.
- If `tea pr merge` fails with "is it still open?" right after a
  force-push, Gitea is still re-checking mergeability — wait a few
  seconds and retry before diagnosing anything.
- `cargo test --workspace` failing only in `luxel-cli`'s `heapstat` test with
  "web/public/gallery.json … No such file or directory" — not a regression,
  you skipped step 5 (any `npm run build` / `gen-gallery.mjs` run fixes it).
  Bitten twice in one session; check this before reading the diff.
