//! HUB75 driver-chip init sequences, as pin-level steps (Gitea #525).
//!
//! Most HUB75 panels are plain shift registers: power them up and they shift.
//! Some carry a driver chip with configuration registers that must be written
//! before the first frame, and the register write is not a data bus
//! transaction — it is the ordinary R/G/B/LAT/OE pins, bit-banged, with the
//! latch raised a fixed number of clocks before the end of a row so the chip
//! counts the latch width and interprets the shifted pattern as a register
//! value instead of pixels.
//!
//! The sequences here are the ones the C++ `ESP32-HUB75-MatrixPanel-I2S-DMA`
//! library performs in `fm6124init` / `dp3246init`, which is the de-facto
//! reference for these parts: FM6124 / FM6126A / ICN2038S share one sequence
//! (a brightness register and an output-enable register), and the DP3246 has
//! its own, longer one.
//!
//! This module only DESCRIBES the sequence. The firmware owns the GPIOs and
//! walks it, before the LCD_CAM peripheral takes the pins over:
//!
//! ```text
//! let mut steps = 0;
//! chip::init_steps(chip, cols, &mut |s| {
//!     set_rgb(s.data); set_lat(s.latch); set_oe(s.oe);
//!     clk.set_high(); clk.set_low();
//!     steps += 1;
//! });
//! ```
//!
//! # Step semantics
//!
//! A [`Step`] is **the pin levels to establish, and then one clock pulse** —
//! so the number of steps is exactly the number of clock pulses the sequence
//! needs, and the caller never has to know where a level change belongs
//! relative to an edge. Consequences worth stating, because the C++ reads as
//! a mix of loops and bare level changes:
//!
//! - `data` is ONE level for all six colour lines (R1 G1 B1 R2 G2 B2) — the
//!   same value is written to every one of them, which is what makes the
//!   pattern land in every driver of the chain at once.
//! - `oe: true` means the OE PIN IS HIGH, i.e. the display is DISABLED. The
//!   pin is active-low, so this is the opposite polarity from the `OE_ACTIVE`
//!   bit of a framebuffer word. Every step of every sequence holds OE high
//!   except the final one.
//! - A bare "drop the latch" between two register passes is NOT a step of its
//!   own: it is the `latch: false` of the following pass's first step, since
//!   levels are established before that step's clock.
//! - The sequences end with a `latch: false, oe: false` step — the C++ drops
//!   the latch, enables the display and then pulses the clock once, which is
//!   exactly one step under this encoding. After that the display is enabled
//!   and the pins can be handed to the DMA.
//! - The DP3246 also wants the pixel clock's opposite phase (`clkphase` in
//!   the C++); that is the firmware's LCD_CAM configuration, not part of the
//!   step sequence.

pub use luxel_core::layout::Chip;

/// What a [`Chip`] means for the framebuffer template and for boot.
///
/// The enum itself is `luxel-core`'s: it is a stored SETTING (the `panel`
/// wire line), so its wire spelling, its parser and the list a UI offers all
/// live with the rest of the Layout. What is added here is the part only a
/// DRIVER cares about, kept out of `luxel-core` so a strip board links none
/// of it: whether the chip needs [`init_steps`] walked at all, and how many
/// clocks of latch every row block ends with.
///
/// [`luxel_core::layout::PanelDriver::latch_clocks`] answers the latch
/// question from the stored driver instead, and the two must agree — pinned
/// by this module's tests.
pub trait ChipInit {
    /// How many clocks at the end of every row block must assert the latch.
    ///
    /// A shift register latches on one clock; the DP3246 wants three (it
    /// measures the latch width to tell a row latch from a register write).
    /// The firmware combines this with the configured blanking into
    /// [`crate::Control`].
    fn latch_clocks(self) -> u8;

    /// Does this chip need [`init_steps`] walked at all?
    fn needs_init(self) -> bool;
}

impl ChipInit for Chip {
    fn latch_clocks(self) -> u8 {
        match self {
            Chip::Dp3246 => 3,
            _ => 1,
        }
    }

