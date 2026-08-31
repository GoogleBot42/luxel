# Firmware size report — where the ~1 MB goes

*2026-08-30, against master + the #168 fmt diet, on `board-c6-devkit` — the
tightest board in the fleet (RISC-V, opt-level "s", fat LTO, codegen-units 1,
**credless** flake build, which is what CI measures and reads ~1.5 KB smaller
than a devshell build with WiFi creds baked in). Regenerate:*

```sh
nix build .#luxel-fw-c6-devkit
python3 tools/size-report.py result/luxel-fw.elf   # symbol buckets — see caveat below
readelf -SW result/luxel-fw.elf                    # sections
espflash save-image --chip esp32c6 result/luxel-fw.elf /tmp/ota.bin  # image total
```

*Per-board image sizes and slot margins live in **docs/boards.md** ("The 1 MiB
OTA-slot ceiling") — that table is the source of truth and is re-measured every
release; this document does not duplicate it.*

## Headline

The `board-c6-devkit` app image is **1,001,472 B — 47,104 B (4.49 %) under the
1 MiB OTA slot**, the smallest margin in the fleet by a wide gap (next tightest
is `board-pixelblaze-v3` at 8.81 %). CI fails a release below 3 % margin and
warns below 6 % (`tools/image-check.sh`, Gitea #160), so the C6 currently sits
in the warn band.

The shape of the image has not changed since the 2026-07-07 edition of this
report: **roughly a third of it is Espressif's closed-source radio stack**,
which every ESP32 WiFi app carries, and the rest splits between the network
plumbing, the Luxel VM/compiler, and our feature code. What changed is the
chip. The C6 is ~90 KB fatter than the C3 for byte-identical Rust — profiling
both RISC-V boards (Gitea #160) showed the entire gap is blob: ~51 KB more blob
symbols, ~11 KB more `.rodata.wifi`, and `.rwtext.wifi` going 33,768 → 55,060 B.
`luxel-core` is byte-identical at 76,628 B on both chips. **There is no
C6-specific diet to write; every win is fleet-wide.**

## Where the bytes go (board-c6-devkit)

Sections (`readelf -SW`, flash-resident only; 1,000,586 B, the rest of the
image is header and segment padding):

| section | bytes | |
|---|---:|---|
| `.text` | 788,586 | all code |
| `.rodata` | 95,616 | |
| `.rwtext.wifi` | 55,060 | blob code that must run from RAM |
| `.rodata.wifi` | 40,696 | |
| `.data` | 12,068 | |
| `.rwtext` | 6,160 | |
| `.trap` | 1,920 | |
| `.data.wifi` | 480 | |

Symbol-level buckets (911,877 B attributable; the rest is padding, alignment,
and locals with no ELF size):

| bucket | bytes | KB | notes |
|---|---:|---:|---|
| Espressif WiFi/PHY blobs (C) | 282,229 | 275.6 | closed-source: 802.11 MAC (incl. 11ax/TWT), PHY cal, WPA2/WPA3 crypto, NVS glue. Irreducible while WiFi is on. |
| luxel-fw (our code) | 156,436 | 152.8 | biggest symbols: main task 36.0 KB, the picoserve route-table future 22.1 KB, MQTT session 16.4 KB, render task 13.0 KB, web task 11.5 KB. |
| other C/asm | 132,911 | 129.8 | **not one thing** — ~73 KB is more blob whose names miss the prefix list (AES `Te0`/`Td0` tables, `rijndael*`, `he_*`/`itwt_*` 802.11ax handlers, radio cal), ~49 KB is Rust anonymous rodata (`.Lanon*`: string literals, tables, vtables from every crate), ~5 KB the C `printf`/`_ftoa` family, ~5 KB switch tables. |
| luxel-core (VM/compiler) | 76,850 | 75.0 | the product: lexer→parser→compiler→VM (`call_builtin` alone is 22.4 KB — 130+ builtins), engine, fixed-point math, noise. |
| rust core | 58,815 | 57.4 | `core::fmt` 22.9 KB (see the diet note below), future/pin poll glue, str/slice ops, sort, panic paths. |
| embassy_executor | 43,932 | 42.9 | misattributed label: these are OUR task state machines (`TaskStorage<…>::poll` monomorphizations). |
| picoserve | 32,948 | 32.2 | HTTP/1.1 + WebSocket server — 22 `IntoResponse::write_to` instantiations are 20.5 KB of it. |
| smoltcp | 21,430 | 20.9 | TCP/UDP/DHCP/DNS/IGMP. |
| esp_hal | 16,766 | 16.4 | SPI, GPIO, clocks, efuse. |
| esp_radio | 13,552 | 13.2 | the Rust side of the WiFi driver. |
| sequential_storage | 12,010 | 11.7 | the pattern/config flash store. |
| rust_mqtt | 11,450 | 11.2 | MQTT v5 client (our session logic is inside luxel-fw). |
| esp_rtos | 9,078 | 8.9 | |
| rust alloc | 7,552 | 7.4 | |
| everything else | 35,918 | 35.1 | embassy-net, esp-bootloader, embassy-sync, edge-dhcp, OTA, esp-storage, … |

**Caveat on `tools/size-report.py`.** The script reads `nm --size-sort`, which
*estimates* a size for symbols that carry none in the ELF — and this RISC-V
image is full of them: linker-script symbols (`_rwtext_len`, a NOTYPE symbol
whose *value* is a length, gets an estimated "size" of 1,082,130,432 B),
RISC-V mapping symbols (`$d`), and `.L*` locals. They all land in "other
C/asm" and blow that row up to ~1 GB. Everything else in the script's output
is unaffected. The table above was produced by bucketing `nm -C -S
--defined-only` instead — real `st_size`, zero-size symbols dropped — which
reproduces every other bucket exactly. Fixing the script is a small change
nobody has made yet; until then, ignore its "other C/asm" row.

## Growth history

**`board-pixelblaze-v3`, 2026-07 (kept for archaeology):**

| version | bytes | change |
|---|---:|---|
| v0.1.17 @ opt-level 3 | 1,051,936 | crossed the 1 MiB OTA slot |
| v0.1.17 @ opt-level "s" | 874,624 | −177 KB from the opt switch |
| v0.1.18 (+DDP/E1.31: udp+multicast+2 tasks) | 887,520 | +13 KB |
| v0.1.19 (+MQTT+HA, +boot guard) | 930,448 | +43 KB |
| v0.1.19 + Fx Display fix (below) | 916,080 | −14 KB |
| v0.1.22 (+sensors, sync, AP mode) | ~956 KB | +40 KB, headroom down to 92 KB |

**`board-c6-devkit`, since the board was added 2026-08-22** — this is the
board that governs now. Devshell builds with creds baked in:

| revision | image | slot margin |
|---|---:|---:|
| v0.1.39, board added (2026-08-22) | 987,600 | 60,976 B (5.8 %) |
| + PB-parity noise rework | 993,648 | 54,928 B |
| + seengreat-hub75 board plumbing | 994,352 | 54,224 B |
| fleet re-measure 2026-08-29 | 997,344 | 51,232 B (4.9 %) |
| + map-aware blur/glow (#140) | 999,600 | 48,976 B (4.7 %) |
| + device output palette (#139) | 1,004,704 | 43,872 B (4.2 %) |

Credless flake builds (what CI measures, ~1.5 KB under the above):

| revision | image | slot margin |
|---|---:|---:|
| #160 profiling baseline (2026-08-30) | 1,003,824 | 44,752 B (4.27 %) |
| #168 merge base | 1,004,112 | 44,464 B (4.24 %) |
| **after the fmt diet (#168), today** | **1,001,472** | **47,104 B (4.49 %)** |

Features still cost what you'd expect — two medium features (#140, #139) took
~7 KB of C6 headroom in two days — and there is still no single mystery lump.
The baseline is dominated by the radio stack; the *margin* is dominated by
which chip you are on.

## What we already fixed (measured)

**`impl Display for Fx` went through `to_f64()`** (2026-07-07) — so printing
any fixed-point value (diagnostics, playlist JSON) dragged in core's full
f64→decimal machinery (grisu + dragon fallback + pow10 tables). Replaced with
a ~25-line exact 16.16 decimal printer (16.16 is exactly representable in ≤16
fractional digits; pinned by test): **−14,368 bytes**, and
`float_to_decimal*`/`flt2dec` vanished from the symbol table entirely. That
printer is now `Fx::dec_str`, shared with `Display` so the two cannot diverge.

**JSON response builders off `core::fmt`** (2026-08-30, Gitea #168) — every
`format!`-built response body in `server.rs`, `playlist.rs`, `patterns.rs`,
`resume.rs`, `devicemap.rs`, `mqtt.rs` and luxel-core's `jsonview.rs` now
builds with `push_str` and non-fmt number printers. Result: **−2,640 B**
(`.text` −2,344, `.rodata` −1,368). Honest accounting, because the estimate
that motivated it was wrong by 3×:

- `core::fmt` **does not leave the image**. `println!` and `Debug` keep it
  linked no matter what the JSON builders do; the fmt bucket only moved
  23,550 → 22,922 B (162 → 159 symbols). The 5–10 KB estimate assumed a
  bucket that was never reclaimable.
- The naive conversion **grew** the image by 8,592 B before it shrank it:
  `String::push_str` inlines a reserve-and-copy at every call site, and the
  builders have hundreds. What turned it around was funnelling every literal
  append through one `#[inline(never)] jsonview::push_piece`.
- The real saving is the *arguments*, not the machinery: each `format!` site
  built an `Arguments` array of `ArgumentV1`s in rodata plus a call-site
  formatting shim, and those do go away.

**Lesson for the rest of the list:** a "crate X is N KB, remove its callers"
estimate is worthless unless the crate actually becomes dead. Before adopting
a diet item, check whether *anything else* keeps the code linked, and measure
the whole image after the change — bucket deltas lie (the #168 build moved
~14 KB between the `embassy_executor` and `luxel-fw` buckets purely by
shifting an LTO inlining boundary, at a net image cost of nothing).

Also worth noting as prior art: factoring three NVS writers onto one
`config::write_record` helper paid back ~600 B (#139). Small, boring
de-duplication is currently the highest-yield-per-risk work in the firmware.

## What we deliberately keep

- **panic/fmt strings** — `-Zbuild-std-features=panic_immediate_abort` would
  likely save 15–30 KB but destroys panic messages, and esp-backtrace's
  readable panics have paid for themselves several times over (the
  stack-overflow incidents). Not worth it. This is also *why* `core::fmt`
  can never be dieted away (above).
- **WebSocket handshake SHA-1** — RFC 6455 requires it; it comes from
  `const-sha1` inside picoserve and is small enough to be inlined (no
  standalone symbol). The `SHA1*` symbols visible in the image belong to the
  WPA blob, not to us.
- **opt-level "z"** is not available: esp-storage's build script hard-rejects
  it (flash-op code must stay out of the lowest opt tier). "s" is the floor.

**No longer true, kept so nobody re-plans around it:** earlier editions listed
**f64 *parsing*** (`dec2flt` + `POWER_OF_FIVE_128`, ~14 KB) as a deliberate,
load-bearing keep for JS-literal parity. It is **not linked at all** —
`nm -C result/luxel-fw.elf | grep -iE 'dec2flt|POWER_OF_FIVE|flt2dec'` returns
nothing. Either the lexer stopped going through core's path or LTO proves it
dead. There is no 14 KB f64 bucket to reclaim, and the JS-parity reasoning no
longer describes what the code does. Likewise the old "`lhash` SHA-1 (8 KB)"
entry: that crate is gone from the tree.

## If we ever need more room (ranked, unimplemented)

Margins live in docs/boards.md; the short version is that the C6 has ~47 KB
and CI red-lights a release under ~31 KB.

1. **picoserve monomorphization collapse** (Gitea #167 — ~45 KB of surface,
   target ≥10 KB recovered): `picoserve::routing` symbols are 24,848 B
   (22,050 B of that is the single `Api::call_path_router_service` closure),
   and there are **22 distinct** `IntoResponse::write_to` instantiations
   totalling 20,478 B — one per response *tuple shape*
   (`(StatusCode, H1, C)`, `(H1, H2, H3, C)`, …), the top six alone ~13 KB of
   near-identical code. Funnel every handler through one response type.
   Mechanical and wide (touches every handler signature), not a behaviour
   change. This is the biggest remaining concrete win.
2. ~~**fmt trimming**~~ — **DONE 2026-08-30 (#168), −2,640 B.** See above; the
   estimate was 5–10 KB and the structural reason it could not be is recorded
   there.
3. **MQTT behind a cargo feature** (~30 KB: `rust_mqtt` 11.5 KB + our session
   and HA-discovery code): only for someone who wants a non-MQTT build. It is
   a product feature, so it stays default-on — this is a C6-profile lever, not
   a fleet-wide one.
4. **Repartition** (last resort, serial reflash): the `assets` partition is
   0xF0000 and the web app uses a fraction of it; slots could go to 1.25 MB.
   Invasive, and it cannot be delivered by OTA — documented in docs/boards.md.

## Verdict

~1 MB is the honest cost of "ESP32-C6 + WiFi + HTTP/WS server + MQTT + a full
language VM": ~355 KB of radio blob you can't touch (275 KB of prefixed blob
symbols plus ~73 KB more hiding in "other C/asm"), ~130 KB network plumbing
and executor, ~75 KB VM (the product), ~157 KB feature code, ~66 KB
core/alloc runtime, ~49 KB of anonymous rodata spread across all of it. Both
pieces of genuine accidental fat found so far (f64 printing, per-site fmt
argument construction) are fixed, and the second one returned a third of its
estimate — assume the next item does too until measured. Watch
`espflash save-image` against the 1 MiB slot on every feature; `tools/image-check.sh`
now enforces it in CI, but the C6 is the board that decides.
