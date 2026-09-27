//! The vector-packer microbench (Gitea #855; docs/hub75-ring-design.md §10
//! step 1). Feature `packbench`, S3 only, off in every shipped image.
//!
//! At boot — before WiFi, on core 0, and again on core 1 as the first thing
//! its executor runs — this times `luxel_hub75::pack` (today's scalar packer)
//! against `luxel_hub75::pie::pack_pie` (the PIE kernel) over ONE 256-column
//! row pair (512 px, the ring driver's unit of work) with the RGB source in
//! internal SRAM and again in the PSRAM arena, and checks the kernel's output
//! word-for-word against the scalar packer at four brightness levels. The
//! results go to serial and to `/api/status` under `"packbench"` — the USB
//! console resets the S3 when it is opened, so the HTTP copy is the one that
//! gets read.
//!
//! Every measurement is best-of and median-of `REPS` calls with interrupts
//! masked on the measuring core, read from the cycle counter, so the number
//! is the packer's own cost (plus PSRAM cache misses for the PSRAM rows)
//! and not the scheduler's. Cycles per pixel = cycles per row pair / 512.

use core::alloc::Layout;
use core::sync::atomic::{AtomicU32, Ordering};

use alloc::vec;
use esp_println::println;
use luxel_hub75::pie::{self, PairPads};
use luxel_hub75::{format, pack, Control, Geometry, Scratch, Tables};

/// The row pair under test: 256 columns (a 128x128 wall's chain), 7 planes.
const COLS: usize = 256;
const PLANES: usize = 7;
const G: Geometry = Geometry::new(1, COLS, PLANES);
/// Pixels in the row pair.
const PIXELS: u32 = (2 * COLS) as u32;
/// Timed calls per cell (after `WARM` untimed ones).
const REPS: usize = 32;
const WARM: usize = 4;

/// One core's results: cycles per row pair, median and best, per cell.
#[derive(Clone, Copy, Default)]
pub struct Row {
    pub ran: bool,
    pub core: u8,
    /// Scalar `pack`, RGB rows in internal SRAM: (median, min).
    pub sram_scalar: (u32, u32),
    /// PIE `pack_pie`, RGB rows in internal SRAM.
    pub sram_pie: (u32, u32),
    /// The gather alone (PIE path with zero planes), SRAM source.
    pub sram_gather: (u32, u32),
    /// Scalar `pack`, RGB rows in the PSRAM arena (0 = no arena on this board).
    pub psram_scalar: (u32, u32),
    /// PIE `pack_pie`, RGB rows in the PSRAM arena.
    pub psram_pie: (u32, u32),
    /// The gather alone, PSRAM source.
    pub psram_gather: (u32, u32),
    /// Every checked brightness packed word-identical to the scalar path.
    pub identical: bool,
    /// First mismatching word index (and the brightness), when not identical.
    pub mismatch: (u32, u8),
    /// Words compared (0 = the PIE path refused its preconditions).
    pub compared: u32,
    /// True when the assembly kernel is compiled in (else the portable model ran).
    pub vector_unit: bool,
}

/// Per-core result cells, plain atomics so `/api/status` reads them from
/// any task without a lock: [core][field].
const FIELDS: usize = 16;
static CELLS: [[AtomicU32; FIELDS]; 2] = [const { [const { AtomicU32::new(0) }; FIELDS] }; 2];

fn store(r: &Row) {
    let c = &CELLS[usize::from(r.core) & 1];
    let pairs = [
        r.sram_scalar,
        r.sram_pie,
        r.sram_gather,
        r.psram_scalar,
        r.psram_pie,
        r.psram_gather,
    ];
    for (i, (med, min)) in pairs.iter().enumerate() {
        c[2 * i].store(*med, Ordering::Relaxed);
        c[2 * i + 1].store(*min, Ordering::Relaxed);
    }
    c[12].store(u32::from(r.identical), Ordering::Relaxed);
    c[13].store(r.mismatch.0, Ordering::Relaxed);
    c[14].store((r.compared & 0x00ff_ffff) | (u32::from(r.mismatch.1) << 24), Ordering::Relaxed);
    c[15].store(1 | (u32::from(r.vector_unit) << 1), Ordering::Release);
}

