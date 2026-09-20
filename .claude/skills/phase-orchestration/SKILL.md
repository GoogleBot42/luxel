---
name: phase-orchestration
description: Running a multi-ticket phase (a milestone of 10+ Gitea tickets spanning web, core and firmware) as waves of Opus subagents, one PR per ticket — use for web UI v2 Phase B/C or any comparable epic, not for a single ticket.
---

How Phase A of the web UI v2 (epic #461, 15 tickets, one day, 2026-09-19) was
run. The shape recurs for Phase B (#477–#482) and C (#483–#486).

## 1. Plan the waves from the dependency graph, not the ticket order

- Group by what they consume: API/wire tickets (fw+mirror) before the web
  pages that read them; core mechanisms (engine, wasm exports) before both.
- **Serialize tickets that touch the same firmware files in ONE agent**
  (A3 → A15 → A5 all touched `main.rs`/the outpipe; one agent, three PRs
  in sequence). Parallel agents on the same file just trade rebase pain.
- **One agent per device at a time.** Assign the device in the brief by
  name and IP; nobody else touches it. Probe every device before planning
  (`curl -sm8 …/api/status`): on 2026-09-19 two of three were off, the Athom
  came back only after its `claude-switch` plug was turned on, the panel
  arrived mid-session WiFi-only. Re-brief running agents when that changes.
- Cheap independent wins (a CI fix, a size survey) run alongside as their
  own agent/PR; don't fold them into a ticket.

## 2. The shared brief

Write ONE `brief-common.md` in the scratchpad and point every task prompt at
it; per-task prompts then carry only the ticket, its inputs (what master
already provides, by PR number and API name), deliverables, the device
assignment and the verification list. Sections that earned their place:
worktree + devshell pinning (absolute paths, `nix develop /path/to/worktree`);
the engineering rules that bite (data-roles, no mass reformat, flat
dispatcher, OTA slot, license split); device found-state rules (READ then
restore exactly, never set brightness); Gitea mechanics (store-path `tea`
binary — it is NOT on PATH even in the devshell; one `Closes #N` per line;
parse the PR number; poll commit status; merge yourself; comment on the
ticket); a self-review step; a ≤60-line final-report format that names the
new store/API/wire names the NEXT agents need. Ticket bodies go in a sibling
`tickets.md` fetched from the API so agents don't each hit Gitea.

## 3. Feed later agents from earlier reports

Each report's "for later agents" section becomes the "what master already
provides" paragraph of the next wave's prompts (PR numbers, exported names,
renamed data-roles). When a report lands mid-flight of a dependent agent,
`SendMessage` it the two-line delta (merged sha + the names) — three agents
rebased cleanly on that alone.

## 4. Gates that bit, and what to tell agents up front

- **Probe image size early on the tightest boards** — `/api/layout` was
  complete and device-verified before its +15 KB tripped the 3 % OTA floor
  (#501). Any firmware ticket's brief must name the current margin on
  `c6-devkit+hosted-ui`, `pixelblaze-v3` AND `athom-music` (not CI-gated,
  #513) and demand before/after numbers.
- A cancelled CI run reads as commit-status `failure`; since PR #498 the
  concurrency group is per-ref, so only your own force-push cancels you.
- `E2E_PORT` must be 100 apart per concurrent session (`e2e-common.mjs`
  offset table); `tools/serve-e2e.mjs`/`mqtt-e2e.mjs` still collide (#502).
- Two full-screen editors are mounted at once; scope harness selectors.

## 4b. UI fidelity is measured, not eyeballed (learned the hard way, 2026-09-19/20)

Phase A shipped green on every harness and Jeremy's verdict was "a pretty poor job of
matching the mocks" (~45 items, #538) — the briefs said "mockups for reference" and
agents treated them as inspiration. A second pass of side-by-side screenshots still left
"big mistakes" he saw at once on the panel. What closed it:

- **`web/tools/mockdiff.mjs` + `mockdiff.map.json`** — per mock frame, an element map
  (mock selector ↔ app data-role) and a computed-style/box/hover/focus/reading-order/
  copy diff with `file:line` attribution; `--sweep` for overflow, clipping, touch targets
  and tab order at five widths; `--device http://ip` read-only against a real board.
  912 deltas at first run; the hand-back bar is **0 deltas on the mirror and 0 UI deltas
  on the real boards**, with `allow` entries only for (a) deviations Jeremy asked for,
  (b) absences his product rules force, (c) ticketed features — each with a reason.
- Run it as **one closure agent per screen** with a shared `closure-common.md`
  (definition of done, allow-list policy, device read-only rules, shared-file etiquette);
  shared primitives (`app.css` range/checkbox/menu) close deltas on other screens, so
  re-run everything after each merge, and the orchestrator runs the full suite last.
- Put the bar in the FIRST brief of any UI ticket: mock frame ids, the mock's CSS quoted,
  and "mockdiff --frames X reads 0" as acceptance. It costs less than a review round.
- The instrument finds product bugs too (a console adopting the pattern's or the
  playground's geometry, #539/#573; opening a pattern hijacking the device, #563/#585) —
  treat every non-CSS finding as a ticket, not noise.

## 5. Housekeeping

- Finished agents' leftover CI pollers keep firing "completed"
  notifications — `TaskStop` the agent id once its report is in.
- Keep an orchestration memory file (which tickets merged / in flight /
  queued, device state) and update it on every completion; it is what a
  resumed session plans from.
- Close the phase with: a full e2e + `tools/ci.sh` run on merged master in
  the orchestrator's own worktree (agents verified their branches, not the
  integrated tree), a deploy to every bench device plus a real-browser
  console pass and `coldload.mjs`, the `reflect` pass, a summary comment on
  the epic, and one push notification.
