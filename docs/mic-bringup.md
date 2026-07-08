# Onboard-mic bring-up plan (PB v3 "SPI audio")

Everything except the pins is ready: `luxel_core::audio::AudioAnalyzer`
(fixed-point 512-pt FFT → the 32 sensor bands, unit-tested against
synthetic sines) feeds `shared::set_sensor_frame`, the exact path the
serial sensor board and `POST /api/sensors` already use — so once samples
flow, sound-reactive patterns work with zero further plumbing.

## What we don't know (closed hardware)

The Pixelblaze v3's onboard microphone type and wiring are undocumented:

- **I2S MEMS** (SPH0645/INMP441 class): 3 signals — BCLK, LRCLK/WS, DATA.
- **PDM MEMS**: 2 signals — CLK, DATA (esp-hal: `I2s::new_pdm`, the ESP32
  has PDM RX filters).

## Bench procedure (needs you + serial)

1. Identify the mic package near the expansion header; photograph traces
   or beep out which ESP32 GPIOs its pads reach (likely candidates: the
   I2S0-capable pins not used by SPI/LEDs).
2. Tell me `type + pins`; I add an `LUXEL_MIC` env knob to the build
   (`pdm:CLK,DATA` or `i2s:BCLK,WS,DATA`), spawn a `mic_task` that
   configures esp-hal I2S RX (DMA, 20 kHz mono, 16-bit) and pumps chunks
   through `AudioAnalyzer::push_samples` → `set_sensor_frame`.
3. Verify: `export var energyAverage` pattern + clap test; then the
   Settings page could grow a mic on/off toggle.

Fallbacks that already work today, no bring-up needed: the playground's
sound toggle streams your laptop mic to the strip (POST /api/sensors), and
the official PB sensor expansion board works plugged into RX0.

## Analyzer notes

- 512-pt radix-2, Q15 twiddles from fmath's sine, Hann window, per-stage
  scaling (no overflow — products widened to i64), integer sqrt magnitudes.
- Bands mirror the sensor board: 37 Hz–10 kHz log-spaced ×32; at 20 kHz
  sample rate the FFT resolution is ~39 Hz (matching the board's 37 Hz
  bottom bin).
- Normalization: full-scale sine ≈ 1.0 in its band (pinned by test).
- Cost: ~512-sample buffer + two 512×i32 scratch vectors on the heap;
  runs ~39 analyses/s at 20 kHz — comparable to the board's ~40 Hz.
