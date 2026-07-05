# Research: Pixel Blaze product & firmware feature inventory

> Raw research (July 2026) informing the Luxel plan. Maker: Ben Hencke ("wizard"),
> Hencke Technologies / ElectroMage. Sites: electromage.com, forum.electromage.com,
> patterns.electromage.com. Firmware is closed source; language, protocols, and file
> formats are publicly documented (see sources at bottom).

## 1. Hardware

### Pixelblaze V3 Standard
- ESP32 (dual-core 240 MHz, WiFi), 34.2 × 39.5 mm. `cpuSpeed` is a settable config field.
- LED chipsets: APA102/SK9822 (HDR, clock 250 kHz–20 MHz), WS2811/2812/2813/2815,
  SK6812 RGB/RGBW, WS2801; later firmware added WS2814, WS2816 + NS108 (48-bit HDR),
  UCS5603, UCS7604, GS8208.
- Data driven at 5 V through 100 Ω; screw terminals + 0.1" header.
- Max pixels: ~5,000 (APA102) / 2,500 (WS2812) per board. Benchmarks: ~48,000 px/s
  average across stock patterns; 100 px: 300–600+ FPS; 1,000 px: 30–65 FPS; 5,000 px: 6–13 FPS.
- Storage: 1.4 MB pattern storage (~100 patterns with previews). "XL" variant: 3 MB.
- Per-pattern VM limits: 256 globals, 256 stack slots (recursion OK), 10,240 total array
  elements, arrays never freed (no GC).
- GPIO: 3 on expansion header + 15 pads; 5 analog inputs; 5 capacitive touch inputs.
  Expansion header: GND, EN, 3.3V, RX0, TX0, IO0, IO25, IO26 (fits sensor board).
- Power: micro-USB or 5 V backfeed; button (pattern cycle / WiFi setup) + status LED.

