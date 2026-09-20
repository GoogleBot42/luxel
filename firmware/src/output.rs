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
//! **One driver, possibly several physical outputs** (Gitea #474). A board
//! that breaks out more than one strip channel (`board::OUTPUTS` > 1 — the
//! Athom) drives them from this ONE driver: each gets its own SPI
//! peripheral and encode buffer, and the `Layout`'s `out` table says which
//! consecutive run of the one pixel space each carries. The render task is
//! unchanged by that; the split happens inside `write_frame`. The whole
//! second channel is behind the `multi_output` cfg (firmware/build.rs), so
//! a one-output board's image is what it always was.
//!
//! Static dispatch on purpose: embassy tasks can't be generic, so the
//! render task takes the concrete [`BoardOutput`] alias. The trait is the
//! contract a new driver must satisfy, not a vtable — there is no `dyn`
//! in the frame path.

#[cfg(not(feature = "hub75"))]
use esp_hal::spi::master::{Config as SpiConfig, ConfigError, SpiDma};
#[cfg(not(feature = "hub75"))]
use esp_hal::spi::Mode;
#[cfg(not(feature = "hub75"))]
use esp_hal::time::Rate;
#[cfg(not(feature = "hub75"))]
use esp_hal::Blocking;
#[cfg(not(feature = "hub75"))]
use esp_println::println;

use crate::leds::Protocol;
#[cfg(not(feature = "hub75"))]
use crate::shared::PROTOCOL;
#[cfg(not(feature = "hub75"))]
use core::sync::atomic::Ordering;
#[cfg(not(feature = "hub75"))]
use luxel_core::layout::Run;
#[cfg(multi_output)]
use luxel_core::outpipe::ColorOrder;

/// The board's output driver — the concrete type behind `render_task`.
/// Boards with a non-strip output switch this alias per feature.
#[cfg(not(feature = "hub75"))]
pub type BoardOutput = SpiStripOutput;
#[cfg(feature = "hub75")]
pub type BoardOutput = crate::hub75::Hub75Output;

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
    ///
    /// Returns whether the frame was actually put on its way to the
    /// fixture. `false` = nothing was written and this frame is gone — a
    /// paused driver, or a panel whose previous buffer swap has not landed
    /// yet. Only `true` frames count towards `out_fps` (Gitea #378).
    fn write_frame(&mut self, rgb: &[[u8; 3]], brightness5: u8) -> bool;

    /// Would a `write_frame` right now do anything, or would the frame be
    /// dropped? A caller that would rather WAIT a moment than lose the
    /// frame polls this (Gitea #387). Drivers that never refuse say `true`.
    fn ready_for_frame(&self) -> bool {
        true
    }

    /// Does this driver have a display clock of its own that the render
    /// loop should pace on — one composed frame per panel rescan instead of
    /// a fixed timer (Gitea #387)? `false` = the caller keeps its own
    /// pacing, which is right for anything wire-bound.
    fn paces_frames(&self) -> bool {
        false
    }
}

/// SPI config for a protocol (only the clock rate differs; mode 0 for both).
#[cfg(not(feature = "hub75"))]
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
#[cfg(not(feature = "hub75"))]
struct EncodeBuf(alloc::vec::Vec<u32>);

#[cfg(not(feature = "hub75"))]
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
#[cfg(not(feature = "hub75"))]
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
/// SPI peripheral and the encode buffer (and, on a multi-output board, the
/// second channel's); reads `shared::PROTOCOL` for output 0's active
/// encoding exactly like the pre-trait render task did (the render task
/// remains that atomic's sole writer).
#[cfg(not(feature = "hub75"))]
pub struct SpiStripOutput {
    spi: SpiDma<'static, Blocking>,
    buf: EncodeBuf,
    /// The board's SECOND output, when the Layout configures one. `None` on
    /// a board that has no second channel, and on one that has it but is
    /// not using it: the peripheral is then never constructed at all.
    #[cfg(multi_output)]
    second: Option<Chan>,
}

