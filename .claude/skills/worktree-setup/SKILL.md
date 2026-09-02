---
name: worktree-setup
description: Making a fresh git worktree of pixler actually buildable — worktrees are missing untracked assets (corpus/, web/public/, node_modules, firmware/creds.env) that the main checkout has.
---

This repo keeps some build inputs gitignored, so a fresh `git worktree add` gives you
source without them. Fix
these before trusting any build/test failure as a real regression.

## Procedure

0. Cut the branch from **`origin/master`, not the local `master`** — the main
   checkout's `master` is only as fresh as its last `git pull`, and with sessions
   merging PRs continuously it is routinely several merges behind (seen 4 behind on
   2026-08-24). `git worktree add -b <branch> <path> master` silently starts you on
   that stale tree. Do `git fetch origin master` first and branch from `origin/master`
   (or `git reset --hard origin/master` in the new worktree).

1. `corpus/` (scraped pattern exports, gitignored) — needed by the **verify
   harness** (`tools/verify/snap.mjs`, `report.mjs`, `review.mjs`) for the
   *original* half of every judged pair, and by the local-only "PixelBlaze
   Library" tab / `pixelblaze-library.json`. The main `gallery.json` and its
   e2e tile-count check are built from the tracked `library/` directory alone
   (well over the e2e threshold) and do NOT require `corpus/` to pass.
   **snap.mjs cannot render a `library/` pattern on its own**: a paired slug
   dies with `corpus file missing: <…>.epe`, and a curated original (one in
   pairs.json's `originals` list rather than `pairs`) dies with `unknown slug`.
   So "render before/after with snap.mjs" is not available in a corpus-free
   worktree. Either symlink corpus, or — when the comparison is library-vs-
   library and corpus would only be a liability — drive `tools/verify/
   enginehost.mjs` + `png.mjs` directly from a scratch script: same engine
   wasm, same pinned seed/delta, no corpus, ~60 lines (done 2026-09-01 for the
   Gitea #225 fix pass).
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
3c. `firmware/vendor/esp-hub75` (gitignored symlink to a patched-crate store
   path) — created automatically by the devshell's shellHook, so it appears on
   the first `nix develop` in the worktree. Only bare-`cargo`-outside-the-shell
   builds ever see it missing; the fix is entering the devshell, not copying
   anything. See firmware/patches/README.md.
   **The shellHook writes that path relative to the shell's cwd**, so entering
   the devshell from inside `firmware/` does NOT create it where cargo looks:
   `cd firmware && nix develop /path/to/worktree --command ./build-esp32.sh`
   dies with `unable to update …/firmware/vendor/esp-hub75` /
   `failed to read …/Cargo.toml` on a perfectly healthy tree (2026-08-31).
   Enter the shell from the worktree ROOT and `cd` inside it:
   `nix develop --command bash -c 'cd firmware && ./build-esp32.sh'`.
3c. Device flash dumps (gitignored `*.bin` in the repo root: `athom-wled-*.bin`,
   `pb-v3-stock.bin`) — required by the QEMU suite (`tools/qemu/run-all.py`
   autodetects them in the repo ROOT of the tree it runs from;
   `takeover-test.py` takes them via `--stock`/`--fs`) and by
   `tools/wledfs-check` runs against real filesystems. Copy them in before
   running either: `cp /home/googlebot/workspace/pixler/*.bin .` — they stay
   untracked, so nothing to clean before committing.
4. `cargo`, `node` AND `python3` are only on `PATH` inside `nix develop` — the bare
   shell has none of them, so a one-liner that pipes API JSON through `python3`/`node`
   dies with "command not found" (`tools/stack-check.sh` uses python3 and is fine
   because it runs inside the shell). Outside it, parse with `grep`/`sed` or dump the
   response to a file and read it. If you need ImageMagick or similar one-off tools not
   in the flake, `nix-shell -p <pkg>` alongside it rather than assuming it's present.
4b. **Bare `nix develop` can enter the MAIN checkout's devshell, not yours.** Its own
   stderr says which: `warning: Git tree '/home/googlebot/workspace/pixler' is dirty`
   is the main checkout, `'…/<your-worktree>' is dirty` is yours. The damage is
   silent, not loud — a *relative* path inside
   `nix develop --command bash -c "cd web && node …"` once resolved `web/node_modules`
   in a THIRD session's worktree entirely, where node happily found `puppeteer-core`
   and would have driven the wrong bindings without a word (2026-08-30). Pass the
   worktree explicitly — `nix develop /path/to/worktree --command …` — and give every
   command inside it ABSOLUTE paths.
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

- **Editing the MAIN checkout by accident after reading prior art from it.**
  The classic sequence: you Read `/home/googlebot/workspace/pixler/web/src/...`
  to study existing code *before* the worktree exists, then later Edit "that
  file" — and the edit lands in another session's tree, silently. Nothing warns
  you; `git status` in your worktree stays clean while the main checkout grows a
  modification you don't own (hit 2026-08-31). Two habits: re-Read the file at
  your worktree path before the first edit to it, and if it does happen,
  `git -C /home/googlebot/workspace/pixler checkout -- <path>` immediately —
  after checking `git status` there so you don't revert someone's real WIP.
- `npm run wasm` failing with an ENOENT on the `cp` into `web/public/` — you skipped step 2.
- e2e gallery/tile-count assertions failing — almost always a stale `web/dist` (rerun
  `npm run build` after any `library/`, `corpus/`, or `gen-gallery.mjs` change), not a
  missing `corpus/` symlink.
- `cargo`/`node`: command not found — you're outside `nix develop`.
- A leftover `corpus` symlink showing up in `git status`/a diff — remove it before
  committing; it's a worktree convenience, never a tracked or intended artifact.
- `nix build .#<output>` failing with "No such file or directory" on a file that
  plainly exists — flake builds copy only **git-tracked** files into the store, so a
  brand-new untracked file (a patch, a new source file) is invisible until `git add`.
  The error message names the `git add` fix; believe it.
- A flake-output test suddenly missing its inputs (e.g. takeover-test.py: "missing
  input: result/luxel-fw-ota.bin" right after it worked) — every `nix build` reuses
  the same `./result` symlink, so building `.#qemu-espressif` clobbers the
  `.#luxel-fw-athom-music` link the test reads. Rebuild the firmware output (cached,
  seconds) or use `--out-link` for the non-firmware build.

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
- **Measurements are verification too — take them AFTER the rebase.**
  Firmware image sizes, `.stack`, heap numbers: a pre-rebase measurement
  compared against a table another session updated the same day gives a
  confidently wrong delta. 2026-08-29: the post-process chain looked like
  +10 KB on RISC-V vs +5 KB on Xtensa (an interesting-sounding codegen
  story that would have shipped into docs/boards.md); re-measured on the
  rebased tree it was an even +3 KB everywhere. The baseline had moved,
  not the cost.
- If `tea pr merge` fails with "is it still open?" right after a
  force-push, Gitea is still re-checking mergeability — wait a few
  seconds and retry before diagnosing anything.
- `cargo test --workspace` failing only in `luxel-cli`'s `heapstat` test with
  "web/public/gallery.json … No such file or directory" — not a regression,
  you skipped step 5 (any `npm run build` / `gen-gallery.mjs` run fixes it).
  Bitten twice in one session; check this before reading the diff.
- `tools/hw-bench.mjs` ENOENT on `web/public/gallery.json` or
  `web/public/luxel.wasm` — it needs BOTH (step 5 for the gallery, steps
  2–3 + `npm run wasm` for the wasm; it compiles patterns locally via
  `web/tools/lxp.mjs`). Two aborted soak launches in one session
  (2026-08-22) from doing only the gallery half. Note the wasm crash
  still exits 0 through a `| tail` pipe — check the output, not the code.
