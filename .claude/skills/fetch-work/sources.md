# Where candidate work lives

Shared source list for the `fetch-work` and `unblock` skills. Both skills
read this file instead of keeping their own copy — if you add a source,
add it here once.

## Repo backlogs

- `docs/ideas.md` — extension ideas: builtins, language, engine,
  integration. Items are tagged effort `[S/M/L]` and value `★`–`★★★`, and
  many are already marked DONE. Only propose items that are *not* marked
  DONE. The stale-open entries the 2026-09-01 sweep hit were fixed in the
  doc itself on 2026-09-08 (Gitea #247) — gallery search, render3D tiles,
  sync pattern distribution and flash-mapped library execution now read
  DONE with their citations, and the residue is ticketed inline (#356 the
  1D waterfall tile, #355 the mapper niceties, #417/#439 the ad-hoc upload
  path). This is still the richest repo backlog: the genuinely open items
  are the language tier (block-scoped `let`, a string type, named/default
  parameters), the engine compositor, and the M5 peripherals/audio tier.
- `docs/pattern-ideas.md` — research backlog for example patterns (effects
  from WLED, FastLED, Aurora, etc. not yet reimplemented in the corpus).
  **EXHAUSTED — skip it.** Every shortlist and stretch item ships in
  `library/`, the clean-room corpus port is complete, and every
  engine/builtin want it generated (blur2D, bulk array math, event
  injection, analytic-derivative noise) is in `BUILTINS`. The doc carries a
  "FULLY IMPLEMENTED" banner as of 2026-09-08 (#247); it is history now.
  Don't re-verify it — read the banner and move on.
- `docs/webui.md` — the web UI redesign backlog. **EXHAUSTED — skip it.**
  Every item is ✅ (the last unticked headings, Phase 1 and "Settings
  page", were ticked 2026-09-08 under #247) and the driving ticket Gitea #4
  is closed. Its only live residue is ticketed and lives in ideas.md:
  **#355** (map drag-editing, Fill/Contain) and **#356** (1D waterfall
  gallery tiles). The doc carries a status banner saying so.
- `docs/UNTESTED.md` — untested-risk journal: machine-verified work
  Jeremy hasn't personally clicked through on the wall unit yet. Items are
  checkboxes; unchecked = still open. This is human-verification work, not
  new agent work — useful for `unblock`, and for `fetch-work` when Jeremy
  wants "things ready for you to check."
- `UPDATES.md` — the running changelog. Grep it for markers that flag
  unfinished threads:
  ```
  grep -inE 'follow-up|deferred|next:|TODO|open item' UPDATES.md
  ```
  Treat every hit as a candidate to verify (it may since have shipped —
  check newer UPDATES.md entries and the code before proposing it).
- `docs/mic-bringup.md` — onboard-mic bring-up plan. Contains an open,
  blocked plan: the mic type/pins are unknown closed hardware, and the
  next step is a bench session only Jeremy can do (identify the mic
  package, report `type + pins`). Relevant to both `fetch-work` (once
  unblocked) and `unblock` (it's blocked right now).

## Gitea issues

`tea issues list --repo zuckerberg/luxel --output simple` — Jeremy's own
tracker for feature-sized work. `tea` is only on PATH inside `nix develop`
in this container (bare shell: `command not found`, 2026-09-01); wrap it
like every other devshell tool. (the WLED installer page, hosted
playground, ESPNow, image display, …). To read ONE issue's body it's
`tea issues --repo zuckerberg/luxel <n>` — there is no `show`
subcommand (`tea issues show <n>` silently re-prints the whole
list) — and it prints the BODY only, so read the comments too: corrections
that supersede the body land there (#329's baseline was corrected in a
comment). Some issues predate work that has
since shipped or partially shipped — the dedupe-against-reality rule
applies with extra force here; check UPDATES.md and comment threads
before proposing one.

## Agent memory

`/home/googlebot/.claude/projects/-home-googlebot-workspace-pixler/memory/`

The memory index (`MEMORY.md`) and its linked files record open bugs,
pending follow-ups, and hardware state that don't live in the repo, e.g.
the Athom intermittent pre-guard first-boot heap-regions panic. Read
`MEMORY.md` first, then follow links for entries that look unresolved
(mentions of "OPEN", "next:", a bug without a matching fix in a later
entry).

Note: `bevy_voxel` is Jeremy's *other* project (a different repo). It has
its own tracked issues but is out of scope for this repo's `fetch-work`
and `unblock` skills — don't pull work from it.

## Source TODO/FIXME markers

Grep the source tree for inline markers (not just docs):

```
grep -rnE 'TODO|FIXME|XXX' --include='*.rs' --include='*.ts' --include='*.tsx' .
```

Treat hits the same as `UPDATES.md` markers: verify still-open before
proposing.

Note (2026-08-30): this vein is currently DRY — a full sweep (including
`HACK`/`WIP`/`todo!()`/`#[ignore]`/`it.skip` variants and the
`tools/corpus/report.mjs` TODO_BUILTINS derivation, which is empty) found
zero actionable code markers; every raw hit was a placeholder
(`luxel-XXXX`), prose, or history. Keep the grep as a cheap first pass,
but expect nothing — the productive sources are the UPDATES.md marker
grep, docs/UNTESTED.md, and the Gitea tracker.

## "Already shipped" is a higher bar than "something like it shipped"

Dropping a candidate needs the SHIPPED WORK to cover the ticket's ask, not
merely to have touched the same area or removed the same motivation. On
2026-09-07 Gitea #340 ("pattern partition format is extremely inefficient —
fixed slots, ~12 patterns") was proposed as a duplicate of #330's extent
store, which landed the same day and reads like an exact match: exact-size
extents, sequential, mappable, `MAX_PATTERNS` 24 → 32. Jeremy said no. The
two things #340 actually asks for survived #330 untouched — allocation is
still PAGE-granular (a 2 KB pattern still burns a 4 KiB page) and
`MAX_PATTERNS` is still a hard compile-time cap set by a fixed-size
directory item, not by space used.

So: read the ticket's ask as a list of properties, and check each one
against the code. A design ticket whose motivation was partly addressed is
still open. When in doubt, PROPOSE it with a note on what shipped nearby —
listing something Jeremy can dismiss in one word is far cheaper than
silently dropping work he wanted.

## Honesty rule (applies everywhere above)

Docs and memory drift out of date. Before proposing *any* candidate,
confirm it's still open: check whether `UPDATES.md` already records it as
shipped, whether the code already does it, and whether a memory entry
supersedes it. Never propose something already done.

Verify against `origin/master` (after a `git fetch`), not the main
checkout's HEAD — the checkout routinely sits days behind while sessions
merge PRs continuously. On 2026-08-30 a sweep run against a six-day-stale
detached HEAD listed the post-process chain (blur/glow/palette) as open
work when it had already shipped.
