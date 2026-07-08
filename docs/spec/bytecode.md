# LXBC — serialized pattern bytecode

Status: **v1, implemented** (`crates/luxel-core/src/bytecode.rs`). This is the
wire/flash encoding of the in-memory `vm::Program` (see `vm.md` §2). It exists
so devices can *execute* patterns without linking the lexer/parser/compiler:
the browser (wasm) or CLI compiles source → LXBC; the device stores the blob
alongside the source and only ever deserializes + runs it.

Design constraints, in priority order:

1. **Safe to load untrusted bytes.** The VM does not bounds-check function,
   global, local, or builtin indices at runtime (it trusts `Program`), so the
   *decoder* validates everything structurally. A malformed or hostile blob
   must produce a decode error, never a panic on device.
2. **Robust to builtin-table growth.** Builtins are referenced **by name**
   through a per-blob import table, resolved to runtime ids at load. Adding
   (or even reordering) builtins in firmware does not invalidate existing
   blobs; only a blob that names a builtin the firmware doesn't know fails —
   with a "firmware too old" style error.
3. **Version skew is detectable, not survivable.** A `version` mismatch is a
   distinct error (`BcError::Version`) so hosts can react (the web IDE
   recompiles from the stored source and re-saves). Devices cannot recompile;
   the format version only changes when the instruction set or container
   actually changes.

## Container

All integers little-endian. `str8` = `u8` length + UTF-8 bytes.

```
0   4   magic "LXBC"
4   u16 version          (currently 4)
6   u16 flags            bit0: debug info present; others reserved (0)
8   u16 pixel_count_g    global slot holding pixelCount
10  u16 n_globals        (≤ 256)
12  u16 n_fns            (≥ 1, ≤ 1024; fn 0 is top-level init)
14  u16 n_exports        (≤ 1024)
16  u16 n_imports        builtin import table size (≤ 512)
18  u16 n_data           const-array pool entries (≤ 4096; v3+)
20  u16 n_msgs           assert-message table entries (≤ 4096; v4+)
22  …   sections, in order: imports, globals, data, msgs, fns, exports
```

**imports** — `n_imports × str8`: builtin names. Instructions reference
builtins by import slot; the decoder resolves each name via `lookup_builtin`
and rewrites the operand to the runtime builtin id.

**globals** — `n_globals ×`:

```
str8 name
u8   flags      bit0 export, bit1 predefined
i32  init       Fx raw (16.16)
```

Predefined globals (pixelCount, PI, GPIO constants, …) are serialized like
any other slot: the blob is self-contained and slot numbering is preserved
exactly (opcodes index globals by slot).

