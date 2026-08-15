---
name: fetch-work
description: Pull a ranked list of candidate work for Luxel — use when Jeremy asks what to do next, asks for work suggestions, or wants you to just start on the best next item.
---

# Fetch work

Use this when Jeremy asks something like "what should we do next," "suggest
some items for new work," or "pick something and start on it."

## Modes

Figure out which mode applies from how he asked:

- **User-specified kind** — he names a category ("firmware work," "web
  polish," "pattern ideas"). Only pull candidates from the matching
  source(s).
- **Agent-chooses-best** — he wants you to just start. Rank candidates,
  pick the top one, and begin working on it. Say which one you picked and
  why before you start.
- **Suggest-list-only** — he wants options, not action. Present the ranked
  list and stop; let him choose.

If it's ambiguous, default to suggest-list-only — it's cheaper to ask a
follow-up than to start the wrong thing.

## Procedure

1. Read `../fetch-work/sources.md` for the full list of where candidate
   work lives, and pull candidates from the sources relevant to the
   requested kind (or all of them, if no kind was specified).
2. **Dedupe against reality before ranking anything.** Docs and memory
   drift stale — something on a backlog list may already be shipped. For
   each candidate, check `UPDATES.md` for a shipped entry and skim the
   relevant code/docs. Drop anything already done. This is the single most
   important step — never propose finished work.
3. Rank the surviving candidates by value ÷ effort, with two hardware
   constraints:
   - Any candidate needing the physical device requires the dev unit to be
     reachable right now (ping it / check recent memory for wedged state).
     If it's not reachable, rank device-dependent items lower or flag them
     as blocked (and consider pointing Jeremy at `unblock` instead).
   - There is only **one** device. Never propose or start two device-work
     items in parallel — pick at most one live device task at a time.
4. Present the ranked list: item, one-line why, effort, and whether it
   needs the device. Give an explicit recommendation for the top pick.
5. In agent-chooses-best mode, start the top item after presenting the
   pick (a one-line "starting on X because Y" is enough — don't wait for
   confirmation unless the item touches the device or is otherwise
   destructive).

## Honesty rule

Never propose a candidate without checking it's still open. A backlog
entry, an idea doc, or a memory note is a lead, not a fact — verify
against `UPDATES.md` and the current code before it goes on the list.
