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

`/api/pixels` is the **engine's** frame, not the wire's: the device output
chain (`/api/output` — palette, blur, glow, colour order, gamma, power cap)
runs after the snapshot on both kinds of board, so a readback shows a
pattern's own `setBlur`/`setGlow`/`setOutputPalette` exactly and the Settings
chain not at all (docs/firmware.md). The playground runs that chain itself
through `lx_outpipe` (Gitea #466) rather than reading it back.

`/api/pixels` answers an **empty body** when there is no frame yet (before the
first render) and, on firmware, when the heap cannot hold the response — at
4096 px that is a 12 KB body on a heap a heavy pattern can leave under 30 KB
free, so it degrades rather than failing the request. Treat a zero-length
response as "no snapshot right now", not as an all-black frame.

`GET /api/status` on **firmware**:

```json
{"name":"luxel-4ae0d4","fps":42,"frame_us":8100,"vm_us":5200,"pipe_us":1400,"out_us":1300,"out_fps":0,
 "rescan_hz":0,"dropped":0,"pixels":300,"max_pixels":2048,
 "geom":{"dims":1,"regular":true,"w":300,"h":1,"source":"board","pattern_dims":1,
        "compatible":true},
 "caps":{"strip_driver":true,"panel":false,"outputs":1,"power_cap":true,"blur_glow":true,
         "layers":3,"text_slots":0,"reboot":true,"ota":true,"psram":false,"assets":false},
 "slot":"ota_0","version":"0.1.39",
 "heap_free":104832,"heap_largest":73728,"engine_heap":21504,"live":null,
 "assets_mapped":true,"code_mapped":true,
 "store":{"used":18452,"total":749568,"dead":0,"patterns":3},
 "src":true,"bc":true,"web":[0,1,0],"vmerr":null}
```

A HUB75 panel additionally carries the pipelined-output members — `out_fps`,
`rescan_hz`, and the two diagnostic objects, which are this quiet on a board
that is not losing anything:

```json
 "out_fps":112,"rescan_hz":115,"dropped":0,
 "drops":{"handoff":0,"overwrite":0,"refused":0,"hist":[],"n":0,"log":[]},
 "swap":{"eof_race":91,"slow_path":604}
```

### `geom` — the engine's EFFECTIVE geometry

**Not the device map.** `GET /api/map` reports what is *installed*; `geom`
reports what the running engine is actually rendering through, which is a
different thing whenever the engine supplied its own geometry. A pattern that
only exports `render2D` (or `renderFrame` plus a coordinate-using bulk op) gets
a fabricated `ceil(√pixelCount)` square grid — PB-compatible behaviour, see
`Engine::set_default_grid_map` — and `/api/map` answers `{"installed":false}`
for it. That is the case `source:"default"` exists to make visible.

Derived in `luxel_core::caps::Geom::derive`, which the firmware and the mirror
both call, so the two cannot drift. Published by the render task when the
engine or the map changes (never per frame).

| field | meaning |
|---|---|
| `dims` | `1`, `2` or `3` — the dimensionality of the engine's installed map, or `1` when it has none (a bare strip's index space). |
| `regular` | The layout is a lattice addressable as `w`×`h`: true for a strip (1×N) and for any grid, procedural or recovered from a coordinate map by `outpipe::detect_grid`. False for an irregular coordinate cloud. |
| `w` / `h` | The lattice, when `regular`. A strip is `w = pixels`, `h = 1`. Both `0` when `regular` is false. |
| `source` | `"user"` a map installed through `POST /api/map` (persisted in flash) · `"board"` the board's own geometry — a HUB75 panel's `64×64` grid, or a strip's bare index space · `"default"` the engine's fabricated square grid, which nothing else reports. |
| `pattern_dims` | What the RUNNING PATTERN wants: `0` no preference (`renderFrame` in index space) · `1` `render` · `2` `render2D` · `3` `render3D`. Differs from `dims` exactly when the pattern is being projected onto a layout of another shape. `0` while no engine is resident. |
| `compatible` | False when the LAYOUT cannot show a pattern of this dimensionality at all (Gitea #538): a strip handed a 2D/3D pattern, a plane handed a 3D one. A host never *offers* such a pattern — the UI hides or flags it — but the engine still renders one it is handed, so a stale playlist entry or a share link cannot black out a device. See docs/spec/projection.md §1a. **It reads the LAYOUT's dims, not `dims`**: a `source:"default"` geometry is the engine's fabricated grid papering over a bare strip, which is exactly the case to flag. |

The combinations, in full:

| situation | `dims` | `regular` | `w`/`h` | `source` | `compatible` |
|---|:-:|:-:|---|---|:-:|
| strip board, 1D or `renderFrame` pattern, no map | 1 | true | `pixels` / 1 | `board` | true |
| strip board, `render2D`-only pattern, no map | 2 | true | the ceil(√n) grid | `default` | **false** |
| HUB75 panel board, nothing installed | 2 | true | 64 / 64 | `board` | true unless the pattern is 3D |
| `POST /api/map grid W H` | 2 | true | W / H | `user` | true unless the pattern is 3D |
| `POST /api/map` 2D coords that `detect_grid` recognises | 2 | true | the detected grid | `user` | true unless the pattern is 3D |
| `POST /api/map` 2D coords that it does not | 2 | false | 0 / 0 | `user` | true unless the pattern is 3D |
| `POST /api/map` 3D coords (never a grid) | 3 | false | 0 / 0 | `user` | true |
| no engine resident (frozen for an OTA) | the device map's | — | — | `board`/`user` | true (`pattern_dims` is 0) |

A map whose pixel count does not match the device's is truncated to the device's
by the engine, so a `grid 16 8` on a 60 px strip is 60 coordinates and no longer
a grid: `regular` goes false. That is the device's real behaviour, not a
reporting artefact.

### `caps` — what the device can do

The UI shows a setting only when its capability is advertised — **absent, never
disabled** (proposal §5.3/§5.7). This replaces the old "`data_pins` missing from
`/api/config` ⇒ this is a panel" inference. Board-shaped facts are `cfg!`s in
`firmware/src/server.rs::device_caps`; the layout-shaped derivation is
`luxel_core::caps::Caps::derive`, shared with the mirror.

| field | meaning |
|---|---|
| `strip_driver` | Drives addressable strips: LED type, colour order and (where `/api/config` reports `data_pins`) the data pin are real settings. False on a HUB75 board. |
| `panel` | Drives a HUB75 matrix: panel size, scan, clock and planes are real. |
| `outputs` | Physical LED outputs the **board** has, whatever the firmware drives today (the Athom has 2; Gitea #474 makes the second one real). `> 1` is what turns the Settings page's Outputs table on. |
| `power_cap` | A per-pixel current model exists. True on strips; false on a panel, which is a fixed load on a supply sized for it. |
| `blur_glow` | The device output chain's blur and glow stages are offered. Two independent gates, ANDed: they need neighbours — index order on a strip, rows/columns on a regular grid — so it follows `geom.regular`; and they need to fit the board's per-frame budget, so `board::BLUR_GLOW` is **false on every HUB75 panel board**, whose compose window is one ~8.66 ms rescan that the two spatial stages over a 64×64 grid overrun (Gitea #476/#446, proposal D12). The pattern-side `setBlur`/`setGlow` are a different chain and are unaffected either way. |
| `layers` | Pattern layers this board affords for a scene (Phase B): 3 at ≤512 px, 2 above — the per-layer 3 B/px frame in internal DRAM is the binding constraint, which is the "2 on the S3 panel" in the design. It sizes the scene editor's "2 of 2 used" note; the real gate is the editor's own budget check against live heap. |
| `text_slots` | Host-settable text slots (proposal §6). `0` everywhere until Phase C. |
| `reboot` | Can reboot itself (setup AP, data-pin change, WiFi change). |
| `ota` | Accepts a firmware image over the network. |
| `psram` | Has the external pattern-array arena (Gitea #253). |
| `assets` | User-uploadable fonts/images as a scene layer source. `false` everywhere — not planned; the assets partition is a whole-bundle path, not a per-file one. |

- `frame_us` / `vm_us` / `pipe_us` / `out_us` — per-stage frame timing, the
  average microseconds per rendered frame over the last second: the whole
  engine branch, `Engine::frame` (the VM), the output pipeline (gamma /
  palette / blur, plus the preview copy where there is one), and the
  LED/HUB75 driver's `write_frame`. The stages nest — `frame_us` ≈ the other
  three plus per-frame bookkeeping. Only *pattern* frames are timed, so all
  four read 0 in a second where live input drove the strip or no engine was
  loaded.

  **Except where `out_fps` is nonzero.** On a board that pipelines the output
  stage onto the other core (HUB75 panels — docs/firmware.md "The frame
  pipeline"), `frame_us` is the render PERIOD and no longer contains
  `pipe_us` or `out_us`; those two come from the output task and are averaged
  over `out_fps` frames, not `fps` frames. The frame period there is
  `max(frame_us, pipe_us + out_us)`.
- `out_fps` — frames the output driver actually **took** in the last second,
  on a pipelined board; `0` everywhere else, where every rendered frame is
  written by construction. On a strip that is frames on the wire; on a HUB75
  panel it is frames the panel scanned out. Frames the driver refused — a
  panel whose previous buffer swap has not landed — are not counted (Gitea
  #378; it used to count every call, which made it a compose rate reading up
  to 125 against a 115 Hz rescan).

  On the panel `fps` and `out_fps` should now read the **same** number, a
  hair under `rescan_hz`: the render loop is paced by the panel itself
  (Gitea #387), so exactly one frame is composed, swapped and displayed per
  rescan and nothing is rendered only to be thrown away. `fps` above
  `out_fps` means composed frames are being discarded; `out_fps` a little
  below `rescan_hz` means the occasional compose overran its rescan window
  and the panel showed the previous frame once more.
- `dropped` — rendered frames the fixture never showed, cumulative since
  boot. **The ground truth for "was that a dropped frame?"** — it is derived
  from the gap between the sequence numbers of consecutive *displayed*
  frames, not by counting known loss routes, so it covers every way a frame
  can go missing between the VM and the wire, including routes the firmware
  does not enumerate. `0` on a non-pipelined board, where every rendered
  frame is written by construction. Under vsync pacing (Gitea #387) the
  pipeline is lossless and this should stay at its boot value; expect a
  handful of drops in the first two seconds after a reboot, before the
  output task has published the driver's pacing capability.
- `drops` — where those frames went, on a pipelined board. `handoff` is
  frames the render task could not hand over (no buffer came back in time),
  `overwrite` is frames replaced in the slot before the output task took
  them, `refused` is frames the driver would not take. `hist` bins lost
  frames by frame number mod 64 as sparse `[column, count]` pairs — with
  `library/frame-rate-scan.js` that is the sweep column, so a cluster says
  the loss correlates with what was being drawn and a flat spread says it
  does not. `log` is the last 16 losses as `[seq, route, ms]`, route being
  0 `handoff` / 1 `overwrite` / 2 `refused`, so a burst is distinguishable
  from a drip. All of it is empty on a healthy board.
- `swap` — HUB75 swap diagnostics from the patched esp-hub75. `eof_race`
  counts buffer swaps armed while a DMA end-of-frame interrupt was raised
  but not yet serviced. That is a window in which the naive "the next EOF is
  the switch" shortcut would hand the compose a framebuffer the DMA is still
  scanning out **and** undo the flip — a frame both torn and never
  displayed, and invisible to `dropped` because the write succeeded. The
  driver detects the window and falls back to a two-EOF wait, so these are
  handled, not lost; the counter exists because the rate is worth knowing
  (measured ~8/min on the bench panel). `slow_path` counts every swap that
  took that two-EOF fallback for any reason, each costing one extra panel
  frame of latency.
- `pass` — HUB75 descriptor-ring forensics (Gitea #395). A *pass* is one full
  traversal of a DMA descriptor ring; `suc_eof` sits on a ring's last
  descriptor only, so consecutive EOFs are exactly one ring apart and every
  pass must be the same length. `short` counts passes under 0.9x `nominal_us`
  — each one would be the engine entering a ring off its head, the only way a
  displayed frame can vanish while `write_frame` still reports success, so it
  **must be 0**. `long` counts passes over 1.5x, which is what a missed or
  coalesced EOF looks like. `per_frame_min`/`per_frame_max` are the fewest and
  most rescans between two consecutive *displayed* frames: `min` of 0 means a
  frame was handed to the DMA and never scanned out at all, and
  `zero_rescan` counts those. `shorts` logs the last few as
  `[corrected_us, packed_flags, frame_count]`.

  **`isr_lat_max` is the honesty check on the rest.** Pass timestamps are taken
  inside the frame-count ISR, so its dispatch jitter lands directly in the raw
  interval; at an EOF the engine has just wrapped to a ring head, so how far
  past that head `OUT_DSCR` has already travelled measures that latency in
  descriptors (~34 us each). The classified length is corrected by it. Without
  the correction the bench panel reported 30 "short" passes of as little as
  3.4 ms against an 8.7 ms nominal — pure ISR jitter, no truncation at all.
  `min_us`/`max_us` still include the pre-settling window, so read `short` and
  `long`, not the extremes.
  `skips` is the one that matters: frames composed and swapped in that the
  panel never scanned out. It is counted **in the frame-count ISR**, which
  sees every pass exactly once and in order; counting it caller-side means
  snapshotting a ring the ISR is concurrently writing, and that produced
  phantom skips twice while this bug was being chased. `repeats` counts passes
  that re-showed the previous frame (benign — the compose overran its window),
  `tags` is the last 24 displayed frame numbers so the sequence can be read
  directly, and `swap.landing_mismatch` counts flips the driver refused to
  call landed because the engine was provably still on the old ring.
- `rescan_hz` — how many times a second the HUB75 panel is really redrawn
  from the framebuffer, read from the driver's own BCM frame counter. `0` on
  every board without a panel. This is the panel's clock **and** the render
  loop's on such a board (Gitea #387), and the ceiling on `fps`/`out_fps`.
  Measured 115 on the 64x64 bench panel at 7
  bitplanes and a 30 MHz LCD_CAM clock; it scales linearly with that clock
  and halves per extra bitplane (docs/boards.md, "The LCD_CAM pixel clock on
  the panel"). Sampled where the frames are, so it reads 0 whenever nothing
  is rendering — exactly when `out_fps` does.
- `max_pixels` — this board's cap: 4096 on HUB75-panel boards, 2048 otherwise.
- `slot` — `factory` / `ota_0` / `ota_1` / `ota_?` / `unknown` (which app
  partition booted). Check this after a power-cycle test: a rollback shows up
  here and nowhere else.
- `heap_free` — bytes, `esp_alloc::HEAP.free()`, measured with the CURRENT
  pattern's engine resident. It is a **sum over the free list**, so it does
  not say whether any single allocation of that size would succeed.
- `heap_largest` — bytes in the largest single allocation the heap can
  satisfy right now (`shared::largest_free_block`). This is the figure that
  decides whether a pattern upload or an engine swap fits; the gap between it
  and `heap_free` is fragmentation. Gitea #390 was exactly that gap: a 30 KB
  upload refused with 74 KB free, cleared by a reboot. esp-alloc 0.10 has no
  API for it (`HEAP.free()`, `HEAP.used()` and `HeapStats`/`RegionStats` are
  all sums, on both the LLFF and the TLSF backend), so the firmware probes it
  — a short binary search of allocations that are freed again immediately.
  Device only; the mirror does not report it.
- `engine_heap` — bytes that engine occupies, measured across its load
  (`shared::ENGINE_HEAP`). **Not decoration: `heap_free` alone is not the
  budget an incoming pattern has.** The render task drops the outgoing engine
  before it decodes the incoming program, so a swap starts from
  `heap_free + engine_heap` — `luxel_core::budget::load_base`. 0 means "not
  measured" (nothing loaded, or a crossfade, which keeps the outgoing engine
  alive on purpose); treat 0 as `heap_free` alone, which is conservative.
  Added for Gitea #287, where predicting against `heap_free` made the
  playground warn about patterns that load fine.
- `live` — `"ddp"` / `"e131"` while a live pixel stream is driving the strip,
  else `null`.
- `assets_mapped` — `true` when the web assets partition is memory-mapped
  through the cache MMU (docs/firmware.md "Flash-mapped regions") and asset
  bodies are served straight from it; `false` means the boot-time mapping
  failed its self-check, the image was built with `flashmap-off`, or it is a
  hosted-ui build — assets (if any) then stream via flash-controller reads.
- `code_mapped` — `true` when the running pattern's bytecode is mapped
  memory the engine builds from without a blob copy (the built-in default's
  rodata, the ad-hoc read-back slot, or the library pattern's bytecode in
  the file log); `false` means the store's mapping is off (a `flashmap-off`
  build, or a refused boot self-check) — the pattern still runs, its
  bytecode is just read into a transient Vec first.
- `store` — the pattern store's file log, in **BYTES** (they were 4 KiB
  pages before Gitea #340): `used` by live files, `total` in the log
  (749,568 = 183 × 4 KiB of the `storage` partition), `dead` held by
  superseded and deleted files that the next compaction gives back, and how
  many `patterns` are stored. One file holds a pattern's header, name,
  source text and bytecode packed to its exact size — see docs/firmware.md
  "The pattern store: a packed file log in a mapped region + a small key
  area". `total` 0 means the store never came up — no `storage` partition,
  or one too small.
- `src` / `bc` — whether the running pattern's source / bytecode are still
  readable back (`GET /api/pattern`); `false` means a flash write shed the copy.
- `web` — per-HTTP-slot lifecycle stage, one entry per connection slot
  (`WEB_TASK_POOL_SIZE`, 2 or 3). `0` accepting · `1` serving · `2` shutdown
  entered · `3` FIN sent · `4` discard done · `5` flush done · `9` abort. A slot
  parked at 3–5 is wedged on a client that won't close.
- `vmerr` — last VM error string, or `null`. Cleared on every pattern load.
  One exception to "last": if the pattern's own init was refused an array
  (`array element budget exceeded` / `array memory budget exceeded`), the
  render pass records nothing further, so that refusal stands until the next
  load rather than being overwritten a frame later by the missing-buffer
  errors it causes (Gitea #420).
- `core1` — dual-core boards only (`esp32`, `esp32s3`); `null` elsewhere.
  The second core and the cross-core flash fence (docs/firmware.md
  "Cores & tasks"):
  `{"stack":[used,total],"fence_timeouts":0,"fence_wait_us":68,`
  `"fences":[begun,completed],"last":{"reset":"…","bb":[…]}}`.
  - `stack` — AppCpu stack high-water / allocated, bytes.
  - `fence_timeouts` — park requests the other core did not acknowledge
    within 100 ms (the flash op proceeded anyway). **Must stay 0.**
  - `fence_wait_us` — longest park wait seen, the render-side cost of one
    flash op.
  - `fences` — `[begun, completed]` this boot. A difference of 1 in
    `last.bb` is a fence that was taken and never released; live, the delta
    around an operation is its fence cost (Gitea #292).
  - `last` — the PREVIOUS run, from a black box in RTC memory that survives
    a reset: `reset` is the reset reason (`SysRtcWdt` = the RTC watchdog
    caught a wedge) and `bb` is
    `[magic, ProCpu phase, AppCpu phase, fences begun, ProCpu parks,
    AppCpu parks, park-ack timeouts, fences completed, call-site tag,
    AppCpu park count at the last fence]`. Phases: 0 idle, 1 waiting for
    the fence lock, 2 waiting for the park ack, 3 inside the fenced window,
    4 waiting for the release ack, 6 inside esp-storage/the ROM SPI1
    routine, 7 that call returned. Call-site tags: 0 other, 1/2/3 asset
    erase/write/read, 4/5 OTA erase/write, 6/7/8 pattern-store
    read/erase/write, 9/10 raw-region erase/write, 11 flash map.

`GET /api/status` on the **mirror** carries `fps`, `pixels`, `max_pixels`,
`geom`, `caps`, `slot` (always `"native"`), `version`, `heap_free` (0 unless
`--heap-free N` was passed), `engine_heap` (0 unless `--engine-heap N` was
passed), `live`, `vmerr` — **no `src`, `bc`, `web`, or the `*_us` stage timers.**
The two heap flags are how the playground's capacity warning is exercised
without hardware: `--heap-free` impersonates a device with that much free, and
`--engine-heap` a device with that much of it about to be handed back by the
outgoing pattern.

The mirror's `caps` advertise what **it** implements, which documents its drift
from the firmware: `reboot:false`, `ota:false`, `psram:false`. `--board panel`
makes it impersonate a 64×64 HUB75 board — `max_pixels` 4096, its own `64×64`
grid installed at startup (and reinstalled when a map is cleared, as
`devicemap::board_default` does on the firmware), `panel:true`,
`strip_driver:false`, `power_cap:false`, `layers:2`, and the matching
`/api/layout` (`kind:"matrix"`, 64×64, 4096 px) — and `--outputs N` sets
`caps.outputs`, which is what lets a two-output `/api/layout` be driven
without the Athom. Together they let the Settings page's capability gating be
driven without the hardware; see docs/tools.md.

The mirror's Layout is **not persisted** — it has no flash, so a restart comes
back to the board default. Everything else about `/api/layout` is identical by
construction: the grammar, the validation and the JSON all live in
`luxel_core::layout`, which both hosts call.

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
  `not enough free memory on the device for this N KB upload …` (the upload
  is over what this device has free at all), `device heap too fragmented for
  this N KB upload (F KB free, largest block L KB) — try again shortly, or
  reboot the device` (enough free in total, no contiguous run — Gitea #390;
  the same pattern usually saves fine a moment later, and always after a
  reboot), `upload truncated`.

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
- Firmware store limits: **32 KB of source** and **40 KB of bytecode** per
  pattern, and **732 KB of log** for all of them together. There is no
  pattern-count limit any more (Gitea #340): a pattern is one exact-sized
  file in the mapped part of the `storage` partition, so the library fills up
  by *space* — 119 of the real `library/` patterns fit, against the 32 the
  old page-granular store's directory item allowed. `/api/status`'s `store`
  reports `used`/`total`/`dead` in bytes. (`MAX_RECS`, 192, is a RAM guard on
  the index, not a storage limit.)
- Firmware save errors are size/space specific: `pattern name required`,
  `name must be 1..=64 bytes`, `this pattern's source is too large for the
  on-device library (…)`, `… compiled code is too large …`, `the device library
  is full (192 patterns) — delete one first`, `the device's pattern storage is
  full — delete some patterns and retry` (not enough room even after a
  compaction), `couldn't write the pattern to flash — the store may be
  busy; try again in a moment`, `the store is busy — try again in a moment`,
  `an update is in progress — try again in a moment`, `pattern storage
  unavailable (device needs reflash)`.
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
           "controls":{"speed":[0.5]},"proj":"x","invalid":"needs 2D map"}]}
```

`sec` is `null` when the item inherits `defaultSec`. `controls` values are
**decimal**. `proj` is present only when the item overrides the device's
projection default, and `invalid` only when the item's `assert()` invariants
fail against the current config (pre-flight check) — absent means fine, or
still being computed.

`POST` body is line-based, not JSON:

| line | meaning |
|---|---|
| `D <sec>` | default seconds per item |
| `X <ms>` | crossfade milliseconds |
| `I <patternId> <sec>` | an item; `sec` `-1` (or unparseable) = inherit the default |
| `C <name> <raw…>` | a control override for the item most recently declared; **raw 16.16** |
| `P <mode>` | projection override for the item most recently declared |

`C` and `P` bind to the `I` above them, so an item's lines are a contiguous
run. Both are optional and both are omitted when there is nothing to say —
a playlist written before `P` existed parses byte-for-byte as it always did,
and a device that does not know the line ignores it (Gitea #470).

`<mode>` is one of `index|x|y|z|xy|xz|yz` (docs/spec/projection.md §2). It is
applied to the slot matching the ITEM'S PATTERN's own dimensionality, so one
token survives a pattern change; a token that means nothing for the current
(pattern dims, Layout dims) pair falls back to that pair's first option, and
an unparseable one leaves the item on the device default. The override is
applied when the item activates, after its `C` values and after the device's
own map/projection defaults.

Unrecognized lines are ignored. Firmware persists the body verbatim to flash;
both sides apply edits live if already playing.

**Item budget.** The POST body shares the 4 KiB cap every POST has (firmware
reads request bodies into 4 KB buffers). An item with no values is
`I <8-hex-id> <sec>` — about 14 B — so a values-free playlist tops out around
**270 items**, far past the ~128 patterns the store holds. A `C` line costs `3 + len(name) + 7` B per scalar
value (raw 16.16 is up to 7 digits plus a sign), so a typical three-slider
item is ~60 B and a `P` line 3–4 B: roughly **60 items** with values, **45**
if every item carries three sliders and a projection. Past that the write is
refused, not truncated.

## `/api/layout` — the one geometry object

One endpoint for the one concept (proposal §2, Gitea #465): what shape the
installation IS. It **supersedes** `/api/config`'s pixel count, `/api/map
grid`, `/api/datapin` and `/api/protocol` as the source of truth; those stay
as aliases for one release (see "Aliases" below).

`GET /api/layout` returns the whole object, the old `/api/map` payload
embedded so a client needs one fetch:

```json
{"kind":"matrix","source":"regular","dims":2,"regular":true,
 "pixels":1024,"max":2048,"w":64,"h":16,
 "matrix":{"pw":32,"ph":16,"cols":2,"rows":1,"start":"tl","dir":"row",
           "snake":1,"rot180":0,"scan":16},
 "outputs":[{"n":0,"pin":18,"proto":"ws2812","order":"grb","count":2,
             "rev":false}],
 "proj":{"proj1d":"index","proj2d":"z","proj3d":"xy"},
 "map":{"installed":true,"dims":2,"count":1024,"kind":"grid","w":64,"h":16}}
```

| field | meaning |
|---|---|
| `kind` | `strip` · `matrix` · `map`. A HUB75 board is always `matrix`; a strip board is `strip` until a matrix or a map is configured. |
| `source` | `regular` (the shape comes from the strip/matrix fields) · `map` (from a map program's coordinates). "Custom" is a coordinate SOURCE, not a dimensionality. |
| `dims` / `regular` / `w` / `h` | The **Layout's own** shape: 1×`pixels` for a strip, `pw·cols`×`ph·rows` for a matrix, the installed map's detected grid (or `0`/`0`, `regular:false`) for a map. |
| `pixels` / `max` | The pixel count and this board's ceiling — the same numbers `/api/config` reports. |
| `matrix` | **Present only when `kind` is `matrix`.** `pw`×`ph` is one panel (or, with `cols`=`rows`=1, the whole grid); `cols`×`rows` tile them; `start` (`tl\|tr\|bl\|br`), `dir` (`row\|col`), `snake`, `rot180` describe how the chain threads the tiles — and, in the one-tile case, how the pixel run threads the grid (a strip-built matrix's wiring, proposal §5.3). `scan` is the HUB75 scan divisor, `0` = the board's own. On a board with a panel driver it also carries `est_hz` and `drive` — see "Panel arrangement" below. |
| `outputs` | One entry per configured output — `n` (0-based, `< caps.outputs`), `pin`, `proto`, `order`, `count` (pixels on a strip Layout, **panels** on a matrix one), `rev`. Each drives a consecutive run of the one pixel space, in `n` order (see "Driving" below). A host with no table configured reports ONE implicit output built from its live data pin, protocol and colour order. |
| `proj` | The §5.4d projection defaults (`docs/spec/projection.md`), tokens `index\|x\|y\|z\|xy\|xz\|yz`. |
| `map` | The `GET /api/map` body verbatim. |

A client that wants the index→coordinate mapping (a console preview showing
"real wiring") has everything here: `kind` + `w`/`h` + the `matrix` block's
`start`/`dir`/`snake`/`rot180`.

### Panel arrangement and estimated refresh (HUB75, Gitea #475)

A HUB75 chain is one ribbon: the driver shifts a single row `pw · panels`
wide and `ph` tall, and the tiles hang wherever the installer put them. The
`matrix` block describes that, and the firmware turns it into a
**panel→pixel remap built once at boot**, so the engine keeps rendering one
`pw·cols` × `ph·rows` row-major grid and never knows about the chain.

**Chain order**, exactly as the firmware walks it. Tiles are visited line by
line; a *line* is a row of tiles when `dir` is `row` and a column when it is
`col`:

- `start` places tile 0 and so sets which way line 0 travels — `tl` is
  left-to-right / top-to-bottom, `tr` mirrors x, `bl` mirrors y, `br` both.
- `snake` = 1 makes every **odd** line run back the other way (a serpentine
  chain, the usual way to wall-mount more than one row of panels).
- `rot180` = 1 marks the tiles on those **odd lines as mounted rotated 180°**
  — which is how a serpentine wall is physically built, since the return row's
  connectors face the other way. It is per line, not per display.

**Two fields a panel board adds to the `matrix` block:**

| field | meaning |
|---|---|
| `est_hz` | Estimated panel rescan rate for the whole configured chain (below). Absent on hosts with no panel driver — the mirror, and every strip board. |
| `drive` | Leading tiles of the chain this board's framebuffer can actually shift out. `drive < cols·rows` means the rest of the arrangement is **dark**: the DMA framebuffer is compile-time sized (Gitea #401) and a 64×64 board drives 64 columns of chain, i.e. one 64-wide tile or two 32-wide ones. |

**Estimated refresh.** Plain BCM shifts the whole chain once per bitplane per
address row, and plane *k* is repeated 2^*k* times, so one rescan costs

```text
est_hz = clock_hz / ( scan · (2^planes − 1) · pw · panels )
```

with `panels = cols · rows`, `planes` the BCM bit depth (7 today) and `scan`
the address depth — the `scan` field when the Layout states one, else `ph / 2`
(a HUB75 panel drives two half-height rows at once through R1G1B1/R2G2B2).
`clock_hz` is the LCD_CAM pixel clock, 30 MHz on the Seengreat board. A UI
computing this in the browser must use the same numbers and can check itself
against `est_hz`; **below ~100 Hz the panel visibly flickers** (show it amber
and name the fix: fewer panels per chain, fewer bitplanes, or a faster clock).

Measured against the bench panel (Gitea #255): 64×64 at 7 planes / 30 MHz
predicts 115 Hz against 115.3–115.5 measured, and 20/40 MHz predict 76/153
against 76.9–77.0 / 153.5–154.0.

**Constraints, which the UI should state:**

- **One chain per output** on this board — its two HUB75 headers are the same
  14 GPIOs wired twice, not two chains (schematic, Gitea #255).
- **Every tile in a chain has the same size and scan.** There is one `pw`,
  one `ph` and one `scan`.
- **Reboot to apply.** `cols rows start dir snake rot180 scan` are
  `reboot_required`; `pw`/`ph` only resize the grid and apply live.
- An arrangement whose chain is wider than `drive` tiles is accepted, stored
  and reported — the board just drives the prefix. That is the honest state,
  not a rejection, because raising the framebuffer is #401.

**`POST /api/layout`** takes a line-oriented body, ≤ 4 KiB, like the playlist.
Blank lines and `#` comments are ignored; line order is free; **at most one**
`strip`/`matrix`/`map` line per body:

```text
strip <pixels>
matrix <pw> <ph> <cols> <rows> <tl|tr|bl|br> <row|col> <snake 0|1> <rot180 0|1> [<scan>]
map [grid <w> <h> | <dims> <raw16.16…>]
out <n> <pin> <sk9822|ws2812> <rgb|rbg|grb|gbr|brg|bgr> <count> [rev]
out none
proj1d|proj2d|proj3d <index|x|y|z|xy|xz|yz>
```

Example — a 120 px strip split across the Athom's two outputs (its DATA1 is
GPIO18, DATA2 GPIO17), the second run wired backwards, with 1D patterns laid
along x:

```text
strip 120
out 0 18 ws2812 grb 60
out 1 17 ws2812 grb 60 rev
proj1d x
```

Only fields meaningful for the kind are accepted. `out` lines are
all-or-nothing: one of them replaces the whole table, a body with none leaves
the outputs untouched, and **`out none` empties the table** — back to the one
implicit output built from the device's live data pin, protocol and colour
order, which is the state a board with nothing stored is in. `map` takes
**exactly** the `POST /api/map`
wire, so a 64×64 panel is still `map grid 64 64` (a coordinate map larger than
the 4 KiB request buffer still cannot be POSTed — that is what the grid form
is for). Bare `map` clears.

The response is the GET body with `"ok":true,"reboot_required":B` in front, so
a client never has to re-fetch. A bad line answers
`{"ok":false,"error":"…","line":N}` and **changes nothing**; `line` is 1-based
(`0` = the body as a whole, e.g. the output-count sum).

**Validation.** Pixel counts against the board's `max`; `pw·ph·cols·rows`
against it too; output indices against `caps.outputs` (a board with one
output refuses `out 1` outright); pins against the board's reserved set (the
same check `/api/datapin` runs) and against each other — two outputs cannot
share a data pad; protocol names from
`GET /api/protocol`'s `options` (aliases accepted); colour orders from the
`/api/output` set; and **the outputs' counts must add up to the Layout's pixel
space** — pixels on a strip, panels on a matrix (proposal D11: an output
drives a consecutive run of the ONE pixel space). A HUB75 board refuses both
`strip` and `out`: it IS a matrix and has no configurable strip output.

**Driving (Gitea #474, proposal D11).** The outputs partition ONE pixel
space: output `n` carries the `count` pixels that follow every lower-indexed
output's run. There is one engine, one map, one pattern, one playlist, one
brightness and one HA light across all of them — an output is wiring, not a
second device. Within its run an output has its own protocol, colour order
and direction: `rev` means that run is wired backwards, so its first physical
LED is the run's LAST pixel.

- **Buffers.** Each output encodes its own run into its own buffer, sized to
  that run — two 30 px WS2812 runs cost the same ~360 B each that one 60 px
  run costs in total, not two full-frame buffers.
- **Timing.** On a strip board the outputs are written **sequentially** on the
  render task, and `out_us` in `/api/status` covers all of them. Splitting a
  fixed pixel count across two outputs does not halve wire time (the same
  pixels still go out, plus one extra protocol latch tail per output); what
  two outputs buy is twice the pixels, at twice the wire time.
- **Power.** The cap (`/api/output` `capMa`) is estimated over the whole
  frame, i.e. summed across every run.
- **Colour order.** The device output chain permutes the frame once, into
  output 0's order; an output with a different `order` fixes up only its own
  run from there. `POST /api/output` still sets output 0's (the chain's).
- A `count` that no longer adds up to the pixel count (an alias moved it —
  see "Aliases") is **clamped**, never fatal: a run past the end of the pixel
  space simply drives nothing until the table is re-stated.

Measured on the Athom (60 px WS2812, `/api/status` `out_us`): 2,524 us for one
60 px output, 2,827 us for `30` + `30 rev` — the +303 us is the second
protocol latch tail, not a second frame. docs/boards.md has the full table.

**Live vs reboot.** These apply on the next frame, no reboot:

- `pixels` (`strip N`, `matrix …`, `map grid W H`), the engine grid, the map,
  and the `proj*` defaults. `/api/status`'s `geom` follows within a frame.

These are **stored and reported only** until a reboot builds them, and a POST
that changes one answers `"reboot_required":true`:

- the chain wiring — `cols`, `rows`, `start`, `dir`, `snake`, `rot180`, `scan`
  (the boot-time panel→pixel remap is Gitea #475);
- every `out` line, because each output's driver INSTANCE — its SPI
  peripheral, its DATA pin, its protocol clock — is built once, at boot (the
  same reason `/api/datapin` reboots). Two parts of an `out` line do not
  actually wait, which is worth knowing when reading `out_us`: the run
  BOUNDARIES are re-read from the Layout every frame, so a re-partition takes
  effect immediately (an output whose run shrank drives fewer pixels at once,
  and one the table no longer covers goes dark), and output 0's protocol and
  colour order write through live because output 0 IS the strip the aliases
  describe. What the reboot adds is the driver for an output that did not
  have one, and a moved DATA pin.

**Persistence.** Firmware stores the Layout as a ~20-byte record (plus 9 B per
output) under the pattern store's reserved `LAYOUT_KEY`. It deliberately does
**not** carry the pixel count or the map payload — those keep their existing
homes (the nvs `LXDV` device record and the map blob), which is exactly what
keeps the aliases below honest instead of a second, drifting copy. A device
with no record boots its board default: a HUB75 board as its panel, a strip
board as its strip, or `kind:"map"` when a map is already installed. The
mirror keeps the same state in memory and forgets it on exit.

**Aliases (deprecated, kept for one release).** `POST /api/config` (pixel
count), `POST /api/map` (the map), `POST /api/datapin` and `POST
/api/protocol` all read and write the same state `/api/layout` reports, so
either endpoint is correct — but new clients should use `/api/layout`, which
is the only one that can express an arrangement, an output table or a
projection default. Two differences worth knowing:

- `POST /api/map grid W H` does **not** resize the pixel space (its
  historical contract); `POST /api/layout map grid W H` does, because a
  Layout is one object.
- With a multi-output table stored, an alias that changes the pixel count
  leaves the table's partition alone (it would have to guess); re-POST
  `/api/layout` with fresh `out` lines to re-partition. With a single output
  the count follows.

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
| `/api/map` | GET | — | `{"installed":bool,"dims":2\|3\|0,"count":N,"kind":"grid"\|"coords"[,"w":W,"h":H]}` | both |
| `/api/map` | POST | `<dims> <raw…>` or `grid <w> <h>` | `{"ok":true,"installed":bool,"count":N}` | both |
| `/api/layout` | GET | — | the whole Layout — see "`/api/layout` — the one geometry object" above | both |
| `/api/layout` | POST | `strip`/`matrix`/`map`/`out`/`proj*` lines | the GET body + `"ok"`/`"reboot_required"`, or `{"ok":false,"error":…,"line":N}` | both |
| `/api/clock` | GET | — | `{"synced":bool,"local":<unix secs, local>,"tzMinutes":N}` | both |
| `/api/clock` | POST | tz offset from UTC in minutes | `{"ok":true,"tzMinutes":N}` | both |
| `/api/clock/sync` | POST | (body ignored) | `{"ok":true,"synced":bool,"local":<unix secs, local>}` | both |

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
  per pixel, **or `grid <w> <h>`** for a procedural row-major grid — no
  per-pixel body, and zero heap on the device (a 64x64 panel's map is 48 KB
  as coordinates, 5 bytes as a grid; Gitea #258). An empty or unparseable
  body **clears** the map and answers `"installed":false` — except on a
  HUB75 panel board, which falls back to its own `PANEL_COLS`×`PANEL_ROWS`
  grid (installed at boot when nothing is stored) and stays `installed:true`.
  Firmware request bodies are read into 4 KB buffers: a large coordinate
  map that does not arrive intact is treated as "clear", so prefer the grid
  form for matrices.
- **Projection defaults** (`proj1d`/`proj2d`/`proj3d`, tokens
  `index|x|y|z|xy|xz|yz`) say how a pattern whose dimensionality differs from
  the Layout's is shown on it — `docs/spec/projection.md` has the table. They
  live on **`/api/layout`** (Gitea #465; the engine mechanism was #473) on
  both hosts, and are applied to the engine at boot and on every POST. The
  `proj*=` tokens the mirror briefly accepted on `POST /api/map` are gone.
  A PER-ITEM override lives beside the item's values instead — the playlist's
  `P` line, which both hosts carry (Gitea #470).
- `POST /api/clock` accepts −840..=840 minutes.
- `POST /api/clock/sync` asks the device to re-sync NOW (Gitea #538). On
  firmware it wakes the SNTP task, which otherwise sleeps out a 6 h period
  (or an exponential backoff after a failure). The sync is **asynchronous**:
  the reply is the clock as it stands at that instant, so `synced` is still
  the PREVIOUS state on a first successful call — poll `GET /api/clock` for
  the result. A device in AP mode has no SNTP task and stays `false`.
- Firmware settings whose flash write fails still apply live and add
  `"note":"not persisted: …"` to the `{"ok":true,…}` body (`/api/brightness`,
  `/api/config`, `/api/protocol`, `/api/layout`).
- **Deprecated for one release (Gitea #465):** `POST /api/config`,
  `POST /api/map`, `POST /api/datapin` and `POST /api/protocol` are aliases of
  `/api/layout`, which is the source of truth for geometry. They read and
  write the same state and keep working; a new client should use
  `/api/layout` — it is the only one that can express a panel arrangement, an
  output table or a projection default. See the `/api/layout` section for the
  two places the alias contracts differ.

## Network, provisioning, and integrations

| route | method | body | response | where |
|---|---|---|---|---|
| `/api/name` | GET | — | `{"name":"…","source":"stored"\|"default"}` | both |
| `/api/name` | POST | the name, or empty to restore the default | `{"ok":true,"name":"…","source":"…","reboot_required":true}` | both |
| `/api/wifi` | GET | — | `{"ssid":"…"\|null,"source":"flash"\|"builtin"\|"none"}` | both |
| `/api/wifi` | POST | `ssid\npassword` | `{"ok":true,"ssid":"…","note":"rebooting to apply"}` — **firmware reboots** | both |
| `/api/apmode` | GET | — | `{"ap":bool}` | both |
| `/api/apmode` | POST | any (ignored) | `{"ok":true,"note":"rebooting into the setup AP (one boot only)"}` — **firmware reboots** | both |
| `/api/reboot` | POST | (body ignored) | `{"ok":true,"note":"rebooting"}` — **firmware reboots** | firmware only (`caps.reboot`) |
| `/api/mqtt` | GET | — | `{"enabled","host","port","user","hasPass","connected"}` | both |
| `/api/mqtt` | POST | `host\nport\nuser\npass` | `{"ok":true,"enabled":bool}` | both |
| `/api/sync` | GET | — | `{"mode","timeMs","leader":{"bootId","ageMs","offsetMs"}\|null}` | both |
| `/api/sync` | POST | `off` \| `leader` \| `follower` | `{"ok":true,"mode":"…"}` | both |

- **`/api/name`** (Gitea #538) is what this device calls itself — the console
  title bar, Settings → Device → Name, and Home Assistant. It lives in this
  section rather than "Device settings" because it is *not* live: the DHCP
  hostname is built from it at boot and the network stack never re-reads it,
  so the POST persists, updates `/api/status` `name` immediately and answers
  `"reboot_required":true`. The setup AP's SSID stays the board's
  `luxel-<mac6>` for now; #536 moves it onto this name (with a password).
  - Validation: 1..=32 bytes of printable UTF-8 — control bytes, `"` and
    `\` rejected. 32 bytes is the 802.11 SSID limit, so one cap covers every
    consumer; the two JSON metacharacters are out so the stored bytes ARE
    the JSON string at every emit site, `/api/status`'s polled path
    included. An empty body **clears** the name, restoring the default.
  - `source` is `"stored"` when the user set one, `"default"` for the board's
    `luxel-<mac6>`. The mirror has no MAC and defaults to `luxel-serve`
    (`luxel serve --name NAME` sets it); it reports `"default"` whenever the
    name equals that.
  - It is persisted in the pattern store's reserved-key blob space, not the
    nvs device record — the nvs partition's four sectors are full and the
    record is a fixed-size struct (see firmware/src/devname.rs).
- `POST /api/reboot` is the other half of `reboot_required` (Gitea #475): the
  other reboots are side effects of their own change (`/api/wifi`,
  `/api/datapin`) and neither exists on a HUB75 board, so a stored chain
  arrangement or output table had no way to be applied. Offer it only when
  `caps.reboot`; the mirror has none.
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
  `GET /api/clock` is always `"synced":true` (host clock), and
  `POST /api/clock/sync` answers `{"ok":true,"synced":true,…}` without doing
  anything — there is no NTP client to poke. `POST /api/name` likewise stores
  the name and returns `"reboot_required":true` for parity.

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
  `POST /api/datapin` reboots too. `POST /api/name` is the one route that
  needs a reboot and does NOT take one: the name is live in `/api/status`
  immediately and only the hostname waits, so it answers
  `"reboot_required":true` and leaves the timing to the caller.
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
