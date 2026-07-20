# Things Jeremy hasn't personally tried yet

Everything since your last hands-on session (the night of 2026-07-06, when
you verified the clean device load + the "untitled pattern" fix). All of it
is machine-verified (unit tests, three e2e suites, and where possible the
real device/broker) — but "works in a test" and "feels right" are different
things. Ordered roughly by how much they benefit from your eyes. Check off
as you go; tell me anything that feels wrong and it becomes a task.

Everything runs on the wall unit at http://192.168.0.205/ unless noted.

## Quick wins (open the web UI and click around)

- [ ] **Playlist** (v0.1.15–17): save a few patterns to the device, add
  them to the Playlist tab (same pattern twice with different slider values
  works), set a default duration + a per-item override, hit play. Reboot the
  device mid-playlist — it should resume where it was.
- [ ] **Crossfade** (v0.1.17): Playlist tab → "crossfade" field (e.g. 2
  seconds) — items should blend into each other on the wall, not snap.
- [ ] **Gallery search + 195 tiles** — the Patterns Library search box; the
  five new **render3D patterns render as rotating point clouds** (search
  "3D").
- [ ] **Editor DX** (your work-day picks): Ctrl/Cmd+S saves, playlist rows
  drag to reorder.
- [ ] **Sound-reactive playground**: open any pattern, add
  `export var energyAverage` and use it in `render`, hit the **sound**
  button next to *debug*, allow the mic — the preview should pump with your
  voice. In device mode the same button ALSO drives the wall
  (mic → strip at ~20 Hz).
- [ ] **Share links with maps**: make a 2D/3D map (layout → "2D map"), share,
  open the link in a private window — geometry should arrive with the
  pattern (`#pj=` in the URL).
- [ ] **3D map preview**: a map program whose z varies (e.g.
  `plot(cos(a), sin(a), index/pixelCount - 0.5)`) → auto-rotating point
  cloud in the preview with a "3D" badge.

## Settings tab (device mode)

- [ ] **WiFi form** — shows the saved network; changing creds reboots onto
  the new one (careful: typos strand it → AP mode should catch it now).
- [ ] **Device map upload** (v0.1.16): "install on device" in the map
  sub-tab; the wall renders with real geometry and it survives reboot.
- [ ] **Network input status row** — while LedFx/xLights (or my test
  script) streams DDP, the row says "receiving DDP" and the pattern resumes
  a few seconds after the stream stops.
- [ ] **Multi-device sync role select** — with one device it just shows
  "broadcasting"/"waiting"; the real test needs a second Luxel someday.
- [ ] **MQTT form** — your broker is already configured and connected
  (I set it up); the row should say "connected".

## Home Assistant (check your HA UI)

- [ ] Device **luxel-4ae0d4** should exist with: a **Light** (power +
  brightness — off blanks the strip, on resumes mid-animation), a
  **Pattern select** (device library by name), and — after the next OTA
  (v0.1.23, pending) — **FPS/heap diagnostic sensors**, a **Playlist
  switch**, and **Next/Previous pattern buttons**.
- [ ] Power/brightness from HA and from the web UI stay in agreement
  (state echoes within ~5 s either way).

## Needs you physically (I can't do these)

- [ ] **AP-mode provisioning** (v0.1.22): Settings → "reboot into setup
  AP", then join `luxel-4ae0d4` from your phone — a captive portal should
  pop with the settings page; save WiFi and it reboots back onto your
  network. One-shot: if anything goes wrong, power-cycle and it boots
  normally. **Nobody has tested the radio path.**
- [ ] **Serial flash flow** (next time you flash): `./build-esp32.sh flash`
  now also writes the web assets — after flashing, the served UI should be
  current with no extra step. Also `./build-esp32.sh image` → single
  full-flash restore file.
- [ ] **Onboard mic bring-up** — a bench session: see
  [docs/mic-bringup.md](mic-bringup.md). The FFT pipeline is ready; we
  need the mic's type + pins probed.
- [ ] **PB sensor board** (only if you own one): plugs into the expansion
  header RX0; sound-reactive patterns should react with zero config.

## Verified hard by machines, low review value (FYI only)

- Boot-loop guard (bad OTA self-heals by flipping slots) — exercised in
  anger during the v0.1.19 wedge; recovery notes in UPDATES.md.
- DDP/E1.31 pixel input — byte-verified on the wall over real WiFi.
- Oracle findings: rendering is now bit-exact vs your Pixelblaze
  (quantization + palette fixes); transforms/pow/log2 verified.
- Firmware size diet (91→87% of the OTA slot), deploy script
  (`tools/deploy.sh <ip>`), hardware bench report
  ([docs/bench-report.md](bench-report.md), regenerating as I write).
