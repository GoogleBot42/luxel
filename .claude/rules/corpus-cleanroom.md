---
paths:
  - "library/**"
  - "tools/corpus/**"
  - "docs/pattern-specs/**"
---

<!-- Deliberate duplication with CLAUDE.md hard rule — keep both. -->

- The scraped pattern corpus (`corpus/`, gitignored, licensing unknown) must
  NEVER be copied into any deliverable — no code, no identifier names, no
  copied numeric constants, in `library/`, `docs/pattern-specs/`, or anywhere
  else that ships.
- Reuse only through the describer-firewall procedure: a describer subagent
  reads the corpus and writes a prose spec with no code in it; a fresh
  implementer subagent that never sees the corpus writes new code from that
  prose. See .claude/skills/cleanroom-port for the procedure.
- Note provenance (which corpus pattern inspired this, via which spec doc) in
  the header comment of every reimplemented file under `library/`.
