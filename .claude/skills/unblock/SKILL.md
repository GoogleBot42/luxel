---
name: unblock
description: Produce a ranked list of things only Jeremy can physically do (serial-flash, hold a button, plug something in, refresh a log feed) that unblock the most agent work — use when Jeremy asks what he needs to do himself or why something is stuck.
---

# Unblock

The complement to `fetch-work`: instead of ranking work the agent can do,
this ranks actions **only Jeremy can do** — things that require hands,
eyes, or physical access the agent doesn't have (no serial console, no
ability to touch hardware).

## What counts as a human action

Anything gated on physical presence or access the agent structurally
lacks. The specifics change over time — check memory and `UPDATES.md` for
current state rather than assuming any of these are still true — but the
*shape* of the thing looks like:

- Serial-flashing a wedged device (the agent has no serial connection).
- Holding a boot/flash-mode button (e.g. GPIO0) during a flash.
- Plugging in, powering, or physically rewiring a device or strip.
- Refreshing an external log feed the agent can't reach directly.
- A bench session to identify unknown hardware (e.g. probing mic pins).

## Procedure

1. Read `../fetch-work/sources.md` for where candidate work lives, and
   scan those sources plus the agent memory directory for items that are
   blocked specifically on a human action (not just "not started yet" —
   look for language like "needs you," "agent has no serial," "OPEN,"
   or a bench-procedure writeup).
2. For each blocked item, work out:
   - **The human action** — stated as a concrete, doable step.
   - **What it unblocks** — the downstream agent work that becomes
     possible once it's done (be specific: which feature, which fix,
     which verification).
   - **Effort tag** — `seconds` / `minutes` / `bench-session`, based on
     how much hands-on time it actually takes.
3. Rank by downstream work unblocked — put the action that frees up the
   most or highest-value agent work first, not the easiest action first.
4. Output a short list, ranked, one line each: action, effort tag, what it
   unblocks. Keep it actionable — this is a to-do list for Jeremy, not a
   report.

## Notes

- Cross-check against `UPDATES.md` and memory before listing anything —
  a blocker may already be resolved (e.g. a device that was wedged may
  since have been recovered).
- If nothing is currently blocked on a human action, say so plainly rather
  than padding the list with low-value items.
