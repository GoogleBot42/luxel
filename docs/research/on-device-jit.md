# On-device JIT for pattern code (2026-09-20)

Jeremy's brief (2026-09-20): per-pixel compute is the bottleneck and native
pattern code is becoming necessary. Keep the interpreter for boards without
the flash for it; make the JIT a firmware feature; accept per-board-class
builds (PSRAM, flash size). Design intent: **the JIT lives on the device**,
so LXBC stays the one portable format between devices (leader/follower
included), and **builtins are ordinary function calls, everything else is
native** — no interpreter/native mode switching inside a running pattern.
Inspiration: HolyC (single pass, no IR, emit straight into memory, every
function native, the runtime is a call away).

The engineering design that follows from it is `docs/jit-design.md`. This document is the research behind the design position taken on the
Gitea ticket. Sources: the current tree (`cb7002f`), Gitea #260/#312/#354
history, esp-hal 1.1.0 (git `7c7f3726`) sources, ESP-IDF sources, and the
prior art listed at the end. `[C]` = verified against a primary source,
`[I]` = inference.

## 1. Facts that settle the premise

**The compiler is not on the device, and never was.** `firmware/Cargo.toml`
builds luxel-core with `default-features = false`, which drops the
`frontend` feature (lexer/parser/compiler, `crates/luxel-core/src/lib.rs`).
`POST /api/code` takes an LXP1 envelope (name + source + precompiled LXBC,
`firmware/src/server.rs` `api_code`); the source is stored and echoed, never
compiled. Even the built-in default pattern is compiled by `firmware/build.rs`
on the host. The browser's wasm build of luxel-core is the compiler. `[C]`

**LXBC v5 is architecture-independent.** One little-endian `u32` word
stream (opcode in bits 0..8, 24-bit operand field), executed in place from
the flash mapping, no pointers, no arch fields; 16.16 fixed point only
(`Fx(i32)`), so no FPU dependence; builtins referenced by runtime id with an
import table that pins name→id per build (`docs/spec/bytecode.md`). The same
blob runs in the browser (wasm), on the host (`luxel bench`) and on every
board. 45 base opcodes + 14 superinstructions. `[C]`

