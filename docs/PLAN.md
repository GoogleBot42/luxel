# Pixler — Plan

A FOSS live-codable LED controller: Pixel Blaze's core idea — write LED patterns in a
JS-like scripting language in a web IDE, see them running instantly on real hardware —
rebuilt as open source from the publicly documented language and protocols.

Research backing this plan: [docs/research/](research/) (product inventory, language
internals, FOSS landscape).

## 1. Vision & positioning

**One sentence:** WLED-class approachability, Pixel Blaze-class expressiveness, fully open.

The gap is confirmed real (research doc 03): WLED deliberately refuses user scripting;
ARTI-FX is dead; MoonLight/ESPLiveScript is C-syntax, Xtensa-native-codegen, and
organizationally unstable; everything else is PC-side or a sequencer. Nobody has an
open on-device pattern VM + live web IDE + pixel-mapped coordinate model.

**Compatibility stance (a core decision):**
- **Source-compatible with the Pixel Blaze pattern language.** The language is publicly
  documented by the vendor (README.expressions.md); implementing it clean-room from docs
  is legally clean. Payoff: 200+ community patterns (`.epe` files at
  patterns.electromage.com) run on day one, and every PB tutorial applies to us.
- **NOT bytecode-compatible.** PB bytecode is undocumented, platform-specific, and their
  compiler is closed. We define our own bytecode + our own open compiler. `.epe` import
  recompiles from `sources.main`.
- **Adopt PB's open peripheral protocols verbatim**: sensor board (`SB1.0` serial framing)
  and output expander (`UPXL` UART framing) are MIT-licensed with published specs — their
  existing hardware just works with us.
- **PB WebSocket-protocol compatibility is a later, optional layer** (M5) — it would make
  pixelblaze-client, Firestorm, PixelTeleporter, and Home Assistant integrations work
  unmodified, but our primary API should be our own clean one.

**Non-goals (v1):**
- No reverse engineering of PB firmware binaries — everything from public docs/specs.
- No sequenced-show authoring (xLights/FPP territory) — instead we speak DDP/E1.31 as an
  *input* so those tools can drive us.
- No ESP8266 support. No cloud dependency for any core function.
- Not a WLED replacement for people who just want canned effects — though shipping a good
  stock pattern library narrows that gap.

## 2. Architecture

The defining move: **one portable core library, three hosts.**

```
                    ┌─────────────────────────────────────────┐
                    │  libpixler (portable C, no OS deps)     │
                    │  lexer → parser → compiler → bytecode   │
                    │  16.16 fixed-point VM + builtins        │
                    │  (sin/wave/hsv/perlin/arrays/transforms)│
                    └───────┬───────────┬───────────┬─────────┘
                            │           │           │
              ┌─────────────┴──┐  ┌─────┴─────┐  ┌──┴──────────────┐
              │ ESP32 firmware │  │ WASM build│  │ Native CLI/sim  │
              │ (ESP-IDF, GPL) │  │ (web IDE  │  │ (tests, fuzzing,│
              │ WiFi, HTTP/WS, │  │ instant   │  │ golden vectors, │
              │ LED drivers,   │  │ preview,  │  │ headless render)│
              │ storage, sync  │  │ no HW req)│  │                 │
              └────────────────┘  └───────────┘  └─────────────────┘
```

Contrast with Pixel Blaze, where the compiler exists only as closed browser JS and the
device only runs bytecode. Our compiler is a portable library, so:
- **The device can compile source itself** → headless API clients upload plain source; the
  device is fully functional without any browser.
- **The browser runs the exact same VM via WASM** → pixel-perfect instant preview while
  typing, before/without touching the device, and a zero-hardware playground for docs and
  pattern sharing.
- **CI runs the exact same VM natively** → differential testing, fuzzing, benchmarks.

### 2.1 libpixler (the core)

