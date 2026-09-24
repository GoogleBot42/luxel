//! External PSRAM as a dedicated pattern-array arena (Gitea #253).
//!
//! The Seengreat panel board carries an ESP32-S3-WROOM-1-N16R8 — 8 MB of
//! octal PSRAM that Luxel never touched. Internal DRAM is the thing that
//! bounds patterns on that board (~40 KB free with a 4096-px pattern
//! running, which is not even one pixel-sized array), so the arrays move
//! out and everything else stays exactly where it was.
//!
//! ## What lives here and what must not
//!
//! `ArrRepr::Owned` element storage and each engine's per-frame pixel
//! buffer (`luxel_core::arena::FrameVec`, Gitea #709) — both routed through
//! `luxel_core::arena`'s hook, which this module installs. The frame joined
//! the arrays because it is the largest thing a resident engine owns
//! (12,288 B at 4096 px) and, unlike everything below, is touched once per
//! pixel per frame rather than once per instruction: the VM writes it
//! sequentially-ish and the compositor reads it straight through, which the
//! S3's data cache carries. Measured on the panel before it was believed —
//! docs/boards.md "Engine frames in PSRAM". Two 4096-px pattern layers do
//! not fit internal DRAM otherwise (65,536 B needed against a steady
//! `load_base` of 47–49 KB) and the panel advertised a layer it could not
//! build.
//!
//! Deliberately NOT here, because PSRAM is cache-backed over an octal SPI
//! bus and several times slower per access than DRAM:
//!
//! * the HUB75 DMA framebuffers and the bitplane tables (`hub75.rs`). Not
//!   because DMA cannot reach PSRAM — the S3's GDMA can (docs/boards.md
//!   #521) — but because the panel's refresh reads every plane of every row
//!   continuously, which is the one access pattern the cache cannot help;
//! * the pipeline's travelling frame, the crossfade stage buffer, the
//!   compositor's per-frame scratch, the strip output buffer — all shared,
//!   all read and written *within* one frame beside the layer frames;
//! * the VM's operand stack, locals, globals and the arena's own slot
//!   vector (`Vm::arrays`) — all touched per instruction;
//! * everything the WiFi blob mallocs. esp-radio's `malloc` shim asks
//!   `esp_alloc::HEAP` with no capability filter, so a PSRAM region added
//!   to the GLOBAL heap could serve a blob allocation that has to be
//!   internal (and DMA-reachable). That is why the arena is a **second,
//!   separate `EspHeap`** rather than a third region of the global one:
//!   `esp_alloc::HEAP.free()` keeps meaning exactly what it meant before,
//!   so `RUNTIME_FLOOR`, `budgeted_engine` and `/api/status`'s `heap_free`
//!   all keep their old semantics, and no allocation can reach PSRAM
//!   except through the arena hook.
//!
//! ## Boot order (three constraints, all satisfied by initialising early)
//!
//! [`init`] runs immediately after the `esp_alloc::heap_allocator!` calls in
//! `main`, which is:
//!
//! * **before `esp_rtos::start` and `core1::start`.** `map_psram` suspends
//!   the data cache while it programs MMU entries; a second core executing
//!   from flash across that window would fault. At this point there is only
//!   one core.
//! * **before `assets::map_region()` / any `flashmap::map`.** PSRAM shares
//!   the S3's DBUS MMU table with flash mappings (both at 0x600C5000,
//!   window 0x3C000000). esp-hal's `map_psram` scans the table BACKWARDS
//!   for the last valid entry and maps PSRAM after it, so a flash mapping
//!   made first would push 128 pages of PSRAM towards the end of a
//!   512-entry table. The reverse order is safe: `flashmap::find_free_run`
//!   already treats an entry with bit 15 set (PSRAM) as occupied.
//! * **before `WifiController::new` and `wait_config_up()`.** The tripwire
//!   in CLAUDE.md is about heap pressure racing WiFi bring-up; this runs
//!   long before the radio exists and allocates nothing.
//!
//! ## Flash fence (#309) and the cache
//!
//! On the S3 a PSRAM access is a cache miss to SPI0 — the same cache a
//! flash op disables, and the same one `flashmap::quiesced` suspends. It is
//! therefore governed by exactly the rule mapped flash reads already follow
//! (`core1.rs`): a PSRAM access may only happen in task context. While an
//! SPI1 flash op runs, the other core is parked in the IRAM park handler
//! spinning on DRAM flags — it touches neither flash nor PSRAM — and the
//! fencing core is inside esp-storage's IRAM routine. So a fenced flash op
//! can never overlap an arena access, and no new fence rule is needed.
//! What WOULD break it is an arena read from an interrupt handler; the VM
//! never runs in one.
//!
//! ## Flash clock
//!
//! `Psram::new` reprograms the SPI0/SPI1 flash clock divider from
//! `PsramConfig::flash_frequency` (esp-hal's
//! `spi_timing_enter_mspi_high_speed_mode`), so passing the wrong number
//! silently re-clocks flash. [`init`] reads the actual value out of the
//! image header at flash offset 0 instead of assuming esp-hal's 80 MHz
//! default, and falls back to the slowest setting if it cannot read it —
//! slowing flash down is always safe, speeding it up is not.

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};

