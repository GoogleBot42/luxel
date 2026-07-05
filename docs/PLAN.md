# Pixler — Plan (v2)

A FOSS live-codable LED controller: write LED patterns in a scripting language in a web
IDE, see them running instantly on real hardware. Inspired by Pixel Blaze (closed
firmware), built clean-room from publicly documented behavior — but **not** a drop-in
clone: same feature class, better integration, no self-imposed ceiling at PB's feature
set.

Research backing this plan: [docs/research/](research/). v1 of this plan (C core,
PB-protocol drop-in compatibility) is preserved in git history (`8181d61`).

## 1. Vision & positioning

**One sentence:** WLED-class approachability, Pixel Blaze-class expressiveness, fully
open, designed to integrate — with home automation, with other Pixler devices, with
whatever comes next.

The gap is confirmed real (research doc 03): WLED deliberately refuses user scripting;
ARTI-FX is dead; MoonLight/ESPLiveScript is C-syntax, native-codegen, organizationally
unstable. Nobody has an open on-device pattern VM + live web IDE + pixel-mapped
coordinate model.

### Compatibility stance
- **Pattern-source compatibility is the keystone and the only hard PB-compat goal.**
  The PB pattern language is publicly documented; implementing it from docs is clean.
  Payoff: 200+ community `.epe` patterns run on day one and every PB tutorial applies.
  `.epe` import (recompile from `sources.main`) ships in core.
- **The language is a frontend, not the system.** Our bytecode VM and builtin ABI are a
  documented, versioned compilation target in their own right. The PB-compatible language
  is frontend #1; other languages targeting the same bytecode are an explicit design goal
  (see §2.1). We are not restricted to PB's features or syntax long-term.
- **Wire-protocol / tooling compatibility with PB is a non-goal.** No Firestorm, no
  pixelblaze-client shim, no PB websocket emulation on the roadmap. We define our own
  clean API. (Nothing prevents a community-contributed compat shim later; the internal
  surfaces should not preclude it, but we spend zero design budget on it.)
- **Peripheral compatibility is optional, generic, and late.** Pixler defines its own
  peripheral abstraction (§2.5); PB's open sensor-board and output-expander protocols
  become just two drivers behind it — nice to have because the hardware exists and the
  specs are MIT, but Pixler is not locked to PB hardware in either direction. Peripherals
  land in M5, after the core is solid. Until then (and whenever a peripheral is absent),
  patterns referencing peripheral bindings get **defined stub values (zeros) plus an
  editor/API warning** ("this pattern uses `frequencyData`; no audio source installed").
- **Reference hardware, not custom hardware.** Any generic ESP32-class devkit is the
  target. We have a real Pixel Blaze available and will use it strictly as a **black-box
  behavioral oracle** (drive it via its public websocket API, compare pixel output for
  language edge cases). No firmware dumping, no binary reverse engineering — public
  documents and observable behavior only.

### Integration goals (what "better PB" means here)
- **Home automation first-class:** MQTT client with **Home Assistant MQTT discovery**
  (device appears in HA automatically: light entity, pattern select, brightness, exported
  pattern vars as entities). This one protocol also covers Node-RED, openHAB, etc.
  ZeroMQ and similar are better served by a small host-side MQTT bridge than by linking
  them into firmware; keep the firmware surface MQTT + HTTP/WS + UDP.
- **Pixler-to-Pixler:** discovery, shared timebase, and leader/follower sync (pattern
  launch, brightness, live-edit broadcast, `nodeId()`-style per-device divergence) over
  our own simple UDP/JSON protocols.
- **Show-tool citizenship:** DDP / E1.31 (sACN) *input* mode so xLights/FPP/LedFx can
  drive a Pixler as a dumb fixture.
- **Everything scriptable:** the full device API is plain JSON over WS/HTTP — no cloud,
  no app, works in AP mode.

