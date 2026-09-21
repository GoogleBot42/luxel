//! ABI pins for the JIT's calling convention (Gitea #642,
//! docs/jit-design.md §7.1).
//!
//! The emitter will trust the C ABI for a **two-word struct return**: a
//! `#[repr(C)] struct { i32, i32 }` comes back in two registers, not
//! through a hidden out-pointer. This file pins the HOST half of that
//! claim — the half `cargo test` can check on every CI run.
//!
//! **The Xtensa half is NOT pinned here.** On `xtensa-esp32s3-none-elf` the
//! question is whether the two words land in `a2:a3`, and the only honest
//! answer comes from disassembling the real thing:
//!
//! ```text
//! cd firmware && EXTRA_FEATURES=dispatch-table BOARD=board-seengreat-hub75 ./build-esp32.sh
//! A=$(xtensa-esp32s3-elf-nm target/xtensa-esp32s3-none-elf/release/luxel-fw \
//!       | awk '/lx_abi_probe_ret2/{print $1}')
//! xtensa-esp32s3-elf-objdump -d --start-address=0x$A --stop-address=$((0x$A+16)) \
//!     target/xtensa-esp32s3-none-elf/release/luxel-fw
//! ```
//!
//! `lx_abi_probe_ret2` is `#[no_mangle]` AND referenced by a `#[used]`
//! static — `#[no_mangle]` alone names a symbol but does not stop
//! `--gc-sections` from dropping a function nobody calls, and the first S3
//! build of #642 dropped it. Measured on the `board-seengreat-hub75` image
//! (2026-09-20):
//!
//! ```text
//! lx_abi_probe_ret2:
//!   entry  a1, 32
//!   add.n  a8, a3, a2      ; val
//!   xor    a3, a3, a2      ; status -> a3
//!   mov.n  a2, a8          ; val    -> a2
//!   retw.n
//! ```
//!
//! Both words in `a2:a3`, no `sret` pointer — and the real `generic`
//! wrappers agree (`callx8` then `mov.n a2, a10 / mov.n a3, a11`).
//! docs/jit-design.md §7.1 carries the result. Phase 2 turns that reading
//! into a checked-in disassembly assertion alongside the encoder tests.

use luxel_core::jit::{lx_abi_probe_ret2, Ret2};

#[test]
fn a_two_word_repr_c_return_round_trips() {
    for (a, b) in [
        (0i32, 0i32),
        (1, -1),
        (-1, 1),
        (i32::MIN, i32::MAX),
        (i32::MAX, i32::MIN),
        (0x1234_5678, 0x7654_3210),
        (7, 9),
    ] {
        let r = lx_abi_probe_ret2(a, b);
        assert_eq!(r.val, a.wrapping_add(b), "val for ({a}, {b})");
        assert_eq!(r.status, a ^ b, "status for ({a}, {b})");
    }
}

#[test]
fn ret2_is_two_words_in_declaration_order() {
    assert_eq!(core::mem::size_of::<Ret2>(), 8);
    assert_eq!(core::mem::align_of::<Ret2>(), 4);
    assert_eq!(core::mem::offset_of!(Ret2, val), 0);
    assert_eq!(core::mem::offset_of!(Ret2, status), 4);
}