use esp_alloc::{EspHeap, HeapRegion, MemoryCapability};
use esp_hal::peripherals::PSRAM;
use esp_hal::psram::{FlashFreq, Psram, PsramConfig, PsramMode, PsramSize, SpiRamFreq};
use esp_println::println;
use esp_storage::FlashStorage;

/// The arena. A second `EspHeap`, never the global one — see the module
/// docs for why that separation is load-bearing.
static ARENA: EspHeap = EspHeap::empty();

/// Mapped PSRAM extent, for routing frees. `END == 0` means "no arena".
static START: AtomicUsize = AtomicUsize::new(0);
static END: AtomicUsize = AtomicUsize::new(0);

/// `esp_image_header_t.spi_speed` → the frequency esp-hal must be told, so
/// that `core_clock / flash_frequency` reproduces the divider the
/// bootloader already programmed. The 80 MHz source is divided by 1/2/3/4;
/// 26.67 MHz (DIV_3) has no `FlashFreq` spelling, so it takes the
/// conservative branch with everything unrecognized.
fn flash_freq_from_header(spi_speed: u8) -> FlashFreq {
    match spi_speed & 0x0f {
        0xf => FlashFreq::FlashFreq80m, // DIV_1
        0x0 => FlashFreq::FlashFreq40m, // DIV_2
        _ => FlashFreq::FlashFreq20m,   // DIV_4, DIV_3, or unreadable
    }
}

/// Read `spi_speed` out of the app image header at flash offset 0.
///
/// `read_nor` into a word-aligned stack buffer, the same discipline
/// `ota::read_guard_raw` and `assets::read_chunk` use — never
/// `FlashStorage::read`, which puts a 4 KiB bounce buffer on the caller's
/// stack (.claude/rules/firmware.md).
fn image_spi_speed(flash: &mut FlashStorage<'static>) -> Option<u8> {
    let mut stage = [0u32; 1];
    // SAFETY: 4 bytes of a live, word-aligned u32.
    let bytes = unsafe { core::slice::from_raw_parts_mut(stage.as_mut_ptr().cast::<u8>(), 4) };
    if flash.read_nor(0, bytes).is_err() {
        return None;
    }
    if bytes[0] != 0xe9 {
        return None; // not an ESP image header — don't touch the clock
    }
    Some(bytes[3])
}

/// Bring up PSRAM and route pattern-array allocations to it.
///
/// Idempotent-by-construction: it consumes the `PSRAM` peripheral
/// singleton, so it cannot be called twice. A board with no PSRAM fitted,
/// or a failed init, leaves the hook uninstalled and every arena
/// allocation on the main heap exactly as before — this is never fatal.
pub fn init(peri: PSRAM<'static>, flash: Option<&mut FlashStorage<'static>>) {
    let flash_frequency = match flash.and_then(image_spi_speed) {
        Some(s) => flash_freq_from_header(s),
        None => FlashFreq::FlashFreq20m,
    };
    let cfg = PsramConfig {
        // The module is an N16R8: octal, 8 MB. Naming the mode is more
        // reliable than esp-hal's auto-detect (its own docs say so) and
        // skips a failed quad probe.
        mode: PsramMode::OctalSpi,
        size: PsramSize::AutoDetect,
        // None → 80 MHz SPI core clock (esp-hal's choice for 40 MHz PSRAM),
        // which is what the bootloader already runs.
        core_clock: None,
        flash_frequency,
        ram_frequency: SpiRamFreq::Freq40m,
    };
    let psram = Psram::new(peri, cfg);
    let (start, size) = psram.raw_parts();
    // The mapping is permanent; the handle only owns the peripheral.
    core::mem::forget(psram);
    if size == 0 {
        println!("psram: not present (arena stays on the main heap)");
        return;
    }
    // SAFETY: the extent comes straight from esp-hal's own mapping, is
    // 'static (never unmapped), and nothing else in the firmware ever
    // addresses it — the arena is the only owner.
    unsafe {
        ARENA.add_region(HeapRegion::new(
            start,
            size,
            MemoryCapability::External.into(),
        ));
    }
    START.store(start as usize, Ordering::Relaxed);
    END.store(start as usize + size, Ordering::Release);
    // SAFETY: called once, at boot, before any Vm exists; `dealloc` below
    // routes by address so it accepts blocks from either heap, which is
    // exactly the contract luxel_core::arena::install asks for.
    unsafe { luxel_core::arena::install(alloc, dealloc) };
    println!("psram: {} B arena at {:p}", size, start);
}

