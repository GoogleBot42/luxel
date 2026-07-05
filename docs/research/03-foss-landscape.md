# Research: FOSS LED controller landscape (July 2026)

> Raw research positioning Luxel. Question: does anything FOSS combine (a) on-device
> pattern VM, (b) JS-like language, (c) live web IDE, (d) pixel-mapping coordinate model?
> Answer: **no** — the niche is open.

## 1. WLED

- Current: **v16.0.0** (May 2026; skipped from 0.15.x), 0.15.4 maintenance line, nightlies.
  v16: segment layering/blending, particle system, GIF playback, PixelForge pixel-art
  editor, custom fonts, new ESP-IDF base. Targets ESP8266, ESP32, S2, S3, C3.
- Architecture: effects are **compiled-in C++** over segments; JSON API + WebSocket, MQTT,
  E1.31/DDP/Art-Net, UDP sync. Extension = usermods (`strip.addEffect()`, build-time).
- **No user scripting, deliberately** — community proposal (wled.discourse.group t/13687,
  Dec 2024) went nowhere; not on roadmap. ESP8266 constraints + architecture cited.
- **ARTI-FX** (MoonModules/WLED-SR): custom C-like interpreted language with
  `renderFrame`/`renderLed` (exactly PB's model) — effectively **dead/archived**.
- License: **EUPL-1.2** since 0.15.0-b6 (was MIT). EUPL is copyleft, GPL-compatible
  outbound; MIT forks can't pull post-2024 code.

## 2. Direct prior art

- **No credible FOSS Pixelblaze firmware reimplementation exists** (extensive searching).
  Ecosystem tooling exists (see language doc §5), but nobody has rebuilt the
  firmware/VM/IDE stack.
- **Closest living competitor: MoonLight + ESPLiveScript** (MoonModules, GPL-3.0):
  - ESPLiveScript (hpwit/Yves): "almost-C" **compiled to native Xtensa code on-device**,
    <10% overhead vs C claim. One-man project, Xtensa-only, limited docs.
  - MoonLight: ESP32-sveltekit UI, live scripts, 3D fixtures, Art-Net, claims 12K LEDs @
    100 FPS. **v1.0.0 (May 2026) was the final release; project rebooted again to
    "ProjectMM"** — the org has churned WLED-SR → WLED-MM → StarLight → MoonLight →
    ProjectMM. C syntax, not PB-compatible, organizationally unstable.
- **ESPHome**: lambdas are build-time C++, no live loop. **LedFx** (GPL-3, active): PC-side
  Python audio-reactive streamer, no on-device scripting. **NodeMCU** (Lua): not
  LED-centric. **MicroPython/CircuitPython**: per-pixel Python far too slow (~40 ms/frame
  for 256 px naive on RP2040; ulab vectorization helps but wrong model). **xLights/FPP**:
  sequenced-show world — complementary; a new device should speak DDP/E1.31 so they can
  drive it. **Chromatik/LX**: source-available but NOT open (non-commercial + $25K cap).
- Pixelblaze retail note: ElectroMage Tindie store "taking a break until May 2027";
  Crowd Supply/Adafruit still list product. Single-vendor, closed firmware.

## 3. Embedded scripting engines for ESP32-class MCUs

Workload: per-pixel callback at 30–60 FPS. 1000 px @ 30 FPS = 30K calls/s × tens of ops
→ ~1–5M interpreted ops/s sustained, alongside WiFi + LED DMA.

| Engine | Notes | Verdict for per-pixel |
|---|---|---|
| **Custom PB-style bytecode VM** | What PB itself does; 16.16 fixed point; proven perf envelope (48K px/s on ESP32) | The benchmark to match; only path to exact PB semantics |
| **Berry** (MIT) | <40 KB flash, on-device compile, ships in every Tasmota32 | Strong, but wrong syntax (breaks PB pattern compat) |
| **JerryScript** (Apache-2.0) | Full ES5.1+, <200 KB ROM; activity slowing since 2024 | Full-JS per-op cost too heavy at 30K calls/s |
| **QuickJS / quickjs-ng** (MIT) | Full ES2020+; 100s KB RAM, GC pauses | Too big/slow for plain ESP32 |
| **MicroQuickJS** (Bellard, late 2025) | ~10 KB RAM JS subset; hobby ESP32 port exists | Immature; watch |
| **mJS / Elk** (GPLv2 / AGPL, Cesanta) | Dormant / source-interpreting | Too slow + license poison |
| **Espruino** (MPL-2.0) | Interprets source text; ~4K loop iters/s | Orders of magnitude short |
| **MicroPython / CircuitPython** (MIT) | Mature | Wrong perf model for per-pixel callbacks |
| **Lua 5.4** (MIT) | Fast register VM | Contender technically; syntax mismatch |
| **wasm3** (MIT) | ~4–15× slower than native; maintenance mode | Possibly sufficient but needs browser WASM toolchain |
| **WAMR** (Apache-2.0) | Official Espressif component; AOT near-native | Viable "compile-in-IDE, run AOT" architecture; heavier toolchain |
| **ESPLiveScript** | On-device native codegen | Fastest, but C ergonomics, Xtensa-only, bus factor 1 |

**Fixed point vs float** (Espressif blog, Oct 2025): hardware single-precision FPU only on
ESP32, S3, P4, H4; **no FPU on S2, C2, C3, C5, C6, H2**; doubles always software.
C3-vs-S3: hard-float ~75–95% faster than soft-float. ⇒ 16.16 fixed point keeps cheap
RISC-V chips (C3/C6) first-class AND matches PB semantics exactly. Optional float build
possible later for S3/P4.

## 4. Hardware platforms & output drivers

- **ESP32 classic**: 8 RMT TX; I2S0 parallel (up to 24 lanes via FastLED, 16 via hpwit).
- **ESP32-S3**: 4 RMT TX; parallel via **LCD_CAM** (16 lanes); PSRAM common. Default big rig.
- **ESP32-C3**: single RISC-V 160 MHz, no FPU, 2 RMT; the $2 node chip.
- **ESP32-P4**: dual RISC-V 400 MHz + FPU, no radio (pair with C6). FastLED supports it
  since 3.10.0. Highest headroom, two-chip networking complication.
- **RP2040/RP2350**: PIO drives LEDs with ~zero CPU; RP2350 has FPU. Weak WiFi story
  (Pico W CYW43) — better as an output-expander-class device than the main controller.
- Driver libraries: **FastLED** (MIT, 3.10.5, RMT4/5 + I2S-parallel + S3 LCD, P4 support,
  fastled-wasm browser compiler is prior art for IDE simulation); **NeoPixelBus**
  (LGPL-3.0, what WLED uses; RMT/I2S/LCD parallel methods); **hpwit I2SClockless drivers**
  (16 strips; virtual variant ~120 strips via shift registers); **Pixelblaze Output
  Expander protocol** as an open UART fan-out standard.

## 5. Licensing

WLED EUPL-1.2 · FastLED MIT · NeoPixelBus LGPL-3.0 · MoonLight GPL-3.0 · LedFx GPL-3.0 ·
xLights GPL-3.0 · Berry MIT · JerryScript/WAMR Apache-2.0 · QuickJS MIT ·
Elk AGPL / mJS GPLv2 (avoid) · Chromatik proprietary · PB firmware closed (docs/protocols open).

Implications:
- MIT/Apache-2.0 maximizes adoption; copyleft (GPL/EUPL) prevents the proprietary-capture
  scenario this project exists to answer, and is required to borrow WLED v16 code.
- LGPL NeoPixelBus static-linked in firmware = relink-offer awkwardness for permissive
  projects; FastLED (MIT) is clean everywhere.
- Sensible split: **portable core (language spec, compiler, VM) permissive** so ecosystem
  tools flourish; **firmware copyleft** so device vendors must share improvements.

## Synthesis

Nothing FOSS combines on-device VM + JS-like language + live web IDE + pixel mapping.
WLED won't do it, ARTI-FX is dead, MoonLight is C-ish/native/unstable, PC-side tools solve
a different problem. Pixel Blaze's own language docs, mapping model, expander/sensor
protocols, and client libraries are open and adoptable. The pivotal technical decision is
the engine, and the analysis strongly favors a custom 16.16 fixed-point bytecode VM.

Key sources: github.com/wled/WLED/releases · wled.discourse.group/t/13687 ·
mm.kno.wled.ge/moonmodules/arti-fx · github.com/hpwit/ESPLiveScript ·
moonmodules.org/MoonLight · developer.espressif.com/blog/2025/10/cores_with_fpu ·
github.com/Makuna/NeoPixelBus/wiki/ESP32-NeoMethods · docs.arduino.cc/libraries/fastled ·
github.com/LedFx/LedFx · heronarts.lx.studio/license
