# Luxel

A fully open-source, live-codable LED controller — write LED patterns in a scripting
language in a web IDE and watch them run instantly on real hardware.

A *luxel* is a "light element" — a pixel of light. Luxel is inspired by the
(closed-source) Pixel Blaze and source-compatible with its pattern language, but
deliberately **not** a drop-in clone: it's a better-integrated take on the same idea — Home Assistant/MQTT integration, multi-device sync, open APIs, and a
documented bytecode VM that other languages can target too.

- **Pattern-source compatibility** with the Pixel Blaze language — the 200+ community
  patterns (`.epe`) should just work. Built clean-room from public documentation only.
- **One portable core** (Rust, `no_std`, 16.16 fixed point): the same compiler + VM runs
  in the ESP32 firmware (esp-hal + embassy), in the browser via WASM (instant
  hardware-free preview), and natively in CI (conformance tests, fuzzing).
- **Integration-first:** MQTT + Home Assistant discovery, DDP/E1.31 input, and
  Luxel-to-Luxel sync — instead of PB-protocol emulation.
- **Generic peripherals:** a capability-based peripheral framework; Pixel Blaze's open
  sensor-board and output-expander protocols are supported as drivers, not as lock-in.
- **Web IDE:** Svelte + TypeScript + CodeMirror 6, served from device flash; no cloud,
  no app — everything works on a device in AP mode.

Status: **M0 in progress** — the language core. Working today: `luxel-core`
(no_std lexer/parser + 16.16 fixed-point type with the documented PB semantics,
conformance-tested) and a `luxel parse` CLI. Dev environment via `nix develop`.
See [docs/PLAN.md](docs/PLAN.md) for architecture, requirements, and milestones,
and [docs/research/](docs/research/) for the research behind it.

Licensing (planned): core, IDE, and CLI under Apache-2.0; device firmware under
GPL-3.0-or-later.

*Pixel Blaze is a trademark of its owner; this is an independent project compatible with
the Pixel Blaze pattern language.*
