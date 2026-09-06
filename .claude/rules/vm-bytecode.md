---
paths:
  - "crates/luxel-core/src/vm.rs"
  - "crates/luxel-core/src/bytecode.rs"
---

- The `BUILTINS` table in `vm.rs` is APPEND-ONLY: the array index IS the
  runtime builtin id ("Order is the builtin id — append only"). Since LXBC
  v5 the instruction words carry that runtime id DIRECTLY (a blob executes
  in place from memory-mapped flash, so nothing can be rewritten at load);
  the per-blob import table lists every used builtin by *name + id* and the
  decoder rejects a blob whose names don't resolve to exactly those ids on
  this build (see `bytecode.rs`). Appending keeps every existing id valid —
  reordering, removing, or renaming an entry breaks every stored pattern
  that uses it, with a "recompile" error at best. Never reorder, remove, or
  rename; only append.
- v5 code is `u32` WORDS: `Program.words` (`Words::Owned` on hosts,
  `Words::Static(&'static [u32])` from `deserialize_lean_static` over a
  mapped flash slot), `FnDef.code_start/code_len` are word counts, and
  every `pc` — frames, breakpoints `(fn_idx, pc)`, `VmError.pc`, debug
  position runs, `insn_start` — is a fn-relative WORD index. The constant
  pool is raw 16.16 words in the same region: `ArrRepr::Const` reads go
  through `ArrView` (`get`/`at`/`len`/`iter`), never `&[Value]`, and
  `arr_mut` materializes an owned `Vec<Value>` on first write. Don't add a
  byte-indexed anything, and don't hand out `&[Value]` for arrays.
- Bumping `FORMAT_VERSION` is deliberate and rare: the recompile path
  (device replies `bc-version`, the web UI recompiles from source) already
  exists, so a bump costs users one recompile, but every stored blob on
  every device goes stale at once. Adding an opcode (superinstructions,
  #261) needs a bump; adding a builtin does not.
- Appending a new builtin does NOT require a bytecode format-version bump.
  Only format changes do. A version mismatch makes the device reply with
  `"code":"bc-version"`, and the web UI auto-recompiles from source in
  response — see `firmware/src/server.rs` and `web/src/App.svelte`'s handling
  of that code.
- Format depth (blob layout, import-table encoding, decode/validate rules)
  lives in docs/spec/bytecode.md; VM semantics (opcodes, builtin dispatch)
  live in docs/spec/vm.md. Read those before changing either file.
- Builtin calls with the wrong argument count DON'T error — missing args
  read as 0, extra args are dropped. So changing a builtin's signature or
  argument semantics silently breaks existing callers: the 2026-08-29
  session found four library/ ports frozen into constants because the
  perlin refit (b37df0a) made 0 octaves an empty sum and their
  old-signature calls fell into it, weeks after the change, with zero
  errors. When you change any builtin's signature/parameter meaning,
  grep library/ (and tools/) for its call sites and check every arity —
  and re-run tools/verify/snap.mjs on pairs that use it.
- `Vm.pixel_count` is 0 while top-level init runs — the engine sets it only
  AFTER `vm.call(init)` (so `mapPixels` at top level is a deliberate no-op).
  A builtin that needs the strip length during init must read the
  `pixelCount` global instead (`Vm::state_pixel_count` does exactly this
  for `setPixelState`; the 2026-09-01 batch-8 work lost a test cycle to
  seeding a buffer of length 0). Engine-owned per-frame state
  (`pixel_state_commit` today) is handed over in `Engine::finish_frame()`
  — route any new normal end-of-frame exit through it, not a bare
  `run_stage = None`.
- An engine-vs-PB semantics question in an issue or FINDINGS entry
  ("presumably runs clean on real PB") is usually decidable in minutes
  against the oracle — probe before building an engine change on the
  presumption (two of the three 2026-08-29 engine-gap issues had wrong
  premises). Probe batteries: tools/oracle/*.mjs, conventions in
  .claude/rules/oracle.md.
