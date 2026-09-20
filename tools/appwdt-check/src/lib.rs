//! Host build of `firmware/src/appwdt.rs` (GPL-3.0-or-later, like the
//! firmware it comes from) — the RTC watchdog's AppCpu liveness gate.
//! The module is deliberately pure (no HAL, no atomics, no clock), so it
//! compiles for the host unchanged and its `#[cfg(test)]` suite is the
//! gate's real coverage: the false-trip states that would reboot a healthy
//! device (no pattern loaded, a 234 ms/frame pattern, a 25 s flash burst,
//! OTA) versus the wedge it exists to catch.

#[path = "../../../firmware/src/appwdt.rs"]
pub mod appwdt;
