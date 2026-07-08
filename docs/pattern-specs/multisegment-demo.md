# Multisegment Demo
kind: 1D
sensors: no (despite the corpus tag, it uses no sound or motion sensors; its external inputs are exported variables set over the websocket/JSON API by a home-automation controller)

## Purpose
A framework pattern, not a single effect: it divides one LED strip into several independent contiguous "zones", each with its own on/off state, color, brightness, effect, effect speed, and size in pixels. It is designed so a home-automation controller can drive every zone by setting exported variables over the JSON/websocket interface; this particular version additionally layers on a full set of web-UI controls for interactive testing. It is a v2-era community pattern by a well-known community author; the automation-oriented variant (and the exact exported-variable naming that automation drivers expect) is publicly documented in that author's GitHub pattern repository — a reimplementation that wants driver compatibility should take the exported names from that public documentation.

## What it looks like (defaults)
On startup the whole strip fades up from black over about two seconds (with a cubic ease). The strip is split into four equal zones. Each zone shows a solid color; hues are spread evenly around the color wheel (so four distinct colors), and brightness alternates zone-to-zone between full and quite dim. From there, everything is driven by the controls: each zone can independently run any of about nineteen effects at its own speed and color.

## Architecture

### Per-zone record
Each zone owns a small exported array of about seven fields: on/off state, hue, saturation, brightness, effect number, size in pixels, and speed. There are a dozen such arrays (the supported maximum number of zones), each individually exported so an external controller can address one zone with a tiny JSON write. A comment explains how to extend beyond a dozen. The active zone count is itself an exported variable (one up to the maximum).

### Other exported control variables
- A protocol/version variable: a negative sentinel means "web-UI mode / automation driver should ignore this device"; a small positive value means automation mode. All web-UI handlers become no-ops in automation mode so the two control paths don't fight.
- A boot handshake flag the driver clears after it has pushed zone data; while set, the fade-in stage holds at black.
- A transition-state variable selecting one of three stages: fade-in, steady run, fade-out. The driver can trigger graceful fades by writing it. Fade-in takes about two seconds with a cubic ease; fade-out about half that, also cubed. A global fade multiplier from this state machine scales every zone's brightness. (The original has some naming collisions and a dead internal state transition in this machine; implement the intent — a three-stage machine with timed eased fades — rather than the letter.)

