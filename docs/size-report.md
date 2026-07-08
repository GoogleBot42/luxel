# Firmware size report — where the ~900 KB goes

*2026-07-07, against v0.1.19 + boot-guard (`board-pixelblaze-v3`, opt-level
"s", fat LTO, codegen-units 1). Regenerate anytime:
`python3 tools/size-report.py firmware/target/xtensa-esp32-none-elf/release/luxel-fw`
(symbol buckets) plus `espflash save-image … | grep "part. size"` (image
total) and `readelf -S` (sections).*

## Headline

The image is **916 KB** (was 930 KB before the fix in this report's wake —
see "what we already fixed"). That is not abnormal for an ESP32 WiFi
firmware: **roughly a third of it is Espressif's closed-source radio
stack**, which every ESP32 WiFi app carries, and the rest splits between
the network plumbing, the Luxel VM/compiler, and our feature code. For
scale: WLED and the stock Pixelblaze app images are well north of 1 MB on
the same chip.

## Where the bytes go

Sections (`readelf -S`, flash-resident only): `.text` 692 KB, `.rodata`
109 KB, `.rodata.wifi` 30 KB, `.rwtext(.wifi)` 63 KB, `.data` 17 KB.

Symbol-level buckets (746 KB attributable; the rest is padding, headers,
and anonymous locals):

| bucket | KB | notes |
|---|---:|---|
| Espressif WiFi/PHY blobs (C) | 211 | closed-source: 802.11 MAC, PHY cal, WPA2 crypto, NVS glue. Irreducible while WiFi is on. |
| other C/asm | 96 | mostly more blob (printf/MD5/AES tables, ROM glue), libgcc soft-float, esp-rtos idle hook. Effectively part of the row above. |
| luxel-core (VM/compiler) | 83 | the product: lexer→parser→compiler→VM (`call_builtin` alone is 14.5 KB — 130+ builtins), engine, fixed-point math, noise. |
| luxel-fw (our code) | 82 | biggest single symbols: the picoserve route-table future (25.6 KB — every /api route's handler state machine in one generic), main task (22 KB), MQTT session (11.7 KB). |
| rust core | 50 | fmt machinery (~19 KB), f64 *parsing* (~14 KB — deliberate, see below), str/slice ops, panic paths. |
| embassy_executor | 48 | misattributed label: these are OUR task state machines (`TaskStorage<…>::poll` monomorphizations) — net_task 19.9 KB (smoltcp poll loop), web_task 12.2 KB, render_task 9.3 KB. |
| picoserve | 30 | HTTP/1.1 + WebSocket server. |
| smoltcp | 21 | TCP/UDP/DHCP/DNS/IGMP. |
| esp_hal | 16 | SPI, GPIO, clocks, efuse. |
| rust_mqtt + session deps | ~24 | the v0.1.19 addition (MQTT v5 client + our session logic). |
| everything else | ~50 | alloc, sequential-storage (pattern flash store), esp-rtos, lhash (ws-handshake SHA-1), OTA, esp-storage, … |

## Growth history (App/part. size)

| version | bytes | change |
|---|---:|---|
| v0.1.17 @ opt-level 3 | 1,051,936 | crossed the 1 MiB OTA slot |
| v0.1.17 @ opt-level "s" | 874,624 | −177 KB from the opt switch |
| v0.1.18 (+DDP/E1.31: udp+multicast+2 tasks) | 887,520 | +13 KB |
| v0.1.19 (+MQTT+HA, +boot guard) | 930,448 | +43 KB |
| v0.1.19 + Fx Display fix (below) | **916,080** | −14 KB |

So features cost what you'd expect (DDP 13 KB, MQTT ~43 KB); there was no
single mystery lump — the baseline was always dominated by the radio stack.

## What we already fixed (measured)

**`impl Display for Fx` went through `to_f64()`** — so printing any
fixed-point value (diagnostics, playlist JSON) dragged in core's full
f64→decimal machinery (grisu + dragon fallback + pow10 tables). Replaced
with a ~25-line exact 16.16 decimal printer (16.16 is exactly representable
in ≤16 fractional digits; pinned by test): **−14,368 bytes**, and
`float_to_decimal*`/`flt2dec` vanish from the symbol table entirely.

## What we deliberately keep

- **f64 *parsing* (~14 KB: `dec2flt` + `POWER_OF_FIVE_128` table)** — the
  lexer parses numeric literals through core's correctly-rounded f64 path
  because the reference language is JavaScript; hand-rolling this risks
  silent literal-rounding divergence from PB patterns. Load-bearing.
- **`lhash` SHA-1 (8 KB)** — the WebSocket handshake (RFC 6455) requires it.
- **panic/fmt strings** — `-Zbuild-std-features=panic_immediate_abort`
  would likely save 15–30 KB but destroys panic messages, and
  esp-backtrace's readable panics have paid for themselves several times
  over (the stack-overflow incidents). Not worth it.
- **opt-level "z"** is not available: esp-storage's build script hard-rejects
  it (flash-op code must stay out of the lowest opt tier). "s" is the floor.

## If we ever need more room (ranked, unimplemented)

Current headroom: **132 KB** to the 1 MiB OTA slot *(update 2026-07-08:
v0.1.22 — MQTT + sensors + sync + AP-mode — is at 956 KB, so headroom is
down to **92 KB**; item 1 below is getting close to "soon")*. If a future
feature (e.g. on-device FFT + audio) threatens it:

1. **picoserve monomorphization** (~10–15 KB est.): the single 25.6 KB
   route-table symbol exists because every route's response type is a
   distinct generic instantiation. Boxing response bodies or splitting the
   router flattens this. Medium effort, touches every handler signature.
2. **fmt trimming** (~5–10 KB est.): `format!` chains in JSON builders each
   instantiate fmt machinery; building strings with `push_str` (as
   patterns.rs already does for big responses) is smaller and faster.
3. **MQTT behind a cargo feature** (~24 KB): if someone wants a non-MQTT
   build. Product feature, so default-on.
4. **Repartition** (last resort, serial reflash): the `assets` partition is
   0xF0000 and the web app currently uses a fraction of it; slots could go
   to 1.25 MB. Invasive — documented in the OTA-slot-ceiling notes.

## Verdict

~900 KB is the honest cost of "ESP32 + WiFi + HTTP/WS server + MQTT +
mDNS-less discovery stack + a full language VM": ~310 KB radio blob you
can't touch, ~120 KB network plumbing, ~83 KB VM (the product), ~82 KB
feature code, ~50 KB core-library runtime. The one piece of genuine
accidental fat found (f64 printing) is fixed. Watch `espflash save-image`
against the 1 MiB slot on every feature — the boot guard now makes a
too-clever image recoverable, but the slot ceiling still rejects it.
