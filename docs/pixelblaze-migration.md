# Pixelblaze v3 → Luxel migration

How a stock Pixelblaze v3 becomes a Luxel device over the air, inheriting
its WiFi and LED settings. The firmware mechanism is the sibling of the WLED
takeover (docs/wled-migration.md) and shares its whole table-writing half;
this page is the Pixelblaze-specific part. **The firmware side is proven
end to end under QEMU only** (`tools/qemu/pb-takeover-test.py`); the delivery
of the image onto a stock Pixelblaze is the open question, settled on
hardware — see "Delivery" below.

## How the takeover works

The `pixelblaze-takeover` cargo feature (`firmware/src/takeover.rs` +
`firmware/src/pbfs.rs` + `firmware/src/pbnvs.rs`) is on for
`board-pixelblaze-v3` only. It is the same foreign-partition-table takeover
as `wled-takeover`: the shared half (read the live table, find our own image
in a foreign app slot, verified self-copy to ota_0, wipe config/otadata,
rewrite the table, reboot) lives in `firmware/src/parttab.rs` and is built on
every board already (the layout migrator needs it too). Only the
settings-inheritance half differs, because a Pixelblaze keeps its settings
somewhere other than WLED does.

A stock Pixelblaze v3's partition table (confirmed from `pb-v3-stock.bin`):

| entry | offset | size |
|---|---|---|
| nvs | 0x09000 | 0x05000 |
| otadata | 0x0E000 | 0x02000 |
| app0 (ota_0) | 0x10000 | 0x140000 |
| app1 (ota_1) | 0x150000 | 0x140000 |
| spiffs | 0x290000 | 0x170000 |

The app slots are **byte-identical** to Luxel's 4 MB `ota_0`/`ota_1`
(`firmware/partitions.csv`), so a Luxel image delivered into either slot
boots under Pixelblaze's table and runs fine (ESP32 app images are
position-independent across slots). On boot the takeover notices the foreign
table and runs, early, after the boot-loop guard:

1. **Discriminate.** `parttab::live_table()` ≠ the embedded Luxel table, and
   `parttab::is_luxel()` is false (a Pixelblaze table has no `storage`/
   `assets` partitions) → this is a takeover, not a layout migration.
2. **Inherit, before anything is written** (best-effort; any failure just
   falls through to the provisioning AP):
   - **WiFi** out of Pixelblaze's **ESP-IDF NVS** partition
     (`firmware/src/pbnvs.rs`): namespace `nvs.net80211`, blob keys
     `sta.ssid` / `sta.pswd`. A stock Pixelblaze keeps no credentials on its
     filesystem, so this is the NVS reader's whole reason to exist.
   - **LED settings** out of Pixelblaze's **SPIFFS** `config.json`
     (`firmware/src/pbfs.rs`): `pixelCount`, `ledType`, `colorOrder`, and
     `brightness` × `maxBrightness`. SPIFFS keeps stale copies at older
     `rev` numbers; the reader walks the object index and takes the live,
     newest one.
   - Mapping into a Luxel device-config record:
     - `ledType` → protocol: 1 (WS2812) → ws2812, 2 (APA102/SK9822) →
       sk9822; anything else keeps the board default.
     - `colorOrder` ("GRB", "BGR", …) → Luxel's `ColorOrder` code relative
       to the mapped protocol's native wire order, the same transform the
       WLED path uses.
     - **brightness**: Pixelblaze shows the strip at the main-page slider
       (`brightness`) scaled by Settings → "Limit brightness"
       (`maxBrightness`). Luxel has one brightness knob and no fractional
       cap, so the two are folded together — the converted strip comes up
       the brightness it visibly *was*, not the raw slider value.
     - **data pin**: Pixelblaze v3 hardwires DATA=GPIO23/CLK=GPIO18, which
       **is** `board-pixelblaze-v3`'s default, so there is nothing to
       import — unlike WLED, whose data pin is configurable.
     - No equivalent in `config.json` for Luxel's mA power cap or
       post-process chain; those stay at their defaults.
3. **Guards**: the flash chip must fit the new table (4 MB — fine); the copy
   destination must not overlap the running image.
4. **Self-copy**: locate our own image by its `esp_app_desc`, copy it to
   ota_0 (0x10000) if it is not already there (an upload into app1 at
   0x150000 is copied down; an upload into app0 is already in place).
5. **Wipe** nvs/guard/otadata sectors 0x9000..0x10000. The inherited WiFi was
   read in step 2 *before* this, which matters here in a way it does not for
   WLED: on a Pixelblaze the credentials live inside this very range.