/// One further physical channel: its own SPI peripheral, its own encode
/// buffer sized to its own run, its own protocol and colour order.
///
/// Its protocol AND its colour order are fixed at boot: the SPI clock is
/// configured when the peripheral is built, and both are copied in here by
/// `attach_second`. Only output 0's are live (`set_protocol` /
/// `/api/protocol`, and `shared::COLOR_ORDER` which the output chain reads
/// every frame) — which is exactly why `Layout::reboot_required` gates
/// protocol and colour order on a FURTHER output but not on output 0
/// (Gitea #550). Its run (`count`, `rev`) is live on both: `write_frame`
/// re-reads the split from the Layout per frame.
#[cfg(multi_output)]
struct Chan {
    spi: SpiDma<'static, Blocking>,
    buf: EncodeBuf,
    proto: Protocol,
    /// [`ColorOrder`] code. The output chain already permuted the shared
    /// frame into output 0's order, so this becomes a per-frame fix-up
    /// relative to that (`ColorOrder::relative`), not a second full pass.
    order: u8,
}

#[cfg(not(feature = "hub75"))]
impl SpiStripOutput {
    pub fn new(spi: SpiDma<'static, Blocking>) -> Self {
        Self {
            spi,
            buf: EncodeBuf::new(),
            #[cfg(multi_output)]
            second: None,
        }
    }

    /// Attach the board's second output: the peripheral main.rs wired to
    /// that output's DATA pad, plus the protocol and colour order its `out`
    /// line asked for. Called once, at boot, only when the Layout configures
    /// output 1 (Gitea #474).
    #[cfg(multi_output)]
    pub fn attach_second(&mut self, spi: SpiDma<'static, Blocking>, proto: Protocol, order: u8) {
        self.second = Some(Chan { spi, buf: EncodeBuf::new(), proto, order });
    }

    fn proto(&self) -> Protocol {
        Protocol::from_u8(PROTOCOL.load(Ordering::Relaxed))
    }
}

/// The run output `n` drives over a `pixels`-long frame, clamped to it —
/// `Run::NONE` for an output the Layout does not configure.
#[cfg(not(feature = "hub75"))]
fn run_of(n: u8, pixels: u32) -> Run {
    crate::layout::run_of(n, pixels).unwrap_or(Run::NONE)
}

/// Encode one output's run and put it on that output's wire. Shared by
/// every channel so the split costs one copy of the encode+write path, not
/// one per output. `false` = the encode buffer could not be allocated and
/// this channel wrote nothing (the caller retries next frame).
#[cfg(not(feature = "hub75"))]
fn write_run(
    spi: &mut SpiDma<'static, Blocking>,
    buf: &mut EncodeBuf,
    proto: Protocol,
    run: Run,
    perm: Option<[u8; 3]>,
    rgb: &[[u8; 3]],
    brightness5: u8,
) -> bool {
    if run.len == 0 {
        return false; // an output whose run the pixel space no longer reaches
    }
    // Exact, not "at least": the whole buffer goes on the wire (the DMA path
    // wants a length that is a multiple of 4, so the <=3 pad bytes ride
    // along), and a buffer left long from a bigger run would put its stale
    // tail out as LED data. An empty buffer is a failed realloc — retry it
    // lazily, heap may have freed up; never index out of bounds.
    let need = proto.buf_len(run.len as usize);
    if buf.len() != need && !realloc_buf(buf, need) {
        return false;
    }
    proto.encode_run(rgb, run, perm, brightness5, buf.bytes_mut());
    if spi.write(buf.bytes()).is_err() {
        println!("spi write error");
    }
    // The wire took it either way: a failed SPI write is a driver error, not
    // a frame the caller could usefully retry.
    true
}