### Pixelblaze V3 Pico
- 11 × 33.3 mm wearable form factor, $29.95 (Adafruit #5943). Same ESP32/firmware.

### Output Expander (open source: firmware MIT, hardware CC-BY-SA)
- UART-driven 8-channel LED driver, 2 Mbps; 3-bit address jumpers → chain 8 boards = 64 channels.
- ~240 RGB / 180 RGBW px per channel at full frame rate (docs); 600–800 practical max.
- Wire frame: magic `"UPXL"`, channel id (u8), record type (u8), payload, CRC-32.
  Commands: `SET_CHANNEL_WS2812` (config + pixel data), `DRAW_ALL` (latch all channels).
- On the Pixelblaze, expander config lives in `/obconf.dat`, exchanged as websocket binary
  type 9; per channel: `ledType, pixelCount, colorOrder, dataSpeed, startIndex` — each
  channel takes a contiguous slice of the single logical pixel array (this is PB's
  multi-segment output mechanism).
- Repo: github.com/simap/pixelblaze_output_expander (+ C and NodeJS client libs).

### Sensor Expansion Board (open source: firmware MIT, hardware CC-BY-SA)
- STM32F0; mic with onboard FFT → 32 bins (37 Hz–10 kHz), energy average, max-frequency;
  ±16 G 3-axis accelerometer; ambient light; 5 × 12-bit analog inputs.
- One-way serial at 115200 baud: `"SB1.0\0"` + 32×u16 freq + u16 energyAverage +
  u16 maxFreqMagnitude + u16 maxFreqHz + 3×s16 accel + u16 light + 5×u16 analog + `"END\0"`.
- Pattern bindings via `export var`: `frequencyData[32]`, `energyAverage`,
  `maxFrequencyMagnitude`, `maxFrequency`, `accelerometer[3]`, `light`, `analogInputs[5]`.
  ~40 updates/s, values normalized 0..1. Since v3.40 a sync leader rebroadcasts sensor data.
- Repo: github.com/simap/pixelblaze_sensor_board.

## 2. Firmware features

### Web IDE / live coding
- SPA served from device flash (`/index.html.gz`). Live compile on keystroke → bytecode
  hot-swapped while LEDs keep running. Inline syntax + runtime error highlighting,
  autocomplete (v3.47+), Var Watcher (live exported vars), in-browser 2D/3D preview
  (v3.24+), live strip preview from binary previewFrame stream. Tab-close warning +
  browser autosave (v3.66). Ships with dozens of documented patterns.
- Other tabs: pattern list w/ JPEG previews, Settings, Mapper, Playlist, WiFi.

### WiFi
- First boot: open AP `Pixelblaze_XXXXXX`, setup at 192.168.4.1. Client mode or AP mode.
  Hold button 5 s to re-enter setup. Cloud discovery service (discover.electromage.com)
  when `discoveryEnable` set.

### OTA
- Update button in settings when internet-connected; websocket `upgradeVersion` /
  `getUpgradeState`. Manual update possible via POST `/update`.

### Storage & file formats
- On-device FS over HTTP: `/list`, `/edit` (POST multipart), `/delete?path=`, GET file.
- `/p/{id}` — PBP "Pixelblaze Binary Pattern": 9×u32 LE header (version, nameOffset/Len,
  jpegOffset/Len, bytecodeOffset/Len, sourceOffset/Len) + name + preview JPEG + bytecode +
  LZString-compressed source. IDs: 17 chars of base-55 charset
  `23456789ABCDEFGHJKLMNPQRSTWXYZabcdefghijkmnopqrstuvwxyz`.
- `/p/{id}.c` — saved UI control values per pattern.
- `.epe` export — JSON `{name, id, sources:{main:"<source>"}, preview:<base64 JPEG>}`;
  preview JPEG columns = one frame each (that's how the pattern site animates).
- PBB backup — JSON `{"files": {"/<path>": "<base64>"}}` of the whole FS.
- Others: `/config.json`, `/obconf.dat`, `/pixelmap.txt` (map source), `/pixelmap.dat`
  (compiled map), `/l/_defaultplaylist_`.

### WebSocket API (port 81; HTTP on 80)
- Text = JSON commands; ~1 Hz stats push:
  `{"fps":41.9,"vmerr":0,"vmerrpc":-1,"mem":2111,"exp":0,"renderType":2,"uptime":…,"storageUsed":…,"storageSize":…}`.
- JSON commands: `getConfig` (→ settings + sequencer + binary expanderConfig, possibly out
  of order), `listPrograms`, `activeProgramId`, `nextProgram`, `sequencerMode` (0 Off /
  1 ShuffleAll / 2 Playlist), `runSequencer`, `sequenceTimer`, `getPlaylist`/`playlist`,
  `deleteProgram`, `getPreviewImg`, `getVars`/`setVars`, `getControls`/`setControls`,
  `brightness` (0–1), `name`, `discoveryEnable`, `timezone`, `autoOff*`, `save` flag,
  `getSources`, `setCode` (size/crc/id announce before bytecode), `savePixelMap`, `pause`,
  `ping`→`ack`, `sendUpdates`, `upgradeVersion`/`getUpgradeState`, `getPeers` (v3.40+).
- Binary frames: byte0 = type, byte1 = flags (except type 5). Types: 1 putSourceCode,
  3 putByteCode, 4 previewImage (JPEG), 5 previewFrame (device→client, header byte +
  3 bytes RGB/pixel), 6 getSourceCode, 7 getProgramList (TSV `id\tname\n`), 8 putPixelMap,
  9 expanderConfig. Flags: 0x1 first, 0x2 middle, 0x4 last.
- Live-code sequence: `{"pause":true,"setCode":{size,crc,id}}` → binary type 3 → `{"setControls":{}}` → `{"pause":false}`.
- v3.66: outbound websocket client ("External WebSocket Server") for control through NAT.

### Discovery / time sync (UDP 1889)
- Beacon ~1 Hz: u32 LE ×3 = packetType 42, senderId/IP, sender ms time.
- Time server reply: u32 LE ×5 = type 43, serverId, server time ms, echoed sender id + time
  (NTP-ish round trip). This keeps `time()` in sync across devices; Firestorm and
  pixelblaze-client both implement it.

### Pixel mapper
- Mapper tab accepts JSON coordinate array `[[x,y],…]` / `[[x,y,z],…]` or a **browser JS
  function** `function(pixelCount){…return map}` evaluated once in the browser.
- Coordinates auto-normalized to world units 0..1 (exclusive); fit modes Fill / Contain
  (`mapperFit`). 1D maps added v3.66.
- Compiled binary (`/pixelmap.dat`, ws type 8): 3×u32 LE header = formatVersion (1|2),
  numDimensions, dataSize; then per-pixel u8 (v1) or u16 LE (v2) quantized coords.

### Playlists / sequencer
- Modes: Off / Shuffle All / Playlist / Synchronized. 64→128 entries, per-item ms,
  fade-through-black + next-pattern preload (v3.40).

### UI controls (export-function naming conventions)
- `export function sliderFoo(v)`, `hsvPickerFoo(h,s,v)`, `rgbPickerFoo(r,g,b)`,
  `toggleFoo(on)`, `triggerFoo()` (button, not called at load), `inputNumberFoo(v)`;
  output widgets `showNumberFoo()` and `gaugeFoo()` (return value displayed).
  Values persist per-pattern; get/set via websocket.

### Native sync (v3.40+, improved to v3.67)
- Leader/follower groups in web UI (works in AP mode). Syncs: pattern launch + code,
  timebase, brightness, playlists, live-edits in realtime, sensor data broadcast.
  `nodeId()` differentiates devices in shared patterns. Leader-timeout fallback (v3.66).

### Release timeline highlights
- v3.17/18 coordinate transforms (2021); v3.20 array literals; v3.24 2D/3D previews,
  backup/restore; v3.30 palettes + Perlin; v3.40 sync groups; v3.47 LED driver rewrite,
  HDR 48-bit, autocomplete; v3.66 community-pattern browser in firmware, 1D maps,
  external websocket (Dec 2024); v3.67 bugfixes (Nov 2025).

## 3. Openness
- **Closed**: main firmware (compiler, VM, LED drivers, web app). No flash-your-own-ESP32.
- **Open**: sensor board (MIT/CC-BY-SA), output expander + protocol (MIT/CC-BY-SA),
  Firestorm source (public, but **no license file**), pixelblaze-client (MIT, Python; the
  de-facto protocol reference), expression/mapper/websocket docs.
- Ecosystem: patterns.electromage.com (200+ community .epe patterns, full-text search,
  in-firmware install), forum (Discourse), Python/Node/MQTT clients, PixelTeleporter,
  jasoncoon's led-mapper.

## Sources
- https://github.com/simap/pixelblaze (README, README.expressions.md, README.mapper.md)
- https://github.com/simap/pixelblaze_output_expander · https://github.com/simap/pixelblaze_sensor_board
- https://github.com/simap/Firestorm · https://github.com/zranger1/pixelblaze-client
- https://electromage.com/docs/… (language-reference, websockets-api, intro-to-mapping, quickstarts)
- https://www.crowdsupply.com/hencke-technologies/pixelblaze-v3 · https://www.adafruit.com/product/5943
- forum.electromage.com release threads (v3.40 sync, v3.47, v3.66)
