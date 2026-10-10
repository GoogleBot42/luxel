//! The frame-store microbench (Gitea #958). Feature `storebench`, S3 +
//! `psram-arena` only, off in every shipped image.
//!
//! The question it answers: why does writing a 16,384-px RGB888 frame into
//! the PSRAM arena cost ~90 cycles per pixel? Either every byte store goes
//! out to the bus (write-through, or uncached), in which case 32-bit stores
//! cut it ~3x, or the stores hit a write-back cache and the cost is the
//! line traffic (a fill on every write-allocate miss plus the eviction
//! write-back), in which case store width barely matters.
//!
//! So it times the same 49,152 bytes written as `[u8; 3]` pixels (three
//! `s8i`, exactly what the engine's `frame[i] = px` compiles to) and as
//! three `u32`s per four pixels, in internal SRAM, in ONE PSRAM buffer
//! that fits the 64 KB data cache (`hot`: every line resident after the
//! warm-up), and cycling through four of them (`cold`: 192 KB, every line a
//! miss), plus a read-only word sweep of each PSRAM case (the fill side
//! alone). Best-of / median-of `REPS` with interrupts masked, cycle counter,
//! at boot on core 0 and again as core 1's first job — before WiFi and the
//! panel driver, so nothing else is on the cache. Published under
//! `/api/status` `"storebench"` because opening the S3's USB console resets
//! it.

use core::alloc::Layout;
use core::hint::black_box;
use core::sync::atomic::{AtomicU32, Ordering};

use esp_println::println;

/// ROM `struct autoload_config` (esp32s3 rom/cache.h).
#[repr(C)]
struct AutoloadConfig {
    order: u8,
    trigger: u8,
    ena0: u8,
    ena1: u8,
    addr0: u32,
    size0: u32,
    addr1: u32,
    size1: u32,
}

unsafe extern "C" {
    fn Cache_Get_DCache_Line_Size() -> u32;
    fn Cache_Config_DCache_Autoload(cfg: *const AutoloadConfig) -> u32;
    fn Cache_Enable_DCache_Autoload();
    fn Cache_Disable_DCache_Autoload();
}

/// Pixels per sweep: the 128x128 wall.
const PX: usize = 16_384;
const BYTES: usize = PX * 3;
/// Internal SRAM is smaller: a quarter frame, scaled to `PX` in the report.
const SRAM_PX: usize = PX / 4;
/// PSRAM buffers the cold case cycles through (4 x 48 KB, past the cache).
const COLD: usize = 4;
const REPS: usize = 8;
const WARM: usize = 2;

/// Cell order, as reported.
const NAMES: [&str; 11] = [
    "sram_bytes",
    "sram_words",
    "hot_bytes",
    "hot_words",
    "cold_bytes",
    "cold_words",
    "hot_read",
    "cold_read",
    "auto_cold_bytes",
    "auto_cold_words",
    "auto_cold_read",
];
const FLAGS: usize = 2 * NAMES.len();
/// [core][2i, 2i+1] = (median, min) cycles per `PX` pixels for cell i;
/// [core][FLAGS] = ran | same << 1; [core][FLAGS + 1] = the autoload
/// control register as found (low half) and as armed (high half).
static CELLS: [[AtomicU32; FLAGS + 2]; 2] = [const { [const { AtomicU32::new(0) }; FLAGS + 2] }; 2];

/// EXTMEM data-cache autoload control (hardware prefetch on a miss), read
/// before and after the ROM arms it so the report shows it took.
const AUTOLOAD_CTRL: *mut u32 = 0x600c_404c as *mut u32;

#[inline(always)]
fn cycles() -> u32 {
    esp_hal::xtensa_lx::timer::get_cycle_count()
}

/// Time `f(k)` (k = call number) with interrupts masked; (median, min).
fn time(mut f: impl FnMut(usize)) -> (u32, u32) {
    let mut k = 0;
    for _ in 0..WARM {
        f(k);
        k += 1;
    }
    let mut s = [0u32; REPS];
    for x in s.iter_mut() {
        let mask = esp_hal::xtensa_lx::interrupt::disable();
        let t0 = cycles();
        f(k);
        *x = cycles().wrapping_sub(t0);
        unsafe { esp_hal::xtensa_lx::interrupt::set_mask(mask) };
        k += 1;
    }
    s.sort_unstable();
    (s[REPS / 2], s[0])
}

