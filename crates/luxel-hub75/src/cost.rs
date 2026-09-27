//! What a HUB75 boot takes out of internal SRAM, as arithmetic a host can run
//! BEFORE it stores a layout (Gitea #822).
//!
//! The firmware allocates the panel's buffers first — before the WiFi blob,
//! embassy-net, the web slots and the engine take their share of the same
//! heap — so a chain that is too wide SUCCEEDS at boot and starves everything
//! after it. On 2026-09-26 a stored `matrix 64 64 2 1` (8192 px) on the
//! Seengreat did exactly that: the panel came up, and the board then answered
//! every HTTP route with 503 `out of memory` or hung. It could not be fixed
//! over the network — not even by storing a smaller layout, because
//! `POST /api/layout` itself could not complete — and an OTA of a corrected
//! image booted straight back into the same stored layout.
//!
//! So the panel side of that cost is predicted here, from the same pure
//! helpers the boot itself uses ([`crate::arrange::fb_geometry`],
//! [`Schedule::plan`]), and `POST /api/layout` refuses a layout whose
//! predicted cost would take the heap under the boot floor.
//!
//! **This covers the PANEL side only.** What the engine, the compositor and
//! the protocol encode buffers cost at a given pixel count is not modelled
//! anywhere and deliberately is not guessed at here — the numbers depend on
//! the pattern, the layer count and the JIT. The real guard against a layout
//! the board cannot serve is therefore the boot SELF-HEAL
//! (`firmware/src/layout.rs`, `luxel_core::layout::heal_decision`): if the
//! heap ends a boot under `luxel_core::budget::RUNTIME_FLOOR`, the stored
//! shape is reverted to the board default and the device reboots. This module
//! is the cheaper half — the refusal that stops the obvious cases from ever
//! being stored, with the numbers in the message.

use crate::{Geometry, Schedule, Tables};

/// The host constants [`boot_cost`] needs — everything that is a property of
/// the BOARD or the DMA peripheral rather than of the geometry. The firmware
/// supplies them from `esp_hub75`; a test supplies its own.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BootAlloc {
    /// Whole framebuffers allocated from internal SRAM.
    ///
    /// 2 for the double-buffered default (`front`/`back`). Under
    /// `hub75-spare-plane` on a board with a PSRAM arena it is 1 — one live
    /// buffer plus [`BootAlloc::spare_planes`], with the compose staging
    /// buffer in the arena — and 2 where that staging buffer comes off the
    /// internal heap instead.
    pub buffers: usize,
    /// Extra single bitplanes allocated from internal SRAM: 0 normally, 1 for
    /// the spare MSB plane of the `hub75-spare-plane` swap.
    pub spare_planes: usize,
    /// DMA descriptor rings (`esp_hub75::DESCRIPTOR_RINGS`) — one per
    /// framebuffer for the frame-atomic swap.
    pub rings: usize,
    /// `size_of::<DmaDescriptor>()`, 12 B on every chip here.
    pub desc_bytes: usize,
    /// `esp_hub75::max_dma_chunk_size()` — what one descriptor can span, and
    /// so how many descriptors a plane's bytes need.
    pub max_chunk: usize,
}

/// Internal-SRAM bytes one boot attempt takes for the panel, itemised so a
/// log line (or a test failure) says WHICH part is the expensive one.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct BootCost {
    /// The framebuffers and any spare plane.
    pub framebuffers: usize,
    /// Every descriptor ring.
    pub descriptors: usize,
    /// The packer's colour lookup tables ([`Tables`]).
    pub tables: usize,
    /// The packer's per-row pads ([`crate::Scratch`]).
    pub scratch: usize,
}

impl BootCost {
    /// Every item added up — what to subtract from the heap the boot started
    /// with.
    #[must_use]
    pub const fn total(&self) -> usize {
        self.framebuffers + self.descriptors + self.tables + self.scratch
    }
}