/// The stored result for `core`, `ran == false` until that core's bench ran.
pub fn result(core: u8) -> Row {
    let c = &CELLS[usize::from(core) & 1];
    let flags = c[15].load(Ordering::Acquire);
    let pair = |i: usize| (c[2 * i].load(Ordering::Relaxed), c[2 * i + 1].load(Ordering::Relaxed));
    let packed = c[14].load(Ordering::Relaxed);
    Row {
        ran: flags & 1 != 0,
        core,
        sram_scalar: pair(0),
        sram_pie: pair(1),
        sram_gather: pair(2),
        psram_scalar: pair(3),
        psram_pie: pair(4),
        psram_gather: pair(5),
        identical: c[12].load(Ordering::Relaxed) != 0,
        mismatch: (c[13].load(Ordering::Relaxed), (packed >> 24) as u8),
        compared: packed & 0x00ff_ffff,
        vector_unit: flags & 2 != 0,
    }
}

/// A 16-byte-aligned, zeroed heap block of `u16`s, freed on drop.
struct Words {
    p: *mut u16,
    n: usize,
}

impl Words {
    fn new(n: usize) -> Option<Self> {
        let layout = Layout::from_size_align(n.max(8) * 2, 16).ok()?;
        let p = unsafe { alloc::alloc::alloc_zeroed(layout) }.cast::<u16>();
        (!p.is_null()).then_some(Self { p, n })
    }
    fn slice(&mut self) -> &mut [u16] {
        // SAFETY: `n` zeroed, aligned `u16`s we own.
        unsafe { core::slice::from_raw_parts_mut(self.p, self.n) }
    }
}

impl Drop for Words {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.n.max(8) * 2, 16).unwrap();
        unsafe { alloc::alloc::dealloc(self.p.cast(), layout) };
    }
}

/// Deterministic pixels: every channel byte value shows up, in every lane.
fn fill(rgb: &mut [[u8; 3]], seed: u64) {
    let mut s = seed;
    for px in rgb.iter_mut() {
        for c in px.iter_mut() {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            *c = (s >> 24) as u8;
        }
    }
    // and the edge values, tiled, so the extremes land at every column
    let edges = [0u8, 1, 127, 128, 254, 255];
    for (i, px) in rgb.iter_mut().enumerate().take(edges.len() * 6) {
        px[i % 3] = edges[i % edges.len()];
    }
}

#[inline(always)]
fn cycles() -> u32 {
    esp_hal::xtensa_lx::timer::get_cycle_count()
}

/// Time `f` `REPS` times with interrupts masked on this core; (median, min).
fn time(mut f: impl FnMut()) -> (u32, u32) {
    let mut samples = [0u32; REPS];
    for _ in 0..WARM {
        f();
    }
    for s in samples.iter_mut() {
        let mask = esp_hal::xtensa_lx::interrupt::disable();
        let t0 = cycles();
        f();
        let dt = cycles().wrapping_sub(t0);
        unsafe { esp_hal::xtensa_lx::interrupt::set_mask(mask) };
        *s = dt;
    }
    samples.sort_unstable();
    (samples[REPS / 2], samples[0])
}

