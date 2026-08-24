# Running judge batches (orchestrator side)

`JUDGE.md` is the judge-facing procedure; this file is the loop that
FEEDS judges, reconstructed so a re-judge session (queued in Gitea #84 /
#99 / #101 after engine fixes) doesn't re-derive it. The 2026-08 sweep ran
~59 batches this way.

## The loop

1. Work in a fresh worktree (see .claude/skills/worktree-setup); symlink
   `corpus/` from the main checkout (never commit the symlink).
2. Pick the next N slugs (max 5 — Jeremy's cap on parallel judges):

   ```
   nix develop -c node -e "
   const fs=require('fs');
   const pairs=JSON.parse(fs.readFileSync('tools/verify/pairs.json','utf8')).pairs;
   const done=new Set(fs.readdirSync('tools/verify/results').filter(f=>f.endsWith('.json')).map(f=>f.replace(/\.json$/,'')));
   console.log(pairs.map(p=>p.slug).filter(s=>!done.has(s)).slice(0,5).join('\n'));"
   ```

3. Launch N parallel **Opus** judge subagents (Opus is the standing model
   choice — see CLAUDE.md Subagents), one per slug, each with the prompt
   template below. Prime a judge with known context where it saves runs:
   sibling-pair verdicts, family defects (e.g. "perlin siblings ran
   15-25x slow"), sensor-model facts for sound patterns, the wall-clock
   engine gap for clock patterns, rig hints for cloud/3D slugs.
4. As each verdict lands, read its friction notes — recurring traps get
   folded into JUDGE.md immediately (that's how JUDGE.md grew ~30 notes),
   genuine harness bugs get fixed in snap.mjs, engine findings get queued
   in SWEEP-NOTES.md and posted to Gitea #84 at milestones.
5. When the batch completes: `git add tools/verify/ && git commit` (one
   commit per batch, plus any JUDGE.md/harness edits) and push. Launch
   the next batch.

## Judge prompt template

> You are an output-only verification judge for the Luxel pattern port
> sweep. Work in `<worktree>`.
>
> Your assigned pattern pair slug: **`<slug>`**
>
> First read `<worktree>/tools/verify/JUDGE.md` completely and follow it
> exactly. It is the authoritative procedure: firewall rules (you must
> NEVER read files under corpus/, library/, docs/pattern-specs/, or the
> harness source in tools/verify/*.mjs — you judge purely from rendered
> output), the mandatory baseline + survey runs, temporal/multi-frame
> verification requirements, the verdict JSON schema, and score anchors.
>
> [optional priming: sibling verdicts, family defects, sensor/clock/rig
> notes — always ending "judge only from what you observe"]
>
> Run the harness only via: `nix develop -c node tools/verify/snap.mjs
> <slug> [flags]` (from the worktree root). Remember patterns are
> ANIMATIONS — verify across multiple frames and time windows, check
> steady-state after --skip, and test dials/controls on both sides. Use
> unique filenames in the scratchpad if you write temp files (other
> judges run concurrently), and use bash syntax for shell loops (the
> login shell is fish, but the Bash tool runs bash).
>
> When done, Write your verdict JSON to
> `<worktree>/tools/verify/results/<slug>.json` following the JUDGE.md
> schema exactly (slug, verdict, score, confidence, summary,
> observations[], dials[], feedback[], experiments[]). Record specific,
> detailed observations — not just the verdict: concrete measurements,
> timings, colors, per-dial effects, and actionable feedback for a
> future fix pass.
>
> Your final message: the verdict JSON you wrote, plus a brief note on
> any harness/procedure friction you hit.

## Notes

- Concurrent judges share the worktree's `tools/verify/out/` — they must
  use unique labels; collisions have happened via truncation-derived
  label names. `nix develop` prints a harmless "Git tree is dirty"
  warning once results files exist.
- Re-judging a slug: just delete/overwrite its `results/<slug>.json` and
  hand it to a judge again; the harness is deterministic and the
  provenance hashes in meta.json will show whether harness/port/epe
  changed since the prior verdict.
- Slugs currently awaiting re-judge after engine fixes: fire-blue,
  fire-red, spring-colors (32.768 s freeze, Gitea #106); both
  music-sequencers (#99 sentinel strip); the silent-null six if the cause
  turns out engine-side (#108).
- Re-judged 2026-08-23 after the #104/#105 fixes: pixelclock (close 6),
  naturallightsync (close 5), sunrise-alarm-clock (close 6),
  utility-scheduled-percent-on-demo (close 7), static-random-colors
  (close 6), synchronized-random-numbers (divergent 4 → re-judged again
  after the #112 pow-saturation fix → divergent 5: engine side settled,
  remaining defects are the port's 5/3 scroll speed and its
  periodic-bounded jitter where the original random-walks unboundedly).
