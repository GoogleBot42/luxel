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
- **Serial**: FTDI adapter at `/dev/ttyUSB0`, 115200 baud.
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
- Only one process may read the serial port at a time. A second or
  forgotten reader (e.g. a stray `cat`) silently steals bytes instead of
  erroring, and this has been mistaken for a dead port before. Kill any
  stray readers before concluding the port is broken.
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
(luxel-dev-device). A ready packet-builder script pattern is in the
UPDATES.md 2026-08-16 entry's session; ~20 lines of node using plain
`fs.writeSync`. Read the response with a separate short-lived `cat`
(single-reader rule — and mind that `pkill -f` patterns containing the
port name match your own compound command's shell and kill it; use a
character-class pattern like `tty[U]SB0`).

## 5. Normal updates vs. recovery

Once the board is back on Luxel, ordinary firmware/asset updates go over
OTA — see the deploy-device skill. Taking the board from stock WLED to
Luxel in the first place is the takeover mechanism documented in
docs/wled-migration.md, not this skill; this skill only covers the
physical rig actions (power, serial, restore) that support it.

## 6. Known open bug

An intermittent first-boot panic, `esp-alloc: Exceeded the maximum of 3
heap memory regions`, has been observed before `ota::init` runs. It
self-heals via the normal panic-reboot handler, but because it fires
before the boot-loop guard arms, a hypothetical deterministic version of
it would loop forever without ever tripping the guard's rollback. Not yet
root-caused (tracked in the athom-flash-rig.md agent memory file). If a
flash/takeover session starts looping instead of settling, this is the
leading suspect — stop and reassess rather than repeating the same flash
attempt.

## Failure modes

- **Board doesn't respond on the network after a restore/flash**: check
  the liveness signal (plug power draw) before assuming a bad flash — it
  may simply still be booting or joining WiFi.
- **Board loops instead of settling after a flash**: see the open bug
  above; don't keep re-flashing the same image expecting a different
  result — capture serial output and stop.
- **esptool can't connect in download mode**: almost always a
  button-hold/power-on timing issue — recoordinate with Jeremy rather than
  retrying blindly.
