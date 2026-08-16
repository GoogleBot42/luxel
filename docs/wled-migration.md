# WLED → Luxel migration

How a WLED device becomes a Luxel device over the air, and how the
**installer web page** (`web/flash.html`) drives it. Mechanism proven end
to end on the Athom LS8P music controller 2026-07-26 (twice); see
UPDATES.md.

## How the takeover works

The takeover is always compiled into the firmware (`firmware/src/takeover.rs`,
~17 KB of image incl. the littlefs reader; a no-op costing one 256-byte
flash read on devices already running the Luxel layout).

1. **Delivery.** WLED's `/update` accepts any ESP32 app image (it checks
   the `0xE9` magic and slot fit) and writes it to its inactive OTA slot.
   The image to upload is `espflash save-image` output (app-only, NOT the
   merged full-flash image): build with `BOARD=... ./build-esp32.sh`, then
   `espflash save-image --chip esp32 --flash-mode dout --flash-size 4mb
   --flash-freq 40mhz target/xtensa-esp32-none-elf/release/luxel-fw out.bin`.
2. **First boot.** WLED's own bootloader (kept forever — never touched)
   boots the slot; ESP32 apps are position-independent across slots via
   the flash MMU, so Luxel runs fine from WLED's layout.
3. **Takeover** (early in boot, after the boot-loop guard):
   - partition table at 0x8000 ≠ embedded Luxel table → proceed;
   - guards: flash chip must fit the new table; the copy destination must
     not overlap the running image;
   - **inheritance**: mount WLED's littlefs read-only (`wledfs.rs`) and
     lift WiFi SSID (cfg.json) + password (wsec.json);
   - locate self by comparing its own `esp_app_desc` (image offset 0x20)
     against each app slot; copy itself to ota_0 @0x10000 if not already
     there (sector-wise erase+write+verify);
   - wipe nvs/otadata sectors 0x9000..0x10000; persist inherited creds
     via `config::write_wifi` (Luxel's normal creds record);
   - rewrite the partition table (the only non-re-runnable ~ms window)
     and reboot. Empty otadata → bootloader falls back to ota_0 → Luxel.
4. **Recovery paths.** Everything before the table write re-runs under
   WLED's intact table after a power cut. A crash-looping takeover build
   trips the boot guard, which flips otadata back to the WLED slot —
   self-rollback to stock. Devices with no inheritable creds boot Luxel's
   provisioning AP (`luxel-XXXX` @192.168.4.1, captive portal) — that's
   the *mandatory* fallback, not an error: factory-fresh/app-provisioned
   WLED stores no creds on its filesystem (verified on real hardware).

## Installer page (`web/flash.html`, shipped 2026-08-15)

One static page, no backend: `web/src/flash/` (wizard UI) +
`lib/releases.ts` (where firmware comes from) + `lib/device.ts` (talking
to the device). E2e: `web/tools/flash-e2e.mjs` against
`web/tools/fake-wled.mjs` (both WLED CORS generations, esp8266 stop,
both firmware-source modes). It ships inside the web dist, so it's also
on every Luxel device and in the release web-dist tarball.

**Firmware source, two modes** (`lib/releases.ts`):

- *bundled*: `firmware/manifest.json` + binaries published NEXT TO the
  page — the release workflow composes exactly that as the GitHub Pages
  site (docs/releases.md). Same-origin, page fetches binaries itself,
  fully automatic. `web/tools/gen-flash-manifest.mjs` builds the
  manifest.
- *github*: no bundle (self-hosted web-dist, device-served copy) —
  release *metadata* comes from api.github.com (CORS `*`), but the
  binary downloads are NOT browser-fetchable: **GitHub's release-asset
  hosts send no `Access-Control-Allow-Origin` on any hop** (measured
  2026-08-15, both the `browser_download_url` chain and the API
  per-asset octet-stream redirect; `Origin: null` too). The page falls
  back to download-link + file-picker for the two files.

**Device protocol** (`lib/device.ts` — all previously proven notes,
now implemented): arch detect via `GET /json/info` `.arch` (esp8266 →
hard stop; s2/s3 → "no builds yet"; WLED 0.13 has no CORS so an opaque
`no-cors` probe means "reachable, pick your board yourself"; 0.14+ is
readable); upload as multipart `POST /update` field `update`
(CORS-safelisted, sendable everywhere, response opaque; OTA-passphrase
rejections surface via the timeout help); success = `/api/status`
answering with `version`+`slot` (3 min poll budget), timeout help covers
the `luxel-XXXX` provisioning AP (no inheritable creds), the OTA
passphrase, and the first-boot panic; assets = `POST /api/assets` with
the LUXA (Luxel serves CORS, so this leg is fully readable).

**Browser-policy edges**: an https-hosted copy (GitHub Pages) can't
normally touch `http://` LAN devices (mixed content) — `device.ts`
passes Chromium's Local-Network-Access `targetAddressSpace: "private"`
hint when the page is https AND the target host is actually
private-space (RFC1918 IP or `.local`; permission-prompt flow on new
Chromium, inert elsewhere). The hint must match the target's real
address space — Chromium fails mismatched requests, measured against a
localhost target from the live site — so localhost/public hosts get no
hint. Every automated step has manual equivalents
(open `/update` yourself, curl line for assets) shown when a fetch is
browser-blocked. Plain-http hosting (a Luxel device serving the page, a
LAN web-dist host) has none of these restrictions — that's the
smoothest origin for converting a *second* device.

- **Credentials**: the page never asks for WiFi creds — inheritance
  covers portal-configured WLED; the provisioning AP covers the rest.
  (Client-side image patching was considered and shelved; UPDATES.md
  2026-07-26.)
- **Open issue, why the page says "beta"**: the intermittent first-boot
  `esp-alloc: Exceeded the maximum of 3 heap memory regions` panic
  (1-in-2 observed). It self-heals via the panic-reboot handler, but it
  fires *before* the boot guard arms, so a deterministic variant would
  loop without the WLED rollback. Root-cause before dropping the beta
  label.

## Bench workflow (this repo, the Athom)

- **Serial rig**: FTDI FT232R USB-UART adapter, `/dev/ttyUSB0`, 115200
  baud. The device node comes back root-owned after every USB replug —
  `chmod` it before use, every time. Only one process may hold the port
  open: a second reader (a stray `screen`/`minicom`/monitor left running)
  silently steals bytes instead of erroring, and looks exactly like a
  dead line from the other reader's side — check for orphaned readers
  first if serial output goes quiet. Nothing wires DTR/RTS to
  EN/IO0 on this rig (same constraint as the Pixelblaze v3's expansion
  header, see docs/firmware.md), so esptool/espflash can't reset the chip
  into the bootloader themselves — pass explicit no-reset on both ends
  (`--before no-reset --after no-reset` for esptool; espflash's
  `--before no-reset --after no-reset` match) and pair every command with
  the button-held power-up below for entering download mode. The FTDI can
  also re-enumerate mid-transfer (the port vanishing and reappearing as
  the same or a new `/dev/ttyUSB*`); treat any flash dump as unverified
  until a second `read-flash` and a hash compare (`sha256sum`) agree with
  the first.
- Restore stock WLED: button-held power-up (GPIO0 = case button, power
  via `zigbee2mqtt/claude-switch`), then
  `esptool --before no-reset --after no-reset write-flash 0x0 athom-wled-stock.bin`.
- Re-takeover: `curl -F "update=@firmware/target/luxel-wled-takeover.bin"
  http://192.168.0.183/update`, watch serial at 115200.
- Corpus for the littlefs reader: `athom-wled-fs-configured.bin` (+ NVS
  sibling; git-ignored, contain real creds) — exercised by
  `tools/wledfs-check`.
