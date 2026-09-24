---
name: seengreat-panel
description: Use when working on the Seengreat RGB Matrix HUB75 S3 + 64x64 panel on Jeremy's bench — flashing, resetting or capturing a boot/panic log over its native USB, OTA-pushing an S3 image, or reproducing a crash on it. The board's USB behaves unlike the Athom's FTDI and the wrong move reboots it.
---

The Seengreat board (`board-seengreat-hub75`, ESP32-S3-WROOM-1-N16R8 + a
64x64 FM6124 panel) is at **192.168.0.238** (`luxel-f6b0a8`). Facts,
numbers and findings live in docs/boards.md "First light"; this skill is
the hands-on procedure. Deploying over the network is the deploy-device
skill (`BOARD=board-seengreat-hub75 tools/ota-push.sh 192.168.0.238` —
`BOARD` is required since #389/#416; a plain `curl --data-binary @app.bin
http://192.168.0.238/api/ota` is the fallback when the script fails silently).

Reading the panel (2026-09-07):
- **`/api/status`'s `web` array is the first thing to read when the board
  "refuses connections".** It is the 3-socket web pool's per-slot stage
  (0 accepting · 1 serving · 2-5 shutting down · 9 abort). `web:[1,1,1]` means
  all three slots are held by live clients — one open console tab holds one for
  as long as it is open — and then a browser cold load gets
  `ERR_CONNECTION_REFUSED` on a native request. The stylesheet is the one
  nothing retries, so the console comes up **completely unstyled** with no error
  (Gitea #592); `luxel.wasm` and every `/api/*` go through the app's fetchgate
  and do retry, so the page otherwise works and it reads as a CSS bug. Seen for
  >10 minutes straight on 2026-09-20 with a single `curl` as our only traffic.
  A bare `curl` that returns EMPTY, or two quick curls where the second returns
  nothing, is the same condition — space reads a few seconds apart.
- **A rejected pattern load leaves the panel DARK** with `out_fps` 0 and
  `rescan_hz` 0 — it reads like a hang but is the documented rejection path
  (array budget / `RUNTIME_FLOOR`). Check `vmerr` before diagnosing.
- `tools/panel-load-bench.mjs` needs a FAST live pattern (push
  `library/frame-rate-scan.js` via `POST /api/code`, re-activate the stored
  pattern by id after); its tab phase repeats to about ±18 %, so take a
  repeated baseline in the same session before claiming a delta. `--clients 3`
  saturates the 3-socket web pool and returns nothing — use `--clients 1`.
- Brightness is Jeremy's setting — read it, never set it.
- **A 30-60 s hole in the network is not automatically a crash.** On 2026-09-20
  the panel went fully unreachable mid-session — chromium
  `net::ERR_ADDRESS_UNREACHABLE` after 3.1 s and `curl` HTTP 000 at its connect
  timeout, i.e. a HANG, not the instant `ECONNREFUSED` an exhausted `web` pool
  gives — and came back on its own about a minute later. `core1.last.reset` then
  read `ChipPowerOn`: somebody power-cycled it at the bench. Read
  `core1.last.reset` and `pass.n` (which restarts from 0) before escalating —
  `ChipPowerOn` = a bench power cycle, `CoreSw` = an OTA's own reboot,
  `SysRtcWdt` = the #294 flash wedge. Slot, playlist, brightness and the live
  pattern all survived it.
- **Screenshotting the console needs ~6 s of settle, not 2.** The app boots in
  ~6 s on this board (`coldload.mjs` measures it) and `[data-role="patterns-panel"]`
  exists while still `hidden`, so an early shot captures the `opening the pattern
  running on the device…` splash. Wait for the role with `visible: true` AND for
  `[data-role="pattern-loading"]` / `[data-role="gallery-loading"]` to be gone.
- **`GET /api/pixels` is the ENGINE's frame, not the wire's.** On a pipelined
  board the preview reads the render → output hand-off buffer, which the
  firmware's output pipeline has not touched yet. A readback therefore shows a
  pattern's own `setBlur`/`setGlow`/`setOutputPalette`/`setGamma` **exactly**
  (that is how #140's 2D kernels were verified on 2026-09-08) and shows the
  DEVICE-level `/api/output` stages **not at all**. Judge an engine change by
  readback; judge an outpipe change by `pipe_us` and by eye.
- **Touching any `/api/output` stage costs ~12.3 KB of heap until reboot**
  at 4096 px (#446): the outpipe's frame scratch keeps its capacity once
  grown, and turning the setting back off does not give it back
  (`heap_free` 41,492 → 26,928 → 29,324 on 2026-09-08). Read `heap_free`
  before and after, and say so when you hand the panel back.

## Its bootloader says 4 MB, so it cannot take the 16 MB table over the air

This panel was serially flashed when `board-seengreat-hub75` still used
`partitions.csv`; `--flash-size 16mb` only arrived for it with #501. The
second-stage bootloader programs `g_rom_flashchip.chip_size` out of **its own**
image header, **an OTA replaces the app and never the bootloader**, and the ROM
bounds-checks every flash op against that number. So on 16 MB of silicon this
board refuses every access at or above `0x400000` — which is what silently
stopped the #501 migration's `storage` erase at `0x610000` (#634, root-caused
under emulation 2026-09-21).

What this means in practice:

- **This board migrates to the 4 MB table, not its own** (Gitea #634, second
  half). A 16 MB image embeds both, and `parttab::target_table()` takes the
  largest layout that fits under `min(chip size, bootloader ceiling)`. Expect
  `layout:"partitions.csv"`, `migrated:true`, `ota_slot_bytes:1310720`,
  `storage_bytes:524288`, `ceiling_bytes:4194304`, `upgrade_available:true` —
  a complete, correct migration, not a holding pattern, and the same code path
  the Athom ran. `assets` stays at `0x310000`, so the bundle is untouched.
  **That is the expected, healthy answer — not a regression to chase.**
- **`upgrade_available:true` is what says the big table is still on the
  table.** `migrated:true` here does NOT mean "done forever": re-flash the
  bootloader and the device migrates a second time on the next boot, to
  `partitions-16mb.csv`.
- **Its OTA slot is 1.25 MiB while it is on the fallback**, not the 3 MiB
  `board_ota_max` says. `tools/image-check.sh` gates releases against the
  nominal slot; the device refuses an over-size push itself and names the
  bootloader (docs/boards.md, "two tiers").
- **Do NOT "fix" it by raising the ROM ceiling at runtime.** It works, the
  migration completes, and the board then boots into a bootloader that refuses
  the table it just installed (`load partition table error!`) forever. Emulated
  and asserted (`tools/qemu/migrate-test.py --board s3 --old-bootloader 4mb`).
- Getting it onto the 16 MB table is one serial flash of bootloader + table +
  app together — `BOARD=board-seengreat-hub75 firmware/build-esp32.sh flash`,
  which needs the BOOT-button hold and is therefore **Jeremy's**, and is now an
  upgrade rather than a rescue. It **erases the store and the assets**: read
  `/api/patterns` (+ each source), `/api/playlist`, `/api/layout` and
  `/api/brightness` out over HTTP first and restore them afterwards.

## The USB port is not a serial console

The data USB-C is the S3's **native USB-Serial/JTAG** (303a:1001 → `/dev/ttyACM0`
once Jeremy has passed that id into the container; `doas chmod 666` if it
comes back `660` after a physical reset). The peripheral treats a host-side
**termios setup (a baud rate) as a chip reset** — a bare `open()` does
nothing, `stty`/`socat …,b115200`/`espflash monitor` all reboot the board.
Consequences:

- **No passive monitoring.** A reader loop that reopens the port reboots the
  board on every reopen (it did, for minutes, on 2026-09-05). Never leave one
  running; use `/api/status` polling for liveness during soaks.
- **The reset is not reliable on a plain open** (2026-09-06, several hours of
  it): a single `socat …,b115200` open often attaches *passively* — no reset,
  and no output at all until the firmware prints something. What resets the
  chip every time is a **second, short socat open while a long-lived reader is
  already attached**; the long reader then captures the whole boot log:
  ```sh
  setsid nohup timeout 3600 socat -u /dev/ttyACM0,raw,echo=0,b115200 STDOUT > boot.log &
  sleep 4
  timeout 2 socat -u /dev/ttyACM0,raw,echo=0,b115200 STDOUT > /dev/null   # the reset
  ```
  `espflash monitor` is NOT an option while the app runs — it insists on
  connecting to a bootloader and fails with "Error while connecting to device".
- **A watchdog reset or panic RE-ENUMERATES the USB node** (a USB-triggered
  reset does not): the reader dies, the node comes back `root:dialout 660`,
  and you need `doas chmod 666` again. So a serial capture that stops
  mid-session is itself evidence the board reset.
- **The board sometimes will not reset over USB at all** — two full
  attempts, zero bytes captured (2026-09-07). Don't spend a session on it:
  fall back to `/api/status` polling and static ELF checks.
- **Never touch serial in the 60 s after an OTA reboot.** The new slot is in
  the bootloader's pending-verify window until the firmware's boot_ok; a reset
  inside it rolls the slot straight back (lost a good OTA that way on
  2026-09-06 by restarting a reader three seconds after the reboot).
- **Kill the reader BEFORE every OTA push, not after.** The reboot
  re-enumerates the node and the host re-applies termios, which is itself a
  reset — an attached reader makes every OTA two boots, and a third flips
  the slot. Re-attach only after boot_ok (~75 s) (2026-09-07).
- **Boot-guard arithmetic still applies**: a reset counts as a boot; three
  boots that don't reach the 60 s "healthy" mark flip the OTA slot.

## Capturing a boot log or a panic (the only way to see one)

One long-lived reader, opened deliberately, costs exactly one reset:

```sh
timeout 200 socat -u /dev/ttyACM0,raw,echo=0,b115200 STDOUT > boot.log &
sleep 85            # boot + WiFi + the 60 s boot_ok — then provoke the thing
# ... push patterns / drive the API ...
pkill -f '^socat -u /dev/ttyACM0'     # NOT "pkill -f ttyACM0": that matches your own shell
sed 's/\x1b\[[0-9;]*m//g' boot.log | tr '\r' '\n'
```

Symbolicate a backtrace with the S3 ELF:
`xtensa-esp32s3-elf-addr2line -f -C -e firmware/target/xtensa-esp32s3-none-elf/release/luxel-fw 0x4209ffac …`
(inside `nix develop`; tools/decode-backtrace.sh defaults to the classic
ESP32 ELF — pass the S3 one).

## The JIT on this board (Gitea #665, 2026-09-24)

- **Native code lives in PSRAM and is ON by default.** `/api/status`'s
  `jit` says `native` + `place:"psram"` for the live pattern;
  `POST /api/jit {"on":false}` is the kill switch for a session,
  `{"place":"internal"}` forces the SRAM1 heap alias (the microbench
  lever). Neither persists; a reboot comes back ON.
- **A doubled `frame_us` on this board is a JIT fallback until `jit.state`
  says otherwise.** Read that field BEFORE attributing a frame to any
  pipeline stage. Aurora 2D is ~51 ms native and ~105 ms interpreted at
  4096 px, so anything that quietly costs the board ~12 KB of internal heap
  reads as a doubled frame — which is how Gitea #705 came to be filed as
  "compositing one layer costs 54 ms" when compositing costs 654 µs and the
  real bug was a resident buffer (#704). Same for a HALVED number: the JIT
  coming back looks like a speedup in whatever you last changed.
- **`jit` is ONE global block and reports the LAST compile — in a scene it
  does not tell you the base layer's state.** A scene compiles bottom → top,
  so what `/api/status` shows is the TOP layer, and a base layer that fell
  back is overwritten by the next success. Read the per-layer state off the
  arithmetic instead: measure each pattern bare, and the scene's `frame_us`
  should be their sum plus ~2.6 ms of compositing at 4096 px (2026-09-24,
  #709: Aurora 2D 51.7 + `_Fairies` 17.7 + 2.6 = 72.0 ms measured). Anything
  far above that is a layer interpreting. Per-layer reporting is #718.
- **`reason:"no-memory"` is the normal answer for the big 2D programs at
  4096 px** (snake-2d, snake-2d-v2, sunrise-2d, stargen-polar-2d…): the
  emitter's bookkeeping needs `words*24 + fns*240 + 1 KB` of INTERNAL heap
  beside the engine and this board has 26–37 KB left. Not a failure — the
  interpreter runs them. Follow-up #671.
- **Bench with ONE socket at a time.** `tools/patbench.mjs` (and a `curl`
  beside it) exhaust the 3-socket web pool — rows come back `TypeError:
  fetch failed` / `ECONNREFUSED` and `restore pattern failed`, which reads
  like a crash and is not. `tools/jit-diff.mjs` is single-socket with
  `connection: close` + retry; copy that shape (Gitea #675).
- **A POST fired right behind another request can come back with an EMPTY
  body AND not take effect** — the 3-socket pool again, but silent. Seen
  twice on 2026-09-24 (#709): `POST /api/scenes/<id>/activate` returned
  nothing, `GET /api/scenes` still showed the previous scene `active`, and
  four perfectly steady `frame_us` samples were of the WRONG scene. Sleep
  2–3 s between device calls, and after any activate re-read the thing that
  proves it landed (`active`, `engines`, `vmerr`) before sampling.
- **A reboot is invisible to a push-based tool now that the JIT boots ON**:
  after a reset the board comes back running the default pattern with the
  switch on, and the tool's next POST/push just proceeds. Run a watcher
  that logs `pass.n` every 30 s (it restarts from 0) beside any long run,
  and read it before believing "0 crashed".
- **The serial reader recipe above WORKS as written** (2026-09-24): the
  long `socat` open attached passively (0 bytes for minutes), the second
  2-second open reset the chip, and the reader then captured every boot,
  `jit:` narration line and PANIC backtrace. Symbolicate against the ELF
  you PUSHED (keep a copy — `tools/stack-check.sh` overwrites the target).
- **Do NOT write a scene while a 2-engine scene is ACTIVE — it reboots the
  board** (Gitea #724, reproduced 2/2 on 2026-09-24). At the 2-pattern-layer
  cap the panel sits at `heap_free` ~10.7 KB / `heap_largest` ~6.6 KB, and
  `scenes.rs commit()` builds the whole scene blob as a `String` with
  infallible allocation, so `POST /api/scenes` panics the allocator. The POST
  returns an EMPTY body (indistinguishable from the socket-pool case above),
  the write is lost, and the board comes back with the scene deactivated. Same
  class as #702; made reachable by #704's resident buffers. Until it is fixed:
  deactivate first (`POST /api/patterns/<id>/activate`), then write.
- **`/api/pixels` is the camera-less proof for a PANEL feature — and it stops
  answering exactly when you need it.** 12,288 B = 4096 px of raw RGB; decode
  3 bytes per pixel, index `y*64 + x`, and print it as ASCII art. That is how
  the Phase C text layer and a console-painted sprite were verified on
  2026-09-24 (HELLO legible at rows 28-33; three sprite cells at the exact
  painted coordinates). But it answers an **empty body** under a 2-engine
  scene — a 12 KB response against `heap_largest` 6.6 KB — so the richest
  scene is the one you cannot read back. Short bodies are the socket pool, not
  an encoding problem: plain `curl` returns the full 12,288 B on an idle
  board, so check the length and retry rather than adding headers.
- **A rejected 30–35 KB pattern load leaves ~3.6 KB of heap and the next
  HTTP request panics** (frogger-2d, music-sequencer-for-v2 at 4096 px,
  JIT off too) — Gitea #678. Expect it in any sweep that pushes the whole
  library; it is why two rows of a jit-diff run read `no-baseline`.

## Reproducing a soak finding

hw-bench rows only say "crashed" or "vmerr"; to tell a panic from
starvation, replay the two patterns around it with the reader attached
(script shape in the 2026-09-05 session: fetch gallery entry N-1 and N from
`web/public/gallery.json`, `lxpBody('', p.source)` → `POST /api/code`,
sample `/api/status` every 5–8 s with a **20 s timeout** — at 1–2 fps the
device answers in 10–20 s and a 4 s timeout reads as "down", #259).

## Rules of thumb

- **`POST /api/layout` has two side effects to undo.** A `matrix …` body
  installs `grid W H` as a **user** map, which flips `/api/status`'s
  `geom.source` from `board` to `user` — put it back with an empty-body
  `POST /api/map` (a panel board then falls back to its own grid). And the
  reboot it asks for can drop the pattern resume, leaving the board default
  running; re-activate by id (`POST /api/patterns/<id>/activate`). Both
  verified 2026-09-19.
- **The board has no serial reset AND no plain reboot before v0.1.41.**
  `POST /api/reboot` landed with #475; before that the only reboot a panel
  board could be asked for was an OTA push (`/api/datapin` does not exist
  here — no strip SPI — and `/api/apmode` takes it off the network).
- **Leave the panel as you FOUND it.** Read brightness, `/api/pattern`,
  `/api/pattern.lxp` and the pixel count before touching anything and put
  those exact values back — brightness is Jeremy's (3 on 2026-09-07), never
  a number restored from a brief or from this file. Note every OTA reboot
  reverts the live pattern to the persisted default (Rainbow, 52 fps at
  4096 px), which reads as a throughput regression; re-push before
  measuring.
- **A pattern that runs at 1–2 fps at 4096 px makes the board unmanageable
  over the network** (#259). `/api/status` takes ~11 s; every client
  timeout under that says "dead". Reset via USB, don't wait.
- **Heap is the constraint at 4096 px**: idle ~68 KB, a 2D pattern ~46 KB,
  the default grid map is 48 KB on top; expect many "pattern too large for
  this device" rejections in a soak and don't treat them as failures.
- Soak with `HW_BENCH_RESET_CMD='timeout 3 socat -u /dev/ttyACM0,raw,echo=0,b115200 STDOUT'`
  so a starved device doesn't end the run (docs/tools.md).
- Flashing from scratch: `BOARD=board-seengreat-hub75 ./build-esp32.sh image`
  → `espflash write-bin --chip esp32s3 -p /dev/ttyACM0 0x0 firmware/target/luxel-full.bin`.
  If it comes up `boot:0x3 (DOWNLOAD…)` after espflash's reset, BOOT is held —
  Jeremy presses EN. Stock-restore image: `seengreat-stock.bin` (repo root, gitignored).
- **OTA to this board wedges the ProCpu inside a flash op about 44 % of the
  time (#294)** — silent, no serial, and usually RTC-watchdog recovered with
  the board back on the OLD slot. Check `slot` after each attempt, and before
  taking any measurement **push the same image to BOTH slots** so a rollback
  can't move you onto a different build mid-run; read `core1.last` afterwards
  (`SysRtcWdt` + ProCpu fence phase 3 = it happened).
  **"Recovered" is not guaranteed, so "push in a retry loop" is not safe
  advice on this board.** On 2026-09-21 (#655) a 986 KB push wedged *during
  the upload* — `POST /api/ota` never answered, `curl` timed out at 300 s
  having received 0 bytes — and the board then went fully off the LAN (no
  ARP entry, HTTP 000 connect hang) and was still gone 9 minutes later. It
  has **no serial node and no agent-controllable plug**, so that is a hard
  stop: a physical power cycle from Jeremy is the only way back. Budget one
  per plan that OTAs this board, decide up front how many attempts you are
  willing to spend, and prefer a single push you have prepared carefully
  over a loop. An earlier push in the same session, same size and same
  script, completed in 11.9 s — the failure is not predictable from the
  previous attempt.
- The OTA half of `tools/deploy.sh`/`ota-push.sh` needs `BOARD=board-seengreat-hub75`
  in the environment (they read the board map for the ELF path and `--chip`).
- Physical EN/BOOT presses and re-plugging are Jeremy's; everything else here
  is pre-authorized like the Athom rig.
