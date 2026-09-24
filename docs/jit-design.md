# On-device JIT — design (v1: ESP32-S3 / Xtensa LX7)

Status: design, 2026-09-20. Tracker: Gitea #607. Research and the case for
on-device compilation: `docs/research/on-device-jit.md`. This document is
the engineering design that ticket's phases implement.

Decisions taken by Jeremy (2026-09-20):

1. The JIT lives on the device; LXBC stays the one portable format.
2. Builtins are ordinary function calls; everything else is native code.
   A running pattern is either wholly native or wholly interpreted.
3. The language does not grow type annotations. The browser-side compiler
   infers which slots never change kind and records that in the bytecode,
   so the bytecode carries both fixed-kind (unboxed) and dynamic (boxed)
   slots.
4. The Seengreat's architecture (Xtensa LX7, PSRAM) first. Every other
   board interprets the same bytecode until its backend exists.

Everything below follows from those four.

## 1. Architecture in one picture

```
 browser / CLI (luxel-core, frontend on)                 device (luxel-core, frontend off)
 ┌──────────────┐  ┌──────────────┐  ┌───────────┐       ┌──────────┐   ┌─────────────────┐
 │ source  ──►  │  │ kind         │  │ LXBC v6   │ ───►  │ decoder  │──►│ interpreter     │ every board
 │ compile      │─►│ inference +  │─►│ typed     │       │ + kind   │   │ (ignores kinds) │
 │ (unchanged)  │  │ Box insertion│  │ blob      │       │ verifier │   ├─────────────────┤
 └──────────────┘  └──────────────┘  └───────────┘       └──────────┘──►│ JIT: LXBC →     │ S3 boards
                                                                        │ Xtensa in PSRAM │ (`jit` feature)
                                                                        └─────────────────┘
```

The blob is the same everywhere; the store, the playlist, sync's
`/api/pattern.lxp` pull and the web UI never learn that native code
exists. `/api/status` gains one object, `jit`, saying what the live
program is running as and why.

## 2. Kinds in the bytecode (LXBC v6)

### 2.1 The kind lattice

```
            Dyn                 (boxed 8-byte Value; anything)
      ┌──────┼──────┬─────┐
     Num    Arr    Fun  Builtin   (unboxed 4-byte word)
             │
           ArrNum                (array whose elements are all Num)
```

`Num` is a raw 16.16 word. `Arr`/`ArrNum`/`Fun`/`Builtin` are the raw
handle/index the `Value` payload holds today. `Dyn` is a `Value` as the
interpreter stores it. A slot's kind is a **proof** that every value that
ever reaches it has that kind; the interpreter's semantics are unchanged,
the kind only fixes the representation the JIT may use.

Why a lattice this small works for this language: arithmetic, comparison
and bitwise operators always yield `Num` whatever they consume
(non-numbers coerce to 0 — `crates/luxel-core/src/vm.rs` `Value::num`), so
`Dyn` does not propagate through expressions. It only propagates through
plain copies, array loads from non-`ArrNum` arrays, call results and
conditional joins. That is why a whole-program inference over a
dynamically typed pattern language proves most slots `Num` (§9 has the
measured share).

### 2.2 What is annotated

Per program: one kind byte per global. Per function: a return kind, then
one kind byte per local SLOT (`locals` bytes — `FnDef::locals` counts the
params, which occupy slots `0..params`, so this is one byte per param and
per local and no more), in slot order. Nothing per instruction: the operand
stack's kinds at every word
are derived by the verifier's linear walk (§2.4), exactly like the JVM's
stack-map verification. Encoding of a kind byte: `0 Dyn, 1 Num, 2 Arr,
3 ArrNum, 4 Fun, 5 Builtin`; other values are a format error.

### 2.3 Inference (compiler side, `luxel-core` `frontend`)

A flow-insensitive, whole-program fixpoint over the *compiled* word stream
(not the AST — the superinstructions and folds must see the same kinds the
device verifies, and running it on the bytecode means one implementation
serves the compiler, the CLI and the verifier).

- **Globals**: join of the kinds of every `StoreG`/`StoreGPop` value
  program-wide, plus `Dyn` if any builtin can write globals by index
  (none does today). **The declared initial value is a store too, with
  one exemption that the census showed is load-bearing** (§9): the
  compiler emits `var hues = 0` as a `Num` init and the top-level code
  then stores `array(n)` into it, and folding that `Num` into the kind
  turns 180 of 307 library patterns `Dyn` on their render path. The init
  value is therefore dropped from the join when the init function
  *definitely assigns* the global before any read: the first `StoreG g`
  in init dominates every `LoadG g` in init, and no call to a pattern
  function precedes that store (a callee could read `g`). Dominance is
  computed on the init function's CFG only; init is small. A global that
  genuinely holds two kinds over its life (`fire-blue.js`: `heat = 0`,
  later `heat = array(n)` inside `beforeRender`) stays `Dyn`, correctly.
- **Locals**: join over all stores within the function; params join over
  every `CallFn` call site's argument kinds. A function that is ever
  referenced as a value (`ConstFun`) or passed as a callback is callable
  from anywhere with anything: its params are `Dyn` and its return is
  joined with `Dyn`-safe usage (§3.5).
- **Returns**: join of every `Ret` value kind; `RetNull` contributes `Num`
  (the interpreter returns `Value::default()` = `Num(0)`).
- **Builtins**: a signature table beside `BUILTINS` gives each entry a
  return kind (`Num` for nearly all; `ArrNum` for `array(n)`; an argument
  verbatim for the twenty-three that return one of their arrays), and, for
  the array-writing builtins, the kind they store. Since #626 no builtin
  takes a callback, so no builtin call can reach pattern code and none can
  store a non-`Num` it did not receive as an argument.
- **Arrays**: `ArrNum` is a property of the array *object*, so it is
  computed per allocation site (`NewArray`, `ConstArr`, `array()` calls)
  by a points-to pass: every `Arr`-kinded slot carries the set of sites
  that can reach it; a `StoreIdx`/array-writing builtin whose array
  operand has provenance `{S…}` and whose value kind is not `Num` demotes
  every site in `{S…}` from `ArrNum` to `Arr`. A store through an array
  of unknown provenance (a `Dyn` slot, a `CallValue` result, an array read
  out of another array) demotes **all** sites — the poison rule; sound and
  cheap, and rare in the library. Nested arrays (`a[i] = array(3)`)
  therefore make `a` an `Arr`, and loads from it `Dyn`.
- **Joins**: when a conditional join (ternary, `&&`/`||`, `?:` inside a
  call argument) has a non-empty operand stack whose top differs in kind
  between edges, the compiler inserts `Box` (§2.5) on the narrower edge so
  the join sees `Dyn` on both. The verifier rejects a join with unequal
  kinds, so the compiler cannot forget.

Iterate to a fixpoint (kinds only move up the lattice; at most three
steps per slot). Cost is linear in program size per iteration.

**Two things this section missed, found while implementing it (#625):**

- **The ENGINE writes globals too, and they are stores like any other.**
  `Engine::set_var` pokes a `Value::Num` into any *exported* global (the
  `/api/vars` surface), so an exported array global cannot be proven
  `ArrNum` — 4 library patterns. `Engine::from_program*` seeds
  `frequencyData`/`accelerometer`/`analogInputs` with arrays before init
  runs, so those globals carry an implicit allocation site. Both are
  modelled in `kinds::infer`; leaving either out would let the JIT unbox a
  slot the host can overwrite.
- **The init exemption assumes init RUNS TO COMPLETION.** A runtime error
  in init (an out-of-bounds read, an array-budget refusal) aborts it, and
  the engine still renders — `var buf = array(n)` would then never have
  executed and `buf` holds `Num(0)` while the annotation says `ArrNum`. A
  consumer that unboxes on the strength of these kinds must therefore fall
  back to the interpreter when init errored (`Engine::take_error` after
  construction). Cheap, and it is the only way to keep the exemption, which
  is worth the whole `ArrNum` result.
- Refining "no call precedes that store" to "no call that can REACH a
  `LoadG g` precedes that store" is sound, cheap (a transitive read-set per
  function) and worth 22 of 307 patterns: a top-level init that builds
  several arrays routinely calls helpers between them.

### 2.4 Verification (device side, and the compiler's own self-check)

A single linear pass per function with an abstract stack of kinds:

- `ConstNum` pushes `Num`; `ConstFun` `Fun`; `ConstBuiltin` `Builtin`;
  `LoadL/LoadG` push the slot's annotated kind; `StoreL/StoreG` require
  the stack top's kind ⊑ the slot's kind (`Num` into a `Dyn` slot is fine
  — it boxes; `Dyn` into a `Num` slot is a verification failure).
- Arithmetic/bitwise/compare/`Not`/`Neg` pop anything, push `Num`.
- `LoadIdx` pops `(arr, idx)`, pushes `Num` if `arr` is `ArrNum`, else
  `Dyn`. `StoreIdx` pops `(arr, idx, val)`, pushes `val`'s kind; if `arr`
  is `ArrNum`, `val` must be `Num`. `ArrLen` pushes `Num`.
- `CallFn` pops `argc`, each ⊑ the callee's param kind, pushes the
  callee's return kind. `CallBuiltin*` pops `argc`, pushes the table's
  return kind. `CallValue` pops `argc + 1`, pushes `Dyn`.