/// Is the strip's SPI2 transfer still running? Read by the flash fence on
/// the OTHER core (core1.rs) before a flash op: on the classic ESP32 an SPI1
/// flash op during an in-flight SPI2 DMA transfer hangs the CPU (the SPI
/// hosts share the DMA engine), so the fence waits for the transaction to
/// end. Reads the `SPI_CMD.usr` bit straight from the register — the driver
/// owns the peripheral, and the flag must reflect hardware, not a software
/// marker the parked core could never clear.
#[cfg(all(not(feature = "hub75"), feature = "esp32"))]
pub fn transfer_busy() -> bool {
    // HSPI (SPI2) on the classic ESP32: base 0x3FF6_4000, CMD register at
    // +0, `usr` = bit 18 (transaction in progress).
    if unsafe { core::ptr::read_volatile(0x3FF6_4000 as *const u32) } & (1 << 18) != 0 {
        return true;
    }
    // The second output (Gitea #474) is VSPI (SPI3), base 0x3FF6_5000 — the
    // same shared-DMA hazard, so the fence must wait for it too. Read only
    // once that peripheral has actually been constructed: a pin is bound iff
    // the driver is live.
    #[cfg(multi_output)]
    if crate::shared::DATA_PIN2.load(Ordering::Relaxed) != crate::shared::NO_PIN
        && unsafe { core::ptr::read_volatile(0x3FF6_5000 as *const u32) } & (1 << 18) != 0
    {
        return true;
    }
    false
}

/// HUB75 (circular DMA, never idle by design) and the other chips: no
/// shared-DMA hazard known; the fence does not wait. The S3 strip build is
/// unverified on metal (Gitea #266).
#[cfg(not(all(not(feature = "hub75"), feature = "esp32")))]
pub fn transfer_busy() -> bool {
    false
}

#[cfg(not(feature = "hub75"))]
impl OutputDriver for SpiStripOutput {
    type Error = ConfigError;

    fn set_protocol(&mut self, p: Protocol) -> Result<(), ConfigError> {
        self.spi.apply_config(&spi_cfg(p))
    }

    /// Re-read the split from the Layout and size each output's buffer to
    /// its OWN run — not to the whole frame. Two 30 px runs cost two 360 B
    /// WS2812 buffers, the same total as the one 60 px buffer they replace
    /// (plus one extra latch tail): a second output is not a second
    /// full-frame buffer.
    fn resize(&mut self, pixels: usize) -> bool {
        let pixels = pixels as u32;
        let need = self.proto().buf_len(run_of(0, pixels).len as usize);
        let mut ok = realloc_buf(&mut self.buf, need);
        #[cfg(multi_output)]
        if let Some(c) = self.second.as_mut() {
            ok &= realloc_buf(&mut c.buf, c.proto.buf_len(run_of(1, pixels).len as usize));
        }
        ok
    }

    /// Write every configured output from the ONE frame, in index order.
    /// Sequential: `spi.write` blocks until the transfer is done, so wire
    /// time adds up across outputs (`out_us` covers them all). `true` if any
    /// output took the frame.
    fn write_frame(&mut self, rgb: &[[u8; 3]], brightness5: u8) -> bool {
        // Re-read the split against THIS frame's length rather than caching
        // it in the driver: the frame the render task hands over can be a
        // pixel count the driver has not been resized for yet (a live
        // `/api/config` drains a frame later), and a cached run would be 12
        // bytes of task statics on every board for a value the Layout
        // already holds — `.stack` is within tens of bytes of its floor
        // (docs/boards.md).
        let px = rgb.len() as u32;
        let proto = self.proto();
        let mut any =
            write_run(&mut self.spi, &mut self.buf, proto, run_of(0, px), None, rgb, brightness5);
        #[cfg(multi_output)]
        if let Some(c) = self.second.as_mut() {
            // the shared frame is already in output 0's colour order
            let perm = ColorOrder(c.order)
                .relative(ColorOrder(crate::shared::COLOR_ORDER.load(Ordering::Relaxed)));
            any |= write_run(&mut c.spi, &mut c.buf, c.proto, run_of(1, px), perm, rgb, brightness5);
        }
        any
    }
}
