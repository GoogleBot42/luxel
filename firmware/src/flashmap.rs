//! Read-only memory mapping of external flash through the cache MMU.
//!
//! Every chip we build for executes the app from flash through a cache
//! (SPI0) whose page table the bootloader fills with the app's IROM/DROM
//! pages. The table has far more entries than the app uses, so any other
//! page-aligned flash range can be mapped into the data bus the same way
//! and read as ordinary memory — no esp-storage `read_nor` copy, no
//! flash-controller (SPI1) transaction, no critical section per read, and
//! no heap staging buffer. Consumers: the web assets partition (assets.rs,
//! Gitea #259), and the pattern engine's execution-ready bytecode (#260).
//! Design, per-chip page arithmetic and the fence/OTA interactions:
//! docs/research/flash-mmap.md; the consumer rules: docs/firmware.md
//! "Flash-mapped regions".
//!
//! Mechanism per chip (all register-level, mirroring esp-idf's `mmu_ll.h`
//! and esp-storage's own private `mmu.rs`; only the cache-maintenance
//! calls go through ROM):
//!
//! | chip  | table                          | entries | window            | page |
//! |-------|--------------------------------|---------|-------------------|------|
//! | ESP32 | DPORT PRO 0x3FF10000 + APP 0x3FF12000 | 64 (DROM0) | 0x3F400000 | 64 K |
//! | S3    | 0x600C5000 (shared I/D)        | 512     | 0x3C000000 (DBUS) | 64 K |
//! | C3    | 0x600C5000 (shared I/D)        | 128     | 0x3C000000 (DBUS) | 64 K |
//! | C6    | SPI_MEM0 MMU_ITEM_INDEX/CONTENT| 256     | 0x42000000 (I+D)  | 8–64 K (register) |
//!
//! Mapping = pick a run of invalid entries (first fit, above the app's own
//! pages), program them, then make the cache coherent: a whole-cache flush
//! on the ESP32 (its only primitive — also what Espressif's QEMU needs to
//! re-sync a page), a suspend/program/resume plus an address-range
//! invalidate on the S3/C3/C6 (the esp-idf `esp_mmu_map` sequence).
//!
//! **Invariants a consumer must keep** (the doc has the reasoning):
//! - A mapped read is a cache miss to SPI0. It must never happen while an
//!   SPI1 op (any `ota::with_flash` / esp-storage call) is in flight on
//!   ANOTHER core: on dual-core builds every flash op parks the other core
//!   (core1.rs' fence) — mapped reads in task context are therefore safe by
//!   construction; never read a mapping from an interrupt handler.
//! - Writing flash under a mapping is fine (the assets upload does it), but
//!   the writer must call [`invalidate`] on the range before anyone reads it
//!   back through the mapping — cache lines do not watch SPI1.
//! - Never map an app slot, and never [`unmap`] a region another task may
//!   still be reading: a load from an invalid entry is a cache-error fault,
//!   not a bus error you can catch.
//! - Only page-aligned partition offsets can be mapped; lengths round up to
//!   whole pages, which is why the partition table keeps every mappable
//!   region 64 KiB-aligned.
//!
//! `flashmap-off` (cargo feature) makes [`map`] always fail, so every
//! consumer's read_nor fallback path ships and can be forced for a bisect.

// hosted-ui builds have no assets to map yet and the VM consumer (#260)
// lands separately — most of the API is unused in some configurations.
#![allow(dead_code)]

use core::sync::atomic::{AtomicBool, Ordering};

/// Why a range could not be mapped. All are answered by the consumer's
/// read_nor path — a failed map is a slower boot, never a broken one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Offset is not a multiple of the chip's MMU page size.
    Unaligned,
    /// Zero length.
    Empty,
    /// The window has no run of free entries long enough.
    NoFreePages,
    /// Built with `flashmap-off`.
    Disabled,
}

/// A live mapping: `pages` consecutive MMU entries from `entry`, presenting
/// flash `[phys, phys + pages * page)` at `vaddr`. Dropping the handle does
/// NOT unmap (a leaked mapping is the common case — a partition mapped at
/// boot for the life of the firmware); [`unmap`] does.
pub struct Mapped {
    vaddr: usize,
    len: usize,
    phys: u32,
    entry: u32,
    pages: u32,
}

impl Mapped {
    /// The mapped bytes. The slice lives exactly as long as the mapping —
    /// use [`Mapped::leak`] for a permanent one.
    pub fn bytes(&self) -> &[u8] {
        // SAFETY: the entries were programmed by `map` and are only ever
        // invalidated by `unmap`, which consumes the handle.
        unsafe { core::slice::from_raw_parts(self.vaddr as *const u8, self.len) }
    }