### Non-goals (v1)
- Drop-in PB replacement (protocols, Firestorm, pixelblaze-client).
- Sequenced-show authoring (xLights territory — we're a fixture for them instead).
- Reverse engineering of PB firmware binaries.
- ESP8266. Cloud dependencies of any kind.

## 2. Architecture

The defining move stands: **one portable core, three hosts** — now in Rust.

```
              ┌──────────────────────────────────────────────┐
              │  core crate (Rust, #![no_std] + alloc)       │
              │  frontend: lexer → parser → bytecode emitter │
              │  documented bytecode + builtin ABI (stable)  │
              │  16.16 fixed-point VM + builtins             │
              └────────┬───────────────┬────────────┬────────┘
                       │               │            │
        ┌──────────────┴───┐   ┌───────┴──────┐  ┌──┴────────────────┐
        │ firmware crate   │   │ wasm32 build │  │ native CLI + test │
        │ esp-hal + embassy│   │ Svelte/TS IDE│  │ harness (conform- │
        │ (GPL-3.0)        │   │ live preview │  │ ance, fuzz, bench,│
        │ WiFi, HTTP/WS,   │   │ zero-hardware│  │ diff-vs-real-PB)  │
        │ MQTT, LED out,   │   │ playground   │  │                   │
        │ storage, sync    │   └──────────────┘  └───────────────────┘
        └──────────────────┘
```

### 2.1 Core crate (`pixler-core`)

- **Language:** Rust, `#![no_std]` + `alloc`, zero-`unsafe` goal outside the dispatch
  loop. Builds for Xtensa/RISC-V (device), `wasm32-unknown-unknown` (IDE), and native
  (CLI/CI). Rust's dual win here: memory safety in a network-facing interpreter, and
  first-class WASM.
- **Numeric model:** 16.16 two's-complement fixed point (`i32` newtype), PB-semantics-
  exact where patterns can observe it: wrap on overflow, bitwise ops on the full word,
  `~` zeroing low 16 bits, `%` truncated vs `mod()` floored, hue/phase wrapping,
  `time()` period = 65.536·interval. We do NOT copy PB's 31-bit-literal quirk. Fixed
  point keeps FPU-less chips (ESP32-C3/C6) first-class and makes rendering bit-identical
  across device, browser, and CI.
- **Compiler frontend:** lexer via the **`logos`** derive crate (no_std-compatible, fast,
  battle-tested — no reason to hand-roll token recognition); parser hand-written
  (Pratt/recursive-descent) because the grammar is small and hand-written parsers give
  the best error recovery + source spans for editor squiggles. (Parser-generator crates
  like `lalrpop`/`chumsky` evaluated and rejected: error-message control and no_std
  ergonomics matter more than grammar brevity here.)
- **Bytecode + builtin ABI as a public contract:** documented instruction set, versioned;
  exports table (name → address) and builtin table (numbered, semver'd) specified in
  `docs/spec/`. This is what makes alternate language frontends possible without forking
  the VM — a frontend is anything that emits this format. Compile-on-device target:
  <50 ms typical pattern.
- **VM:** interpreter with arena allocation matching the PB model (arrays never freed →
  arena is the natural fit). Limits PB-shaped but configurable: 256 globals, 256 stack
  slots, 10K array elements. Runtime errors never take down the engine — set
  `vmerr` + PC, blank output, report to editor. Stack vs register design: prototype both
  on the blinkfade benchmark in M0 before locking (open question §7).
- **Builtins:** the full documented PB API (research doc 02 §2). Host services (LED
  write, GPIO, sensors, clock, random) enter via a host trait object, keeping the crate
  portable; peripheral-backed builtins read stub values when the host reports the
  peripheral absent (§1).
- **Conformance suite:** golden vectors (source + inputs → exact pixel output) encoded
  from every documented gotcha *before* implementation; the jvyduna/pb-examples corpus +
  scraped community patterns compiled and smoke-run in CI; a differential harness that
  runs the same pattern on our VM and on the real PB (websocket-driven, previewFrame
  compared) for edge-case arbitration.

### 2.2 Firmware crate

- **Basis: `esp-hal`** (Espressif's official no_std Rust HAL, 1.x stable) + **embassy**
  async executor + `esp-wifi`/`smoltcp` networking; HTTP/WS via `picoserve`
  (embassy-native); mDNS (`pixler.local`); storage on `littlefs2` or
  `sequential-storage` over `esp-storage`; MQTT via a no_std client (`rust-mqtt`/
  `minimq` — evaluate in M4).
- **Targets in order:** ESP32-S3, ESP32 classic, ESP32-C3 (fixed-point VM keeps C3
  viable). Note: classic/S3 are Xtensa → `espup` toolchain; C3 is RISC-V on mainline
  Rust. P4 later.
- **LED output:** RMT driver (esp-hal `Rmt` + smartled adapter path) for WS281x; SPI+DMA
  for APA102/SK9822. Output-driver trait from day one so parallel drivers (I2S/LCD_CAM)
  and the expander driver slot in later without touching the render loop.
- **Contingency, explicitly scoped:** if a blocker surfaces in the no_std stack (most
  likely candidates: OTA robustness, WiFi provisioning corner cases, TLS for MQTT), the
  fallback is **std Rust on ESP-IDF** (`esp-idf-svc`) — same language, same core crate,
  swap the host layer. Decision checkpoint at M2 exit; we do not fall back to C.
- **Concurrency:** render loop on its own core/task with double-buffered pixel hand-off;
  network on the other. Brightness limiter + configurable max-current model.
- **Storage model:** patterns stored as plain source + metadata JSON + preview (compile
  on load; no bytecode cache in v1). `.epe` import/export. Whole-FS backup/restore JSON.
- **OTA:** A/B partitions, upload via web UI first; signed release channel later.

### 2.3 Web IDE

- **Stack: Svelte + TypeScript + Vite + CodeMirror 6** (strict TS, no JS source files),
  single gzipped bundle ≤512 KB served from device flash; fully offline-capable (AP
  mode, no CDN).
- **Features:** live compile-on-keystroke with inline errors (WASM core does the
  compiling — identical errors to device); hot-swap over WS while LEDs keep running;
  **WASM-VM local preview** (1D strip + 2D/3D views) alongside the device's true
  preview stream; var watcher; auto-generated controls UI (slider/pickers/toggle/
  trigger/inputNumber/showNumber/gauge); pattern browser; mapper tab (sandboxed JS/TS
  map function → normalized binary map, Fill/Contain); playlist editor; settings; WiFi
  onboarding; peripheral status (installed/missing, with the stub-warning surfacing
  here).
- **Also ships hosted/static:** the same IDE with only the WASM VM = zero-hardware
  playground for onboarding, docs embeds, and pattern sharing.

### 2.4 Control & integration API

- **JSON over WebSocket + REST** for files/config: versioned, request-IDs, consistent
  naming. Functional surface: config, pattern CRUD, active pattern, vars get/set,
  controls get/set, playlists, brightness, preview subscription, stats (~1 Hz: fps,
  vmerr, mem, storage).
- **MQTT + Home Assistant discovery** (M4): availability, light entity, pattern select,
  brightness, exported vars as sensors/numbers; command topics mirror the WS API.
- **UDP:** device discovery beacon + timebase sync (our own minimal framing — we adopt
  the *idea* of PB's NTP-ish exchange, not its wire format), and the sync-group
  protocol.
- **DDP / E1.31 input** mode (M4).

### 2.5 Peripheral framework (M5)

- A `Peripheral` trait: identity, capability descriptors (e.g. `audio.fft32`,
  `imu.accel3`, `env.light`, `analog[n]`), a poll/stream interface feeding named
  bindings into the VM host table, and health/presence reporting.
- Capabilities — not board identities — are what patterns bind to; the PB sensor board
  driver (SB1.0 serial framing) is simply the first provider of `audio.fft32 + imu +
  light + analog[5]`. An I2S MEMS mic + on-device FFT is an obvious second audio
  provider (removes the PB-hardware dependency for sound-reactivity). Output expander
  (UPXL) is likewise one provider behind the output-driver trait.
- Missing peripheral ⇒ bindings resolve to documented defaults (zeros) + warning
  surfaced in IDE/API; patterns must not crash.

## 3. Requirements

### Functional (v1.0)
1. Full PB pattern-language source compatibility per research doc 02 (§1–2): arrays,
   lambdas-without-closures, dispatch tables, exports, UI-control conventions,
   transforms, palettes, perlin, sensor *bindings* (stubbed until M5).
2. ≥95% of the public `.epe` corpus compiles and renders plausibly; conformance suite
   green (bit-exact on golden vectors).
3. WS281x + APA102-class output; 1D/2D/3D pixel maps; playlists + shuffle; brightness
   limit; persistent per-pattern control state.
4. Web IDE served from device: live coding, dual preview (local WASM + device stream),
   var watch, mapper, controls, TS/Svelte.
5. First-run WiFi onboarding, no app/cloud; fully functional in AP mode.
6. `.epe` import/export; device backup/restore; OTA.
7. MQTT + HA discovery; DDP/E1.31 input; multi-Pixler sync.

### Non-functional
- **Performance:** ≥30 FPS at 1,000 WS2812 px on ESP32-S3 with a blinkfade-class
  pattern; ≥40K px/s average over the stock corpus on classic ESP32 (PB v3: ~48K).
- **Determinism:** identical pixel output for identical (pattern, inputs, deltas) across
  device / WASM / native.
- **Robustness:** pattern errors and OOM never crash device or render loop;
  watchdog-safe; brownout-safe writes.
- **Footprint:** fits 4 MB flash with OTA A/B (app ≤ ~1.5 MB + IDE ≤ 512 KB gz); VM RAM
  per pattern ≤ 64 KB.
- **DX:** `cargo test` on any desktop for the core; devkit flash in <10 min with
  documented `espup`/`espflash` steps; IDE dev loop is plain `npm run dev` against the
  WASM core.

## 4. Licensing

- **Core crate + IDE + CLI + specs: Apache-2.0** (or MIT/Apache dual, Rust convention) —
  the language/bytecode ecosystem should be adoptable by anyone, including other
  firmwares and commercial tools.
- **Firmware crate: GPL-3.0-or-later** — the project exists because a closed firmware
  bothered us; vendors shipping Pixler-based products must share improvements. GPL
  firmware linking Apache/MIT Rust crates is clean; esp-hal/embassy are MIT/Apache.
- Trademark hygiene: "Pixel Blaze" is ElectroMage's mark — "compatible with the Pixel
  Blaze pattern language" in prose only, never in project or crate names.

## 5. Milestones

**M0 — Language core (pure software).**
Workspace + `pixler-core`: logos lexer, Pratt parser, bytecode emitter, VM, full builtin
set (peripheral builtins stubbed); native CLI (`run pattern.js --pixels 150 --frames 300
→ PPM/JSON`); conformance + fuzz harness; stack-vs-register bake-off; **differential
harness against the real PB in the house** (websocket-driven, black-box).
*Exit: ≥90% of corpus compiles; golden vectors locked; VM design locked by benchmark.*

**M1 — Browser playground.**
wasm-bindgen build + Svelte/TS IDE core (editor, strip/2D preview, controls, var
watcher) as a static page. First shareable artifact, zero hardware needed.
*Exit: live-edit rainbow/blinkfade/xorcery at 60 FPS for 1K virtual pixels.*

**M2 — First light (firmware alpha).**
esp-hal + embassy app: WiFi onboarding, FS, HTTP+WS, serve IDE, on-device compile, RMT
WS281x, live-code end-to-end, preview stream, stats.
*Exit: type in browser → LEDs change; evening-long soak without a crash. Checkpoint:
no_std stack viable, or invoke esp-idf-svc contingency.*

**M3 — Feature-parity core.**
APA102/SPI; settings; pattern CRUD + `.epe` import/export; mapper (1D/2D/3D) + render
selection + transforms; controls persistence; playlists/shuffle; auto-off; backup/
restore; OTA.
*Exit: a PB user migrates a real 2D-mapped project without touching pattern code.*

**M4 — Integrations.**
MQTT + HA discovery; DDP/E1.31 input; Pixler-to-Pixler discovery, timebase sync,
leader/follower groups (pattern launch, brightness, live-edit broadcast, `nodeId()`);
SNTP + timezone clock functions.
*Exit: device auto-appears in Home Assistant; two devices render one synced pattern;
xLights drives a Pixler over DDP.*

**M5 — Peripherals.**
Peripheral framework (§2.5); PB sensor board driver (SB1.0) as first audio/IMU provider;
I2S mic + on-device FFT as second audio provider; UPXL output-expander driver; GPIO/
analog/touch builtins wired to real pins.
*Exit: sound-reactive classics run from either audio source; missing-peripheral warnings
behave as specced.*

**Later:** second language frontend targeting the documented bytecode; parallel output
drivers (I2S/LCD 8–16 lanes); ESP32-P4; HDR pipelines; self-hostable pattern-sharing
site; RP2350-based expander firmware.

## 6. Risks & mitigations

| Risk | Mitigation |
|---|---|
| VM too slow (make-or-break) | Benchmark vs PB's published numbers in M0 week 1; stack-vs-register bake-off; fixed point avoids soft-float cliffs; PB proves the ceiling on identical silicon. Rust interpreters can match C with careful dispatch (match-based first, measured; `unsafe` tail-call/computed-goto tricks only if numbers demand). |
| no_std Rust stack immaturity (esp-wifi, OTA, TLS) | Scoped contingency: esp-idf-svc (std Rust) behind the same host traits; decision gate at M2 exit. Core crate unaffected either way. |
| Xtensa toolchain friction (espup fork) | C3 (mainline RISC-V Rust) kept as a first-class target so contributors without espup can still build firmware; document setup; CI builds all targets. |
| Language edge-case mismatch breaks community patterns | Conformance-first: every documented gotcha is a failing test before implementation; the in-house PB is the black-box oracle for undocumented corners. |
| Scope creep (PB is 8 years of polish) | Serial milestones; M1 delivers standalone value even if firmware slips; integrations (M4) deliberately after parity (M3). |
| IDE bundle > flash budget | Svelte (no runtime) + TS strict; code-split mapper/playlist; size gate in CI. |
| WS2812 timing vs WiFi jitter | RMT/DMA (solved problem); render task isolation under embassy. |
| Solo-project stall | Portable core + browser playground maximize no-hardware contributor surface; conformance suite makes drive-by PRs safe. |
| Legal/trademark | Public docs + black-box behavior only; provenance documented in research docs; no PB-derived names. |

## 7. Open questions

1. **Name.** Requirements: not derivative of "Pixel Blaze", searchable, available.
   Checked candidates: **Tindra** (Swedish "to twinkle" — clean in the software/LED
   namespace; recommended), **Luxel** ("a pixel of light" — evocative, minor collisions:
   Valve lightmap term, a GitHub username), ~~Lampyre~~ (taken, OSINT tool). "Pixler"
   remains the working title until decided.
2. Stack vs register bytecode — settle by M0 benchmark.
3. Extension surface: strict PB parity for v1 frontend, with Pixler-only builtins gated
   behind a pattern pragma? (Leaning yes — keeps `.epe` round-tripping honest.)
4. Preview transport: raw RGB frames first (3 KB/frame @ 1K px is fine on LAN);
   delta-encode only if it shows up in profiles.
5. Second-frontend candidates (post-v1): a lisp? a Rust-y DSL? Decision deferred, but the
   bytecode spec doc is written as if the second frontend already exists.
