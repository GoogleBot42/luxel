# Where candidate work lives

Shared source list for the `fetch-work` and `unblock` skills. Both skills
read this file instead of keeping their own copy — if you add a source,
add it here once.

## Repo backlogs

- `docs/ideas.md` — extension ideas: builtins, language, engine,
  integration. Items are tagged effort `[S/M/L]` and value `★`–`★★★`, and
  many are already marked DONE. Only propose items that are *not* marked
  DONE.
- `docs/pattern-ideas.md` — research backlog for example patterns (effects
  from WLED, FastLED, Aurora, etc. not yet reimplemented in the corpus).
- `docs/webui.md` — the web UI redesign backlog. Organized into phases;
  many entries are already checked off (✅) or struck through. Only
  unchecked, non-struck items are open.
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
tracker for feature-sized work (the WLED installer page, hosted
playground, ESPNow, image display, …). To read ONE issue's body it's
`tea issues --repo zuckerberg/luxel <n>` — there is no `show`
subcommand (`tea issues show <n>` silently re-prints the whole
list). Some issues predate work that has
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

## Honesty rule (applies everywhere above)

Docs and memory drift out of date. Before proposing *any* candidate,
confirm it's still open: check whether `UPDATES.md` already records it as
shipped, whether the code already does it, and whether a memory entry
supersedes it. Never propose something already done.
