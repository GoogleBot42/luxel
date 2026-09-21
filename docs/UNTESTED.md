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
- [ ] **Event injection** (v0.1.38/39): open *Typing Heatmap 2D*,
  *Crosshair Pulse 2D*, *Ripples 2D* or *Slime mold palette* and click/drag
  the preview — hits should land where
  you click (and on the wall too, in device mode; *SaberDeploy Tutorial*
  takes any event as a button press). Then from HA (or
  `mosquitto_pub` at your broker): publish `1 0.5 0.5` to
  `luxel/luxel-4ae0d4/event` while one of those patterns runs — the wall
  should react. Machine-verified against local mosquitto via the mirror;
  the real-HA hop is the untested bit.
  (Firmware HTTP path hardware-soaked 2026-08-22 on the Athom —
  tools/event-soak.mjs, 30,286/30,286 delivered, malformed rejected,
  heap/fps stable. Still open here: the preview click-through and the
  real-HA MQTT hop on the wall unit — both tracked as Gitea #216.)
- [x] **Share links carry the pattern only** (changed by Gitea #463): a map is
  the Layout's, not the pattern's, so a link made today is `#p=` even with a
  custom map installed, and an old `#pj=` link still restores its map as the
  playground's Layout choice. Both machine-verified in `web/tools/e2e.mjs`.
- [ ] **3D map preview**: a map program whose z varies (e.g.
  `plot(cos(a), sin(a), index/pixelCount - 0.5)`) → auto-rotating point
  cloud with a "3D" badge, on the map program's own screen AND in the editor
  preview once the map is in use. (The screen is reached from the playground's
  "Preview as → Custom map program" since A10/#471.)

## Settings tab (device mode)

The whole page was reshaped by A8 (Gitea #469, v2): Device (brightness first)
· LED layout · WiFi, then one **Advanced** list whose rows each state their
value while collapsed. Everything below is on the new page.

- [ ] **The ranked page itself** — does the order match what you actually open
  Settings for? Brightness is the first control; the Advanced rows should be
  scannable without expanding any of them.
- [ ] **LED layout on the Athom** — the summary line, the pixel count (it
  resizes live, no reboot), LED type / colour order / data pin. The Athom
  advertises two outputs, so the **Outputs table** should be there with an
  `+ Add output`: adding one splits the strip in half and says
  `pixels 0–29 / 30–59`. The table is built at boot (#474), so the page says
  "applies after a reboot" and offers a `Reboot to apply` beside it — with a
  second strip on GPIO 19 the back half of the fixture should light after that.
- [ ] **LED layout on the HUB75 panel** — there should be NO layout picker at
  all (the board has no choice) and no LED type / colour order / data pin;
  the panel size, scan and `Panels [c] across × [r] down` instead. Setting
  2×2 should draw the chain picture and drop the estimated refresh to ~28 Hz
  in amber, and — because a 64×64 board only shifts 64 columns of chain — say
  that three of the four panels would stay dark. **Do not apply it** unless you
  actually have four panels: the `Reboot to apply` button beside the note is
  what builds it, and it reboots.
- [ ] **The estimated refresh number** — one 64×64 panel should read 115 Hz,
  which is what the panel measures (`rescan_hz`, shown beside it as
  "measured now"). The figure is the DEVICE's `est_hz` when it reports one, so
  a disagreement there means the firmware and the panel disagree, not the
  browser.
- [ ] **Firmware & recovery → Update…** — pick a `luxel.bin` for that board
  and let it flash itself over the network. **Never run against hardware**
  (the mirror advertises no OTA, so no harness can reach this path) —
  Gitea #526 has the full procedure and what to read before and after.
- [ ] **WiFi form** — now collapsed behind `Change network…`; shows the saved
  network, and changing creds reboots onto the new one (careful: typos strand
  it → AP mode should catch it now).
- [ ] **Device map upload** (v0.1.16): **Install on device**, the single
  primary action of the map program's own screen (A10/#471 — reached from
  Settings → LED layout → "Custom map program →"); the wall renders with real
  geometry and it survives reboot. Note that nothing hands the PROGRAM back
  from a device yet (Gitea #517) — only its coordinates — so check the program
  text is the one this browser wrote.
- [ ] **Network input status row** — while LedFx/xLights (or my test
  script) streams DDP, the collapsed Advanced row says "receiving DDP" and the
  pattern resumes a few seconds after the stream stops.
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

- [x] **The partition repartition on metal — the 4 MB half** (Gitea #634,
  from #501): done on the **Athom** 2026-09-20. One reboot, under 8.6 s end
  to end, patterns / playlist / layout / name / brightness / asset bundle all
  intact, `partitions.migrated true`, slot 1,310,720 B, store 524,288 B.
  Timeline and the full before/after tables are on #634; the fleet status
  table is in docs/boards.md ("On metal"). Two things came out of it: the
  resume-after-a-cut path got **no** coverage (the migration finished before
  the cut window could open — Gitea #644), and an OTA across an LXBC format
  bump leaves every stored blob unreadable on a device that cannot recompile
  itself (Gitea #643).
- [ ] **The partition repartition on metal — the Seengreat** (Gitea #634):
  still to do, and the Athom going well does not make it easier. It is the
  only 16 MB device, its table moves the 960 KiB **asset bundle** as well as
  the store, and that copy — the one stage the 4 MB layout does not take —
  has never run anywhere: QEMU's `esp32s3` machine reads the 16 MB table and
  loads the app but then prints nothing at all, so it is covered by
  assertion (`tools/qemu/migrate-test.py --plan-16mb`) rather than by
  execution. It has no serial console and no power plug, it runs your
  playlist, and the residual risk is a cut inside the single 4 KiB table
  write — milliseconds, serial-recovery-only, and you are the serial
  recovery.
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
- [ ] **Strip on a moved data pin** (Gitea #238, #154): Settings → Device →
  "Data pin", pick a free output pin (Athom: GPIO33), apply & reboot, move
  the strip's DATA wire there — it should light; pick the board default to
  put it back. Headless-verified: the setting persists across the reboot and
  `/api/config` reports the bound pin; nobody has seen pixels on the new pin.
- [ ] **A real button through `digitalRead`** (Gitea #238, #177): the pattern
  in #238 on the Athom's case button (GPIO0) — dim strip, bright while held.
  Headless-verified up to the press: the pull-up idles the pad HIGH, and a
  written level reads back on the same pad.
- [ ] **Map-aware 2D blur/glow on a real matrix** (Gitea #448; kernels #140,
  panel #75): on the 64×64 HUB75 panel with its grid map installed, run a
  pattern with a bright point, then raise Settings → Output → Blur and
  Glow. It should soften into a round halo in both axes with no smear along
  the wiring and no bright/dark seam at the row folds. **Machine-verified on
  the panel 2026-09-08** (firmware `f62a45e`/v0.1.40, 4096 px, a real
  `kind: "grid"` 64x64 map — the old #258 blocker is gone): a one-pixel
  probe read back through `GET /api/pixels` gives the separable 1-2-1 kernel
  in *both* axes for `setBlur(0.5,1)`, a 5x5 disc for two passes, and a 3x3
  max-bloom halo with the source undimmed for `setGlow(0.6)`; a point at the
  last pixel of a row (r31c63) spreads to six edge-clamped cells and puts
  **nothing** at r32c0, its index neighbour — so no fold along the wiring.
  `vmerr` null, fps/heap healthy, `/api/output` round-trips (numbers and
  costs: docs/bulk-render.md “Map-aware 2D blur/glow on the 64x64 panel”,
  Gitea #446). What is left is the part a readback cannot do: **Jeremy
  looking at the panel**.
- [ ] **The hosted console reaching a device over https** (Gitea #162,
  needs a HEADFUL browser — no agent can do this one): open
  `https://googlebot42.github.io/luxel/?device=http://<device-host>` in
  normal Chrome on the LAN. Expect a Local Network Access permission
  prompt; allow it and the console should connect exactly as it does when
  served from the device. Deny it (or dismiss it) and the page should say
  the browser blocked it and offer the manual routes — that half is
  machine-verified (`web/tools/lna-e2e.mjs`), the granted half is not.
- [x] **WLED→Luxel installer page against real WLED** — DONE 2026-08-30:
  full stock-WLED-restore → Improv provision → credless-master takeover →
  boot-guard-healthy cycle on the Athom (Gitea #53 closed, PR #171, beta
  banner dropped). Only the https-origin / Chrome local-network-permission
  path remains, tracked as Gitea #162 (hardware-confirmed blocked in
  headless chromium; needs a headful browser).

## Verified hard by machines, low review value (FYI only)

- Boot-loop guard (bad OTA self-heals by flipping slots) — exercised in
  anger during the v0.1.19 wedge; recovery notes in UPDATES.md.
- DDP/E1.31 pixel input — byte-verified on the wall over real WiFi.
- Oracle findings: rendering is now bit-exact vs your Pixelblaze
  (quantization + palette fixes); transforms/pow/log2 verified.
- Firmware size diet (91→87% of the OTA slot), deploy script
  (`tools/deploy.sh <ip>`), hardware bench report
  ([docs/bench-report.md](bench-report.md), regenerating as I write).
