---
name: athom-rig
description: Use when the Athom ESP32 board on Jeremy's bench needs serial-level recovery, a stock restore, or a manual power cycle — the only Luxel hardware the agent can drive through a full brick/recovery cycle without Jeremy doing all of the electrical work himself.
---

Scope: this skill is for serial- and power-level operations on the Athom
rig — recovering a bricked board, restoring stock WLED, dumping and
verifying flash. For routine firmware/asset updates to the same board over
the network, use the deploy-device skill instead; this skill is the
fallback when that path is unavailable. The WLED-to-Luxel takeover
mechanism itself (how the app image installs over WLED's OTA endpoint,
partition-table rewrite, credential inheritance) is owned by
docs/wled-migration.md, including the physical serial-rig facts
(esptool flags, wiring) — this file points there rather than duplicating
it, and covers only the agent-side procedure for driving that rig.

## 1. Rig inventory

- **Board**: Athom ESP32 music-reactive WLED controller, normally running
  Luxel at 192.168.0.183 (DHCP hostname `luxel-797e10`).
- **Serial**: FTDI adapter at `/dev/ttyUSB0`, 115200 baud. The hotplugged
  node can be ABSENT from the container (gone all of 2026-08-22) — check it
  exists first; if missing, panic/reboot detection falls back to polling
  `/api/status` (1 Hz worked) + post-hoc `slot` checks, and only Jeremy can
  restore it (replug, then `doas chmod 666 /dev/ttyUSB0`).
- **Power**: a zigbee2mqtt smart plug, topic `zigbee2mqtt/claude-switch`.
  Publish `{"state":"ON"}` or `{"state":"OFF"}` to
  `zigbee2mqtt/claude-switch/set` (e.g. via `mosquitto_pub`, available
  through `nix shell nixpkgs#mosquitto`). Broker connection details and
  credentials live in agent memory (jeremy-ha-broker.md). Publishing to
  this specific topic is an explicitly authorized exception (granted
  2026-07-26) to that memory's publish-only-under-`luxel/*` care rule —
  don't extend the exception to any other topic on that broker.
- **Liveness signal**: the plug reports power draw; roughly 0.7 W idle
  confirms the board is powered, without needing serial or network to
  answer.

## 2. Serial gotchas

- The device node arrives root-owned after every replug — chmod it before
  use (Jeremy runs this step with `doas`).
- **Configure the line before reading it.** A freshly hotplugged node is
  not at 115200 raw, and a bare `cat /dev/ttyUSB0 > log` against an
  unconfigured port produces **zero bytes** — not garbage, nothing at all,
  straight through a device reboot. Run
  `stty -F /dev/ttyUSB0 115200 raw -echo` first; then the same `cat`
  works. Cost a "the serial must not be wired to this board" detour on
  2026-08-22 with a perfectly healthy port.
- Only one process may read the serial port at a time. A second or
  forgotten reader (e.g. a stray `cat`) silently steals bytes instead of
  erroring, and this has been mistaken for a dead port before. Kill any
  stray readers before concluding the port is broken.
- **`pkill` for a stray reader must be its own command, mentioning the
  port nowhere else.** `pkill -f` matches the whole command line of every
  process — including the shell running your own compound command — so a
  character-class pattern like `tty[U]SB0` protects you only if the
  literal `/dev/ttyUSB0` doesn't also appear later in the same line (e.g.
  in the `stty` you chained after it). It did, and the command killed its
  own shell mid-run (2026-08-22). One `pkill -f "^cat /dev/ttyUSB0"`, on
  its own, then everything else.
- The converse trap: a background capture (`timeout N cat /dev/ttyUSB0 >
  log`) dies SILENTLY when its timeout expires, and the log's tail then
  reads as plausible-but-stale output. Before trusting a capture's tail as
  live, confirm the file is still growing (two `wc -c` a few seconds
  apart). Cost a confused test-read on 2026-08-15. Expect dropped bytes in
  captures too (FTDI flakiness) — grep counts are lower bounds, not exact.
- There's no DTR/RTS wired to the board, so `esptool` can't auto-reset it
  — it needs no-reset flags on the relevant esptool invocations. Use
  whatever docs/wled-migration.md's bench-workflow section currently shows
  for the exact flag spelling rather than guessing; it's easy to get
  subtly wrong.

## 3. Entering flash/download mode

The board has no reset button — the single case button is wired to GPIO0
and must be held down while power comes up to enter ROM download mode.
This is a two-person action: Jeremy holds the physical button, the agent
toggles power over MQTT. It cannot be done by the agent alone —
coordinate the timing with Jeremy before starting a serial flash.
UNVALIDATED — verify on first use: whether the power-toggle and the
button-hold need any particular sequencing beyond "button down before, and
held through, power-on."

