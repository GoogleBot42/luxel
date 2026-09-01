---
name: review-decisions
description: Batch-processing Jeremy's review-UI verdicts on the ported-pattern library — the decisions.json → fix-agents → addressedAt-stamp pipeline that ran as PR #176 (pass 1) and PR #226 (pass 2).
---

Jeremy reviews pairs in the review UI (`tools/verify/review.mjs`), which
persists per-slug verdicts to the tracked `tools/verify/decisions.json` in
the MAIN checkout. "Start batching through feedback" means: act on every
entry whose `decision` is not `good` and that has no `addressedAt` stamp.

## Semantics

- `good` — no action. Old feedback text may linger on a re-decided entry;
  the current `decision` field is authoritative.
- `delete` — remove `library/<slug>.js`, `docs/pattern-specs/<slug>.md`,
  `tools/verify/results/<slug>.json`, and any `fixups.json` entries.
- `needs-work` — fix toward the feedback; fidelity to the corpus original
  still matters. **No feedback text** means Jeremy saw an obvious defect in
  the live side-by-side — diagnose from snap.mjs renders + the judge
  verdict. (Pass 2: most no-feedback entries were real bugs, half of them
  the `arrayReplace`-as-fill accumulator freeze.)
- `fork` (+ optional `forkName`) — fidelity waived; improve beyond the
  original in place (keep slug/filename + provenance header lines, add a
  "deliberately departs, <date> review" header note).
- Deletes were historically not stamped `addressedAt` (pass 1), so tally
  against `library/<slug>.js` existence, not the stamp, before redoing them.
- Re-deciding in the UI clears `addressedAt`; a re-opened entry keeps its
  old feedback with new text appended by Jeremy only sometimes — read the
  decidedAt timestamp to tell fresh feedback from stale.

## Pipeline (what worked, twice)

1. Fresh worktree from origin/master. **Commit the decisions.json snapshot
   from the main checkout as the FIRST commit**, then symlink `corpus`,
   build (`npm ci`, `npm run wasm`, `cargo build --release -p luxel-cli`,
   `gen-gallery`).
2. Do deletes yourself; regen `pairs.json` later, after all renames land.
3. Fan out fix agents (Opus) over disjoint slug batches: controls-only /
   defaults-tweaks / specific defects / no-feedback fidelity diagnosis /
   creative redesigns. Give them a shared brief file covering: the corpus
   firewall (pixels via snap.mjs allowed, corpus source forbidden), the
   house control style (real-unit `//#` bounds on the line above the
   export, defaults reproducing shipped constants, **controls-only changes
   must prove a byte-identical untouched render** via before/after
   `snap.mjs` + `cmp` on port.png, never a brightness control),
   verification via `snap.mjs --probe-controls` + `luxel run` at several
   pixel counts, and "no git commands, no shared-artifact rebuilds".
4. Afterwards: stamp `addressedAt` on every acted non-good entry (atomic
   tmp+rename), regen pairs.json + gallery, remove the corpus symlink,
   run `tools/check-library.sh` **without piping through `tail`** (the
   pipe eats the lint's failing exit code), UPDATES.md entry, rebase
   (UPDATES.md conflict = keep both, yours on top), re-verify, PR, merge.
5. After the merge, if the MAIN checkout's decisions.json is byte-identical
   to your committed snapshot (i.e. Jeremy didn't review more meanwhile),
   overwrite it with merged master's stamped version so the review UI
   shows the "addressed" chips; if it differs, leave it alone.

## Gotchas

- The `//#` directive lint flags prose comments that contain `//#` plus
  `default=`/`min=` etc. — word comments so they don't (pass 2:
  "these MUST agree with the //# default= directives below" failed the
  gate).
- Redesigned/forked patterns leave `docs/pattern-specs/<slug>.md`
  understating the port — accepted drift; the spec is provenance for the
  original description, and the file header carries the departure note.
- Agents renaming a pattern change only the `// name:` display line;
  filenames/slugs and provenance lines are load-bearing for pairs.json.