- `Jmp*`/`CmpJf`: the abstract stack at the target must equal the stack
  on the fall-through edge (a first visit records it; later visits
  compare). Loops are fine because the compiler never carries a non-empty
  stack around a back-edge.
- `Ret` requires top ⊑ the return kind. `Box` requires a non-`Dyn` top
  and replaces it with `Dyn`.

Depth and reach checks already exist (`docs/spec/bytecode.md` "Decoder
validation"); the kind walk piggybacks on the same pass. Failure is a
`BcError::Kinds` and the device answers `{"ok":false,"code":"bc-kinds"}`
exactly like `bc-version`, so the playground recompiles and, if the blob
is still wrong, shows the message — a verifier failure is a compiler bug
and should be loud. The pass costs O(words) at load. It is compiled in
under luxel-core's `kinds` feature (ON by default, so the browser, the CLI
and the mirror all verify; the firmware depends on luxel-core with
`default-features = false` and never names it). Boards without a backend
skip the
section (kinds are advisory to the interpreter). Measured cost of that skip
on a board that carries no verifier: +160 B on `board-c6-devkit` +
`hosted-ui`, +16 B on `board-pixelblaze-v3`, −96 B on
`board-seengreat-hub75` (#625).

### 2.5 Format changes

- `FORMAT_VERSION` 5 → 6. Header `flags` bit 1 = `TYPED`. A `kinds`
  section is appended after `exports` (before the padding to `words_off`):
  `n_globals` kind bytes, then per function `1 + locals` kind
  bytes, then zero padding to a multiple of 4. A v6 blob without `TYPED`
  is legal (everything `Dyn`) and runs only in the interpreter.
- One new opcode, `Box` = `0x4F` (no operand). The interpreter treats it
  as a no-op (its `Value` is already boxed). Fusion never crosses it.
- The version bump is the existing skew loop: an old device answers
  `bc-version`, the playground recompiles from source. The device store
  keeps source, so nothing is lost — but every stored blob is stale until
  its source is recompiled by a client, same as the 4 → 5 bump (#330
  documents that wipe). Do it once, together with any other format change
  waiting in the queue.
- `Value` gets `#[repr(C, u32)]` with fixed discriminants
  (`Num = 0, Arr = 1, Fun = 2, Builtin = 3`). Layout today is
  rustc-chosen; the JIT needs the tag at offset 0 and the payload at 4,
  and the boxed-argument scratch it builds for builtin calls must *be* a
  `[Value; n]`. The tag stays a u32 (the #314 finding).
  **SHIPPED (Gitea #642)**, together with `#[repr(transparent)]` on `Fx`
  (the payload has to *be* the raw word) and `vm::ValueRaw` as the named
  byte image, `Value::raw()`/`from_raw()` as the conversions and
  `TAG_NUM…TAG_BUILTIN` as the tag constants. It is indeed what rustc
  already picked: on `board-seengreat-hub75` the whole `luxel-core` text
  came out at **117,514 B before and after**, with every symbol the same
  size and `Vm::run` byte-identical at 13,256 B — only the linker's
  function order moved. docs/spec/vm.md §1 now states the layout as a
  contract.

### 2.6 What the interpreter does with kinds

Nothing, in v1: it keeps boxing every `Value`, `Box` is a no-op, and its
throughput is unchanged. Kinds are a free future lever (typed fast arms
for `Num`-only locals) but not part of this design.

## 3. Native code (Xtensa LX7, windowed ABI)

### 3.1 Unit of compilation and entry points

At activation the whole program compiles or nothing does (§6). Each
bytecode function becomes one native function; every export the engine
calls (`beforeRender`, `render`/`render2D`/`render3D`, `renderFrame`) gets
its entry recorded in a `NativeProgram { entries, fn_table, code }`.
Pattern-to-pattern calls are direct `callx8` through a literal holding the
callee's address (the pool is emitted before the code, so every address is
known by the time the code is emitted; functions compile in index order
and forward references are patched from a fixup list).

### 3.2 Calling convention

Every native function is a normal windowed function: `entry a1, F` on
entry, `retw.n` on exit, called with `callx8`. Under the window rotation
the caller's `a10…a15` are the callee's `a2…a7`; the caller's `a8…a15` are
clobbered by any call; `a2…a7` (the callee's own view) survive across its
calls.

| register (callee view) | holds |
|---|---|
| `a2` | `ctx: *mut JitCtx` — the one pointer every helper needs |
| `a3…a7` | params 0–4 of `Num`/handle kind, one word each |
| frame | params ≥ 5, every `Dyn` param (two words: tag, payload), locals, operand-stack homes, boxed-args scratch |
| `a8…a15` | expression temporaries; dead across every call |

Return: kind `Num`/handle in `a2` (caller's `a10`); kind `Dyn` in
`a2:a3` (tag, payload). Params beyond the five register slots and all
`Dyn` params are handed over through `ctx.args[]` by the caller and
copied into the callee's frame in its prologue before anything else runs,
which keeps recursion safe (the handoff area is only live between the
call instruction and the callee's first copy).

Helpers (Rust, `extern "C"`, `#[no_mangle]` not needed — the emitter takes
`fn as usize` at compile time) use the same ABI. Fallible helpers return
`#[repr(C)] struct Ret2 { val: i32, status: i32 }` (Xtensa returns two-word
structs in `a2:a3`), and the emitted code does `bnez a11, bail`. A
`RetDyn` helper returns `(tag, payload)` and signals errors through
`ctx.status` instead. Every ABI assumption here is pinned by tests
(§7.1), not by the ISA manual.

### 3.3 Frame layout

`entry a1, F` reserves `F` bytes (multiple of 16, ≤ 32 760): 16 bytes of
window spill area at `a1+0`, then the function's slots:

```
a1 + 16                      locals not homed in registers (4 B Num/handle, 8 B Dyn)
a1 + 16 + L                  operand-stack homes, one per static depth (4 or 8 B by kind)
a1 + 16 + L + S              boxed-args scratch: [Value; max_argc] for builtin calls (8 B each)
a1 + F                       caller's frame
```

The operand stack's depth at every word is static (the verifier computed
it), so every stack slot has a fixed home offset and a fixed kind. There
is no runtime stack pointer for the operand stack.

### 3.4 Register plan (v1: static stack allocation, no allocator)

Single pass, deterministic, no liveness analysis:

- Stack slot at static depth `d` lives in `a8 + d` while `d < 6`
  (`a8…a13`), else in its frame home. `a14`/`a15` are scratch for
  three-operand sequences.
- Params 0–4 live in `a3…a7` for the whole function. Other locals live in
  the frame (`l32i`/`s32i`, 2-cycle load-use on the LX7).
- Before any `callx8`, every live stack slot below the call's arguments
  that lives in `a8…a13` is spilled to its home; after the call the slots
  still needed are reloaded on first use (lazily: the emitter tracks
  `in_reg[d]`). Argument slots are moved into `a10…a15` as the call's
  operands, so a builtin call with all operands already at the stack top
  costs only the moves.

What this buys: `LoadL x; LoadL y; Add; StoreLPop z` on register-homed
slots is `add a8, a3, a4; mov a5, a8` (two instructions, one cycle each)
against ~330 cycles for the same four ops today. The plan leaves cycles
on the table around calls (spills of slots a smarter allocator would keep
in `a3…a7`), which is where a v2 "keep the deepest live slots in
callee-preserved registers" pass would go, if measurement says so.

### 3.5 Instruction selection

`t` = temp, `s`/`r` = source/result register per the plan above.
Fixed-point semantics are `crates/luxel-core/src/fixed.rs`, cited per row.

| bytecode | kind(s) | emitted | notes |
|---|---|---|---|
| `ConstNum k` | → Num | `movi r, k` if −2048 ≤ k < 2048, else `l32r r, lit` | pool entry per distinct constant |
| `ConstFun`/`ConstBuiltin` | → handle | `movi r, idx` | |
| `LoadL/LoadG`, `StoreL/StoreG` | any | `mov` / `l32i` / `s32i`; Dyn = two words | `StoreL` keeps the value on the stack (peek); `StoreLPop` does not |
| `Add`, `Sub` | Num,Num | `add r, a, b` / `sub` | wrapping, `fixed.rs` `Add`/`Sub` |
| `Mul` | Num,Num | `mull t, a, b; mulsh r, a, b; ssai 16; src r, r, t` | exact `(a·b) >> 16` of the 64-bit product, `fixed.rs` `Mul` |
| `Div`, `Pow` | Num,Num | `callx8 fx_div` / `fx_pow` | `Div` has three exact 32-bit paths and an i64 fallback; keep it in Rust. Cannot fail |
| `Rem` | Num,Num | `beqz b, +; rems r, a, b` with `r = 0` on zero | `wrapping_rem`, sign of dividend |
| `Neg` | Num | `neg r, a` | |
| `Not` | Num | `movi r, 0; bnez a, +; movi r, 1<<16` (l32r) | truthy = `Num != 0`; on Dyn: any non-Num is truthy → `beqi tag, 0, num_path` first |
| `BitNot` | Num | `movi t, -1; xor r, a, t; srli r, r, 16; slli r, r, 16` | `!x & !0xFFFF`, `fixed.rs` `Not` |
| `BitAnd/Or/Xor` | Num,Num | `and/or/xor` | |
| `Shl`, `Shr` | Num,Num | `srai t, b, 16; ssl t; sll r, a` / `ssr t; sra r, a` | `(rhs>>16) & 31`; `ssl`/`ssr` take the low 5 bits |
| `Lt…Ne` | Num,Num | `movi r, 0; b<inv> a, b, +; l32r r, 0x10000` | result 0 or ONE; all six relations exist as branches (`blt/bge/beq/bne` + operand swap) |
| `CmpJf` | Num,Num | one conditional branch | the fused form the compiler emits for `if`/loops — no materialised boolean |
| `JmpIfFalse` | Num | `beqz` | Dyn: `beqi tag, 0, +; j taken` then `beqz payload` |
| `Jmp` | | `j` (or a short branch) | back-edges also emit the fuel check |
| arithmetic on a `Dyn` operand | Dyn | unbox inline: `beqi tag, 0, use; movi payload, 0; use:` | coercion of non-Num to 0 is two instructions, no helper |
| `LoadIdx` | ArrNum,Num | `callx8 arr_load_num` → `Ret2` | truncating index, bounds error (`vm.rs` `index_read`) |
| `LoadIdx` | Arr/Dyn,Num | `callx8 arr_load_dyn` → tag,payload | |
| `StoreIdx` | any | `callx8 arr_store` | CoW promotion of `ArrRepr::Const`, byte budget |
| `ArrLen`, `NewArray n`, `ConstArr i` | | helpers | budget errors |
| `CallBuiltin b, argc` | direct signature | args → `a10…a13; callx8 tbl[b].direct` | the numeric tier-1 set: `sin cos abs floor … hsv rgb`; no `Vm` needed for pure ones, `ctx` in `a10` for `hsv`/`rgb`/`time`/`random` |
| `CallBuiltin b, argc` | generic | box args into scratch; `a10 = ctx, a11 = &scratch, a12 = argc; callx8 tbl[b].generic` → `Ret2`/`RetDyn` | any builtin, any arity; missing args read 0 as today |
| `CallBuiltinC/CC` | | as above with the immediates as `movi`/`l32r` | |
| `CallFn f, argc` | | args → `a11…a15` + `ctx.args`; `a10 = ctx; callx8 lit(f)`; `l32i t, ctx, STATUS; bnez t, bail` | |
| `CallValue argc` | | box args; `callx8 call_value` | helper resolves `Fun` → `ctx.fn_table[idx]` (native, Dyn params) or `Builtin` → generic table |
| `Ret`/`RetNull`/`PopRetNull` | | `mov a2, r` (`a2:a3` for Dyn); `retw.n` | `RetNull` returns `Num 0` |
| `Assert m` | | `callx8 assert_fail` when the top is falsy | sets `is_assert` |
| `Box` | | `movi tag, 0` beside the payload; the slot becomes two words | tag from the static kind: `Num 0, Arr 1, Fun 2, Builtin 3` |
| `Dup`, `Dup2`, `Pop` | | register moves / nothing | `Dup` on an empty stack pushes `Num 0` — the verifier forbids it |

Fusion is transparent: a superinstruction emits the concatenation of its
parts, minus the moves the register plan makes unnecessary.

### 3.6 Fuel, depth, errors, debug

- **Fuel**: the interpreter charges 1 per instruction up to `FUEL =
  8_000_000` per host entry. Native code charges at back-edges and calls
  only: `l32i t, ctx, FUEL; addi t, t, -1; s32i t, ctx, FUEL; beqz t,
  bail` (a straight-line body costs nothing). The budget is the same
  number; a native loop iteration costs 1 unit instead of ~10, so a
  runaway pattern gets ~10× more work before `ERR_EXEC_LIMIT` — still
  bounded, and the render-task watchdog is unchanged.
- **Call depth**: prologue `l32i t, ctx, STACK_LIMIT; bltu a1, t, bail`
  (the native stack replaces `MAX_DEPTH`); the limit is set from the
  render task's stack size minus a margin.
- **Errors**: every fallible helper can fill `ctx.err: Option<VmError>`
  and return a non-zero status; the emitted code branches to a shared
  per-function `bail` epilogue that sets `ctx.status` and `retw`s. Before
  each fallible helper call the emitter stores the instruction's
  fn-relative word index into `ctx.insn_at` (2 instructions), so
  `err_at` attributes exactly as today (`vm.rs` `err_at` / `pos_at`).
  Errors unwind through native frames by the status check after every
  `CallFn`.
- **Debug**: the debugger steps the interpreter. `Engine::debug_enabled`
  is known at activation and never set on the firmware path; a program
  activated with debugging on, or a `debug_set_enabled(true)` on a live
  one, runs the interpreter (whole-program rule).

### 3.7 Literal pool, branch reach, buffer layout

One code buffer per program: `[literal pool][fn 0][fn 1]…`, 4-byte
aligned. `l32r` reaches ~256 KB backwards only and there is no LITBASE on
these cores, so the pool goes first and the cap on program size (§5) keeps
every function in reach. Conditional branches reach ±128 B; the emitter
always emits forward conditionals as `b<inverted> +6; j target` (6 bytes,
one fixup) and backward ones directly when in range. No branch islands,
no second pass over the code. Zero-overhead `loop` is not used in v1.

### 3.8 `JitCtx`

**SHIPPED (Gitea #642): `crates/luxel-core/src/jit/ctx.rs`.** `#[repr(C)]`,
with a `pub const OFFSET_*` per field derived by `core::mem::offset_of!` —
**those constants, and `SIZEOF_JITCTX`, are the ONLY thing the emitter reads
about this struct.** The 32-bit (device) layout is additionally pinned by
literal `const` assertions, so a reorder is a build failure rather than a
silent ABI break:

```
                     32-bit offset
vm: *mut Vm                     0
prog: *const Program            4    (added — see below)
status: i32                     8
insn_at: u32                   12
fn_idx: u16 + _pad: u16        16
fuel: i32                      20
stack_limit: usize             24
args: [i32; 34]                28
err: *mut Option<VmError>     164    (out of line — see below)
fn_table: *const usize        168
builtins: *const BuiltinEntry  172     sizeof = 176
```

Two fields deviate from the sketch above, both deliberately:

- **`err` is out of line.** `VmError` owns a `String`, so it is neither
  `repr(C)` nor a fixed size, and inlining it would make every offset after
  it rustc's choice. It is a `*mut Option<VmError>` pointing at a slot the
  CALLER owns; helpers fill it and generated code never touches it — it only
  tests `status` (`STATUS_OK = 0`, `STATUS_ERR = 1`).
- **`prog: *const Program` was added.** The generic builtin wrappers run the
  interpreter's own arms, and those take `&Program` (the constant pool an
  `ArrRepr::Const` array reads through, the assert messages). There is
  nowhere else to get it: `Vm` holds no `Program`, because a program can be
  a borrowed `'static` flash slot (`Words::Static`).

`Vm` itself is untouched by generated code; every access to arrays,
globals-by-builtin, the pixel brush, the frame buffer and the RNG goes
through helpers that are thin `extern "C"` wrappers over today's `Vm`
methods. Globals are the exception: a `LoadG`/`StoreG` on a typed global
is `l32i`/`s32i` into a `#[repr(C)]` globals array the helper side shares
(`Vm::globals` becomes that array; the interpreter reads it as before).

### 3.9 §3 as built (phase 2, Gitea #651)

**SHIPPED: `crates/luxel-jit`** — `xtensa.rs` (the encoder), `plan.rs` (the
frame and register plan), `emit.rs` (the selection table). `no_std` +
alloc, not linked into the firmware or the wasm playground yet.
`compile(prog, kinds, env) -> Result<NativeImage, Refusal>` is a pure
function: it executes nothing, allocates nothing executable and touches no
device, which is what lets `tests/library_diff.rs` run its output through
an Xtensa ISA model on x86 and compare against the interpreter.

Everything above is as designed except the following, all deliberate and
all found by building it.

**Two ISA facts the design had wrong.** Both would have been silent
miscompiles on metal:

- **`retw` restores only the low 30 bits of the return address**
  (`PC ← PC[31:30] || a0[29:0]`), so **every `callx8` target must share
  bits 31..30 with the code calling it**. A helper a gigabyte away returns
  into the wrong gigabyte — a wild jump, not a wrong value. `compile`
  checks every `Helpers` address against `Env::code_base` and refuses
  (`address-region`). On the S3 the IBUS window, the flash mapping and
  IRAM are all in `0x4…`, so this only fires on a wiring mistake — but the
  test harness hit it on its first run.
- **`l32r`'s 16-bit field is ONE-extended, not sign-extended**: the target
  is `((pc+3) & !3) + ((0xFFFF0000 | imm16) << 2)`, always a negative word
  offset. The reach is therefore the full 256 KB backwards, not 128 KB.

**§3.2, calling convention.** The register/`ctx.args` split is per
FUNCTION, not per parameter: `ParamConv::Regs` (at most five parameters,
none `Dyn`, in `a3…a7`) or `ParamConv::CtxArgs` (every parameter through
the handoff area, one word for a non-`Dyn` one and two for a `Dyn` one).
Mixing the two per parameter would make the register assignment depend on
which parameters happen to be `Dyn`; one bit per function lets caller and
callee agree by looking at the same thing. `NativeImage::abi` reports it.
The common case — all-`Num`, few parameters — still lands in registers.

**§3.3, frame — and §3.3 WAS WRONG about the window save area.** The
sketch reserves "16 bytes of window spill area at `a1+0`". Both halves of
that are wrong, and it was a device crash (Gitea #658): the Xtensa windowed
ABI puts the save areas just below the CALLER's stack pointer, and `entry
a1, N` sets `a1 = caller_sp - N`, so they sit at the **TOP** of the
callee's frame — and a `call8` chain needs **32** bytes there, not 16:

```
   a1 + F        caller's sp
   a1 + F - 16   base save area:  caller's a0...a3
   a1 + F - 32   call8 extra:     caller's a4...a7
   ...           locals, operand-stack homes, boxed-args scratch
   a1 + 0
```

`xtensa-lx-rt`'s `_WindowOverflow8` is the authority: `s32e aX, a9, -16...-4`
for `a0...a3` and `s32e aX, a0, -32...-20` for `a4...a7`, both relative to
the saved sp. With the layout as designed, any generated function deep
enough to take a window-overflow exception had the top 32 bytes of its own
data overwritten by the handler, and the underflow handler then reloaded a
corrupted `a1`. §7.1's ISA model has a flat 64-register file and never
spills, so every host gate passed it for two phases; §7.2 caught it on the
first patterns whose builtin calls nest deep enough (`aurora-2d`,
`bulk-canvas-ripples-2d`). The model now TRAPS a generated store into any
live frame's save area, with a self-test that the old layout would have
tripped.

32 and not 48 (a `call12` caller's requirement): nothing calls generated
code with one — the emitter emits `callx8` only, and the engine enters
through a Rust `extern "C"` pointer, which is `call8` on these targets.

Every frame home is a uniform 8 bytes laid out as a
`Value` (tag at +0, payload at +4) rather than 4 or 8 by kind. One stride
means one offset formula for locals, operand-stack homes and boxed
arguments alike, and a `Dyn` home is then byte-identical to the `Value` a
builtin wrapper wants. A function's median frame is a few dozen bytes
either way.

**§3.4, register plan.** `a8`/`a9` hold operand-stack depths 0 and 1;
`a10…a15` are scratch. The design's six register-homed depths with two
scratch registers does not work: the `value_eq` sequence alone wants three
scratch live at once, a `Dyn` operand costs one more each, and a wide
frame offset borrows another — a four-scratch first cut still ran out on
real library patterns. The census (§9b) puts the median function's peak
depth at 2, so two register homes cover the shape that matters and
everything deeper was going to spill under any v1 plan.

Register homing is **intra-basic-block only**: every register-homed depth
is spilled to its frame home before any branch, at every branch target and
before every `callx8`. That removes the cross-edge agreement problem
entirely — no two edges into a block can disagree about what is where —
at the cost of reloading across a branch. A `Dyn` depth is never
register-homed; one register cannot hold a tag and a payload.

**§3.5, selection.** Four rows were wrong or incomplete:

- **`Shl`/`Shr`: `srai t, b, 16` is a FLOOR and the semantic is TRUNCATION
  TOWARD ZERO.** `Fx`'s shift count is `rhs.to_int_trunc() & 31` and
  `to_int_trunc` is `wrapping_div(65536)`, so a right-hand side in (−1, 0)
  — `x << -0.5`, which the oracle pins as a shift by zero — would have
  shifted by 31. Emitted as `l32r k, 65536; quos k, b, k; ssl/ssr k;
  sll/sra`: one `quos` is the exact truncating divide and its divisor is a
  constant, so it can never trap.
- **`==`/`!=` are REFERENCE IDENTITY, so the "unbox a `Dyn` operand to 0"
  rule is wrong for them.** `vm::value_eq` is false across kinds, so
  `arr == 0` is false — unboxing the array to 0 would make it true.
  Emitted as a tag compare then a payload compare; two operands whose
  static kinds have different tags fold to a constant.
- **Truthiness on a `Dyn` operand is `tag ≠ 0 OR payload ≠ 0`**, not just
  a payload test: a reference is always truthy (`Value::truthy`). That
  binds `Not`, `JmpIfFalse`, both peeking jumps and `Assert`.
- **`BitNot`'s low-16 clear needs two `srli`s.** `srli`'s immediate is
  four bits, so a shift of 16 does not exist: `xor r, a, -1; srli r, r,
  15; srli r, r, 1; slli r, r, 16`.

And two rows are narrower than designed:

- **The `direct` tier-1 path is used only when the call's arity matches
  the signature EXACTLY.** §4 wanted the emitter to materialise a
  builtin's defaults (the 0.5 duty of a one-argument `square`); it carries
  no table of defaults, and a mismatched arity falls back to `generic`,
  which is the interpreter's own marshalling and cannot be wrong.
- **`CallValue` uses a RESOLVE-ONLY helper.** §3.5 has the helper resolve
  the callee and call it, which is a Rust → native trampoline that §4 says
  this design does not have and which cannot be modelled on a host.
  `call_value_target` returns a native address, a builtin id or an error,
  and the three-way dispatch is native code.

**§3.6.** The site store is BOTH `insn_at` and `fn_idx`, not just
`insn_at`: a returning `CallFn` has left `ctx.fn_idx` set to the callee's,
so an error after a call would otherwise be attributed to the wrong
function. Fuel is charged at every backward-branch TARGET and before every
`CallFn`/`CallValue`.

**§3.7.** The pool goes first in the image as designed, but its size is
not known until the last function is emitted — so code is emitted into its
own buffer at code-local offsets and the pool is PREPENDED. Branches and
`j` are pc-relative and survive a uniform shift untouched, and every
`l32r` was going to be a fixup anyway.

**§3.8.** `JitCtx` gained `globals: *mut ValueRaw` (appended, so no offset
moved). §3.8 says a typed `LoadG`/`StoreG` is an `l32i`/`s32i` into a
`repr(C)` globals array, and since #642 `Vm::globals` already IS one; what
was missing was a way for generated code to FIND it, because `Vm` is not
`repr(C)` and cannot be offset into. Beside it, `jit::ctx::dev32` spells
the 32-bit device layout as literal numbers: the `OFFSET_*` constants are
`offset_of!` on the host, where `OFFSET_PROG` is 8 and not 4, and the
emitter must encode device offsets whatever machine it runs on.

**Refusals** (§4a's vocabulary, all of them whole-program and none a
panic): `unsupported`, `too-large`, `l32r-reach`, `frame-size`, `kinds`,
`param-overflow`, `offset-reach`, `scratch`, `address-region`,
`jump-reach`, `untyped`. Over `library/` **none of them occurs**: 307 of
307 patterns compile.

**Not done in phase 2**, and deliberately: no `loop`, no branch islands,
no liveness analysis, and `Div`/`Pow` are helper calls per §3.5 even
though `Div` is on the per-pixel path. A real register allocator is the
phase-4 lever if measurement asks for one.

## 4. Builtin table

**SHIPPED (Gitea #642): `crates/luxel-core/src/jit/table.rs`.**

`BUILTINS` stays the append-only name table. Beside it, under the `jit`
feature, `BUILTIN_ENTRIES: [BuiltinEntry; 188]` with one entry per id, in
the same order (a `const` assertion holds the two lengths equal, so a
builtin appended without a table line fails the build):

```rust
#[repr(C)]
pub struct BuiltinEntry {
    pub generic: unsafe extern "C" fn(*mut JitCtx, *const Value, u32) -> RetDyn,
    pub direct: Direct,    // one word: 0, or a numeric-signature fn (tier-1 set)
    pub direct_sig: u8,    // DirectSig: None, N1..N4, C0..C3
    pub ret_kind: u8,      // RET_NUM / RET_NEW_ARRNUM / RET_DYN / RET_ARG_BASE|n
}
```

- `RetDyn` is `#[repr(C)] { tag: u32, payload: u32 }` — a `Value` returned
  by value in two words (`a2:a3`, §3.2, pinned by objdump in §7.1). **A
  failing call never reports through the return value**: it sets
  `ctx.status` non-zero and fills `*ctx.err`, which is what lets all 188
  wrappers have one shape, tombstones included.
- `direct` is spelled as a one-word `#[repr(C)] union Direct` rather than
  `usize`, because a function pointer cannot be cast to an integer during
  const evaluation and so a `usize` field could not be initialised in a
  `static`. Same word, same meaning (`none == 0` ⇔ no direct form);
  `BuiltinEntry::direct_addr()` hands the emitter the address.
- `ret_kind` is **derived from `vm::builtin_sig` at compile time** —
  `builtin_sig` became a `const fn` for exactly that, so the §2.3 signature
  table stays the single source of truth and `kinds`/`jitlint` read it
  unchanged. (Its arms are `str_eq` chains rather than a `match` on `&str`:
  string patterns are not const-evaluable on rustc 1.96.)

The `generic` wrappers call exactly the arms `builtin_fast` / `builtin_hot`
/ `builtin_cold` run today — literally: `Vm::builtin_ladder` was split out
of `Vm::call_builtin` (`#[inline(always)]`, so the interpreter's code is
unchanged) and the wrappers enter it with the interpreter's own marshalling
(`[Value; MAX_ARGS]`, missing arguments read `Num(0)`, extras dropped,
`argc` capped at `MAX_ARGS`). Each wrapper is a 23-byte thunk on Xtensa that
tail-calls one shared out-of-line body with the id in a register, so the
table costs I-cache like one function, not like 188. **No wrapper ever calls
back into pattern code**, so there is no Rust → native trampoline in this
design at all.

**The tier-1 `direct` set** (§3.5), keyed on the `Builtin` rather than the
name so the aliases come along: `abs floor ceil round trunc frac sqrt sin
cos wave triangle` (N1), `min max mod square` (N2), `clamp mix` (N3),
`random prng time` (C1), `hsv rgb` (C3) — plus `fract` (= `frac`), `lerp`
(= `mix`) and `hsv24` (= `hsv`). Everything else is `direct = 0` and goes
through `generic`. They are raw 16.16 words in registers with no boxing:
`d_abs` is `entry / abs a2, a2 / retw.n` and `d_clamp` is `entry / max /
min / retw.n` on the S3. Two contracts worth naming: a direct fn takes the
**effective** arguments, so a one-argument `square(t)` call site must
materialise the 0.5 duty itself (the emitter knows `argc` statically); and
the five ctx-taking ones reach the VM by CALLING `Vm::builtin_fast` with a
constant `Builtin`, which folds to the one arm — there is no second
implementation of any builtin anywhere in the JIT.

**Interpreter-through-table: BUILT, OFF, and DEFERRED to hardware.** A
`dispatch-table` cargo feature (luxel-core, and `EXTRA_FEATURES=dispatch-table`
on the firmware) replaces `Vm::call_builtin`'s tier ladder with one indirect
call through `BUILTIN_ENTRIES[id].generic`. Whether that beats the tiers is
a measurement, not an opinion — #328 showed the tiers are an I-cache budget
and #312 test 2b showed how badly host numbers mislead here — and no Luxel
hardware was reachable when #642 landed. So it ships **off on every board**,
with the whole luxel-core suite (the library render gate included) passing
with it ON, and the size cost measured: **+8,288 B of app image on
`board-seengreat-hub75`** (188 × 23 B of thunks, the shared body, the direct
fns, and 2,256 B of table in `.rodata`), 0.26 % of that board's slot. The
Seengreat A/B is the open item.

**DONE (Gitea #626).** The six higher-order builtins (`arrayForEach`,
`arrayMutate`, `arrayMapTo`, `arrayReduce`, `arraySortBy`, `mapPixels`)
were the only builtins that re-entered the VM. They are now written in the
pattern language, in `crates/luxel-core/src/prelude.js`, linked into a
program only when it uses one. `Vm::dispatch_direct` and the six arms are
deleted and the ids are `BKind::Removed` tombstones; v1 compiles
`CallValue` natively (native → native through the entry table, `Dyn` params
and return), so nothing is left for the JIT to refuse. Two rules make the
prelude hold, and both are implemented:

- **Always, never per target.** One blob runs everywhere (store, sync
  pull, playlist), so the interpreter runs the prelude loop too. Nine of
  the ten library call sites are init-time fills, where the cost is
  invisible. The one per-frame use (`arrayMapTo` twice over 256 cells in
  `bulk-canvas-ripples-2d`) costs **+10.75 µs/frame on the host** (22.00 →
  32.75 µs, `luxel bench --map-grid 64x64`, median of 9); the S3 ratio
  will differ and is not measured yet.
- **Specialise on a static callback.** Inside a prelude function the
  callback is a parameter, so its call is `CallValue` and its params would
  be `Dyn` (§2.3). When the call-site argument is a literal lambda or an
  identifier naming a function — every library and corpus call site — the
  compiler clones the helper for that site, drops the callback parameter
  and binds the call into a direct `CallFn`, so the callback keeps typed
  params. A callback that is only a run-time value goes through the shared
  copy, boxed and correct.

`arraySortBy` is an insertion sort in the prelude. `mapPixels` needed one
new builtin, `pixelCoord(i, axis)` (id 187): the mapped coordinate of one
pixel with the transform applied, returning `Num`.

Census effect over `library/` (§9a's harness, against the pre-#626 tree):
patterns with a fully typed render path stay 286 / 307 and the `Box` sites
are unchanged, but the v1 exclusion set drops from 19 to 12 — every
remaining one is a genuine `CallValue`, none is a callback builtin — so
**fully typed AND v1-eligible goes 281 → 286**.

### 4a. Refusal semantics and the editor warning

A refusal is **whole-program**: the pattern runs in the interpreter
exactly as today, at the interpreter's speed, with the same pixels — no
function-level mixing, ever (decision 2). `/api/status` carries
`jit: {state: "interp", reason}` with `reason` one of `too-large`,
`psram`, `kinds` (verifier failure — a compiler bug, reported loudly),
`debug` (debugger attached), `unsupported` (anything else, with the
opcode).

**SHIPPED (Gitea #627):** `luxel_core::jitlint::jit_eligibility(prog, kinds)`
is the one place a compile-time refusal is decided, and
`jitlint::dyn_lints` names and anchors every `Dyn` slot; the browser reads
both through the wasm export `lx_kinds` and renders them as described in
docs/web-architecture.md ("Lints: boxed variables and interpreter mode").
Later phases add their reasons (`TooLarge`, …) to `JitRefusal` and every
surface follows.

**The `callbacks` reason no longer exists** (#626): v1 compiles
`CallValue`, and no builtin reaches pattern code, so no construct in
`library/` forces interpreter mode. The refusal list is empty and the
editor has nothing to warn about at compile time; the remaining reasons
are all device-side and surface from `/api/status` next to the frame rate
after activation. Should a future construct become compile-time
refusable, the warning belongs in the same channel as the `Dyn`-variable
lint (§11 answer 3) — that was Jeremy's instruction on 2026-09-20 and it
still stands.

## 5. Executable memory and lifecycle on the S3

- **Where**: PSRAM. The Seengreat's 8 MB is already mapped by esp-hal into
  the DBUS window at `0x3C00_0000` through the S3's shared I/D MMU table
  (`firmware/src/psram.rs`, `docs/research/flash-mmap.md`); the same
  pages are fetchable at `+0x0600_0000` in the IBUS window — the mirror
  Espressif's own `elf_loader` uses on the S3. The `psram-arena` heap
  gains an `alloc_exec(len)` alongside `arena::install`; the JIT writes
  through the DBUS pointer (byte-writable), then
  `esp_hal::soc::esp32s3::cache_writeback_addr(dbus, len)` (exposed in
  esp-hal 1.1, `#[doc(hidden)]`) and `Cache_Invalidate_Addr(ibus, len)`
  (already declared in `flashmap.rs`), and calls at `dbus + 0x0600_0000`.
  No MMU programming, so no `flashmap::quiesced` fence and no other-core
  park. The S3's caches are shared by both cores, so compiling on the
  render task (core 1) needs no cross-core invalidation.
- **Internal-SRAM fallback** (s3-devkit, no PSRAM): the whole internal
  SRAM is dual-mapped (`iram_seg` `0x4037_8000` ≡ `dram_seg`
  `0x3FC8_8000`, offset `0x6F_0000`); a program small enough for a
  `JIT_INTERNAL_MAX` (default 8 KB) is placed in a heap allocation and
  executed at `+0x6F_0000` with no cache step at all (no L1 on these
  cores). The main heap sits at ~41 KB free on the panel today, so this
  is a fallback, not the plan.
- **Cost of PSRAM code**: instruction fetch through the 32 KB ICache,
  like flash today; a compiled `render` is a few hundred bytes to a few
  KB and stays resident. Measured, not assumed: the first on-metal
  microbench runs the same native function from PSRAM and from internal
  SRAM (§7.3).
- **Lifecycle**: compile inside `try_budgeted_engine` (the choke point
  every activation funnels through — boot default, `/api/code`, store
  activate, library swap, crossfade). Crossfade keeps two engines and so
  two code buffers; `drop_prev` frees the outgoing one. Program cap:
  `JIT_MAX_CODE = 128 KB` (well inside `l32r` reach); over it, or PSRAM
  exhausted, or any function refusing to compile → interpreter, with the
  reason in `/api/status`:
  `jit: { state: "native" | "interp", reason, code_bytes, compile_us }`.
- **Compile time**: single pass over ≤ 65 536 words per function with no
  allocation beyond the output buffer and the fixup list; expected low
  milliseconds on a 240 MHz core, and it happens on the render task,
  which is already blocked for decode + engine construction during a
  swap. `compile_us` reports it; if it ever matters, persisting native
  blobs in the store is the lever (`patlog` has a `ver` byte and two
  reserved bytes), but it needs a per-build id the firmware does not have
  today.

## 6. Engine integration

- `Program` gains `native: Option<NativeProgram>` (`jit` feature).
- `Engine::render_pixels` checks it: with native code the per-pixel loop
  becomes `native.render(ctx, i_fx, x, y, z)` with the four `Fx` words in
  `a11…a14` (the same values `render_args` builds today, minus the
  `Value` boxing), the brush read back from `Vm::pixel` as now. That
  replaces the ~400-cycle entry (`begin_pixel_pass`/`render_pixel`: frame
  push, locals reserve, fuel reset, `run_unwinding`) with a `callx8` plus
  the callee's `entry` — on the order of 20 cycles.
- `beforeRender(delta)` and `renderFrame()` call their entries the same
  way; the frame-buffer lend (`frame_buffer_out/in`) is unchanged because
  bulk ops are builtins and read `Vm::frame` through helpers.
- `Value` gets its `repr`. `Vm::globals` becomes a `#[repr(C)]` word
  array. Everything else in `Vm` is untouched — there is no mode
  indirection to build, because no builtin calls pattern code (§4).
- The interpreter path is byte-for-byte the same code as today when
  `native` is `None`; on boards without the `jit` feature the field does
  not exist.

### 5 and 6 as built (phase 3, Gitea #658)

**SHIPPED**: `luxel_core::jit::native` (the engine's half),
`Engine::install_native` + the two call sites, `firmware/src/jit.rs` (the
exec buffer, the call, compile-at-activation), `/api/status`'s `jit`
object, `POST /api/jit`, `tools/qemu/jit-test.py`.

**§7.2's gate earned its keep immediately.** It found a trap §7.1's
could not: two patterns (`aurora-2d`, `bulk-canvas-ripples-2d`) compiled,
started, and then took the AppCpu down with a clobbered stack guard and
EXCCAUSE 0. The cause was §3.3's window save area — see the frame note
above. It is fixed, both patterns now run natively to completion, and the
ISA model traps that whole class from now on.

**Still BUILT INTO THE S3 IMAGES AND OFF AT RUNTIME.** Not because
anything is known to be wrong, but because §7.3 has not run: no S3 has
executed a byte of this. `LUXEL_JIT_ENABLED` defaults to false so turning
it on stays a deliberate act, with someone watching the panel.
*(Superseded 2026-09-24 by phase 4 — §7.3 ran and the default is now
`true`. See "5 and 6 as built (phases 4 and 5)" below.)*

Everything above is as designed except the following.

**`native` lives on the `Engine`, not on the `Program`.** §6's first bullet
says `Program` gains it. `Program` is `Clone` and a claim on executable
memory is not; making it clonable would mean either refcounting the exec
buffer or a `Program` whose clone silently loses its code. The engine is
also the thing whose LIFETIME the code has to match — activation builds it,
`drop_prev` frees it — so it is where the field belongs.

**The call is behind a trait.** §6 describes `Engine::render_pixels`
calling the native entry directly. It calls
`NativeCall::enter(addr, ctx, abi, args)` instead, and that indirection is
the phase's most useful decision: the device installs `XtensaCall`, which
transmutes the address to a typed `extern "C"` pointer, and the host test
suite installs a caller that runs the same image through the phase-2 ISA
model. `Engine`'s own path is then literally the same code under both,
which is what makes `crates/luxel-jit/tests/engine_diff.rs` — 307 of 307
library patterns, four frames each, bit-identical — a test of the GLUE and
not of a second implementation of it. A `renderFrame` pattern is only
reachable this way at all: the frame builtins need the engine's lent
buffer, which `library_diff.rs`'s by-hand harness does not have.

**No inline assembly on the call path.** §3.2's windowed convention is
exactly what Rust's `extern "C"` emits a `callx8` for on these targets, so
a typed fn-pointer call is enough for every `FnAbi` shape. The engine reads
no result, so the pointer is declared `-> i32` even for `ret_dyn` — a
two-word return arrives in `a10:a11` with no hidden `sret`. The one `asm!`
in the tree is a bare `isync` after writing the exec buffer.

**`FnAbi` gained `dyn_params`** and `luxel_core::jit::ctx_arg_words` is the
`ParamConv::CtxArgs` layout, written once and used by both callers: a
caller and a callee that disagree about which handoff word is a tag is a
wild read of somebody's frame, so there is exactly one place to get it
wrong.

**§5's PSRAM arena is NOT built.** Phase 3 ships the `.rwtext` static and
only that. On the S3 that is a real limitation rather than a staging step:
`.rwtext` and `.stack` are one budget, and after the 24 KB stack floor
there is **1.7 KB** left — enough for `rainbow` and nothing else. The
buffer is instead bought by trading `iram-vm` away on a JIT board (it holds
`Vm::run`, the interpreter loop a native pattern never enters), which gets
the Seengreat to 14 KB / 7 KB per image / 91 % of `library/`. That is a
working JIT, but §5's PSRAM route — 8 MB already mapped, costing no
internal SRAM — is what actually lifts the cap, and the measurement is the
argument for building it. docs/firmware.md "JIT" carries the table.
*(Built in phase 4 — below.)*

**The refusal vocabulary grew three device-only reasons** — `init-error`
(§2.3's exemption needs init to have completed), `no-buffer` (both exec
halves in flight) and `disabled` (the `POST /api/jit` switch) — beside
§4a's `debug`. All four are things no compile-time lint could predict,
which is why §4a routes them through `/api/status`. *(Phase 4 added a
fourth, `no-memory`.)*

### 5 and 6 as built (phases 4 and 5, Gitea #665 / #666)

**SHIPPED**: `psram::alloc_exec` + the arena `Lease` and its cache steps,
the internal-SRAM fallback, `compile_into` (the emitter writes into the
caller's buffer), the `no-memory` gate, `jit.place` / `POST /api/jit
{"place":…}`, the classic-ESP32 tier, `tools/jit-diff.mjs`,
`crates/luxel-jit/tests/alloc_peak.rs`. §7.3 ran on 2026-09-24 on two
boards; the numbers live in docs/boards.md "JIT on metal".

**§5's PSRAM arena is built, and built as designed.** `psram::alloc_exec`
takes 64 B-aligned blocks out of the same second `EspHeap` that already
holds pattern arrays — arena only, never the main heap, because the caller
executes through the PSRAM window's mirror and a main-heap block needs a
different alias entirely. The publish step is exactly §5's: the image is
written through the DBUS pointer, `rom_Cache_WriteBack_Addr` pushes the
data cache out (with `Cache_Suspend_DCache_Autoload` around it, as esp-hal
does), `Cache_Invalidate_Addr` drops the instruction side's copy of the
same lines, `isync` last; execution is at `+0x0600_0000`. One deviation in
the spelling: the write-back is the ROM symbol out of `esp32s3.rom.ld`, not
esp-hal's `#[doc(hidden)]` `cache_writeback_addr` wrapper this section
named. A block that lands outside the window we know how to mirror is
freed and refused rather than executed through a guessed alias. No MMU
programming, so no `flashmap::quiesced` fence and no other-core park, as
designed.

**The internal-SRAM fallback is built too**, at §5's `JIT_INTERNAL_MAX`
= 8 KB, through `dram_seg` `0x3FC8_8000` ≡ `iram_seg` `0x4037_8000`
(`+0x006F_0000`), with `isync` as the whole coherency step. It is charged
against `RUNTIME_FLOOR`, since the block stays for the pattern's life. It
is also the §7.3 microbench's lever, which is why it is reachable on
purpose from `POST /api/jit {"place":"internal"}` and not only by accident
on a PSRAM-less board.

**There is no `JIT_MAX_CODE`-sized internal buffer any more.** §5 assumed
the emitter produced an owned image that the firmware then copied into exec
memory. `luxel_jit::compile_into(prog, kinds, env, out: &mut [u8])` emits
straight into the caller's slice instead (`xtensa::Asm` gained a
slice backing beside its owned `Vec`; `compile()` is now a host wrapper,
and `NativeImage::words` became `bytes` with a word view for the ISA
model). On the S3 that slice IS the exec block, so the internal heap never
holds a copy of the image. The classic ESP32 is the exception and has to
be: SRAM0 faults on a sub-word access, so there the image is staged in a
fallibly-reserved heap `Vec` and copied over with 32-bit stores. The block
is still allocated at the full 128 KB cap, so `psram_free` drops by that
much per native image whatever the code weighs — shrinking it to the
emitted length is a follow-up.

**§5 counted the wrong allocation.** "No allocation beyond the output
buffer and the fixup list" was wrong about the one that mattered:
`luxel_core::kinds::StackMap` was `Vec<Option<Vec<Kind>>>`, a `Vec` header
and an allocator block per bytecode word, and beside a resident 4096-px
engine it took the panel down on the first native run of `snake-2d` — a
`Vec<Kind>` clone inside `plan_all` with 48 bytes of heap left. Two fixes:
the map went flat (~9 B/word instead of ~55), and the compile now asks
before it starts. `crates/luxel-jit/tests/alloc_peak.rs` fits a rule to the
emitter's bookkeeping peak over all 307 library patterns under a counting
allocator — `words × 24 + fns × 240 + 1024` host bytes — and
`jit::emit_heap_need` applies the same rule on the device against
`HEAP.free()`, refusing `no-memory` rather than running out half way. The
floor left under it is `COMPILE_FLOOR`, 12 KB, deliberately smaller than
`RUNTIME_FLOOR`'s 20 KB: the bookkeeping lives for one compile on the
render task, not for the pattern's life. docs/firmware.md "The compile's
own heap" is the full argument.

**`iram-vm` came back on the S3.** Phase 3's trade is off: arena code costs
no internal SRAM, so `board-target.sh` and flake.nix put the interpreter's
per-pixel loop back on the JIT boards. `.stack` 26,268 B, against 25,484 B
for phase 3's static-without-`iram-vm` build.

**Phase 5 shipped the classic-ESP32 tier** — `#666`, a 24 KB `.rwtext`
static in SRAM0 split into two halves, verified on the Athom at 1.9–5.2×.
§8's "the classic ESP32 and RISC-V boards do not carry the feature" is
retired for the Xtensa half: `board-target.sh` sets
`JIT="${CLASSIC_JIT:-0}"` on those three boards, so the tier is one env var
away and not a rebuild of anything. It does not ship yet, and the reason is
slot arithmetic rather than doubt: the image with the emitter is ~72 KB over
the 1,048,576 B pre-#501 slot that the migrating release still weighs every
image against (Gitea #635, docs/boards.md).

## 7. Verification plan

### 7.1 Host (no device, every CI run)

- **Encoder**: each instruction form is emitted into a buffer and
  disassembled with the devshell's own objdump; the test compares
  mnemonics and operands. This is the whole "did I get the bit layout
  right" question, answered by the vendor toolchain.

  **SHIPPED (Gitea #651): `crates/luxel-jit/tests/objdump.rs`, 150 forms.**
  **Correction: the oracle is `xtensa-esp32s3-elf-objdump`, not
  `xtensa-esp-elf-objdump`.** The latter is built for a GENERIC Xtensa
  configuration and desynchronises on our byte stream — it reads `entry
  a1, 32` as `excw`, invents FLIX bundles and drifts by a byte, because
  the instruction lengths of an unknown configuration are not ours. The S3
  variant, from the same derivation, is exact. It caught four encodings
  that reasoning from the field layout had wrong: `movi`'s 12-bit split,
  `slli`'s `op2` (bit 4 of `32 - sa`, so shifts of 17..31 were encoding
  1..15), `beqi`/`bnei`'s `op0` (6, not 7 — the wrong one decodes as
  `bnone`/`bbsi`, a wild branch), and the 24-bit `nop`'s `r` field (2, not
  0 — with 0 the same word is `callx12 a0`).
- **ABI pins**: a Rust `extern "C"` helper returning `Ret2` is compiled
  for `xtensa-esp32s3-none-elf` and its disassembly asserted to return in
  `a2:a3`; `JitCtx`/`Value`/`BuiltinEntry` offsets are `const`-asserted
  against the emitter's constants.

  **The `a2:a3` half is ANSWERED (Gitea #642), by reading, not yet by an
  automated assertion.** `luxel_core::jit::lx_abi_probe_ret2` is a
  `#[no_mangle]` `extern "C" fn(i32, i32) -> Ret2` kept alive by a `#[used]`
  static (`#[no_mangle]` alone does not survive `--gc-sections`). In the
  `board-seengreat-hub75` image it disassembles to

  ```
  entry a1, 32 / add.n a8, a3, a2 / xor a3, a3, a2 / mov.n a2, a8 / retw.n
  ```

  — both words in `a2:a3`, no `sret` pointer. The real `generic` wrappers
  agree: each is `entry / <shuffle> / callx8 generic_call / mov.n a2, a10 /
  mov.n a3, a11 / retw.n`, i.e. the callee's `a2:a3` arriving as the
  caller's `a10:a11`. **Phase 2 owes the checked-in version** of this —
  disassembling the probe as part of the encoder test suite rather than by
  hand — and the same treatment for `Ret2`'s `status` half once a fallible
  helper exists. The `Value`/`RetDyn`/`JitCtx`/`BuiltinEntry` layouts are
  already `const`-asserted and host-tested (`src/jit/tests.rs`,
  `tests/jitabi.rs`, `tests/abi_probe.rs`).
- **Builtin table parity** (shipped, #642): every one of the 188 ids is
  called through `BUILTIN_ENTRIES[id].generic` at every arity from 0 to
  `MAX_ARGS` with mixed-kind arguments and compared against the
  interpreter's own `CallBuiltin` path — return value, error message, error
  site, and every piece of VM state a builtin can touch (the brush, the plot
  coordinate, the arena charge, the globals). Nothing is skipped: the
  stateful builtins are deterministic functions of VM state and both sides
  start from the same freshly seeded VM. Every `direct` entry is swept
  against its own `generic` over the `Fx` extremes, ±1, ±0.5 and zero.
- **Inference + verifier**: unit tests per rule; the library-wide census
  (§9) is a test that pins the count of typed render paths so a compiler
  regression is caught.
- **Emitter as pure function**: `compile(&Program) -> Result<Vec<u32>,
  Refusal>` runs on x86; a golden test pins the bytes for a handful of
  patterns so an accidental codegen change is a visible diff.

  **SHIPPED (Gitea #651): `crates/luxel-jit/tests/golden.rs`** — size,
  pool length, function count and an FNV-1a digest for five patterns
  (`rainbow`, `snake`, `snake-2d`, `perlin-fire-wind-tunnel`, the
  `renderFrame` pattern `bulk-canvas-ripples-2d`), plus the library-wide
  size total phase 4's code cache is sized against.
- **The ISA gate — and it is the one that matters**
  (**SHIPPED, Gitea #651**: `crates/luxel-jit/tests/isa/` +
  `tests/library_diff.rs`). §7.1 as written had nothing that EXECUTED the
  generated code on a host, which left "does this code compute what the
  interpreter computes" to QEMU (§7.2) and metal (§7.3). It does not have
  to: a ~900-line Xtensa interpreter covering exactly the forms the
  encoder emits — the windowed ABI included, as a flat 64-register file
  with a window base — runs the real image, and a `callx8` to a helper or
  a builtin wrapper traps out to the harness, which marshals the register
  and memory state into a REAL Rust call and the result back. A real `Vm`
  sits behind the context, `Vm::globals` is mirrored into model memory and
  synchronised around every such call, and the frame's boxed-argument
  scratch is read out of model memory as a real `[Value]`.

  Then: compile every `library/*.js`, run init and `beforeRender`
  natively, render a spread of pixel indices, and compare `Vm::pixel`,
  `Vm::pixel_written`, every global and the error/no-error verdict against
  the interpreter, bit for bit. Errors are compared down to the message,
  the function index, the word index and the source position.

  **These two gates are phase 3's entry criteria.** No image goes near the
  Seengreat until both are green on every library pattern: a codegen bug
  on metal is a crash on core 1 → watchdog reboot, on a board without
  serial.

### 7.2 QEMU (no device)

The packaged Espressif QEMU carries an `esp32s3` machine model, but the
harness and all its patches are for the classic `esp32` and our S3 image
has never been booted there — try `-M esp32s3` first (with PSRAM
emulation, if the model has it). If that does not boot, the classic
ESP32 route is cheap: a 16 KB `#[link_section = ".rwtext"] static` code
buffer is executable RAM on the ESP32 (SRAM0 is instruction-bus memory,
written with 32-bit stores), the LX6 executes every instruction the
emitter uses, and the existing `tools/qemu/run-all.py` shape runs the
firmware unmodified. The differential test drives the library through
`/api/code` with `jit` forced on and off and compares `/api/pixels`
snapshots per pattern — the interpreter is the oracle, bit-exact.

### 7.3 On metal (Seengreat, once 7.1 + 7.2 are green)

1. Same differential run over the library against the panel.
2. Microbench: one native function from PSRAM vs internal SRAM (§5).
3. The #260 table re-measured (`tools/hw-bench.mjs`, `patbench`,
   `opbench`), plus `compile_us` and PSRAM/heap deltas in `/api/status`.

A codegen bug on metal is a crash on core 1 → watchdog reboot, on a board
without serial; hence the order above, and the boot-loop guard stays.

**RAN 2026-09-24 (Gitea #665/#666).** All three, on two boards — the
Seengreat panel at 4096 px with the image in PSRAM, and the Athom at 144
and 2048 px with it in `.rwtext`. Every number is in docs/boards.md "JIT
on metal"; the headlines:

1. **Differential.** `tools/jit-diff.mjs` is the harness — the hardware
   counterpart of §7.2's gate, running the same off-then-on frame
   comparison over HTTP, with the honest caveat that a live board's clock
   cannot be frozen so time-dependent patterns are filtered rather than
   asserted. Both boards rendered correct pixels on every pattern tried.
   <!-- TODO(p4): the full library sweep's results (jit-diff.mjs) and the soak -->
2. **Microbench: PSRAM instruction fetch costs about 1 %** — `rainbow`
   7,276 µs from PSRAM against 7,305 µs from internal SRAM,
   `perlin-fire-wind-tunnel` 62,010 against 61,353. Both images are
   resident in the 32 KB instruction cache after the first frame, which is
   the answer §5 hoped for and the reason the arena is the default.
   `POST /api/jit {"place":"internal"}` is the lever that made it
   measurable.
3. **Speedups, panel** (4096 px, interpreted → native): `rainbow` 2.73× ·
   `snake` 5.13× · `perlin-fire-wind-tunnel` 3.02× · `aurora-2d` 2.08× ·
   `bulk-canvas-ripples-2d` 2.06×. **Athom** 1.85–5.15× at 144 and 2048 px.
   `compile_us` is 3.5–30 ms depending on program size and placement, once,
   at activation. `heap_free` does not move with a native image on the S3;
   `psram_free` drops by the 128 KB cap per image.

`snake-2d` at 4096 px is the one refusal on the panel, and not for a
codegen reason: the emitter's bookkeeping does not fit beside the engine
(`no-memory`, §5 as-built above). It compiles and runs natively on the
Athom.

## 8. Size, features, boards

- `luxel-core` feature `jit` (implies the verifier and the emitter; `no_std`,
  host-testable). Firmware feature `jit` adds the PSRAM exec allocator,
  cache steps and the `/api/status` object. `board-target.sh` sets
  `JIT=1` for `board-s3-devkit` and `board-seengreat-hub75`, `0`
  elsewhere, next to `IRAM`/`CORE_O3`; `JIT_OFF=1` is the A/B lever.

  **Updated by phase 5 (#666).** The three classic-ESP32 boards
  (`board-pixelblaze-v3`, `board-athom-music`, `board-esp32-generic`) have
  the tier as well, and `board-target.sh` sets `JIT="${CLASSIC_JIT:-0}"`
  for them — built and verified on the Athom, but not in a shipped image
  until the migrating release retires the 1,048,576 B slot check (Gitea
  #635; the image is ~72 KB over it). Flipping that default to 1 is the
  follow-up. The two RISC-V boards still set `0` and always will: there is
  no backend for them.

  As of #642 the luxel-core half exists and is **ON by default** (like
  `kinds`), so the browser wasm, the CLI and every `cargo test` carry the
  ABI surface and its tests; `jit = ["kinds"]`. The firmware depends on
  luxel-core with `default-features = false` and does **not** name it, which
  is why the phase-1 image delta on all three gate boards is **zero bytes**
  (`board-c6-devkit` + `hosted-ui` 1,026,752 B, `board-pixelblaze-v3`
  1,020,848 B, `board-seengreat-hub75` 985,632 B, unchanged). The separate
  `dispatch-table` feature (§4) does not imply `jit`, so the A/B build
  carries the table and nothing else.
- Budget: emitter + verifier + helpers + builtin table, estimated 25–40 KB
  at `opt-level = "s"` (MicroPython's Xtensa emitter is ~900 lines; ESPB's
  two-backend JIT is 56 KB). The S3 boards have ~170 KB of slot; the
  classic ESP32 and RISC-V boards do not carry the feature and gain only
  the kinds-section skip in the decoder (a few dozen bytes — the C6 gate
  must be checked, #543).
- No other board's behaviour changes: same blob, same interpreter.

## 9. Library census

### 9a. As shipped (phase 0, Gitea #625)

The prototype's numbers are in §9b below; these are what
`crates/luxel-core/src/kinds.rs` — the code the compiler runs, the decoder
checks and `jitcensus` now drives — actually produces over the 307
`library/*.js` patterns. They are pinned by
`crates/luxel-core/tests/kinds.rs`.

| result | with prelude (#626) | phase 0 (#625) | prototype (§9b) |
|---|---:|---:|---:|
| patterns typed + verified | 307 / 307 | 307 / 307 | — |
| **fully typed render path** | **286 / 307** | 286 / 307 | 291 / 307 |
| **… AND v1-eligible** | **286** | 281 | — |
| v1-excluded (`CallValue`) | 12 | 19 | — |
| locals proven `Num` | 94.7 % | 94.5 % | 95.1 % |
| render-path locals proven `Num` | 94.1 % | 94.0 % | 94.0 % |
| globals `ArrNum` / `Arr` / `Dyn` | 858 / 22 / 72 | 848 / 29 / 79 | 899 / 30 / 17 |
| `Box` sites (patterns) | 8 (6) | 8 (6) | 11 (9) |

The gap from the prototype to phase 0 is entirely the two soundness rules
§2.3 was missing: 4 patterns to the exported-global host write, 2 to the
declared init value (which the prototype dropped unconditionally). Nothing
regressed. The #626 column is the prelude: specialisation keeps the
callbacks' parameters typed, which recovers 7 patterns from the exclusion
set and 10 array globals from `Arr` to `ArrNum`.

### 9b. Prototype census (measured 2026-09-20)

This is the ONE-OFF measurement that sized the design, taken with a
throwaway inference that lived inside `jitcensus.rs`. That copy is gone —
the example drives `luxel_core::kinds` now (§9a) — so the table below is
history, kept because the design's decisions were made against it.

The prototype compiled every `library/*.js`
(307 patterns, shipping options: folded, fused, store-forwarded), walked
the v5 word stream and ran a prototype of the §2.3 inference — per-site
array provenance, callee sets for function values, globals seeded from
stores only (the §2.3 init rule). Self-check: the abstract walk reaches
99.8 % of all instructions; the rest are dead epilogues after an explicit
`return`.

| construct | patterns of 307 |
|---|---:|
| `CallValue` | 12 |
| `ConstFun` (function values) | 19 |
| higher-order builtin with a pattern-function callback | 8 (`arrayMutate` 5, `mapPixels` 2, `arrayMapTo` 1) |
| stores a non-`Num` into an array | 13 |
| conditional join whose edges differ in kind (needs `Box`) | 9 patterns, 11 sites |
| `renderFrame` / `render` / `render2D` / `render3D` | 35 / 183 / 128 / 27 |
| allocates no array at all | 121 |

Size: code words per pattern min 10, median 207, max 3 102 (93 196
total); functions median 7, max 57; locals median 15, max 167. **Max
static operand-stack depth per function: median 2, max 10** (2 of 2 399
functions exceed 8) — the §3.4 register plan's six register-homed depths
cover the common case and no function needs more than ten frame homes.

| inference result | share |
|---|---:|
| locals proven `Num` | 95.1 % (94.0 % of those on a render path) |
| user globals proven `Num` | 77.2 % |
| array globals proven `ArrNum` (vs `Arr`) | 899 vs 30 (96.8 %) |
| globals `Dyn` | 17 of 10 976 |
| **patterns with a fully typed render path** | **291 / 307 (94.8 %)** |

Only six causes of `Dyn` exist in the whole library, by slot count:
params of functions reached only through callbacks or `CallValue` (128
slots, 12 patterns); loads from arrays holding arrays or functions
(`utility-palettes` keeps lambdas in `modes[]`; `multisegment-demo` nests
`zones[i]`) (13); `CallValue` results (7); a load from an array of unknown
provenance (3); one assignment merge (`fire-blue.js`); one ternary of
differing kinds (`pew-pew-pew.js`). No builtin return kind was ever the
cause (`arrayReduce` is unused), and nothing triggers the poison rule.
Refining callback-argument kinds or `array(n)`'s zero fill each moves the
total by one pattern — not worth building.

**v1 scope check** (as measured then — the callback-builtin half of it is
moot since #626, see §9a): refusing every program that uses `CallValue` or
a callback-taking builtin excludes 19 / 307 (6.2 %); `renderFrame` patterns
are not meaningfully over-represented (4 of 35, three from one author's
sequencer family). **286 / 307 (93.2 %) are both v1-eligible and fully
typed**; of the 288 eligible, exactly two carry a `Dyn` slot on the render
path (`fire-blue`, `multisegment-demo`), and both compile — they just run
those slots boxed.

## 10. Risks and what retires them

| risk | retired by |
|---|---|
| windowed-ABI or `Ret2` return mismatch → wild jump | **retired for the two-word return** (#642): objdump of `lx_abi_probe_ret2` and of the `generic` thunks on the S3 image shows `a2:a3`, §7.1. The rest of §7.1's pins still owed before any device run |
| PSRAM instruction fetch slower than expected | **retired** (#665): the §7.3 microbench measured the same native function from both places — `rainbow` 7,276 vs 7,305 µs, `perlin-fire-wind-tunnel` 62,010 vs 61,353 — **~1 %**, because a few-KB image is resident in the 32 KB icache after frame one. The internal-SRAM placement stayed anyway, as the no-PSRAM fallback and as the lever that made this measurable |
| inference proves too little (`Dyn` on hot paths) | §9 census before the emitter is written; per-site array provenance is the refinement |
| `Value` `repr` change moves interpreter numbers | **retired** (#642): it was indeed the layout rustc already picks — `luxel-core` text 117,514 B before and after on the S3, every symbol the same size, `Vm::run` byte-identical, so there is no number to move |
| compile at activation blocks the render task | **retired** (#665/#666): `compile_us` on metal is 3,968–14,335 µs on the panel and 3,465–29,086 µs on the Athom (`snake-2d`'s 11 KB image is the 29 ms outlier) — 4 to 30 ms, once, at an activation that is already blocked for decode and engine construction. Nobody watching a panel can see it. Persisted blobs remain the lever if it ever matters |
| planner heap beside a 4096-px engine | **retired** (#665), the hard way: it was not a risk, it was a crash — `StackMap`'s `Vec`-per-word took the panel down on the first native run of `snake-2d`. Retired by the flat `StackMap`, the `tests/alloc_peak.rs` rule and the `no-memory` refusal that applies it before claiming exec memory. **Still open:** the bookkeeping is internal heap on a board with 8 MB of PSRAM idle beside it, which is why `snake-2d` stays interpreted at 4096 px — moving the planner's allocations into the arena is the follow-up |
| runaway native loop | fuel at back-edges, depth check in prologues, watchdog unchanged |

## 11. Open questions for Jeremy

All four answered by Jeremy on 2026-09-20 (Gitea #607):

1. Format bump to v6 — **accepted** (stale stored blobs recompile through
   the existing `bc-version` loop).
2. `Value` layout pinned by `#[repr(C, u32)]` — **accepted**.
3. Boxed (`Dyn`) variables shown as an editor lint — **accepted**.
4. Callback builtins — **resolved as §4/§4a, and shipped as #626**: all
   six keep their Pixelblaze names and semantics but stop being builtins,
   becoming pattern-language prelude functions the compiler links and
   specialises. Nothing is refused, so the editor has nothing to warn
   about.
