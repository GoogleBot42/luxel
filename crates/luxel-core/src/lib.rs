//! Luxel core: portable compiler + VM for the Luxel pattern language.
//!
//! This crate is `no_std` + `alloc` and builds for three hosts: ESP32-class
//! firmware, wasm32 (the web IDE's live preview), and native (CLI, tests,
//! fuzzing). Everything observable by a pattern must behave identically on
//! all three — see `fixed` for the 16.16 numeric model that anchors that.
//!
//! Language semantics follow the publicly documented Pixel Blaze pattern
//! language (see docs/research/02-pixelblaze-language.md). Places where the
//! public docs are silent are marked `TODO(oracle)` and are to be settled by
//! black-box differential testing against real hardware.

#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod ast;
pub mod diag;
pub mod fixed;
pub mod lex;
pub mod parse;