## 4. Stock restore

Write the untracked, gitignored backup at the repo root back to flash from
address 0 (after the button-held power-up above):
`esptool --before no-reset --after no-reset write-flash 0x0 athom-wled-stock.bin`
(the exact command lives in docs/wled-migration.md's bench workflow). This is the same backup docs/wled-migration.md
references in its bench-workflow section. If it's ever missing, don't take
a fresh one casually — check with Jeremy first, since overwriting a
known-good backup with a bad dump is unrecoverable.

Verify any flash dump (stock backup or otherwise) with a second full read
and a hash compare before trusting it — the FTDI adapter has re-enumerated
mid-dump before, which silently truncates the read without an obvious
error.

### Provisioning restored WLED with zero button-holds (proven 2026-08-16)

The stock backup is UNCONFIGURED — restored WLED boots its captive-portal
AP, unreachable from the container. Don't ask for a phone: WLED 0.13
speaks **Improv over serial**. Write one Improv RPC packet (type 0x03,
command 0x01: `[ssid_len, ssid…, pass_len, pass…]`, header `"IMPROV"`
+ version 1, trailing sum-checksum + `\n`) to `/dev/ttyUSB0` at 115200
and WLED joins the LAN in seconds AND persists both cfg.json and
wsec.json (verified by cold power cycle rejoin — so takeover credential
inheritance works afterwards too). WiFi creds are in agent memory
(luxel-dev-device). The packet builder is now a real tool:
`node tools/improv-provision.mjs <port> <ssid> <pass>` (docs/tools.md) —
it reads WLED's reply itself, so stop any serial capture first
(single-reader rule — and mind that `pkill -f` patterns containing the
port name match your own compound command's shell and kill it; use a
character-class pattern like `tty[U]SB0`).

## 5. Normal updates vs. recovery

Once the board is back on Luxel, ordinary firmware/asset updates go over
OTA — see the deploy-device skill. Taking the board from stock WLED to
Luxel in the first place is the takeover mechanism documented in
docs/wled-migration.md, not this skill; this skill only covers the
physical rig actions (power, serial, restore) that support it.

## 6. The pre-guard heap-regions panic (root-caused + fixed 2026-08-16, PR #50)

The intermittent first-boot panic `esp-alloc: Exceeded the maximum of 3
heap memory regions` (before `ota::init`, self-heals on reboot) is
root-caused and fixed. Cause: only two `heap_allocator!` calls fill 2 of
esp-alloc's 3 region slots, so a flash-read flake corrupting the `HEAP`
static's `.data` slot array during the WLED bootloader's `.data` copy on
the takeover boot overflows it. The fix, `ota::preboot_guard`, arms before
the heap allocators (heap-free) and rolls back to WLED after 3 consecutive
pre-guard panics instead of looping forever. Reproduced + verified under
QEMU (`tools/qemu/heap-regions-test.py`); writeup in
docs/research/qemu-emulation-spike.md.

**Metal-validated 2026-08-30** (Gitea #53): a full stock-WLED → takeover
conversion ran clean with the guard armed — no heap-regions panic
recurred, first-attempt self-copy verify, cold-cycle rejoin, `boot
guard: healthy`. The 3-consecutive-panic rollback path itself remains
QEMU-verified only (it exists for a flake that can't be summoned on
demand). If a flash/takeover session ever loops instead of settling,
this remains the leading suspect: stop and reassess rather than
repeating the flash. The serial prints `preboot guard: … rolling back
to the other OTA slot` when the rollback fires.

## Power-cycle testing and the OTA boot-loop guard

Rapid power cycles — cycling again before the firmware reaches `boot_ok` —
trip the OTA boot-loop guard, which **silently rolls back to the other OTA
slot**: the board comes up healthy-looking but running the previous build
(bit a persistence test on 2026-08-30; the "surviving" behavior under test
was actually the old firmware's). After any power-cycle sequence, check
`slot` in `/api/status` (or the `booted from:` serial line) matches the
build you deployed before trusting the results, and leave time for
`boot_ok` between deliberate cycles.

## Failure modes

- **Board doesn't respond on the network after a restore/flash**: check
  the liveness signal (plug power draw) before assuming a bad flash — it
  may simply still be booting or joining WiFi.
- **Board loops instead of settling after a flash**: the pre-guard
  heap-regions panic (§6) is the leading suspect; don't keep re-flashing
  the same image expecting a different result — capture serial output and
  stop. `preboot_guard` should roll it back to WLED after 3 boots.
- **esptool can't connect in download mode**: almost always a
  button-hold/power-on timing issue — recoordinate with Jeremy rather than
  retrying blindly.
