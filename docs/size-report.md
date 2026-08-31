# Firmware size report — where the ~1 MB goes

*2026-08-30, against master + the #167 picoserve response collapse, on
`board-c6-devkit` — the tightest board in the fleet (RISC-V, opt-level "s",
fat LTO, codegen-units 1, **credless** flake build, which is what CI measures
and reads ~1.5 KB smaller than a devshell build with WiFi creds baked in).
Regenerate:*

```sh
nix build .#luxel-fw-c6-devkit
python3 tools/size-report.py result/luxel-fw.elf   # symbol buckets
readelf -SW result/luxel-fw.elf                    # sections
espflash save-image --chip esp32c6 result/luxel-fw.elf /tmp/ota.bin  # image total
```

*Per-board image sizes and slot margins live in **docs/boards.md** ("The 1 MiB
OTA-slot ceiling") — that table is the source of truth and is re-measured every
release; this document does not duplicate it.*

## Headline

The `board-c6-devkit` app image is **980,784 B — 67,792 B (6.46 %) under the
1 MiB OTA slot**, still the smallest margin in the fleet by a wide gap (next
tightest is `board-pixelblaze-v3` at 10.87 %). CI fails a release below 3 %
margin and warns below 6 % (`tools/image-check.sh`, Gitea #160); the C6 spent
August inside that warn band and the #167 collapse (below) is what pulled it
back out.

*The 1,001,472 B / 4.49 % this section used to quote was already stale when it
was written: the easing-builtins batch landed on top of it and the real
pre-#167 baseline, re-measured on the same credless flake build, was
**1,005,168 B — 43,408 B (4.14 %)**. Both numbers are in the history table
below so the deltas reconcile.*

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

Sections (`readelf -SW`, flash-resident only; 980,406 B, the rest of the
image is header and segment padding):

| section | bytes | (pre-#167) | |
|---|---:|---:|---|
| `.text` | 768,174 | 791,818 | all code |
| `.rodata` | 95,832 | 96,584 | |
| `.rwtext.wifi` | 55,060 | 55,060 | blob code that must run from RAM |
| `.rodata.wifi` | 40,696 | 40,696 | |
| `.data` | 12,084 | 12,084 | |
| `.rwtext` | 6,160 | 6,160 | |
| `.trap` | 1,920 | 1,920 | |
| `.data.wifi` | 480 | 480 | |

The whole −24,384 B of #167 is `.text` (−23,644) plus a little `.rodata`
(−752); nothing else moved a byte.

Symbol-level buckets from `tools/size-report.py` (891,924 B attributable; the
rest is padding, alignment, and symbols carrying no ELF `st_size` — the script
prints how many it skipped, so the accounted total is a lower bound, not a
reconciliation of the image):

| bucket | bytes | KB | notes |
|---|---:|---:|---|
| Espressif WiFi/PHY blobs (C) | 282,229 | 275.6 | closed-source: 802.11 MAC (incl. 11ax/TWT), PHY cal, WPA2/WPA3 crypto, NVS glue. Irreducible while WiFi is on. |
| luxel-fw (our code) | 166,434 | 162.5 | biggest symbols: main task 36.0 KB, the flat route dispatcher 26.2 KB, MQTT session 16.4 KB, render task 13.0 KB, web task 11.5 KB, the one `Reply::write_to` 6.6 KB. **Up 10.0 KB from #167** and that is the whole point: the response code that used to be 22 picoserve instantiations is now inlined into our dispatcher once. Read this row together with the `picoserve` one. |
| other C/asm | 133,370 | 130.2 | **not one thing** — ~73 KB is more blob whose names miss the prefix list (AES `Te0`/`Td0` tables, `rijndael*`, `he_*`/`itwt_*` 802.11ax handlers, radio cal), ~49 KB is Rust anonymous rodata (`.Lanon*`: string literals, tables, vtables from every crate), ~5 KB the C `printf`/`_ftoa` family, ~5 KB switch tables. |
| luxel-core (VM/compiler) | 80,082 | 78.2 | the product: lexer→parser→compiler→VM (`call_builtin` alone is 25.0 KB — 150+ builtins after the easing batches), engine, fixed-point math, noise. |
| rust core | 50,843 | 49.7 | future/pin poll glue, str/slice ops, sort, `core::fmt`, panic paths. **Down 8.0 KB from #167**: every distinct header-value type used to drag in its own `Display`/`ForEachHeader` formatting shim. |
| embassy_executor | 43,932 | 42.9 | misattributed label: these are OUR task state machines (`TaskStorage<…>::poll` monomorphizations). |
| smoltcp | 21,430 | 20.9 | TCP/UDP/DHCP/DNS/IGMP. |
| esp_hal | 16,766 | 16.4 | SPI, GPIO, clocks, efuse. |
| esp_radio | 13,552 | 13.2 | the Rust side of the WiFi driver. |
| sequential_storage | 12,010 | 11.7 | the pattern/config flash store. |
| rust_mqtt | 11,450 | 11.2 | MQTT v5 client (our session logic is inside luxel-fw). |
| esp_rtos | 9,078 | 8.9 | |
| rust alloc | 7,552 | 7.4 | |
| picoserve | 7,278 | 7.1 | HTTP/1.1 + WebSocket server. **Was 32,948 B / 48 syms before #167** — one `write_to` and one `HeadersChain` instead of 22 and 37. |
| everything else | 35,918 | 35.1 | embassy-net, esp-bootloader, embassy-sync, edge-dhcp, OTA, esp-storage, … |

**Methodology note.** The script buckets `nm -C -S --defined-only` — real ELF
`st_size`, symbols without one dropped. It used to read `nm --size-sort`,
which *estimates* a size for size-less symbols from the next symbol's
address, and RISC-V images are full of them: linker-script symbols
(`_rwtext_len`, a NOTYPE symbol whose *value* is a length, got an estimated
"size" of 1,082,130,432 B), `$d` mapping symbols, `.L*` locals. They all
landed in "other C/asm" and blew that row up to ~1 GB. Fixed 2026-08-30
(Gitea #174); don't reintroduce `--size-sort`.

**There is a ~±0.7 KB noise floor, and it is not about your code.** The flake
source hash feeds rustc's `.Lanon.<hash>` local-symbol names, and renaming
those reshuffles `.L_MergedGlobals` packing and occasionally re-codegens a
function. **Editing only markdown in this repo moves the C6 image ~600 B** —
observed twice, in both directions, while writing these #167 entries; the
symbol diff was `.text` ±646 B of pure `.Lanon`/`.L_MergedGlobals` churn plus
one 486 B function, with every bucket of interest byte-identical. Builds are
reproducible for identical whole-repo content, but a headline number is only
meaningful against the revision it was measured with. So: A/B with everything
except the change held constant (that is how the #167 and #168 numbers here
were taken), and never read a sub-1 KB delta as signal.

The Xtensa boards never showed the ~1 GB blow-up only because their
`_rwtext_len` estimate landed above the script's old `>= 0x80000000` drop
guard, but their numbers were guesses too: hand-written asm and ESP32 ROM
stubs (`_WindowOverflow*`, `save_context`, `mktime`, `atoi`, `idle_hook_fn`,
`g_wifi_osi_funcs`, …) carry no `.size`, so the old output over-counted a
`board-athom-music` build by ~27 KB (99,097 → 74,949 B in "other C/asm",
215,207 → 212,003 B in blobs). Every Rust-crate bucket is byte-identical
before and after on both architectures. Those asm bytes are real but
unattributable, which is the reason the accounted total is a lower bound.

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
| after the fmt diet (#168) | 1,001,472 | 47,104 B (4.49 %) |
| + easing builtins batch 7 (#167 merge base) | 1,005,168 | 43,408 B (4.14 %) |
| **after the picoserve collapse (#167), today** | **980,784** | **67,792 B (6.46 %)** |

Features still cost what you'd expect — two medium features (#140, #139) took
~7 KB of C6 headroom in two days — and there is still no single mystery lump.
The baseline is dominated by the radio stack; the *margin* is dominated by
which chip you are on.

## What we already fixed (measured)

**Every HTTP response was its own monomorphization** (2026-08-30, Gitea #167)
— **−24,384 B on `board-c6-devkit`** (1,005,168 → 980,784), −23.0 to −24.4 KB
on every other board. The largest single win the firmware has taken since the
opt-level switch, and the one diet item that beat its estimate (≥10 KB) rather
than missing it.

`server.rs` returned thirteen different response *shapes* — `(CORS, JSON,
String)`, `(gzip, etag, cc, FlashAsset)`, `(StatusCode, [(&str,&str); 4],
&str)`, … — and picoserve monomorphizes `IntoResponse::write_to` per tuple
shape, `ForEachHeader::call` per header-*value* type, and
`Response`/`HeadersChain`/`ContentBody` per combination. The image carried 22
`write_to` instantiations, 37 header-machinery symbols, and a `Display` shim
per value type. All of it is now one concrete `Reply { status, headers:
heapless::Vec<(&'static str, HVal), 4>, body: ApiBody }`, where `HVal` is the
single header-value enum and `ApiBody` the single `Content` enum:

| symbol group (C6, `nm -C -S --defined-only`) | before | after |
|---|---:|---:|
| `IntoResponse::write_to` (incl. drop glue) | 20,478 B / 22 syms | 6,882 B / 3 syms |
| …of which real closures | 19,534 B / 11 | 6,556 B / **1** |
| `HeadersIter`/`HeadersChain`/`ContentHeaders`/`ForEachHeader` | 8,492 B / 37 | 966 B / **4** |
| `Api::call_path_router_service` closure | 22,050 B | 26,204 B |
| `picoserve` bucket | 32,948 B / 48 | 7,278 B / 25 |
| `rust core` bucket | 58,815 B | 50,843 B |

Three things are worth carrying forward:

- **The dispatcher grew 4.2 KB and that is fine.** With one response type
  the LTO boundary moves: the write path inlines into
  `call_path_router_service` instead of standing alone 22 times. Bucket
  deltas lie; only the image total counts.
- **`core::fmt` really does shrink here** (unlike #168, where it could not):
  the 8 KB off `rust core` is per-value-type `Display`/`ForEachHeader`
  formatting shims that had no other caller, so they actually became dead.
  `HVal::fmt` is a single `write_str` — no `Arguments` build, per #168.
- **The routing shape is load-bearing.** The win depends on there being
  exactly one `W` writer type in the image, which is why `server.rs` keeps
  its hand-written flat-match `PathRouterService`: picoserve's `MethodRouter`
  wraps the writer in a private `IgnoreBody<W>` for HEAD, a second `W` that
  would duplicate every GET instantiation straight back.

RAM cost, measured with `tools/stack-check.sh` on `board-pixelblaze-v3`:
`.stack` 29,124 → 27,828 B (the single `write_to` future is the union of all
body types, so the per-slot static task arena grew ~430 B × 3 slots). Still
3,828 B above the 24 KB floor, and the dispatcher's own poll frame went
2,128 → 4,928 B against a 12,288 B budget — the flat-match discipline holds,
but this is now the frame to watch when adding routes.

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

Margins live in docs/boards.md; the short version is that the C6 has ~66 KB
and CI red-lights a release under ~31 KB.

1. ~~**picoserve monomorphization collapse**~~ — **DONE 2026-08-30 (#167),
   −24,384 B.** See "What we already fixed" above for the measurements and
   for the two invariants (single writer type, single header-value type) that
   keep the win from silently unwinding.
2. ~~**fmt trimming**~~ — **DONE 2026-08-30 (#168), −2,640 B.** See above; the
   estimate was 5–10 KB and the structural reason it could not be is recorded
   there.
3. ~~**serve the UI from flash**~~ — **DONE 2026-08-30 (#11), −13.9 to
   −14.6 KB** as the `hosted-ui` cargo feature: no LUXA reader, no streaming
   asset body, no ETag/304 machinery, no `POST /api/assets`, and the ~641 KB
   bundle never gets flashed. Same shape as the MQTT item below — an opt-in
   profile for a tight board, not a fleet default, because the on-device UI
   is the product's "works with no internet" story. Numbers per board and the
   whole-mode description are in docs/boards.md.
4. **MQTT behind a cargo feature** (~30 KB: `rust_mqtt` 11.5 KB + our session
   and HA-discovery code): only for someone who wants a non-MQTT build. It is
   a product feature, so it stays default-on — this is a C6-profile lever, not
   a fleet-wide one.
5. **Repartition** (last resort, serial reflash): the `assets` partition is
   0xF0000 and the web app uses a fraction of it; slots could go to 1.25 MB.
   Invasive, and it cannot be delivered by OTA — documented in docs/boards.md.
   A `hosted-ui` build (item 3) frees that whole partition on paper, which is
   what would make this worth doing; the table fork it needs is Gitea #199.

## Verdict

~960 KB is the honest cost of "ESP32-C6 + WiFi + HTTP/WS server + MQTT + a
full language VM": ~355 KB of radio blob you can't touch (275 KB of prefixed
blob symbols plus ~73 KB more hiding in "other C/asm"), ~105 KB network
plumbing and executor, ~78 KB VM (the product), ~163 KB feature code, ~58 KB
core/alloc runtime, ~49 KB of anonymous rodata spread across all of it. Three
pieces of genuine accidental fat have been found and fixed (f64 printing,
per-site fmt argument construction, per-shape response monomorphization), and
their estimates came in at 1×, ⅓×, and 2.4× respectively — the estimate is
never the number, so measure the whole image before and after. Watch
`espflash save-image` against the 1 MiB slot on every feature;
`tools/image-check.sh` now enforces it in CI, but the C6 is the board that
decides.
