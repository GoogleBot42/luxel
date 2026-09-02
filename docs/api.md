# HTTP API reference

Every route a Luxel device (`firmware/src/server.rs`) and the native mirror
(`luxel serve`, `crates/luxel-cli/src/serve.rs`) serve. The two are kept in
lockstep on purpose — the playground's device mode talks to either — so the
**Where** column only ever says "both" unless something genuinely exists on
one side.

- Device: port **80** (`http://<device-ip>/`).
- Mirror: `luxel serve --port 8720` binds **127.0.0.1** only.
- Every response carries `Access-Control-Allow-Origin: *`, and `OPTIONS <any
  path>` answers the CORS preflight (`204`, `Allow-Methods: GET, POST, DELETE,
  OPTIONS`, `Allow-Headers: Content-Type`, `Max-Age: 86400`). Cross-origin
  `DELETE` needs it; simple GET/POST skip it.
- **Request bodies are never JSON.** They are plain text (whitespace- or
  line-separated), or a binary frame. Responses are JSON except where noted.
- Errors come back as **HTTP 200** with `{"ok":false,"error":"…"}`. A `404`
  means the route (or asset) does not exist at all. Don't switch on the status
  code — read `ok`.

## Fixed-point: what "raw 16.16" means

The engine's numbers are `Fx`, a signed 16.16 fixed-point value. `raw` is that
value as an `i32`: **`raw = round(number * 65536)`**. Which representation a
field uses is not uniform, so check the table:

