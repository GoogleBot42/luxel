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
- **A rejected pattern load leaves the panel DARK** with `out_fps` 0 and
  `rescan_hz` 0 — it reads like a hang but is the documented rejection path
  (array budget / `RUNTIME_FLOOR`). Check `vmerr` before diagnosing.
- `tools/panel-load-bench.mjs` needs a FAST live pattern (push
  `library/frame-rate-scan.js` via `POST /api/code`, re-activate the stored
  pattern by id after); its tab phase repeats to about ±18 %, so take a
  repeated baseline in the same session before claiming a delta. `--clients 3`
  saturates the 3-socket web pool and returns nothing — use `--clients 1`.
- Brightness is Jeremy's setting — read it, never set it.

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

## Reproducing a soak finding

hw-bench rows only say "crashed" or "vmerr"; to tell a panic from
starvation, replay the two patterns around it with the reader attached
(script shape in the 2026-09-05 session: fetch gallery entry N-1 and N from
`web/public/gallery.json`, `lxpBody('', p.source)` → `POST /api/code`,
sample `/api/status` every 5–8 s with a **20 s timeout** — at 1–2 fps the
device answers in 10–20 s and a 4 s timeout reads as "down", #259).

## Rules of thumb

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
  time (#294)** — silent, no serial, RTC-watchdog recovered, and the board
  comes back on the OLD slot. Push in a retry loop and check `slot` after each
  attempt. Before taking any measurement, **push the same image to BOTH slots**
  so a rollback can't move you onto a different build mid-run, and read
  `core1.last` afterwards (`SysRtcWdt` + ProCpu fence phase 3 = it happened).
- The OTA half of `tools/deploy.sh`/`ota-push.sh` needs `BOARD=board-seengreat-hub75`
  in the environment (they read the board map for the ELF path and `--chip`).
- Physical EN/BOOT presses and re-plugging are Jeremy's; everything else here
  is pre-authorized like the Athom rig.
