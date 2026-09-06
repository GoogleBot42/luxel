# Memory-mapping flash through the cache MMU (2026-09-05)

Decision (Jeremy, 2026-09-05, Gitea #260): large immutable data — the
pattern engine's execution-ready bytecode and constant pool first, the web
assets partition and the pattern store's read-back blobs after it — is
**read directly out of flash through the cache/MMU (XIP-style)**, not
copied into RAM through the flash controller. RAM is the scarce resource on
every board; decoding into the heap was considered and overruled. esp-hal
1.1.0 (our pinned git rev `7c7f3726`) exposes no flash-mapping API, so
this document works the mechanism out per chip and `firmware/src/flashmap.rs`
implements it. The web assets partition is the first consumer wired
end-to-end (Gitea #259's 31–62 s bundle download is the measurable payoff);
the VM's code stream is the second (the API contract is in "The VM
consumer" below).

This is the third time the need has come up. UPDATES.md 2026-07-06
root-caused two device bugs — multi-chunk asset serving stalling WiFi, and
OTA tripping the hardware watchdog with assets installed — to reading
flash through esp-storage's controller path, and named "memory-map the
assets flash region" as the fix. Gitea #259 measured the same path at
~3.7–7 KB/s on the S3 panel.

## Verdict

**Works on every chip we build, at the register level, without patching
esp-hal.** Each chip's MMU page table is a memory-mapped register block
(ESP32: DPORT; S3/C3: EXTMEM; C6: SPI_MEM0's indexed item registers) that
the bootloader fills with the app's own IROM/DROM pages and otherwise
leaves invalid. There are 61–495 free data-bus entries per chip against a
4 MiB flash that needs at most 64, so every partition we care about can be
mapped at boot and left mapped. Only cache maintenance goes through ROM
(`Cache_Flush_rom` on the ESP32; `Cache_Suspend/Resume_ICache` /
`_DCache` and `Cache_Invalidate_Addr` on the others), and every one of
those symbols is in the esp-rom-sys linker scripts we already link.

Espressif's QEMU models the ESP32's DPORT MMU tables and the flush-driven
page sync, so the table arithmetic and the read-side plumbing are covered
hardware-free (`tools/qemu/flashmap-test.py`, in `run-all.py`). What it
cannot answer — SPI0/SPI1 contention timing, the WiFi-starvation win, the
S3/C3/C6 register paths — is the hardware follow-up (Gitea #271; see
"Hardware follow-up").

## Why the flash-controller path hurts

Two SPI masters share the flash pins: **SPI0** is the cache's fill engine
(every instruction fetch and rodata load that misses), **SPI1** is the
"flash controller" the ROM `esp_rom_spiflash_*` functions drive.
esp-storage's `read_nor` is an SPI1 transaction inside a critical section
(interrupts masked on the calling core) — the ROM function bit-bangs the
command, polls for data, copies out — and the calling core cannot do
anything else, WiFi included, until it returns. A 4 KiB read is ~1 ms of
that; a 228 KB asset is ~60 of them plus the heap staging buffer
`read_chunk` needs to satisfy the word-alignment rule, plus the 1 ms timer
yield the server had to insert between chunks so the WiFi task got any
airtime at all (UPDATES.md 2026-07-06). None of that exists for a cached
load: a mapped read that misses costs one SPI0 line fill (32 B, a few µs,
transparent to software) and a hit costs nothing.

esp-storage does **not** disable the cache around its ROM calls (it used
to be described that way in this repo; `hardware.rs` at our rev is
`maybe_with_critical_section(|| esp_rom_spiflash_read(...))`, nothing
more). What it relies on is that nothing *fetches* from flash while the
op runs: its own wrappers are `#[ram]`, the ROM is the ROM, and the
critical section keeps flash-resident code off the calling core. That
same reliance is what the mapping has to respect — see "Concurrency".

## The MMU, per chip

All numbers measured on the tree this landed in (2026-09-05, v0.1.40 +
this change), from `readelf -lS` of the devshell builds. "DROM pages" is
how many entries the bootloader programs for the app's rodata segment:
the segment starts 0x20 into its first page (image header) and the
bootloader maps whole pages from the page-aligned flash address.

| chip | data window (vaddr) | entries | page | table | app DROM pages | app IROM pages | first free data entry | free data entries |
|---|---|---|---|---|---|---|---|---|
| ESP32 | DROM0 `0x3F400000–0x3F7FFFFF` | 64 (of a 256-entry table per core) | 64 KiB | `0x3FF10000` (PRO), `0x3FF12000` (APP), u32 per entry, page number, `0x100` = invalid | 3 (segment 0x20174 B) | 12 (`0x400D0020`, entries 77–88 of IRAM0/IROM0 — a different sub-window) | 3 → `0x3F430000` | 60 (entries 3–62; 63 reserved) |
| ESP32-S3 | DBUS `0x3C000000–0x3DFFFFFF` (IBUS `0x42000000` shares the table) | 512 | 64 KiB | `0x600C5000`, page number, bit 14 = invalid, bit 15 = PSRAM | 2 (0x1E728 B) | 14 (`0x42020020` → entries 2–15) | 16 → `0x3C100000` | 495 |
| ESP32-C3 | DBUS `0x3C000000–0x3C7FFFFF` (IBUS `0x42000000` shares) | 128 | 64 KiB | `0x600C5000`, page number, bit 8 = invalid | 2 (0x1CF44 B) | 12 (entries 2–13) | 14 → `0x3C0E0000` | 113 |
| ESP32-C6 | one window `0x42000000–0x42FFFFFF` for I and D | 256 | 64 KiB by default (`SPI_MEM_MMU_POWER_CTRL[4:3]`: 64/32/16/8 K; read at runtime) | `0x60002000+0x380` (index) / `+0x37C` (content), page number, bit 9 = valid, bit 10 = flash-encryption "sensitive" | 3 (0x21C48 B) | 12 (`0x42030020` → entries 3–14) | 15 → `0x420F0000` | 240 |

What we map, both 64 KiB-aligned in `firmware/partitions.csv` (the table
was laid out that way in July for exactly this):

| region | offset | length | pages | consumer |
|---|---|---|---|---|
| `assets` | `0x310000` | `0xF0000` | 15 | web assets (wired in this PR) |
| `storage` upper half (raw pages) | `0x290000` | `0x80000` | 8 | current-pattern read-back slot (`patterns.rs` `CUR_*`), the VM code arena (below) |
| `storage` lower half (sequential-storage map) | `0x210000` | `0x80000` | 8 | library blobs — only worth mapping once they are page-contiguous (below) |

31 entries at most, against 60 free on the tightest chip. The app slots
(`ota_0`/`ota_1`) are never mapped: the OTA writer erases and programs them
sector by sector while the *other* slot executes, and a mapping over the
slot being written would present a half-written image to whoever read it.

The classic ESP32 has one more constraint worth stating: only DROM0
supports byte loads. Its IRAM0/IROM0 cache bus (`0x400D0000…`) is 32-bit
only — a `u8` load faults — so data mappings must land in DROM0's 64
entries, which is why the table above counts only those. esp-idf's
`SPI_FLASH_MMAP_INST` trick of spilling data into the instruction window
is not available to us and is not needed.

## Mapping, unmapping, invalidating

`flashmap::map(offset, len)` — `offset` page-aligned, `len` rounded up to
whole pages — does, under a critical section:

1. scan the table for the first run of `pages` invalid entries below the
   last one (the esp-idf bootloader keeps the last entry for its own
   reads and esp-storage's encrypted-read path assumes the same; both
   allocate from the top, we allocate from the bottom, so the two never
   meet);
2. program them, then make the cache coherent — this is the only
   chip-specific part:

**ESP32.** Write the entry (`paddr >> 16`) into BOTH cores' tables (the
bootloader maps the app for both; the APP table must mirror the PRO one
or the second core faults the moment it touches the region), then
`Cache_Flush_rom(0)` and `Cache_Flush_rom(1)`. The ESP32 has no
address-range invalidate; a flush empties the whole cache and both cores
refill from flash on their next miss — a few hundred µs of hiccup at map
time, which happens once per boot. The flush with the cache enabled is the
sequence esp-idf uses after every flash write
(`spi_flash_check_and_flush_cache`) and esp-storage uses after every
encrypted read, and it is **required by Espressif's QEMU**, whose model
only re-reads a changed page from the flash file on a flush or a
cache-enable transition (`hw/misc/esp32_dport.c`,
`esp32_cache_data_sync`). esp-idf's legacy `spi_flash_mmap` skips the
flush for invalid→valid transitions; that path would read a stale page
under QEMU. We always flush.

**S3 / C3.** `Cache_Suspend_ICache` (S3: `rom_Cache_Suspend_ICache`, plus
`Cache_Suspend_DCache` — the S3's DBUS goes through its DCache), write the
entries (page number; flash has bit 15 clear on the S3), `Cache_Resume_*`
with the saved autoload state, then `Cache_Invalidate_Addr(vaddr, len)`.
That is esp-idf's `esp_mmu_map` → `mmu_hal_map_region` sequence minus the
"stall the other CPU" half, which is the fence's job (next section). The
programming function is `#[esp_hal::ram]` with every table accessor
`#[inline(always)]`: while the caches are suspended nothing may fetch from
flash, so it is ROM calls and volatile stores only.

**C6.** Same shape with the indexed registers (`MMU_ITEM_INDEX` then
`MMU_ITEM_CONTENT`), the page size read from `MMU_POWER_CTRL`, and the
entry carrying `VALID` plus `SENSITIVE` when the eFuse says flash
encryption is on (mirrors esp-storage's `format_paddr`; we do not run
encrypted flash, but the bit costs nothing).

`unmap` writes the invalid pattern back and repeats the invalidate.
`invalidate(m, rel, len)` / `invalidate_slice` is the consumer's duty after
it writes flash under a mapping: cache lines do not watch SPI1, so the
old bytes stay "valid" in the cache until dropped (address range on
S3/C3/C6; whole-cache flush on the ESP32).

## Concurrency: SPI1 ops, the second core, WiFi, OTA

The rule the mapping adds to the firmware is short: **a mapped read is an
SPI0 cache miss, and no SPI0 fill may happen while an SPI1 op is in
flight.** During an erase or program the flash chip answers garbage; during
a plain SPI1 read the two masters contend and esp-idf treats it as unsafe
too (it stalls the other CPU and disables its cache for reads as well as
writes). Working through where a mapped read can come from:

- **Same core as the SPI1 op.** Impossible: every esp-storage op runs
  inside a critical section on the calling core, and every ROM
  `esp_rom_spiflash_*` write/erase polls the chip to idle before it
  returns, so there is no window outside the critical section in which
  the chip is busy. The only thing that could read the mapping from
  inside that window is an interrupt handler that the critical section
  does not mask (the ESP32's level-6 WiFi NMI) — hence the rule *never
  touch a mapping from an ISR*. esp-radio's NMI code is IRAM-resident and
  reads no flash; that is the status quo the firmware already depends on
  for its own code fetches.
- **The other core (ESP32, S3).** This is the case the flash fence in the
  second-core branch (`firmware/src/core1.rs`, Gitea #259/#260, not yet on
  master) exists for: every `ota::with_flash` / `AsyncFlash` op parks the
  other core in an IRAM spin before the SPI1 transaction. A parked core
  cannot be mid-load from a mapping — the park interrupt is taken at an
  instruction boundary, so any load it interrupted has completed its line
  fill. Mapped reads in task context on either core are therefore safe by
  construction once the fence is in; nothing in the consumers needs to
  know. Two additions the fence branch has to make when it lands (Gitea
  #272): route `flashmap`'s table operations
  (`quiesced`) through `core1::fenced`, because the ESP32's DPORT MMU
  registers must not be read while the AppCpu runs (the silicon DPORT-read
  hazard esp-idf wraps every `cache_flash_mmu_set` in
  `DPORT_STALL_OTHER_CPU`) and a whole-cache flush must not race a core
  executing from flash; and note that a fence *timeout* (the other core
  failed to park in 100 ms and the op proceeds anyway) now risks a garbage
  mapped read on that core in addition to the garbage code fetch it always
  risked — the `fence_timeouts` counter in `/api/status` is the tell
  either way.
- **Today's master (single core).** No second core runs; the critical
  section is the whole story and `flashmap::quiesced` is exactly that.
- **WiFi.** esp-radio and esp-rtos never touch flash at runtime (no
  `esp_rom_spiflash`/`Cache_` references in either crate; PHY calibration
  data is compiled in, there is no NVS). WiFi's only interaction with the
  mapping is the one the mapping removes: it no longer gets starved.
- **OTA.** The OTA writer programs the *inactive* app slot through
  `with_flash`; the slot is not mapped, so there is nothing to invalidate
  and no reader to protect. The July watchdog trip (assets installed +
  OTA erase burst) was the assets *reader* holding the CPU between erase
  ops; that reader is now a memcpy.
- **Flash writes under a mapping.** The assets upload (`AssetWriter`,
  `tools/deploy.sh --assets-only`, the installer) erases and programs the
  mapped region sector by sector. Readers in flight see the region change
  under them — the same partial data the old path served — but never
  fault, because the entries stay valid. `commit()` invalidates the whole
  region before re-parsing the TOC. The pattern store's `write_raw` will
  do the same for the read-back slot once that consumer is wired.

## Alignment and what the pattern store needs

The mapping is page-granular, so a consumer's *region* must be page-aligned
(the partition table already is). Within a region there is no constraint
beyond what the consumer's own reads want: the VM will execute a
fixed-width instruction stream and index a constant pool as `&[u32]`
/ `&[Value]`, so each pattern's executable blob must be **contiguous and
4-byte aligned** in flash. That single property is what rules out every
off-the-shelf flash filesystem — littlefs, ekv and sequential-storage all
store a file as linked or GC-moved blocks, none of which has a stable
address to execute from.

- The **current-pattern read-back slot** (`patterns.rs` `CUR_SRC_OFF` /
  `CUR_BC_OFF`, written by `store_current` on ad-hoc pushes) is raw pages
  at a fixed, page-aligned offset — mappable as-is. `read_current_bc` (the
  38 KB transient Vec the engine rebuild allocates) becomes a slice into
  the mapping.
- **Library patterns** live as ≤3,840-byte sequential-storage chunk items
  scattered across the wear-leveled map — not contiguous, not aligned, and
  moved by GC. Mapping the map region is possible but pointless for XIP.
  What they need is a **code arena** in the raw upper half of `storage`
  with a tiny allocator of its own, filled by `save()` and by an activation
  that found no extent, and never written per activation (the 2026-08-15
  wear rule). The sequential-storage chunks stay the source of truth for
  the source text and for the API's `/api/patterns/:id` body.

**Implemented (second PR, 2026-09-05): the store side, decoupled from the
instruction format.** `patterns.rs` maps the whole raw half (`0x290000`,
512 KiB, 8 pages) at boot with the same self-check discipline, gives the
ad-hoc slot two 64 KiB bytecode sides (a swap writes the side the engine is
not executing from), and carves the rest into the arena.
`patterns::current_code()` is the contract's slice; `Msg::Library { id, ms }`
replaced the envelope-carrying library swaps so no source/blob/envelope Vec
exists in a library activation at all.

**Arena v2 (2026-09-06, Gitea #281): page extents, not fixed slots.** The
first cut was 7 fixed 40 KiB slots with LRU eviction — it cached seven
patterns and wasted ~90 % of the region, since the median library blob is
under 1 KB. It is now a **page-granular extent allocator**: the allocation
unit is the 4 KiB erase page, a pattern's blob occupies a contiguous
first-fit run of them, and the directory (seq → start page, byte length,
bytecode generation, FNV-1a) persists under the same reserved map key the
slot table used. Trimming the ad-hoc source region from 96 KiB to 32 KiB
(`MAX_SOURCE` is 30 KB — read-back never needed more) grew the pool to
`0xA9000..0x100000` = **348 KiB / 87 pages**, enough for every pattern a
device can store (`MAX_PATTERNS` = 24) many times over. Eviction is gone;
a save that finds no contiguous hole compacts instead, sliding live
extents toward page 0 one page-copy per `with_flash` op, never touching
the running pattern's extent. The planning half (bitmap, first-fit,
compaction plan, directory format) is `firmware/src/extents.rs` — pure,
`no_std`, allocation-free and host-tested via `tools/extent-check`.
docs/firmware.md "The pattern store's mapped half and the code arena" has
the layout table and the full rule set; what the VM format work still owns
is making `Program` borrow the slice instead of `deserialize_lean`
copying it.

## RAM accounting

`cargo test -p luxel-cli --release --test heapstat -- --nocapture`
(the counting-allocator model of the device lifecycle) on the gallery at
this revision. `swap(vec)` is the library-activation lifecycle the
firmware had until the code arena: source Vec + blob Vec read from the
chunk store, the envelope Vec built from them, the two dropped, the
Program decoded from the envelope, the envelope dropped, the budgeted
engine built and run three frames — measured as one peak. `swap(xip)` is
the arena lifecycle: the blob is mapped flash (not heap), `deserialize_lean`
copies it into the Program, engine, three frames. Bytes; the biggest
offenders and three typical patterns:

| pattern | blob | program (decoded) | engine | run-peak | swap(vec) | swap(xip) |
|---|---:|---:|---:|---:|---:|---:|
| Main Stage | 18,459 | 28,512 | 41,678 | 42,030 | **85,389** | **31,819** |
| Frogger 2D | 14,878 | 21,821 | 30,045 | 30,461 | 68,271 | 22,272 |
| Opening Act | 13,293 | 23,810 | 36,524 | 36,652 | 63,763 | 28,715 |
| 2D Fireworks Fade | 12,666 | 17,785 | 36,838 | 40,438 | 57,153 | 33,514 |
| Flash Posterize + Music Sequencer | 9,143 | 15,674 | 32,318 | 32,670 | 46,055 | 27,955 |
| Infinite Snake | 7,728 | 12,386 | 29,186 | 29,570 | 32,795 | 24,995 |
| Chasing Rainbows & HSLuv | 6,098 | 10,370 | 21,031 | 21,031 | 26,279 | 17,432 |
| novas | 3,916 | 7,040 | 19,364 | 19,748 | 18,359 | 17,417 |

Across all 299 gallery patterns: Σ swap(vec) = 3,937,961 B, Σ swap(xip)
= 2,870,875 B, an average of 3,568 B (27.1 %) less per activation; **5
patterns exceed 45 KB at swap under the old lifecycle, 0 under the arena
one** (the new worst case is 2D Fireworks Fade at 33.5 KB; Main Stage
alone sheds 53.6 KB). The old figure was dominated by the three transient
Vecs (source + blob + envelope ≈ 2 × (source + blob)) that the envelope-
carrying `Msg::Code` needed for a library swap; with `Msg::Library` only
the id travels and none of them exist. Resident cost is unchanged at this
step — `deserialize_lean` still copies the code and constant pool into
the `Program` — and drops by the code + constant bytes once the fixed-
width format lets `Program` borrow the slice (#260): for Infinite Snake
most of its 12.4 KB program, for Main Stage most of 28.5 KB. The mapping's
own RAM cost is **zero** — no buffers, no allocations; four `AtomicUsize`s
for the two region base/len pairs and a 7-entry slot table in `.bss`.

Measured with the v5 format (LXBC v5, UPDATES.md 2026-09-05), `heapstat`
now decoding the swap(xip) column with `deserialize_lean_static` over an
aligned `'static` blob — the program BORROWS its words — and reporting
that program's resident RAM as `mapped`:

| pattern | blob (v5) | program (copy) | swap(vec) | swap(xip) | mapped |
|---|---:|---:|---:|---:|---:|
| Main Stage | 25,688 | 35,540 | 99,847 | 22,451 | 8,933 |
| Frogger 2D | 20,488 | 27,274 | 79,491 | 15,061 | 6,421 |
| Opening Act | 17,556 | 27,798 | 72,289 | 23,419 | 10,577 |
| 2D Fireworks Fade | 18,028 | 23,037 | 67,877 | 20,934 | 4,713 |
| Infinite Snake | 10,528 | 15,075 | 38,395 | 21,464 | 4,280 |
| Chasing Rainbows & HSLuv | 8,492 | 12,657 | 31,067 | 14,699 | 4,038 |

Across all 299 (re-measured 2026-09-06 at the extent allocator, #293):
Σ swap(vec) = 4,113,080 B, Σ swap(xip) = 1,902,964 B — an average of
7,391 B (53.7 %) less per activation; 6 patterns exceed 45 KB at swap
under swap(vec), **0** under the arena lifecycle. The word format is
~1.8× the byte format's size, so the copying paths are costlier than the
v4 table above; on the borrowing path the resident program is the header
tables alone.

**Now borrowed on the device** (2026-09-06, Gitea #260, UPDATES.md). The
firmware called the copying `deserialize_lean` at every mapped site until
this date, so `mapped` modelled something the device did not actually do.
It does now: the boot default (`PATTERN_BC`, 4-aligned so the borrow can
happen at all — `include_bytes!` has alignment 1), the `Msg::Library` swap
arm (`patterns::code_of`) and all three mapped branches of the engine
`rebuild()` closure decode with `deserialize_lean_static`, so the running
pattern's resident cost on metal is the `mapped` column. Only the
transient-`Vec` sites still copy — the `Msg::Code`/`Msg::Crossfade`
envelope, the chunk-store fallbacks, the playlist `check_asserts`
pre-flight, the HTTP upload's `validate` — because their bytes do not
outlive the call. The lifetime contract that comes with borrowing (no
extent an engine executes from may be written, moved or freed — including
a crossfade's OUTGOING engine, which the store's old one-pattern
“running” notion did not cover) is the pin set documented in
docs/firmware.md, “The borrowing invariant and the pin set”.

For the assets consumer the saving is smaller but immediate: the 4 KiB
`read_chunk` staging Vec plus the 4 KiB response buffer per in-flight
asset response are gone (the body is written straight from the mapping in
4 KiB slices), and `init()`'s TOC parse allocates nothing beyond the
entries themselves.

## What QEMU emulates

Espressif's fork (`tools/qemu/qemu-espressif.nix`, `esp-develop-9.2.2`)
models the ESP32 cache MMU in `hw/misc/esp32_dport.c`:

- both cores' DROM0/IRAM0 tables at their DPORT addresses (`0x3FF10000`
  / `0x3FF12000`, 64 entries per sub-window, entry mask `0x1FF`,
  `0x100` = invalid) — reads and writes;
- `PRO/APP_CACHE_CTRL` enable and `CTRL1` mask bits, mapped onto QEMU
  memory regions being enabled/disabled;
- the cache contents as a 4 MiB RAM-backed "rom device" per window that
  is **re-read from the flash block device page by page when an entry is
  marked changed AND a flush (`CACHE_FLUSH_ENA`) or an enable transition
  happens** — so a table write alone changes nothing the guest can see,
  exactly the property the always-flush sequence above satisfies;
- invalid-entry accesses returning a fill value and raising the cache
  illegal-access interrupt when enabled.

It does not model timing (SPI0/SPI1 contention, fill latency), the DPORT
read hazard, or the second core's cache state beyond the same table logic.
The S3 and C3 have their own cache models in the fork (`esp32s3_cache.c`,
`esp32c3_cache.c`) but our derivation builds only `qemu-system-xtensa` for
the `esp32` machine and the harness only runs that; extending it is a
separate spike.

`tools/qemu/flashmap-test.py` (in `run-all.py`; needs no device dumps)
boots `result/luxel-fw.bin` — espflash's merged bootloader + partition
table + app, padded to 4 MiB — with a synthetic two-file LUX2 archive at
`0x310000` and asserts the serial narration: the mapping line with its
self-check, the vaddr/entry/page arithmetic, and `assets: 2 files
installed` parsed *after* the mapping was established (i.e. through it).
The self-check is a real firmware feature, not test scaffolding:
`assets::map_region` reads the region's first 4 KiB both ways and refuses
the mapping on any disagreement, so a chip or emulator where the MMU does
not present the page we asked for degrades to the flash-controller path
with a log line instead of serving garbage. The image under test is
byte-identical to what ships.

## The API that landed

`firmware/src/flashmap.rs` (GPL-3.0-or-later like the rest of the
firmware), chip-agnostic surface, per-chip `chip` modules behind the
existing `esp32` / `esp32s3` / `esp32c3` / `esp32c6` features:

```rust
pub fn map(offset: u32, len: u32) -> Result<Mapped, Error>; // page-aligned offset
pub fn unmap(m: Mapped);                                    // nothing may still read it
pub fn invalidate(m: &Mapped, rel: u32, len: u32);          // after writing flash under it
pub fn invalidate_slice(bytes: &[u8]);                      // same, for a leaked mapping
pub fn page_size() -> u32;                                  // 64 KiB, or the C6 register
pub fn any_mapped() -> bool;

impl Mapped {
    pub fn bytes(&self) -> &[u8];          // lives as long as the handle
    pub fn leak(self) -> &'static [u8];    // permanent mapping
    pub fn vaddr(&self) -> usize; pub fn phys(&self) -> u32;
    pub fn pages(&self) -> u32;  pub fn first_entry(&self) -> u32;
}
pub enum Error { Unaligned, Empty, NoFreePages, Disabled }
```

Dropping a `Mapped` does not unmap (leaking is the normal case). The
`flashmap-off` cargo feature makes `map` fail with `Disabled`, which every
consumer answers with its read_nor path — the bisect build for "is it the
mapping?".

Store side (second PR): `patterns.rs` — `current_code() -> Option<&'static
[u8]>` (the contract's slice), `code_of(id)`, `with_code(id, f)`,
`validate_stored(id)`, `source_stat(id) -> (len, fnv1a)`,
`cache_code(id, bc, may_compact).await`, `current_slot_code(len)` /
`current_slot_src(len)`, `raw()`, `arena_stats()`; `shared::Msg::Library {
id, ms }` and `shared::set_pattern_hash_raw`. `/api/status` reports
`code_mapped` and `arena: [used_pages, total_pages]`.

Consumer wired: `assets.rs` maps `0x310000+0xF0000` at boot
(`map_region`, after `ota::init` and the takeover check), self-checks it,
and leaks it; `init()` parses the TOC through it; `FlashAsset::write_content`
hands the socket 4 KiB slices of the mapping with a `yield_now` between
them (no timer sleep — there is no cache-off window to protect WiFi from
any more); `AssetWriter::commit` invalidates the region before the
re-parse. `/api/status` reports `assets_mapped`, and `tools/image-check.sh`
asserts the mapping code is linked into every non-hosted-ui image.

## The VM consumer (agreed shape for #260)

The parent session's fixed-width format work builds on this API as
follows — this is the contract, not a proposal. Items 1, 2, 4, 5 and 6 are
implemented by the store-side PR (the code arena); item 3's "Program
borrows the slice" half is the format work's:

1. **Where the bytes live.** The engine receives `&'static [u8]` (or a
   `&'static [u32]` view; the arena is 4-byte aligned) obtained from a
   region mapped once at boot — the current-pattern slot today, the code
   arena above once the store grows it. It never calls `map` itself and
   never holds a `Mapped`; `patterns.rs` owns the mapping and the slot
   table, and hands out slices. A slice stays valid until the store
   overwrites that slot, which the store only does for a slot no engine
   references (the RAM index knows which pattern is running).
2. **What the engine may do with it.** Read it, from task context on any
   core, at any time — including from the render loop, per pixel, with no
   fence, no lease, no critical section. It may not write it (it is a
   `&[u8]`), and it may not read it from an interrupt handler.
3. **Swaps.** A swap decodes the header (fn table, globals, exported names
   — small, RAM) from the slice, builds the engine referencing the slice
   for code and constants, then drops the old engine. The bytecode Vec and
   the `Program.code` Vec disappear from the lifecycle; `try_budgeted_engine`'s
   floor check stays exactly where it is.
4. **Rebuilds** (pixel-count change, map clear) re-decode from the same
   slice — `read_current_bc`'s Vec goes away.
5. **Store writes** (`store_current`, the arena writer) call
   `flashmap::invalidate` on the slot they wrote before publishing it in
   the RAM index; the ESP32 pays a whole-cache flush per save, which is
   the same order of cost as the erase burst that precedes it.
6. **Fallback.** With no mapping (`flashmap-off`, or a chip where the
   boot self-check refused it) the store reads the slot into a Vec exactly
   as today and the engine gets a `&[u8]` into that Vec — the engine code
   is identical either way; only the lifetime owner differs.

## Verification without hardware

- Builds: `board-athom-music`, `board-pixelblaze-v3`, `board-c3-devkit`,
  `board-s3-devkit`, `board-c6-devkit`, `board-seengreat-hub75`, plus
  `board-c6-devkit` + `hosted-ui` and `board-athom-music` + `flashmap-off`,
  all green; sizes and `.stack` in docs/boards.md and UPDATES.md.
- `tools/stack-check.sh` on the default board: clean; the mapping adds no
  buffers (the self-check's 4 KiB is a heap Vec).
- `tools/image-check.sh`: the new `flashmap: assets ` marker links in
  every non-hosted image; hosted-ui images unaffected.
- QEMU: `tools/qemu/run-all.py` — the existing suite plus `flashmap`.
  `flashmap` and both heap-regions tests pass; the three takeover tests
  fail on a pristine `origin/master` build exactly as they do here (a
  boot-1 marker for the WLED pin import that never appears — Gitea #273,
  pre-existing, not touched by this change).
- `tools/ci.sh` before the PR.

## Hardware follow-up

Gitea #271 (the parent session coordinates the devices; nothing here
touched one). What to look at, per board:

1. Boot serial: `flashmap: assets 0x310000+0xf0000 -> 0x<vaddr> (15 x 64
   KiB pages from entry <n>), self-check ok`, with `<vaddr>` in the chip's
   data window and `<n>` = the "first free" column above; then
   `assets: N files installed`. A `self-check FAILED` line means the
   fallback engaged (the device still works; open an issue with the
   line). No line at all before `patterns:` means a fault during `map` —
   the boot guard rolls the slot back after three tries; the serial
   signature of a cache fault is `Cache error` / `Cache disabled but
   cached memory region accessed` (Xtensa `IllegalInstruction` or
   `LoadProhibited` with `EXCVADDR` inside the data window) or, on
   RISC-V, a `Load access fault` at a `0x3C…`/`0x42…` address.
2. `/api/status` → `"assets_mapped":true`.
3. Re-measure Gitea #259's table: `curl -o /dev/null -w '%{time_total}'`
   of `/assets/index-*.js` (228 KB) on the Athom (baseline 2.1 s) and the
   Seengreat panel (31.1 s rainbow / 61.7 s snake). The mapping removes
   the flash-side cost; the render-loop share of #259 stays until the
   second core lands, so expect the Athom number to move most.
4. Asset upload while serving: `tools/deploy.sh <ip> --assets-only` with a
   bundle download in flight, then reload — the new bundle must serve
   (commit's invalidate) and nothing must panic (writes under a mapping).
5. OTA with assets installed (the July watchdog case): `tools/deploy.sh
   <ip>` round trip, then a bundle download from the new slot.
6. `tools/hw-bench.mjs` soak with assets traffic mixed in, watching
   `heap_free` (should be flat — the mapping allocates nothing).

### Results — Athom (classic ESP32), 2026-09-06

Steps 1 and the arena steps could not be observed: `/dev/ttyUSB0` was absent
from the container, so nothing below rests on a boot line. Builds were pinned
from `/api/status` (`core1` present = the second-core build, `assets_mapped` /
`code_mapped` / `arena` = this facility).

On master `731ce81` — the mapping and the code arena **without** the second
core (PR #280):

| step | result |
|---|---|
| 2. `assets_mapped` | `true`; `code_mapped` `true` and `arena [0,7]` on a fresh boot |
| 3. bundle download (228,553 B) | **2.13 s** at 60 px (baseline 2.1 s), **20.77 s** at 2048 px — table on Gitea #259 |
| 4. write under the mapping | **pass** — 728,437 B installed in **17.1 s**, all 8 files verify with `gunzip -t`, no panic, `heap_free` unchanged at 104,984 B idle |
| 5. OTA with assets installed, then a download from the new slot | pass, five OTAs |
| `flashmap-off` | boots, `assets_mapped:false`, `arena:[0,7]`, device fully functional — the fallback works as documented |

The arena filled to `[7,7]` from **saving** ten patterns, before any of them
ran: `cache_code` claims a slot on save, so the eighth and later stored
patterns can never become mapped (activation never evicts). With the arena
full, a pattern without a slot takes the copying path — `"code_mapped":false`,
`heap_free` 83,532 B for Main Stage against 104,984 B for one that owns a slot,
and 1.3–2.7 s to activate against 0.14 s.

On master `0f83975` (v5 + the 87-slot extent arena + borrowed words + the second
core) the consumer side of the contract works as designed when the board stays
up: `"code_mapped":true` for a stored library pattern, `arena [6,87]` for Main
Stage and `[11,87]` for Frogger 2D, activation **394 ms cold / 72 ms warm**
against 1.6–2.7 s on the copying path, and the pattern's resident heap cost down
44 % / 39 % (table on Gitea #277).

**Step 6 and the arena-eviction steps are blocked on Gitea #292**: on any build
carrying the second core, sustained fenced flash traffic wedges the ProCpu inside
the op and the RTC watchdog reboots the board — 5/5 on a `POST /api/assets`, and
twice inside 25 minutes it burned the boot guard and rolled the slot back.
`EXTRA_FEATURES=flashmap-off` wedges identically and the single-core build does
not wedge at all, so this facility is cleared — the fence is the variable.

One trap this exercise turned up, worth designing against here: an
**interrupted asset install leaves the region corrupt but parseable**. The TOC
is written near the start of the archive, so after a reboot `assets_mapped` is
`true`, every `/assets/…` returns 200 with a plausible length and ETag, and 6
of 8 bodies were nonetheless garbage. Nothing in `/api/status` reports it.
Verifying an install means reading the bodies back, not reading status.

## Open risks

- **S3/C3/C6 register paths are untested on silicon.** They are transcribed
  from esp-idf's `mmu_ll.h` and esp-storage's `mmu.rs` (which Espressif
  runs in HIL for encrypted reads on those chips), and the self-check
  refuses a mapping that presents the wrong page — but a *fault* during
  the suspend/program/resume window would not be caught by the
  self-check, only by the boot guard. First boot of this on an S3 should
  be watched on serial.
- **Cache_Flush on the ESP32 with the cache enabled** is the documented
  esp-idf post-write sequence and esp-storage's encrypted-read sequence,
  but our first flush happens with WiFi not yet up and the second core
  halted; the hardware follow-up's upload-while-serving step is the one
  that exercises a flush under load.
- **The fence hook** (`flashmap::quiesced`) is a critical section until
  the second-core branch routes it through `core1::fenced`; merging that
  branch without doing so leaves the ESP32's DPORT reads during `map`
  racy against a running AppCpu (map runs at boot before the core starts
  today, so the race is theoretical until a second mapping is made at
  runtime).
- **QEMU coverage is ESP32-only** and proves arithmetic and sequencing,
  not timing.