6. **Persist** the inherited WiFi (`config::write_wifi`) and LED settings
   (`config::write_device`) into Luxel's own records, so they survive.
7. **Rewrite the table** to Luxel's (`parttab::install` — the only
   non-re-runnable ~ms window) and reboot. Pixelblaze's own second-stage
   bootloader (never touched) finds otadata erased and falls back to ota_0 →
   Luxel.

**After the switch**, Luxel's `storage` (0x290000) and `assets` (0x310000)
both land inside what was Pixelblaze's SPIFFS, so `patterns::init` formats a
fresh pattern store and the playground UI is absent until the installer
pushes assets (`POST /api/assets`) — exactly as in the WLED case.

**Crash-safety / flake-safety** are inherited unchanged from the shared
skeleton: everything before the table rewrite re-runs under the Pixelblaze
table after a power cut; a crash-looping takeover build trips
`ota::preboot_guard`, which rolls otadata back to the Pixelblaze slot on the
third failed boot; and an aborted self-copy reboots to retry up to three
boots before settling into the provisioning AP (issue #35).

### The NVS-page hazard (`firmware/src/pbnvs.rs`)

`ota::preboot_guard` runs before the heap allocators, before the takeover,
and on **every** boot it erases and rewrites its boot-guard record at
**0xC000** — which, under Pixelblaze's table, is one of the NVS partition's
pages. So by the time the takeover reads NVS, one 4 KiB page is already
erased or holds a foreign "LXBG" record. The reader tolerates a missing or
garbage page and takes the credentials from the surviving pages (in the
reference dump `sta.ssid`/`sta.pswd` live on the 0xA000/0xB000 pages, and the
namespace registration on 0xB000). This is verified by `tools/pbnvs-check
--wipe-guard` and by the QEMU test, which boots through a real
`preboot_guard` pass.

## Delivery (the open question for the hardware test)

The WLED takeover rides in on WLED's `/update` page, which accepts any ESP32
app image with the `0xE9` magic. **Pixelblaze's own web updater appears to
verify a firmware signature** (its update files are `.stfu`, "Signed
Transfer Firmware Update"), so a plain, unsigned Luxel image may be rejected
by a stock Pixelblaze's `/update` or `/recovery.html`. We do not have, and
will not forge, Pixelblaze's signing key.

This does **not** affect the takeover itself — the takeover is independent of
how the image arrived; it only needs a Luxel image sitting in app0 or app1
under Pixelblaze's table. Two delivery paths to settle on hardware, in order
of preference:

1. **Pixelblaze's web updater**, if it in fact accepts an unsigned/foreign
   image through `/update` or `/recovery.html`. Unverified; this is the
   first thing to try on the real device, since it would make the conversion
   fully no-serial like WLED's.
2. **Serial seed.** A Pixelblaze v3's expansion header carries UART + the
   strapping pins (docs/firmware.md, "Board: Pixelblaze v3"), so the Luxel
   image can be written into an app slot over serial — after which the
   takeover converts the layout and inherits WiFi/LED settings on the next
   boot. This is strictly more capable than today's "serial-flash the whole
   Luxel image" path (it preserves the user's WiFi and strip config without
   re-provisioning), and it is the guaranteed fallback if (1) is blocked by
   the signature check.

Either way, **back up the stock flash first** (`espflash read-flash 0
0x400000 pb-v3-stock.bin`): it is the only restore path, and it holds the
device's WiFi config and saved patterns — keep it, don't commit it
(gitignored).

## Verifying without hardware

- `tools/qemu/pb-takeover-test.py` — composes a 4 MiB flash from
  `pb-v3-stock.bin` with a `.#luxel-fw-pixelblaze-v3` image in an app slot,
  boots it under the patched QEMU, and asserts the takeover's serial
  narration and the resulting flash bytes (Luxel table at 0x8000, inherited
  `LXCF`/`LXDV` records, the copied image, boot-guard/otadata state).
  `--slot app1` (default) exercises the self-copy; `--slot app0` the
  already-in-place path. Registered in `tools/qemu/run-all.py` and run by
  `CI_QEMU=1 tools/ci.sh`. Skipped when the gitignored dump is absent.
- `tools/pbfs-check <dump>` — runs the SPIFFS config reader against a flash
  dump and prints the parsed wiring (device name length only; no secrets).
- `tools/pbnvs-check <dump> [--wipe-guard]` — runs the NVS WiFi reader and
  prints only credential **lengths**, never the values; `--wipe-guard`
  proves the reader survives the 0xC000 erase.

`pb-v3-stock.bin` contains real WiFi credentials — it is gitignored, and
every tool and test prints lengths only.