/// Run the bench on the calling core and record the row.
pub fn run(core: u8) {
    let mut row = Row { core, vector_unit: pie::has_vector_unit(), ..Row::default() };
    let Some(mut dst_a) = Words::new(G.words()) else {
        println!("packbench: no heap for the framebuffer");
        return;
    };
    let Some(mut dst_b) = Words::new(G.words()) else {
        println!("packbench: no heap for the second framebuffer");
        return;
    };
    let mut sram = vec![[0u8; 3]; G.pixels()];
    fill(&mut sram, 0x2026_0927_0855 + u64::from(core));
    let mut tables = Tables::zeroed();
    let mut scratch = Scratch::for_geometry(G);
    let mut pads = PairPads::for_geometry(G);
    let ctrl = Control::default();

    // ---- correctness first: PIE vs scalar at four brightness levels ----
    row.identical = true;
    for b5 in [31u8, 14, 3, 0] {
        tables.build(&crate::hub75::brightness_lut(b5));
        let (want, got) = (dst_a.slice(), dst_b.slice());
        format(want, G, ctrl);
        format(got, G, ctrl);
        pack(want, G, &sram, &tables, &mut scratch);
        if !pie::pack_pie(got, G, &sram, None, &tables, &mut pads) {
            println!("packbench: pack_pie refused the {}-word framebuffer", G.words());
            row.identical = false;
            break;
        }
        row.compared += G.words() as u32;
        if let Some(i) = want.iter().zip(got.iter()).position(|(a, b)| a != b) {
            println!(
                "packbench: MISMATCH at b5={} word {} (plane {} col {}): scalar {:#06x} pie {:#06x}",
                b5,
                i,
                i / COLS,
                i % COLS,
                want[i],
                got[i]
            );
            row.identical = false;
            row.mismatch = (i as u32, b5);
            break;
        }
    }

    // ---- timing, SRAM source ----
    tables.build(&crate::hub75::brightness_lut(14));
    {
        let dst = dst_a.slice();
        format(dst, G, ctrl);
        row.sram_scalar = time(|| pack(dst, G, &sram, &tables, &mut scratch));
        row.sram_pie = time(|| {
            pie::pack_pie(dst, G, &sram, None, &tables, &mut pads);
        });
        let g0 = Geometry::new(1, COLS, 0);
        row.sram_gather = time(|| {
            pie::pack_pie(&mut dst[..0], g0, &sram, None, &tables, &mut pads);
        });
    }

    // ---- timing, PSRAM source (the arena; skipped without one) ----
    #[cfg(feature = "psram-arena")]
    if crate::psram::stats().is_some() {
        let layout = Layout::array::<[u8; 3]>(G.pixels()).unwrap();
        let p = crate::psram::alloc_bulk_zeroed(layout).cast::<[u8; 3]>();
        if !p.is_null() {
            // SAFETY: `pixels()` zeroed elements the arena handed us and never
            // frees; nothing else references them.
            let psram = unsafe { core::slice::from_raw_parts_mut(p, G.pixels()) };
            psram.copy_from_slice(&sram);
            let dst = dst_a.slice();
            row.psram_scalar = time(|| pack(dst, G, psram, &tables, &mut scratch));
            row.psram_pie = time(|| {
                pie::pack_pie(dst, G, psram, None, &tables, &mut pads);
            });
            let g0 = Geometry::new(1, COLS, 0);
            row.psram_gather = time(|| {
                pie::pack_pie(&mut dst[..0], g0, psram, None, &tables, &mut pads);
            });
        }
    }

    row.ran = true;
    store(&row);
    let per_px = |(med, _): (u32, u32)| (med * 100 / PIXELS) as f32 / 100.0;
    println!(
        "packbench core {}: {} columns x 2 rows, {} planes, {} — scalar {} cyc/px, pie {} cyc/px \
         (gather {}), from PSRAM scalar {} / pie {} (gather {}); identical {} over {} words{}",
        core,
        COLS,
        PLANES,
        if row.vector_unit { "PIE kernel" } else { "portable model (no vector unit)" },
        per_px(row.sram_scalar),
        per_px(row.sram_pie),
        per_px(row.sram_gather),
        per_px(row.psram_scalar),
        per_px(row.psram_pie),
        per_px(row.psram_gather),
        row.identical,
        row.compared,
        if row.identical {
            alloc::string::String::new()
        } else {
            alloc::format!(" — FIRST MISMATCH word {} at b5={}", row.mismatch.0, row.mismatch.1)
        },
    );
    println!(
        "packbench core {}: cycles per row pair (median/min): sram scalar {}/{}, pie {}/{}, gather {}/{}; \
         psram scalar {}/{}, pie {}/{}, gather {}/{}",
        core,
        row.sram_scalar.0,
        row.sram_scalar.1,
        row.sram_pie.0,
        row.sram_pie.1,
        row.sram_gather.0,
        row.sram_gather.1,
        row.psram_scalar.0,
        row.psram_scalar.1,
        row.psram_pie.0,
        row.psram_pie.1,
        row.psram_gather.0,
        row.psram_gather.1,
    );
}

/// The `/api/status` block: `"packbench":[{…core 0…},{…core 1…}]`.
pub fn status_json(out: &mut impl core::fmt::Write) -> core::fmt::Result {
    write!(out, "[")?;
    for core in 0..2u8 {
        let r = result(core);
        if core == 1 {
            write!(out, ",")?;
        }
        write!(
            out,
            "{{\"core\":{},\"ran\":{},\"vector_unit\":{},\"pixels\":{},\"identical\":{},\"compared\":{},\
             \"mismatch_word\":{},\"mismatch_b5\":{},\
             \"sram\":{{\"scalar\":[{},{}],\"pie\":[{},{}],\"gather\":[{},{}]}},\
             \"psram\":{{\"scalar\":[{},{}],\"pie\":[{},{}],\"gather\":[{},{}]}}}}",
            core,
            r.ran,
            r.vector_unit,
            PIXELS,
            r.identical,
            r.compared,
            r.mismatch.0,
            r.mismatch.1,
            r.sram_scalar.0,
            r.sram_scalar.1,
            r.sram_pie.0,
            r.sram_pie.1,
            r.sram_gather.0,
            r.sram_gather.1,
            r.psram_scalar.0,
            r.psram_scalar.1,
            r.psram_pie.0,
            r.psram_pie.1,
            r.psram_gather.0,
            r.psram_gather.1,
        )?;
    }
    write!(out, "]")
}
