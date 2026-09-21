//! Shared test plumbing: the library, the compile front end, and the
//! SYNTHETIC ADDRESS SPACE the emitter compiles against on the host.
//!
//! The emitter always compiles for a 32-bit device, so it cannot be handed
//! real x86-64 function pointers. Instead every helper and every builtin
//! entry gets a made-up 32-bit address; the ISA model traps a `callx8` to
//! one of those and the harness calls the matching Rust function directly
//! (docs/jit-design.md §7.1 — this is what makes the whole backend
//! testable without a device).

#![allow(dead_code)]

use luxel_core::compile::compile;
use luxel_core::kinds::Kinds;
use luxel_core::vm::Program;
use luxel_jit::{Env, Helpers};

/// Where the compiled image pretends to live: the S3's IBUS alias of the
/// PSRAM window (docs/jit-design.md §5), so the numbers in a failure
/// message look like the ones a device would print.
pub const CODE_BASE: u32 = 0x4200_0000;
/// Where the fake `BUILTIN_ENTRIES` table is laid out in model memory.
pub const BUILTINS_ADDR: u32 = 0x3000_0000;
/// The first synthetic helper address. Nothing is ever fetched from these
/// — the model traps the call. **They must share bits 31..30 with
/// [`CODE_BASE`]**: `retw` restores only the low 30 bits of the return
/// address, so a helper in another gigabyte returns into nowhere (found by
/// the ISA model, docs/jit-design.md §3.2).
pub const HELPER_BASE: u32 = 0x4100_0000;
/// Synthetic address of `BUILTIN_ENTRIES[id].generic`.
pub const GENERIC_BASE: u32 = 0x4300_0000;
/// Synthetic address of `BUILTIN_ENTRIES[id].direct`.
pub const DIRECT_BASE: u32 = 0x4400_0000;

/// Which helper a synthetic address names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelperId {
    FxDiv,
    FxPow,
    ArrLoadNum,
    ArrLoadDyn,
    ArrStore,
    ArrLen,
    NewArray,
    ConstArr,
    CallValueTarget,
    AssertFail,
    BailFuel,
    BailDepth,
}

pub const HELPER_ORDER: [HelperId; 12] = [
    HelperId::FxDiv,
    HelperId::FxPow,
    HelperId::ArrLoadNum,
    HelperId::ArrLoadDyn,
    HelperId::ArrStore,
    HelperId::ArrLen,
    HelperId::NewArray,
    HelperId::ConstArr,
    HelperId::CallValueTarget,
    HelperId::AssertFail,
    HelperId::BailFuel,
    HelperId::BailDepth,
];

/// Address of helper `h`. Four bytes apart so a stray off-by-one lands on
/// no helper at all rather than on the next one.
pub fn helper_addr(h: HelperId) -> u32 {
    let i = HELPER_ORDER.iter().position(|x| *x == h).unwrap();
    HELPER_BASE + i as u32 * 16
}

/// Reverse of [`helper_addr`].
pub fn helper_of(addr: u32) -> Option<HelperId> {
    if addr < HELPER_BASE || (addr - HELPER_BASE) % 16 != 0 {
        return None;
    }
    HELPER_ORDER.get(((addr - HELPER_BASE) / 16) as usize).copied()
}

pub fn helpers() -> Helpers {
    Helpers {
        fx_div: helper_addr(HelperId::FxDiv),
        fx_pow: helper_addr(HelperId::FxPow),
        arr_load_num: helper_addr(HelperId::ArrLoadNum),
        arr_load_dyn: helper_addr(HelperId::ArrLoadDyn),
        arr_store: helper_addr(HelperId::ArrStore),
        arr_len: helper_addr(HelperId::ArrLen),
        new_array: helper_addr(HelperId::NewArray),
        const_arr: helper_addr(HelperId::ConstArr),
        call_value: helper_addr(HelperId::CallValueTarget),
        assert_fail: helper_addr(HelperId::AssertFail),
        bail_fuel: helper_addr(HelperId::BailFuel),
        bail_depth: helper_addr(HelperId::BailDepth),
    }
}

pub fn env_with_fake_addresses() -> Env {
    Env {
        code_base: CODE_BASE,
        builtins: BUILTINS_ADDR,
        helpers: helpers(),
        // `JIT_MAX_CODE` of docs/jit-design.md §5.
        max_code: 128 * 1024,
    }
}

/// Compile source to a program plus its kinds section.
pub fn program_of(src: &str) -> Result<(Program, Kinds), String> {
    let prog = compile(src).map_err(|e| format!("{e:?}"))?;
    let kinds = prog
        .kinds
        .clone()
        .ok_or_else(|| "compiled without a kinds section".to_string())?;
    Ok((prog, kinds))
}

/// Every `library/*.js`, by name.
pub fn library() -> Vec<(String, String)> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../library");
    let mut out: Vec<(String, String)> = std::fs::read_dir(dir)
        .expect("library/ must exist")
        .filter_map(|e| {
            let p = e.ok()?.path();
            if p.extension()? != "js" {
                return None;
            }
            let name = p.file_name()?.to_string_lossy().to_string();
            Some((name, std::fs::read_to_string(&p).ok()?))
        })
        .collect();
    out.sort();
    out
}

/// The ISA model, and the bridge that puts a real `Vm` behind it.
#[path = "../isa/mod.rs"]
pub mod isa;
pub mod bridge;
