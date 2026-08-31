---
name: cleanroom-port
description: Reimplementing a pattern idea from the scraped Pixelblaze corpus into library/ without licensing contamination — use whenever corpus code, identifiers, or constants might otherwise leak into a deliverable.
---

The corpus (`corpus/*.epe`, ~293 patterns fetched by `tools/corpus/fetch.mjs` from
patterns.electromage.com, gitignored, never redistributed) is community-owned with
unknown licensing. The one-line policy lives in `CLAUDE.md` and
`.claude/rules/corpus-cleanroom.md`; this skill owns the full procedure. The full
283-pattern sweep is done — every corpus-derived pattern already has a spec in
`docs/pattern-specs/` and an implementation in `library/`. Use this skill for new
patterns (after a `tools/corpus/fetch.mjs` refresh) or when deliberately porting an idea
you found some other way (a screenshot, a description, a video).

**Hard rule: the same context must never hold both the original source and the new
implementation.** Never copy code, identifier names, or literal numeric constants from
the original — describe magnitudes qualitatively ("a few percent chance per frame", not
"0.02"). This applies even to your own summarizing/paraphrasing in a single context: if
you've read the `.epe`, don't also write the `library/` file yourself in that same
conversation.

## Procedure

1. Pick the corpus file by name/slug only. Do not open or read the `.epe` yourself in
   the orchestrating context — that context is the one that will (or might later) also
   touch `library/`, and reading the source there breaks the firewall for good.
2. Spawn a describer subagent (Agent tool) whose only input is the path to that single
   `.epe` file. Its job: read *only* that file and write a prose-only functional spec to
   `docs/pattern-specs/<slug>.md` — what it looks like, its algorithm/state in words, no
   code blocks, no copied identifiers, no copied numeric literals. Existing files in
   `docs/pattern-specs/` (e.g. `1d-aurora-borealis.md`) show the expected shape: a "What
   it looks like" section and an "Algorithm" section in plain prose. Instruct the
   subagent's final report back to you to likewise contain no source code or verbatim
   corpus text — a confirmation and the spec's file path are enough.
3. In a fresh context (or the orchestrator, as long as it never opened the `.epe`), read
   *only* `docs/pattern-specs/<slug>.md` and write fresh code to `library/<slug>.js`
   from scratch, matching the existing `library/*.js` conventions (a `// name:` header
   comment, since `gen-gallery.mjs` uses it for the gallery tile's display name).
4. Note provenance in the new file's header comment: pattern name and "clean-room
   reimplementation from prose spec" (see any existing `library/*.js` file's header for
   the exact phrasing convention already in use).
5. Validate: `node web/tools/gen-gallery.mjs` (or `npm run build`/`npm run dev` in
   `web/`, which call it) picks up the new `library/*.js` file into `gallery.json`
   automatically — no registration step. If you touched several patterns or want a
   compile-compat sanity check, `cargo build --release -p luxel-cli && node
   tools/corpus/report.mjs` runs `luxel check` over the corpus and refreshes
   `tools/corpus/last-report.json` — useful for gauging corpus-wide compatibility, but
   note it checks `corpus/*.epe` directly, not your new `library/` file; it's a
   sanity/context check, not a gate for this specific port. To validate the new pattern
   itself, run it through the playground (`verify-webui` skill) or `luxel check` on the
   single file.

## Measuring an original is not reading it

The firewall is about SOURCE, not behaviour. Running the original through the
verify harness and reading numbers back out is explicitly fine, and it is how
fidelity work should be settled — `tools/verify/snap.mjs` already renders both
sides for exactly this reason. The same licence covers the original's **exported
var values**: a throwaway script that loads the `.epe`, compiles it via
`tools/verify/enginehost.mjs`, calls `setControl(name, v)` and prints `vars()`
reports only the public vars surface a client sees over PB's `/api/vars` — no
source ever enters your context. Keep the script's output to values only.

This is the definitive answer to "does my dial map the way the original's does?",
and it is much better than guessing from pixels. 2026-08-30, Gitea #181 item 5:
line-dancer-2d was filed as a Speed *authority-model* mismatch ("the original
trims a large baseline, the port is a master multiplier from standstill"). One
sweep of the original's exported `speed` var showed it is exactly `1 + 9v` —
identical to the port. The real defects were the slider DEFAULTS and a different
Twist range, which the same sweep handed over directly. Reach for this before
theorising about a control difference.

## Failure modes

- Reading the `.epe` "just to understand it" in the same context you'll write
  `library/` code in — this is the actual violation, not a technicality. Use a subagent
  even when it feels like overkill for a small pattern.
- A describer subagent's report quoting a snippet of the original source "for
  clarity" — reject/redo; the report itself must stay clean, not just the spec file.
- Copying a distinctive numeric constant (a specific decay rate, a magic frame-timing
  divisor) verbatim because it "worked" — describe the qualitative behavior and pick
  your own constant that reproduces it.
- Assuming `tools/corpus/report.mjs` validates your new `library/` file — it doesn't
  read `library/` at all; it's a corpus-vs-engine compatibility report only. UNVALIDATED
  — verify on first use whether any other script cross-checks a `library/` port against
  its source `.epe`; none was found as of this writing.
