# Library overhaul — shared brief for every subagent

Repo worktree (ALL reads and edits happen here, never in /home/googlebot/workspace/pixler):

    W=/home/googlebot/workspace/pixler/.claude/worktrees/bridge-cse_014QiyztoBqGP3qwhg36jCbt

Luxel is a FOSS Pixel Blaze-compatible LED controller. The pattern language is a JS
subset with 16.16 fixed-point numbers and LED builtins; read `$W/docs/lang.md` for the
language and the full builtin reference (sections: Builtin reference, Whole-frame
rendering/`renderBulk`, Text, Control bounds). Scenes (layer stacks of patterns, text,
sprites, colour washes with blend modes, ramps, projection) are `$W/docs/spec/scenes.md`;
the shipped scene/sprite source grammar is `$W/library/README.md`. House style for
library patterns is `$W/.claude/rules/library.md` (real-unit `//#` control bounds,
never a pattern-level brightness control, `gridIndex` not row arithmetic in renderBulk).

The library today: 308 patterns in `$W/library/*.js` (255 clean-room ports of a scraped
Pixel Blaze community corpus, 53 curated originals), 11 sprites in `library/sprites`,
5 scenes in `library/scenes`. Each port's prose spec is `$W/docs/pattern-specs/<slug>.md`.

## Jeremy's goals for this overhaul (the owner's words, paraphrased)

1. **Dedupe.** Many patterns are near-identical; combine them, with controls/modes where
   the variants earn it.
2. **Add.** New patterns, own ideas and ideas found online.
3. **Fix the clean-room ports.** The old ports chased pixel-for-pixel fidelity and the
   results were often poor. New principle: **capture the IDEA and make it better on our
   own.** A pattern identical to the original is not the goal — it is harmful. Use the
   engine's strengths (simplex/curl noise, OKLCH, easings, renderBulk, canvases,
   post-process chain, scenes) to make something better.
4. **Performance.** Many patterns could be far cheaper — algorithm changes, renderBulk,
   fillNoise/fillCanvas, fewer per-pixel builtins, smaller canvases. The device is an
   ESP32 (Xtensa, no FPU, 16.16 math); a 64x64 HUB75 panel (4096 px) and a 128x128 chain
   (16384 px) are the real targets, plus 60–300 px strips.
5. **Coolness.** Patterns that are sad, lame, uninteresting, or test/utility scaffolding
   have no place in the shipped library. Remove them.
6. **Patternflow.** Clean-room the ideas from the Patternflow project's patterns too, with
   the same "better, not identical" principle; one source idea may become several Luxel
   patterns or scenes.
7. **Scenes/sprites are first-class.** Some examples are better as an example scene than
   as a pattern; some patterns are inefficient precisely because they should be a scene
   or a sprite. Converting is in scope.
8. **Engine features** that would help the library (e.g. scripted scenes / scripted layer
   positioning) are in scope to propose.

Jeremy explicitly authorizes: removing/deleting patterns, rewriting ports from scratch,
converting examples to scenes/sprites.

## Licensing firewall (unchanged, non-negotiable)

- The scraped corpus `/home/googlebot/workspace/pixler/corpus/*.epe` and the Patternflow
  sources (MIT, but treated the same way) are read ONLY by describer agents, who write
  prose-only specs (no code, no identifier names, no copied numeric literals).
- An implementer agent never reads a corpus `.epe`, a Patternflow source, or any
  original — only prose specs and the existing `library/` code.
- The same agent context must never hold both an original source and a new library file.

## Output discipline

- Scratch files go under `$SCRATCH/libfix/<your-task-name>/` where
  `SCRATCH=/tmp/claude-1000/-home-googlebot-workspace-pixler--claude-worktrees-bridge-cse-014QiyztoBqGP3qwhg36jCbt/478e04dd-960d-5a5c-b1f6-15fd084b8103/scratchpad`.
- Tools (`node`, `cargo`, `python3`) exist only inside the devshell:
  `nix develop $W --command bash -c '<cmd with absolute paths>'`. No `perl`. A
  python one-liner with apostrophes dies inside that quoting — write scripts to a file.
- Never `git stash`, never `git add -A`, never touch the main checkout, never `pkill -f`.
- Report back concisely; the orchestrator reads many reports.
