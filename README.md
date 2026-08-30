# Luxel

A fully open-source, live-codable LED controller — write LED patterns in a
scripting language in a web IDE and watch them run instantly on real
hardware.

A *luxel* is a "light element" — a pixel of light. Luxel is inspired by the
(closed-source) Pixel Blaze and source-compatible with its pattern
language, but deliberately **not** a drop-in clone: it's a
better-integrated take on the same idea — Home Assistant, network pixel
protocols, multi-device sync, and open APIs everywhere.

## What works today

**The engine** (`crates/luxel-core`, Rust `no_std`, 16.16 fixed point) —
one compiler + VM + frame engine. The VM runs identically on the ESP32,
in the browser via WASM, and natively for tests; the compiler runs in the
browser/CLI and ships **LXBC bytecode** to the device (like a real Pixel
Blaze, the firmware executes bytecode only — no parser on the chip, so no
compile-time stack or RAM spikes there either). The full
documented builtin surface (waveforms, math, arrays, color incl. oklch,
transforms, perlin/simplex, palettes, clock, pixel maps, sensors) plus
Luxel extensions (easings, `blur1D`/`feedback`, `beatSin`, `hash`, …).
Differentially tested against a real Pixel Blaze — rendering is
**pixel-bit-exact** (quantization, hsv, palettes) and semantics match on
every probe that isn't PB's own float noise. All 283 valid community
patterns compile and run.

**The playground** (`web/`) — a Svelte + TypeScript IDE served from device
flash (or any static host): CodeMirror editor with live recompile, a
322-pattern gallery with animated tiles (1D bars, 2D grids, rotating 3D
clouds), step debugger with breakpoints, map editor (a debuggable map
*program*), auto-generated controls, var watcher, shareable pattern URLs
(maps included), `.epe` import/export, and a **sound toggle** that drives
sound-reactive patterns from your microphone — locally and on the device.

**The firmware** (`firmware/`, esp-hal + embassy; release images for ESP32, C3, C6, S3, and HUB75-panel boards):

- Live coding over WiFi: type in the browser, the strip follows.
- SK9822/APA102 + WS281x over SPI; runtime pixel count, protocol, color
  order, gamma, and power-cap settings — all changed live, all persisted.
- **Playlists** with per-item parameters, durations, and crossfades;
  survive reboots.
- **Home Assistant** via MQTT discovery: light (power + brightness),
  pattern select, playlist switch + next/prev, diagnostics, and an
  event topic for automations — full topic reference in
  [docs/mqtt.md](docs/mqtt.md).
- **DDP + E1.31/sACN input** — xLights/LedFx/Resolume drive the strip
  directly; the pattern resumes when the stream stops.
- **Luxel-to-Luxel sync**: leader broadcasts its timebase, sensor data,
  and running pattern; followers phase-lock and adopt it.
- **Sensors**: the (open) PB sensor-board serial protocol, network
  injection (`POST /api/sensors`), and a ready fixed-point FFT for the
  onboard-mic bring-up.
- **External events**: `readEvent`/`eventCount` builtins + `POST
  /api/events` + the `luxel/<id>/event` MQTT topic — keyboards, HA
  automations, and preview clicks drive keypress-reactive patterns.
- NTP wall clock (+ timezone) so `clockHour()` patterns work unplugged
  from a browser.
- **AP-mode provisioning**: a device with no network boots as
  `luxel-xxxx` with a captive settings portal.
- OTA updates with a boot-loop guard (a bad image rolls itself back), and
  a pattern library + maps in flash.

**The mirror** (`luxel serve`) — a native replica of the device HTTP API
so the whole UI is developed and e2e-tested without hardware.

## Quickstart

Everything runs through nix (flake in the repo root):

```sh
# browser playground (no hardware needed)
nix develop
cd web && npm install && npm run dev

# native mirror of a device (for UI dev / API poking)
cargo run -p luxel-cli -- serve --port 8720

# flash an ESP32 (Pixelblaze v3 wiring by default) — app + web assets:
cd firmware && cp creds.env.example creds.env  # your WiFi, git-ignored
BOARD=board-pixelblaze-v3 ./build-esp32.sh flash

# later updates over the air, firmware + web app in one:
tools/deploy.sh <device-ip>
```

CLI extras: `luxel run pattern.js` (PPM frame strip), `bench`, `parse`,
`check`, `vars`, `pixels`.

## Map of the repo

| where | what |
|---|---|
| `crates/luxel-core` | compiler + VM + engine (`no_std`), sensor/net parsers, output pipeline |
| `crates/luxel-wasm` | C-ABI wasm bindings for the browser |
| `crates/luxel-cli` | native CLI + the device-API mirror (`luxel serve`) |
| `firmware/` | ESP32 firmware (esp-hal, embassy, picoserve) |
| `web/` | the playground/IDE (Svelte, CodeMirror), e2e suites in `web/tools/` |
| `tools/` | deploy, OTA push, hardware bench, oracle harness (`tools/oracle/`) |
| `docs/` | plan, language reference, firmware notes, research, update log |

Good starting docs: [docs/lang.md](docs/lang.md) (the pattern language),
[UPDATES.md](UPDATES.md) (what shipped, newest first),
[docs/PLAN.md](docs/PLAN.md) (architecture),
[docs/tools.md](docs/tools.md) (every script/harness: soak, oracle,
corpus, e2e, deploy — what to reach for and when),
[docs/research/04-oracle-findings.md](docs/research/04-oracle-findings.md)
(how we know it matches the real thing).

## License

Luxel is licensed by component:

| Component | Path | License |
|---|---|---|
| Engine (compiler + VM + engine) | `crates/luxel-core` | Apache-2.0 |
| Browser/WASM bindings | `crates/luxel-wasm` | Apache-2.0 |
| CLI + device-API mirror | `crates/luxel-cli` | Apache-2.0 |
| Playground / IDE | `web/` | Apache-2.0 |
| Pattern library | `library/` | Apache-2.0 |
| Device firmware | `firmware/` | GPL-3.0-or-later |

Full texts: [LICENSE-APACHE](LICENSE-APACHE) (SPDX `Apache-2.0`) and
[LICENSE-GPL](LICENSE-GPL) (SPDX `GPL-3.0-or-later`, also copied into
[firmware/LICENSE](firmware/LICENSE) so the firmware subtree is
self-contained). Unless a file or directory says otherwise, code outside
`firmware/` is Apache-2.0.

*Pixel Blaze is a trademark of its owner; this is an independent project,
built clean-room from public documentation, compatible with the Pixel
Blaze pattern language.*
