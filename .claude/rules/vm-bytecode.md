---
paths:
  - "crates/luxel-core/src/vm.rs"
  - "crates/luxel-core/src/bytecode.rs"
  - "crates/luxel-core/src/compile.rs"
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
- **New engine code is charged against a ~10 KB image budget.** The
  classic-ESP32 boards sit at ~4 % OTA-slot margin, and the CI gate
  (`tools/image-check.sh`, 3 % floor on `board-pixelblaze-v3`) is the
  first thing a luxel-core-only PR can fail. The sixteen `renderFrame`
  builtins (#335, ~550 lines of Rust) cost +14.9 KB on the first cut —
  four monomorphized copies of one generic `paint_shape` closure alone
  were 3.5 KB, `hsv_to_rgb`+`quantize` inlined into five loops, `format!`
  error strings, and eight inlined copies of a builtin-name table scan.
  `&mut dyn FnMut`, shared `#[inline(never)]` texel/blend helpers and
  static error strings took 6.5 KB back bit-identically. Measure with the
  ci.sh recipe (`build-esp32.sh` → `espflash save-image` →
  `image-check.sh`) against the merge base BEFORE opening the PR, and
  re-measure after any rebase — #328 moved the base by −8.5 KB.
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
- The "never across a SOURCE POSITION" rule binds the PEEPHOLE, not the
  compiler. `compile::forward_stores` (#320) breaks it on purpose — it
  deletes the `Pop; LoadL a` after a `StoreL a`, leaving the stored value
  live across the statement boundary — and pays for it with its own
  argument: identical stack depth at every point (so identical MAX_STACK
  verdicts), and only the second statement's FIRST instruction deleted,
  never its last, so its position run survives and the debugger still
  stops on it. A new pass that wants to cross a statement boundary owes
  the same two proofs plus `tests/storefwd.rs`-style evidence (pixels,
  error text and position, debugger stops AND the locals reported at each
  stop). Do not add such a rewrite as a peephole template.
- Three independent compile passes now run in a fixed order —
  `const_fold` (#312), `forward_stores` (#320), `peephole` (#261) — each
  with its own `CompileOpts` switch (`--no-fold`, `--no-storefwd`,
  `--no-fuse`). They compose, and no one of them may depend on another for
  correctness: every combination has to render identically. Measure a new
  one with the peephole OFF as well as on, because a `LoadL` that would
  have fused into a superinstruction anyway hides the saving — #320 fires
  at 1,183 library sites but nets 465 words once #261 has run.
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
- **…but "the host" includes the wasm PLAYGROUND, so a device win that
  costs the host is a user-visible regression, not a free trade.** The rule
  above is about *dispatch* shape, where the host number is just noise. It
  does NOT license arbitrary host slowdowns in `luxel-core`: the same code
  renders every preview in the browser. #312's first fmath cut replaced one
  wide machine divide with a 16-step 32-bit loop — right on Xtensa, where
  the wide form is a ROM `__divdi3` call, and **−31 % on x86** for a
  `dist`-heavy pattern. When a rewrite is only a win because the target
  lacks a 64-bit ALU, keep BOTH forms and select on the `NARROW_WORD`
  pattern in `fmath.rs`:
  `const NARROW_WORD: bool = cfg!(any(target_arch = "xtensa", target_arch =
  "riscv32"));` then `if NARROW_WORD { narrow(..) } else { wide(..) }`.
  A `cfg!()` **value**, never `#[cfg]` on the definitions — that keeps both
  forms compiled and type-checked on every target, so a host `cargo test`
  can assert `narrow == wide == reference` three ways and prove the
  device's path bit-exact. It const-folds: the Xtensa output is
  byte-identical to the ungated version. Leave a narrowing UNconditional
  only when it is neutral-or-better on both (i32 intermediates that were
  never 64-bit-wide anyway); gating those too doubles the code for noise.
- **`#[inline(never)]` on a shared arm body to shrink the dispatch loop is
  a trap.** It reads like the right lever and #312 measured it as a pure
  loss: `index_read` marked `#[inline(never)]` left the three indexing arms
  the same length or two instructions LONGER on Xtensa (it only moved bytes
  out of `Vm::run`) while costing the host ~20 % on array-heavy patterns.
  Only pull a body out of line if the ARM count drops, not just the
  function size.
- **…and `#[inline(always)]` on one is a worse trap, for the opposite
  reason.** `Vm::run` (~13 KB) and `Vm::call_builtin` (~21 KB) compete for
  the flash instruction cache, and on any pattern that calls a builtin that
  competition is worth FAR more than anything inside the dispatch loop.
  #318: inlining `binop_const` into the three `Const c; <op>` arms is
  **−7.5 % on the loop microbenchmark and +38 % on a noise-heavy pattern**;
  the one-arm variant is *smaller* and worse still (+57 %). Running the
  other way, #325 taking 1.3 KB out of `Vm::run` read −4.5 % on the loop and
  **−46.5 %** on that pattern. It is layout, not size — non-monotonic, tens
  of percent per kilobyte moved. Treat `Vm::run`'s footprint as a first-class
  cost.
- **The builtin dispatch is a THREE-tier ladder and the tiers are a cache
  budget, not a style choice** (#328). `builtin_fast` (in-loop, 22 arms) →
  `call_builtin` + `builtin_hot` (the ~30 builtins a `render` calls per
  pixel) → `builtin_cold` (`#[cold] #[inline(never)]`, the other 90 arms,
  reached only through `builtin_hot`'s `_` arm). Which tier an arm belongs
  in is measured with `tools/profile-library.mjs`, which leaves a 40× gap
  between the least-used tier-2 arm and the most-used tier-3 one; tiers 1+2
  answer 99.4 % of the library's builtin calls. A new arm goes in
  `builtin_cold` unless a pattern calls it PER PIXEL, and a fat leaf
  (noise, the wide fmath) stays `#[inline(never)]` so it is not dragged
  through the cache by patterns that never call it — inlined,
  `simplex2_inner`/`simplex3_inner` alone put 7 KB inside `builtin_hot`.
  Do not add a fourth dispatch level and do not merge these back together
  without measuring both benches on BOTH chips.
- **Cache placement is chaotic, and only IRAM makes it reproducible.** The
  hot/cold split ALONE, with `Vm::run` byte-identical (13,170 B) in both
  builds, was **+80 % on `perlin-fire-wind-tunnel` and +73 % on `snake`** on
  the Athom and a wash on the S3; with `Vm::run` pinned in `.rwtext` the
  same split is a 4.2× win on `kaleidoscope-2d`. `snake` executes nothing
  but `Vm::run`, so that swing is *pure placement*. Never explain a
  luxel-core perf result by instruction count until you have checked
  whether the functions moved (#328; docs/firmware.md "Code placement").
- **Never judge a `luxel-core` change on `tools/opbench.mjs` alone.** Its
  K-sweep loop executes no builtin, so it is blind to the term above; run
  `tools/patbench.mjs` on a builtin-heavy pattern as well and report BOTH
  (#312/#318). The probe pattern must be **stateless**: `perlin-fire-wind-
  tunnel` is a pure function of time and coordinates and repeats to ±0.3 %,
  while `snake-2d` carries game state whose per-frame work varies — it swung
  74 % between two builds a kilobyte apart and is useless as an A/B probe.
- **`opbench.mjs` counts ops with a freshly built host `luxel` but pushes
  bytecode built from `web/public/luxel.wasm`.** After a rebase over any
  compiler change, a stale wasm has the device running the OLD op stream
  while the profiler counts the NEW one — #312 read 138.7 cycles/op for a
  build that was actually 115.7, a 20 % phantom regression. Run
  `npm run wasm` before any post-rebase measurement.
- **Host `luxel bench` on this box has 6–19 % run-to-run spread.** A single
  run, or even median-of-3, will invent double-digit regressions that
  vanish on re-measurement (#312 chased three of them). Interleave the two
  binaries inside the loop, use ≥ 200 frames, and take the BEST of 5–11
  rounds — throughput noise only ever costs time, so the max is the least
  biased estimator. Treat anything inside ±3 % as noise.
- **`Value`'s payload widths decide the discriminant's width, and that is
  hot.** With any `u16` payload rustc lays the tag out as a `u16`, so every
  `match` on a `Value` — including the `Option<Value>` niche test left by
  `Vec::pop` — needs `l32r 0xffff; and` before the compare, and Xtensa has
  no 32-bit immediate, so the mask is a literal-pool LOAD that register
  pressure re-issues at every use. Keep every `Value` payload 32-bit
  (#312: −12.5 % cycles/op, `Vm::run` −1.5 KB).
- **64-bit arithmetic in `fixed.rs`/`fmath.rs` is a ROM call, and the ROM
  call is invisible in the source.** Xtensa has a 32×32→64 widening
  multiply (`mull`+`muluh`) and a 32-bit divide, and nothing wider: an
  `i64`/`i128` divide — including a *constant* divisor, which on any 64-bit
  host is a free magic-multiply — becomes `l32r` + `callx8` to
  `__udivdi3`/`__divdi3`/`__umoddi3`/`__multi3` at `0x4000xxxx`. #312 found
  141 such sites in `luxel-core` and removed 93 of them; `fmath` alone was
  19 × `__udivdi3` + 22 × `__divdi3`. Audit any new fixed-point code with
  `xtensa-esp32s3-elf-objdump -d … | grep -E "__(u?div|u?mod|mul)di3"` and
  narrow to 32 bits with the bound proved in a comment.
- Per-instruction bookkeeping in FIELDS of `Vm` is expensive out of all
  proportion to its instruction count: `fuel` (load/compare/decrement/store)
  and `insn_start` (a store) together cost ~7 cycles of a ~94-cycle op on the
  S3. They live in locals now; anything that must survive a `return` or a
  re-entry into the VM (`call_builtin` can call back through an array
  callback) has to be published at that boundary. Don't add a new one.
- `Vm::run`'s `fail!`/`push!`/`pop!` are `macro_rules!`, so anything their
  bodies name must be in scope where the MACRO IS DEFINED, not where it is
  used: a local declared later in the function is invisible to them, and a
  loop LABEL is invisible full stop (`break 'frame` inside a macro body does
  not compile). Declare a new loop-carried local ahead of the macro block,
  and exit through `return`, not a labelled break.
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
- A COMPILER-side change (`compile.rs`: the #261 peephole, the #312
  `const_fold` passes) must not be judged by host `luxel bench` throughput
  at all — not even directionally. #312's compiler pass moved it by ±17 % on
  patterns whose dynamic opcode histogram was IDENTICAL and whose `Vm::run`,
  `Engine::frame` and `call_builtin` disassembled instruction-for-instruction
  identically: the compiler is not on the hot path, so the number is pure x86
  code placement (`rainbow`'s blob is byte-identical between the binaries and
  still measured +8 %). The two numbers that mean something are dynamic
  **ops/px** (`luxel bench --profile`, `tools/profile-library.mjs`) and the
  **Xtensa instruction count** of the ops involved, from the S3 disassembly —
  fewer ops is not automatically faster, so quote both. Prove a compiler change
  did not touch the runtime by diffing the two binaries' `Vm::run`
  disassembly, and prove it changed nothing observable with byte-identical
  PPMs (`luxel run --out x.ppm`) across all 299 library patterns.
- Both compiler rewrite passes obey the same two contracts — never across a
  JUMP TARGET, never across a SOURCE POSITION — and `docs/spec/bytecode.md`
  ("Superinstructions", "Constant folding") is the reference for what a blob
  reader may therefore assume. A pass that wants to move a value ACROSS a
  statement boundary (Gitea #320) is outside that argument and needs its own:
  stepping, the variable inspector, `clear_run` on error, and `MAX_STACK`
  all see the deeper stack.
