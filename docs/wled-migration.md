# WLED → Luxel migration

How a WLED device becomes a Luxel device over the air, and working notes
for the **installer web page** (not yet built). Mechanism proven end to
end on the Athom LS8P music controller 2026-07-26 (twice); see UPDATES.md.

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

## Installer page — implementation notes

The whole migration can be one static web page (no backend), because:

- **Chip detection is installer-side, not on-device.** An Xtensa image
  can't run on a RISC-V chip, so per-chip artifacts are unavoidable —
  but WLED tells us which to pick: `GET http://<ip>/json/info` →
  `.arch` ("esp32", "esp32-s3", "esp32c3", "esp8266"…). ESP8266 → hard
  stop, unsupported forever. S2/S3 → need Luxel builds first (esp-hal
  supports them; we just don't build/test them yet).
- **Upload**: `POST http://<ip>/update`, multipart form, field name
  `update`, filename set, the .bin as payload. WLED replies with an HTML
  page and reboots. If the device has an OTA passphrase set, WLED wants
  it (form auth on /update) — ask the user when the upload bounces.
- **Progress watching**: after upload, poll `http://<ip>/api/status`
  (1–2 s interval, ~3 min budget: WLED flash-write ≈ 30 s, takeover copy
  ≈ 15 s, two reboots, WiFi join). Same IP is expected — the MAC doesn't
  change, so the DHCP lease survives. Success signature: JSON with
  `version` + `slot` keys (WLED has neither). Also handle the AP case:
  if polling times out, tell the user to look for the `luxel-XXXX` AP —
  creds inheritance may have found nothing.
- **Assets**: the firmware image has no web app in it (arithmetic: app
  ~910 KB + assets ~930 KB don't fit any slot). After Luxel answers,
  `POST /api/assets` with the LUXA bundle (`web/tools/pack-assets.mjs`
  output) — hot reload, no reboot, CORS already handled by the device
  (deploy.sh --assets-only does exactly this from the CLI today). Until
  assets arrive the device serves the built-in fallback page.
- **CORS caveat for the WLED side**: WLED 0.13 sends no CORS headers, so
  a cross-origin installer page cannot read `/json/info` responses. The
  upload POST itself works (form POSTs don't need CORS to *send*), but
  arch detection needs one of: `mode:"no-cors"` + asking the user which
  chip, a tiny "copy this URL's output" step, or serving the installer
  from something LAN-local. PROTOTYPE THIS FIRST — it shapes the UX.
  (WLED 0.14+ added `Access-Control-Allow-Origin: *` to the JSON API —
  check per-version.)
- **Credentials**: never ask for WiFi creds in the page. Inheritance
  covers portal-configured WLED; the provisioning AP covers the rest.
  (A third option — patching creds into the image client-side + fixing
  the image checksum/SHA in JS — was considered and shelved; see
  UPDATES.md 2026-07-26.)
- **Release artifacts** the page needs per chip: `luxel-takeover-<chip>.bin`
  (save-image output with the takeover built in — i.e. any release app
  image) + one LUXA asset bundle (chip-independent) + a manifest with
  versions/hashes.
- **Open issue to resolve before shipping this to strangers**: the
  intermittent first-boot `esp-alloc: Exceeded the maximum of 3 heap
  memory regions` panic (1-in-2 observed). It self-heals via the
  panic-reboot handler, but it fires *before* the boot guard arms, so a
  deterministic variant would loop without the WLED rollback. Root-cause
  first.

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
