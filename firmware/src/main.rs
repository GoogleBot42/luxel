//! Luxel firmware, milestone 2 "first light" scaffold.
//!
//! ESP32-C3 + SK9822/APA102 strip over SPI (data GPIO7, clock GPIO6 by
//! default — see PINOUT below). Runs the Luxel engine on an embedded pattern
//! and pushes frames as fast as they render. No WiFi yet: this stage proves
//! engine-on-target and the LED path.
//!
//! Flash + monitor (devkit over USB):
//!   cd firmware && cargo run --release

#![no_std]
#![no_main]

extern crate alloc;

use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::main;
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::spi::Mode;
use esp_hal::time::{Instant, Rate};
use esp_println::println;
use luxel_core::engine::Engine;
use luxel_core::fixed::Fx;

esp_bootloader_esp_idf::esp_app_desc!();

// ---- board configuration (bare C3 devkit defaults) ----
// PINOUT: SK9822/APA102 CLOCK → GPIO6, DATA → GPIO7. On the Athom LS4P (C3
// generation) there is no clock terminal; WS281x-over-RMT lands next.
const PIXEL_COUNT: u32 = 300;
/// Global brightness 0–31 (APA102 5-bit current limiter). Keep modest on
/// USB power — 300 px at full white would brown out a devkit.
const APA_BRIGHTNESS: u8 = 4;
const SPI_MHZ: u32 = 8;

const PATTERN: &str = include_str!("../../examples/rainbow.js");

/// SK9822/APA102 frame: 4×0x00 start, per-LED (0xE0|global, B, G, R),
/// 4×0x00 SK9822 reset, then ≥ pixels/2 extra clock edges (zero bytes are
/// safe for both chip families).
fn frame_len(pixels: usize) -> usize {
    4 + pixels * 4 + 4 + pixels.div_ceil(16)
}

fn encode_apa102(rgb: &[[u8; 3]], out: &mut [u8]) {
    let mut i = 4; // leading zeros already there
    for px in rgb {
        out[i] = 0xE0 | (APA_BRIGHTNESS & 0x1F);
        out[i + 1] = px[2]; // B
        out[i + 2] = px[1]; // G
        out[i + 3] = px[0]; // R
        i += 4;
    }
    // trailing reset + end clocks stay zero
}

#[main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let p = esp_hal::init(config);
    esp_alloc::heap_allocator!(size: 200 * 1024);

    println!("luxel-fw: boot ({} px SK9822, SPI {} MHz)", PIXEL_COUNT, SPI_MHZ);

    let mut spi = Spi::new(
        p.SPI2,
        SpiConfig::default()
            .with_frequency(Rate::from_mhz(SPI_MHZ))
            .with_mode(Mode::_0),
    )
    .expect("spi init")
    .with_sck(p.GPIO6)
    .with_mosi(p.GPIO7);

    let mut engine = match Engine::new(PATTERN, PIXEL_COUNT, 1) {
        Ok(e) => e,
        Err(d) => {
            println!("pattern compile error: {}", d.message);
            loop {
                // nothing to render; keep the panic handler out of it
            }
        }
    };
    if let Some(e) = engine.take_error() {
        println!("pattern init error: line {}:{}: {}", e.line, e.col, e.message);
    }

    let mut buf = alloc::vec![0u8; frame_len(PIXEL_COUNT as usize)];
    let mut last = Instant::now();
    let mut frames: u32 = 0;
    let mut fps_mark = Instant::now();

    loop {
        let now = Instant::now();
        let delta_us = (now - last).as_micros();
        last = now;
        // µs → 16.16 ms
        let delta = Fx::from_raw(((delta_us << 16) / 1000) as i32);

        let px = engine.frame(delta);
        encode_apa102(px, &mut buf);
        if let Err(e) = spi.write(&buf) {
            println!("spi write error: {:?}", e);
        }
        if let Some(e) = engine.take_error() {
            println!("vmerr: line {}:{}: {}", e.line, e.col, e.message);
        }

        frames += 1;
        if (Instant::now() - fps_mark).as_millis() >= 1000 {
            println!(
                "fps: {}  ({} px/s)",
                frames,
                frames * PIXEL_COUNT
            );
            frames = 0;
            fps_mark = Instant::now();
        }
    }
}
