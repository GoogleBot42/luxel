//! Output drivers: how a rendered frame leaves the chip.
//!
//! The render task (main.rs) is output-agnostic — it hands every
//! post-outpipe RGB888 frame to an [`OutputDriver`] and never touches a
//! peripheral directly. Today there is one driver, [`SpiStripOutput`]
//! (SK9822/WS2812 strips over SPI+DMA); parallel drivers (HUB75 via
//! LCD_CAM, I2S multi-lane, the output expander) slot in as new impls
//! plus a [`BoardOutput`] alias switch, without touching the render loop
//! (docs/PLAN.md "output-driver trait"; Gitea #71).
//!
//! Static dispatch on purpose: embassy tasks can't be generic, so the
//! render task takes the concrete [`BoardOutput`] alias. The trait is the
//! contract a new driver must satisfy, not a vtable — there is no `dyn`
//! in the frame path.

use esp_hal::spi::master::{Config as SpiConfig, ConfigError, SpiDma};
use esp_hal::spi::Mode;
use esp_hal::time::Rate;
use esp_hal::Blocking;
use esp_println::println;

use crate::leds::Protocol;
use crate::shared::PROTOCOL;
use core::sync::atomic::Ordering;

/// The board's output driver — the concrete type behind `render_task`.
/// Boards with a non-strip output will switch this alias per feature.
pub type BoardOutput = SpiStripOutput;

/// One frame sink. Contract notes for implementors:
///
/// - `write_frame` must tolerate a failed/deferred `resize`: it re-checks
///   capacity per frame and silently skips output (never panics, never
///   indexes out of bounds) until an allocation succeeds — the render
///   task treats output as best-effort and keeps the engine ticking.
/// - `set_protocol` reconfigures for a strip protocol. Drivers with a
///   fixed wire format (HUB75) may reject switches; the render task keeps
///   the previous protocol on `Err` (its policy, not the driver's).
/// - Methods run on the render task between frames — blocking briefly is
///   fine (SPI DMA writes block today), `await` is not available.
pub trait OutputDriver {
    /// Peripheral reconfiguration error, surfaced in the render task's log.
    type Error: core::fmt::Debug;
    /// Reconfigure the wire for `p` (clock rate etc.). Must not commit any
    /// state the encode path reads — `shared::PROTOCOL` stays the render
    /// task's to write, after this succeeds.
    fn set_protocol(&mut self, p: Protocol) -> Result<(), Self::Error>;
    /// (Re)size internal buffers for `pixels`. `false` = allocation failed:
    /// output stays paused (see `write_frame`) until a later resize or a
    /// lazy per-frame retry succeeds.
    fn resize(&mut self, pixels: usize) -> bool;
    /// Emit one post-outpipe RGB888 frame. `brightness5` is the 0–31
    /// global level (0 = black; drivers without a hardware brightness
    /// field scale in software).
    fn write_frame(&mut self, rgb: &[[u8; 3]], brightness5: u8);
}

/// SPI config for a protocol (only the clock rate differs; mode 0 for both).
fn spi_cfg(p: Protocol) -> SpiConfig {
    SpiConfig::default()
        .with_frequency(Rate::from_hz(p.spi_hz()))
        .with_mode(Mode::_0)
}

/// SPI encode buffer, u32-backed: the DMA driver only streams a slice
/// zero-copy when its base is 4-byte-aligned and its length a multiple
/// of 4 (classic-ESP32 DMA rule) — a Vec<u8> guarantees neither, and the
/// fallback re-chunks the frame through a bounce buffer (wire gaps: the
/// exact WS2812 corruption DMA is here to prevent). The ≤3 pad bytes
/// stay zero — harmless on both protocols (SK9822 end clocks / WS2812
/// latch tail).
struct EncodeBuf(alloc::vec::Vec<u32>);

impl EncodeBuf {
    const fn new() -> Self {
        Self(alloc::vec::Vec::new())
    }
    fn len(&self) -> usize {
        self.0.len() * 4
    }
    fn bytes(&self) -> &[u8] {
        // u32 → u8 reinterpret: alignment only loosens, length is exact
        unsafe { core::slice::from_raw_parts(self.0.as_ptr().cast(), self.0.len() * 4) }
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.0.as_mut_ptr().cast(), self.0.len() * 4) }
    }
}

/// Resize the SPI encode buffer, releasing the old allocation BEFORE
/// reserving the new one — a protocol switch can more than double it
/// (WS2812 is 9 B/px vs SK9822's ~4 B/px; at 2048 px that's an 18 KB
/// allocation which must not coexist with the old buffer on a tight heap).
/// Fallible: on false the buffer is left empty and the encode paths (which
/// check the length) skip SPI output rather than indexing out of bounds.
fn realloc_buf(buf: &mut EncodeBuf, len: usize) -> bool {
    buf.0 = alloc::vec::Vec::new(); // free the old allocation first
    let words = len.div_ceil(4);
    if buf.0.try_reserve_exact(words).is_err() {
        return false;
    }
    buf.0.resize(words, 0);
    true
}

/// SK9822/WS2812 strips over SPI+DMA — the classic Luxel output. Owns the
/// SPI peripheral and the encode buffer; reads `shared::PROTOCOL` for the
/// active encoding exactly like the pre-trait render task did (the render
/// task remains that atomic's sole writer).
pub struct SpiStripOutput {
    spi: SpiDma<'static, Blocking>,
    buf: EncodeBuf,
}

impl SpiStripOutput {
    pub fn new(spi: SpiDma<'static, Blocking>) -> Self {
        Self { spi, buf: EncodeBuf::new() }
    }

    fn proto(&self) -> Protocol {
        Protocol::from_u8(PROTOCOL.load(Ordering::Relaxed))
    }
}

impl OutputDriver for SpiStripOutput {
    type Error = ConfigError;

    fn set_protocol(&mut self, p: Protocol) -> Result<(), ConfigError> {
        self.spi.apply_config(&spi_cfg(p))
    }

    fn resize(&mut self, pixels: usize) -> bool {
        let len = self.proto().buf_len(pixels);
        realloc_buf(&mut self.buf, len)
    }

    fn write_frame(&mut self, rgb: &[[u8; 3]], brightness5: u8) {
        let proto = self.proto();
        // the buffer is empty after a failed realloc — retry it lazily
        // (heap may have freed up); never index out of bounds
        let need = proto.buf_len(rgb.len());
        if self.buf.len() >= need || realloc_buf(&mut self.buf, need) {
            proto.encode(rgb, brightness5, self.buf.bytes_mut());
            if let Err(e) = self.spi.write(self.buf.bytes()) {
                println!("spi write error: {:?}", e);
            }
        }
    }
}
