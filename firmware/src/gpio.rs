//! Real GPIO behind the pattern builtins (Gitea #177 item 4).
//!
//! The engine never touches a pad: it keeps the pattern's VIEW of every
//! pin — the last `pinMode`, the last `digitalWrite` level, the level
//! `digitalRead` reports, the value `analogRead` reports — and a host
//! keeps that view in step with the world. In the playground the host is
//! a person with a pin panel; here it is this module, run by the render
//! task between frames: configure pads the pattern named, copy written
//! levels out, read input levels (and ADC samples) back in through the
//! same injection ABI the playground uses (`Engine::set_pin` /
//! `Engine::set_analog_pin`). Nothing in the engine knows the difference.
//!
//! Pins are esp-hal *types*, but pin NUMBERS are what a pattern has, so
//! this is the one place the firmware erases the type at runtime
//! (`AnyPin::steal`). Safety rests on `board::pin_is_board_free` +
//! `pin_is_free`: a pattern can only reach pads nothing else in the
//! firmware owns (not the strip, not the relay, not flash/PSRAM/console),
//! so the "one instance per pin" rule holds by construction — every other
//! GPIO is either consumed by name in main.rs's wiring section or never
//! touched.
//!
//! Cost when unused: a pattern that names no pin costs one mask test per
//! frame. `touchRead` is not wired (no pad to calibrate against; the
//! engine's injected value still serves it) — Gitea ticket on the PR.

use core::sync::atomic::Ordering;

use esp_hal::gpio::{AnyPin, DriveMode, Flex, InputConfig, Level, OutputConfig, Pull};
use esp_println::println;
use luxel_core::engine::Engine;

use crate::board;

/// Pins 0..=63 — the engine's window (`luxel_core::vm::MAX_TRACKED_PIN`).
const N: usize = 64;

/// Arduino/ESP32 `pinMode` bits, as the engine records them.
const MODE_OUTPUT: u8 = 0x02;
const MODE_PULLUP: u8 = 0x04;
const MODE_PULLDOWN: u8 = 0x08;
const MODE_OPEN_DRAIN: u8 = 0x10;

/// A pad a pattern may use right now: board-free AND not the strip's
/// configured DATA pin (a runtime setting, so not in the const tables).
pub fn pin_is_free(n: u8) -> bool {
    board::pin_is_board_free(n) && n != crate::shared::DATA_PIN.load(Ordering::Relaxed)
}

pub struct PinHost {
    /// One erased pad per pin the pattern has configured, created lazily.
    flex: [Option<Flex<'static>>; N],
    /// The mode last applied to each pad (0 = untouched / released), so a
    /// pattern that calls `pinMode` every frame costs no register writes.
    applied: [u8; N],
    /// Bit per pin: currently configured by us (`applied != 0`).
    active: u64,
    /// Bit per pin: refused (reserved, non-existent, output on an
    /// input-only pad) and already logged — say it once, not per frame.
    warned: u64,
    #[cfg(any(feature = "esp32", feature = "esp32c3", feature = "esp32s3"))]
    adc: Option<adc::Bank>,
    /// The analog pin set the ADC bank was built for.
    adc_mask: u64,
}

impl PinHost {
    pub fn new() -> Self {
        Self {
            flex: [const { None }; N],
            applied: [0; N],
            active: 0,
            warned: 0,
            #[cfg(any(feature = "esp32", feature = "esp32c3", feature = "esp32s3"))]
            adc: None,
            adc_mask: 0,
        }
    }