/// The engine's store: one `[u8; 3]` per pixel (three `s8i`; `[u8; 3]` has
/// alignment 1, so LLVM cannot merge them). The value varies per pixel so
/// nothing folds the loop into a memset.
#[inline(never)]
fn store_bytes(buf: &mut [[u8; 3]], seed: u8) {
    let g = black_box(seed);
    for (i, p) in buf.iter_mut().enumerate() {
        *p = [i as u8, g, (i >> 8) as u8];
    }
}

/// The same bytes as words: four pixels = three `u32` (`s32i`).
#[inline(never)]
fn store_words(buf: &mut [u32], seed: u8) {
    let g = u32::from(black_box(seed));
    for (q, w) in buf.chunks_exact_mut(3).enumerate() {
        let i = (q * 4) as u32;
        let (a, b, c, d) = (i, i + 1, i + 2, i + 3);
        // pixel n = [n as u8, g, (n >> 8) as u8], little-endian
        let hi = (i >> 8) & 0xff; // the four pixels of a group share n >> 8
        w[0] = (a & 0xff) | (g << 8) | (hi << 16) | ((b & 0xff) << 24);
        w[1] = g | (hi << 8) | ((c & 0xff) << 16) | (g << 24);
        w[2] = hi | ((d & 0xff) << 8) | (g << 16) | (hi << 24);
    }
}

#[inline(never)]
fn read_words(buf: &[u32]) -> u32 {
    buf.iter().fold(0u32, |a, &w| a.wrapping_add(w))
}

/// A 16-byte-aligned block: the arena when `psram`, else the global heap.
/// Never freed (a boot-time bench in a non-shipping image).
fn block(bytes: usize, psram: bool) -> Option<&'static mut [u32]> {
    let layout = Layout::from_size_align(bytes, 16).ok()?;
    let p = if psram {
        crate::psram::alloc_bulk_zeroed(layout)
    } else {
        unsafe { alloc::alloc::alloc_zeroed(layout) }
    };
    if p.is_null() {
        return None;
    }
    // The arena falls back to the global heap when it is full; a "PSRAM"
    // cell measured in SRAM would be a lie.
    let in_psram = (p as usize) >= 0x3c00_0000 && (p as usize) < 0x3e00_0000;
    if psram != in_psram {
        println!("storebench: block at {:p} is not where it was asked for", p);
        return None;
    }
    // SAFETY: `bytes` zeroed bytes, 16-aligned, owned for 'static.
    Some(unsafe { core::slice::from_raw_parts_mut(p.cast::<u32>(), bytes / 4) })
}

fn as_px(w: &mut [u32]) -> &mut [[u8; 3]] {
    // SAFETY: same bytes, alignment 1, length a multiple of 3 words.
    unsafe { core::slice::from_raw_parts_mut(w.as_mut_ptr().cast::<[u8; 3]>(), w.len() * 4 / 3) }
}