### Derived per-frame tables
Each frame, before rendering, the code walks the active zones in order and computes:
- each zone's starting pixel index (running sum of sizes), with a huge sentinel value planted after the last zone;
- an "enabled" flag per zone (on, non-zero size, and not pushed entirely off the strip's end by resizing);
- an effective brightness per zone (zone brightness times the global fade multiplier);
- then calls the current effect's per-frame preparation function for each enabled zone.

### Effect dispatch
Effects live in two parallel function tables (a per-frame prep function and a per-pixel render function per effect, selected by the zone's effect number). Each effect may use up to three private scratch variables per zone, kept in one flat pool indexed by zone number times three — this is how one effect implementation runs simultaneously in several zones without interfering.

### Render loop
The per-pixel renderer exploits ascending pixel order: it keeps a "current zone" cursor and advances it whenever the pixel index crosses the next zone's start (the sentinel guarantees the last comparison never advances past the end). Enabled zones dispatch to their effect's render function; disabled or zero-size zones render black. This relies on pixels being rendered in increasing index order — a layout assumption worth noting.

### Zone resizing rule
Zone sizes are in pixels, zero up to the whole strip. Growing one zone clamps the following zones so the total never exceeds the strip; the first zone always starts at pixel zero and the last zone implicitly absorbs the remainder (the UI refuses to directly resize the last zone).

## The effects (about nineteen, selectable per zone)
Speeds below are at default settings; every effect's tempo scales with the zone's speed field, which multiplies its timer periods.

1. **Solid** — every pixel the zone's color at the zone's brightness.
2. **Glitter** — random sparkle field. A hand-rolled xorshift pseudo-random generator is re-seeded from a true random number at intervals of roughly a quarter second (accumulating elapsed time in a scratch variable); per pixel, a value derived from the seed and pixel index gives a brightness that is then cubed, with saturation pulled slightly below full for the brightest sparks so they whiten. Reads as red-family glitter.
3. **Rainbow bounce** — a rainbow gradient spanning the zone whose hue offset swings back and forth (triangle wave, a few seconds per swing).
4. **Mini scanner** — a bright bar about a fifth of the zone wide (never narrower than a few pixels) sweeping back and forth on a triangle wave; brightness falls off linearly with distance from the bar center, then squared. Uses the zone's color.
5. **Breathe** — whole zone pulses smoothly between a small floor and full, a couple of seconds per breath. Zone's color.
6. **Slow color** — whole zone solid, hue cycling continuously around the wheel over several seconds.
7. **Snow** — zone shows its solid color; a small handful of pixels (roughly the brightest few percent of a per-pixel random draw) render pure white sparkles; the random seed refreshes about three times a second.
8. **Chaser up / 9. Chaser down** — a sinusoidal brightness ripple marching along the zone (one variant each direction), several-second cycle, zone's color. (Its spatial term mixes pixel offset into a sine without normalizing by zone size, so the ripple texture depends on zone length — reproduce the flavor, not the accident.)
10. **Strobe** — hard on/off square-wave flash, on for about a quarter of each short cycle (several flashes per second). Zone's color.
11. **Wipe up / 12. Wipe down** — a boundary sweeps across the zone about twice a second; pixels behind it show a "new" hue, ahead of it the "previous" hue. When a sweep completes, the new hue becomes the old and a fresh hue is sampled from a slowly cycling timer, so successive wipes paint successive rainbow hues.
13. **Springy theater chase** — evenly spaced single-pixel dots marching along the zone while the spacing itself oscillates between about two and about ten pixels on a triangle wave, giving an accordion feel. Zone's color.
14. **Color twinkles** — a port of the classic twinkle pattern: hue and brightness per pixel come from nested sines of the pixel offset (divided by small constants) phase-shifted by two slow timers; brightness is cubed and gated below a small threshold to zero so only distinct twinkles show. Full-spectrum colors.
15. **Plasma** — a brightness wave scrolls through the zone while the whole zone's hue drifts; saturation is reduced where brightness peaks (an over-unity saturation term clamps to whiter peaks); brightness cubed.
16. **Ripples** — three sine ripples at spatial frequencies roughly in a ten/six/three ratio across the zone, moving at different rates (two by sawtooth timers in opposite directions, one oscillating), each folded to a V shape, averaged, then squared; saturation dips where bright. A miniature "oasis"-style water shimmer in the zone's color.
17. **Spin cycle** — a fan of hue bands whose spatial frequency itself changes over time, with several triangle-wave brightness dots, brightness cubed; hue folded into a half-wheel window that drifts. Quirk: unlike every other effect it computes position from the whole-strip index rather than zone-relative offset, so its phase depends on where the zone sits on the strip.
18. **Rainbow up / 19. Rainbow down** — a full rainbow gradient across the zone scrolling continuously in one direction (a variant each way), fast (several wheel cycles per several seconds).

## Web UI controls (all no-ops when automation mode is active)
All are sliders except the color picker, since the host UI offers sliders/pickers:
- **Active zone** (slider): selects which zone the other controls edit (scaled across the active zone count).
- **Zone on/off** (slider used as a toggle): floors to zero or one for the selected zone.
- **Effect** (slider): scaled and floored across the effect count to pick the selected zone's effect.
- **Speed** (slider): inverted so right = faster; it maps the slider to a multiplier applied to every timer period in the zone's effect (with a tiny floor so it never reaches zero). Mid-slider is intended to feel like the stock speed.
- **Zone size** (slider): sets the selected zone's pixel count (scaled across the strip length); the last zone cannot be resized directly.
- **Color** (HSV picker): sets the selected zone's hue, saturation, and brightness.
- **Zone count** (slider): one up to the maximum dozen; changing it re-initializes all zones to defaults.
- **Enable web UI** (slider as toggle): flips between web-UI mode and automation mode by setting the protocol/version variable.

Each numeric handler keeps a "last value" latch and only writes to the zone record when the incoming value actually changes, so controller-written values aren't clobbered by the UI's periodic re-delivery of unchanged slider positions. Note these latches are shared across zones (not per-zone) in the original — a mild quirk; per-zone latches or the same shared behavior are both acceptable.

## Randomness
True randomness is used only to seed the glitter/snow generators periodically; within a frame those effects use the deterministic xorshift generator so all pixels of a frame derive from one seed (stable sparkle placement between reseeds).

## Layout assumptions
Pure 1D, ascending-index rendering assumed. Strip length comes from the runtime pixel count; nothing hardcoded except the maximum zone count (a dozen) and the per-zone scratch-variable count (three), both meant to be easy to raise.

## Non-obvious details worth preserving
- One flat scratch pool plus zone-indexed accessors is what lets a single effect implementation run concurrently in many zones.
- The sentinel start value after the last zone lets the render loop advance zones with a single comparison per pixel and no bounds check.
- The per-frame recomputation of zone starts/enables means zones can be resized or toggled live at any time without any explicit re-layout step.
- The dual control plane (exported JSON variables vs. web UI) guarded by one mode variable is the pattern's core design idea; the header explicitly warns the two interact badly if both are active.
