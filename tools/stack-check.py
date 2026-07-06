#!/usr/bin/env python3
"""Fail if any function's stack frame exceeds a byte budget.

Parses the `.stack_sizes` section emitted by `rustc -Z emit-stack-sizes`
(see tools/stack-check.sh, which builds with the flag). Unlike the clippy
`large_stack_arrays` lint, this sees EVERY function in the linked image —
including dependencies and build-std core/alloc — so it catches library
frames like esp-storage's `FlashStorage::read` 4 KiB bounce buffer, the
original OTA-crash culprit that a lint on our own source cannot see.

Pure stdlib: parses the ELF32 (little-endian, Xtensa/RISC-V) directly, no
pyelftools. Usage: stack-check.py <elf> [budget_bytes]  (default 4096).
"""
import struct
import sys


def read_uleb128(data, off):
    result = shift = 0
    while True:
        b = data[off]
        off += 1
        result |= (b & 0x7F) << shift
        if not (b & 0x80):
            return result, off
        shift += 7


def parse_elf(path):
    with open(path, "rb") as f:
        blob = f.read()
    if blob[:4] != b"\x7fELF":
        sys.exit(f"{path}: not an ELF file")
    if blob[4] != 1:
        sys.exit("only ELF32 supported (this firmware is 32-bit)")
    # e_shoff@0x20, e_shentsize@0x2e, e_shnum@0x30, e_shstrndx@0x32 (ELF32 LE)
    e_shoff, = struct.unpack_from("<I", blob, 0x20)
    e_shentsize, e_shnum, e_shstrndx = struct.unpack_from("<HHH", blob, 0x2E)

    sections = []
    for i in range(e_shnum):
        base = e_shoff + i * e_shentsize
        name, typ, flags, addr, off, size, link, info, align, entsize = \
            struct.unpack_from("<IIIIIIIIII", blob, base)
        sections.append(dict(name=name, type=typ, off=off, size=size,
                             link=link, entsize=entsize))

    def sec_name(sh):
        strtab = sections[e_shstrndx]
        start = strtab["off"] + sh["name"]
        end = blob.index(b"\x00", start)
        return blob[start:end].decode("utf-8", "replace")

    by_name = {sec_name(s): s for s in sections}

    # symbol table: addr -> function name (ELF32 Sym is 16 bytes)
    addr_to_name = {}
    symtab = by_name.get(".symtab")
    if symtab:
        strtab = sections[symtab["link"]]
        n = symtab["size"] // 16
        for i in range(n):
            st_name, st_value, st_size, st_info, st_other, st_shndx = \
                struct.unpack_from("<IIIBBH", blob, symtab["off"] + i * 16)
            if (st_info & 0xF) != 2:  # STT_FUNC
                continue
            s = strtab["off"] + st_name
            nm = blob[s:blob.index(b"\x00", s)].decode("utf-8", "replace")
            addr_to_name[st_value] = nm

    # .stack_sizes: repeated [address: 4B LE][stack size: ULEB128]
    ss = by_name.get(".stack_sizes")
    if not ss:
        sys.exit("no .stack_sizes section — build with tools/stack-check.sh "
                 "(RUSTFLAGS=-Z emit-stack-sizes)")
    out = []
    off, end = ss["off"], ss["off"] + ss["size"]
    while off < end:
        addr, = struct.unpack_from("<I", blob, off)
        off += 4
        size, off = read_uleb128(blob, off)
        out.append((size, addr_to_name.get(addr, f"<0x{addr:08x}>")))
    return out


def main():
    if len(sys.argv) < 2:
        sys.exit("usage: stack-check.py <elf> [budget_bytes]")
    elf = sys.argv[1]
    # Gate budget. NOT 4096: embassy polls every task future on the single
    # ~60 KB main-task stack, so the inherent big frames — picoserve's
    # request-handler state machine (~9 KB) and the esp-storage /
    # esp-bootloader flash sector buffers (~4 KB each: NorFlash::write,
    # FlashRegion::read/write, read_partition_table) — are expected and safe
    # (deepest chain ~17 KB « 60 KB). 12 KB clears the known max with margin
    # so this passes the good build and fires on a genuine regression (e.g.
    # reintroducing a multi-KB *stack* buffer). The full landscape prints
    # regardless. The original OTA crash was these same ~4 KB frames on the
    # *old 15.6 KB* stack — a depth problem the heap fix, not this gate,
    # solved; the gate guards against new pathological frames.
    budget = int(sys.argv[2]) if len(sys.argv) > 2 else 12288
    frames = parse_elf(elf)
    frames.sort(reverse=True)

    print(f"stack-check: {len(frames)} functions, budget {budget} bytes")
    print("  largest frames:")
    for size, name in frames[:15]:
        flag = "  <-- OVER" if size > budget else ""
        print(f"    {size:>7}  {name}{flag}")

    over = [(s, n) for s, n in frames if s > budget]
    if over:
        print(f"\nFAIL: {len(over)} function(s) exceed the {budget}-byte "
              f"stack budget:")
        for size, name in over:
            print(f"    {size:>7}  {name}")
        return 1
    print(f"\nok: no function exceeds {budget} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
