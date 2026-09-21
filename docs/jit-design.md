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
one kind byte per param and per local (`params + locals` bytes, in slot
order). Nothing per instruction: the operand stack's kinds at every word
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
  return kind (`Num` for nearly all; `Arr`/`ArrNum` for `array(n)`; `Dyn`
  for the few that return an element or a callback result), and, for the
  array-writing builtins, the kind they store (`Num` for the bulk math
  ops; the callback's return kind for `arrayMapTo` / `arrayMutate`).
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
under the `jit` feature only; boards without a backend parse and skip the
section (kinds are advisory to the interpreter), so the C6's 14 B of slot
margin is untouched.

### 2.5 Format changes

- `FORMAT_VERSION` 5 → 6. Header `flags` bit 1 = `TYPED`. A `kinds`
  section is appended after `exports` (before the padding to `words_off`):
  `n_globals` kind bytes, then per function `1 + params + locals` kind
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

`#[repr(C)]`, offsets asserted by a test against the constants the
emitter uses:

```
vm: *mut Vm            status: i32          insn_at: u32        fn_idx: u16
fuel: i32              stack_limit: usize   args: [i32; 34]     err: Option<VmError>
fn_table: *const usize builtins: *const BuiltinEntry
```

`Vm` itself is untouched by generated code; every access to arrays,
globals-by-builtin, the pixel brush, the frame buffer and the RNG goes
through helpers that are thin `extern "C"` wrappers over today's `Vm`
methods. Globals are the exception: a `LoadG`/`StoreG` on a typed global
is `l32i`/`s32i` into a `#[repr(C)]` globals array the helper side shares
(`Vm::globals` becomes that array; the interpreter reads it as before).

## 4. Builtin table

`BUILTINS` stays the append-only name table. Beside it, under the `jit`
feature, `BUILTIN_ENTRIES: [BuiltinEntry; N]` with one entry per id:

```rust
#[repr(C)]
pub struct BuiltinEntry {
    generic: unsafe extern "C" fn(*mut JitCtx, *const Value, u32) -> RetDyn,
    direct:  usize,        // 0, or a numeric-signature fn for the tier-1 set
    direct_sig: u8,        // N1 (i32)->i32, N2, N3, N4, C1 (ctx,i32)->i32, C3 (hsv/rgb)…
    ret_kind: u8,          // §2.3 signature table, shared with the inference
}
```

The `generic` wrappers call exactly the arms `builtin_fast` / `builtin_hot`
/ `builtin_cold` run today (the interpreter's dispatch is not changed by
this design — #328 showed its tiers are about I-cache residency, and the
JIT does not go through them). Higher-order builtins call back through
`Vm::dispatch_direct`, which becomes an indirection: interpreter mode →
`run_on_top`; native mode → the callee's native entry with `Dyn` params
via `ctx.args`. That keeps the five callback builtins (`arrayForEach`,
`arrayMutate`, `arrayMapTo`, `arrayReduce`, `arraySortBy`, `mapPixels`)
native without any interpreter re-entry.

**Decided 2026-09-20 (Jeremy):** v1 compiles `CallValue` natively (it is
native → native through the entry table; the callee's params and return
are `Dyn`). The six callback builtins are all Pixelblaze API
(`arrayForEach`, `arrayMutate`, `arrayMapTo`, `arrayReduce`,
`arraySortBy`, `mapPixels`; 8 of 294 scraped PB patterns and 8 of 307
library patterns use one) and **stay in the language**. They leave the
JIT's hard set by being **defined in the pattern language as a prelude**
(Jeremy's idea, 2026-09-20, Gitea #626), not by a trampoline: the
compiler bundles their definitions, links in the ones a program uses, and
on the wire and on the device they are ordinary pattern functions. Two
rules make it hold:

- **Always, never per target.** One blob runs everywhere (store, sync
  pull, playlist), so the interpreter runs the prelude loop too. Cost:
  ~10 ops/element at ~100 cycles/op ≈ 17 ms per 4096-element call on the
  S3 interpreter — irrelevant for the init-time fills that are 9 of the
  10 library call sites, ~2 frames for the one per-frame use
  (`arrayMapTo` in `bulk-canvas-ripples-2d`), measured in #626.
- **Specialise on a static callback.** Inside a prelude function the
  callback is a parameter, so its call is `CallValue` and its params
  would be `Dyn` (§2.3). When the call-site argument is a literal lambda
  or a named function — every library and corpus call site — the compiler
  clones the prelude function for that site and binds the callback into a
  direct `CallFn`, so the callback keeps typed params. A run-time callback
  value goes through the unspecialised copy, boxed.

`arraySortBy` is an insertion sort in the prelude; `mapPixels` needs one
small builtin returning a pixel's mapped coordinates by index and then
lowers the same way. With #626 landed the §4a refusal list is empty for
the library, and the Rust → native trampoline (`dispatch_direct`
indirection) is never built. **They are no longer builtins at all**
(Jeremy, 2026-09-20): the `vm.rs` arms, `dispatch_direct` and `run_on_top`
are deleted — no old blob can reach them after the v6 bump — and the six
`BUILTINS` ids become append-only tombstones (`BKind::Removed`); the
Pixelblaze oracle is the reference for the prelude's correctness.

### 4a. Refusal semantics and the editor warning

A refusal is **whole-program**: the pattern runs in the interpreter
exactly as today, at the interpreter's speed, with the same pixels — no
function-level mixing, ever (decision 2). `/api/status` carries
`jit: {state: "interp", reason}` with `reason` one of `callbacks`
(a callback that is only a run-time value — the prelude's unspecialised copy is `CallValue`, which v1 compiles; so this reason exists only for a construct the prelude cannot express yet), `too-large`, `psram`,
`kinds` (verifier failure — a compiler bug, reported loudly), `debug`
(debugger attached), `unsupported` (anything else, with the opcode).

**Jeremy's note (2026-09-20): the editor must warn when a construct forces
interpreter mode.** The compiler knows at compile time whether a program
will be refused for `callbacks` (the only reason that is a property of
the source), so the playground shows a warning at the offending call
site — "`mapPixels` runs this pattern in the interpreter on JIT boards" —
in the same channel as the `Dyn`-variable lint (§11 answer 3), before the
pattern is ever pushed. The device-side reasons (`too-large`, `psram`)
surface from `/api/status` next to the frame rate after activation.

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
- `Vm::dispatch_direct` becomes the mode indirection (§4). `Value` gets
  its `repr`. `Vm::globals` becomes a `#[repr(C)]` word array. Everything
  else in `Vm` is untouched.
- The interpreter path is byte-for-byte the same code as today when
  `native` is `None`; on boards without the `jit` feature the field does
  not exist.

## 7. Verification plan

### 7.1 Host (no device, every CI run)

- **Encoder**: each instruction form is emitted into a buffer and
  disassembled with the devshell's `xtensa-esp-elf-objdump -D -m xtensa
  -b binary`; the test compares mnemonics and operands. This is the whole
  "did I get the bit layout right" question, answered by the vendor
  toolchain.
- **ABI pins**: a Rust `extern "C"` helper returning `Ret2` is compiled
  for `xtensa-esp32s3-none-elf` and its disassembly asserted to return in
  `a2:a3`; `JitCtx`/`Value`/`BuiltinEntry` offsets are `const`-asserted
  against the emitter's constants.
- **Inference + verifier**: unit tests per rule; the library-wide census
  (§9) is a test that pins the count of typed render paths so a compiler
  regression is caught.
- **Emitter as pure function**: `compile(&Program) -> Result<Vec<u32>,
  Refusal>` runs on x86; a golden test pins the bytes for a handful of
  patterns so an accidental codegen change is a visible diff.

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

## 8. Size, features, boards

- `luxel-core` feature `jit` (implies the verifier and the emitter; `no_std`,
  host-testable). Firmware feature `jit` adds the PSRAM exec allocator,
  cache steps and the `/api/status` object. `board-target.sh` sets
  `JIT=1` for `board-s3-devkit` and `board-seengreat-hub75`, `0`
  elsewhere, next to `IRAM`/`CORE_O3`; `JIT_OFF=1` is the A/B lever.
- Budget: emitter + verifier + helpers + builtin table, estimated 25–40 KB
  at `opt-level = "s"` (MicroPython's Xtensa emitter is ~900 lines; ESPB's
  two-backend JIT is 56 KB). The S3 boards have ~170 KB of slot; the
  classic ESP32 and RISC-V boards do not carry the feature and gain only
  the kinds-section skip in the decoder (a few dozen bytes — the C6 gate
  must be checked, #543).
- No other board's behaviour changes: same blob, same interpreter.

## 9. Library census (measured 2026-09-20)

`crates/luxel-cli/examples/jitcensus.rs` compiles every `library/*.js`
(307 patterns, shipping options: folded, fused, store-forwarded), walks
the v5 word stream and runs a prototype of the §2.3 inference — per-site
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

**v1 scope check**: refusing every program that uses `CallValue` or a
callback-taking builtin (§3.5 lists them as native, but they are the
last thing to build) excludes 19 / 307 (6.2 %); `renderFrame` patterns
are not meaningfully over-represented (4 of 35, three from one author's
sequencer family). **286 / 307 (93.2 %) are both v1-eligible and fully
typed**; of the 288 eligible, exactly two carry a `Dyn` slot on the render
path (`fire-blue`, `multisegment-demo`), and both compile — they just run
those slots boxed.

## 10. Risks and what retires them

| risk | retired by |
|---|---|
| windowed-ABI or `Ret2` return mismatch → wild jump | §7.1 objdump pins before any device run |
| PSRAM instruction fetch slower than expected | §7.3 microbench; internal-SRAM placement for `render` only is the fallback |
| inference proves too little (`Dyn` on hot paths) | §9 census before the emitter is written; per-site array provenance is the refinement |
| `Value` `repr` change moves interpreter numbers | opbench/patbench A/B on the panel; the layout is what rustc already picks |
| compile at activation blocks the render task | `compile_us` measured; persisted blobs are the lever |
| runaway native loop | fuel at back-edges, depth check in prologues, watchdog unchanged |

## 11. Open questions for Jeremy

All four answered by Jeremy on 2026-09-20 (Gitea #607):

1. Format bump to v6 — **accepted** (stale stored blobs recompile through
   the existing `bc-version` loop).
2. `Value` layout pinned by `#[repr(C, u32)]` — **accepted**.
3. Boxed (`Dyn`) variables shown as an editor lint — **accepted**.
4. Callback builtins — **resolved as §4/§4a**: keep all six (they are
   Pixelblaze API), lower the four loop-shaped ones in the compiler when
   the callback is static, refuse only `arraySortBy`/`mapPixels` in v1,
   and **warn in the editor** whenever a construct forces interpreter
   mode.