    fn needs_init(self) -> bool {
        !matches!(self, Chip::ShiftReg)
    }
}

/// One clock's worth of pin levels: establish these, then pulse the clock.
///
/// See the module docs for the polarity of `oe` and for what `data` drives.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Step {
    /// Level for all six colour lines (R1 G1 B1 R2 G2 B2).
    pub data: bool,
    /// Latch / STB level.
    pub latch: bool,
    /// OE pin level — `true` = HIGH = display DISABLED.
    pub oe: bool,
}

/// FM6124 REG1, MSB first: `0000011111100000`. Sets global brightness.
const FM_REG1: u16 = 0b0000_0111_1110_0000;
/// FM6124 REG2, MSB first: `0000000001000000`. A single bit enables output.
const FM_REG2: u16 = 0b0000_0000_0100_0000;

/// DP3246 REG1, MSB first: `0000000011111111` — full output current.
const DP_REG1: u16 = 0b0000_0000_1111_1111;
/// DP3246 REG2, MSB first: `1111111100000000` — blanking potential and
/// current-source inflection point at maximum, every option bit off.
const DP_REG2: u16 = 0b1111_1111_0000_0000;

/// Bit `l % 16` of a 16-bit register pattern written MSB first — the drivers
/// are 16-bit shifters and the same pattern is repeated across the chain.
const fn pattern_bit(pattern: u16, l: usize) -> bool {
    (pattern >> (15 - (l % 16))) & 1 != 0
}

/// Walk `chip`'s init sequence, `cols` being the number of pixels in one row
/// of the whole chain (`PIXELS_PER_ROW` in the C++).
///
/// `emit` is called once per clock pulse with the levels to establish first;
/// see the module docs. [`Chip::ShiftReg`] emits nothing.
///
/// Step counts are exact: `3 * cols + 2` for FM6126A / ICN2038S,
/// `4 * cols + 2` for the DP3246.
pub fn init_steps(chip: Chip, cols: usize, emit: &mut dyn FnMut(Step)) {
    match chip {
        Chip::ShiftReg => {}
        Chip::Fm6126a | Chip::Icn2038s => fm6124_init(cols, emit),
        Chip::Dp3246 => dp3246_init(cols, emit),
    }
}

/// FM6124 / FM6126A / ICN2038S.
///
/// Two register passes of `cols` clocks each, the latch raised 11 clocks
/// before the end of the first and 12 before the end of the second (the C++
/// tests `l > cols - 12` / `l > cols - 13` on every iteration, so a chain
/// narrower than the window has the latch up for the whole pass — reproduced
/// here by saturating the start index at 0). Then a blank pass of `cols`
/// clocks to clear the shifters, one latching clock, and one clock with the
/// display enabled.
fn fm6124_init(cols: usize, emit: &mut dyn FnMut(Step)) {
    // REG1 — global brightness.
    let from = cols.saturating_sub(11);
    for l in 0..cols {
        emit(Step { data: pattern_bit(FM_REG1, l), latch: l >= from, oe: true });
    }
    // REG2 — output enable. Its first step carries `latch: false`, which IS
    // the "drop the latch and save REG1" of the C++.
    let from = cols.saturating_sub(12);
    for l in 0..cols {
        emit(Step { data: pattern_bit(FM_REG2, l), latch: l >= from, oe: true });
    }
    // Blank the data registers so the panel is dark after the manipulation.
    for _ in 0..cols {
        emit(Step { data: false, latch: false, oe: true });
    }
    // Latch the blanked row, then enable the display.
    emit(Step { data: false, latch: true, oe: true });
    emit(Step { data: false, latch: false, oe: false });
}