/// Is `ptr` inside the arena? `END == 0` (no arena) makes this always false.
#[inline]
fn in_arena(ptr: *mut u8) -> bool {
    let a = ptr as usize;
    a >= START.load(Ordering::Relaxed) && a < END.load(Ordering::Acquire)
}

/// Arena allocation hook. Falls back to the main heap when the arena is
/// full so a pattern degrades the way it always did (byte-budget vmerr)
/// instead of failing differently.
unsafe fn alloc(layout: Layout) -> *mut u8 {
    let p = unsafe { GlobalAlloc::alloc(&ARENA, layout) };
    if p.is_null() {
        unsafe { alloc::alloc::alloc(layout) }
    } else {
        p
    }
}

/// Paired free. Routes on the address, not on bookkeeping — see `alloc`.
unsafe fn dealloc(ptr: *mut u8, layout: Layout) {
    if in_arena(ptr) {
        unsafe { GlobalAlloc::dealloc(&ARENA, ptr, layout) }
    } else {
        unsafe { alloc::alloc::dealloc(ptr, layout) }
    }
}

/// Zeroed allocation for a large buffer that is only ever touched from task
/// context — the HUB75 spare-plane staging framebuffer (Gitea #610). Arena
/// first, main heap if the arena is absent or full, exactly like the array
/// hook; the caller never frees it. Null when neither has room.
pub fn alloc_bulk_zeroed(layout: Layout) -> *mut u8 {
    let p = unsafe { GlobalAlloc::alloc_zeroed(&ARENA, layout) };
    if p.is_null() {
        unsafe { alloc::alloc::alloc_zeroed(layout) }
    } else {
        p
    }
}

/// Executable-code allocation for the JIT (Gitea #665, docs/jit-design.md
/// §5). Arena ONLY — never the main heap: the caller executes this memory
/// through the IBUS mirror of the PSRAM window, and a main-heap block
/// would need a different alias entirely (`jit.rs` handles that case
/// itself). Null when there is no arena or it is full, and the JIT then
/// reports `no-buffer` and interprets.
///
/// The address returned is the DBUS (data) address, byte-writable through
/// the data cache. Cache-line aligned (the S3's data cache line is 32 B;
/// 64 keeps it true for either configuration) so the write-back after
/// filling it never touches a neighbouring allocation's line, and so the
/// invalidate on the instruction side covers whole lines.
pub fn alloc_exec(len: usize) -> *mut u8 {
    let Ok(layout) = Layout::from_size_align(len.max(4), EXEC_ALIGN) else {
        return core::ptr::null_mut();
    };
    if END.load(Ordering::Acquire) == 0 {
        return core::ptr::null_mut();
    }
    unsafe { GlobalAlloc::alloc(&ARENA, layout) }
}

/// Free an [`alloc_exec`] block. `len` must be what was asked for.
///
/// # Safety
/// `ptr` must have come from [`alloc_exec`] with this `len`, and nothing
/// may be executing from it any more (the `NativeProgram` that owned it
/// is dropped with its engine, which is what guarantees that).
pub unsafe fn free_exec(ptr: *mut u8, len: usize) {
    if let Ok(layout) = Layout::from_size_align(len.max(4), EXEC_ALIGN) {
        unsafe { GlobalAlloc::dealloc(&ARENA, ptr, layout) }
    }
}

/// Alignment of [`alloc_exec`] blocks — see there.
pub const EXEC_ALIGN: usize = 64;

/// `(free, total)` bytes of the arena — `None` when there is no arena.
/// Reported by `/api/status` as `psram_free` / `psram_total`.
pub fn stats() -> Option<(usize, usize)> {
    let end = END.load(Ordering::Acquire);
    if end == 0 {
        return None;
    }
    Some((ARENA.free(), end - START.load(Ordering::Relaxed)))
}
