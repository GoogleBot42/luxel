# Luxel VM specification

Status: v0 draft, tracks `crates/luxel-core` as of M2. The VM is the
compilation target for the Pixel-Blaze-compatible frontend and is designed
to host **other language frontends** later: everything a frontend needs to
emit — the value model, instruction set, program container, and calling
convention — is specified here. In-memory structures are the contract for
now; a serialized format will be added when patterns are stored on-device.

## 1. Value model

A value (`Value`) is one of:

| variant      | payload | meaning                                     |
|--------------|---------|---------------------------------------------|
| `Num(Fx)`    | 16.16   | number (the default; `0` when uninitialized) |
| `Arr(u32)`   | handle  | array reference into the VM array arena      |
| `Fun(u16)`   | fn index| reference to a pattern function              |
| `Builtin(u16)`| table index | reference to a builtin function          |

### 1.1 Numbers: 16.16 fixed point (`Fx`)

- Two's-complement `i32` raw, value = raw / 65536. Range ±32768 (exclusive),
  resolution 1/65536.
- **All arithmetic wraps** on overflow (hardware-confirmed against Pixel
  Blaze: `hypot` and large products wrap, they don't saturate).
- Source literals are quantized as **16.15**: parse as binary64, truncate
  toward zero to 16.16, then clear the LSB (`raw & !1`). Applies to every
  literal including predefined constants (`PI` = raw 205886, not 205887).
  Runtime results use the full 16.16 resolution.
- Division/modulo by zero → 0. `log`/`log2` of ≤ 0 → most negative value.
  `sqrt` is sign-preserving (`sqrt(-4) = -2`). `round(x) = floor(x + 0.5)`.
- Shift counts use `raw >> 16` of the operand, masked `& 31`.
- Truthiness: a number is true iff raw ≠ 0. References are always true.

### 1.2 Arrays

- Arena of `Vec<Value>` owned by the VM; `Arr(u32)` is an arena index.
  Arrays are reference values (aliasing is visible, as in PB).
- Allocation is budgeted: total elements across all arrays ≤
  `array_budget` (default 10 240); exceeding it is a runtime error.
- Indexing truncates fractional indices toward zero — for reads **and**
  writes. Any out-of-bounds or negative index (read or write) raises a
  runtime error that aborts the current entry point. (PB divergence, on
  purpose: PB aborts on *literal*-index fractional writes but truncates
  variable-index ones; we truncate uniformly. All OOB behavior matches.)

### 1.3 Equality

`Eq`/`Ne` compare numbers numerically; references compare by identity
(same handle / same function index). `===`/`!==` in the PB frontend fold
to the same instructions (PB's compiler does the same).

## 2. Program container

```
Program {
  fns:          Vec<FnDef>,          // fns[0] = top-level init code
  globals:      Vec<GlobalDef>,      // slot-indexed
  exported_fns: Vec<(String, u16)>,  // "render", "beforeRender", …
  pixel_count_g: u16,                // global slot of pixelCount
}

FnDef {
  name:   String,
  params: u8,             // leading local slots receive arguments
  locals: u8,             // total local slots, params included
  code:   Vec<Insn>,
  pos:    Vec<(u32,u32)>, // debug: 1-based (line,col) per instruction
  local_names: Vec<String>,
}

GlobalDef { name, export: bool, init: Fx, predefined: bool }
```

- **`fns[0]` is the module initializer**: global statements compile into
  it; the host runs it once after construction. All other functions are
  called by index.
- Globals are a flat slot array. `predefined` marks engine-provided slots
  (constants, `pixelCount`, GPIO names) — debuggers hide these.
- `export` on a global makes it visible to the host's vars API;
  `exported_fns` are the host-callable entry points. Frontends decide what
  to export; the engine looks for `render`, `render2D`, `render3D`,
  `beforeRender` and control functions by name.

## 3. Execution model

Stack machine with three per-VM stacks: operand stack (`Value`), locals
(contiguous, frames own a base offset), and call frames
`{fn_idx, pc, locals_base, stack_base}`.

- **Fuel**: each host entry (init, `beforeRender`, each `render` call, any
  `call()`) resets a fuel counter (8 000 000 instructions); running dry is
  a runtime error. Guarantees the host loop never hangs on a bad pattern.
- **Call depth** ≤ 48 frames.
- **Runtime errors never crash the host**: they carry `(fn_idx, pc)` plus
  resolved `(line, col)` and abort only the current entry point. The engine
  reports the first error per frame and keeps rendering.
- **Calling convention**: `CallFn`/`CallValue` pop `argc` arguments (max
  16). Missing parameters default to `Num(0)`; extra arguments are popped
  and dropped. Every function returns exactly one value (`RetNull` returns
  0). Builtins may re-enter the VM (higher-order builtins like `arrayMap`
  run their callback to completion — no debug pausing inside).
- The interpreter is resumable: `start`/`resume` return `Done(Value)` or
  `Paused` (breakpoint/step hit). Frames, locals, and the operand stack
  stay intact while paused; the engine builds mid-frame pause and stepping
  on top of this (see `docs/research/` debugger notes).

## 4. Instruction set

Operands are immediates encoded in the instruction. Stack effects use
`[before] → [after]`, top of stack on the right.

### Constants, variables, arrays

| insn | operands | effect | notes |
|---|---|---|---|
| `Const` | `Value` | `[] → [v]` | numbers, function refs, builtin refs |
| `LoadG` | slot:u16 | `[] → [v]` | |
| `StoreG` | slot:u16 | `[v] → [v]` | assignment is an expression; value stays |
| `LoadL` | slot:u8 | `[] → [v]` | slot is frame-relative; params first |
| `StoreL` | slot:u8 | `[v] → [v]` | |
| `LoadIdx` | | `[arr i] → [elem]` | truncates i; OOB = error |
| `StoreIdx` | | `[arr i v] → [v]` | ditto |
| `ArrLen` | | `[arr] → [len]` | |
| `NewArray` | n:u16 | `[] → [arr]` | n zeros; counts against array budget |

### Stack

| insn | effect |
|---|---|
| `Dup` | `[a] → [a a]` |
| `Dup2` | `[a b] → [a b a b]` |
| `Pop` | `[a] → []` |

### Arithmetic and logic

All operate on numbers (references coerce to 0 in arithmetic) and wrap:

`Add Sub Mul Div Rem Neg` — 16.16 arithmetic (`Div`/`Rem` by 0 → 0;
`Rem` is the truncated remainder on raws, sign of the dividend).
`Not` — logical: `[a] → [!truthy(a)]` as 0/1.
`BitAnd BitOr BitXor` — on the **full raw** including fraction bits
(PB semantics; this is why `1 >> 16` has raw 1).
`Shl Shr` — shift the raw; count = integer part of the rhs, masked `& 31`.
`BitNot` — `!raw`, then the low 16 bits are cleared (PB documents `~` as
the one bitwise op that zeros the fractional bits of its result).
`Lt Le Gt Ge Eq Ne` — push 0/1.

### Control flow

| insn | operands | effect |
|---|---|---|
| `Jmp` | pc:u32 | absolute target within the function |
| `JmpIfFalse` | pc:u32 | pops condition |
| `JmpIfTruePeek` | pc:u32 | `\|\|`: jump keeping lhs on stack, else pop |
| `JmpIfFalsePeek` | pc:u32 | `&&`: dito |
| `CallFn` | fn:u16, argc:u8 | pops argc args, pushes result |
| `CallBuiltin` | b:u16, argc:u8 | index into the builtin table |
| `CallValue` | argc:u8 | `[callee a1..an] → [result]`; callee is `Fun`/`Builtin`, anything else = runtime error |
| `Ret` | | pops return value, unwinds frame |
| `RetNull` | | returns 0 |

### Builtins

The builtin table (`vm::BUILTINS`) is part of this specification: a
frontend resolves builtin names to stable indices at compile time.
Builtins are first-class (`Const(Builtin(i))` makes them assignable and
passable, matching PB). The set covers PB's documented API: math,
waveforms, array HOFs, `hsv`/`rgb`, coordinate transforms, time/clock,
pixel maps, perlin noise, prng, GPIO stubs. See the table in
`crates/luxel-core/src/vm.rs` for the authoritative list; semantics that
diverge from real PB hardware are documented in
`docs/research/04-oracle-findings.md`.

## 5. Host interface (engine contract)

The engine (any host: firmware, wasm, CLI) drives a program as:

1. construct VM, set `pixelCount` global, run `fns[0]` (init);
2. per frame: call `beforeRender(delta_ms)` if exported, then `render*`
   per pixel. Coordinate-transform state is **not** auto-reset between
   frames (hardware-confirmed) — patterns call `resetTransform()`
   themselves; ops compose by pre-multiplication so points transform in
   call order;
3. `hsv`/`rgb` write `vm.pixel` + `pixel_written`, which the host reads
   after each render call;
4. exported globals are readable/writable between frames (vars API);
   control functions (`sliderX`, `hsvPickerY`, …) are invoked with the
   UI-provided values.

Debug hooks (breakpoints as `(fn_idx, pc)`, step Continue/Over/Into/Out,
frame/locals/globals inspection) are host-optional; `dbg: None` is the
zero-overhead fast path.

## 6. Stability

The instruction set is **not yet frozen**. Until a serialized bytecode
format ships, the compiler and VM are versioned together and other
frontends should target the `Program`-building API in-process. When the
on-flash format lands, this document gains opcode numbers, an encoding,
and a version header — that is the point where backward compatibility
starts being guaranteed.
