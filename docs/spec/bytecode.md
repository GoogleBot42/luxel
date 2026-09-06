# LXBC — serialized pattern bytecode

Status: **v5, implemented** (`crates/luxel-core/src/bytecode.rs`). This is the
wire/flash encoding of the in-memory `vm::Program` (see `vm.md` §2). It exists
so devices can *execute* patterns without linking the lexer/parser/compiler:
the browser (wasm) or CLI compiles source → LXBC; the device stores the blob
alongside the source and only ever validates + runs it.

Design constraints, in priority order:

1. **Safe to load untrusted bytes.** The VM does not bounds-check function,
   global, local, or builtin indices at runtime (it trusts `Program`), so the
   *decoder* validates everything structurally. A malformed or hostile blob
   must produce a decode error, never a panic on device.
2. **Execution-ready.** The instruction stream and the constant pool are one
   4-byte-aligned region of little-endian `u32` words that the VM runs
   exactly as stored: nothing is rewritten, re-encoded or unpacked at load.
   A device whose pattern store is memory-mapped (`firmware/src/flashmap.rs`,
   docs/research/flash-mmap.md) executes straight from flash, and the
   pattern costs no RAM for its code or constants — RAM is the scarce
   resource on every board (Gitea #260).
3. **Robust to builtin-table growth.** Instruction words carry the runtime
   builtin id (`vm::BUILTINS` is append-only, so ids are stable), and every
   id a blob uses is also listed by *name* in its import table. The decoder
   checks each name resolves to exactly that id on this build; a blob from a
   newer compiler fails with the builtin's name ("`foo` is not available on
   this firmware — recompile the pattern"), never with a bare index.
4. **Version skew is detectable, not survivable.** A `version` mismatch is a
   distinct error (`BcError::Version`) so hosts can react (the web IDE
   recompiles from the stored source and re-saves). Devices cannot recompile;
   the format version only changes when the instruction set or container
   actually changes.

## Container

All integers little-endian. `str8` = `u8` length + UTF-8 bytes.

```
0   4   magic "LXBC"
4   u16 version          (currently 5)
6   u16 flags            bit0: debug info present; others reserved (0)
8   u16 pixel_count_g    global slot holding pixelCount
10  u16 n_globals        (≤ 256)
12  u16 n_fns            (≥ 1, ≤ 1024; fn 0 is top-level init)
14  u16 n_exports        (≤ 1024)
16  u16 n_imports        builtin import table size (≤ 512)
18  u16 n_data           const-array pool entries (≤ 4096)
20  u16 n_msgs           assert-message table entries (≤ 4096)
22  u32 words_off        byte offset of the word region (multiple of 4, ≥ 30)
26  u32 n_words          word-region length in u32 words (≤ 65 536)
30  …   tables, in order: imports, globals, pool, msgs, fns, exports
    …   zero padding to a multiple of 4 (0–3 bytes)
words_off  n_words × u32   the word region: code, then the constant pool
```

`words_off + 4·n_words` must equal the blob length: the word region is the
tail of the blob, so a blob stored at a 4-byte-aligned flash offset has its
words 4-byte aligned in memory and the VM reads them as `&[u32]` in place.
The tables may be followed only by the (< 4 bytes of) zero padding.

**imports** — `n_imports × { str8 name, u16 id }`: every builtin the code
references, with the runtime id the compiler emitted. The decoder resolves
each name via `lookup_builtin` and requires the result to equal `id`; each
`Const Builtin` / `CallBuiltin` operand must then be one of the imported
ids. Nothing is rewritten.

**globals** — `n_globals ×`:

```
str8 name
u8   flags      bit0 export, bit1 predefined
i32  init       Fx raw (16.16)
```

Predefined globals (pixelCount, PI, GPIO constants, …) are serialized like
any other slot: the blob is self-contained and slot numbering is preserved
exactly (opcodes index globals by slot).

**pool** — `n_data × { u32 start, u16 len }`: the constant-array pool
table. Entry `d` is the `len` words at `words[start .. start+len]`, each a
raw 16.16 value (constant arrays are all-numeric by construction). This is
the pattern's "`.rodata`": every all-numeric array literal, **deduplicated
by content** (pixel-art patterns repeat identical rows hundreds of times).
Total elements ≤ 65 536; every range must lie inside the word region. The
`ConstArr` opcode allocates an arena array that *references* a pool entry
copy-on-write: reads decode words to `Value::Num` on access, and no bytes
are copied until (unless) the array is written.

**msgs** — `n_msgs × str8`: `assert()` messages (custom text, or the
condition's source when none was given), **deduplicated**. These are
user-facing error text, not debug info — lean decodes keep them, so a
compiler-less device reports `pattern requires: <message> (pixelCount = N)`
verbatim.

**fns** — `n_fns ×`:

```
str8 name                (kept even without debug info: runtime errors name it)
u8   params
u16  locals              total slots incl. params (≤ 255; params ≤ locals)
u32  code_start          word index of the function's code in the word region
u32  code_len            code length in WORDS (≤ 16 384 = 64 KiB)
if flags.debug:
  u32 n_runs             source-position runs, strictly ascending pcs
  n_runs × { u32 pc, u32 line, u32 col }
  locals × str8          local slot names (params first)
```

The VM executes `words[code_start .. code_start+code_len]` **in place**:
`pc` is a function-relative word index into it, and each position run
covers from its pc to the next run's. Breakpoints (`(fn_idx, pc)`), error
locations and the debugger's frame pcs are all word indices.

**exports** — `n_exports × { str8 name, u16 fn_idx }`.

## Instruction words

One `u32` per instruction: **bits 0..8 the opcode, bits 8..32 a 24-bit
operand field**. How the field is read depends on the opcode:

| field | bits | used by |
|---|---|---|
| `u8` | 8..16 | LoadL, StoreL, CallValue (argc) |
| `u16` | 8..24 | Const Fun, Const Builtin, LoadG, StoreG, NewArray, ConstArr, Assert |
| `u16` + `u8` | 8..24 index, 24..32 argc | CallFn, CallBuiltin |
| `u24` | 8..32 | Jmp, JmpIfFalse, JmpIfTruePeek, JmpIfFalsePeek (word-index target) |
| (none) | — | everything else; the field must be 0 |

`Const Num` is the only two-word instruction: the opcode word (field 0)
is followed by one immediate word holding the raw 16.16 `i32`. Bits an
opcode does not use must be zero — the encoding is canonical, and the VM
masks only what it needs (`bytecode::enc`). The dispatch loop is one
bounds-checked word load per instruction: `op = w as u8`, `imm = w >> 8`,
and `code[pc + 1]` for a constant.

Jump targets are function-relative **word indices**; a target equal to
`code_len` means "fall off the end" (returns like RetNull). The decoder
proves every jump lands on an instruction boundary (i.e. never on a
`Const Num` immediate), so the in-place interpreter can never misalign.

| op | insn | operand |
|----|------|---------|
| 0x01 | Const Num | — (+ 1 immediate word: i32 Fx raw) |
| 0x02 | Const Fun | u16 fn index |
| 0x03 | Const Builtin | u16 runtime builtin id (must be imported) |
| 0x04 | LoadG | u16 |
| 0x05 | StoreG | u16 |
| 0x06 | LoadL | u8 |
| 0x07 | StoreL | u8 |
| 0x08 | LoadIdx | |
| 0x09 | StoreIdx | |
| 0x0A | ArrLen | |
| 0x0B | NewArray | u16 |
| 0x0F | ConstArr | u16 const-pool index |
| 0x0C | Dup | |
| 0x0D | Dup2 | |
| 0x0E | Pop | |
| 0x10–0x15 | Add Sub Mul Div Rem Pow | |
| 0x16–0x1D | Neg Not BitNot BitAnd BitOr BitXor Shl Shr | |
| 0x20–0x25 | Lt Le Gt Ge Eq Ne | |
| 0x30 | Jmp | u24 word index |
| 0x31 | JmpIfFalse | u24 word index |
| 0x32 | JmpIfTruePeek | u24 word index |
| 0x33 | JmpIfFalsePeek | u24 word index |
| 0x38 | CallFn | u16 fn, u8 argc |
| 0x39 | CallBuiltin | u16 runtime builtin id (must be imported), u8 argc |
| 0x3A | CallValue | u8 argc |
| 0x3E | Ret | |
| 0x3F | RetNull | |
| 0x40 | Assert | u16 msg-table index |
| 0x41–0x4E | superinstructions | fused sequences — see below |
| 0x4F–0xFF | *reserved* | |

`Assert` pops the condition; falsy aborts the run with an `is_assert`
error carrying the message (plus pixelCount context). The compiler only
emits it in fn 0 (top-level init).

`Const` of an array value is not representable (the compiler never emits it;
arrays are built at runtime by `NewArray` / `ConstArr`).

Cost: 4 bytes per instruction, 8 for a numeric constant — about 4.6 B per
instruction on the library versus 2.6 B for v4's byte encoding. That is the
price of fixed-width dispatch and of executing without a RAM copy; on the
device the blob lives in flash either way.


## Superinstructions

Opcodes `0x41..0x4E` are **fused sequences**: each one does exactly what a
run of two or three base instructions did, in one dispatch. The compiler's
peephole (`compile::peephole`) emits them; nothing else in the language,
the VM's semantics or the container changes, so **the format version does
not move** — a v5 blob that contains none of them (an older producer's, or
`luxel bench --no-fuse`'s) validates and runs exactly as before, and every
decoder that reads v5 must accept these opcodes.

They were chosen from dynamic execution counts over all 299 library
patterns (`tools/profile-library.mjs`, see docs/tools.md), not from
inspection: the ranked adjacent-pair and adjacent-triple tables are the
selection criterion.

| op | insn | operand field | replaces |
|----|------|---------------|----------|
| 0x41 | StoreLPop | u8 local | `StoreL n; Pop` |
| 0x42 | StoreGPop | u16 global | `StoreG n; Pop` |
| 0x43 | LoadLL | u8 (8..16), u8 (16..24) | `LoadL a; LoadL b` |
| 0x44 | LoadLG | u8 (8..16), u16 (16..32) | `LoadL a; LoadG g` |
| 0x45 | LoadGL | u16 (8..24), u8 (24..32) | `LoadG g; LoadL a` |
| 0x46 | LoadLIdx | u8 local | `LoadL a; LoadIdx` |
| 0x47 | LoadGLIdx | u16 (8..24), u8 (24..32) | `LoadG g; LoadL a; LoadIdx` |
| 0x48 | ConstOp | u8 sub-opcode (+ 1 immediate word) | `Const c; <binop>` |
| 0x49 | LoadLConstOp | u8 local, u8 sub-opcode (+ 1 immediate word) | `LoadL a; Const c; <binop>` |
| 0x4A | LoadGConstOp | u16 global, u8 sub-opcode (+ 1 immediate word) | `LoadG g; Const c; <binop>` |
| 0x4B | CallBuiltinC | u16 builtin, u8 argc (+ 1 immediate word) | `Const c; CallBuiltin b, argc` |
| 0x4C | CallBuiltinCC | u16 builtin, u8 argc (+ 2 immediate words) | `Const c1; Const c2; CallBuiltin b, argc` |
| 0x4D | CmpJf | u8 sub-opcode (+ 1 word: target) | `<cmp>; JmpIfFalse t` |
| 0x4E | PopRetNull | — | `Pop; RetNull` |

`0x4F..0xFF` stay free.

**Sub-opcodes** are the base opcode bytes of the operation they stand for.
`ConstOp` / `LoadLConstOp` / `LoadGConstOp` accept the two-operand value
ops (`Add Sub Mul Div Rem Pow BitAnd BitOr BitXor Shl Shr Lt Le Gt Ge Eq
Ne`); `CmpJf` accepts only the comparisons (`Lt Le Gt Ge Eq Ne`). Anything
else is a decode error.

**`CmpJf` is the one instruction whose jump target is not in the operand
field** — the field is spoken for by the sub-opcode, so the target word
index is the FOLLOWING word. It is validated like any other target
(on an instruction boundary, or `== code_len` for "fall off the end").

Everything the decoder proves for a base opcode it proves for the fused
form: local and global slots (both of `LoadLL`'s), builtin ids against the
import table, argc caps, jump targets, reserved bits zero.

Two rules in the peephole keep fusion invisible to everything but the
dispatch count, and they are the reason the debugger and error reporting
do not change:

- **Never fuse across a jump target.** Any instruction a branch can land
  on stays addressable.
- **Never fuse across a source position.** Positions are set per statement,
  so a fused run always lies inside one statement, the position runs are
  identical, and a breakpoint or a runtime error still names the same line.

The one deliberate difference is FUEL: a fused instruction costs 1 unit
instead of 2 or 3, so the runaway-loop budget stretches slightly further.
## Decoding

Three entry points, one validator:

- `deserialize(&[u8])` — full decode with debug info; **copies** the word
  region into a `Vec<u32>` (`Words::Owned`). Hosts, the CLI, tests.
- `deserialize_lean(&[u8])` — no debug info; copies. Hosts without a
  mapping (wasm, a device slot read into a Vec).
- `deserialize_lean_static(&'static [u8])` — no debug info; **borrows** the
  word region (`Words::Static(&'static [u32])`) when it is 4-byte aligned in
  memory, copies otherwise (alignment never fails a decode). This is the
  device path over a memory-mapped flash slot: the resident `Program` is
  the header tables (fn/global/export names, pool table, assert messages)
  and nothing else. `Program` stays `Send + Sync` either way.
- `validate(&[u8])` — the same checks, no `Program`, near-zero allocation —
  what request handlers call on small-heap devices.

## Decoder validation

Rejected at decode time (`BcError::Malformed`): bad magic; section overrun /
trailing bytes; counts over the caps above; invalid UTF-8; a word region
that is not 4-aligned or does not end the blob; unknown opcode; reserved
operand bits set; a `Const Num` whose immediate would fall past its
function's code; `fn_idx ≥ n_fns` (in `Const Fun`, `CallFn`, exports); an
import whose name is unknown or resolves to a different id on this build; a
builtin operand not in the import table; global operand ≥ `n_globals`;
`pixel_count_g ≥ n_globals`; local operand ≥ `locals`; `params > locals`;
`argc > 16`; a jump target past `code_len` or not on an instruction
boundary; a function or pool range outside the word region; debug runs not
strictly ascending or past the code. Total blob size is capped at 256 KiB.
`BcError::Version` is reserved for a `version` field mismatch.

An accepted blob's word region is used verbatim and re-encodes
byte-identically (the corpus round-trip test asserts this). Every invariant
the in-place interpreter relies on is proven here.

## Debug info

`flags.debug` gates source positions (pc-keyed runs — statement granularity,
so a handful per function) and local names. Devices decode with the lean
entry points, which skip storing both: runtime errors report `fn`/`pc` but
`(line, col) = (0, 0)`, and debugger stack panes lose local names.
Everything else — by-name vars, controls, sensor bindings, exported
functions — works either way, since global and function names are always
present. Producers always emit debug info.

## History

- v1: instruction-indexed jumps, per-instruction positions; the VM
  materialized an instruction array at load.
- v2: byte-encoded instructions executed in place, byte-offset jumps,
  offset-keyed position runs.
- v3: const-array pool + `ConstArr`.
- v4: assert-message table + `Assert`.
- v5: fixed-width u32 words in one aligned region shared with the pool;
  word-index pcs; runtime builtin ids checked against the import table
  instead of rewritten; the borrowing decoder for memory-mapped stores.
- v5 (no version bump): superinstructions `0x41..0x4E`, appended. Old v5
  blobs keep running; producers that do not emit them stay valid.

## What LXBC is not

- Not an interchange format for pattern *sharing* — `.epe` (source) remains
  that. LXBC accompanies source; source is the durable artifact.
- Not stable across `version` bumps, deliberately. Devices reject stale
  blobs; hosts with a compiler recompile from the stored source.