**data** (v3+) — `n_data × { u16 len, len × i32 }`: the pattern's
"`.rodata`" — every all-numeric array literal, **deduplicated by content**
(pixel-art patterns repeat identical rows hundreds of times; Emoji
Animation #2's 768 `[r,g,b]` literals collapse to 4 pool entries). Total
elements ≤ 65 536. The `ConstArr` opcode allocates an arena array that
*shares* a pool entry copy-on-write: each literal occurrence keeps its own
mutable identity, but no bytes are copied until (unless) it is written.

**msgs** (v4+) — `n_msgs × str8`: `assert()` messages (custom text, or the
condition's source when none was given), **deduplicated**. These are
user-facing error text, not debug info — lean decodes keep them, so a
compiler-less device reports `pattern requires: <message> (pixelCount = N)`
verbatim.

**fns** — `n_fns ×`:

```
str8 name                (kept even without debug info: runtime errors name it)
u8   params
u16  locals              total slots incl. params (≤ 255; params ≤ locals)
u32  code_len            code section length in BYTES (≤ 65 536)
code_len bytes of code   (see opcode table)
if flags.debug:
  u32 n_runs             source-position runs, strictly ascending offsets
  n_runs × { u32 offset, u32 line, u32 col }
  locals × str8          local slot names (params first)
```

The VM executes this code section **in place**: `pc` is a byte offset into
it, and each position run covers from its offset to the next run's. v1
(instruction-indexed jumps, per-instruction RLE positions) required
materializing an instruction array at load — several times the blob size in
RAM, which is what small-heap devices cannot afford.

**exports** — `n_exports × { str8 name, u16 fn_idx }`.

## Opcodes

One `u8` opcode + fixed-width operands. Jump targets are function-relative
**byte offsets**; a target equal to `code_len` means "fall off the end"
(returns like RetNull). The decoder proves every jump lands on an
instruction boundary, so the in-place interpreter can never misalign.

| op | insn | operands |
|----|------|----------|
| 0x01 | Const Num | i32 (Fx raw) |
| 0x02 | Const Fun | u16 fn index |
| 0x03 | Const Builtin | u16 import slot |
| 0x04 | LoadG | u16 |
| 0x05 | StoreG | u16 |
| 0x06 | LoadL | u8 |
| 0x07 | StoreL | u8 |
| 0x08 | LoadIdx | |
| 0x09 | StoreIdx | |
| 0x0A | ArrLen | |
| 0x0B | NewArray | u16 |
| 0x0F | ConstArr | u16 const-pool index (v3+) |
| 0x0C | Dup | |
| 0x0D | Dup2 | |
| 0x0E | Pop | |
| 0x10–0x15 | Add Sub Mul Div Rem Pow | |
| 0x16–0x1D | Neg Not BitNot BitAnd BitOr BitXor Shl Shr | |
| 0x20–0x25 | Lt Le Gt Ge Eq Ne | |
| 0x30 | Jmp | u32 |
| 0x31 | JmpIfFalse | u32 |
| 0x32 | JmpIfTruePeek | u32 |
| 0x33 | JmpIfFalsePeek | u32 |
| 0x38 | CallFn | u16 fn, u8 argc |
| 0x39 | CallBuiltin | u16 import slot, u8 argc |
| 0x3A | CallValue | u8 argc |
| 0x3E | Ret | |
| 0x3F | RetNull | |
| 0x40 | Assert | u16 msg-table index (v4+) |

`Assert` pops the condition; falsy aborts the run with an `is_assert`
error carrying the message (plus pixelCount context). The compiler only
emits it in fn 0 (top-level init).

`Const` of an array value is not representable (the compiler never emits it;
arrays are built at runtime by `NewArray`).

## Decoder validation

Rejected at decode time (`BcError::Malformed`): bad magic; section overrun /
trailing bytes; counts over the caps above; invalid UTF-8; unknown opcode;
an instruction overrunning its function's code section; `fn_idx ≥ n_fns`
(in `Const Fun`, `CallFn`, exports); `slot ≥ n_imports`; unresolvable
builtin name; global operand ≥ `n_globals`; `pixel_count_g ≥ n_globals`;
local operand ≥ `locals`; `params > locals`; `argc > 16`; a jump target
past `code_len` or not on an instruction boundary; debug runs not strictly
ascending or past the code. Total blob size is capped at 256 KiB.
`BcError::Version` is reserved for a `version` field mismatch.

An accepted blob's code section is kept verbatim (builtin import slots are
rewritten to runtime ids in the copy — same width) and re-encodes
byte-identically (the corpus round-trip test asserts this). Every invariant
the in-place interpreter relies on is proven here.

## Debug info

`flags.debug` gates source positions (offset-keyed runs — statement
granularity, so a handful per function) and local names. Devices decode
with `deserialize_lean`, which skips storing both: runtime errors report
`fn`/`pc` but `(line, col) = (0, 0)`, and debugger stack panes lose local
names. Everything else — by-name vars, controls, sensor bindings, exported
functions — works either way, since global and function names are always
present. Producers always emit debug info.

## What LXBC is not

- Not an interchange format for pattern *sharing* — `.epe` (source) remains
  that. LXBC accompanies source; source is the durable artifact.
- Not stable across `version` bumps, deliberately. Devices reject stale
  blobs; hosts with a compiler recompile from the stored source.
