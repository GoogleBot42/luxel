# Library overhaul, 2026-11-01 — the planning record

The artifacts the campaign that merged as PR #1009 planned from (they lived in
a session scratchpad; kept here as archaeology and as the backlog the next
session continues from). Nothing here is executed by a build.

| file | what |
|---|---|
| `brief.md`, `brief-implementer.md`, `audit-schema.md` | the shared briefs every agent read (Jeremy's goals, the firewall, the bar, the per-pattern record schema) |
| `audit/range-0{0..3}.jsonl` | one JSON record per OLD pattern (308): verdict, coolness, perf smell, merge target, action |
| `perf-sweep.md` | host perf sweep of the old library at 64x64 / 128x128 / 300 px with diagnoses |
| `ideas-online.md` | 79 new-pattern ideas from WLED/FastLED/LedFx/Aurora/Tidbyt/demoscene/generative art; ranked top 25 — the unshipped ones are the new-pattern backlog |
| `patternflow-index.md` | scores for the 34 Patternflow prose idea specs (`docs/pattern-specs/patternflow/`) |
| `patternflow-note-draft.md` | DRAFT thank-you/credit note to the Patternflow author — not sent; Jeremy decides |
| `scene-ideas.md` | 30 example scenes, 34 sprites, conversions and the scene-system gaps G1–G15 (the ones not shipped are backlog) |
| `engine-ideas.md` | 13 ranked engine/scene features; #1–#2, #4–#5 shipped as PRs #979–#984, the rest are tickets #985–#1006 |
| `PLAN.md` | the reconciled master plan: a disposition for all 308 files (§A), survivor specs (§B), new content (§C — §C.1 patterns not yet shipped are backlog), engine shortlist (§D), 40 work batches (§E) |
| `deferred-notes.md` | the ledger every batch deferred to the final pass; superseded by tickets #985–#1011 |

Procedure: `.claude/skills/phase-orchestration` §2c. Outcome: UPDATES.md 2026-11-01.

Four arcade-named patterns were renamed after the overhaul (UPDATES.md
2026-11-02): `asteroids` → `rock-field`, `breakout-2d` → `brick-breaker-2d`,
`frogger-2d` → `road-hopper-2d`, `tetrix-2d` → `falling-blocks-2d`. The
planning files above keep the old names as history.