    /// Give up the ability to unmap in exchange for a `'static` slice.
    pub fn leak(self) -> &'static [u8] {
        unsafe { core::slice::from_raw_parts(self.vaddr as *const u8, self.len) }
    }

    pub fn vaddr(&self) -> usize {
        self.vaddr
    }

    pub fn phys(&self) -> u32 {
        self.phys
    }

    pub fn pages(&self) -> u32 {
        self.pages
    }

    pub fn first_entry(&self) -> u32 {
        self.entry
    }
}

/// The chip's MMU page size in bytes (a register on the C6, 64 KiB
/// elsewhere).
pub fn page_size() -> u32 {
    chip::page_size()
}

/// True once any mapping has been made this boot (status reporting).
static ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn any_mapped() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// Map `len` bytes of flash starting at page-aligned `offset` read-only
/// into the data bus. Whole pages are mapped; the returned slice is
/// exactly `len` bytes.
pub fn map(offset: u32, len: u32) -> Result<Mapped, Error> {
    if cfg!(feature = "flashmap-off") {
        return Err(Error::Disabled);
    }
    if len == 0 {
        return Err(Error::Empty);
    }
    let page = chip::page_size();
    if offset % page != 0 {
        return Err(Error::Unaligned);
    }
    let pages = len.div_ceil(page);
    // Scan + program under one critical section so two mappers can't pick
    // the same run. See `quiesced` for the dual-core half of the story.
    let entry = quiesced(|| {
        let entry = chip::find_free_run(pages)?;
        chip::program(entry, offset, pages);
        Ok(entry)
    })?;
    let vaddr = chip::VADDR_BASE + (entry * page) as usize;
    // Drop any lines a previous mapping of these entries left behind.
    chip::invalidate(vaddr, pages * page);
    ACTIVE.store(true, Ordering::Relaxed);
    Ok(Mapped {
        vaddr,
        len: len as usize,
        phys: offset,
        entry,
        pages,
    })
}

/// Release a mapping. The caller guarantees nothing reads it any more.
pub fn unmap(m: Mapped) {
    let page = chip::page_size();
    quiesced(|| chip::unprogram(m.entry, m.pages));
    chip::invalidate(m.vaddr, m.pages * page);
}

/// Make `len` bytes at `rel` (relative to the mapping's start) coherent
/// after the flash underneath was written through esp-storage. On the
/// ESP32 this is a whole-cache flush of both cores (the chip has no
/// address-range primitive); on the others an address-range invalidate.
pub fn invalidate(m: &Mapped, rel: u32, len: u32) {
    let rel = rel as usize;
    let len = (len as usize).min(m.len.saturating_sub(rel));
    if len == 0 {
        return;
    }
    chip::invalidate(m.vaddr + rel, len as u32);
}

/// Same, for a leaked mapping addressed by its slice.
pub fn invalidate_slice(bytes: &[u8]) {
    if !bytes.is_empty() {
        chip::invalidate(bytes.as_ptr() as usize, bytes.len() as u32);
    }
}

/// Run an MMU table operation with nothing else touching the cache: a
/// critical section on this core. On dual-core chips the OTHER core must be
/// parked too — the ESP32's DPORT MMU registers must not be read while the
/// AppCpu runs (the silicon DPORT-read hazard esp-idf wraps every
/// `cache_flash_mmu_set` in `DPORT_STALL_OTHER_CPU` for) and a whole-cache
/// flush must not race a core executing from flash. That is exactly what
/// core1.rs' flash fence does for esp-storage ops, so on dual-core boards
/// every table operation runs inside it (Gitea #272): the other core is
/// parked in an IRAM spin — no DPORT reads racing ours, no code or mapped
/// fetch under the flush — and the critical section on THIS core still
/// keeps its own ISRs out. Fence outside, critical section inside, same
/// order as `ota::with_flash` (the fence's spin-waits need interrupts
/// enabled). Single-core builds: `fenced` is the identity.
#[inline(always)]
fn quiesced<R>(f: impl FnOnce() -> R) -> R {
    crate::core1::fenced_as(crate::core1::tag::MAP, || critical_section::with(|_| f()))
}

// ---------------------------------------------------------------------------
// classic ESP32: DPORT tables (one per core), DROM0 window, whole-cache flush

#[cfg(feature = "esp32")]
mod chip {
    use super::Error;

    pub const VADDR_BASE: usize = 0x3F40_0000;
    const PAGE: u32 = 0x1_0000;
    /// DROM0 = the first 64 entries of each core's 256-entry table
    /// (0x3F400000..0x3F800000); the rest is IRAM0/IROM0, which supports
    /// only 32-bit loads and is not usable for byte data.
    const ENTRIES: u32 = 64;
    const PRO_TABLE: usize = 0x3FF1_0000;
    const APP_TABLE: usize = 0x3FF1_2000;
    const INVALID: u32 = 0x100;