**Leader/follower already ships the pattern, and it ships LXBC.** The
`LXS2` beacon on UDP :4049 carries boot id, time, an FNV-1a hash of the
running pattern and optional sensors (21 bytes). A follower that sees a
different hash pulls `GET /api/pattern.lxp` from the leader over HTTP — the
LXP1 envelope, source + bytecode — "this device has no compiler"
(`firmware/src/netin.rs` `adopt_leader_pattern`). Playlist distribution is
the unbuilt half (#354). `[C]`

So Jeremy's model is right on every point: bytecode is the interchange
format, the device cannot compile source, and a native blob would have to
be derived per device. Nothing in the wire format, the store, the playlist
or the UI has to learn that native code exists.

**Values are fully dynamic.** `Value { Num(Fx), Arr(u32), Fun(u32),
Builtin(u32) }`, 8 bytes, u32 tag (chosen so the Xtensa `match` needs no
literal-pool mask). Locals and stack slots are untyped; only counts are
static. Non-numbers coerce to 0 in arithmetic. `[C]` This is the one
design point below that matters more than any codegen detail.

**Builtins are a `match`, not a table.** 187 entries in `BUILTINS`
(append-only), dispatched through three tiers (`builtin_fast` inlined in
the loop, `builtin_hot` ~2 KB, `builtin_cold`), all `match` arms; every
builtin takes `&mut Vm`; missing args read 0, extra args drop. `[C]` A JIT
needs a per-builtin entry point with a fixed ABI, which does not exist yet.

## 2. Where the cycles are today (why a 2× JIT is not worth building)

Measured on metal (docs/boards.md, docs/firmware.md, docs/bulk-render.md):

| term | cost | source |
|---|---|---|
| dispatch, per bytecode op | 83 cycles (S3), ~101 (classic ESP32) | `tools/opbench.mjs` |
| per-pixel entry floor (`render(index)` call, frame/locals reset, fuel reset) | ~400–440 cycles/px (1.85 µs/px on the panel) | docs/boards.md, bulk.rs |
| cost model | `≈ 400 + insns_per_px × ~110` cycles/px | docs/bulk-render.md |
| Pixelblaze v3, same loop microbench | ~90 cycles/op | #312 |
| builtin-heavy patterns vs PB | parity, 0.88–1.17× | `tools/oracle/fps-compare.mjs` |

The interpreter has already been squeezed (#260/#312/#314/#328: two-level
dispatch, in-place binops, fuel in locals, u32 tags, tiered builtins, IRAM
placement, O3 for luxel-core). Two structural costs remain that no
interpreter change removes: the ~400-cycle per-pixel entry, and ~80–100
cycles for what is one or two machine instructions of 16.16 arithmetic.
On the Xtensa parts a third term dominates real patterns: whether
`Vm::run` (~13 KB) and the builtin dispatchers stay resident in the flash
cache; a 1.3 KB code move was worth −46 % on a real pattern
(docs/firmware.md "I-cache residency"). JIT'd code in SRAM is immune to
that term by construction.

**What each approach buys, from published numbers** `[C]` unless marked:

| approach | over a switch interpreter | evidence |
|---|---|---|
| threaded / tail-call dispatch (no codegen) | 2–3× | Deegen +171 % over PUC Lua; Ertl & Gregg "almost 2×" |
| baseline JIT, **boxed values, helper calls for everything** | 1.4–2.5× | V8 Sparkplug +41–45 %; MicroPython `@native` 2.0–2.4× on ESP32; Espruino ~1.5× |
| baseline JIT, **unboxed typed values, arithmetic inlined** | 10–16× | MicroPython `@viper` 16× on the same ESP32 (487 → 30 ms) |
| copy-and-patch templates | 1× (CPython 3.13) to 10× (paper, vs a naive interpreter) | PEP 744; Xu & Kjolstad 2021 |
| host AOT (WAMR `wamrc`) | 0.68–0.94× of gcc -O3 | WAMR docs; no Xtensa/RV32 numbers exist |

The 2.4× vs 16× gap is the whole design. "Builtins are calls" is right;
"every VM operation is a call" is the 2× version. The JIT must inline the
16.16 arithmetic, comparisons, locals and stack traffic on raw `i32`s, and
only cross into Rust for builtins, arrays and anything non-numeric.

## 3. Design position

### 3.1 Typing: infer, unbox, bail

There is no free tag bit in a 16.16 word (wrapping semantics use all 32
bits), so the JIT cannot NaN-box. The alternative is a whole-program
abstract interpretation over the LXBC before emission: each local, stack
slot and global gets `Num | Arr | Fun | Builtin | Unknown`. In the PB
dialect arrays come only from `array(n)`, literals and a handful of
builtins, and are consumed by indexing and builtins; functions are called
directly (`CallFn`) except through the few higher-order builtins. Expect
nearly every slot on a render path to prove `Num`. `[I]` — measurable on
the host today, over the 299-pattern library, before any emitter exists.

- `Num` slots: raw `i32` in registers / the native frame; add/sub/compare
  are one instruction; mul is `mull`+`mulsh`+`src` (Xtensa) or
  `mul`+`mulh` (RV32IM); div is a helper (Fx::div has an i64 fallback,
  #316).
- `Arr`/`Fun`/`Builtin` slots: raw handle in a register; every use is a
  helper call (index load/store with bounds + budget checks, call-value).
- `Unknown` slots: boxed 8-byte `Value` in the native frame, every op a
  helper call with the exact interpreter semantics (coercion to 0, etc.).

A function whose hot slots are `Unknown` still compiles; it just runs at
the 2× tier. Coercion semantics are preserved exactly because the
inference is sound (a slot is `Num` only if every reaching definition is).

### 3.2 Granularity: whole program or nothing, decided at activation

Compile at pattern activation (`/api/code`, store activate, playlist
advance, follower adopt). Every pattern function becomes a native function;
pattern-to-pattern calls are direct native calls. If any function fails to
compile (unsupported construct, code-cache full, inference blow-up) the
whole program runs in the interpreter, and `/api/status` says so
(`jit: {state, reason, code_bytes, compile_us}`). No OSR, no deopt, no
mixed frames — exactly Jeremy's constraint, and it is what removes the
complexity every "real" JIT carries. Compile time for a few KB of bytecode
on a 240 MHz core is milliseconds `[I]`, so live-edit pushes recompile
every time; crossfade holds two programs, so the cache is sized for two.

The per-pixel loop stays native Rust in `engine.rs` (it already hoists the
invariants and picks the projection), calling the JIT'd `render` through a
fixed ABI with `index, x, y, z` as raw `i32` args. That collapses the
~400-cycle entry to a call. `beforeRender`, `renderFrame` and bulk ops work
the same way; higher-order builtins (`arrayForEach` etc.) get the callback
as a native fn pointer instead of re-entering `Vm::run`. Fuel becomes a
back-edge counter (decrement + branch at loops and calls, a few cycles
instead of 7 per op); errors return through an epilogue with the faulting
instruction index stored before each fallible helper call, so `vmerr` and
its source position keep working.

### 3.3 Builtin ABI

Replace the three-tier `match` with a table indexed by builtin id (the id
stays append-only, so the table is the same shape as `BUILTINS`), each
entry `{ fn: extern "C" fn(&mut Vm, *const i32, argc) -> i32, kind }` plus
direct specialised signatures for the numeric tier-1 set (`sin`, `cos`,
`abs`, `clamp`, … as `fn(i32) -> i32` / `fn(i32, i32) -> i32`, no Vm).
The interpreter can dispatch through the same table (a measured decision —
#328 found the `match` tiers matter for I-cache residency, and a table in
DROM costs one load; #312 test 2b showed a table load beats a compare
tree). The JIT emits `callx8` / `jalr` through the table entry's address
resolved at compile time, so generated code never embeds a symbol that an
OTA could move — and there is nothing to persist across builds.

### 3.4 Executable memory, per chip, in esp-hal terms

The firmware is bare-metal esp-hal + esp-rtos (no ESP-IDF), so IDF's
`MALLOC_CAP_EXEC` / `CONFIG_ESP_SYSTEM_MEMPROT` machinery does not apply;
the equivalents are linker regions and esp-hal's own protection setup.

| chip | executable RAM | how | status |
|---|---|---|---|
| ESP32 (Athom, PB v3) | `iram_seg` = SRAM0 0x40080400, 128 KB, instruction bus only (esp-hal `ld/esp32/memory.x`); `.rwtext` uses ~43 KB of a 130 KB region today (docs/boards.md IRAM table; `.rwtext.wifi` 55 KB must be checked against the map) | reserve a `.jitcode` region after `.rwtext`; write with 32-bit aligned stores at the same address; no cache maintenance (no L1 on Xtensa ESP32; `isync` only) | measure the free tail from the linker map |
| ESP32-S3 (Seengreat) | the whole internal SRAM is dual-mapped: `iram_seg` 0x40378000 ≡ `dram_seg` 0x3FC88000, offset 0x6F0000 (esp-hal `ld/esp32s3/memory.x`; IDF `SOC_I_D_OFFSET`) | allocate from the ordinary heap (byte-writable through the D alias), execute at `+0x6F0000`; PSRAM overflow via the D→I mirror `+0x0600_0000` after `Cache_WriteBack` + I-invalidate (Espressif `elf_loader` does exactly this on the S3) | the cheapest of all; `psram.rs` already holds a second heap there |
| ESP32-C3 | I/D aliases at offset 0x700000; esp-hal cannot set PMP ("the bootloader locks all available PMP entries", `esp-hal/src/soc/mod.rs`) | a dedicated linker region in the IRAM alias | moot until the OTA slot grows |
| ESP32-C6 | unified SRAM, one address; esp-hal locks PMP entries: `.rwtext` X-R, stack and `.data..noinit` -WR | a region outside those entries (M-mode allows what no entry matches) | moot until the OTA slot grows |

The classic ESP32's SRAM1 D/IRAM alias is **word-inverted**
(IDF `SOC_DIRAM_INVERTED`); the design above avoids SRAM1 entirely. `[C]`

Security posture: esp-hal already runs the Xtensa boards with no W^X, so
a code cache changes nothing there; on RISC-V the cache is one more RWX
PMP region of esp-hal's own choosing. Worth one line in docs/firmware.md,
not a policy decision. `[I]`

### 3.5 Backend order: Xtensa first, tested on QEMU

The performance need is the S3 panel and the classic-ESP32 strips; the
RISC-V boards are the ones that cannot afford the flash anyway (§3.6).
LX6 and LX7 share every ISA option that matters (`core-isa.h`: windowed,
density, `MUL32`/`MUL32_HIGH`/`DIV32`, `L32R`, FPU present on both, no
double). One Xtensa emitter covers the Athom, the PB v3 unit and the
Seengreat. The S3 additionally has PIE (128-bit integer SIMD) — a later
bulk-op lever for a fixed-point VM, unreachable from a float VM. `[C]`

The QEMU harness (`tools/qemu/`, `-machine esp32`) runs the shipped image
unmodified, so the JIT is testable with no hardware: a differential test
that renders the library through the interpreter and through the JIT and
compares pixel buffers is the correctness gate; the devshell's
`xtensa-esp-elf-objdump` checks the encoder's bytes against mnemonics on
the host. Cycle counts from QEMU mean nothing; correctness does.

Xtensa specifics a single-pass emitter must plan for `[C]` (LLVM Xtensa
backend, `core-isa.h`):

- Windowed ABI: generated functions open with `entry a1, N` (N a multiple
  of 8, ≤ 32760) and close with `retw.n`; callees are reached with
  `callx8` with args in `a10…a15`, result in `a10`. Window overflow costs
  ~0.005 %; not a concern.
- `L32R` reaches only backwards (≈256 KB, 4-aligned, no LITBASE on these
  cores): emit the literal pool *before* the code in the same buffer.
  `MOVI` covers ±2047.
- Conditional branches reach ±128 B (`BEQZ`/`BNEZ` ±2 KB); `J` ±128 KB.
  Single-pass shape: emit `b<cond-inverted> +6; j target` for every
  forward branch, patch on label bind. 6 bytes per branch, no islands.
- `LOOP` bodies are ≤ 256 B; skip the zero-overhead loop initially.
- 16.16 mul = `mull` + `mulsh` + `src` (funnel shift); `quos`/`rems` for
  32-bit division where Fx semantics allow, helper otherwise.

No Rust Xtensa encoder exists (crates.io has only HAL/PAC/rt crates;
cranelift has no Xtensa, dynasm-rs no Xtensa). MicroPython's
`py/asmxtensa.{c,h}` (MIT, ~870 lines) is the model for a hand-written
one; the two people who have done this (MicroPython's port, ESPB) both say
the window ABI and literal pools are where the time went. `[C]`

RV32 later: `asmkit-rs` (MIT/Apache, `no_std`, RV32 + C) and
`pixelspark/rv32jit` (MIT, an ESP32-C3 Rust JIT harness) exist; Linux's
eBPF RV32 JIT is 1.4 k lines over a 1.3 k-line encoder. C3/C6 have no FPU
and no vector unit; 16.16 keeps them fast.

### 3.6 Size and the board tiers

Precedents: ESPB's Xtensa+RV32 JIT costs ~56 KB flash and 11–12 KB RAM;
MicroPython's per-arch encoder is ~900–1,500 lines over a ~3,100-line
shared emitter. A Rust Xtensa backend + inference at `opt-level = "s"`
lands somewhere in 25–45 KB `[I]`. Against the OTA slot today
(docs/boards.md, latest tables):

| board | margin | JIT fits? |
|---|---:|---|
| seengreat-hub75 / s3-devkit | ~170 KB (17 %) | yes, now |
| pixelblaze-v3 | 46 KB (4.4 %) | no — needs #199 repartition |
| athom-music (2-output) | 21 KB (2.0 %) | no — needs #199 |
| c3-devkit | 78 KB (7.5 %) | not on the RISC-V backend's timeline |
| c6-devkit (+hosted-ui) | 14 B over the floor after #598 | no |

So Jeremy's "different builds for different board classes" is forced, not
optional. Proposed tiers as Cargo feature bundles selected in
`board-target.sh` next to `IRAM`/`CORE_O3`:

- `tier-lite`: C3/C6 as today (interpreter, hosted-ui where needed).
- `tier-std`: classic ESP32 after #199 — JIT into the `iram_seg` tail.
- `tier-psram`: S3 with PSRAM — JIT into the dual-mapped heap, PSRAM
  overflow, PSRAM array arena (#253), room for PIE bulk ops later.

The JIT feature must be a pure addition: the interpreter stays the
semantics reference and the fallback in the same image, so a tier-std or
tier-psram device never loses a pattern it could run before.

### 3.7 What not to do

- **Host-side AOT (what #260 item 3 sketched).** A native blob is bound to
  a firmware *build* (builtin addresses, `Vm` layout, ABI), not just an
  ISA: it needs an import table, per-build version pinning, a signing
  story, a per-target matrix in the envelope, and it breaks the
  store/sync/UI invariance above. And the host has no Xtensa backend to
  lean on (cranelift: none; LLVM Xtensa is Espressif's fork), so the same
  hand-written emitter gets written either way — on-device is the *less*
  work option, not the more.
- **Threaded/closure dispatch as a step.** 2–3× on paper, but it leaves
  the entry floor, the boxing and the flash-cache term untouched, and it
  is Rust-hostile (no computed goto; tail calls are not guaranteed). Not a
  stepping stone to the JIT — the type inference and builtin table are.
- **A boxed-value JIT.** MicroPython `@native`'s 2.4× is the ceiling of
  that shape. Do the inference first or do not start.
- **Persisting native blobs in the store.** Tempting (patlog has a `ver`
  byte and two reserved bytes to extend on), but compile-at-activation is
  milliseconds and a persisted blob needs a build id the firmware does not
  have (`CARGO_PKG_VERSION` only). Revisit only if compile time shows up.

## 4. Expected outcome, honestly

With the cost model `400 + 110·n` cycles/px replaced by roughly
`call + 2–4·n_arith + builtin_calls × (call overhead + the builtin's own
work)` `[I]`:

- arithmetic-heavy per-pixel patterns (the snake-2d shape, wave math,
  anything with loops in `render`): 5–15×.
- rainbow-class (`hsv` + a few ops): ~4×, then capped by the panel's 115
  Hz rescan; the `hsv` builtin itself is unchanged.
- builtin-bound patterns (perlin/fbm-heavy): 1.3–2× — the builtin *is* the
  work, and it is already native. Those want the S3 PIE bulk path or the
  #265 two-core pixel split, which stacks with the JIT.

The unknown that decides everything is the `Unknown`-slot rate of the
inference over the library, and that is a host-only experiment.

## 5. Prior art consulted

- MicroPython `@native`/`@viper`, emitters for xtensawin and rv32, ESP32
  port's `esp_native_code_commit` (assemble in DRAM, copy to exec memory).
- ESPB (AGPL — technique only, never link): Xtensa+RV32 JIT on
  ESP32/C3/C6, 56 KB flash.
- xcc700/rcc700 (MIT): self-hosted C compilers emitting LX7 / RV32 on the
  S3, loaded through Espressif's `elf_loader`.
- WAMR: no JIT on any Espressif chip (fast-jit is x86-64 only); AOT is
  host-compiled. wasm3: interpreter, 4–15× slower than native.
- Espruino JIT: ARM Thumb only. LuaJIT: no Xtensa, RV64 only.
- Pixelblaze: Ben Hencke has said on the forum he has prototyped PB →
  native Xtensa and that "results are promising", and that the ISA
  question (Xtensa vs RISC-V vs ARM) is what held him back — the same
  question the on-device design answers.
- Deegen, Ertl & Gregg, Berndl et al. for dispatch-technique numbers;
  V8 Sparkplug and PEP 744 for baseline/copy-and-patch numbers.
- HolyC: the shape (single pass, direct emission, native everywhere,
  runtime as calls). Two things it did not have to solve: a windowed ABI
  with literal pools and branch-reach limits, and a dynamically typed
  input — HolyC is statically typed, which is why §3.1 is the part of
  this design HolyC cannot inform.