| surface | encoding |
|---|---|
| `GET /api/vars`, `GET /api/readouts` | **raw** integers (`1.5` → `98304`) |
| `POST /api/control`, `POST /api/var` | **raw** integers |
| `POST /api/map` coordinates | **raw** integers |
| `POST /api/playlist` `C` lines | **raw** integers |
| `GET /api/playlist` → `items[].controls` | **decimal** (`Fx`'s `Display`, e.g. `0.5`) |
| `POST /api/events` frame payload | **raw** i32-LE |
| everything else (brightness, gamma tenths, cap mA, percentages) | plain integers |

The playlist asymmetry is real: you POST raw and you GET decimal.

## Status and diagnostics

| route | method | body | response | where |
|---|---|---|---|---|
| `/api/status` | GET | — | see below | both |
| `/api/pixels` | GET | — | `application/octet-stream`: last rendered frame, raw RGB, 3 bytes per pixel | both |

`GET /api/status` on **firmware**:

```json
{"fps":42,"pixels":300,"max_pixels":2048,"slot":"ota_0","version":"0.1.39",
 "heap_free":104832,"live":null,"src":true,"bc":true,"web":[0,1,0],
 "vmerr":null}
```

- `max_pixels` — this board's cap: 4096 on HUB75-panel boards, 2048 otherwise.
- `slot` — `factory` / `ota_0` / `ota_1` / `ota_?` / `unknown` (which app
  partition booted). Check this after a power-cycle test: a rollback shows up
  here and nowhere else.
- `heap_free` — bytes, `esp_alloc::HEAP.free()`.
- `live` — `"ddp"` / `"e131"` while a live pixel stream is driving the strip,
  else `null`.
- `src` / `bc` — whether the running pattern's source / bytecode are still
  readable back (`GET /api/pattern`); `false` means a flash write shed the copy.
- `web` — per-HTTP-slot lifecycle stage, one entry per connection slot
  (`WEB_TASK_POOL_SIZE`, 2 or 3). `0` accepting · `1` serving · `2` shutdown
  entered · `3` FIN sent · `4` discard done · `5` flush done · `9` abort. A slot
  parked at 3–5 is wedged on a client that won't close.
- `vmerr` — last VM error string, or `null`.

`GET /api/status` on the **mirror** carries `fps`, `pixels`, `max_pixels`
(always 2048), `slot` (always `"native"`), `version`, `heap_free` (0 unless
`--heap-free N` was passed), `live`, `vmerr` — **no `src`, `bc`, or `web`.**

## Live coding and the running pattern

| route | method | body | response | where |
|---|---|---|---|---|
| `/api/code` | POST | LXP1 envelope, empty name (binary) | `{"ok":true}`, or an error shape | both |
| `/api/pattern` | GET | — | `text/plain`: the running pattern's source | both |
| `/api/pattern.lxp` | GET | — | `application/octet-stream`: the running pattern as an LXP1 envelope (empty name) | both |
| `/api/controls` | GET | — | `[{"kind","label","name"},…]` (`[]` when none) | both |
| `/api/control` | POST | `name raw0 [raw1 raw2]` | `{"ok":true}` | both |
| `/api/vars` | GET | — | `{"name":raw \| [raw,…] \| null,…}` | both |
| `/api/var` | POST | `name raw` | `{"ok":true}` | both |
| `/api/readouts` | GET | — | `{"showName":raw \| null,…}` — `showNumber`/`gauge` controls only | both |

`kind` is one of `slider`, `hsvPicker`, `rgbPicker`, `toggle`, `trigger`,
`inputNumber`, `showNumber`, `gauge`.

**The LXP1 envelope** (`luxel_core::bytecode::encode_envelope`) is the upload
format for both `/api/code` and `/api/patterns`. Devices do not compile — the
browser or CLI does, and ships source + bytecode together:

```
"LXP1" | u8 name_len | name | u32-LE src_len | source | u32-LE bc_len | bytecode
```

all little-endian, name capped at 255 bytes. `POST /api/code` sends an **empty
name**; `POST /api/patterns` sends the pattern's name.

Error shapes from an upload:

- `{"ok":false,"error":"…"}` — malformed envelope or invalid bytecode.
- `{"ok":false,"code":"bc-version","error":"…"}` — the blob's format version
  doesn't match this firmware. **Recompile from source and re-upload**; the
  client is expected to branch on `code`.
- Firmware only: `empty upload (the request needs a Content-Length body)`,
  `pattern upload too large (N KB; this device accepts up to 80 KB)`,
  `not enough free memory on the device for this N KB upload …`,
  `upload truncated`.

A successful `/api/code` **stops the playlist** — a manual push takes over.

`POST /api/control` on the firmware also records the tweak for single-pattern
reboot resume, but only while no playlist is playing and the running pattern is
a stored one.

## Pattern library

| route | method | body | response | where |
|---|---|---|---|---|
| `/api/patterns` | GET | — | `{"patterns":[{"id","name"},…]}` | both |
| `/api/patterns` | POST | LXP1 envelope with a name | `{"ok":true,"id":"<hex>"}` | both |
| `/api/patterns/<id>` | GET | — | `{"id","name","source"}` | both |
| `/api/patterns/<id>` | DELETE | — | `{"ok":true}` | both |
| `/api/patterns/<id>/activate` | POST | — | `{"ok":true}` | both |

- A missing `<id>` returns **200** with `{"ok":false,"error":"no such
  pattern"}`, not a 404 — on both sides, deliberately.
- Saving under a name that already exists **overwrites** that entry and returns
  its existing `id`.
- Firmware save errors are size/space specific: `pattern name required`,
  `name must be 1..=64 bytes`, `this pattern's source is too large for the
  on-device library (…)`, `… compiled code is too large …`, `the device library
  is full (N patterns) — delete one first`, `couldn't write the pattern to flash
  …`, `pattern storage unavailable (device needs reflash)`.
- `activate` re-validates the stored blob, so it too can answer
  `{"ok":false,"code":"bc-version",…}` after a firmware format bump. It resets
  controls to the pattern's defaults.
- A bad sub-path under `/api/patterns/` answers `{"ok":false,"error":"bad
  patterns route"}`.

## Playlist

| route | method | body | response | where |
|---|---|---|---|---|
| `/api/playlist` | GET | — | see below | both |
| `/api/playlist` | POST | line format, see below | `{"ok":true}` | both |
| `/api/playlist/play` | POST | start index (decimal, default `0`) | `{"ok":true}` | both |
| `/api/playlist/stop` | POST | — | `{"ok":true}` | both |
| `/api/playlist/next` | POST | — | `{"ok":true}` | both |
| `/api/playlist/prev` | POST | — | `{"ok":true}` | both |

`GET`:

```json
{"defaultSec":30,"crossfadeMs":500,"playing":true,"index":2,
 "items":[{"id":"1a5e0001","name":"sparks","sec":null,
           "controls":{"speed":[0.5]},"invalid":"needs 2D map"}]}
```

`sec` is `null` when the item inherits `defaultSec`. `controls` values are
**decimal**, and the `invalid` key is present only when the item's `assert()`
invariants fail against the current config (pre-flight check) — absent means
fine, or still being computed.

`POST` body is line-based, not JSON:

| line | meaning |
|---|---|
| `D <sec>` | default seconds per item |
| `X <ms>` | crossfade milliseconds |
| `I <patternId> <sec>` | an item; `sec` `-1` (or unparseable) = inherit the default |
| `C <name> <raw…>` | a control override for the item most recently declared; **raw 16.16** |

Unrecognized lines are ignored. Firmware persists the body verbatim to flash;
both sides apply edits live if already playing.

## Device settings

All of these apply **live** and (on firmware) **persist to flash** — no reboot.

| route | method | body | response | where |
|---|---|---|---|---|
| `/api/brightness` | GET | — | `{"brightness":0..31,"max":31}` | both |
| `/api/brightness` | POST | `0`..`31` | `{"ok":true,"brightness":N}` | both |
| `/api/config` | GET | — | `{"pixels":N,"max":N,"protocol":"sk9822"}` + on strip-board firmware `"data_pin":N,"data_pin_default":N,"data_pin_next":N\|null,"data_pins":[…]` | both (pin fields firmware only) |
| `/api/config` | POST | pixel count `1..=max` | `{"ok":true,"pixels":N}` | both |
| `/api/datapin` | POST | GPIO number from `data_pins`, or `default` | `{"ok":true,"data_pin":N,"note":"rebooting to apply"}` — **firmware reboots**; a rejected pin answers `{"ok":false,…}` and does not | firmware only (strip boards) |
| `/api/protocol` | GET | — | `{"protocol":"sk9822","options":["sk9822","ws2812"]}` | both |
| `/api/protocol` | POST | protocol name | `{"ok":true,"protocol":"…"}` | both |
| `/api/output` | GET | — | see below | both |
| `/api/output` | POST | `<order> <gamma_tenths> <cap_ma> [<bright_curve_tenths> <blur_pct> <glow_pct>]` | `{"ok":true,"order","gamma","capMa","brightCurve","blur","glow"}` | both |
| `/api/output/palette` | POST | `<amount_pct> <pos> <r> <g> <b> …` | firmware `{"ok":true}`; mirror `{"ok":true,"palette":[…],"paletteAmount":N}` | both |
| `/api/output/palette` | DELETE | — | `{"ok":true}` | both |
| `/api/map` | GET | — | `{"installed":bool,"dims":2\|3\|0,"count":N}` | both |
| `/api/map` | POST | `<dims> <raw…>` | `{"ok":true,"installed":bool,"count":N}` | both |
| `/api/clock` | GET | — | `{"synced":bool,"local":<unix secs, local>,"tzMinutes":N}` | both |
| `/api/clock` | POST | tz offset from UTC in minutes | `{"ok":true,"tzMinutes":N}` | both |

- `POST /api/config` `max` is the board cap (2048, or 4096 on HUB75 boards);
  the mirror is always 2048.
- `/api/datapin` is the one setting here that is NOT live (Gitea #154): the
  strip driver binds its DATA pin at boot, so the value is persisted and the
  device reboots. `data_pin_next` in `GET /api/config` is non-null only
  between a POST and that reboot. See docs/boards.md "Runtime pins" for
  which pins a board allows and why.
- Protocol names accepted: `sk9822`/`apa102`, and
  `ws2812`/`ws2811`/`ws2815`/`ws281x`. The reply always echoes the canonical
  `sk9822` or `ws2812`.
- `GET /api/output` →
  `{"order":"grb","gamma":22,"capMa":1500,"brightCurve":22,"blur":20,
  "glow":40,"palette":[pos,r,g,b,…],"paletteAmount":0..100}`. One fetch backs
  the whole Output card. `palette` is a flat `[pos,r,g,b,…]` array, 0..=255 per
  component; `[]` means no device palette.
- `POST /api/output`'s last three fields are optional — absent means "keep the
  stored value", so pre-post-process clients keep working. Present but
  out-of-range fails the whole request. Ranges: gamma tenths 0–50, cap mA
  0–20000, bright-curve tenths 0–50, blur/glow percent 0–100. Order is one of
  `rgb rbg grb gbr brg bgr`.
- The device palette **composes with** a pattern's own `setOutputPalette`
  rather than replacing it.
- `POST /api/map` takes `dims` (2 or 3) followed by `dims` raw 16.16 coordinates
  per pixel. An empty or unparseable body **clears** the map and answers
  `"installed":false`.
- `POST /api/clock` accepts −840..=840 minutes.
- Firmware settings whose flash write fails still apply live and add
  `"note":"not persisted: …"` to the `{"ok":true,…}` body (`/api/brightness`,
  `/api/config`, `/api/protocol`).

## Network, provisioning, and integrations

| route | method | body | response | where |
|---|---|---|---|---|
| `/api/wifi` | GET | — | `{"ssid":"…"\|null,"source":"flash"\|"builtin"\|"none"}` | both |
| `/api/wifi` | POST | `ssid\npassword` | `{"ok":true,"ssid":"…","note":"rebooting to apply"}` — **firmware reboots** | both |
| `/api/apmode` | GET | — | `{"ap":bool}` | both |
| `/api/apmode` | POST | any (ignored) | `{"ok":true,"note":"rebooting into the setup AP (one boot only)"}` — **firmware reboots** | both |
| `/api/mqtt` | GET | — | `{"enabled","host","port","user","hasPass","connected"}` | both |
| `/api/mqtt` | POST | `host\nport\nuser\npass` | `{"ok":true,"enabled":bool}` | both |
| `/api/sync` | GET | — | `{"mode","timeMs","leader":{"bootId","ageMs","offsetMs"}\|null}` | both |
| `/api/sync` | POST | `off` \| `leader` \| `follower` | `{"ok":true,"mode":"…"}` | both |

- The password is **never** returned by `GET /api/wifi` or `GET /api/mqtt`
  (`hasPass` is the only signal). `source` says where the next boot's SSID comes
  from: flash creds, a build-time `LUXEL_SSID`, or nothing.
- `POST /api/wifi` validates: `ssid must be 1..=32 bytes`,
  `password too long (max 64 bytes)`. A rejected body does **not** reboot.
- `POST /api/apmode` sets a one-shot force-AP flag; the device comes back as
  `luxel-xxxx` at `192.168.4.1` with a captive portal for exactly one boot.
  While in AP mode, **any unknown GET path answers `307` to
  `http://192.168.4.1/`** (captive-portal detection).
- `POST /api/mqtt` with an empty host disables MQTT. Port `0` or unparseable
  becomes `1883`. The MQTT task reconnects live — no reboot. Topic reference:
  `docs/mqtt.md`.
- Sync `mode` is `off` / `leader` / `follower`; `timeMs` is the engine
  timebase. Leader beacons are UDP `:4049` (`LXS2`), not HTTP; a follower
  adopting the leader's pattern fetches `GET /api/pattern.lxp` from it.
- Mirror stubs: it has no radio, so `GET /api/apmode` is always
  `{"ap":false}`, `POST /api/apmode` answers `{"ok":true,"note":"mirror: no
  radio; …"}` for parity, and `POST /api/wifi` stores the SSID without
  rebooting (it still returns the `"rebooting to apply"` note). Its
  `GET /api/clock` is always `"synced":true` (host clock).

## Injection surfaces

Three ways to drive a pattern's inputs from outside. All are POST-only, all
answer `{"ok":true}` on acceptance.

| route | method | body | response | where |
|---|---|---|---|---|
| `/api/events` | POST | binary `EV1\0` frame | `{"ok":true}` / `{"ok":false,"error":"not an event frame"}` | both |
| `/api/sensors` | POST | binary sensor-board frame | `{"ok":true}` / `{"ok":false,"error":"not a sensor-board frame"}` | both |
| `/api/pins` | POST | text, one `<pin> <level>` or `a <pin> <0..1>` per line | `{"ok":true,"pins":N}` | **mirror only** |

**`/api/events`** — feeds `readEvent()` / `eventCount()`.
`luxel_core::netin::parse_events`:

```
"EV1\0" | u8 count | count × 4 × i32-LE raw 16.16 [type, x, y, value]
```

The length must match **exactly** (`5 + count*16`) and `count` is capped at the
engine's event-queue size (`vm::MAX_EVENTS`); anything else is rejected whole.
`luxel_core::netin::build_events` builds one, and the web client encodes the
same layout in TS.

**`/api/sensors`** — feeds the sensor bindings (`frequencyData`,
`energyAverage`, `accelerometer`, `light`, `analogInputs`, …). The body is one
raw PB sensor-expansion-board frame, byte-identical to what the serial board
streams (`luxel_core::netin::parse_sensor_board`): 98 bytes,
`"SB1.0\0"` + 32×u16 freq + u16 energyAverage + u16 maxFreqMagnitude +
u16 maxFreqHz + 3×s16 accel + u16 light + 5×u16 analog + `"END\0"`, all
little-endian. u16 fields are raw 16.16 fractions in 0..1.

**`/api/pins`** — feeds `digitalRead()` and `analogRead()`/`touchRead()`. Text,
one write per line.

```
26 0            digital: pin 26 LOW
27 high         digital: pin 27 HIGH
4 x             digital: release pin 4 to its pinMode idle level
a 33 0.42       analog:  analogRead(33)/touchRead(33) read 0.42
analog 33 x     analog:  release pin 33 (an undriven analog pin reads 0)
```

A digital level is `0`/`low`/`false`/`off`, `1`/`high`/`true`/`on`, or
`x`/`-`/`release`/`idle` to hand the pin back to its `pinMode` idle level. A
leading `a` (`analog`/`touch` also accepted) marks an **analog** write (Gitea
#206), whose value is a 0..1 number clamped by the engine; `x`/`release` there
means 0, because an undriven analog pin reads 0 and there is no idle level to
return to. Both builtins share one value per pin.

Blank/comment/unparseable lines are skipped and the response reports how many
writes actually landed (`"pins":N`); a body that yields zero writes answers
`{"ok":false,"error":"want lines of \"<pin> <0|1|x>\" or \"a <pin> <0..1>\""}`.
At most `PIN_MAX_BATCH` writes per request.

> **`/api/pins` is mirror-only, by design.** On a device the pins a pattern
> names are real pads, synced with the engine every frame (Gitea #177 item 4,
> `firmware/src/gpio.rs`) — an injected level would be overwritten by the wire
> on the next frame, so there is nothing for the route to do. A POST to it on
> a device 404s.

## Firmware maintenance (device only)

| route | method | body | response | where |
|---|---|---|---|---|
| `/api/ota` | POST | raw app image (streamed) | `{"ok":true,"bytes":N}` — **then reboots** | firmware only |
| `/api/assets` | POST | LUXA asset archive (streamed) | `{"ok":true,"bytes":N,"files":N}` | firmware only |

- `POST /api/ota` writes the inactive OTA slot, then reboots ~400 ms after
  replying so the response reaches the client. It freezes the render engine
  first to free heap for the flash phase. Failures answer
  `{"ok":false,"error":"…"}` and do **not** reboot. Driven by
  `tools/ota-push.sh` / `tools/deploy.sh`; see `docs/firmware.md`.
- `POST /api/assets` streams the web-app archive into the assets flash region
  and hot-reloads the TOC — **no reboot**. A serial flash leaves this partition
  stale, so follow one with `tools/deploy.sh <ip> --assets-only`.
- On a **`hosted-ui`** image `/api/assets` exists but refuses:
  `{"ok":false,"error":"hosted-ui build: this image has no on-device web app"}`
  — deliberately, so `--assets-only` gets an explanation instead of a 404.
- The mirror serves neither route (a POST 404s): it has no flash and its
  playground comes from `web/dist` on disk.

## Pages and static assets

| route | method | response | where |
|---|---|---|---|
| `/` | GET | the installed playground's `index.html`, else the embedded minimal page | both |
| `/min` | GET | always the embedded minimal page | both |
| any other GET | GET | a static asset, else `404 not found` | both |

- Firmware serves assets out of the flash archive with a strong `ETag` and
  `Cache-Control`, answering `304` when `If-None-Match` still matches.
  Content-hashed bundle paths (`/assets/index-<hash>.js`) get
  `public, max-age=31536000, immutable`; everything else `no-cache`.
- A `hosted-ui` firmware image serves no assets at all — only `/` and `/min`
  (the embedded page) plus the API.
- The mirror serves from the built playground directory (`--web-dir`, else
  `web/dist` / `dist` / `../web/dist`), refusing path traversal; when nothing is
  built, `/` falls back to the same minimal page.

## Gotchas

- **`ok:false` arrives with HTTP 200.** Only genuinely-unrouted paths 404.
- **Raw 16.16 in, decimal out** for playlist controls (see the table at the
  top). `/api/vars` is raw on both sides — `tools/event-soak.mjs` divides by
  65536 for exactly this reason.
- **Two routes reboot the device**: `POST /api/wifi` and `POST /api/apmode`
  (immediately after replying), plus `POST /api/ota` on success. Nothing else
  does — brightness, pixel count, protocol, output, map and MQTT all apply live.
- **The device has 2–3 HTTP connection slots**, keep-alive, with a 45 s
  whole-body read timeout. An abandoned upload pins a slot until it expires.
  Client-side: serialize your requests (the playground gates every fetch to 2
  in flight with backoff-retry) rather than fanning out.
- **Don't parse `/api/pixels` as text** — it is `application/octet-stream`,
  `3 × pixels` bytes.
- **`/api/pattern` and `/api/pattern.lxp` stream from flash** on a device and
  are padded with `\n` to the promised `Content-Length` if a read fails
  mid-response — so a well-formed but newline-tailed body can mean a busy
  flash, not an empty pattern. `GET /api/status`'s `src`/`bc` flags say whether
  read-back is available at all.
- **Uploads are capped at 80 KB** on a device, and can still be refused for
  free heap below that.
- **No route takes a query string**, and the two targets disagree about them:
  the mirror strips `?…` before matching, the firmware does not. Don't append
  one.
- **`version` + `slot` are the Luxel fingerprint.** The installer page
  (`web/src/flash/lib/device.ts`) classifies a host as a Luxel device by
  `GET /api/status` returning both as strings — that is what distinguishes it
  from a WLED box on the same LAN.

## Where this is used

Reference consumers, if you want a worked example rather than a table:

- `web/src/lib/device.ts` — `DeviceSession`, the playground's typed client for
  nearly every route here. All of its requests go through
  `web/src/lib/fetchgate.ts`, which caps the app at 2 in-flight fetches with
  backoff-retry on refused connections.
- `tools/wire-check.sh` — curl-level contract check of the HTTP surface
  (Content-Type/Content-Length, asset 200/304, the four preflight headers, the
  404 shape). Run it after any `firmware/src/server.rs` change lands.
- `tools/serve-e2e.mjs` — fetch-only smoke test of the mirror, including the
  events and pins injection paths end-to-end into a live pattern's pixels.
- `web/tools/device-e2e.mjs` — the full browser-driven pass over the settings
  surfaces.
- `web/tools/lxp.mjs` — builds the LXP1 envelopes uploads need.

`tools/verify/review.mjs` runs its **own** unrelated local server that also
uses `/api/…` paths (`/api/data`, `/api/decision`, `/api/decisions`). It has
nothing to do with the device API.
