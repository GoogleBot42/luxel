# MQTT & Home Assistant

The user-facing reference for Luxel's MQTT surface. The wire contract is
implemented once, in `crates/luxel-core/src/hamqtt.rs`, shared by the
firmware and the native mirror (`luxel serve`) — this page documents it;
the source is authoritative. Verified end to end against a real
mosquitto by `tools/mqtt-e2e.mjs` (see docs/tools.md).

## Enabling

Settings tab → MQTT form, or the API:

```
POST /api/mqtt      body = "host\nport\nuser\npass"   (empty host disables)
GET  /api/mqtt      {"enabled","host","port","user","hasPass","connected"}
```

Port 0/blank means 1883. There is no separate discovery toggle —
configuring a broker is opting in. The device reconnects on config
change (no reboot) and retries every 10 s while the broker is down.

`<id>` below is the device id: `luxel-` + the last three MAC bytes
(e.g. `luxel-4ae0d4`), shown in the web UI wordmark. The mirror uses
the fixed id `luxel-native`.

## Home Assistant discovery

On connect the device publishes retained discovery configs under
`homeassistant/…/config`, so HA (with the MQTT integration) picks it up
with zero YAML. One device card containing:

| entity | what it does |
|---|---|
| light | power + brightness (HA JSON schema) |
| select | the on-device pattern library, by name; re-announced when the library changes |
| sensor × 2 (diagnostic) | FPS and free heap, published every ~15 s |
| switch | playlist play/stop |
| button × 2 | playlist next / prev |

Availability rides `luxel/<id>/status` (`online`, retained; LWT flips
it to `offline` if the device drops).

## Topics

| topic | dir | payload |
|---|---|---|
| `luxel/<id>/status` | out | `online` / `offline` (retained, LWT) |
| `luxel/<id>/light/set` | in | HA JSON, e.g. `{"state":"ON","brightness":128}` — either field optional |
| `luxel/<id>/light/state` | out | `{"state":"ON","brightness":132}` |
| `luxel/<id>/pattern/set` | in | a library pattern name, verbatim |
| `luxel/<id>/pattern/state` | out | the running pattern's name |
| `luxel/<id>/playlist/cmd` | in | `play` / `stop` / `next` / `prev` |
| `luxel/<id>/playlist/state` | out | `ON` / `OFF` |
| `luxel/<id>/diag` | out | `{"fps":120,"heap":45000}` every ~15 s |
| `luxel/<id>/event` | in | pattern events — see below |

Brightness scales: HA speaks 0–255, the device stores 0–31 (the SK9822
current field). The round-trip is lossless device→HA→device, and a
tiny-but-nonzero HA value never rounds down to off.

## The event topic

`luxel/<id>/event` injects events into the running pattern's
`readEvent()` queue — the same queue as `POST /api/events` and
playground preview clicks (see the "External events" section of
docs/lang.md for the pattern-author side). It is command-only: no HA
entity is announced for it; it's a target for automations.

Payload: text, **one event per line**, whitespace-separated decimals:

```
type [x [y [value]]]
```

Missing `x`/`y` default to `0`, missing `value` to `1` — so a minimal
automation can publish just `1`. Lines that don't start with a number
are skipped; up to 32 events per publish (the queue size — older
events are dropped first when it overflows). No exponent notation.

Example — an HA automation that fires a pulse at a random spot on every
doorbell press (with, say, *Crosshair Pulse 2D* running):

```yaml
automation:
  - alias: Doorbell lights
    trigger:
      - platform: state
        entity_id: binary_sensor.doorbell
        to: "on"
    action:
      - service: mqtt.publish
        data:
          topic: luxel/luxel-4ae0d4/event
          payload: "1 {{ (range(0, 100) | random) / 100 }} 0.2"
```

What `type` and `value` mean is the pattern's business — the convention
so far: type 1 = pointer/hit with normalized `x`/`y`. Patterns that
ignore events simply let the queue age out; there is no cost to
publishing at a pattern that doesn't listen.

## Testing without hardware

`node tools/mqtt-e2e.mjs` runs the whole story locally: a scratch
mosquitto (in the dev shell), the native mirror, and assertions from
connect through event → pixel output. For a live broker, point the
mirror at it via `POST /api/mqtt` — but don't aim the mirror at a
broker that real devices use for discovery, or HA will see a phantom
`luxel-native` device.
