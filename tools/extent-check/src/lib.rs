//! Host build of `firmware/src/extents.rs` (GPL-3.0-or-later, like the
//! firmware it comes from). The module is `no_std` and allocation-free, so
//! it compiles for the host unchanged and its `#[cfg(test)]` suite is the
//! allocator's real test coverage — see the module for what it asserts.

#[path = "../../../firmware/src/extents.rs"]
pub mod extents;