    unsafe extern "C" {
        /// ROM: flush (invalidate all lines of) one core's cache. Legal with
        /// the cache enabled — esp-idf calls it after every flash write and
        /// esp-storage after every encrypted read; QEMU re-syncs the page
        /// contents on it.
        fn Cache_Flush_rom(cpu: u32);
    }

    pub fn page_size() -> u32 {
        PAGE
    }

    #[inline(always)]
    fn read(entry: u32) -> u32 {
        unsafe { (PRO_TABLE as *const u32).add(entry as usize).read_volatile() }
    }

    #[inline(always)]
    fn write_both(entry: u32, val: u32) {
        unsafe {
            (PRO_TABLE as *mut u32).add(entry as usize).write_volatile(val);
            (APP_TABLE as *mut u32).add(entry as usize).write_volatile(val);
        }
    }

    pub fn find_free_run(pages: u32) -> Result<u32, Error> {
        super::first_fit(pages, ENTRIES, |e| read(e) & INVALID == 0)
    }

    /// IRAM: a flush empties this core's cache, so nothing here may fetch
    /// from flash until it completes.
    #[esp_hal::ram]
    pub fn program(entry: u32, offset: u32, pages: u32) {
        let first = offset / PAGE;
        for k in 0..pages {
            write_both(entry + k, first + k);
        }
        unsafe {
            Cache_Flush_rom(0);
            Cache_Flush_rom(1);
        }
    }

    #[esp_hal::ram]
    pub fn unprogram(entry: u32, pages: u32) {
        for k in 0..pages {
            write_both(entry + k, INVALID);
        }
        unsafe {
            Cache_Flush_rom(0);
            Cache_Flush_rom(1);
        }
    }

    /// No address-range primitive on this chip: flush everything (both
    /// cores — the APP table mirrors the PRO one).
    pub fn invalidate(_vaddr: usize, _len: u32) {
        critical_section::with(|_| flush_all());
    }

    #[esp_hal::ram]
    fn flush_all() {
        unsafe {
            Cache_Flush_rom(0);
            Cache_Flush_rom(1);
        }
    }
}

// ---------------------------------------------------------------------------
// ESP32-S3 / ESP32-C3: one table for both buses, DBUS window at 0x3C000000

#[cfg(any(feature = "esp32s3", feature = "esp32c3"))]
mod chip {
    use super::Error;

    pub const VADDR_BASE: usize = 0x3C00_0000;
    const PAGE: u32 = 0x1_0000;
    #[cfg(feature = "esp32s3")]
    const ENTRIES: u32 = 512; // 32 MiB window
    #[cfg(feature = "esp32c3")]
    const ENTRIES: u32 = 128; // 8 MiB window
    const TABLE: usize = 0x600C_5000;
    #[cfg(feature = "esp32s3")]
    const INVALID: u32 = 1 << 14;
    #[cfg(feature = "esp32c3")]
    const INVALID: u32 = 1 << 8;

    unsafe extern "C" {
        fn Cache_Invalidate_Addr(addr: u32, size: u32) -> i32;
        #[cfg_attr(feature = "esp32s3", link_name = "rom_Cache_Suspend_ICache")]
        fn Cache_Suspend_ICache() -> u32;
        fn Cache_Resume_ICache(autoload: u32);
        #[cfg(feature = "esp32s3")]
        fn Cache_Suspend_DCache() -> u32;
        #[cfg(feature = "esp32s3")]
        fn Cache_Resume_DCache(autoload: u32);
    }

    pub fn page_size() -> u32 {
        PAGE
    }

    #[inline(always)]
    fn read(entry: u32) -> u32 {
        unsafe { (TABLE as *const u32).add(entry as usize).read_volatile() }
    }

    #[inline(always)]
    fn write(entry: u32, val: u32) {
        unsafe { (TABLE as *mut u32).add(entry as usize).write_volatile(val) }
    }

    pub fn find_free_run(pages: u32) -> Result<u32, Error> {
        // an S3 entry with bit 15 set maps PSRAM — still occupied
        super::first_fit(pages, ENTRIES, |e| read(e) & INVALID == 0)
    }

    /// IRAM: the caches are suspended while the table changes (the esp-idf
    /// `mmu_hal_map_region` discipline), so nothing here may fetch from
    /// flash — ROM calls and volatile stores only.
    #[esp_hal::ram]
    pub fn program(entry: u32, offset: u32, pages: u32) {
        let first = offset / PAGE;
        unsafe {
            let i = Cache_Suspend_ICache();
            #[cfg(feature = "esp32s3")]
            let d = Cache_Suspend_DCache();
            for k in 0..pages {
                write(entry + k, first + k);
            }
            #[cfg(feature = "esp32s3")]
            Cache_Resume_DCache(d);
            Cache_Resume_ICache(i);
        }
    }

