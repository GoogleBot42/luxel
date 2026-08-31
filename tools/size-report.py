#!/usr/bin/env python3
"""Bucket firmware symbols by crate/origin from `nm -C -S --defined-only` output.

Uses the ELF `st_size` of each defined symbol. Do NOT go back to
`nm --size-sort`: for symbols that carry no size it *estimates* one from the
next symbol's address, which on RISC-V images turns linker-script NOTYPE
symbols (`_rwtext_len`, whose value is a length, not an address), `$d`
mapping symbols and `.L*` locals into hundreds of megabytes of phantom
"other C/asm" (Gitea #174). Size-less symbols are dropped here instead.
"""
import re
import subprocess
import sys
from collections import defaultdict

ELF = sys.argv[1]
out = subprocess.run(
    ["nm", "-C", "-S", "--defined-only", ELF], capture_output=True, text=True
).stdout

# C-symbol prefixes from the Espressif closed blobs / ROM glue
BLOB_PREFIXES = (
    "ieee80211", "pp_", "ppT", "ppR", "ppP", "ppM", "ppInstall", "ppCal",
    "pm_", "phy_", "rc_", "rcG", "rcU", "rcT", "lmac", "wdev", "wDev",
    "esf_", "hal_", "mac_", "sta_", "ap_", "scan_", "wifi_", "ic_", "cnx_",
    "dbg_", "chm_", "trc_", "gWpa", "wpa_", "wps_", "sae_", "dpp_",
    "esp_wifi", "esp_phy", "coex", "bt_", "r_", "lldesc", "eb_", "ets_",
    "hostap", "config_", "chip_", "register_chipv7", "get_phy", "force_wifi",
    "noise_check", "ram_", "rom_", "tx_", "rx_", "SHA1", "aes_", "ccmp",
    "michael", "pk_", "mgmt_", "sm_", "offchannel", "csi_", "ftm_",
    "g_", "s_", "our_", "BSS", "TxRxCntl", "net80211", "lhash",
)

def bucket(name: str) -> str:
    if "::" in name:
        root = name
        # strip generic receivers:  <crate::path::Type ...>::method
        m = re.match(r"^<?&?(?:dyn |mut )?([A-Za-z_][A-Za-z0-9_]*)::", root.lstrip("<&"))
        if m:
            crate = m.group(1)
        else:
            # e.g. "<embassy_executor::raw::TaskStorage<luxel_fw::...>>::poll"
            m = re.search(r"([A-Za-z_][A-Za-z0-9_]*)::", root)
            crate = m.group(1) if m else "unknown-rust"
        aliases = {
            "luxel_fw": "luxel-fw (our code)",
            "luxel_core": "luxel-core (VM/compiler)",
            "core": "rust core (fmt, slices, str, float)",
            "alloc": "rust alloc",
            "compiler_builtins": "compiler-builtins (soft-float, memcpy)",
        }
        return aliases.get(crate, crate)
    for p in BLOB_PREFIXES:
        if name.startswith(p):
            return "espressif wifi/phy blobs (C)"
    return "other C/asm"

sizes = defaultdict(int)
counts = defaultdict(int)
top = defaultdict(list)
total = 0
skipped_sizeless = 0
for line in out.splitlines():
    # "<value> <size> <type> <name>"; code+ro+data only, skip bss.
    # Symbols with no st_size print without the size column and are skipped.
    m = re.match(r"^([0-9a-fA-F]+)\s+([0-9a-fA-F]+)\s+([tTrRdD])\s+(.*)$", line)
    if not m:
        if re.match(r"^[0-9a-fA-F]+\s+[tTrRdD]\s", line):
            skipped_sizeless += 1
        continue
    size = int(m.group(2), 16)
    if size == 0:
        skipped_sizeless += 1
        continue
    name = m.group(4)
    b = bucket(name)
    sizes[b] += size
    counts[b] += 1
    top[b].append((size, name))
    total += size

print(f"accounted flash bytes (text+rodata+data symbols): {total:,}")
print(f"(skipped {skipped_sizeless:,} defined symbols carrying no ELF size)\n")
print(f"{'bucket':46} {'bytes':>9} {'KB':>7}  syms")
for b, s in sorted(sizes.items(), key=lambda kv: -kv[1]):
    print(f"{b:46} {s:9,} {s/1024:7.1f}  {counts[b]}")

print("\n== top symbols per major bucket ==")
for b, s in sorted(sizes.items(), key=lambda kv: -kv[1])[:8]:
    print(f"\n-- {b} ({s/1024:.1f} KB) --")
    for size, name in sorted(top[b], reverse=True)[:6]:
        print(f"  {size:7,}  {name[:150]}")