    /// Bring the pads in line with the pattern's view, then the pattern's
    /// view in line with the pads. Runs between frames on the render task.
    pub fn sync(&mut self, eng: &mut Engine) {
        let used = eng.pins_used();
        if used == 0 && self.active == 0 && self.adc_mask == 0 && eng.analog_pins_used() == 0 {
            return;
        }
        // Pads we configured for a previous pattern that this one does not
        // name: back to a floating input, so a swap never leaves a pad
        // driven by a pattern that no longer exists.
        let mut stale = self.active & !used;
        while stale != 0 {
            let pin = stale.trailing_zeros() as usize;
            stale &= stale - 1;
            self.release(pin);
        }
        let out_high = eng.pins_out_high();
        let mut todo = used;
        while todo != 0 {
            let pin = todo.trailing_zeros() as usize;
            todo &= todo - 1;
            let bit = 1u64 << pin;
            if !pin_is_free(pin as u8) {
                self.warn(bit, pin, "reserved on this board");
                continue;
            }
            // `digitalRead`/`digitalWrite` without a `pinMode`: a pad is an
            // input by default (Arduino semantics), so read it as one.
            let mut mode = eng.pin_mode(pin as i32);
            if mode == 0 {
                mode = 0x01;
            }
            if mode & MODE_OUTPUT != 0 && !board::gpio_can_output(pin as u8) {
                self.warn(bit, pin, "input-only pad, OUTPUT ignored");
                continue;
            }
            if self.applied[pin] != mode {
                self.configure(pin, mode);
            }
            let f = self.flex[pin].as_mut().unwrap();
            if mode & MODE_OUTPUT != 0 {
                f.set_level(if out_high & bit != 0 { Level::High } else { Level::Low });
            }
            // What the pad actually reads — the injected level beats the
            // engine's idle guess, so a pulled-up pad with a button to
            // ground reads exactly what the button is doing.
            eng.set_pin(pin as i32, Some(f.is_high()));
        }
        self.sync_analog(eng);
    }

    fn warn(&mut self, bit: u64, pin: usize, why: &str) {
        if self.warned & bit == 0 {
            self.warned |= bit;
            println!("gpio: pattern named GPIO{} — {}", pin, why);
        }
    }

    fn configure(&mut self, pin: usize, mode: u8) {
        let f = self.flex[pin].get_or_insert_with(|| {
            // SAFETY: `pin_is_free` proved nothing else in the firmware owns
            // this pad (see the module docs); one Flex per pin for the life
            // of the task.
            Flex::new(unsafe { AnyPin::steal(pin as u8) })
        });
        let pull = if mode & MODE_PULLUP != 0 {
            Pull::Up
        } else if mode & MODE_PULLDOWN != 0 {
            Pull::Down
        } else {
            Pull::None
        };
        if mode & MODE_OUTPUT != 0 {
            let drive = if mode & MODE_OPEN_DRAIN != 0 {
                DriveMode::OpenDrain
            } else {
                DriveMode::PushPull
            };
            f.apply_output_config(&OutputConfig::default().with_drive_mode(drive).with_pull(pull));
            f.set_output_enable(true);
        } else {
            f.set_output_enable(false);
            f.apply_input_config(&InputConfig::default().with_pull(pull));
        }
        // input buffer on for outputs too: `digitalRead` of an OUTPUT pin
        // reports the wire, which is also how a driven pad is verified
        f.set_input_enable(true);
        self.applied[pin] = mode;
        self.active |= 1u64 << pin;
    }

    fn release(&mut self, pin: usize) {
        if let Some(f) = self.flex[pin].as_mut() {
            f.set_output_enable(false);
            f.apply_input_config(&InputConfig::default());
            f.set_input_enable(false);
        }
        self.applied[pin] = 0;
        self.active &= !(1u64 << pin);
    }