    #[esp_hal::ram]
    pub fn unprogram(entry: u32, pages: u32) {
        unsafe {
            let i = Cache_Suspend_ICache();
            #[cfg(feature = "esp32s3")]
            let d = Cache_Suspend_DCache();
            for k in 0..pages {
                write(entry + k, INVALID);
            }
            #[cfg(feature = "esp32s3")]
            Cache_Resume_DCache(d);
            Cache_Resume_ICache(i);
        }
    }

    pub fn invalidate(vaddr: usize, len: u32) {
        unsafe {
            Cache_Invalidate_Addr(vaddr as u32, len);
        }
    }
}

// ---------------------------------------------------------------------------
// ESP32-C6: indexed table registers on SPI_MEM0, one window for both buses,
// page size chosen by the bootloader

#[cfg(feature = "esp32c6")]
mod chip {
    use super::Error;

    pub const VADDR_BASE: usize = 0x4200_0000;
    const ENTRIES: u32 = 256;
    const SPI_MEM0: usize = 0x6000_2000;
    const ITEM_CONTENT: usize = 0x37C;
    const ITEM_INDEX: usize = 0x380;
    const POWER_CTRL: usize = 0x384;
    const VALID: u32 = 1 << 9;
    const SENSITIVE: u32 = 1 << 10;

    unsafe extern "C" {
        fn Cache_Invalidate_Addr(addr: u32, size: u32) -> i32;
        fn Cache_Suspend_ICache() -> u32;
        fn Cache_Resume_ICache(autoload: u32);
    }

    /// MMU_POWER_CTRL bits [4:3]: 0 = 64 K, 1 = 32 K, 2 = 16 K, 3 = 8 K.
    pub fn page_size() -> u32 {
        let ctrl = unsafe { ((SPI_MEM0 + POWER_CTRL) as *const u32).read_volatile() };
        match (ctrl >> 3) & 3 {
            0 => 0x1_0000,
            1 => 0x8000,
            2 => 0x4000,
            _ => 0x2000,
        }
    }

    #[inline(always)]
    fn read(entry: u32) -> u32 {
        unsafe {
            ((SPI_MEM0 + ITEM_INDEX) as *mut u32).write_volatile(entry);
            ((SPI_MEM0 + ITEM_CONTENT) as *const u32).read_volatile()
        }
    }

    #[inline(always)]
    fn write(entry: u32, val: u32) {
        unsafe {
            ((SPI_MEM0 + ITEM_INDEX) as *mut u32).write_volatile(entry);
            ((SPI_MEM0 + ITEM_CONTENT) as *mut u32).write_volatile(val);
        }
    }

    pub fn find_free_run(pages: u32) -> Result<u32, Error> {
        super::first_fit(pages, ENTRIES, |e| read(e) & VALID != 0)
    }

    #[esp_hal::ram]
    pub fn program(entry: u32, offset: u32, pages: u32) {
        let page = page_size();
        let first = offset / page;
        let flags = if esp_hal::efuse::flash_encryption() {
            VALID | SENSITIVE
        } else {
            VALID
        };
        unsafe {
            let i = Cache_Suspend_ICache();
            for k in 0..pages {
                write(entry + k, (first + k) | flags);
            }
            Cache_Resume_ICache(i);
        }
    }

    #[esp_hal::ram]
    pub fn unprogram(entry: u32, pages: u32) {
        unsafe {
            let i = Cache_Suspend_ICache();
            for k in 0..pages {
                write(entry + k, 0);
            }
            Cache_Resume_ICache(i);
        }
    }

    pub fn invalidate(vaddr: usize, len: u32) {
        unsafe {
            Cache_Invalidate_Addr(vaddr as u32, len);
        }
    }
}

/// First run of `pages` invalid entries in `[0, entries - 1)`. The last
/// entry is skipped: the esp-idf bootloader keeps it for its own flash
/// reads and esp-storage's encrypted-read path assumes the same.
fn first_fit(pages: u32, entries: u32, valid: impl Fn(u32) -> bool) -> Result<u32, Error> {
    let limit = entries.saturating_sub(1);
    let mut run_start = 0;
    let mut run = 0;
    for e in 0..limit {
        if valid(e) {
            run = 0;
            run_start = e + 1;
        } else {
            run += 1;
            if run == pages {
                return Ok(run_start);
            }
        }
    }
    Err(Error::NoFreePages)
}
