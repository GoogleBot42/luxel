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
  every device goes stale at once. **APPENDING an opcode does not need a
  bump** — a decoder that knows the new opcode still accepts every old
  blob, which is the only compatibility direction that matters here
  (devices never read blobs a newer host has not produced). The
  superinstructions (#261) went in at `0x41..0x4E` with `FORMAT_VERSION`
  left at 5, and `tests/superinsns.rs` pins that an unfused blob still
  validates and runs. Adding a builtin does not need a bump either.
  CHANGING the meaning or encoding of an existing opcode does.
- Superinstructions (`0x41..0x4E`, docs/spec/bytecode.md) are fused base
  sequences, emitted only by `compile::peephole` and executed by arms that
  must stay byte-for-byte equivalent to the sequence they replace —
  including error messages, `insn_start` attribution and the MAX_STACK
  limits the elided intermediate pushes would have hit. Two peephole rules
  make that hold and must survive any new template: never fuse across a
  JUMP TARGET, and never fuse across a SOURCE POSITION (the second is what
  keeps the debugger stopping per source line). `compile_with(src,
  CompileOpts { superinstructions: false })` / `luxel bench --no-fuse` is
  the A/B lever; `tests/superinsns.rs` compares the two directly.
- The dispatch loop's size is load-bearing. Adding arms is not free even
  for programs that never execute them: the #261 arms cost ~10–16 % of
  host throughput on the unfused path (measured `--no-fuse` vs master),
  which the fused instruction count then has to earn back. Keep new arms
  small — big shared bodies belong in `#[inline(never)]` helpers
  (`index_read`, `call_builtin_slow`, `err_static`) — but do NOT merge
  several opcodes into one arm with an inner `match opcode`: that cost
  another ~14 % when it was tried on `LoadIdx`/`CallBuiltin`. And measure
  BOTH sides (`luxel bench` with and without `--no-fuse`) before believing
  a dispatch change helped. The same tax applies to a new live VARIABLE, not
  just new arms: seeding `Vm::run`'s frame context from the caller (to skip
  its prologue) cost 16–21 % on x86 in 2026-09-06's #260 work, because the
  seed stays live across the whole loop. `Vm::run` should come out of a
  perf change byte-identical unless the change IS the loop — diff its
  disassembly to prove it.
- The per-pixel entry is `Vm::begin_pixel_pass` + `Vm::render_pixel`
  (Gitea #260), NOT `start()`: `start`/`resume` stay for the debugger and
  map mode, which is why `Engine::render_pixels` runs only when
  `!debug_enabled && !is_map`. `render_pixel` must stay semantically equal
  to `start(prog, fn_idx, &args[..argc], false)` — defaults for local slots
  past the argument count, `clear_run` on error — and folding it back into
  `start` costs ~28 % of the Xtensa entry path.
- **Engine/VM perf work needs an Xtensa instruction count, not a host
  benchmark.** x86 is out-of-order and hides windowed calls; the device is
  dispatch-bound. Build the panel image (`cd firmware && BOARD=board-
  seengreat-hub75 ./build-esp32.sh`), `xtensa-esp32s3-elf-objdump -d
  firmware/target/xtensa-esp32s3-none-elf/release/luxel-fw`, then walk the
  hot path by hand — `Engine::frame`'s pixel loop inlines `render_pixels`,
  so find its back edge (`j` to an address inside `Engine::frame`) and
  count instructions on the TAKEN path only, following each branch as the
  empty-render case would. Watch for `callx8` (each is an `entry`/`retw`
  window transition) and for ROM `memcpy`/`memset` calls, which
  `copy_from_slice`/`resize` emit for even one or two words.
- **Judge an engine change on ops/px × cycles/op, never on ops/px alone.**
  Fewer, fatter operations are not automatically faster: a superinstruction
  that dispatches more expensively, or that costs `Vm::run` a register, can
  lose. Report both halves for every candidate — ops/px from `luxel bench
  --profile` (or `tools/opbench.mjs`, which measures it itself), cycles/op
  from `tools/opbench.mjs` on the device — and keep the faster variant, not
  the shorter stream. We have one instance each way already: on x86 the
  fused stream was a wash-to-slower (register pressure), while on the S3
  fusing was −14 % / −18 % vm (#298).
- **Host benchmarks and the device disagree, routinely and by sign.** The
  in-place `binnum!`/`replace_top!` rewrite in #312 was +9 % SLOWER on x86
  and −2.3 % faster on the S3. Xtensa is the target that matters for engine
  work; use the host only for fast iteration, and never land or reject a
  dispatch change on a host number alone.
- **`Value`'s payload widths decide the discriminant's width, and that is
  hot.** With any `u16` payload rustc lays the tag out as a `u16`, so every
  `match` on a `Value` — including the `Option<Value>` niche test left by
  `Vec::pop` — needs `l32r 0xffff; and` before the compare, and Xtensa has
  no 32-bit immediate, so the mask is a literal-pool LOAD that register
  pressure re-issues at every use. Keep every `Value` payload 32-bit
  (#312: −12.5 % cycles/op, `Vm::run` −1.5 KB).
- Per-instruction bookkeeping in FIELDS of `Vm` is expensive out of all
  proportion to its instruction count: `fuel` (load/compare/decrement/store)
  and `insn_start` (a store) together cost ~7 cycles of a ~94-cycle op on the
  S3. They live in locals now; anything that must survive a `return` or a
  re-entry into the VM (`call_builtin` can call back through an array
  callback) has to be published at that boundary. Don't add a new one.
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