    #[cfg(any(feature = "esp32", feature = "esp32c3", feature = "esp32s3"))]
    fn sync_analog(&mut self, eng: &mut Engine) {
        let mut want = eng.analog_pins_used();
        // Only pads that exist, are free, and reach ADC1; the rest are
        // refused once and read the engine's injected value (0).
        let mut check = want;
        while check != 0 {
            let pin = check.trailing_zeros() as usize;
            check &= check - 1;
            let bit = 1u64 << pin;
            if !pin_is_free(pin as u8) {
                self.warn(bit, pin, "reserved on this board (analogRead)");
                want &= !bit;
            } else if !board::adc_pin(pin as u8) {
                self.warn(bit, pin, "not an ADC1 pin on this chip (analogRead reads 0)");
                want &= !bit;
            }
        }
        if want != self.adc_mask {
            // The bank is built for a fixed channel set (attenuation is
            // programmed at construction), so a new set is a new bank.
            self.adc = None;
            self.teardown_analog(self.adc_mask);
            self.adc_mask = want;
            if want != 0 {
                self.adc = Some(adc::Bank::new(want));
                // Switching a pad to its analog function drops the digital
                // config esp-hal finds there (pulls, output enable). A pin
                // the pattern ALSO configured with `pinMode` — a pot with
                // `INPUT_PULLUP`, say — gets it re-applied on top: the RTC
                // pull resistors work in analog mode, so the pull survives.
                // The pad is the ADC's from here: an OUTPUT does not drive
                // it and `digitalRead` of it reads 0.
                let mut again = want & eng.pins_used();
                while again != 0 {
                    let pin = again.trailing_zeros() as usize;
                    again &= again - 1;
                    let mode = eng.pin_mode(pin as i32).max(0x01);
                    self.configure(pin, mode);
                }
            }
        }
        if let Some(bank) = self.adc.as_mut() {
            bank.read_into(eng);
        }
    }

    /// Undo `set_analog` on pads leaving the ADC set: the pattern that
    /// sampled them is gone (or stopped), and the next one may want the
    /// same pad as a plain digital input. esp-hal's `Flex::new` resets the
    /// IO_MUX side; on Xtensa the RTC mux that analog mode switched in has
    /// to be routed back to IO_MUX by hand, or the digital input buffer
    /// stays disconnected and every `digitalRead` of the pad reads 0
    /// (seen on the Athom: GPIO33 read 0 under `INPUT_PULLUP` right after
    /// an `analogRead` pattern). Dropping the Flex forces a fresh
    /// `Flex::new` + config the next time the pattern names the pin.
    #[cfg(any(feature = "esp32", feature = "esp32c3", feature = "esp32s3"))]
    fn teardown_analog(&mut self, mask: u64) {
        let mut m = mask;
        while m != 0 {
            let pin = m.trailing_zeros() as usize;
            m &= m - 1;
            #[cfg(any(feature = "esp32", feature = "esp32s3"))]
            {
                use esp_hal::gpio::{RtcFunction, RtcPin};
                // SAFETY: momentary handle to a pad this module owns (it is
                // in `mask`, so it passed `pin_is_free`); the Flex for it is
                // dropped right below, before anything else can touch it.
                let p = unsafe { AnyPin::steal(pin as u8) };
                p.rtc_set_config(false, false, RtcFunction::Digital);
            }
            self.flex[pin] = None;
            self.applied[pin] = 0;
            self.active &= !(1u64 << pin);
        }
    }

    #[cfg(not(any(feature = "esp32", feature = "esp32c3", feature = "esp32s3")))]
    fn sync_analog(&mut self, eng: &mut Engine) {
        let want = eng.analog_pins_used();
        if want != self.adc_mask {
            self.adc_mask = want;
            let mut check = want;
            while check != 0 {
                let pin = check.trailing_zeros() as usize;
                check &= check - 1;
                self.warn(1u64 << pin, pin, "no ADC driver for this chip (analogRead reads 0)");
            }
        }
    }
}

/// ADC1 sampling for `analogRead`. esp-hal's ADC API is typed per GPIO, so
/// the runtime pin number goes through one `match` arm per ADC1 pad of the
/// chip — the `slots!` table below. Attenuation is fixed at 11 dB (the
/// full 0..3.3 V span, matching what a pattern expects `analogRead` to
/// cover); the 12-bit code maps to 0..1 with no calibration, which is
/// plenty for a pot or a light sensor. ADC2 is deliberately absent: it is
/// unusable while WiFi runs.
#[cfg(any(feature = "esp32", feature = "esp32c3", feature = "esp32s3"))]
mod adc {
    use esp_hal::analog::adc::{Adc, AdcConfig, AdcPin, Attenuation};
    use esp_hal::peripherals::ADC1;
    use esp_hal::Blocking;
    use luxel_core::engine::Engine;
    use luxel_core::fixed::Fx;