/// Run the bench on the calling core and record the cells.
pub fn run(core: u8) {
    let mut r = [(0u32, 0u32); NAMES.len()];
    let Some(sram) = block(SRAM_PX * 3, false) else {
        println!("storebench: no SRAM");
        return;
    };
    let Some(hot) = block(BYTES, true) else {
        println!("storebench: no PSRAM");
        return;
    };
    let mut cold: [Option<&'static mut [u32]>; COLD] = [None, None, None, None];
    for c in cold.iter_mut() {
        *c = block(BYTES, true);
        if c.is_none() {
            println!("storebench: no PSRAM for the cold set");
            return;
        }
    }
    let x4 = |(m, n): (u32, u32)| (m * 4, n * 4);
    r[0] = x4(time(|k| store_bytes(as_px(sram), k as u8)));
    r[1] = x4(time(|k| store_words(sram, k as u8)));
    r[2] = time(|k| store_bytes(as_px(hot), k as u8));
    r[3] = time(|k| store_words(hot, k as u8));
    r[4] = time(|k| store_bytes(as_px(cold[k % COLD].as_deref_mut().unwrap()), k as u8));
    r[5] = time(|k| store_words(cold[k % COLD].as_deref_mut().unwrap(), k as u8));
    r[6] = time(|_| {
        black_box(read_words(hot));
    });
    r[7] = time(|k| {
        black_box(read_words(cold[k % COLD].as_deref().unwrap()));
    });
    // The cold cells again with autoload on over the whole data-bus window
    // through the ROM (section 0 = the 32 MB data-bus window, trigger on
    // miss, ascending), then off again — boot leaves it off.
    let found = unsafe { AUTOLOAD_CTRL.read_volatile() };
    let cfg = AutoloadConfig {
        order: 0,
        trigger: 0,
        ena0: 1,
        ena1: 0,
        addr0: 0x3c00_0000,
        size0: 0x0200_0000,
        addr1: 0,
        size1: 0,
    };
    unsafe {
        Cache_Disable_DCache_Autoload();
        Cache_Config_DCache_Autoload(&cfg);
        Cache_Enable_DCache_Autoload();
        // section 0 by hand as well, in case the ROM left it off
        (0x600c_4050 as *mut u32).write_volatile(0x3c00_0000);
        (0x600c_4054 as *mut u32).write_volatile(0x0200_0000);
        AUTOLOAD_CTRL.write_volatile(AUTOLOAD_CTRL.read_volatile() | 1);
    }
    let armed = unsafe { AUTOLOAD_CTRL.read_volatile() };
    r[8] = time(|k| store_bytes(as_px(cold[k % COLD].as_deref_mut().unwrap()), k as u8));
    r[9] = time(|k| store_words(cold[k % COLD].as_deref_mut().unwrap(), k as u8));
    r[10] = time(|k| {
        black_box(read_words(cold[k % COLD].as_deref().unwrap()));
    });
    unsafe { Cache_Disable_DCache_Autoload() };
    // byte and word kernels must agree, or the word cells timed other work
    store_bytes(as_px(sram), 7);
    let want: alloc::vec::Vec<u32> = sram.to_vec();
    store_words(sram, 7);
    let same = want == *sram;

    let c = &CELLS[usize::from(core) & 1];
    for (i, (m, n)) in r.iter().enumerate() {
        c[2 * i].store(*m, Ordering::Relaxed);
        c[2 * i + 1].store(*n, Ordering::Relaxed);
    }
    c[FLAGS + 1].store(found | (armed << 16), Ordering::Relaxed);
    c[FLAGS].store(1 | (u32::from(same) << 1), Ordering::Release);
    let pp = |v: u32| v as f32 / PX as f32;
    println!(
        "storebench core {}: cycles/px (median) {} — kernels agree: {}",
        core,
        NAMES
            .iter()
            .zip(r.iter())
            .map(|(n, (m, _))| alloc::format!("{n} {:.1}", pp(*m)))
            .collect::<alloc::vec::Vec<_>>()
            .join(", "),
        same
    );
}

/// The `/api/status` block: `"storebench":[{core, ran, same, px, <cell>:[median,min]…}, …]`,
/// cycles per `px` pixels (divide for cycles/px).
pub fn status_json(out: &mut impl core::fmt::Write) -> core::fmt::Result {
    write!(out, "[")?;
    for core in 0..2usize {
        let c = &CELLS[core];
        let flags = c[FLAGS].load(Ordering::Acquire);
        if core == 1 {
            write!(out, ",")?;
        }
        write!(
            out,
            "{{\"core\":{},\"ran\":{},\"same\":{},\"px\":{},\"autoload_ctrl\":{},\"line\":{}",
            core,
            flags & 1 != 0,
            flags & 2 != 0,
            PX,
            c[FLAGS + 1].load(Ordering::Relaxed),
            unsafe { Cache_Get_DCache_Line_Size() }
        )?;
        for (i, n) in NAMES.iter().enumerate() {
            write!(
                out,
                ",\"{}\":[{},{}]",
                n,
                c[2 * i].load(Ordering::Relaxed),
                c[2 * i + 1].load(Ordering::Relaxed)
            )?;
        }
        write!(out, "}}")?;
    }
    write!(out, "]")
}