/// What booting `g` under schedule `s` costs internal SRAM on a host whose
/// allocation shape is `a`.
///
/// `g` must already carry [`Geometry::with_trail`] from `s.needs_trail()` —
/// the trailing display block is part of every plane, so leaving it off
/// under-counts a truncated schedule by `1/rows` of the framebuffers.
///
/// Nothing here allocates or touches hardware: it is the same arithmetic
/// `firmware/src/hub75.rs::try_boot` performs with the allocator, written
/// once so the prediction cannot drift from the boot.
#[must_use]
pub fn boot_cost(g: Geometry, s: &Schedule, a: &BootAlloc) -> BootCost {
    BootCost {
        framebuffers: g.bytes() * a.buffers + g.plane_bytes() * a.spare_planes,
        descriptors: s.descriptors(g.plane_bytes(), a.max_chunk) * a.rings * a.desc_bytes,
        tables: core::mem::size_of::<Tables>(),
        scratch: crate::scratch_bytes(g.cols),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{arrange, Control, MAX_PLANES};
    use luxel_core::layout::{Matrix, PanelDriver};

    /// The Seengreat's allocation shape: two framebuffers, two descriptor
    /// rings, 12-byte descriptors, 4092-byte DMA chunks (esp-hal's ceiling).
    fn seengreat() -> BootAlloc {
        BootAlloc { buffers: 2, spare_planes: 0, rings: 2, desc_bytes: 12, max_chunk: 4092 }
    }

    fn cost_of(m: &Matrix, d: &PanelDriver, a: &BootAlloc) -> BootCost {
        let g = arrange::fb_geometry(m, usize::from(d.planes)).expect("has a framebuffer");
        let s = Schedule::plan(g, Control::new(d.blank, d.latch_clocks()), d.lsb);
        boot_cost(g.with_trail(s.needs_trail()), &s, a)
    }

    /// One 64x64 tile at the default 7 planes — the board default, which the
    /// Seengreat boots with ~87 KB of its 152 KB heap left.
    #[test]
    fn one_tile_fits_the_seengreat() {
        let c = cost_of(&Matrix::single(64, 64), &PanelDriver::default(), &seengreat());
        // 32 address rows x 64 words x 7 planes x 2 B = 28,672 B per buffer
        assert_eq!(c.framebuffers, 2 * 28_672);
        assert_eq!(c.tables, 2048);
        assert_eq!(c.scratch, 64 * 14);
        // one plane is 4,096 B, four bytes past the 4,092 B chunk ceiling =>
        // 2 descriptors per emission, 127 emissions at 7 stock planes, two
        // rings, 12 B each
        assert_eq!(c.descriptors, 2 * 127 * 2 * 12);
        assert_eq!(c.total(), 57_344 + 6_096 + 2_048 + 896);
    }

    /// The 2x1 chain that bricked the board over the network: the panel side
    /// alone is over 120 KB of a 152 KB heap, which is what leaves nothing
    /// for WiFi, the web slots and the engine.
    #[test]
    fn the_2x1_chain_is_the_one_that_broke_it() {
        let m = Matrix { cols: 2, rows: 1, ..Matrix::single(64, 64) };
        let c = cost_of(&m, &PanelDriver::default(), &seengreat());
        // twice as many words per address row => twice the framebuffer
        assert_eq!(c.framebuffers, 2 * 57_344);
        assert_eq!(c.scratch, 128 * 14);
        assert!(c.total() > 120 * 1024, "{c:?}");
        // and it is the framebuffers, not the descriptors, that do it
        assert!(c.framebuffers > 10 * c.descriptors);
    }

    /// Spare-plane mode (#610) is what makes that chain fit: one internal
    /// framebuffer plus one plane, the staging buffer in the PSRAM arena.
    #[test]
    fn spare_plane_halves_the_internal_cost() {
        let m = Matrix { cols: 2, rows: 1, ..Matrix::single(64, 64) };
        let plain = cost_of(&m, &PanelDriver::default(), &seengreat());
        let spare = cost_of(
            &m,
            &PanelDriver::default(),
            &BootAlloc { buffers: 1, spare_planes: 1, ..seengreat() },
        );
        // one 57,344 B buffer plus one 8,192 B plane, against two buffers
        assert_eq!(spare.framebuffers, 57_344 + 8_192);
        assert_eq!(plain.framebuffers, 2 * 57_344);
        // exactly 48 KB back, which is the difference between "starves the
        // board" and "boots with room to spare" on a 152 KB heap
        assert_eq!(plain.total() - spare.total(), 48 * 1024);
    }

    /// Fewer bit planes is the other lever, and it is linear in the
    /// framebuffers — the whole point of the `panel` line's `planes` field.
    #[test]
    fn planes_are_linear_in_the_framebuffers() {
        let m = Matrix::single(64, 64);
        let four = cost_of(&m, &PanelDriver { planes: 4, ..PanelDriver::default() }, &seengreat());
        let eight = cost_of(&m, &PanelDriver { planes: 8, ..PanelDriver::default() }, &seengreat());
        assert_eq!(eight.framebuffers, four.framebuffers * 2);
        assert_eq!(eight.tables, four.tables);
        assert_eq!(eight.scratch, four.scratch);
    }

    /// A truncated schedule (`lsb`) needs the trailing display block (#795),
    /// which is `1/rows` more framebuffer — and far fewer descriptors,
    /// because the low planes stop being re-shifted.
    #[test]
    fn a_truncated_schedule_pays_the_trail_and_saves_descriptors() {
        let m = Matrix::single(64, 64);
        let stock = cost_of(&m, &PanelDriver::default(), &seengreat());
        let trunc =
            cost_of(&m, &PanelDriver { lsb: 8, ..PanelDriver::default() }, &seengreat());
        // 32 address rows + 1 trailing block
        assert_eq!(trunc.framebuffers, stock.framebuffers / 32 * 33);
        assert!(trunc.descriptors < stock.descriptors, "{trunc:?} vs {stock:?}");
    }

    /// `total()` is the sum of the items, at every depth.
    #[test]
    fn total_is_the_sum() {
        for planes in 1..=MAX_PLANES {
            let g = Geometry::new(32, 64, planes);
            let s = Schedule::plan(g, Control::default(), 0);
            let c = boot_cost(g, &s, &seengreat());
            assert_eq!(c.total(), c.framebuffers + c.descriptors + c.tables + c.scratch);
        }
    }

    /// The scratch item is exactly what `Scratch::for_geometry` will hold —
    /// the one number that could silently drift from the allocation.
    #[test]
    fn scratch_matches_the_real_allocation() {
        for cols in [16usize, 64, 128, 256] {
            let g = Geometry::new(32, cols, 7);
            let s = Schedule::plan(g, Control::default(), 0);
            assert_eq!(boot_cost(g, &s, &seengreat()).scratch, crate::Scratch::new(cols).bytes());
        }
    }
}