    type Bank1 = Adc<'static, ADC1<'static>, Blocking>;

    macro_rules! slots {
        ($($n:literal => $G:ident),* $(,)?) => {
            pub enum Slot {
                $($G(AdcPin<esp_hal::peripherals::$G<'static>, ADC1<'static>>)),*
            }
            fn enable(cfg: &mut AdcConfig<ADC1<'static>>, pin: u8) -> Option<Slot> {
                match pin {
                    $($n => Some(Slot::$G(cfg.enable_pin(
                        // SAFETY: `pin_is_free` + `board::adc_pin` gate every
                        // pin that reaches here (see the module docs).
                        unsafe { esp_hal::peripherals::$G::steal() },
                        Attenuation::_11dB,
                    ))),)*
                    _ => None,
                }
            }
            fn read(adc: &mut Bank1, slot: &mut Slot) -> u16 {
                match slot {
                    $(Slot::$G(p) => loop {
                        if let Ok(v) = adc.read_oneshot(p) {
                            break v;
                        }
                    }),*
                }
            }
            /// Upper bound on simultaneously sampled pins = ADC1 pads.
            const MAX_SLOTS: usize = [$($n),*].len();
        };
    }

    #[cfg(feature = "esp32")]
    slots! { 32 => GPIO32, 33 => GPIO33, 34 => GPIO34, 35 => GPIO35,
             36 => GPIO36, 37 => GPIO37, 38 => GPIO38, 39 => GPIO39 }
    #[cfg(feature = "esp32c3")]
    slots! { 0 => GPIO0, 1 => GPIO1, 2 => GPIO2, 3 => GPIO3, 4 => GPIO4 }
    #[cfg(feature = "esp32s3")]
    slots! { 1 => GPIO1, 2 => GPIO2, 3 => GPIO3, 4 => GPIO4, 5 => GPIO5,
             6 => GPIO6, 7 => GPIO7, 8 => GPIO8, 9 => GPIO9, 10 => GPIO10 }

    pub struct Bank {
        adc: Bank1,
        slots: heapless::Vec<(u8, Slot), MAX_SLOTS>,
    }

    impl Bank {
        /// `mask` has already been filtered to free ADC1 pads.
        pub fn new(mask: u64) -> Self {
            let mut cfg = AdcConfig::new();
            let mut slots = heapless::Vec::new();
            let mut m = mask;
            while m != 0 {
                let pin = m.trailing_zeros() as u8;
                m &= m - 1;
                if let Some(slot) = enable(&mut cfg, pin) {
                    let _ = slots.push((pin, slot));
                }
            }
            // SAFETY: the ADC1 singleton is consumed only here, and only one
            // Bank exists at a time (`PinHost::adc`); a rebuild drops the
            // old one first.
            let adc = Adc::new(unsafe { ADC1::steal() }, cfg);
            Self { adc, slots }
        }

        /// One blocking conversion per pad (tens of µs each), injected as
        /// 0..1: 12-bit code / 4095.
        pub fn read_into(&mut self, eng: &mut Engine) {
            for (pin, slot) in self.slots.iter_mut() {
                let raw = read(&mut self.adc, slot).min(4095) as i32;
                eng.set_analog_pin(*pin as i32, Fx::from_raw((raw << 16) / 4095));
            }
        }
    }
}