/// DP3246.
///
/// A register-clearing pass, REG1, REG2, one spare clock with the latch down,
/// a blanking pass, and one clock with the display enabled. The C++ raises
/// the latch with a one-shot `l == cols - N` and leaves it raised for the
/// rest of the pass, so a chain narrower than the window never raises it at
/// all — hence `checked_sub` rather than `saturating_sub` here.
fn dp3246_init(cols: usize, emit: &mut dyn FnMut(Step)) {
    // Clear the registers: the last 3 clocks latch (the DP3246's row-latch
    // width), data low throughout.
    let from = cols.checked_sub(3);
    for l in 0..cols {
        emit(Step { data: false, latch: from.is_some_and(|f| l >= f), oe: true });
    }
    // REG1 — output current. Latch up for the last 11 clocks.
    let from = cols.checked_sub(11);
    for l in 0..cols {
        emit(Step {
            data: pattern_bit(DP_REG1, l),
            latch: from.is_some_and(|f| l >= f),
            oe: true,
        });
    }
    // REG2 — blanking / inflection / option bits. Latch up for the last 12.
    let from = cols.checked_sub(12);
    for l in 0..cols {
        emit(Step {
            data: pattern_bit(DP_REG2, l),
            latch: from.is_some_and(|f| l >= f),
            oe: true,
        });
    }
    // One clock with the latch dropped. The colour lines still hold REG2's
    // last bit here: the C++ only blanks them after this pulse.
    emit(Step {
        data: cols.checked_sub(1).is_some_and(|l| pattern_bit(DP_REG2, l)),
        latch: false,
        oe: true,
    });
    // Blanking pass, latching the last 3 clocks as before.
    let from = cols.checked_sub(3);
    for l in 0..cols {
        emit(Step { data: false, latch: from.is_some_and(|f| l >= f), oe: true });
    }
    // Enable the display.
    emit(Step { data: false, latch: false, oe: false });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::vec;
    use std::vec::Vec;

    fn steps(chip: Chip, cols: usize) -> Vec<Step> {
        let mut v = Vec::new();
        init_steps(chip, cols, &mut |s| v.push(s));
        v
    }

    /// Indices where `latch` is high, as (start, len) runs.
    fn latch_runs(s: &[Step]) -> Vec<(usize, usize)> {
        let mut runs = Vec::new();
        let mut i = 0;
        while i < s.len() {
            if s[i].latch {
                let start = i;
                while i < s.len() && s[i].latch {
                    i += 1;
                }
                runs.push((start, i - start));
            } else {
                i += 1;
            }
        }
        runs
    }

    #[test]
    fn a_shift_register_needs_no_init() {
        assert!(steps(Chip::ShiftReg, 64).is_empty());
        assert!(!Chip::ShiftReg.needs_init());
        assert!(Chip::Fm6126a.needs_init());
    }

    #[test]
    fn chip_names_round_trip() {
        for c in Chip::ALL {
            assert_eq!(Chip::parse(c.as_str()), Some(c));
        }
        assert_eq!(Chip::parse("shiftreg"), Some(Chip::ShiftReg));
        assert_eq!(Chip::parse("fm6124"), None);
        assert_eq!(Chip::parse(""), None);
        // the wire default (the pre-#525 behaviour) is no init at all
        assert_eq!(luxel_core::layout::PanelDriver::default().chip, Chip::ShiftReg);
    }

    #[test]
    fn only_the_dp3246_wants_three_latch_clocks() {
        assert_eq!(Chip::ShiftReg.latch_clocks(), 1);
        assert_eq!(Chip::Fm6126a.latch_clocks(), 1);
        assert_eq!(Chip::Icn2038s.latch_clocks(), 1);
        assert_eq!(Chip::Dp3246.latch_clocks(), 3);
    }

    /// The trait here and `PanelDriver::latch_clocks` in `luxel-core` are two
    /// answers to one question — the framebuffer template comes from the
    /// second and the chip's own sequence from the first, so a drift between
    /// them would silently mis-latch every row.
    #[test]
    fn the_stored_driver_agrees_about_the_latch() {
        for c in Chip::ALL {
            let d = luxel_core::layout::PanelDriver { chip: c, ..Default::default() };
            assert_eq!(d.latch_clocks(), c.latch_clocks(), "{}", c.as_str());
        }
    }

    #[test]
    fn the_fm6124_sequence_is_three_passes_and_two_clocks() {
        for cols in [64usize, 128] {
            let s = steps(Chip::Fm6126a, cols);
            assert_eq!(s.len(), 3 * cols + 2, "{cols}");
            assert_eq!(s, steps(Chip::Icn2038s, cols), "icn2038s shares the sequence");

            // OE high (display disabled) for everything but the last clock.
            assert!(s[..s.len() - 1].iter().all(|x| x.oe), "{cols}");
            assert!(!s[s.len() - 1].oe, "{cols}");
            assert!(!s[s.len() - 1].latch);

            // REG1 latches the last 11 clocks of its pass, REG2 the last 12,
            // then the blank pass has none and the penultimate clock latches.
            assert_eq!(
                latch_runs(&s),
                vec![(cols - 11, 11), (cols + cols - 12, 12), (3 * cols, 1)],
                "{cols}"
            );

            // The register patterns, 16-bit and repeated across the chain.
            for l in 0..cols {
                let want1 = matches!(l % 16, 5..=10);
                assert_eq!(s[l].data, want1, "REG1 bit {l} of {cols}");
                let want2 = l % 16 == 9;
                assert_eq!(s[cols + l].data, want2, "REG2 bit {l} of {cols}");
                // the blank pass drives the colour lines low
                assert!(!s[2 * cols + l].data, "blank bit {l} of {cols}");
            }
            assert!(!s[3 * cols].data);
            assert!(!s[3 * cols + 1].data);
        }
    }

    #[test]
    fn the_dp3246_sequence_is_four_passes_and_two_clocks() {
        for cols in [64usize, 128] {
            let s = steps(Chip::Dp3246, cols);
            assert_eq!(s.len(), 4 * cols + 2, "{cols}");
            assert!(s[..s.len() - 1].iter().all(|x| x.oe), "{cols}");
            assert!(!s[s.len() - 1].oe, "{cols}");

            // clear pass: last 3; REG1: last 11; REG2: last 12; one bare
            // clock; blank pass: last 3. The REG2 run and the blank run are
            // separated by that bare clock, so they never merge.
            assert_eq!(
                latch_runs(&s),
                vec![
                    (cols - 3, 3),
                    (2 * cols - 11, 11),
                    (3 * cols - 12, 12),
                    (4 * cols + 1 - 3, 3),
                ],
                "{cols}"
            );

            // clear pass drives the colour lines low throughout
            assert!(s[..cols].iter().all(|x| !x.data), "{cols}");
            for l in 0..cols {
                // REG1 = 0000000011111111 MSB first
                assert_eq!(s[cols + l].data, l % 16 >= 8, "REG1 bit {l} of {cols}");
                // REG2 = 1111111100000000 MSB first
                assert_eq!(s[2 * cols + l].data, l % 16 < 8, "REG2 bit {l} of {cols}");
                // blanking pass, after the bare clock at 3*cols
                assert!(!s[3 * cols + 1 + l].data, "blank bit {l} of {cols}");
            }
            // the bare clock still holds REG2's last bit: (cols-1) % 16 == 15
            assert!(!s[3 * cols].data);
            assert!(!s[3 * cols].latch);
        }
    }

    /// A chain narrower than a latch window: the FM sequence holds the latch
    /// for the whole pass (its test runs every iteration), the DP3246 never
    /// raises it (its test is a one-shot on an index that does not exist).
    #[test]
    fn a_chain_narrower_than_the_latch_window() {
        let s = steps(Chip::Fm6126a, 8);
        assert_eq!(s.len(), 26);
        assert!(s[..8].iter().all(|x| x.latch));
        assert!(s[8..16].iter().all(|x| x.latch));
        assert!(s[16..24].iter().all(|x| !x.latch));

        let s = steps(Chip::Dp3246, 8);
        assert_eq!(s.len(), 34);
        // only the two 3-clock windows survive at cols = 8
        assert_eq!(latch_runs(&s), vec![(5, 3), (8 + 8 + 8 + 1 + 5, 3)]);
    }

    #[test]
    fn a_zero_width_chain_emits_only_the_tail_clocks() {
        assert_eq!(steps(Chip::Fm6126a, 0).len(), 2);
        assert_eq!(steps(Chip::Dp3246, 0).len(), 2);
    }
}
