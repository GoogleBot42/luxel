---
name: reflect
description: End-of-substantial-work harness review — checks whether this session learned something that should update CLAUDE.md, .claude/rules, or .claude/skills, and prunes the memory index. Use after finishing a substantial piece of work, not after small or routine tasks.
---

# Reflect

A self-maintenance pass for the Claude Code setup itself, run after
substantial work — not a summary of the work for Jeremy.

**"No changes needed" is the expected, common outcome.** Most sessions
don't surface anything worth codifying. Do not invent a change just to
have something to report; say plainly that nothing needed updating if
that's the case.

## Procedure

1. **Review what this session learned the hard way.** Look back over the
   session for: guidance you followed that turned out wrong, a gotcha that
   wasn't documented anywhere and cost time to rediscover, or a procedure
   you had to re-derive from scratch that should have been written down.
   Routine work that went smoothly on existing guidance needs no entry
   here.
2. **Patch stale guidance.** If something in `CLAUDE.md`, `.claude/rules`,
   or `.claude/skills` is now wrong or missing a gotcha, make the smallest
   correct edit. Don't rewrite surrounding material that's still accurate.
3. **Extend or propose a skill only if the procedure will recur.** A
   one-off doesn't earn a skill. If an existing skill under-covers a
   procedure you just did twice, extend it; if no skill fits and the
   procedure is clearly repeatable, propose a new one (don't create it
   unasked for something speculative).
4. **Prune the agent memory index**
   (`/home/googlebot/.claude/projects/-home-googlebot-workspace-pixler/memory/MEMORY.md`).
   If an entry's content is now codified in the repo setup (a rule, a
   skill, a doc), collapse it to a one-line "codified in X" pointer so the
   same guidance isn't loaded twice from two places. Leave entries that
   are still purely runtime/state facts (device IPs, current hardware
   status, open bugs) alone — those belong in memory, not the repo.
5. Report back tersely: either "no changes needed" or a short list of what
   was patched/pruned and why.

## Constraints

- Never move secrets (credentials, keys, tokens) out of memory into
  tracked files, even if it seems like "codifying" them. Memory is the
  right place for secrets; the repo is not.
- Keep edits minimal. This skill is about correcting drift, not
  refactoring the setup.
