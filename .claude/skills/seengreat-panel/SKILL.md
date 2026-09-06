---
name: seengreat-panel
description: Use when working on the Seengreat RGB Matrix HUB75 S3 + 64x64 panel on Jeremy's bench — flashing, resetting or capturing a boot/panic log over its native USB, OTA-pushing an S3 image, or reproducing a crash on it. The board's USB behaves unlike the Athom's FTDI and the wrong move reboots it.
---

The Seengreat board (`board-seengreat-hub75`, ESP32-S3-WROOM-1-N16R8 + a
64x64 FM6124 panel) is at **192.168.0.238** (`luxel-f6b0a8`). Facts,
numbers and findings live in docs/boards.md "First light"; this skill is
the hands-on procedure. Deploying over the network is the deploy-device
skill (pass the S3 image explicitly — `ota-push.sh`'s default ELF path is
the classic-ESP32 one).

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
- **Free remote reset** for a starved or hung board, no Jeremy needed:
  `timeout 3 socat -u /dev/ttyACM0,raw,echo=0,b115200 STDOUT`. The node does
  NOT re-enumerate on this reset (it does on EN/flash resets).
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
- Physical EN/BOOT presses and re-plugging are Jeremy's; everything else here
  is pre-authorized like the Athom rig.