- **Language:** C11, zero dependencies, no malloc surprises (caller-provided arena;
  matches PB's model where arrays are never freed — an arena makes that natural).
  Rationale over Rust: first-class ESP-IDF integration, trivial WASM via clang, and the
  widest embedded-contributor base. The VM is small enough that C's safety cost is
  containable and offset by fuzzing in CI.
- **Numeric model:** 16.16 two's-complement fixed point, PB-semantics-exact where patterns
  can observe it (wrap on overflow, bitwise ops on the full word, `~` zeroing low 16 bits,
  `%` truncated vs `mod()` floored, hue/phase wrapping, `time()` period = 65.536·interval).
  We do NOT copy the 31-bit-literal quirk — our literals carry full 32-bit precision.
  Fixed point keeps FPU-less chips (ESP32-C3/C6) first-class and makes rendering
  bit-identical across device, browser, and CI.
- **Compiler:** hand-written lexer + Pratt parser → single-pass-ish bytecode emitter with
  a source map (for editor error highlighting). Compile target speed: <50 ms for a
  typical pattern on ESP32 (patterns are hundreds of lines at most).
- **VM:** stack machine, computed-goto dispatch (fallback switch for MSVC/WASM). Limits
  mirroring PB as defaults but configurable: 256 globals, 256 stack slots, 10K array
  elements. Runtime errors never crash the engine — set `vmerr` + PC, blank the pattern,
  report to editor.
- **Builtins:** the full documented PB API (research doc 02 §2): waveforms, math, arrays
  (fn + method forms), color (hsv/rgb/palettes/paint), perlin family, transforms
  (4×4 matrix stack, ≤31 deep), map introspection, controls plumbing, sequencer hooks,
  sensor bindings, GPIO hooks (host-provided function table so libpixler stays portable).
- **Frame pipeline:** `beforeRender(delta)` → render fn per pixel (selection priority per
  map dimension, incl. `render(index, x)` second arg) → host writes LEDs.
- **Conformance suite:** golden test vectors (source + inputs → exact pixel outputs),
  cross-checked against a real Pixel Blaze where practical, plus the jvyduna/pb-examples
  corpus and scraped community patterns compiled + smoke-run in CI.

### 2.2 Firmware (ESP-IDF)

- **Framework:** ESP-IDF 5.x directly (not Arduino) — control over RMT/I2S/LCD peripherals,
  partitions, OTA. Targets in order: **ESP32-S3, ESP32 classic, ESP32-C3** (fixed-point VM
  makes C3 viable). P4 later.
- **LED output:** start with ESP-IDF RMT (`led_strip`-style, DMA) for WS281x + hardware SPI
  for APA102/SK9822; design an output-driver interface so I2S/LCD parallel drivers
  (FastLED MIT internals as reference) slot in later. Also implement the **UPXL output
  expander protocol** (UART 2 Mbps) — instant 8–64-channel fan-out using ElectroMage's own
  open boards or clones.
- **Networking:** AP-mode captive-portal first-run setup → client mode; mDNS
  (`pixler.local`); HTTP server serving the gzipped IDE from flash; WebSocket for control +
  preview stream; UDP time-sync beacons (adopt PB's port-1889 type-42/43 framing — it's
  simple, published, and Firestorm/pixelblaze-client already speak it); DDP + E1.31 input
  as a "just be a dumb fixture" mode; OTA via upload + (later) signed release channel.
- **Storage:** LittleFS. Patterns stored as **plain source + metadata JSON + preview** (no
  bytecode caching in v1 — compile-on-load at <50 ms is fine and removes a cache-invalidation
  axis). Import/export `.epe` natively. Full-filesystem backup/restore as JSON (PBB-like).
- **Concurrency:** render loop pinned to one core; network/WiFi on the other; pixel
  hand-off via double buffer. Brightness limiter + configurable max-current model.

### 2.3 Web IDE

- **Stack:** Svelte + Vite + CodeMirror 6, built to a single gzipped bundle small enough
  for device flash (budget: ≤512 KB gz; PB proves feasibility). No external CDN — must
  work fully offline on an AP-mode device.
- **Features (parity list):** live compile-on-keystroke with inline errors; hot-swap to
  device over WS while LEDs keep running; **WASM-VM local preview** (1D strip + 2D/3D
  scatter view) alongside the device's true previewFrame stream; Var Watcher; auto-generated
  controls UI (slider/pickers/toggle/trigger/inputNumber/showNumber/gauge); pattern list
  with previews; mapper tab (JS map function evaluated in browser sandbox → normalized
  binary map, Fill/Contain); playlist editor; settings; WiFi setup.
- **Also ships as a hosted/desktop page**: the same IDE with only the WASM VM = a
  zero-hardware pattern playground (great onboarding + docs embeds).

### 2.4 Control API

- **v1 API (ours):** JSON over WebSocket + REST for files. Same functional surface as PB's
  (config, pattern CRUD, active pattern, vars get/set, controls get/set, playlists,
  brightness, preview subscription, stats @1 Hz incl. fps/vmerr/mem/storage) but with
  consistent naming, request IDs, and versioning from day one.
- **M5 PB-compat shim:** WS on :81 + PB JSON verbs + binary frame types 4/5/6/7/8 + UDP
  1889 → pixelblaze-client / Firestorm / PixelTeleporter work against Pixler. (Type-3
  bytecode upload accepted only from our own compiler format; PB-native bytecode is
  explicitly out of scope.)

## 3. Requirements

### Functional (v1.0)
1. Full PB pattern-language source compatibility per research doc 02 (§1–2), including
   arrays, lambdas-without-closures, dispatch tables, exports, UI-control conventions,
   sensor bindings, transforms, palettes, perlin.
2. ≥95% of the public pattern corpus compiles and renders plausibly; conformance suite
   green (bit-exact on curated golden vectors).
3. WS281x + APA102-class output; 1D/2D/3D pixel maps; playlists + shuffle; brightness
   limit; persistent per-pattern control state.
4. Web IDE served from device with live coding, preview, var watch, mapper, controls.
5. First-run WiFi onboarding without any app or cloud; everything works in AP mode.
6. `.epe` import/export; device backup/restore.
7. Sensor-board serial input (`SB1.0`) and output-expander output (`UPXL`).

### Non-functional
- **Performance:** ≥30 FPS at 1,000 WS2812 pixels on ESP32-S3 with a mid-weight pattern
  (blinkfade-class); VM throughput target ≥40K px/s average on stock patterns on classic
  ESP32 (PB v3 does ~48K — get within striking distance, then optimize).
- **Determinism:** identical pixel output for identical (pattern, inputs, delta sequence)
  across device/WASM/native builds.
- **Robustness:** pattern runtime errors and OOM never crash the device or the render
  loop; watchdog-safe; brownout-safe storage writes.
- **Footprint:** firmware + IDE fits a 4 MB flash part with OTA A/B (i.e. app ≤ ~1.5 MB +
  IDE ≤ ~512 KB in FS); VM RAM per pattern ≤ 64 KB.
- **DX:** clone → flash on a bare devkit in <10 minutes with `idf.py`; core library builds
  and tests on plain `cmake` on any desktop.

## 4. Licensing

- **libpixler + web IDE + CLI + language spec: Apache-2.0** (patent grant; lets editors,
  simulators, other firmwares, even commercial tools adopt the language — this is how the
  ecosystem outgrows a single device, and mirrors how PB's open docs seeded its community).
- **Firmware: GPL-3.0-or-later** — the project exists because a closed firmware bothered
  us; copyleft ensures device vendors shipping Pixler-based products contribute back.
  Also keeps the door open to borrowing from WLED (EUPL-1.2 → GPL-compatible) and
  MoonLight (GPL-3.0). Use FastLED (MIT) internals as driver reference rather than
  NeoPixelBus (LGPL static-link awkwardness) where we vendor code.
- Trademark hygiene: "Pixel Blaze" is ElectroMage's mark — describe as "compatible with
  the Pixel Blaze pattern language," never in the project name.

## 5. Milestones

**M0 — Language core (pure software, no hardware).**
libpixler: lexer/parser/compiler/VM + full builtin set; native CLI (`pixler run
pattern.js --pixels 150 --frames 300 → PPM/JSON`); conformance + fuzz harness; compile
the scraped `.epe` corpus. *Exit: ≥90% of corpus compiles; golden vectors locked.*

**M1 — Browser playground.**
WASM build + minimal IDE (CodeMirror, strip/2D preview, controls, var watcher) as a static
page. Zero hardware needed — first shareable artifact, first community hook.
*Exit: live-edit rainbow/blinkfade/xorcery in browser at 60 FPS for 1K virtual pixels.*

**M2 — First light (firmware alpha).**
ESP-IDF app: WiFi onboarding, LittleFS, HTTP+WS, serve the IDE, on-device compile,
RMT WS281x output, live coding end-to-end, preview stream, stats. Single hardcoded-config
strip. *Exit: type in browser → LEDs change, over an evening-long soak without a crash.*

**M3 — Feature parity core.**
APA102/SPI; settings UI (led type/count/order/speed, brightness limit); pattern
CRUD + `.epe` import/export; mapper (1D/2D/3D) + render selection + transforms; controls
persistence; playlists/shuffle; auto-off scheduling; backup/restore; OTA.
*Exit: a PB user can migrate a real 2D-mapped project without touching pattern code.*

**M4 — Ecosystem hardware & inputs.**
Sensor board protocol + sound-reactive builtins; output expander protocol; GPIO/analog/touch
builtins; DDP/E1.31 input mode; clock functions via SNTP + timezone.
*Exit: PB sensor board plugged into a Pixler device drives the classic sound patterns.*

**M5 — Multi-device & compat.**
UDP time-sync (port 1889 framing); leader/follower sync groups (pattern launch, timebase,
brightness, live-edit broadcast, `nodeId()`); PB WebSocket-compat shim validated against
pixelblaze-client's test suite and Firestorm.
*Exit: two devices render one synced pattern; `pixelblaze-client` controls a Pixler
device unmodified.*

**Later:** parallel output drivers (I2S/LCD 8–16 lanes), ESP32-P4, HDR/16-bit pipelines,
palettes-from-images, pattern-library federation (self-hostable pattern site), RP2350
expander firmware.

## 6. Risks & mitigations

| Risk | Mitigation |
|---|---|
| VM too slow (the make-or-break) | Benchmark against PB's published numbers from M0 week 1; computed-goto dispatch; superinstructions for hot pairs (`hsv` args, array index); fixed-point avoids soft-float cliffs; profile-guided opcode design. PB proves the ceiling is fine on identical silicon. |
| Language edge-case mismatch breaks community patterns | Conformance-first development: encode every documented gotcha (research doc 02 §4) as a test before implementing; differential-test odd corners against real PB hardware (~$30 buys the oracle). |
| Scope creep — PB is 8 years of polish | Milestones are strictly serial; M1 (browser-only) delivers standalone value even if firmware slips. |
| IDE bundle > flash budget | Svelte (no runtime), code-split mapper/playlist views, Brotli, measure in CI. PB fits theirs in ~1.4 MB-class budgets. |
| WS2812 timing vs WiFi jitter | RMT/DMA drivers (solved problem — WLED/FastLED); render core isolation. |
| Solo-project stall (the MoonModules failure mode) | Portable core + browser playground maximize contributor surface without hardware; conformance suite makes drive-by PRs safe. |
| Legal/trademark friction | Clean-room from published docs only; no binary RE; no "Pixelblaze" in name; document provenance of every spec detail (done — research docs). |

## 7. Open questions (decide by end of M0)

1. Bytecode design: register vs stack VM — prototype both on the blinkfade benchmark
   before locking (stack is simpler; register VMs often win 20–40% on dispatch-bound
   interpreters).
2. Where non-PB extensions live (e.g. `render(index, x)` is already PB 3.x; do we add
   Pixler-only builtins behind a `pixler.*` namespace or freeze at strict parity for v1?).
   Leaning: strict parity for v1, extensions gated in the compiler by a pattern pragma.
3. Preview transport: keep PB's raw-RGB type-5 stream or delta-encode? (Raw first; it's
   3 KB/frame at 1K pixels — fine on LAN.)
4. Hosted pattern sharing: static-site + git-backed registry vs dynamic service. (Static
   first.)
