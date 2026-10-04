//! ESP32-S3 data-cache write-back that is safe beside a running second core.
//!
//! The S3 has a hardware erratum around a MANUAL write-back (the
//! `EXTMEM_DCACHE_SYNC_CTRL` path every "push my PSRAM bytes out so the
//! DMA / the instruction side sees them" caller uses): while the write-back
//! runs, an access to a line being written back from the other core, or from
//! an interrupt on the writing core, can produce a cache hit error — the
//! access completes with the wrong data. ESP-IDF ships a software patch for
//! its ROM `Cache_WriteBack_Addr` (`esp_rom_cache_writeback_esp32s3.S`): mask
//! interrupts, FREEZE the data cache so the other core stalls on a miss
//! instead of touching the line, trigger the write-back by hand, unfreeze.
//! esp-hal links the raw ROM routine, so anything that calls
//! `rom_Cache_WriteBack_Addr` here has been running the erratum.
//!
//! Found 2026-10-04 on the Seengreat 2x2 chain (16384 px): a full-library
//! soak hit `Exception occurred on AppCpu 'Illegal'` inside freshly published
//! JIT code every ~40 pattern pushes (7 in 308), with the ISA-model
//! differential gate green for every one of those patterns — the bytes
//! were right, the way they reached the CPU was not. Both write-back sites
//! race the other core: the JIT publishes on the render core while core 0
//! packs frames out of PSRAM, and the ring packer writes back on core 0 four
//! times a frame while the render core reads its arrays and literal pool.
//!
//! This port freezes for the WHOLE range, not only IDF's partial edge lines:
//! the cost is the other core stalling on data-cache misses for the length
//! of one write-back (a few hundred µs for a JIT image, tens of µs for a
//! packer chunk), and the alternative — trusting that "the same line" means
//! "the same address" — is the gamble that just lost.
//!
//! Rules for the frozen window, straight from the IDF routine: the writing
//! core must not touch anything the data cache serves (PSRAM, flash rodata,
//! a literal in flash) or it stalls on its own freeze. So the inner routine
//! lives in IRAM, builds every constant in registers, and its only loads and
//! stores are the EXTMEM registers.

/// Write `len` bytes at `addr` (a PSRAM data-bus address) back from the
/// data cache, line-aligned outward, with the erratum workaround.
pub fn writeback(addr: *const u8, len: usize) {
    unsafe extern "C" {
        // esp32s3.rom.ld
        fn Cache_Get_DCache_Line_Size() -> u32;
        fn Cache_Suspend_DCache_Autoload() -> u32;
        fn Cache_Resume_DCache_Autoload(v: u32);
    }
    if len == 0 {
        return;
    }
    let line = match unsafe { Cache_Get_DCache_Line_Size() } as usize {
        0 => 32,
        l => l,
    };
    let a = addr as usize;
    let start = a & !(line - 1);
    let end = (a + len + line - 1) & !(line - 1);
    let items = ((end - start) / line) as u32;
    // Interrupts off on this core for the whole sequence (IDF: XCHAL_NMILEVEL);
    // autoload suspended so a line landing mid-operation is not swept in.
    let mask = esp_hal::xtensa_lx::interrupt::disable();
    let al = unsafe { Cache_Suspend_DCache_Autoload() };
    unsafe { writeback_items_frozen(start as u32, items) };
    unsafe { Cache_Resume_DCache_Autoload(al) };
    unsafe { esp_hal::xtensa_lx::interrupt::set_mask(mask) };
}

/// `cache_writeback_items_freeze` from ESP-IDF, in Rust inline asm.
///
/// EXTMEM is at `0x600C_4000`: `DCACHE_SYNC_CTRL` +0x28 (bit 1
/// `WRITEBACK_ENA`, bit 3 `SYNC_DONE`), `DCACHE_SYNC_ADDR` +0x2C,
/// `DCACHE_SYNC_SIZE` +0x30 (a count of cache lines), `DCACHE_FREEZE` +0x150
/// (bit 0 `ENA`, bit 1 `MODE` — 0 stalls the other core on a miss, 1 fakes a
/// hit; IDF uses 0 — bit 2 `DONE`). The base is assembled from two `movi`s so
/// the routine has no literal pool to fetch while the cache is frozen.
///
/// # Safety
/// `addr` must be line-aligned and in the data-cache window; interrupts must
/// be masked by the caller.
#[esp_hal::ram]
#[inline(never)]
unsafe fn writeback_items_frozen(addr: u32, items: u32) {
    unsafe {
        core::arch::asm!(
            "movi {b}, 0x600",
            "slli {b}, {b}, 20",
            "movi {t}, 0xC4",
            "slli {t}, {t}, 12",
            "add  {b}, {b}, {t}",
            // SYNC_ADDR / SYNC_SIZE, then make sure every earlier access landed
            "s32i {addr}, {b}, 0x2C",
            "s32i {items}, {b}, 0x30",
            "memw",
            // freeze: MODE = 0, ENA = 1; wait for DONE
            "l32i {t}, {b}, 0x150",
            "movi {u}, -3",
            "and  {t}, {t}, {u}",
            "movi {u}, 1",
            "or   {t}, {t}, {u}",
            "s32i {t}, {b}, 0x150",
            "1:",
            "l32i {t}, {b}, 0x150",
            "memw",
            "bbci {t}, 2, 1b",
            // write-back: WRITEBACK_ENA; wait for SYNC_DONE
            "l32i {t}, {b}, 0x28",
            "movi {u}, 2",
            "or   {t}, {t}, {u}",
            "s32i {t}, {b}, 0x28",
            "2:",
            "l32i {t}, {b}, 0x28",
            "memw",
            "bbci {t}, 3, 2b",
            // unfreeze: ENA = 0; wait for DONE to drop
            "l32i {t}, {b}, 0x150",
            "movi {u}, -2",
            "and  {t}, {t}, {u}",
            "s32i {t}, {b}, 0x150",
            "3:",
            "l32i {t}, {b}, 0x150",
            "memw",
            "bbsi {t}, 2, 3b",
            addr = in(reg) addr,
            items = in(reg) items,
            b = out(reg) _,
            t = out(reg) _,
            u = out(reg) _,
            options(nostack),
        );
    }
}
