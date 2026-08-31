# Update log

## 2026-08-30 — The library gate never ran a mapless strip (Gitea #193)

The #167 soak turned up one error in 303 patterns: `sound - spectromatrix
render2D` → `array index out of bounds`. The issue guessed a sensor-less
device hands the pattern an empty `frequencyData`; that was wrong on both
counts. `Engine::from_program_budgeted_at` declares `("frequencyData", 32)`
unconditionally, so a sensor-less engine gets 32 zeroed bins like any other,
and the array that actually overflowed was the pattern's own 256-slot
`persist[]` buffer.

The pattern's **1D fallback** was the bug:

```js
var fbWidth = max(1, floor(sqrt(pixelCount)))
render2D(index, col / fbWidth, row / fbWidth)   // row count is NOT fbWidth
```

`floor(sqrt(n))` gives the width; the row count is `ceil(n / fbWidth)`, which
is larger on every count that isn't a perfect square. At 60 px the matrix is
7 wide and **9** rows, so the last rows normalize to `y = 8/7 = 1.14`, and
`floor(y * 15.99) * 16` walks `persist[]` past 255. Fixed by deriving the
height separately (`fbHeight = max(1, ceil(pixelCount / fbWidth))`), which
also keeps `y < 1` strictly. Swept clean now on every count 1..400 plus 512 /
600 / 1000 / 1024 / 1500 / 2048.

The interesting part is why 322/322 sweeps kept missing it. `luxel check`
**always installed a 2D grid map**, and with a map the engine picks
`render2D` — so a 2D pattern's own `render` fallback is code the acceptance
gate never executed, on any grid. Only a real mapless strip runs it. So
`luxel check` gained `--strip N` (N pixels, no map) and
`tools/check-library.sh` now sweeps five rigs: grids `default`/`16x16` plus
**mapless strips at 60 / 300 / 512 px**. Baseline **303/303 on all five**.
The strip counts stay under the PB-compat 10,240-element array budget, which
a couple of patterns legitimately exceed at 2048 px; `STRIPS=` drops the
strip half.

Two secondary notes from the sweep. The soak ran at **60 px, not 300** — the
issue's "at 300 px" came from the report's hardcoded *"fps at 300 px"* line,
and 300 px is one of the counts where this bug happens to be benign
(`y = 17/17 = 1.0` exactly, which still floors to slot 255). And `oasis`
goes out of bounds above ~1450 px from 16.16 overflow in
`(index + off) * w`, filed separately.

Verified on the Athom rig at its real 60 px: the pre-fix source pushed via
`/api/code` reports `vmerr: "array index out of bounds"`, the fixed source
reports `vmerr: null` at the same fps and heap.

## 2026-08-30 — One `Reply` type for the whole device API: −24 KB of firmware

`firmware/src/server.rs` returned **thirteen different response shapes** —
`(CORS, JSON, String)` for the ~40 JSON routes, `(gzip, etag, cc, FlashAsset)`
for a compressed asset, `(StatusCode, [(&str,&str); 4], &str)` for the CORS
preflight, and so on. picoserve monomorphizes `IntoResponse::write_to` once
per tuple *shape*, `ForEachHeader::call` once per header-*value* type, and
`Response`/`HeadersChain`/`ContentBody` once per combination, so those
thirteen shapes cost 22 `write_to` instantiations (20,478 B), 37
header-machinery symbols (8,492 B), and a `core::fmt` `Display` shim per value
type. Gitea #167 collapses all of it into one concrete type:

```rust
struct Reply { status: StatusCode, headers: heapless::Vec<(&'static str, HVal), 4>, body: ApiBody }
enum HVal  { Static(&'static str), Owned(String) }          // the ONE header value type
enum ApiBody { Json(String), Text{…}, Bytes(Vec<u8>), Asset(FlashAsset),
               Source(CurrentSource), Envelope(CurrentEnvelope), Empty{…} }  // the ONE body type
```

Exactly one `Response<HeadersChain<ContentHeaders, &[(&str, HVal)]>,
ContentBody<ApiBody>>` now exists in the image, and therefore exactly one
`write_to`.

**Measured, `board-c6-devkit` credless flake build: 1,005,168 → 980,784 B
(−24,384), slot margin 43,408 B / 4.14 % → 67,792 B / 6.46 %.** Every other
board came in between −23.0 and −24.4 KB (whole-fleet table in
docs/boards.md; both columns are an A/B with only `server.rs` differing —
which turned out to matter, because a side-finding of this work is that
editing *only markdown* moves the C6 image ~600 B via `.Lanon` symbol
renaming. That ~±0.7 KB noise floor is now written down in
docs/size-report.md). This is the largest single reduction the firmware has
taken
since the opt-level "s" switch, and — unusually — it *beat* its ≥10 KB
estimate rather than missing it. The C6 is out of CI's 6 % warn band for the
first time since the board was added.

Symbol-level: `write_to` 22 syms/20,478 B → 3 syms/6,882 B (one real closure
at 6,556 B plus drop glue); header machinery 37 syms/8,492 B → 4 syms/966 B;
the `picoserve` bucket 32,948 → 7,278 B; `rust core` 58,815 → 50,843 B (the
per-value-type formatting shims genuinely became dead — unlike #168, where
`core::fmt` could never leave). Our own `luxel-fw` bucket *grew* 10 KB and the
flat dispatcher's closure went 22,050 → 26,204 B: with one response type the
LTO boundary moves and the write path inlines into the dispatcher instead of
standing alone 22 times. Bucket deltas lie; the image total is the number.

**What had to be preserved, and was.** The pad-don't-truncate discipline in
`stream_flash_readback`/`stream_store_readback` (a short body wedges a pool
slot for 45 s), the exact-from-snapshot `content_length()` of
`FlashAsset`/`CurrentSource`/`CurrentEnvelope`, the 1 ms `Timer` between 4 KiB
flash reads, the flat-match dispatcher with a single `write_to` exit per arm,
the reply-then-reboot ordering on `POST /api/wifi` and `/api/apmode`, and all
four headers on the OPTIONS preflight (`MAX_HEADERS` is sized for exactly that
arm, with a compile-time assert, because a silently dropped CORS header breaks
cross-origin DELETE with no trace). The routing shape is now load-bearing for
size too: picoserve's `MethodRouter` would wrap the writer in a private
`IgnoreBody<W>` for HEAD — a second writer type that duplicates every GET
instantiation — so the hand-written `PathRouterService` stays, and says so in
a comment next to the existing stack warning.

**Two wire changes, both fixes.** JSON/octet-stream/HTML responses used to go
out with **two** `Content-Type` headers (picoserve's `Content` impl for
`String`/`&str` emits `text/plain; charset=utf-8`, then our explicit header
was appended); now there is exactly one, correct, and matching the native
mirror in `crates/luxel-cli/src/serve.rs`. And a 304 asset revalidation now
carries the `Content-Type`/`Content-Length` its 200 would have carried (RFC
9110 §15.4.5) where before it carried neither — `Response::new` always emits
both and cannot suppress them, and RFC 9112 §6.3 keeps a 304 body-less
regardless of what Content-Length says.

**RAM cost.** The single `write_to` future is the union of every body type, so
the per-slot static task arena grew: `tools/stack-check.sh` on
`board-pixelblaze-v3` reads `.stack` 29,124 → 27,828 B (3,828 B above the
24 KB floor) and the dispatcher's poll frame 2,128 → 4,928 B against a
12,288 B budget. Both within limits; the poll frame is now the one to watch
when adding routes.

**Gates.** All 8 board images build and pass `tools/image-check.sh` (markers
+ margin); `tools/stack-check.sh` clean; `cargo clippy` on the default board
shows no new lints; `cargo test --workspace` 223/223; the full QEMU harness
(`tools/qemu/run-all.py` — takeover app1/app0/fault, heap-regions
selfheal/rollback) passes on the athom image built from this change;
`tools/serve-e2e.mjs` green. The firmware's HTTP surface has no hardware-free
e2e, so the wire contract was verified on the Athom rig after an OTA of this
build (slot ota_1 → ota_0, same 0.1.39 version string — slot is the
discriminator): every response family checked with raw curl — single
`Content-Type` on JSON/octet/HTML, `Content-Length` == body bytes everywhere
including the flash-streamed `/api/pattern`/`/api/pattern.lxp`, asset 200 with
ETag+Cache-Control+gzip, body-less 304 revalidation carrying the would-be-200
`Content-Type`/`Content-Length` and the same ETag, all four OPTIONS preflight
headers, text/plain 404, captive-portal shape untouched. Then a full
`tools/hw-bench.mjs` soak: 302/303 patterns clean through the new reply path
(the one error, an `array index out of bounds` in sound-spectromatrix-render2d
at 300 px, does not reproduce natively under snap.mjs and is a VM/sensor-env
issue unrelated to this change — filed as #193), pixel-count curve unchanged,
device healthy after (vmerr null, heap ~86 KB free at 60 px).

## 2026-08-30 — Lint detached `//#` directives; sweep 13 library patterns

A `//#` control directive parses in exactly two places — trailing on the
`export function` line, or on the line directly above it (web/src/lib/
hints.ts). Written on the first line *inside* the handler body, which reads
perfectly naturally, it is silently ignored: the control degrades to an
unbounded 0..1 slider with no default. That was the whole ryb-colors defect
in the review pass, and Gitea #179 found the same inert placement across the
library.

**The lint (the durable half).** `tools/check-library.sh` now runs a directive
lint before it compiles anything: any line carrying a `//#` with a
`min`/`max`/`step`/`default` key that is not in one of the two parsed
positions fails the sweep, naming file:line. Prose that merely mentions `//#`
is not flagged. Implemented as inline awk mirroring the two hints.ts regexes,
so the gate has no new dependency.

**The sweep.** 13 files carried 37 detached directives: b-lightning-flashes,
bouncy-boxes, dire-spider-2d, m5stack-hex-panels, multisegment-demo,
music-sequencer-for-v3-only, rainbow-comet, rock-sparks,
sound-music-spectrum-visualizer, sunrise-2d, thunderstorm,
twinkling-classic-xmas-strands, voronoi-2d. All 13 untouched renders stayed
byte-identical (snap.mjs port.png md5, before vs after) — moving a comment is
inert to the engine. The four "partial misses" the issue also listed
(perlin-simplex-noise-1d, rgbclock-2d, rocket-by-tony-hampton,
spinning-plasma-2d) turned out to be clean; the issue's scan had counted prose
mentions of `//#`.

**Two real-unit handler bugs, exposed by attaching.** A directive that
declares non-0..1 bounds makes the UI send real units, so a handler that
rescales as if it got 0..1 breaks. dire-spider-2d's `sliderLegWidth`
(min=0.1 max=0.2) would have collapsed to a 0.11..0.12 dead slider; both its
handlers now clamp instead of rescale. voronoi-2d had the same bug in three
*already-attached* directives, i.e. live on master: `sliderPoints`
(min=1 max=8) ran `1 + floor(v * 7)`, so the UI's own declared default of 5
set `count = 36` against an 8-element array — the pattern went black with
"array index out of bounds" the moment the playground seeded its defaults.
Both fixed and verified in real chromium.

**The playground seeds `default=` on load** (App.svelte), so attaching a
directive makes it the effective default. Seven patterns' UI-default render
now differs from their top-level initializer state, because the declared
defaults were written while the directives were inert and nobody could see
them: b-lightning-flashes (halfWidth 8 px → 3.7 on a 60 px strip) and
rainbow-comet (headInterval 0.09 → 0.075) are the visible ones; bouncy-boxes,
rock-sparks and twinkling-classic-xmas-strands drift by a few percent;
dire-spider-2d differs by one LSB of 16.16 control quantization; voronoi-2d
re-rolls its seed positions. Retuning the control math so each `default=`
reproduces its shipped constant is Gitea #188, deliberately kept out of a
placement fix.

## 2026-08-30 — snap.mjs probes dials across their real range, not 0/0.5/1

`--probe-controls` swept every control at raw 0, 0.5 and 1 and ignored the
`//#` directive entirely. That was fine when ports were 0..1 sliders; with
the library now largely on real-unit bounds (the controls pass below), it
under-drove nearly everything — a `min=1 max=60` dial was poked across 1% of
its range, and `clamp(floor(v), 1, 12)` collapsed all three probe points onto
the same value — so dials reported borderline or inert while working fine.
Five separate fix-pass agents hit this independently and fell back to hand-run
`--controls-port` sweeps (Gitea #180).

The probe now sweeps each control at the **min, midpoint and max of its own
`//#` range**. A one-sided directive takes the raw default for the other end;
the midpoint snaps to `step` when that lands strictly inside the range, so an
integer dial is probed at an integer (`min=1 max=8 step=1` → 1/5/8, not
1/4.5/8). Raw 0/0.5/1 remains the fallback for a control with no usable
bounds — which is every corpus ORIGINAL, since Pixel Blaze has no directives —
and for pickers, whose three components are colour axes rather than a range.
`probe.json` gained `bounds`, `probedAt` and `boundsProbed` per control, and
the stdout table a `probedAt` column that stars the bounds-driven sweeps.
Measured on real ports: chill-confetti's HueJitter went from a whole-rig delta
of 0.76 (under the bar, rescued only by the lit threshold) to 8.55;
2d-wandering-fireball's BallSizePercent from 9.87 to 31.82.

`meta.json`'s per-side `controls` entries now carry the parsed `bounds`
alongside `{name, kind, label}`, so a snap alone confirms a directive was
seen — and a port control with no `bounds` key has one that did NOT bind
(detached by a blank line, misspelled, on the wrong export), which used to be
invisible without re-reading the source.

Plumbing: the `//#` parser was duplicated in `web/src/lib/hints.ts` and
`tools/verify/review/engine.js`; rather than adding a third copy, the harness
copy moved to `tools/verify/hints.mjs` (served to the review UI as
`/hints.mjs`, the same way `sensormodel.mjs` already is) and now also owns
`directiveRange()`, the min/mid/max derivation. `web/tests/hints.test.mjs`
runs every parser case against BOTH implementations so the twins can't drift,
and covers `directiveRange` edge cases (one-sided, step coarser than the
range, inverted/degenerate bounds). `hints.mjs` folds into `harnessSha256`,
so cached runs from before this invalidate correctly. Also fixed the two
header-doc drifts #180 named: per-side blocks live under a top-level `sides`
key, and `--controls-orig/-port` match a control by export name **or** display
label.

## 2026-08-30 — digitalRead honours pinMode: a pulled-up pin idles HIGH

`digitalRead()` returned 0 unconditionally, which for the standard
button-to-ground wiring (`pinMode(pin, INPUT_PULLUP)`) reads as "button
held forever" — the exact opposite of idle, and the reason every button
corpus pattern renders its pressed state on both sides of a review pair.
The VM now remembers the pull-up bit of the last `pinMode` per pin (a
64-bit mask, pins 0..63; ESP32 tops out at 39) and `digitalRead` reports
the resulting idle level: 1 under `INPUT_PULLUP`, 0 for plain `INPUT`,
`INPUT_PULLDOWN`, outputs, and unconfigured pins — so the default stays
what it was. Masking on the pull-up bit (0x04) rather than comparing to
`INPUT_PULLUP` (5) also covers a hand-built `INPUT | 4`.

No shipped pattern changes behavior: sunrise uses plain `INPUT`,
lightbulb-crank uses `INPUT_PULLDOWN`, and example-button-w-debounce
drives its debouncer from a UI control (its header comment about the
stub is updated). This is Gitea #177 item 1 only — there is still no way
to *drive* a pin from outside the pattern, so the pin-injection ABI
(item 2), fixups control pins (3), and real firmware GPIO (4) stay open.

## 2026-08-30 — Fix pages/release CI: crates.io API 403 on esp-hub75 fetch

GitHub pages (and release) builds were dying in `nix develop`: the
esp-hub75 source fetch hit `crates.io/api/v1/.../download`, which now
returns 403 to non-browser clients. Swapped the fetchurl to the
`static.crates.io` CDN URL (same bytes, hash unchanged — verified by
sha256 before the swap). Also added `flake.nix`/`flake.lock` to
pages.yml's push path filter, since devshell changes can break the site
build.

## 2026-08-31 — The review verdict lands: 110 pattern decisions acted on

Jeremy sat down with the review UI and decided all ~139 judged pairs
(tools/verify/decisions.json): 29 good, 25 delete, 82 needs-work, 3 fork.
This pass implements every one of the 110 actionable decisions, fanned out
across ~20 subagents in one worktree.

**Deleted (25)** — ports plus specs, verdicts, and their fixups entries
(automap's vars pin, skypirate's grid rig); pairs.json regenerated (268 →
267 pairs after the flag redesign below).

**Controls pass (53 patterns)** — the big "perfect, but give me controls
the original doesn't have" bucket. House style established: real-unit
`//#` bounds (seconds, degrees, pixels, percent, integer counts with
step=1) on the line above the export, handler math shaped so the declared
default reproduces the shipped constant, untouched renders byte-identical
(verified port.png-vs-port.png per slug), never a pattern-level brightness
control. Two look changes were explicitly requested and made:
color-twinkle-bounce (full-value coherent crests, four palettes) and
rainbow-fonts (the "muted" was a 0.35 value cap).

**Defect fixes (17)** — highlights: crossfading measured everything as
the wrong canvas fraction (5 bars vs 8, braid pinned at pixel 0);
glittering-jewels indexed 20-element arrays at 381 (real-units slider,
0..1 handler); kitt-w-color-picker's darkness was decay-before-draw plus
a fade shorter than one frame; line-dancer-2d's melt phase and
millipede's backwards-walking gait were both derivative bugs (a `+1`
floor that never collapsed; a triangle wobble whose derivative is a
square wave); blinky-eyes-2d mixed folded and display units (aspect 0.71
vs the original's 1.28); animated-asterisks' AnimateWidth was a ~59 s
cycle that silently overrode the width slider (removed); ryb-colors had
all its `//#` directives inside function bodies where the parser
silently ignores them.

**Rewrites and forks (10 new/replaced patterns)** — us-flag + us-flag-2d
(cloth-wave redesigns replacing 1-usflag-blink, 13 crisp stripes at every
size); fireworks-finale (full shell lifecycle, forked from 4th);
lightning-strike (near-white restrike lightning, forked from
lightning-zap); matrix-green-waterfall-1d; bouncing-balls-rgb-2d (true
ballistic analog — the existing 2d one is Lissajous drift); Grinch's
Heist (bulb-stealing story pattern) and Infinite Snake (self-playing
snake whose Smartness dial measures 413 deaths → 9 across its range)
replacing the-grinch and snake-2d; continuous-cellular-automata reworked
into a self-evolving ring-topology waterfall with a convergence watchdog.

**Engine: 21 easing builtins appended** (batch 7 — the full Sine/Quart/
Quint/Expo/Circ trios plus the missing Back/Elastic/Bounce variants),
tested against f64 references; easing-library-v1-0 now showcases the
builtins as first-class values instead of 30 hand-rolled lambdas.
`board-c6-devkit` re-measured post-rebase: 1,005,488 B (+4,016 B over
the #168 fmt-diet baseline), margin 43,088 B / 4.11 % —
above the 3 % floor, but the next VM growth should measure C6 first.

**Harness** — review UI no longer fabricates untouched slider positions
(the holiday-diagonal-stripes "sync bug": both panes drew 0.5 while the
engines held 0.4 vs 0.5; unhinted untouched controls now show a `?`
placeholder), and it grew an "addressed" filter chip: the fix pass stamps
`addressedAt` on every decision entry it acts on, re-deciding clears the
stamp. example-button-w-debounce is drivable from the UI (Button hold /
too-short Tap / DebounceMs / Modes) pending real GPIO.

Deferred items are on Gitea (GPIO input stubs, the playground's copy of
the fabricated-slider bug, a `//#`-placement lint, probe-controls
ignoring bounds, minor fidelity residue).

## 2026-08-30 — size-report reads real ELF sizes; the ~1 GB RISC-V row is gone (#174)

`tools/size-report.py` bucketed `nm -C --size-sort`, which *estimates* a
size for symbols that carry none by subtracting from the next symbol's
address. On `board-c6-devkit` that turned linker-script NOTYPE symbols
(`_rwtext_len`, whose value *is* a length), `$d` mapping symbols and `.L*`
locals into a 1,082,830,941-byte "other C/asm" row. Now it buckets
`nm -C -S --defined-only` — real `st_size`, size-less symbols dropped and
counted — so the C6 row reads **133,402 B** and the accounted total
915,322 B against a 1,005,024-byte OTA image.

Xtensa never showed the blow-up only because its `_rwtext_len` estimate
landed above the old `>= 0x80000000` drop guard — but its numbers were
guesses too. On a credless `board-athom-music` build the fix moves "other
C/asm" 99,097 → 74,949 B and the blob bucket 215,207 → 212,003 B: 87
size-less symbols, all hand-written asm or ESP32 ROM stubs
(`_WindowOverflow*`, `save_context`, `mktime`, `atoi`, `idle_hook_fn`,
`g_wifi_osi_funcs`) plus pure markers like `_rwtext_end`, which the old
script credited with 12,028 B of bytes it does not own. **Every
Rust-crate bucket is byte-identical before and after on both
architectures** — the deltas are exclusively size-less symbols. Those asm
bytes are real but unattributable, so the accounted total is now an
honest lower bound and the script says how many symbols it skipped.
docs/size-report.md's "ignore the other C/asm row" caveat is retired.

## 2026-08-30 — The playground stops fabricating untouched control values (#178)

The review UI's fabricated-slider fix, ported to `web/src/components/Controls.svelte`.
An untouched control is running whatever the pattern's own top-level code
put in the variable, and the engine cannot hand that back — `lx_set_control`
with no args *invokes* the handler. Only a `//# default=` declares it. The
playground was nonetheless drawing `?? 0.5` (or `?? 0` / `?? 1` per kind)
and presenting it as the running value, which is exactly what hid the
orig-0.4 / port-0.5 Slope gap in holiday-diagonal-stripes on the review side.

Untouched controls with no `//#` default now render as placeholders: the
slider/number widgets dim to 0.4, an amber `?` badge carries a tooltip
explaining the position is a guess, and toggles go **indeterminate** (the
native tri-state says "unknown" better than any badge). Triggers, gauges
and showNumbers are exempt — nothing to guess. First user interaction
writes `values[name]` and the row settles to a normal control. Controls
*with* a `//#` default are seeded into `controlValues` at compile, so they
were never guesses and are untouched by this change.

Verified in real chromium (puppeteer-core): Blink Fade's undeclared
`sliderSpeed` renders dimmed + badged, a real mouse drag settles it to
0.831 at full opacity; Audio Volume Meter's 7 `//#`-defaulted sliders and
its gauge render exactly as before while both undeclared toggles show
indeterminate, and clicking one settles only that one. `svelte-check` 0
errors, 11/11 `npm test`, full `web/tools/e2e.mjs` suite green (69 checks,
including the pre-existing `//# default applied` and slider/number
round-trip assertions).

## 2026-08-30 — fmt diet: JSON builders off core::fmt; C6 margin 4.24 → 4.49 % (#168, #169)

Converted every `format!`-built JSON/response body — `server.rs` (41
sites), `playlist.rs`, `patterns.rs`, `resume.rs`, `devicemap.rs`,
`mqtt.rs`, `main.rs`'s vmerr strings, and luxel-core's shared
`jsonview.rs` — to `push_str`-style building with new non-fmt printers:
`Fx::dec_str` (the exact 16.16 decimal printer, now shared with `Display`
so they can't diverge) and `jsonview::{push_u32, push_i32, push_u64,
push_i64, push_hex}`, all pinned against `format!` output by unit tests.

**The headline is the lesson, not the number.** `core::fmt` never leaves
the image — `println!` and `Debug` keep it linked, so the measured
23.5 KB fmt bucket was never reclaimable and only dropped ~0.6 KB
(23,550 → 22,922 B by #160's measure). Worse, the naive conversion GREW the C6 image by
8,592 B: `String::push_str` inlines a reserve-and-copy at every call
site, and the builders have hundreds. The fix that turned it around is
`#[inline(never)] jsonview::push_piece` — one shared append function
every literal goes through. Net: `board-c6-devkit` 1,004,112 →
**1,001,472 B** (−2,640 B, margin 44,464 → 47,104 B / 4.49 %),
`board-athom-music` 955,376 → 954,016 B (−1,360 B). #168's 5–10 KB
estimate was wrong for the structural reason above; the remaining diet
lever is the picoserve monomorphization collapse (#167).

Verified: byte-identical bodies (mechanical literal-sequence comparison
over all 236 append sites, plus a live before/after capture of 18 GET
endpoints on the Athom — only volatile fields differ), 219 luxel-core
tests green, both toolchains build clean, all 5 QEMU harness tests pass,
stack-check clean, OTA'd to the Athom (ota_1) and 300-poll status soak
with stable heap. Also refreshed `docs/size-report.md` per #169 (the
f64-parsing bucket it listed as deliberate is no longer linked at all)
and recorded the new margins in docs/boards.md.

## 2026-08-30 — WLED takeover validated on metal; installer sheds its beta banner (#53)

Second full bench conversion on the Athom, this time with the
preboot_guard build (the fix that was QEMU-only since 2026-08-16): stock
WLED 0.13.2 restored over serial (button-held download mode — the one
Jeremy step), provisioned onto the LAN via Improv-serial, then a real
takeover from a **credless master image** (the faithful user path —
release images never carry creds) uploaded to WLED's `/update`. Result:
foreign table detected, WiFi **and settings** inherited (30 px, ws2812,
brightness, power cap, gamma — all carried over from WLED), 936 KiB
self-copy verified clean on the FIRST attempt (the 08-16 verify flake
did not recur), table rewritten, boot from ota_0, LAN rejoin on
inherited creds at the same address, and `boot guard: healthy` after a
deliberate cold power cycle. No heap-regions panic; the armed
preboot_guard never had to fire (its 3-panic rollback path stays
QEMU-verified — it guards a flake we can't summon). One transient
RTC-WDT reset before the first takeover boot self-recovered instantly.

Shipped on the back of it: the installer's beta banner is gone
(`Flash.svelte`), docs/wled-migration.md's "why beta" section is now a
history section, and the Improv packet-builder the 08-16 session left
in a scratchpad is a real tool — `tools/improv-provision.mjs`
(docs/tools.md) — with the athom-rig skill updated to point at it.

## 2026-08-30 — CI now fails on a thin OTA slot, not just a full one (#160)

The release workflow only ever asked "does the app image fit the 1 MiB OTA
slot?", which is the wrong question one release too late. `board-c6-devkit`
lost ~7 KB of headroom in two days (map-aware blur/glow, then the device
output palette) and sits at 43,872 B / 4.18 % free — and it is the one
board nobody here can serial-recover (#56), so the failure mode is a device
that `/api/ota` refuses to update.

`tools/image-check.sh` grew a second gate beside the //SIZETEST marker
check: for app images it computes the slot margin, **fails below 3 %**
(31,458 B) and **warns below 6 %** (62,915 B). All eight board variants go
through it in `.github/workflows/release.yml`, which drops its own
duplicate ceiling check — one place owns the size rule now. The math is
integer-only shell (no bc/python on a minimal runner) and the thresholds
are env-overridable (`MIN_MARGIN_PCT` / `WARN_MARGIN_PCT` / `OTA_MAX`).
ELF inputs skip the size half, since build-esp32.sh's local call hands it
an ELF and an ELF is not the artifact that has to fit.

Why 3 %: it is ~12 KB under today's tightest board, so master stays green
today, but roughly two more medium features trip it — which is the point at
which the diet in docs/size-report.md stops being optional. Verified
against stubbed images at all eight recorded board sizes plus the
threshold edges (31,458 B passes with a warning, 31,457 B fails, the exact
ceiling fails, one byte over fails with the "EXCEEDS" message), and against
the real credless C6 flake build: 1,003,824 B → warns at 4.26 %, and
`MIN_MARGIN_PCT=5` on the same image exits 1.

**Where the C6's bytes actually go.** Profiling both RISC-V boards with
`tools/size-report.py` killed the standing guess. The 91,616-byte gap
between the C3 (912,208 B) and the C6 (1,003,824 B) is almost entirely
Espressif's radio blob — ~51 KB of blob symbols plus ~11 KB of
`.rodata.wifi`, with `.rwtext.wifi` going 33,768 → 55,060 B. Our Rust is
chip-independent to within a rounding error: `luxel-core` is byte-identical
at 76,628 B on both chips and `picoserve` at 32,948 B. So there is no
C6-specific diet to write; every win is fleet-wide, and the only C6-only
lever is dropping a feature from that board's profile. The candidate list
(picoserve's 22 `IntoResponse::write_to` monomorphizations at 20,478 B,
the 24,434 B routing table, core::fmt at 23,550 B, MQTT at 29,383 B) is on
Gitea #160; nothing was optimized in this pass on purpose.

## 2026-08-30 — v0.1.29 protocol-re-init checklist closed out on the Athom (#155)

The two never-verified items of the v0.1.29 hardware checklist
(docs/webui.md) ran on the Athom rig (v0.1.39, ota_0, serial captured
throughout — zero panic/reset lines):

- **Item 2, encode-buffer realloc at 2048 px under heap pressure**: with
  the heaviest allowed pattern loaded (~24 KB of arrays, heap_free
  ~34 KB), 3× sk9822↔ws2812 round-trips — every switch clean, the ~10 KB
  encode-buffer delta visible in heap_free each way, no reboot, no slot
  rollback. An over-budget 6-array variant (vmerr "array memory budget
  exceeded") switched just as cleanly. Notably the "output paused"
  fallback path is unreachable via patterns alone — the VM array budget
  keeps enough headroom that the realloc always succeeds — so that branch
  stays QEMU/review-verified only.
- **Item 4, switch under live traffic**: 6 protocol switches under a
  ~60 fps DDP stream (`live:"ddp"` held throughout) and 6 more
  mid-crossfade (4 s blend, playlist auto-advancing) — all clean, no
  vmerr, no reboot. Visual no-tearing confirmation remains the one
  eyes-on residue (tracked in #155).

Items 1/3/5 turned out to have been hardware-verified back on 2026-07-19
(v0.1.30 session) with the checklist never updated — docs/webui.md now
records per-item status. Device restored to as-found (60 px ws2812,
brightness 4). Also fixed two stale README claims: "195-pattern gallery"
(now 322) and "ESP32/ESP32-C3" (release images now cover ESP32, C3, C6,
S3, and the HUB75-panel boards).

## 2026-08-30 — A device with no web UI now links to one (#11, first slice)

A device whose assets partition is empty served a dead end: a dark page
saying "the web UI isn't installed" and a `tools/deploy.sh` line, which
assumes you have the repo, a checkout, and `nix develop`. Everything
needed to do better was already in place and simply never wired up — the
playground has honoured `?device=<base>` since device mode existed, the
firmware answers every route with `Access-Control-Allow-Origin: *`
precisely so a foreign origin can drive it, and CI has published the web
dist to `https://googlebot42.github.io/luxel/` since 2026-08-15. The
embedded page now closes the loop: one anchor, its `?device=` filled in
client-side from `location.host`, so the device tells the hosted UI its
own address. `tools/deploy.sh` stays as the second paragraph.

**Cost, because this lives in the app image.** `firmware/src/index.html`
is `include_str!`'d into the binary (`server.rs`, and the native mirror
in `luxel-cli/src/serve.rs` embeds the same file), so every byte is
1 MiB-OTA-slot budget — see #160. Source +275 B; measured `.text` on
riscv32imc +272 B, `.data`/`.bss` unchanged. The whole feature is one
`<a>` and a 44-character `<script>`; no framework, no fetch, nothing to
go stale.

**The part that isn't done, and why it's a separate ticket (#162).** The
Pages copy is https and devices are http, so the playground's calls to
the device are mixed-content / Local-Network-Access requests.
`web/src/flash/lib/device.ts` already handles exactly this for the WLED
installer — it passes `targetAddressSpace: "local"` when the page is
https and the target is genuinely local-space — but the playground's
`gatedFetch` has no equivalent, so it rides on Chromium's auto-detection
alone and has no browser-blocked message when that fails. Filed as #162
with the measured constraints from docs/wled-migration.md; the link is
unambiguously right on a plain-http host today, and #11 stays open for
the device-side build mode that omits the assets entirely.

Verified: `cargo build --release` (riscv32imc) before/after for the size
delta, `tools/serve-e2e.mjs` green (`/` fallback + `/min` routing), and
the page rendered in real chromium against a stand-in host — the anchor
resolves to `https://googlebot42.github.io/luxel/?device=http://<host>`.

## 2026-08-30 — The installer stops claiming only two chips are built (#57)

The installer page told anyone with an unsupported chip that "only classic
ESP32 and ESP32-C3 are built today". That stopped being true on 2026-08-22:
the release workflow builds eight board variants across ESP32, C3, S3 and
C6. The images exist; what does not exist is any hardware to test the new
ones on (#56), so they ship as artifact downloads and the takeover flow
deliberately doesn't offer them. The copy now says that instead — the
takeover covers the chips it covers, other ESP32 chips have release images
that are untested on real hardware and flashed by hand. Same for the
footer, which made the same "works on ESP32 and ESP32-C3" claim about
Luxel as a whole rather than about the takeover.

The durable part is where the chip list lives. It was written out three
times — the `archUnsupported` predicate, the error text, the footer — so
adding a chip meant finding all three. `releases.ts` now owns one
`FLASHABLE_CHIPS` record; `isFlashableChip()` and the user-facing
`FLASHABLE_CHIP_NAMES` string both come from it, and the `BOARDS` comment
says out loud that it is a *subset* of what the pipeline builds rather than
a mirror of it. Neither the error text nor the footer enumerates the
release variants, so the release workflow can grow boards without dating
this page again.

`flash-e2e.mjs` grew scenario 3b for the case the old copy got wrong: a
WLED device reporting `esp32-s3` hits the stop with the release-download
wording and gets no flash button. The pre-existing esp8266 stop (a
genuinely never-supported chip, not a not-yet one) is untouched. Verified
in real chromium: 15/15 flash-e2e checks green, `svelte-check` clean.

No decision was made about whether S3/C6 belong in the board list — that is
still #57, still gated on #56.

## 2026-08-30 — The installation's palette is a device setting now (#139)

`setOutputPalette(pal, amount)` shipped with the global post-process chain,
but only as a *pattern-side* builtin: the device-settings half landed for
brightness curve, blur and glow and stopped there. The reason was storage,
not plumbing — those three are one byte each in the fixed-size `LXDV`
record, and a palette is a variable-length stop list. It now has its own
storage, its own routes, and an editor in the Output card.

**Where a variable-length setting actually goes.** The obvious home was a
fifth nvs record, and that is what the first cut did — until the Athom rig
answered a power cycle with an empty palette. The nvs partition is four
4 KiB sectors and all four are taken: WiFi, device settings, MQTT, and —
this was the collision — `ota::GUARD_OFFSET` at 0xC000, the boot-loop
guard, which rewrites (and therefore erases) its sector on *every boot*.
A record parked there is gone before anything can read it. The fix is the
mechanism the device map, playlist and resume record already use: a
reserved-key blob in the pattern store (`patterns::store_blob`,
`PALETTE_KEY`), which is variable-length by construction and CRC-checked by
sequential-storage. `config.rs` now carries a header comment saying the nvs
partition is full and where the next setting goes, so nobody rediscovers
0xC000 the same way.

The blob is `u8 version=1  u8 amount_pct  u8 count` then `count` stops of
`(pos, r, g, b)`. `outpal::deserialize` validates the count against the
cap before reserving — the rule a torn pattern-store TOC taught this
codebase the hard way — and rejects an unsorted stop list as corrupt rather
than sampling nonsense from it. Clearing writes a zero-count record, the
same way the device map clears itself.

Adding a fourth persisted setting was still the moment to stop
copy-pasting the nvs writer: `config::write_record(offset, rec)` now does
the pad-to-word, word-aligned staging, erase and write for all three nvs
records. That deleted three duplicates of the `unsafe` staging block and
paid back about 600 B of image on the tightest board.

**Composition, not override.** A device palette does not replace a
pattern's. `apply_outpipe` runs the device stage on the frame the engine
already finished — which may already have been recolored by the pattern's
own `setOutputPalette` — exactly the way device blur stacks on top of
pattern blur. The device setting is the installation's look; the pattern
keeps its own voice. Stage order inside `apply_outpipe` mirrors the
engine's chain: recolor, then spread light, then the output transfer.

The 256-entry lookup is cooked off the hot path. `shared::set_post_palette`
bumps an epoch inside the same critical section that swaps the stop list,
and the render task caches `(cooked-for epoch, Box<[[u8;3];256]>)` beside
its gamma LUT — an unchanged palette costs one atomic compare per frame,
and the cache updates its epoch even when the result is "no palette" so an
empty list can't re-cook forever. (The epoch bump is a load/store under the
lock, not `fetch_add`: the C3's riscv32imc has no atomic read-modify-write,
which the build found before hardware did.)

`outpipe::fill_palette_lut` and `outpipe::parse_palette_stops` are the
shared halves — the engine's `ensure_remap_lut` and the firmware's cache
cook through the same function, and the firmware, the native mirror and the
tests all parse the wire form through the same parser, with
`MAX_OUTPUT_PALETTE_STOPS` defined once.

API: `POST /api/output/palette` takes `"<amount_pct> <pos> <r> <g> <b> …"`,
all 0..=255, positions ascending, at most 32 stops; `DELETE` clears it;
`GET /api/output` echoes `palette` (the flat array) and `paletteAmount`
alongside the existing fields. Boot reads the blob before WiFi — a few
hundred bytes, unlike the multi-KB pattern/playlist resume that has to wait
for `wait_config_up()`.

Verified on the Athom rig (60 px WS2812): set a three-stop palette, power
cycle, palette still there and applied at boot; `DELETE`, power cycle,
still gone. That the stage really reaches the wire — `/api/pixels` shows
the pre-outpipe frame, so it can't answer this — was measured at the wall
plug at brightness 31: 11.4 W with no palette, **4.5 W** with an all-black
palette, 11.4 W with an all-white one, 11.4 W again after `DELETE`.

UI: the Output card grows a Palette editor — a gradient preview that models
the engine's asymmetric ends (below the first stop clamps, past the last is
black), one row per stop with a color swatch and a 0–255 position, and
add/remove/clear plus the blend amount. Covered end-to-end in
`device-e2e.mjs` against the native mirror.

Size: +5,120 B on `board-c6-devkit` (1,004,704 B, 43,872 B of slot left)
and +4,256 B on `board-pixelblaze-v3`; `.stack` 29,228 → 29,196 B.

## 2026-08-30 — The post-process blur follows the panel, not the wire (#140)

`setBlur`/`setGlow` — and the `/api/output` blur/glow device settings —
worked along the pixel index. On a strip that is physical order and the
result is right; on a matrix it followed the wiring, so a serpentine panel
got a horizontal smear that folded back at every row end and never spread
vertically at all. The chain now recognizes a grid and sweeps it in two
dimensions.

**The grid is six bytes, not a neighbour table.** `outpipe::detect_grid`
runs once per `Engine::set_map` and answers one question: is this map a
regular matrix walked row by row? It reads the coordinates as contiguous
runs — equal length, same fast-axis values forwards or backwards, both axes
strictly monotonic — and returns `GridMap { w, h, serpentine }`. Cell →
pixel index is then arithmetic (`row * w + col`, mirrored on odd rows when
serpentine), so the per-frame cost of map awareness is *zero* allocation,
zero cached indices, and one branch per lookup. That mattered more than
generality: a 4096-pixel neighbour table would have been 8–16 KB of ESP32
heap held forever for a stage that is off by default.

Two wirings cover the space because mirroring is free: the kernels are
symmetric and clamp at the edges, so a panel wired entirely backwards, or
one whose rows run right-to-left first, describes the same neighbourhoods
as its mirror. Column-wired panels fall out as the transpose. Anything
else — a ring, a scatter, a ragged last row, a 3D map — returns `None` and
keeps the exact index-space behaviour it had, which the tests pin.

`blur_frame_grid`/`glow_frame_grid` are separable: one sweep along the
rows, one down the columns, sharing a single inner loop between the two
axes (an `along_rows` flag rather than a closure per axis — worth ~370 B of
image on the tightest board). One `passes` is one row sweep plus one column
sweep, so the 2D kernel is the 1D one squared, and glow's corner cells pick
up `g²/256` — a naturally round falloff.

Firmware: `apply_outpipe` takes the engine's `GridMap` (`Engine::grid()`,
`Copy`, read before the frame borrow) and uses the grid kernels when it
matches the frame length. The live DDP/E1.31 path gets it too whenever a
pattern engine is loaded.

Measured, not estimated: app image +2,272 B on `board-c6-devkit` (997,328 →
999,600 B, the fleet's tightest margin, still 48,976 B free) and +2,208 B on
`board-pixelblaze-v3`; `.stack` 29,244 → 29,228 B, no function frame near
the 12 KB budget. 206 core tests green including eight new ones — detection
across four wirings and seven rejections, no row-end seam, symmetric spread
on serpentine, and the index-space fallback byte-for-byte unchanged.

Docs: docs/lang.md's "Index space, not map space" note is now "map space
when the map is a grid", with the fallbacks spelled out; ideas.md, webui.md
and the editor's builtin help follow. **The visual check needs the 64×64
HUB75 panel, which hasn't arrived** — tracked on #75 and docs/UNTESTED.md.

## 2026-08-30 — Language gap-fill: `switch`, `**=`, and the ternary /
## compound-member audit (docs/ideas.md "Language" batch)

Three queued language items closed in one pass. Two of them turned out to
be documentation-and-tests work, not fixes.

**Ternary chains — already correct, now pinned and documented.** `?:`
parses right-associatively (`a ? 1 : b ? 2 : c ? 3 : 4` is a run of
else-ifs), a ternary in the *then* slot is closed by its own `:`, each
branch is a full assignment expression, and only the taken branch is
evaluated. All of that already worked; nothing was broken. The gap was
that docs/lang.md mentioned `?:` in a single operator-list clause. It now
has a **The conditional operator** section covering associativity, the
then-slot nesting rule, the parentheses requirement in the condition
slot, and single-branch evaluation — with tests for each claim
(`ternary_chains_are_right_associative` in tests/semantics.rs, plus an
AST-shape test in parse.rs).

**Compound member ops — one real gap, `**=`.** Audited the whole family
against array elements: `+= -= *= /= %= <<= >>= &= |= ^=` and
prefix/postfix `++`/`--` all worked, with correct JS result values
(`a[i]++` yields the old value), correct copy-on-write on const-pooled
literal arrays, and — the thing worth checking — the array and index
sub-expressions evaluated exactly **once** (`a[idx()] += 3` calls `idx`
once; the codegen already used `Dup2`/`LoadIdx`). The one missing member
of the family was `**=`, which the `**` extension never got: it lexed as
`**` followed by a stray `=` and died with "expected an expression". Now
a lexer token + one match arm, working on variables and elements alike.

**`switch` — implemented, no new opcodes.** JS semantics: the
discriminant is evaluated once, labels are compared with the language's
`==` in source order (only up to the match), bodies fall through until a
`break`, and `default` may sit anywhere — including in the middle, where
it is still the no-match target *and* still falls through into the arm
below it. `break` binds to the innermost switch-or-loop; `continue`
skips past a switch to the enclosing loop. It lowers onto instructions
that already existed:

```
<disc>
Dup; <label_i>; Ne; JmpIfFalse T_i     one per `case`, in order
Pop; Jmp default|end                   nothing matched
T_i: Pop; Jmp body_i                   trampoline drops the discriminant
body_0 … body_n                        source order ⇒ fall-through is free
```

Every path pops the discriminant *before* entering a body, so arms run on
an ordinary statement-context stack and `break`/`return` out of them need
no unwinding. `BUILTINS` untouched, LXBC format version untouched;
docs/spec/vm.md gains a note on the lowering. Compiler-side, the old
`LoopFrame` became `BreakFrame` with an `is_loop` flag — that flag is
what routes `continue` past a switch to the loop that owns it.

`switch`/`case`/`default` become reserved words. Nothing in `library/`
used them as identifiers, and they are reserved on PB too (its compiler
is JS-based), so PB-source compatibility is unaffected.

**PB compatibility, probed compile-only** (`tools/oracle/compiler.mjs`
runs PB's own compiler locally — no websocket, so no oracle-wedge risk):
PB **rejects** `switch` ("Unsupported type SwitchStatement") and `**`, so
both are documented as Luxel extensions. PB's ternary and `arr[i] +=`
accept the same shapes we do.

**Verification.** 7 new test functions and ~90 new assertions across `parse.rs`,
`lex.rs` and `tests/semantics.rs` (fall-through, default-in-the-middle,
nested switch, switch-in-a-loop with `break` vs `continue`, `return` out
of an arm, `var` hoisting out of an arm, per-pixel dispatch in `render`,
single evaluation of the discriminant and of the labels past the match).
`cargo test --workspace` green; `tools/check-library.sh` 323/323 on both
grids; `tools/wasm-smoke.mjs` native↔wasm identical; firmware builds
(no_std clean); `web/tools/e2e.mjs` green; and a real-chromium check that
a `switch` pattern compiles and renders its 1-bright/2-dim dispatch in the
playground, with `switch` in the editor's autocomplete.
## 2026-08-30 — `orig-unrenderable` reaches zero: the last one was never
## broken, it was non-visual (#123), plus a tracker sweep

**`performance-test-framework` is excluded, not scored (Gitea #123).** The
original is a CPU benchmark whose render body is the comment
`//sorry, no blinkenlights!` — it reports through PB's Vars Watch, and
all-black is its correct output on real hardware too. The 2026-08 sweep
filed it `orig-unrenderable` after ten runs (every rig, fps, seed, wall
clock and skip) proving the original is black: right observation, wrong
bucket. The original does not *fail* on our engine; it succeeds at drawing
nothing.

The mechanism is a fourth fixup kind, `nonVisual`, a one-string reason in
`tools/verify/fixups.json`:

- `fixups.mjs` gains `nonVisualReason(slug)` — a malformed marker throws
  rather than silently putting the pair back in the scored population.
- `snap.mjs` warns on **every** run of such a slug and records the reason in
  `meta.json`'s `provenance.fixups`, so the next judge stops instead of
  spending a batch re-proving the original is black.
- `report.mjs` and the review UI file it under its own `non-visual`
  heading/badge with **no score** — "0/10" on an excluded pair reads as a
  failing port.
- `JUDGE.md` documents `non-visual` as a manifest decision, not a judgement
  a judge makes on its own, and distinguishes it from the
  degenerate-constant-output subtype ("we cannot get output out of it" vs
  "it correctly has no output, on real PB too").

The annotation lives in `fixups.json` and **not** `pairs.json` because
`gen-pairs.mjs` regenerates `pairs.json` from `library/` + `corpus/` — it
would silently drop either an annotation or a deletion. The port's
phase/progress readout stays: the playground has no Vars Watch and a
permanently black gallery tile reads as broken, so it is now a documented
deliberate deviation rather than an unscored defect.

**Sweep headline numbers refreshed** in `tools/verify/FINDINGS.md`, which
still carried the original 2026-08 table long after the re-judges moved
things. Recounted from `results/*.json` (293 files): match 24, close
**122** (was 119), divergent **128** (was 123), broken **18** (was 17),
orig-unrenderable **0** (was 10), non-visual 1; mean 5.38/10 over the 292
scored pairs. `orig-unrenderable` is empty for the first time — the sweep's
ten were all diagnosed and re-judged (#99 sentinel-strip, #106 wrap freeze,
#108 silent-nulls, #109 array budget, #122 var-driven `automap`) and this
was the last. The table now carries the one-liner that recounts it, so the
next reader doesn't have to trust a stale number.

Verified in real chromium against the live review UI: the `non-visual`
filter chip appears, the pair badges as `non-visual` with no score, its
card shows the exclusion summary, original renders black and port renders
its bar. `snap.mjs` re-run confirms the warning and the provenance stamp.

**Tracker hygiene** — five stale issues closed against evidence, two new
ones filed for the pieces that were actually still open:

- **#4 "Improve UI"** — the docs/webui.md redesign backlog is done; the
  live backlog moved to numbered issues long ago. The one genuinely open
  thing in that document was its "v0.1.29 hardware-verification checklist",
  untracked anywhere → filed as **#155**.
- **#3 "Board presets"** — shipped: seven board features, the
  "Adding a board (a five-minute diff)" recipe, the installer board picker,
  and WLED cfg.json import on takeover (PR #76). The remaining piece, a
  runtime LED data-pin picker (pins are esp-hal *types*, so today the pin
  is fixed by the board build and takeover only *logs* WLED's), → **#154**.
  Mic enablement was already #119, the untested S3/C6 boards #56/#57.
- **#5 "less space"** — docs/boards.md already records the decision: A/B
  OTA alone is 2 MB and `storage` became load-bearing in v0.1.34, so 2 MB
  variants are out. Dropping the webui wouldn't help anyway: assets live in
  their own partition, not in the app image.
- **#143 "16 MB partitions"** — none of its own three revisit triggers has
  fired (shipped asset bundle ~615-641 KB of 983,040 B; `storage` manages
  512 KiB of its 1 MB and caps at 24 patterns; no bulk-storage consumer
  exists). Closed won't-do, as its text says is legitimate.
- **#147 "6-arg arrayReplace hangs the oracle"** — oracle-only research
  with no Luxel correctness gap, and probing further costs a hang.
  Closed answered/wontfix; `tools/oracle/oob-probes.mjs`'s Q8 preamble
  refreshed, since it still described the pre-#107 engine behaviour that
  783978f settled the other way.

## 2026-08-30 — `//#` control hints now bind from the line above the export
## too (#146), which is how 112 of 130 library patterns write them

`parseControlHints` (`web/src/lib/hints.ts`) only ever matched a **trailing**
`//#` directive on the export line — its `[^\n]*?` can't cross a newline. Most
of `library/` puts the directive on its own line above the export, so those
controls silently fell back to the default 0..1 / step 0.001 / value 0.5
slider instead of the bounds they declare. Nothing errored; the patterns just
came up wrong.

**Decision: widen the parser, don't normalize the library.** The own-line form
is the one pattern authors actually reach for (it reads better above a long
function body), `docs/lang.md` already told them to use it, and the review UI's
own copy of the parser (`tools/verify/review/engine.js`) had accepted both
placements since it shipped. So the fix is two regexes — trailing, plus
`^[ \t]*//#…\n[ \t]*export function …` — merged per control name, own-line
winning on shared keys. No `library/*.js` was touched.

- **Docs**: `docs/lang.md` gains a *Control bounds* section under Luxel
  extensions spelling out both placements, the four keys, that a blank line
  between directive and export breaks the association, and that `default` is a
  UI starting position, not a variable initializer. The frame-model bullet and
  the PB-divergence bullet now link to it instead of describing one placement.
- **Tests**: `web/` had no test runner; it does now — `npm test` runs node's
  built-in runner with type stripping (`--experimental-strip-types`), so
  `web/tests/hints.test.mjs` imports `hints.ts` directly with zero new
  dependencies. 11 cases: both placements, indentation, negatives/fractions,
  multi-line bodies not stealing the next control's directive, merge
  precedence, unknown keys, blank-line separation, non-export functions.
- **Verified in real chromium** (not just `svelte-check`): `library/eye-of-sauron.js`,
  `1d-aurora-borealis.js`, and `2d-fireworks-fade.js` opened in the playground.
  Before, all twelve sliders read min 0 / max 1 / step 0.001 / value 0.5; after,
  each shows its declared bounds (e.g. Eye of Sauron's AngularDensity 2..18
  step 1 at 8, Dilation 0.15..0.6 step 0.01 at 0.35). `tools/e2e.mjs` all-green.

The three consumers (`App.svelte` ×3, `PlaylistRow.svelte`) call the same
function and needed no change.

## 2026-08-30 — automap port fix: exported var renamed to `pixel`, invented idle scan removed (#136)

Two non-visual defects from the #122 re-judge of `library/automap.js`
(clean-room port of the community mapping helper), both fixed with minimal
edits to our own file:

1. **The port renamed the interface.** It exported `pixelIndex` where the
   original exports `pixel`. For a pattern whose entire purpose is to be
   driven over the vars API by a mapper/companion client, the NAME *is* the
   interface — a client that writes `pixel` drove the original and silently
   failed to drive the port, leaving it in its scan state. The export (and
   its only reference, in `render`) is now `pixel`.
2. **The port invented an idle self-scan.** At a negative index it swept one
   red pixel along the strip every ~3.3 s; the original is completely inert
   when undriven (measured with `--no-vars`: 1200/1200 zero-motion frames,
   still black at t=320 s). The sweep and its `target` indirection are gone —
   `render` is now the single line `hsv(0, 1, index == pixel)`, so an
   unset/negative/out-of-range index renders black, as the original does.

Acceptance checks from the issue, run in-worktree with `tools/verify/snap.mjs`:

| check | result |
|---|---|
| (a) `sides.*.varsExported` reads the same name on both sides | both `["pixel"]`; `varsApplied` `{pixel: 30}` on both, no warnings |
| (b) `--no-vars` posts meanBrightness 0 and zero lit pixels on BOTH sides | 60 s @ 5 fps: mean/R/G/B/motion all max 0, `zeroMotionFrames` 300/300 both sides; `--dump` at t = 0.6/20/40/59 s → 0 lit of 60 px on both |

No visual cost: a driven `--dump` (pin `pixel=30`) is still byte-identical
between the sides at t = 0.5/2/3.9 s — one `[255,0,0]` pixel at index 30,
everything else black.

Also updated: `tools/verify/fixups.json` pins the port side to `pixel` (the
per-side form stays, since pairs may still name a var differently), and the
matching sentence in `docs/tools.md`. The judge verdict in
`tools/verify/results/automap.json` is left as the historical record.

## 2026-08-29 — Out-of-range writes: the oracle says PB does NOT tolerate
## them either (#107 closed), and the splat builtins were the real gap

The verify sweep's engine gap 3 held that "real Pixel Blaze tolerates
out-of-range array/pixel writes where Luxel hard-errors", blocking a
re-judge of five pairs. **The premise is false.** `tools/oracle/oob-probes.mjs`
grew a second battery (Q1–Q8, fw 3.67) that asks the question shape by
shape instead of inferring it from patterns that "visibly work":

| probe | source (`a = array(3)`) | PB |
|---|---|---|
| Q1 / Q2 | `a[5] = 1` / `a[-1] = 1` | **aborts** |
| Q3a / Q3b | `v = a[5]` / `v = a[-1]` | **aborts** |
| Q4a | `v = a[1.5]` | tolerated, truncates |
| Q4b | `v = a[3.5]` | **aborts** — truncate first, then bounds-check |
| Q4c/d/e/f | fractional write: variable index, literal index, array literal, init scope | tolerated, truncates — **all four** |
| Q5 | `array(4)`, `a[6] = 1`, read every slot back | untouched: no clamp, no wrap, no partial write |
| Q6 | out-of-range write every 3rd invocation | frames 21 → 138 in 1.5 s — the pattern survives |
| Q7 | `t = array(4); t[0](1, 2)` | **aborts** — calling an unassigned slot is not a no-op |

So PB errors on exactly what Luxel errors on. What made `nano-orbital` and
friends look tolerant on a device is the error's narrow blast radius — one
handler invocation, not the pattern — which #84/PR #88 already matched.
`rainbow-comet`'s one-shot frame-982 error and `tixy` walking off its
formula table after ~46 modes happen on real hardware too; both are
original-side faults, which is how the judges had already scored them, and
the three `fixups.json` rig pins are correct rig data, not workarounds.
**No fixup removed, no verdict changed** (rainbow-comet 4, tixy 4,
nano-orbital 2, orv-christmas-tree 5, rainbow-smiley 6 — all re-rendered,
stats unchanged).

Two things did come out of it:

- **A real divergence in the sibling splat path.**
  `arrayReplace`/`arrayReplaceAt` were silently dropping every element that
  fell outside the array and clamping a negative offset to slot 0 — the
  opposite of what `a[i] = v` does. The oracle validates the span up front:
  `offset + count > length` is a runtime error leaving the array
  **completely** untouched (not even the in-bounds prefix lands, Q8f),
  `offset + count == length` is the accepted boundary, and a negative
  offset shifts rather than clamps. `vm.rs` now does exactly that, pinned
  by `array_replace_span_is_bounds_checked` and
  `rejected_replace_span_leaves_the_array_untouched`. No library or corpus
  pattern uses either builtin, so nothing rendered changed.
- **A VM panic found on the way in.** `arrayReplaceAt(b)` — the offset form
  with nothing to splat — indexed `args[2..1]`, an inverted slice range, so
  ordinary pattern source could panic the VM (on device: a reboot). It is
  now the PB-shaped no-op, with the three too-few-args spellings pinned.
- **A stale "deliberate divergence" retired.** A 2026-07 note claimed PB
  aborts on a literal-index fractional write (`a[1.5] = 9`) and that
  Luxel's uniform truncation diverged on purpose. Q4d/Q4e/Q4f say
  otherwise in every form — it is an exact match. `vm.rs`, `docs/spec/vm.md`
  and `docs/research/04-oracle-findings.md` corrected.

Also filed: **#147**, two 6-argument `arrayReplace*` shapes that
reproducibly HANG the oracle (websocket stops acking, device off WiFi for
~a minute, recovers on its own — and explains two mystery dropouts during
this session). Four-arg versions of both error cleanly, so the bounds rule
was settled without them; `oob-probes.mjs`'s header names both and says not
to add them back.

Gates: `cargo test --workspace` green, `tools/check-library.sh` 322/322 on
both grids, `tools/wasm-smoke.mjs` native ↔ wasm bit-identical.

## 2026-08-29 — The global post-process chain is finished: `setOutputPalette`
## → `setBlur` → `setGlow` → `setGamma`, plus a device brightness curve

`setGamma` shipped as a lone stage months ago and the docs/ideas.md entry
has read "STARTED" ever since. The rest of it is in, as an actual *chain*:
whole-frame stages the engine runs **once per frame** after the last
`render()`, in one fixed order, rather than a bag of per-pixel tweaks.

Order is the point. Recolor first (the remap works on the pattern's own
luma), then spread light spatially, then apply the output transfer curve
last — the order a display pipeline uses. That meant moving `setGamma`
out of the per-pixel quantize step, where it was, and onto the end of the
chain; otherwise a remapped pixel would have skipped gamma entirely and a
blur would have averaged already-curved values.

- **`setBlur(amount, passes = 1)`** — 3-tap blur along the pixel index.
  `amount` is each neighbour's share (0.5 = the classic 1-2-1 kernel, 1 =
  a pure neighbour average), `passes` 1–8 widens the radius. Ends clamp,
  so light that reaches the last pixel stays on the strip. Allocation-free
  — an in-place pass only has to remember one pixel (the previous
  pre-blur value), which is what keeps it honest on an ESP32.
- **`setGlow(amount)`** — light-bleed bloom: each pixel takes the brighter
  of itself and `amount` of its brightest neighbour. Unlike blur the
  source keeps its full value, so highlights spread without the frame
  going dim. Also allocation-free, one pass.
- **`setOutputPalette(pal, amount = 1)`** — recolor the finished frame by
  luma through a `setPalette`-format stop list: the pattern's structure
  survives, its hues are replaced. This is the "put the installation's
  palette over any pattern" knob. The 256-entry table is cooked on
  install (tracked by a VM epoch counter) so the per-pixel cost is a
  3-multiply luma plus a lookup, and the stops are a snapshot — unlike
  `setPalette`, which PB keeps live against the source array.
- Every stage is **off by default and costs one comparison per frame when
  unset**. An untouched pattern renders byte-identically to before.

Settings-page half (the ideas entry asked for one), device config record
**v6 → v7**, three new bytes in the reserved pad area of the `LXDV`
record, all on `/api/output` and in Settings → Output:

- **brightness curve** (`brightCurve`, gamma×10) — deliberately *not* the
  same knob as gamma. Gamma shapes every pixel's channels (content); the
  brightness curve shapes only how the master dimmer responds (control),
  which is the actual fix for "everything above 20% looks the same". A
  non-zero brightness never curves to 0 — a lit strip stays lit.
- **blur %** and **glow %** — the same two spatial stages as an
  installation setting, applied by the firmware after the engine's chain
  so you can dial a soft look in without editing patterns.
- The POST body's three new tokens are **optional** — an older client
  still sending `<order> <gamma> <cap>` keeps the stored values, so no
  flag day. v6 records still read (the version table gained an explicit
  `ver == 6` arm; `ver == DEV_VER` alone would silently have stopped
  accepting them).

Index space, not map space: the spatial stages follow the pixel index, so
on a serpentine matrix they follow the wiring path. That's documented,
and the two follow-ups are ticketed rather than left as prose — palette
remap as a *device* setting needs real palette storage in flash (Gitea
#139) and a map-aware 2D pass needs a neighbour index or a grid fast path
(Gitea #140).

`library/post-process-chain.js` is a demo whose pattern deliberately draws
nothing but hard single-pixel sparks on black — everything soft in it is
the chain, and each slider switches one stage off so you can see what it
was doing. Driving it in real chromium turned up a separate bug worth
knowing about: `//#` control hints only bind as a **trailing** comment on
the export line, so the several library patterns that put them on the
line above have been silently running with default 0..1 sliders all along
(Gitea #146).

Sizes were re-measured on all eight board combos (docs/boards.md): the
chain costs an even **+2.8–3.6 KB everywhere**, and the C6 still owns the
tightest margin at 51,232 B (4.9 %). Worth noting how that number was
nearly wrong — measuring against the table as it stood *before* the same
day's HUB75 board work made the chain look like it cost 10 KB on RISC-V
and 5 KB on Xtensa. It doesn't; the baseline had simply moved under us.
Re-measure after the rebase, not before. `tools/stack-check.sh`
is unmoved (no frame over budget, `.stack` 29,244 B), and
`web/tools/device-e2e.mjs` grew eleven output-pipeline checks covering the
six-key GET, the optional-token back-compat, out-of-range rejection, and
the three new Settings fields.

## 2026-08-29 — HUB75 panel boards: `board-seengreat-hub75` + per-board
## MAX_PIXELS (whole 64x64 panel finally addressable) (#73, #74)

Code-side prep so the Seengreat RGB Matrix HUB75 S3 and its 64x64 panel
can be brought up the day they arrive (#75). **No hardware was touched —
every number below is a build/link measurement, not a panel.**

- **`board-seengreat-hub75`** joins the board features. Real pin map,
  transcribed by signal name from the [vendor wiki](https://seengreat.com/wiki/214/):
  R1 IO5 · G1 IO4 · B1 IO6 · R2 IO15 · G2 IO7 · B2 IO17 · A IO8 · B IO18 ·
  C IO10 · D IO9 · **E IO16** (64 rows need it) · CLK IO12 · LAT IO11 ·
  OE IO13. Both panel outputs (ribbon connector and plug-in header) share
  those pins. The vendor's table is *not* in R1,G1,B1,R2,G2,B2 pin order —
  transcribe by name or you get a colour-swapped panel.
- **The HUB75 pin map moved out of main.rs into `board::hub75_pins!`**
  (#73's scope note). Pins are esp-hal types, so it's a macro rather than
  a const table, but it lives beside the `def` blocks: a second panel
  board is now a def-block + macro-arm diff, and main.rs keeps exactly one
  wiring line for all panel boards. Being a panel board, the feature
  enables `hub75` itself (`["esp32s3", "hub75"]`) instead of needing it
  passed at build time.
- **`MAX_PIXELS` is per board** (#74), moved from `shared` to `board.rs`
  and re-exported: **4096** with `hub75`, **2048** everywhere else. Before
  this, a 64x64 panel was clamped to half its area and the bottom 32 rows
  rendered black. It is deliberately not a global raise: at 4096 px the
  classic ESP32's WS2812 encode buffer alone would be ~36 KB against an
  80 KB heap. The panel path never builds that buffer at all — the driver
  owns two ~28 KB bitplane framebuffers allocated once at boot — so the
  extra pixels only cost the ~12 KB-each per-frame buffers. A `const`
  assertion now fails the build if a panel's area ever exceeds its board's
  cap, so the half-dark panel cannot come back silently.
- **The cap reaches the editor.** `/api/status` (firmware *and* the native
  mirror) now carries `max_pixels`, and the playground takes its pixel
  clamp from there on every poll, falling back to `/api/config`'s `max`
  for older firmware. Status wins over config when both answer — the
  precedence matters, and getting it backwards is exactly what the first
  cut did (caught in chromium, not review). New regression check:
  `web/tools/maxpixels-e2e.mjs`, which drives the 4096 path by
  intercepting status to impersonate a panel board — the only way to
  exercise it without the panel.
- **Partition decision (#73): the standard 4 MB table stays**, even though
  the module is 16 MB. OTA slots are 1 MiB-capped either way, storage is
  nowhere near full, and the assets partition holds a 641 KB bundle in
  983,040 B. A second table would have to be threaded through
  build-esp32.sh, flake.nix, the release workflow, build.rs and
  `takeover.rs` — real complexity for space nothing needs yet. PSRAM stays
  uninitialised for the same reason (DMA buffers must be internal SRAM
  regardless); the array-arena idea is unchanged and still future work.
- **Verification (all eight board/feature combos):** every one builds,
  links its load-bearing features (`tools/image-check.sh`) and fits the
  1 MiB OTA slot — tightest is still the C6 at 994,352 B / 54,224 B
  margin; the new board is 882,976 B / 165,600 B. Existing boards moved
  by 80–550 B, all of it the new status field and codegen noise.
  `tools/stack-check.sh` on all eight: no frame over the 12 KB budget,
  `.stack` from 29,324 B (classic ESP32) to 141,256 B (C6), the new board
  at 50,500 B — identical to `board-s3-devkit` + `hub75`, as expected.
  `cargo test --workspace` green; device-mode e2e green. Sizes and stack
  figures refreshed in docs/boards.md.

Still open, and deliberately so: **FPS at 4096 px, real `heap_free` under
load, and whether the panel's driver IC is even a shift-register type**
all need the hardware and ride on #75.

## 2026-08-29 — Verify harness learns to DRIVE a pattern: `--vars-*` pins
## exported vars, and automap goes orig-unrenderable 0 → close 7 (#122)

The last silent-null holdout from #108 was never an engine gap. `automap`
is a mapping HELPER: an external client writes a pixel index into an
exported var and the pattern lights exactly that pixel. At its default
index nothing lit is the CORRECT render — the harness simply had no way
to write to a pattern, so the judge saw black and scored the pair
`orig-unrenderable` 0.

- **`enginehost.mjs` gained `setVar()`** — wrapping the `lx_set_var` ABI
  entry that already existed (values scale into raw 16.16; only EXPORTED
  globals are settable, exactly as on hardware, so the wrapper returns
  whether the name existed instead of no-oping silently).
- **`snap.mjs` gained `--vars-orig` / `--vars-port`** (per side, because
  the two sides of a pair may NAME the same variable differently — which
  is exactly what automap does) and `--no-vars` (render a pinned pair
  UNDRIVEN; an empty `--vars-*` cannot express "no value"). Values land
  once, after init and after any `--controls-*`, before the first frame —
  the same single write a companion app makes. meta.json now carries
  `varsExported` + `varsApplied` per side, so a judge can see the var
  interface and what actually landed.
- **`fixups.json` gained a per-side `vars` key**, and automap pins both
  sides to mid-strip index 30 declaratively. All three consumers apply it
  (snap.mjs, report.mjs, and the live review UI — verified in real
  chromium: both canvases light pixel 30 red).
- **Re-judged: `orig-unrenderable` 0 → `close` 7** (high confidence, ~50
  render experiments). Driven, the two sides are byte-identical at every
  index tried, on strip / 32×8 grid / 5×5×5 cloud, 12–300 px, across
  seeds, clocks and a 60 s window. Two real port defects fell out that no
  visual diff could ever have shown: the port renames the exported var
  (`pixelIndex` vs `pixel`), so a mapper client written for the original
  silently fails to drive the port, and at a negative index the port runs
  a self-scanning demo the original does not have (original: black).
- JUDGE.md now teaches the surface (a black side that exports vars may be
  DRIVEN, not unrenderable), plus a trap the re-judge hit: whole-rig
  `meanBrightness` rounds to 0 on single-pixel patterns, so `mean 0` is
  not evidence of a black render.

Verified: renders on three untouched pairs are byte-identical (PNG
sha256) to the same runs on the pre-change harness, so the no-vars path
is provably unchanged.

## 2026-08-29 — #132: const→owned COW promotion is budget-checked

`Vm::arr_mut`'s copy-on-write materialization added
`array_cost(len) - CONST_ENTRY_COST` to `array_bytes` with no check, so on
a device-budgeted VM the first write to a `[…]` literal could push the
byte ledger past `array_byte_budget` (bounded by the element budget, so an
overshoot rather than a leak). The delta now goes through a new
`charge_array_bytes` — the byte half of `charge_array`, split out because
re-checking the element budget at the promotion site would demand a
spurious extra header's worth of headroom for an entry that allocates no
new arena slot — and it is checked *before* the copy is reserved. Error
semantics are unchanged from the OOM path already at that site: an
ordinary pattern-level runtime error, not a resource guard, so the PB
blast radius from #84 holds (the handler invocation aborts, the pixel pass
still runs). Two regression tests (`cow_promotion_*` in
`crates/luxel-core/tests/engine.rs`) pin the budget edge and the
within-budget delta; the edge one fails against the old code. docs/spec/vm.md
§1.2 documents the promotion charge. Workspace tests green.

## 2026-08-29 — Small-items batch: MSv3 300 px re-test clean on v0.1.39,
## playlist pre-flight dedup (#125), truthful corpus report, #124 pinned

Four picked from a fetch-work sweep, three landed by parallel worktree
agents (PRs #129/#130/#131), one run live on the Athom:

- **"Music Sequencer - for V3 ONLY" re-test at 300 px** (Athom, v0.1.39):
  the 2026-07-19 soak's one capacity holdout, fixed by v0.1.34's
  flash-resident pattern — reconfirmed on current firmware. Push accepted,
  120 s run, no vmerr, no reboot, min heap_free 71,260 B (matches the
  v0.1.34-era ~70 KB figure); fps swings 20–63 with the pattern's phases
  (some phases sit below the 30 fps SLOW line — capacity fine, some phases
  are just heavy). Rig restored exactly as found (60 px, brightness 4,
  prior pattern; post-restore /api/status byte-identical to the snapshot).
- **#125 → PR #129**: the playlist pre-flight in firmware main.rs now
  calls `budget::array_budget` instead of open-coding the same
  floor/headroom arithmetic. The inline constants matched the helper
  exactly, so this is pure dedup — the two paths can no longer drift.
- **Stale `TODO_BUILTINS` → PR #130**: all 39 hardcoded "not yet
  implemented" names in `tools/corpus/report.mjs` were long since
  implemented (BUILTINS has 138 impls, zero Todo entries), so the corpus
  report's headline gap column was pure fiction. The set is now derived
  from vm.rs at run time (loud failure if the table can't be parsed),
  `tools/corpus/last-report.json` regenerated (326 stale `uses.todo`
  lines dropped), derivation documented in docs/tools.md.
- **#124 → PR #131**: the `array(0)`-in-a-loop unbounded arena growth was
  already fixed as a side effect of #109's ledger alignment (zero-length
  arrays charge `ARRAY_HEADER_UNITS`, capping the arena at 2,559 slots) —
  verified by measurement plus a negative control, then pinned: 4
  regression tests, a compile-time `ARRAY_HEADER_UNITS > 0` assert,
  `Engine::arena_stats()`, and a docs/spec/vm.md §1.2 note. Adjacent
  finding filed as **#132**: `arr_mut`'s const→owned COW promotion adds
  to `array_bytes` without a `charge_array` check (budget overshoot,
  not a leak).

## 2026-08-29 — Re-judge queue cleared: 15 pairs, three judge batches
## (closes the re-judge halves of #99 and the #126 follow-ups)

All pairs unblocked by the engine-gap settlements got fresh output-only
verdicts, run per tools/verify/ORCHESTRATION.md (5 parallel Opus judges
per batch, one commit per batch):

- **Freeze family** — the oracle-confirmed 32.768 s freeze now scores
  AGAINST the ports (none reproduces it): fire-blue divergent/4,
  fire-red divergent/5, spring-colors close/6 (its active phase is
  near-exact). New reference detail: the freeze onset is fps-dependent
  and vanishes entirely at 40 fps on spring-colors.
- **Music-sequencers (#99)** — both sentinel strips work; the originals
  compile and render for the FIRST time, superseding orig-unrenderable:
  both divergent/4. Shared headline defect: the ports are
  x-coordinate-driven where the originals are index-only (vertical
  stripes vs index runs on the default grid); v2's sequencer-grid
  lattice is otherwise bit-exact, v3's macro schedule misses the
  original's 181 s dark phases.
- **Fixed silent-nulls** — both engine fixes confirmed working in
  anger: fast-palette-blending close/6 (setPalette live-alias; port's
  sweep is triangle-wave at 0.48 amplitude vs the original's full-strip
  sine) and slime-mold-palette close/6 (late-bound render2D; port grows
  4x too fast and lacks the original's remap startup animation).
  coral-plasma divergent/2 (port field ~40x too fine spatially, ~30x
  too fast — one shared scale constant suspected).
  skypirate-s-centered-spectrum divergent/4 on its 3x600 fixup rig
  (index-only original with hard-coded 300-px meter centres vs a
  normalized-coordinate port).
- **Fixup-rig pairs** — nano-orbital broken/2 (original: 12 dots at
  exactly 12 px/s; port ~60x slow and accumulates arcs into a frozen
  wash), orv-christmas-tree divergent/5 (byte-identical tree silhouette;
  port's snow is 2x2-snapped, smears instead of drifting, dwells 3x
  long; ornaments confetti vs red/blue garlands).
- **De-orphaned perlin ports** — perlin-fire divergent/4 (up from
  broken/2: crash gone, noise field now correlates 0.69-0.96 with the
  original; stays cold, one Mode band pure black),
  coronal-mass-ejection divergent/4 (improved but rings-vs-rays
  topology stands), eye-of-sauron broken/1 and
  distance-function-kaleidoscope-2 broken/0 (both ports still
  near-black — full fix-pass targets with precise numeric targets in
  their verdicts).

Cross-cutting finding, recorded in ORCHESTRATION.md: sweep-era verdicts
predating the perlin refit can describe a STALE original render (the
refit changed how perlin-using ORIGINALS draw). The 2026-08-29 verdicts
supersede those references; other perlin-heavy pairs deserve a
re-render before their old verdicts are trusted in the #101 fix pass.
JUDGE.md gained nine trap notes from judge friction (beat-aliasing
onsets, sensors-off at ≥20 fps, gradient-dominated cross-correlation,
whole-second dump lists, large-grid noise-vs-scale lens, fps-400
decorrelation, slow-cycle and marker-only probe false-inerts, rj-label
convention). Verdict distribution over the 15: 0 match, 3 close, 8
divergent, 4 broken — mean score 3.7, and every pair now has a current,
actionable verdict for the #101 fix pass and for review in the
tools/verify/review.mjs UI.

## 2026-08-29 — Engine gaps 4/5/7 settled against the oracle: wrap is
## authentic, setPalette aliases live, render late-binds, the array
## ledger is PB-exact (Gitea #106, #108, #109)

Three oracle probe batteries (fw 3.67; `tools/oracle/overflow-probes.mjs`,
`budget-bisect.mjs`, `alias-probes.mjs`) settled the remaining verify-sweep
engine gaps, and two of the four answers flipped the issue's premise:

- **#106 closed as authentic**: plain add/subtract/`+=` WRAP on real PB
  (`32000+1000` reads −32536 via exported vars; a post-wrap `>= delay`
  gate reads false), so the fire-blue/fire-red/spring-colors 32.768 s
  freeze family happens on real hardware too. No engine change — docs
  now say so (spec/vm.md, oracle findings), re-judges queued.
- **#109 reframed**: PB never frees arrays either — per-frame
  `array(100)` kills the pattern on the oracle at exactly frame 98. The
  actionable half was PRECISION: bisecting the real ledger gave
  **10,236 units with every array costing len+4** (single max 10,232;
  5113+5113 ok / 5116+5116 abort; all boundary points check). Engine now
  charges that exact model (was flat 10,240, no per-array cost), with
  boundary + exhaustion tests pinning the device numbers.
- **#108, the six silent-null originals**: two were real engine gaps,
  both oracle-confirmed and fixed — `setPalette(arr)` holds a LIVE
  reference (in-place writes re-cook the palette; snapshot dropped) and
  a render function assigned to `export var render`/`render2D` at
  runtime now dispatches (entry re-resolved each frame through the
  global; slime-mold-palette renders, and live re-assignment swaps
  entries like the oracle does). The other four: coral-plasma was a port
  arity bug (6-arg `perlinRidge`, fixed), skypirate needs its author's
  1800-px 3-column rig (fixups.json pins grid 3×600),
  performance-test-framework is non-visual BY DESIGN, automap needs a
  harness `--vars` flag (ticketed).
- **Bonus family**: the perlin octave refit (b37df0a) silently orphaned
  four ports written against the old min-1-octave clamp — their calls
  now ran 0 octaves and froze into constants (perlin-fire's "fire" was a
  static smear on current master, distinct from its frame-512 crash).
  De-orphaned with explicit single-octave calls (perlin-fire,
  eye-of-sauron, coronal-mass-ejection, distance-function-kaleidoscope-2)
  and killed the frame-512 crashes (perlin-fire + eye-of-sauron
  installed their setPalette literal per frame; hoisted to init).

Verified: workspace tests green (5 new engine tests pin the oracle
boundary numbers, palette aliasing, and late-bound dispatch);
snap.mjs re-runs show all four fixable silent-null sides rendering with
motion; FINDINGS.md carries the full addendum. The PB oracle was left on
its original pattern (restore-in-finally per probe).

## 2026-08-29 — Cold loads back to 10/10: the installer page's second
## vite entry was overflowing the device's 3-socket pool (Gitea #92)

Root cause of the 0/10 cold-load regression: commit `0b651c4` (the
WLED→Luxel installer page, 2026-08-15) added `flash.html` as a second
rollup input — 97 minutes *after* the 10/10 baseline was recorded on the
single-entry bundle. Vite then hoists the modules shared by the two
pages into an `app-*.js` chunk (injected as `<link rel="modulepreload">`)
and, with the default `cssCodeSplit: true`, emits a separate `app-*.css`
for it — so the browser-native burst right after `index.html` parses
went from 2 requests to 4. Native loads can't go through fetchgate, the
default pool is 3 sockets (`server.rs` `WEB_TASK_POOL_SIZE`), and a
`web_task` only listens while parked in `accept()` — smoltcp answers the
4th SYN with a RST, which Chromium reports as `ERR_CONNECTION_REFUSED`.
The refused victim (`/assets/app-*.css`, 758 bytes) didn't even exist at
baseline. The pool-churn from that burst also knocked over one *gated*
fetch (`/luxel.wasm`, refused then retried clean by fetchgate ~200 ms
later) — the second per-load failure the issue recorded at 60 px.

Fix is two lines of vite config, no firmware change and no new
dependency: `build.cssCodeSplit: false` (one shared stylesheet instead
of per-entry files) and `build.modulePreload: false` (the shared chunk
is fetched via its static import after the entry chunk arrives, instead
of preloaded in parallel). Native burst is now html → entry js +
stylesheet, 2 concurrent sockets worst case, and the second-wave
`app-*.js` lands after the first sockets close.

Verified on the Athom (v0.1.39, default build, assets pushed with
`deploy.sh --assets-only`): before-fix repro 0/3 with the exact issue
signature, after-fix **10/10 clean cold loads, 0 failed requests
total**, load times unchanged (3.6–4.3 s vs 3.9–4.4 s dirty). Playground
e2e and flash-e2e both pass (the installer page shares the merged
stylesheet), and the settled device UI screenshots clean. Docs touched:
ideas.md's stale "10/10 clean" claim now records the regression window;
tools.md's coldload row no longer calls the default pool 2-socket.

## 2026-08-24 — Interactive port review UI: both sides of all 293 pairs
## live in a browser, with per-pattern decisions that persist

`tools/verify/review.mjs` turns the finished verification sweep into
something a human can actually work through. It's a zero-dependency
local server (`node tools/verify/review.mjs`, default port 4183) that
serves the engine wasm and a plain-ES-module UI which compiles and runs
**both** sides of every pair live in the page — the corpus original and
its clean-room port, on the same engine, same rig, seed 1, the same
pinned wall clock and the same beat120 synthetic sensor feed the judges
saw. So what you watch is what was judged, except you can drive it.

List view is one card per pair: two live canvases (strip bar, pixelated
grid, or cloud z-slices), rig/verdict/decision badges, the judge's
summary, and a compact decision bar. The sticky top bar carries a global
fps slider (1–60), pause/reset-all, verdict and decision-status filters,
and slug/name search. Clicking a card opens a detail modal with bigger
canvases, per-side control panels (sliders honouring `//#` bounds hints —
including the line-above placement `library/` files use — plus hsv/rgb
channel sliders, toggles, triggers and polled showNumber/gauge
readouts), a per-side reset, and the full verdict: summary,
observations, a per-dial match table, feedback. 293 × 2 engines would
melt the tab, so an IntersectionObserver keeps only visible cards live,
capped at ~40 engines, with a single rAF loop round-robining a fixed
step budget — the same shape as the playground's Gallery.

The point of the tool is the **decision**: delete / good / fork (with an
optional new name) / needs-work, each with an optional feedback note
that goes verbatim to the agent doing the fix pass. A decision POSTs
immediately and lands in the tracked `tools/verify/decisions.json` via an
atomic tmp+rename write, so a review survives restarts and spans as many
sittings as it takes. Everything else is assembled at request time —
edit a `library/*.js` and reload.

Alongside it, `tools/verify/fixups.json` + `fixups.mjs`: a declared
per-slug fixup manifest shared by snap.mjs, report.mjs and review.mjs.
It strips author-planted tripwire lines from **originals** (the
deliberately-invalid sentinels a pattern's README tells the user to
delete — both `music-sequencer-*` originals were scored
`orig-unrenderable` purely because of these, and now compile and render
on both sides), and overrides the rig for **both** sides where an
original only renders on a specific geometry: `nano-orbital` ≥144 px,
`nyan-lights` a 300-px strip, `orv-christmas-tree` grid 20×20 — the
three manifest fixes SWEEP-NOTES.md had been carrying as a to-do. Fixups
in force are stamped into snap.mjs's `provenance.fixups`, and the
manifest is folded into `harnessSha256` so editing it correctly
invalidates cached runs. It is deliberately not a place to patch
patterns into working.

Verified in real chromium: 293 cards, canvases animating, both
music-sequencer originals rendering with no compile error, modal
controls rendering and a slider drag causing no runtime error, and a
needs-work decision plus feedback surviving a server restart and a fresh
page load. `docs/tools.md` gains rows for review.mjs and the fixup
manifest; the report.mjs row now says it's superseded for triage.

## 2026-08-23 — pow/exp2 overflow now saturates, PB-exact (Gitea #112)

Follow-on from the re-judge batch: PB's `pow` saturates on overflow —
positive to raw `0x7FFFFFFF`, negative-odd to raw `0x80000000`, both
oracle-pinned exactly (fw 3.67) — while our `exp2` wrapped
(`pow(2,16)` = 0, `pow(2,15)` = −32768). This is the one non-wrapping
corner of PB arithmetic found so far; documented in
docs/research/04-oracle-findings.md. Fix in `fmath::exp2` (saturating
integer shift) + `pow`'s negative-odd path (MIN, not −MAX). Unit tests
pin all seven probed values. With it, `synchronized-random-numbers`'s
original BSD-rand LCG (`% pow(2,16)`) comes alive — prng_state now
walks [0, 32768) like the oracle's. Workspace tests + wasm smoke green.

## 2026-08-23 — Engine gaps #104/#105 fixed: wall clock reaches init,
## random(negative) is PB-exact; sweep's wall clock was never applied

The verify sweep's two front engine gaps are root-caused and fixed
(Gitea #104, #105), plus a harness bug the sweep itself hid behind:

- **#104 time-of-day builtins.** Two stacked bugs. (1) snap.mjs recorded
  `--wall-clock` in meta.json but never passed it to `renderSide` — every
  render of the entire 293-pair sweep ran at epoch 0 via
  `setWallClock(undefined)` → NaN → 0. (2) The engine ran top-level init
  inside construction, before any host could hand it the clock, so
  init-time `clockHour()` reads were always 0 — on device and CLI too, not
  just the harness. New: `Engine::new_at`/`from_program_budgeted_at`
  (clock at construction), `lx_set_default_wall_clock` in the wasm ABI,
  and every host (firmware, serve, CLI, playground, enginehost) now
  supplies the clock at build time; the verify hosts throw on a
  non-finite clock. `pixelclock` renders now differ across wall clocks on
  both sides.
- **#105 init-time randomness.** Not init-specific: `random(max)` clamped
  negative `max` to 0, and `random(0xffff)` is `random(-1.0)` (16.16
  literal wrap, PB-identical). Oracle probe (fw 3.67): PB draws the whole
  signed range for negative max — `scale_random` now multiplies by the
  raw word unsigned, PB-exact; positive max unchanged.
  `static-random-colors` goes solid-red → 59/60 distinct colors;
  `synchronized-random-numbers` regains motion.

Filed the remaining untracked sweep gaps as Gitea #106 (2^15 ms freeze),
#107 (OOB-write tolerance residual), #108 (silent-null originals), #109
(array element budget). Tests: init-clock + negative-random semantics
tests added; workspace suite, wasm smoke, firmware build, web
typecheck+build all green. FINDINGS.md carries a dated addendum — sweep
verdict observations involving wall clocks describe epoch-0 renders.

**Re-judge batch (same day, post-fix):** all six unblocked pairs
re-judged by fresh Opus judges — pixelclock close/6 (port's real defect:
an i/60·60 16.16 round-trip that floors to i−1 except at 0/15/30/45 →
+1-shifted markers, 4/5/6-px hour bar, second-dot dropouts at
14/29/44/59), naturallightsync close/5 (port too white at noon, pale
night, sunset an hour early, ramp law off), sunrise-alarm-clock close/6
(neither side reads the wall clock — it's a 1 h/s time-lapse; port's
Cloudiness dial inverted+weak, pixel 0 dead, clock origin +5.7 h),
utility-scheduled-percent-on-demo close/7 (prior "total collapse" was
purely the harness bug; real defects: hour quantizer one LSB low at
exact k/24 sliders, invented 08:00–20:00 default schedule),
static-random-colors close/6 (fully-saturated pixel mass 16% vs 58%),
synchronized-random-numbers divergent/4. That last judge caught a NEW
engine gap: the original's BSD-rand LCG needs pow(2,16), and our
pow/exp2 WRAPS on overflow where PB SATURATES — oracle-pinned to raw
0x7FFFFFFF (pos) / 0x80000000 (neg) exactly. Filed as Gitea #112.
JUDGE.md gained a clock-driven-static-slug section (dense --wall-clock
sweeps; --probe-controls at one fixed clock can fake dead/mirror-image
dials); ORCHESTRATION.md re-judge queue updated.

## 2026-08-24 — Clean-room port verification sweep COMPLETE: all 293
## pairs judged (tools/verify/results/ + FINDINGS.md)

Every corpus/library pair now has an output-only verdict from an
independent Opus judge (5 parallel judges per batch, ~59 batches):
**24 match · 119 close · 123 divergent · 17 broken · 10
orig-unrenderable**, mean score 5.42/10. Each
`tools/verify/results/<slug>.json` carries measured observations,
per-dial comparisons and concrete acceptance numbers for a fix pass;
`tools/verify/FINDINGS.md` synthesizes the systemic defect families
(PB time-base constants, frame-vs-time coupling, missing lifecycle
management, control-surface drift, coordinate/units errors, colour
constant families incl. the Christmas template, direction flips) plus
the engine gaps the sweep surfaced (time-of-day builtins pinned —
confirmed; init-random constant; out-of-range write intolerance; the
32.768 s freeze family; silent-null originals) — tracked in Gitea #84
and #99. JUDGE.md grew ~30 measurement-trap notes contributed by the
judges as they hit them; snap.mjs gained `--wall-clock` and a
false-clamp-warning fix along the way.

## 2026-08-22 — The DEFAULT build takes the mild WiFi RX trim too
## (+6.4 KB idle, soaked with serial attached — Gitea #60)

Follow-up to the same day's small-chip WiFi tuning, and the first soak on
the Athom with `/dev/ttyUSB0` actually present — which is the whole reason
#60 was left open. `static_rx_buf_num` 10→6 on the **default** build; that
is the entire diff. AMPDU RX stays on and `dynamic_rx_buf_num` stays at
32, deliberately: `static_rx_buf_num` is the only knob that reclaims
anything at idle (those buffers are allocated in `esp_wifi_init` and never
freed), while the dynamic pool and the block-ack buffers are on-demand, so
trimming them would bound the worst case and cost RX throughput on a busy
network for no idle gain. `rx_ba_win` stays 6 and still validates
(6 < 32 dynamic, 6 < 2 × 6 static). `small-chip` keeps its harder 4/16/off.

**A/B on the Athom rig (v0.1.39, 60 px WS2812, idle `heap_free`, 20
samples each, both immediately post-OTA):**

| build | idle heap_free | Δ |
|---|---:|---:|
| default, stock pools (master) | 98,352 | — |
| default, `static_rx_buf_num` 6 | **104,832** | **+6,480 B** |

Exactly 4 × ~1,620 B, i.e. the arithmetic the small-chip session
predicted, with none of the estimate error that entry warned about. App
image is byte-identical in size (946,288 B both builds), so the 1 MiB slot
margin is untouched. `.stack` 29,396 B (athom) / 29,372 B (pixelblaze-v3),
clear of the 24 KB floor, no frame over 12 KB.

**Soak, all on the trimmed build, with a live serial capture the whole
time** (the thing the small-chip session could not do):

- `tools/hw-bench.mjs`: **321/322**, ~45 min. The one failure is the
  long-standing pattern-side array OOB in "sound - spectromatrix
  render2D", present on every prior soak. Lowest `heap_free` across the
  churn 78,408 B; fps-vs-pixels curve unremarkable (123 @ 60 px … 5 @
  2048 px). Serial shows exactly one boot in the whole run — the OTA's own
  reboot — and no panic, no `rst:` other than that, no boot-guard trip.
- **RX-pool stress**, A/B'd rather than just run (new `tools/rx-stress.mjs`,
  docs/tools.md): 180 s of DDP at ~244 pkt/s × 300 px (~217 KB/s inbound
  UDP) concurrent with a 6-worker HTTP API hammer.

  | build | DDP frames | HTTP served / refused | min heap_free |
  |---|---:|---|---:|
  | default, stock pools | 44,096 | 617 / 2,346 | 78,352 |
  | default, static RX 6 | 44,104 | 641 / 2,238 | 84,844 |

  The trimmed build served *more* requests under identical load, so the
  smaller pool costs nothing measurable here, and its heap floor under
  load is ~6.5 KB higher. Every watchdog sample during the run reported
  `live: "ddp"` — the frames were being received, not silently dropped
  (the harness fails if that count is zero, precisely so a dropping RX
  path can't read as a clean pass). Slot held `ota_0`, no vmerr, nothing
  on serial. Then a **640,026 B streaming asset upload** succeeded.
- `web/tools/coldload.mjs`: **at parity, and the parity is the finding.**
  Trimmed and stock-pool builds both score 0/10 clean at 300 px (1 refused
  sub-resource per load) and 0/5 clean at 60 px (2 per load), with
  identical timings; every load still boots fully (`boot ok`, editor
  populated, 0 page errors). So today's master does not hit the 10/10
  docs/ideas.md records for the 3-slot pool — that is a pre-existing
  regression on master, unrelated to the RX pools (an undersized esp-radio
  pool presents as a crash or dropped frames, never as a clean
  `ERR_CONNECTION_REFUSED` before any body). Filed as **Gitea #92**.

New tool: **`tools/rx-stress.mjs`** — the DDP+HTTP RX gate, written
because this stress has now been hand-rolled twice; it also watches
`slot` for the silent boot-loop rollback. Builds verified for
athom-music, pixelblaze-v3, c3-devkit, c6-devkit and athom+small-chip.
The post-rebase merged build was re-OTA'd and reproduces the same
104,832 B idle figure plus a clean 60 s stress, so the number survives
the day's other merges. Device left as found: 60 px WS2812, brightness 4,
playlist empty and stopped, running the merged build.

## 2026-08-22 — Perlin family fitted to the oracle and matched bit-for-bit
## (Gitea #65)

Offline fit of the 3,320 raw samples the 2026-08-22 oracle session
captured into `tools/oracle/sweeps/`. **PB's `perlin`/`perlinFbm`/
`perlinRidge`/`perlinTurbulence`/`setPerlinWrap` are a float32 port of
Sean Barrett's `stb_perlin.h`, using its non-power-of-two wrap variant**
(`stb_perlin_noise3_wrap_nonpow2`). `crates/luxel-core/src/noise.rs` now
reproduces it; the previous implementation was an invented stand-in.

How it was pinned down, from captured input/output only (no firmware
reversed) — full derivation in docs/research/04-oracle-findings.md:
per-cell polynomial fits of the fine sweep collapse at *exactly* degree 6
(⇒ gradient noise × quintic fade — value noise would be 5, simplex 8);
the recovered per-corner gradients all fall in the ±1/±1/0 basis; and the
lattice byte at every sampled cell is `randtab[randtab[floor(x) mod wrap]
+ seed]` against stb's own tables — the *double* lookup that distinguishes
the nonpow2 variant from plain `noise3_internal`, which fits no seed at
all. Arithmetic is f32 because that is what PB does: a careful 16.16
re-derivation sits ±5 raw units off, while f32 + truncate-toward-zero into
16.16 is bit-exact on 99.5% of the samples and within 1 LSB on 100%.

`compare-sweeps.mjs` before → after, all ten noise sweeps: 0% exact (max
error ~1.0 in value) → perlin1d_fine, perlin_seed, perlin_wrap4,
fbm_arg4/5/6 and ridge1d **100% bit-exact**; perlin1d, fbm1d, turb1d 99.0
–99.5% exact and **100% within one raw LSB** (1/65536). That harness had
been silently dropping each sweep's `setup` line, so `perlin_wrap4` was
being compared without its `setPerlinWrap(4,4,4)` — fixed here too.

**Existing patterns' noise visuals change** — that is the point: they now
look like they do on a Pixel Blaze. Behaviours worth knowing: the fractal
variants are *not* normalized (fbm at gain 0.5 spans ~±1.75, ridge is
non-negative and can exceed 1); each octave uses the octave index as its
seed, so layers never share lattice lines; `seed` wraps mod 256; ridge
starts at amplitude 0.5 and weights each octave by the previous octave's
value. `octaves` truncates toward zero (≤ 0 → 0) and is capped at 32 so a
runaway argument can't stall a frame.

New host tests lock the behaviour against subsampled device fixtures
(`matches_pixelblaze_perlin`, `matches_pixelblaze_fractals`, ±1 raw
tolerance) plus octave-count and seamless-wrap tests. Verified:
luxel-core 69/69, `cargo test --workspace` green, clippy clean in
noise.rs, stack-check ok (no frame over 12,288 B, `.stack` 29,372 B).
Flash cost on the tightest board (c6-devkit) +1,472 B → 993,648 B app
image, 54,928 B of slot margin. `web/public/luxel.wasm` in the main
checkout lags master as always — rebuild in your own worktree.

## 2026-08-22 — Runtime-error blast radius now matches PB: an error kills
## the handler call, not the frame (Gitea #84)

The two corpus originals that "hit array-OOB at frame 0 and render
all-black" (Nano Orbital, Orv - Christmas Tree) weren't hitting an
array-semantics gap at all — `array(3.2)` truncates to 3 slots on PB
exactly like ours (probed), so both patterns OOB on the real device too.
The gap was what happens NEXT. Oracle probes (fw 3.67, new
`tools/oracle/oob-probes.mjs`, self-judging): a runtime error aborts only
the current handler invocation — writes made before the abort stick, a
`beforeRender` abort does NOT skip the per-pixel pass, and a `render(i)`
abort keeps that pixel's pre-error hsv while later pixels render
normally. Our engine ended the whole frame on any error, so a
pattern erroring in `beforeRender` every frame stayed black forever.

`engine.rs drive()` now coerces a non-fatal error to the handler's normal
completion (the existing continuation logic then does the right thing —
`vm.pixel` already holds the pre-abort color); first error of a frame
wins `last_error` (a per-pixel error would otherwise re-alloc its message
per pixel). Deliberately still frame-fatal: `assert()` (init-only by
construction, belt-and-braces) and the VM resource guards — step limit,
value-stack bounds (named consts + `VmError::is_resource_guard`) — since
re-running a stuck handler per pixel would multiply the step limit by
pixel_count per frame and starve the firmware watchdog. Blast radius
recorded in docs/research/04-oracle-findings.md §10 and docs/lang.md.

Verified: luxel-core suite + full workspace green (5 new blast-radius
tests; 1 old test updated — it pinned the pre-oracle blanking behavior);
both originals render their real designs at 64 px via `luxel-cli pixels`
(orbit dots / full tree scene) with the OOB still reported as vmerr;
native↔wasm goldens bit-identical (wasm-smoke); athom-music firmware
builds, image-check ok, app 946,432 B (90.26% of slot), stack-check ok.
Follow-up: the output-verifier sweep (#84 filing) should re-judge the two
pairs as renderable once it lands/reruns.

## 2026-08-22 — Hygiene sweep: stale wasm goldens, the clippy deny that
## couldn't hold, and an indexed library sweep (#85, #79, #67)

Three small things that had each been quietly blocking a gate.

**#85 — `tools/wasm-smoke.mjs` goldens were stale.** The PB-exact
floor-quantization change (`floor(v·255)`, `quantize()` in
`crates/luxel-core/src/engine.rs`) moved the golden bytes, but only the
native tests were updated; the JS mirror was red against a healthy build.
Both goldens now match their native counterparts, cross-checked rather than
copied from what the wasm happened to emit: the rainbow frame's 128s → 127
(`tests/engine.rs::rainbow_golden_frame`), and — the one the issue missed —
the 2D map/transform frame's 255s → 254, because grid world coords max out
at ≈0.99998 and the quantization floors (`tests/semantics.rs::map_and_introspection`
asserts exactly that). Comments now name the native test each golden mirrors.

**#79 — `cargo clippy` failed on the firmware with 7
`large_stack_arrays` errors** (clippy 1.96), so the deliberate
`#![deny(clippy::large_stack_arrays)]` in `main.rs` gated nothing. Fixed
without weakening the deny:

- `netin.rs` (`bufs!`, 3 instantiations) and `provision.rs` (dhcp + dns
  tasks) now use `static_cell::ConstStaticCell` instead of
  `StaticCell` + `.init([0; N])`. This is a real fix, not an annotation:
  the zeroed array becomes a *const* initializer stored in the static
  itself (`.bss`), and `take()` just hands out the reference — no multi-KB
  value built at the call site that the optimizer is merely *likely* to
  elide. The 1024-byte DNS buffers were converted too; they sit exactly on
  the lint threshold. Metadata arrays stay `StaticCell` (not
  const-constructible here, and far under the threshold).
- `ota.rs:315` (`preboot_guard`) keeps its 3 KiB stack array under a scoped
  `#[allow]` with the real constraint spelled out: it runs *before* the
  `heap_allocator!` calls, so the `alloc::vec!` every other `OtaUpdater`
  site in that file uses would allocate from a heap that does not exist yet.
- `hub75.rs:83` likewise — the array is inside esp-hub75's
  `hub75_dma_descriptors!` macro (third-party, carried as a patch file),
  and DMA descriptors must live in a fixed static anyway.

Verification: `cargo clippy --release` clean (0 errors, warning count
unchanged at 32) on **board-c3-devkit** (default), **board-c6-devkit**,
**board-athom-music** (Xtensa/esp32) and **board-s3-devkit,hub75** — the
last is where the 7th error lives, invisible to a default-feature run.
`cargo build --release` still builds; `.bss` and `.data` are byte-identical
to master (296392 / 9564), `.text` +20 B — the buffers did not move out of
`.bss`. `tools/stack-check.sh` identical before and after: c3
`.stack` 39616 B over 1179 functions, largest frame 8944 B; the default
pixelblaze-v3 board `.stack` 29372 B over 1260 functions — no function over
the 12288 B budget on either.

Note for future Xtensa clippy runs: `cargo clippy` picks `clippy-driver`
off `PATH`, so the esp toolchain's `bin/` must be *prepended* to `PATH` —
setting `RUSTC` alone (what `stack-check.sh` does, which is enough for
plain builds) leaves mainline clippy compiling the fork's `core` and it
dies on intrinsic mismatches.

**#67 — `tools/check-library.sh`**, the library sweep every session was
re-deriving from prose. Builds `luxel-cli`, runs `luxel check` (compile +
LXBC round-trip + 3-frame smoke) over every pattern in `library/` on both
rigs the established acceptance uses — `check`'s default 10×10 and an
explicit 16×16 — prints a per-grid pass count plus the file and engine
stage/error for each failure, and exits non-zero if any failed. `GRIDS=`
overrides the rig list, an optional positional arg points it at another
directory. Indexed in `docs/tools.md`, alongside a row for
`tools/wasm-smoke.mjs`, which turned out to be the one script in `tools/`
the index had never listed.

Verification: sweep run end to end — **322/322 on the default grid,
322/322 on 16×16**. `cargo test -p luxel-core --release` green (163 tests,
0 failed). `node tools/wasm-smoke.mjs` passes against a freshly built
`luxel_wasm.wasm`.

## 2026-08-22 — Output-only port verifier: render-and-judge harness for
## the clean-room library (tools/verify/)

Many clean-room ports work poorly or not at all, and nothing measured
that. New harness verifies a port against its Pixelblaze original purely
from rendered output — no code inspection, which also keeps it cleanly on
the right side of the corpus firewall. `tools/verify/`:

- `gen-pairs.mjs` — pairs all **293/293** corpus `.epe` with their
  `library/` ports (provenance-comment key + slug key + 5 name-drift
  fixups; 4 ambiguous duplicate-name cases carry candidate id lists).
  29 library files are Luxel originals with no corpus counterpart.
- `snap.mjs` — renders BOTH sides headlessly (node + luxel wasm, zero
  deps) on an identical rig (strip 60 px / grid 16×16 / cloud 5×5×5),
  same seed + pinned wall clock + fixed delta ⇒ byte-deterministic.
  Artifacts per side: waterfalls (1D/3D), timestamped contact sheet +
  consecutive-frame filmstrip + full-window rhythm waterfall (2D),
  `meta.json` (controls, errors, stats summary + trend flags,
  provenance hashes), `stats.json` (full per-frame series),
  `--probe-controls` → per-dial responsive/inert fingerprint. Judge-
  safe: artifacts never contain pattern source.
- `JUDGE.md` — the judge-agent procedure: firewall rules, animations-
  not-frames doctrine (mandatory 60 s survey run, steady-state check,
  filmstrip/rhythm-based motion evidence), dial probing, verdict schema
  with output-level improvement feedback for a later fixing pass.
- `enginehost.mjs` / `png.mjs` — wasm C-ABI host and dependency-free
  PNG encoder (+3×5 digit font for cell timestamps).

Calibration verdicts (in `tools/verify/results/`): **amoeba** = broken
2/10 (port freezes ~2 s in and decays to black; original churns at
steady brightness indefinitely); **2d-fireworks-fade** = divergent 4/10
(mode cycle + palette match, but bulbs→solid bars, one sweeping beam →
always-on comb, missing blue phase, and a fully mismatched dial
surface). Both verdicts carry fixer-ready feedback. The corpus-wide
sweep runs next; findings so far: originals `nano-orbital` and
`orv-christmas-tree` hit array-OOB on our engine while their ports run
(engine gaps, ticketed separately).
## 2026-08-22 — HUB75 output driver: ESP32-S3 LCD_CAM via esp-hub75
## (Gitea #72, feature `hub75`)

The first non-strip `OutputDriver`: `firmware/src/hub75.rs` composes each
post-outpipe RGB888 frame into a bitplane BCM framebuffer that a circular
DMA chain rescans autonomously (esp-hub75 0.14 `circular-dma` +
`skip-black-pixels`) — refresh is decoupled from engine rate and costs no
ISR work. Two heap-leaked framebuffers (~28 KB each, 64x64 x 7 planes,
allocated at wiring time while the heap is fresh — alloc failure disables
output, never panics) double-buffer via esp-hub75's atomic descriptor
swap; the swap is waited at the *next* frame's start, so it's free at
engine rates. Compile-time geometry (const-generic DMA statics): 64x64,
7 bitplanes (~77 Hz at the 20 MHz example clock; 8 planes would halve
that — depth/clock tuning is on-metal work, #75). Brightness scales in
software like WS2812; `set_protocol` returns `Err` per the output.rs
fixed-wire-format contract (render task keeps the old protocol).
`PowerModel` branch in `luxel_core::outpipe`: HUB75 divides the strip
estimate by the scan ratio (1/32 for 64-row panels) — host-tested,
deliberately ~2x conservative vs typical rated panel draw.

esp-hub75 targets release esp-hal 1.1.0 and our stack pins esp-hal git
main; the drift (two renamed DMA APIs) is carried as a **patch file**
(`firmware/patches/`, per Jeremy's preference over vendoring) — the flake
materializes the patched source into gitignored `firmware/vendor/` (devshell
symlink + hermetic copy-in), `[patch.crates-io]` points at it. New nix
variant `luxel-fw-s3-hub75` is in the release board loop; image-check
gained feature-gated markers (`EXPECT_FEATURES`) with a `hub75:` marker.
Default pixel count on hub75 builds = 2048 (cap-clamped half panel) until
#74 lifts `MAX_PIXELS` per-board.

Verified: builds clean for s3+hub75, c3-devkit, athom-music, c6-devkit;
luxel-core host tests 65/65; hermetic `nix build .#luxel-fw-s3-hub75`
works (app image 884,320 B creds-baked → 164,256 B slot margin — smaller
than plain s3-devkit, the strip encoders drop out); stack-check ok (no
frame over the 12,288 B budget, `.stack` 50,548 B ≥ 24 KB floor — the
framebuffers are heap, only the ~600 B descriptor static lands in .bss).
UNTESTED ON METAL — no S3 on the bench; hardware bring-up is #75 (QEMU
can't model LCD_CAM, and the harness-isolation rule forbids faking it).

## 2026-08-22 — Output-driver abstraction: the render loop no longer
## knows it's talking SPI (Gitea #71, HUB75 prereq)

The output-driver trait docs/PLAN.md promised ("so parallel drivers slot
in later without touching the render loop") now exists: new
`firmware/src/output.rs` with an `OutputDriver` trait (`set_protocol` /
`resize` / `write_frame`) and `SpiStripOutput`, which absorbed
`EncodeBuf`, `realloc_buf`, and `spi_cfg` from main.rs verbatim — the
u32-alignment DMA invariant and the lazy per-frame realloc retry moved
with them, unchanged. `render_task` now takes the `BoardOutput` type
alias (static dispatch — embassy tasks can't be generic, and there's no
`dyn` in the frame path); the engine-freeing retry policy on tight-heap
protocol switches stays in the task, where the engines live. HUB75 (#72)
becomes "new impl + alias switch" instead of "rewrite the render loop."

Zero intended behavior change. Verified: builds for c3-devkit, s3-devkit,
c6-devkit, athom-music+small-chip; stack-check on default and
athom+small-chip (.stack 30,236 B, floor 24 KB); full QEMU suite 5/5
PASS; athom OTA image 946,880 B vs master's 948,400 B (−1.5 KB, slot
margin unaffected). One log-format nit: the protocol-switch alloc-failure
message now reports pixels, not bytes.

**Athom hardware soak (default profile, OTA'd to ota_0):** targeted
exercises of the refactored paths first — live protocol switch
ws2812→sk9822→ws2812 and pixel count 60→2048→60, including the worst
case (ws2812 @ 2048 px = the 18 KB encode-buffer realloc; heap deltas
matched the math, no reboot, no vmerr). Then `tools/hw-bench.mjs`:
**321/322 clean** — the one failure is the long-standing pattern-side
array OOB in "sound - spectromatrix render2D", same as every prior soak
(see the 2026-08-22 RX-pool entry). Lowest heap_free 63,992 B (vs
65,840 B in the v0.1.37 default-profile baseline; the gap is the bigger
ws2812-vs-sk9822 encode buffer at 300 px). fps-vs-pixel-count curve
unchanged (122 @ 60 px … 5 @ 2048 px). Slot held ota_0 throughout; no
rollback. Device restored to as-found config (60 px ws2812).

Context: HUB75 support planned with Jeremy 2026-08-22 (Seengreat HUB75
S3 + 64x64 panel ordered; series = Gitea #71–#75). Pre-existing, NOT
from this change: `cargo clippy` on master fails with 7
`large_stack_arrays` errors in netin/ota/provision (clippy 1.96 bump) —
filed separately.

## 2026-08-22 — Takeover imports WLED's LED wiring: a converted device
## comes up configured, not defaulted (the open half of Gitea #3)

The takeover already mounted WLED's littlefs and parsed cfg.json for the
SSID — and threw the rest away, so every conversion booted 60 px ws2812
at default brightness regardless of what the WLED install was actually
driving. Now the same read lifts the wiring and boot defaults, and the
takeover writes a full `LXDV` device-config record right next to the
`LXCF` creds record it already wrote:

| WLED cfg.json | Luxel field | mapping |
|---|---|---|
| `hw.led.ins[0].len` (fallback `total`) | pixel_count | clamped 1..=2048 |
| `hw.led.ins[0].type` | protocol | **22** (WS281x RGB) → ws2812, **51** (APA102) → sk9822; anything else (RGBW, WS2801, analog, matrix, virtual) keeps the board default + a serial note — deliberately conservative, a wrong protocol is worse than a default one |
| `hw.led.ins[0].order` | color_order | **relative to the chip-native order** (see below) |
| `def.bri` 0–255 | brightness 0–31 | rounded, floored at 1 (a >31 write voids the whole LXDV record — config.rs:261) |
| `hw.led.maxpwr` | cap_ma | 0 = limiter off stays off; clamped 20 A |
| `light.gc.col` | gamma_tenths | 2.8 → 28; 1.0/garbage → off |
| `hw.led.ins[0].pin` | — | logged only: Luxel pins are compile-time per board (board.rs:1-5) |

**The color-order mapping is the subtle part.** WLED's `order` is the
strip's *wire* order (COL_ORDER_*: 0=GRB…); Luxel's `ColorOrder` is a
*pre-encoder* remap with identity 0="rgb", and the encoders already emit
each chip's native wire order (ws2812 GRB, sk9822 BGR — leds.rs). So
WLED "GRB" on a WS2812 maps to Luxel *identity*, not Luxel "grb": the
import solves P in native∘P = wled_order per protocol. Pinned by a
construction test (`wire_order_roundtrip`) that simulates both pipelines
byte-for-byte across all 6 orders × both protocols rather than trusting
the hand-derived tables. Order is only imported when the protocol also
mapped — remapping against a guessed protocol would be a color bug.

Mechanics: scope-aware JSON scanners in wledfs.rs (balanced-bracket
ranges with string/escape tracking) because the interesting keys collide
all over cfg.json — `hw.btn.ins[].type`, `ir.type`, `relay.pin` — where
the WiFi lift's first-match-anywhere trick would grab the wrong value.
Same best-effort posture as the creds: every field individually
optional, any failure → board default, never a retry reboot. Import
happens before anything is modified; the write lands after the config
wipe, non-fatal like `write_wifi`.

Verification (no hardware; the QEMU harness carries it):
- 7 new host tests via `tools/wledfs-check` (`cargo test`), including
  decoy-key resistance and the wire-order construction proof; the rig
  binary now prints the wiring too, and against the real configured
  Athom dump extracts exactly the bench ground truth: 30 px, pin 18,
  type 22, order 0, bri 128, maxpwr 850, gamma 2.8.
- `tools/qemu/run-all.py` all green (5/5 suites) — takeover-test.py
  gained 13 assertions: two serial lines plus the full LXDV record at 0xA000
  byte-for-byte (30 px, protocol 1, order 0 — GRB-is-native proven on
  real config data — brightness 16/31 from bri 128, cap 850, gamma 28,
  checksum, erased tail).
- athom + c3 builds, image-check ok, stack-check clean.

Not done here: WLED's multi-bus configs import bus 0 only (Luxel drives
one output); no mic/button/IR import (no Luxel consumers yet — mic is
docs/mic-bringup.md); `rgbwm` ignored (no RGBW support at all).

## 2026-08-22 — Event injection hardware-soaked on the Athom (v0.1.39)

The deferred half of the v0.1.38/39 event work ("on-device soak when a
device is back"): 15 minutes against the rig at 192.168.0.183, serial-less
(/api/status + /api/vars polling — note /api/vars returns raw 16.16).
New indexed tool: `tools/event-soak.mjs`.

- **Delivery: 30,286/30,286** — steady 1–32-event batches at ~50 ev/s all
  arrived, and even the deliberate 30×32-batch overflow bursts were fully
  drained (at ~120 fps the queue empties between sequential HTTP posts;
  the drop path stays covered by unit tests).
- **Malformed frames: 44/44 rejected** (bad magic, truncation, count 33,
  count/length mismatch) — clean `ok:false`, no crash.
- **Heap stable** (97.7 KB idle → 92.5 KB min under load → 97.1 KB after
  cooldown, no drift), **no vmerr, no reboot** (evTotal monotonic), fps
  115–119 under combined injection+polling vs 123 idle.
- Fun correctness sighting: the counter pattern's `frames` export wrapped
  i32 right on schedule past 32768.0 — the documented two's-complement VM
  semantics, live on hardware. (It also false-positived the tool's first
  reboot detector; fixed to key on evTotal only.)
- Device left as found: ad-hoc live pushes persist nothing; rainbow
  restored.

Remaining for Jeremy (docs/UNTESTED.md unchanged in scope): the playground
click-through and the real-HA MQTT hop on the wall unit — still gated on
the dev unit coming back online.

## 2026-08-22 — TODO(oracle) sweep: every probe-able marker settled,
## two bug-for-bug fixes, perlin sweeps captured

All 24 `TODO(oracle)` markers are gone from the source. About half were
stale — settled by the July probe sessions but never cleaned up (div/mod
by zero, shift edge cases, sub-epsilon truncation, hsv rounding, refs-as-0,
transform order/sign) — and the rest were settled today against the live
oracle (fw 3.67) with a new self-judging battery,
`tools/oracle/todo-probes.mjs`.

**Two real divergences found and fixed in luxel-core:**

- **Transform stack cap is silent on PB.** 40 stacked translates: dx
  stalls after 31 ops, no error, pattern keeps running. `push_op` now
  ignores ops past 31 instead of erroring
  (test: `transform_stack_caps_at_31_silently`).
- **Palette lookup past the last stop is BLACK, not a clamp** — and the
  ends are asymmetric: below-first clamps to the first color. Hard edge
  exactly at the stop; single-stop palettes agree. `palette_lookup` now
  matches bug-for-bug (test: `palette_edges_match_pixelblaze`).

**Confirmed matches** (comments updated, no code change): method-form
`a.replace()` writes from index 0; `arrayReplaceAt` exists on PB and
matches; rotateX/Y/Z all CCW right-handed; no-palette paint = grayscale
ramp; palette state does not leak across live-code reloads; `null` = 0 at
runtime; clock civil conversion exact vs America/Denver. `undefined` is
REJECTED by PB's compiler — ours stays as a documented leniency.

**Perlin family:** arities verified identical via PB's own compiler
(perlin 4, fbm 6, ridge 7, turbulence 6, setPerlinWrap 3) and 3,320 raw
samples captured into `tools/oracle/sweeps/` (1D slices, seed sweep,
wrap-4 periodicity, per-arg fbm sweeps) for offline algorithm fitting —
filed as a ticket.

**Regression:** full `run.mjs` battery 138/175 — every diff is a
documented category (PB approximation error/seam bugs, deliberate prng
divergence, map-dependent transform specials pinned by unit tests
instead). Workspace tests green. Still open, all hardware-blocked: 1D
transform coords (oracle can never be mapless), sensor-board scaling,
no-time clock behavior. Findings doc has the full session record.
## 2026-08-22 — library: the last fake-trigger controls now listen for
## real events (`readEvent`)

The follow-up v0.1.38 explicitly left on the table ("swapping the
patterns' trigger controls over where it helps"). A trigger button is a
stand-in for a poke that comes from *outside*; now that `eventCount()` /
`readEvent()` exist, the patterns whose trigger only ever emulated one
consume the real thing, following the Typing Heatmap / Crosshair Pulse
idiom: **keep the manual control, add an event drain**. Backward
compatible — nothing that worked before stopped working.

Audited all 13 library files mentioning "trigger"; 4 exposed a
trigger-style *control* (2 already converted in v0.1.38) and 2 more used
a slider/toggle as a fake momentary button. Converted three:

- **Ripples 2D** — `triggerSplash` restarted drop 0 at a random point.
  Events now splash at the poked `(x, y)`; the trigger keeps its random
  splash and both go through one `splash()` that recycles the
  furthest-expanded ring instead of always clobbering slot 0. Rain keeps
  falling during real input (unlike the reference patterns' phantom
  generators, the drops *are* the pattern — no `quiet` window here).
- **Slime mold palette** — `triggerSeedAPixel` planted at a random free
  cell. Events plant at the poked cell (repainting an already-painted one,
  so a deliberate poke always shows), which is what lets an outside source
  steer where the blobs start.
- **SaberDeploy Tutorial** — its header already admitted the UI toggle
  "stands in for a momentary pushbutton". Events are now the real button:
  any frame carrying at least one event is one press (a burst inside one
  frame deliberately doesn't flip twice and cancel out); the toggle stays
  as the by-hand path.

Left alone, deliberately: *Golden Tix*'s `sliderSlideRightToReset` (a
slider-as-button hack, but resetting a live-coding sandbox's clock is an
editor action, not an external stimulus) and the ~9 patterns whose
"trigger" is an internal edge-trigger latch on audio/physics, not a
control.

Verification: three new tests in `crates/luxel-core/tests/engine.rs` push
an event into each converted pattern and assert the poked cell / blade
direction changes against a same-seed unpoked control — negative-controlled
(all three fail against the pre-change sources). Full `luxel check` sweep
over `library/` clean at 322/322 on both the default and 16×16 grids,
400-frame soaks clean, `cargo test --workspace` green, gallery regenerates
at 322.

## 2026-08-22 — WiFi-blob buffer tuning: the `small-chip` profile's
## missing half (+9.9 KB heap, soaked on the Athom)

The 2026-07-29 agreed follow-up from docs/ideas.md is done. esp-radio's
RX buffer pools are throughput-tuned by default; the `small-chip` cargo
feature now trims them, completing the feature that until today was only
its web-pool half.

**What changed** — three `cfg(feature = "small-chip")` lines on the
`ControllerConfig` main.rs already builds:

| knob | default | small-chip |
|---|---|---|
| `static_rx_buf_num` | 10 | 4 |
| `dynamic_rx_buf_num` | 32 | 16 |
| `ampdu_rx_enable` | true | false |

TX counts stay at the defaults deliberately: dynamic TX buffers are
allocated on demand, so lowering the cap reclaims nothing at idle and
only buys TX starvation under load. `rx_ba_win` stays at 6, which still
satisfies `ControllerConfig::validate()` against the trimmed pools
(6 < 16 dynamic, 6 < 2 × 4 static), so the pairing stays legal if AMPDU
RX is ever switched back on. The default build is untouched.

Plumbing: `firmware/build-esp32.sh` and `tools/stack-check.sh` both take
`EXTRA_FEATURES=` now — there was previously no way to build or
stack-check a non-board feature at all, which is a large part of why the
small-chip half sat unfinished.

**Measured (Athom rig, v0.1.39, idle `heap_free` from /api/status):**

| build | heap_free | Δ |
|---|---:|---:|
| default (as shipped) | 98,352 | — |
| small-chip, WiFi untuned | 115,548 | +17.2 KB |
| small-chip, WiFi tuned | **125,460** | **+26.5 KB** |

So the WiFi tuning's own share is **+9,912 B (9.7 KB)**, isolated by an
A/B of the two small-chip builds rather than inferred.

**The estimate was wrong in an instructive way.** The 2026-07-29 note
predicted 15–25 KB; the truth is 9.7 KB, and essentially all of it is
`static_rx_buf_num` (6 fewer buffers × ~1.6 KB = 9.6 KB, allocated inside
`esp_wifi_init` and never freed). The dynamic RX pool and the AMPDU
block-ack buffers are *on-demand* allocations — capping them bounds the
worst case but reclaims ~nothing at idle. Anyone tempted to chase the
remaining ~40 KB of blob draw should know it isn't sitting in the
configurable pools.

**Soak** (all on the tuned small-chip build, Athom, 300 px WS2812):

- `tools/hw-bench.mjs`: **321/322 clean**, identical to the default
  build's long-standing result (the one failure is the same pattern-side
  array OOB in "sound - spectromatrix render2D"). Lowest `heap_free`
  across the whole churn: 99,036 B — still above the *default* build's
  idle figure.
- **RX-pool stress**, the thing that would actually bite: 44,172 DDP
  frames at 245 pkt/s × 300 px (~220 KB/s inbound UDP) concurrent with a
  6-worker HTTP API hammer for 180 s, then a 629 KB streaming asset
  upload. No panic, no reboot, no rollback (slot held `ota_0`
  throughout).
- `web/tools/coldload.mjs`: 9/10 and 9/10 across two runs.

**The two refusals are the pool-2 tradeoff, not the buffers.** Both are
`ERR_CONNECTION_REFUSED` on the navigation itself at ~150 ms, before any
body — picoserve having no free slot, which is exactly the
"occasionally-refused first nav" cost the 2026-08-15 pool decision
accepted (Chromium wants ~3 sockets at a cold nav). An undersized
esp-radio RX pool presents as a StoreProhibited crash or dropped frames,
never as a clean TCP refusal. Same story for the hammer's 5,841
refused/reset vs 1,605 served: **zero** body-level failures, so
sustained throughput is fine and it's parallel fan-out that's capped.
docs/boards.md now states both costs with numbers instead of adjectives.

`tools/stack-check.sh` on both builds: `.stack` 29,516 B (default) and
30,340 B (small-chip), both clear of the 24 KB floor, no frame over
12 KB.

**Caveat on this session's evidence:** `/dev/ttyUSB0` was not present in
the container, so there was no serial capture — panic detection was a
1 Hz `/api/status` poller plus post-hoc `slot` checks (a boot-loop
rollback flips slots and would have been visible; none happened). Device
was left on the default build, `ota_1`, 60 px, playlist empty and
stopped, as found.
## 2026-08-22 — The editor warns before a pattern is too big for the device (Gitea #15)

Jeremy's framing on #15: *"Different ESP32s have different amounts of memory.
A pattern can work on one device with X pixels but not another. Fortunately,
we run the exact same pixel VM engine in WASM. We can have the device report
back how much memory it supports. While we are executing the script, we see if
we go over that threshold."* Plus a near-the-threshold warning.

The gap was real and worse than it looked: the device's capacity rejection is
**asynchronous**. `POST /api/code` answers 200, and only then does the render
task fail the post-load floor check and record a `pattern too large for this
device` vmerr. The editor showed nothing at all — the strip simply kept
running the previous pattern and the user was left to wonder.

**The estimator is a measurement, not a heuristic.** `lx_device_model`
(luxel-wasm, new) replays the firmware's own load sequence under a counting
allocator — the same instrument `crates/luxel-cli/tests/heapstat.rs` used to
establish the model in the first place, moved into the wasm build: LXP
envelope resident across `deserialize_lean`, dropped, then
`from_program_budgeted` at the device's array budget, then three frames. Peak
live bytes is what the floor check sees. Two things make this *better* than
the host test it descends from — wasm32 is 32-bit like the ESP32 (structures
measure at hardware width, where the 64-bit host test inflates every pointer),
and it counts the **whole envelope**, source included, which for a
source-heavy pattern is the actual peak rather than the engine.

**One definition of the budget.** `RUNTIME_FLOOR` (20 KB) and the array-budget
arithmetic moved out of `firmware/src/main.rs` into a new
`luxel_core::budget` — firmware and wasm now import the same constants, so
the prediction cannot drift from the device that enforces it. `load_headroom()`
encodes the subtle part: `heap_free` is measured with the *current* pattern
still resident, and since the firmware builds the new engine before releasing
the old one, that number really is the incoming pattern's headroom.

**Calibration against known ground truth.** Modelled over all 322 gallery
patterns at 300 px: 322/322 clean at 90 KB free, matching the
"full-library capacity, 322/322 modeled" figure in docs/ideas.md. Drop the
device to 70 KB free and exactly one pattern goes over — *"Music Sequencer -
for V3 ONLY"*, at 54.9 KB modelled against 51.2 KB of headroom. That is the
single pattern the 2026-07-19 full-library hardware soak actually saw
rejected. The model reproduces the hardware's one real verdict.

**UI.** A banner in the editor's right-hand stack, `.banner` idiom,
`data-role="capacity-warning"` with `data-level="tight"|"over"` and the byte
breakdown in the `title`. Severity follows **certainty, not size**: the local
model is advice (amber) and the device's own vmerr is a fact (red,
`data-role="capacity-rejected"`). Non-blocking throughout — the push still
goes, because the device is the authority and the editor only says what it
expects. The last 15 % of headroom counts as "tight": the model is exact but
the device's heap moves underneath it between the status read and the load.

**Silence where we don't know.** The playground has no device and therefore no
budget to judge against, and must not sprout a device affordance to say so. A
device that reports `heap_free` 0 (native mirror, older firmware) is silent
too — an unknown budget is not a small one, and guessing would cry wolf on
every pattern.

**Verification** (no hardware touched — the rig was another session's):
- `luxel serve --heap-free BYTES` (new) lets the mirror impersonate a device
  with that much free heap; default 0 keeps it honest about being a host.
- device-e2e spawns a second mirror claiming 30 KB free — 10 KB of load
  headroom with the arena clamped at its 16 KB minimum, which puts all four
  verdicts within reach of a one-line pattern. 9 new checks: silent on
  heap_free 0, clean/tight/over bands, the array-arena path, the numbers in
  the text, the push not being blocked, and the warning clearing. **100/100
  device-mode checks pass.** Playground e2e gained a check that an
  array-heavy pattern raises nothing there; all pass. Screenshots taken in
  real chromium.
- Firmware rebuilt (`image-check: ok`), `tools/stack-check.sh` clean,
  `cargo test --workspace` clean. The one clippy error in luxel-core
  (`LN_2`) is pre-existing on master.

Deliberately left out: no firmware API change (`heap_free` was already on the
wire and suffices — a device reporting its own derived budget would be
strictly more robust but needs hardware to verify); no pixel-count sweep
("this fits at 300 px but not at 2048"); and the warning is not shown on the
Device Patterns / Playlist lists, only in the editor.

## 2026-08-22 — ESP32-S3 and ESP32-C6 board features (builds only,
## untested on metal)

`board-s3-devkit` and `board-c6-devkit` join the four existing boards,
closing part (1) of docs/ideas.md "Small-chip profile + more board
features" (half of Gitea #3). Both ship as **builds, untested on metal**
— there is no S3 or C6 on the bench, so all that is established is: they
compile, link every load-bearing feature (`tools/image-check.sh`), fit
the 1 MiB OTA slot, and keep a `.stack` well above the 24 KB floor. Pin
choices, heap sizing and radio behaviour are unverified, and every place
they appear says so (Cargo.toml, board.rs, main.rs, flake.nix,
release.yml, docs/boards.md, docs/firmware.md, docs/releases.md).

Pins are each chip's SPI2/FSPI IO_MUX set, so DMA gets the direct route:
S3 CLK GPIO12 / DATA GPIO11 (clear of the octal-PSRAM pins GPIO33–37),
C6 CLK GPIO6 / DATA GPIO7 (clear of the devkit's RGB LED on GPIO8 — and
the same numbers as the C3 by coincidence of the IO_MUX tables).

**"Five-minute diff" held for the firmware, not for the build plumbing.**
The firmware side was exactly what docs/boards.md promised — a feature, a
`def` block, one `with_sck/with_mosi` line, plus widening the SPI-DMA cfg
from `esp32c3` to `not(esp32)` (C3/S3/C6 are all GDMA). No logic changed.
What actually needed doing was the surrounding machinery, which had the
classic-ESP32 chip/target/toolchain hardcoded in three places:

- `firmware/board-target.sh` (new) is now the single board → chip / rust
  target / toolchain map. `firmware/build-esp32.sh` and
  `tools/stack-check.sh` source it, so they can't drift. Both scripts
  drive *any* board now, Xtensa (`-Zbuild-std` + the Espressif fork) or
  RISC-V (mainline rustc, no build-std) — `BOARD=board-c3-devkit
  ./build-esp32.sh` works too, which it never did before.
- The flake needed `riscv32imac-unknown-none-elf` (the C6's target is one
  ISA letter off the C3's `riscv32imc`) in both the devshell toolchain
  and `riscvRust`, plus the two `firmwareVariants` entries.
- `.github/workflows/release.yml` builds and size-gates all six now.

Per-board app image at v0.1.39 (devshell builds, creds baked in) and
1 MiB-slot margin:

| board | app image | margin |
|---|---:|---:|
| `board-s3-devkit` | 885,840 B | 162,736 B |
| `board-c6-devkit` | 987,600 B | **60,976 B** |
| `board-c3-devkit` | 894,496 B | 154,080 B |
| `board-pixelblaze-v3` | 944,832 B | 103,744 B |
| `board-athom-music` | 944,720 B | 103,856 B |
| `board-esp32-generic` | 944,688 B | 103,888 B |

The C6 is the finding worth remembering: ~93 KB fatter than the C3 for
identical source, and at 5.8% it owns the tightest OTA margin in the
fleet — it is the board that will cross the ceiling first, so it's the
one to size-check on any release that grows the image. (docs/boards.md
now says this next to the table.)

`tools/stack-check.sh` on all four devkit/PB boards: no frame over
12 KB, `.stack` = 29,412 (pb-v3) / 39,632 (c3) / 51,172 (s3) / 141,320
(c6) bytes. The S3/C6 numbers come from reusing the C3's 160 KB heap on
chips with more DRAM; when hardware exists, that slack should become heap
(pattern capacity), not stack.

Verified: all six boards build in the devshell, `nix build
.#luxel-fw-s3-devkit` / `.#luxel-fw-c6-devkit` produce hermetic
images that pass `image-check.sh`, and the four pre-existing boards still
build byte-for-byte the way they did.

Deliberately NOT done: the installer page (web/flash.html) still lists
only the four known-good boards. It's a WLED-takeover flow for real
products, adding a chip there means extending the `Chip` union, the
arch-unsupported copy and the e2e fixtures plus a real-chromium pass, and
unknown board ids in a release manifest are skipped by design — so the
new artifacts publish harmlessly without it. Tracked as Gitea #57;
hardware bring-up for the two boards is Gitea #56.

## 2026-08-22 — Builtins batch 5: canvasAdd, seedable random(),
## timeScale / setFrameRate

The three small `docs/ideas.md` items that were left in the engine/runtime
section, appended to the builtin table (ids stable, no LXBC version bump,
every stored blob still valid) and each pinned by test.

- **`canvasAdd(buf, w, x, y, v)`** — the accumulate half of the canvas
  helpers: `cell += v` at exactly the cell `canvasSet` writes (same
  edge-clamped `floor(x·w)` addressing, same "of a non-array" runtime
  error, same degenerate-canvas no-op). Particle deposits and heatmap
  splats stop hand-rolling the read-modify-write. Returns the cell's
  **new** value, the way `+=` evaluates in JS.

- **Determinism, pinned.** Both generators are now documented as contract
  and asserted by test (`random_seed_pins_the_documented_sequence`,
  `prng_pins_the_documented_sequence`, both recomputing the sequence
  independently rather than snapshotting a run): `random()` is splitmix64
  (low 32 bits), `prng()` is xorshift32 13/17/5, state ← the seed's raw
  16.16 word, scaled `(r · max) >> 32`. The new **`randomSeed(s)`** seeds
  `random()`'s stream the way `prngSeed` seeds `prng()`'s — so a synced
  installation gets an agreed-on sequence without porting patterns from
  `random()` onto `prng()`. It returns the previous *seed* (the 64-bit
  state can't round-trip through an `Fx`, where `prngSeed`'s 32-bit state
  can, and that save/restore property is now tested too). **No existing
  sequence changed** — the algorithms are what Luxel always ran; they were
  just undocumented and unpinned, with a stale `TODO(oracle)` on `prng`.

- **`timeScale(s)` / `setFrameRate(fps)`** — in-pattern timing. `timeScale`
  scales the frame delta before it advances the clock, so `time()`,
  `beat()`, sync's `time_ms` and `beforeRender`'s delta all slow (or
  freeze, or speed up) together; negatives clamp to 0. `setFrameRate`
  holds the previous frame — same pixels, no pattern code run — until
  `1000/fps` ms of **real** time have accumulated, then hands
  `beforeRender` the whole interval, so delta-driven motion lands where it
  would have uncapped. Period clamped to 60 s.

  Both are enforced in `Engine::frame`, not per host, so firmware, WASM
  playground and CLI behave identically — no host plumbing, no
  QEMU/firmware exposure, and nothing to fake. Two honest consequences,
  now in docs/lang.md and docs/spec/vm.md: the host's output stage is
  untouched (LEDs/preview still refresh at the host's cadence re-sending
  the held frame, so the reported `fps` does **not** follow the cap — the
  cap throttles pattern evaluation, which is the expensive part), and
  the clock keeps running while frames are held so sync stays continuous
  (a sync *jump* is no longer misreported to the pattern as elapsed
  delta — `set_time_ms` now moves the render mark with it).

Verification: `cargo test --workspace` green; the 322-pattern `luxel check`
sweep over `library/` clean (compile + LXBC round-trip + smoke); web
`npm run build` (svelte-check 0/0) plus `e2e.mjs`, `device-e2e.mjs` and
`sync-e2e.mjs` all green — sync especially, since the delta path moved.
Autocomplete + hover for all four names driven in real chromium
(screenshot), including a pattern using every one of them running in the
playground preview. Clippy warning set identical to master. No firmware
source touched.

## 2026-08-16 — Pre-guard heap-regions panic root-caused + fixed, and a
## one-command QEMU test harness

Closed the last open thread on the WLED-takeover arc: the intermittent
`esp-alloc: Exceeded the maximum of 3 heap memory regions` panic that hit
the first Athom takeover (2026-07-26), fired before `ota::init`, and
self-healed on reboot — the one the installer page's beta banner was
waiting on.

**Root cause** (established under the QEMU harness, disassembly-confirmed):
the athom firmware makes exactly **two** `esp_alloc::heap_allocator!` calls
→ two `add_region()`s into esp-alloc's **three**-slot region array (the
`#[ram(reclaimed)]` 96 KiB + the 80 KiB region; the other arms are
`cfg`-gated out, and nothing in esp-hal/esp-rtos/esp-radio adds a region).
A clean boot fills 2 of 3 slots, so "exceeded 3" can only happen if the
slot array already holds stale `Some` entries when the allocators run —
which is what a flash-read flake corrupting the `HEAP` static's `.data`
`[None; 3]` initializer during the **ancient WLED bootloader's `.data`
copy** produces on the takeover boot. Same flake family as issue #35's
self-copy verify flake: intermittent, pre-`ota::init`, tied to the
via-WLED boot.

**The real danger** wasn't the intermittent panic (it self-heals via
`custom_halt`'s reboot) but a *deterministic* one: the boot-loop guard ran
**after** `ota::init`, past the allocators, so a pre-guard panic rebooted
forever without ever counting toward rollback — a bricked device that
never falls back to WLED.

**Fix**: `ota::preboot_guard`, armed **before** the heap allocators
(heap-free by necessity — stack buffers only, borrowing the `FlashStorage`
that is handed to `ota::init` once the heap is up). It increments the same
`LXBG` failed-boot counter the old guard used and, on the third
consecutive boot that never reached `boot_ok`, rolls back to the other OTA
slot (→ WLED on a takeover device). It fully replaces the old post-init
`boot_guard()`; `boot_ok` still clears the counter, and the takeover-retry
logic still zeroes it on a deliberate retry. Same increment/rollback
semantics, just early enough to catch a pre-heap panic. `stack-check`
clean (main-task stack unchanged at 29,492 B — no new heap statics; the
3 KiB partition-table buffer runs at the top of `main` with no WiFi NMI in
play). Builds green on athom + c3 + pixelblaze-v3.

**Harness** (the second half of the ask): `tools/qemu/run-all.py` is now
the single entry point for every emulator-backed test — it builds the
firmware + QEMU (separate out-links so they don't clobber each other),
autodetects the gitignored Athom dumps, and runs takeover (app1/app0/
fault) + the new `heap-regions-test.py` (selfheal/rollback), printing a
pass/fail summary (~36 s warm, 5/5 green). New QEMU tests slot into its
`suite` list. Also added: `tools/qemu/gdbrsp.py`, a dependency-free
GDB-remote client for driving QEMU's gdbstub from the Python harnesses
(the flake stays free of a `gdb`/pygdbmi dependency). All indexed in
docs/tools.md; root-cause writeup in docs/research/qemu-emulation-spike.md.

## 2026-08-16 — Takeover reboot-to-retry + attributable copy diagnostics
## (Gitea #35)

The bench conversion's one rough edge is now self-healing: a takeover
that aborts on anything possibly-flaky (self-copy verify, config wipe,
descriptor read/size) reboots and re-attempts, at most 3 boots total,
before settling into the provisioning AP — with the counter in byte 6 of
the boot-guard record (`LXBG` @0xC000) and each retry reboot zeroing the
failed-boot counter so a deliberate restart can't trip `boot_guard`'s
rollback-to-WLED (which would otherwise fire after two aborts, before
the retry cap). Exhaustion clears the counter, so a manual power cycle
starts a fresh budget; a successful takeover's config wipe erases it
anyway. Deterministic aborts (flash too small, image exceeds slot,
overlap) still settle immediately.

The copy loop also now says WHICH op failed — erase / write / read-back
/ data mismatch, with a mismatch classified further (differing-byte
count, first diff, and whether the sector read back still-erased 0xFF vs
stale data). The 2026-08-16 bench flake printed only "verify failed" and
left the failing stage forever unknowable; the next occurrence will be
attributable. The table write (the brick window) additionally gained
in-place retries ×3 with full read-back verify — a reboot can't rescue
that step, so it retries without one.

Tested hardware-free: the QEMU harness (harness-side per the isolation
rule) gained an opt-in write-fault injector in the m25p80 flash model
(`LUXEL_FLAKY_WRITE=<addr>:<len>:<budget>`, inert unless set), and
`takeover-test.py --inject-fault` drops both of boot 1's program
attempts at sector 0x10000 — byte-for-byte the bench failure signature
(erase lands, program doesn't, sector reads back 0xFF). 32 assertions:
boot 1a fails exactly like the bench did, reboots itself with `1/3
attempts used`, boot 1b converts cleanly, and the final flash state is
identical to the happy path (retry counter included). Both existing
variants re-verified green on the new firmware + patched QEMU.

The underlying silicon-side flake itself remains unreproduced/un-root-
caused — if it recurs, the new per-stage log lines are the evidence to
file the follow-up with.

## 2026-08-16 — The WLED takeover now has a hardware-free
## compose→boot→assert test (Gitea #43)

`tools/qemu/takeover-test.py` builds a 4 MiB flash in the exact state a
real Athom is in the instant WLED's OTA updater accepts a Luxel upload —
stock dump, configured WLED littlefs at 0x310000, `luxel-fw-ota.bin` in an
app slot, an otadata entry selecting it — boots it under the patched
emulator from this morning's spike, and asserts on both the takeover's
serial narration and the flash bytes it leaves behind: Luxel's partition
table at 0x8000, the `LXCF` record carrying the inherited SSID, the copied
image byte-for-byte at ota_0, WLED's littlefs untouched, and the
otadata/boot-guard state. One QEMU invocation covers both boots
(`software_reset()` reboots in-process); it exits the moment boot 2 prints
`wifi: creds from flash ("…")`, which is the inheritance proof and also
the last safe moment — the PHY panic's reboot would otherwise let a third
boot's `boot_guard` rewrite otadata.

Two variants, both passing against a **stock** image (isolation rule
holds: nothing here touches firmware source):

- `--slot app1` (default) — the realistic post-upload state, exercising
  the full 920 KiB self-copy where issue #35's verify flake lives. 23
  assertions, **12 s**.
- `--slot app0` — image already at the destination; the skip-the-copy
  path. 20 assertions, **1.5 s**.

Three things the first run taught us, now encoded as comments rather than
worked around: the `software_reset()` boot doesn't survive under QEMU (ROM
banner truncates, TG0 watchdog fires, and *that* reset loads Luxel —
cosmetic, pre-bootloader); the ESP-IDF bootloader **writes otadata back**
on the erased-otadata fallback (`seq=1`, `ESP_OTA_IMG_VALID`), so
"erased" was the wrong expectation there; and only the first 0x80000 of
WLED's app1 survives boot 2, because under the new table 0x210000 is
Luxel's `storage` partition and the pattern store legitimately reclaims
it. Also newly confirmed: `esp-storage`'s `FlashStorage::capacity()`
reports the true 4 MiB under emulation, so the takeover's flash-size
preflight is exercised, not skipped.

Espressif's QEMU fork is now a proper flake output
(`nix build .#qemu-espressif`), which was the standing TODO in
`tools/qemu/qemu-espressif.nix`; the `--impure` expression still works and
the test falls back to it.

## 2026-08-16 — QEMU emulation unblocked: stock firmware boots under
## emulation, takeover-in-CI is go

Resumed the morning's spike and killed the blocker — plus the two behind
it. Three root causes, all fixed QEMU-side in the nix derivation
(`tools/qemu/`), none of them touching the firmware:

1. **CPENABLE.** ESP32 silicon resets it to 0xff; QEMU leaves it 0, and
   nothing in ROM/bootloader/app ever writes it (disassembly-verified) —
   esp-hal relies on the silicon value. So the first float trapped
   `Cp0Disabled`, xtensa-lx-rt's `float-save-restore` `save_context`
   re-faulted on its own `rur.fcr`, and the guest looped silently in the
   double-exception handler. (espressif/qemu#154; PR #155 is unmerged and
   s3-only — no merged fix exists anywhere.)
2. **DPORT `PRO/APP_INTR_STATUS_REG_0..2` were never implemented.** That
   per-source pending bitmap is what esp-hal's level-interrupt dispatcher
   reads; it returned 0, so no handler ran, nothing acked, and esp-rtos
   livelocked in an interrupt storm on its scheduler-start interrupt.
   esp-hal is the *only* guest OS dispatching from those registers
   (IDF/Zephyr/NuttX use the Xtensa INTERRUPT sreg) — which is why this
   sat unnoticed. No prior art; we're first.
3. **TIMG level interrupts gated on `TIMG_INT_ENA`**, which is inert on
   ESP32/S2 silicon — the real gate is `Tx_LEVEL_INT_EN` in the timer
   config reg, what esp-hal writes — so the scheduler tick never fired.
   Folded in espressif/qemu#69 (an alarm already behind the counter now
   fires immediately instead of disarming).

Plus `tools/qemu/make-efuse.py`: a rev-3.0 eFuse image so **stock release
images** clear esp-hal's min-chip-revision gate with no build override.

Result: a byte-identical-to-shipping firmware image boots through engine
init, pattern storage and settings to the WiFi task, where the esp-radio
PHY blob faults on an unmapped peripheral alias (radio modeling is the
next frontier, and it's large). That's well past what the takeover test
needs — the takeover path runs before WiFi init — so
**compose→boot→assert takeover testing in CI is now unblocked**. Full
writeup, run instructions, QEMU-vs-silicon divergences and
upstream-filing candidates: docs/research/qemu-emulation-spike.md.

Standing rule recorded in the doc: the harness stays strictly isolated —
fixes go in `tools/qemu/`, never into the firmware.

## 2026-08-16 — QEMU emulation spike: takeover-in-CI is ~80% viable, one
## precisely-characterized blocker left

Jeremy asked whether the takeover could be tested in an emulator. Spike
findings (full writeup: docs/research/qemu-emulation-spike.md; nix
derivation for Espressif's QEMU fork: tools/qemu/qemu-espressif.nix):
the emulator builds reproducibly under nix (meson wrap subprojects
vendored, four one-line robustness patches to their RSA/AES device
models — all worth upstreaming), boots our real bootloader + partition
table + app from a plain flash FILE (perfect for compose→boot→assert
takeover tests), and esp-hal's chip-revision gate has a build knob.
Remaining wall: a guest double fault in the xtensa-lx-rt FPU
save_context path on the main task's first float (Cp0Disabled →
save_context faults at 0x400C200C) — hardware runs the same binary
fine, so it's a QEMU-vs-silicon divergence in coprocessor handling.
Next steps + worth-it assessment in the research doc; docs/tools.md
row added.

## 2026-08-16 — image-check: linked-feature guard (the //SIZETEST answer)

Jeremy asked what prevents the takeover class of regression. Answer:
`tools/image-check.sh` — asserts that load-bearing features are actually
LINKED into every built image by grepping for their distinctive serial
strings (dead-code elimination strips a feature's rodata along with its
code, so a commented-out call makes the strings vanish). Markers:
takeover, AP-mode provisioning, boot-loop guard. Wired into
build-esp32.sh (every local build) and release.yml next to the OTA size
guard (every board, every release). Test-of-the-test done: a clean
athom build passes; rebuilding with the call re-commented fails with
"MISSING marker 'takeover: foreign partition table'". actionlint +
shellcheck clean.

## 2026-08-16 (midnight bench session) — the WLED→Luxel takeover was
## SHIPPED DISABLED since v0.1.31; fixed, and the full conversion proven
## end to end through the installer page on real hardware

The first real via-WLED install (installer page → Athom restored to
stock WLED 0.13.2) exposed it: `takeover::maybe_takeover()` in main.rs
has been commented out as `//SIZETEST` **since commit 2e5d6ff — the
v0.1.31 commit literally titled "takeover always-on"**. A size
measurement toggle that never got reverted. Every release (v0.1.36,
v0.1.37) advertises WLED takeover and ships it dead; nothing noticed
because no via-WLED install had been run since the July 26 TAKEOVER=1
feature builds. Uncommented (with a do-not-disable comment), athom
image 938,240 B (107 KB OTA margin), stack-check clean.

**Round 1 (v0.1.37 release image, takeover dead) documented what
"dead" looks like**: one silent first boot (zero serial bytes, RTC-WDT
reset), then Luxel running absurdly from WLED's app1 under WLED's
partition table, credless → provisioning AP. Also proved: WLED
**Improv-serial provisioning persists both cfg.json and wsec.json**
(WLED rejoined after a cold power cycle) — the bench can provision
stock WLED with zero button-holds (`scratchpad improv script; packet
format in the session log`; worth a tools/ script if we do this again).

**Round 2 (fixed image, via the page's bundled mode)**: takeover ran —
foreign table detected, **WiFi inherited from WLED's littlefs**
("MOMCorp Intranet"), 920 KiB self-copy... first attempt hit an
**intermittent verify failure on the first sector** (both in-boot
retries), aborted exactly as designed (WLED table untouched) into the
provisioning AP; the next power cycle's re-attempt ran clean end to
end: otadata/nvs wiped, table rewritten, reboot to ota_0, **joined the
LAN on the inherited creds at the same DHCP address**, page detected
Luxel and pushed the web app — "All done — open your Luxel 🎉". Device
end state: Athom = Luxel v0.1.38 (local build = master + this fix),
ota_0, Luxel partition table, fresh store, full web app, 122 fps.

Open (documented in docs/wled-migration.md's beta list): the
intermittent first-sector verify flake; the takeover has no
reboot-to-retry after an abort (device waits in AP mode for a power
cycle); the two first-boot anomaly classes. The installer page's
timeout guidance ("power cycle helps") turned out to be literally the
right advice.

Harness note: both bench runs crashed the puppeteer driver mid-wait
(CDP WaitTask against a live device; the fake-wled e2e has never done
this) — unexplained, low priority, the device outcome was unaffected.

## 2026-08-16 — v0.1.39: MQTT topic → pattern events (`luxel/<id>/event`)

The follow-up v0.1.38 left on the table, and the last leg of the event
surface: HA automations (or anything with an MQTT client) can now drive
`readEvent()` patterns directly.

- **Topic**: `luxel/<id>/event`, command-only (no HA entity/discovery —
  it's an automation target, not a control). Subscribed by the firmware
  MQTT task and the mirror alongside the existing three; also added to
  `hamqtt::command_topics` (which, note, neither consumer actually calls
  — both keep inline lists; all three places updated).
- **Payload**: text, one event per line — `type [x [y [value]]]`,
  whitespace-separated decimals; x/y default 0, value defaults 1 (an
  automation can publish just `"1"`), junk lines skipped, 32-event batch
  cap. Parsed by `hamqtt::parse_event_lines` with a hand-rolled
  integer-math decimal→Fx parser (keeps the no_std path off core's
  dec2flt; that ~19 KB table-heavy machinery is in the firmware image
  only incidentally today). Both feed the same queues as
  `POST /api/events`.
- **Harness**: `tools/mqtt-e2e.mjs` (docs/tools.md) — a REAL mosquitto
  (now a dev-shell flake dep) + the mirror: connect, retained
  availability, event → pixels red, value scaling, junk tolerance,
  bare-type defaults. 8/8. This finally gives the MQTT bridge an
  automated check; v0.1.19's verification was a manual procedure.
- Mirror refactor: the inline drop-oldest push in `/api/events` became
  `queue_events`, shared with the MQTT path.

Verified: full workspace tests (hamqtt gains topic/parse/decimal suites),
mqtt-e2e 8/8, serve-e2e, stack-check clean, OTA image 941,952 B
(106,624 B margin). Measured attribution: current master builds the
SAME 941,952 with or without this diff — the mapping itself costs ~0
flash. (The v0.1.38 entry's 922,496 came from that branch's own
worktree build and doesn't reproduce on master; cause not chased —
margin is ample either way.) On-device + real-HA hop: queued in
UNTESTED.md (broker details live in agent memory; the wall unit is
offline).

Same-day follow-up: **docs/mqtt.md** — the MQTT surface finally has a
user-facing reference (enabling, HA discovery entity list, the full
`luxel/<id>/…` topic/payload table, event-topic grammar + an HA
automation example, brightness-scale note, mqtt-e2e pointer). Until now
the only topic list lived in hamqtt.rs source comments; README, lang.md,
and the hamqtt module doc now link to it.

## 2026-08-15 — v0.1.38: external event injection (`readEvent`) + webui.md dedusting

The ideas.md ★★★ item, done end to end (no-device work by design; the
firmware side is mirror-verified, on-device soak deferred until a device
is back online):

- **Engine**: a 32-slot drop-oldest FIFO on the VM (fresh per pattern —
  a switch clears it) + builtins batch 4: `eventCount()` and
  `readEvent(out)` filling `out[0..4] = [type, x, y, value]` (returns
  1/0; non-array or short `out` is a clean vmerr, and only when an event
  was actually there to deliver). `Engine::push_event` try_reserves the
  queue once, dropping events instead of erroring on a starved heap.
  Semantics pinned in tests: FIFO order, drain idiom, overflow keeps the
  newest 32, error cases.
- **Wire**: `"EV1\0" + u8 count + count × 4×i32-LE raw 16.16` — parser +
  builder in `luxel_core::netin` (round-trip + reject tests). `POST
  /api/events` accepts it on the firmware AND the CLI mirror; the render
  task/loop drains a shared batch buffer between frames, same shape as
  sensor frames.
- **Web**: click/drag anywhere on the preview (strip, waterfall, grid,
  or map canvas) injects a type-1 event with normalized x/y into the
  local WASM engine (`lx_push_event`), and in device mode also forwards
  it to the strip (batched ~50 ms per POST). Crosshair cursor +
  `touch-action: none` on the canvases; hover docs + autocomplete for
  both builtins.
- **Patterns**: Typing Heatmap 2D + Crosshair Pulse 2D now consume real
  events; their phantom generators go quiet for 4 s whenever real input
  flows (both soak 300 frames clean).
- **Docs**: lang.md "External events" section; README feature bullet;
  ideas.md item marked DONE. Folded in: webui.md stale-line fixes (3D
  mapping, MQTT/HA, AP-mode were all still marked open despite shipping
  in v0.1.19/v0.1.22/Phase 4).

Follow-up left on the table: an MQTT-topic → event mapping in the
firmware bridge (the generic surface now exists), and swapping the
sound-reactive corpus patterns' trigger controls over where it helps.

## 2026-08-15 (late) — WLED→Luxel installer page (issues #2/#9): one static
## page drives the whole takeover, riding the release pipeline

`web/flash.html` — a wizard that converts a WLED device to Luxel over the
air: probe the device, pick/auto-detect the board, upload the release OTA
image through WLED's own `/update`, watch `/api/status` for the Luxel
signature (3 min budget, with AP-mode/OTA-passphrase/first-boot-panic
troubleshooting on timeout), then push the LUXA web app. Svelte entry
`web/src/flash/` (+`lib/releases.ts`, `lib/device.ts`), second Vite page
next to the playground — so it ships in the web-dist tarball AND onto
every Luxel device's assets partition (+~11 KB gz; a Luxel on the LAN is
the friction-free plain-HTTP origin for converting the next device).

The load-bearing measurement (docs/wled-migration.md): **GitHub's
release-asset downloads send no CORS headers on any hop** (checked
`browser_download_url` and the API octet-stream redirect, incl.
`Origin: null`), so a browser can only fetch firmware same-origin. Hence
two firmware-source modes: *bundled* — the release workflow now composes
a GitHub Pages site (whole web dist + `firmware/` with per-board OTA
bins, LUXA, `manifest.json` via new `web/tools/gen-flash-manifest.mjs`),
fully automatic. (Same-night follow-up after Jeremy enabled Pages: the
v0.1.37 tag predated this work, so deployment moved from a release-job
step to a standalone `.github/workflows/pages.yml` — master-push /
release-published / manual triggers, latest-release firmware via the
GitHub API — which also means installer fixes deploy without waiting
for a firmware release.) *github* mode — API metadata (CORS `*`) +
download-link + file-picker fallback for self-hosted/device-served
copies. WLED-side quirks handled per docs/wled-migration.md: 0.13 has no
CORS (opaque no-cors probe → manual board pick, pointer at `/json/info`),
multipart `/update` POST is CORS-safelisted so it sends everywhere,
esp8266 → hard stop, s2/s3 → "no builds yet", https→LAN mixed content →
Chromium LNA `targetAddressSpace` hint + manual-steps fallback. The page
wears a visible **beta** banner until the pre-guard first-boot
heap-regions panic is root-caused.

Verified per verify-webui: `web/tools/flash-e2e.mjs` in real chromium
against new `web/tools/fake-wled.mjs` (a fake WLED that "reboots into
Luxel" after /update) — 14/14 checks across 4 scenarios (CORS-less full
auto run with byte-counted upload + byte-exact LUXA landing; 0.14-CORS
arch detect + c3 board filtering; esp8266 stop; github-mode file-picker
run), screenshots reviewed. actionlint clean on the workflow. No
hardware touched (Athom in use by another session; the takeover
mechanism itself was hardware-proven 2026-07-26). Untested on real
hardware end-to-end as a *page*: needs a WLED device on the bench —
noted for the next Athom restore-to-stock window.

**Site went live the same night** (Jeremy enabled Pages):
https://googlebot42.github.io/luxel/flash.html, deploying via the new
pages.yml, serving the v0.1.37 firmware set. Smoke-testing the LIVE
site caught a real bug and a spec rename (PRs #27/#28): Chromium
hard-fails a fetch whose `targetAddressSpace` hint mismatches the
target's real address space, and the current LNA spec's value is
`"local"` (PNA's `"private"` renamed) — the hint is now derived from
the target host (RFC1918/.local only; loopback/public get none).
Measured and documented: headless chromium DENIES local-network access
outright (no prompt; CDP grant ineffective in Chromium 150), so from
the https site the page correctly degrades to its manual-steps UX —
the automatic flow from https needs a real user's headful Chrome
permission prompt, still unverified. After the Athom freed up:
current assets pushed (installer included), and the **device-served
page verified against the real device** — resolves v0.1.37 from the
GitHub API in github mode and detects the Athom as already-Luxel.
Remaining for the full conversion proof: stock-WLED restore (needs
Jeremy's button-hold) then the page end to end.

## 2026-08-15 (late night) — v0.1.37: flash-wear fix — playlist swaps
## write nothing

The wear finding from the fairness session (same day, below) is fixed:
`store_current` used to erase the same raw-slot sectors on EVERY pattern
swap, so a 5 s playlist burned ~17k erase cycles/day against a ~100k NOR
spec — days-to-weeks to spec-exhaustion on the header sector. Now
**library swaps (playlist advance, activate, MQTT select, boot resume)
write nothing at all**: their source + blob already live in the pattern
store, and read-back serves from there.

Mechanics: the library id rides IN `Msg::Code`/`Msg::Crossfade` ("" =
ad-hoc) and the RENDER task stamps id + hash + read-back location
atomically at the swap — which also fixes a latent race, since every
sender used to `set_current_pattern_id` AFTER queueing (a fast render
task could bind the previous item's id to the new content). New
`SrcLoc::Library`/`BcLoc::Library` variants: `/api/pattern` and the sync
envelope serve via `source_of`/`bytecode_of` into a transient Vec
(exact-length framing kept — truncate/pad on a mid-session re-save or
delete, mirroring `stream_flash_readback`), and engine rebuilds fetch
the store's CURRENT blob (not the snapshot length — a re-save is the
truth). The slot write remains only for ad-hoc `/api/code` pushes and
sync adoption, and now logs `slot write (ad-hoc…)` on serial — a
tripwire line: seeing it on playlist advances means the fix regressed.

Verified on the Athom (v0.1.37, serial captured): **0 slot writes across
~10+ churn swaps** and exactly one from a deliberate ad-hoc push;
`/api/pattern` byte-identical to the playing item's library copy;
`.lxp` envelope framing computed == actual; pixel-count 300→150→300
mid-playlist rebuilds live off the store; activate → OTA-reboot resumes
the activated pattern; 3/3 clean cold loads under churn; full hw-bench
soak (results in docs/bench-report.md). Stack-check clean, C3 builds.

## 2026-08-15 (night) — CI/releases (issue #8): Gitea-tags → GitHub-builds
## pipeline, modeled on open-nanokvm-pro

Jeremy created github.com/GoogleBot42/luxel and pointed at
open-nanokvm-pro as the reference; the same architecture now exists here
(docs/releases.md is the canonical writeup):

- **Gitea stays the source of truth** (Tailscale-only); the push mirror
  Jeremy configured already replicates to GitHub (verified: the mirror
  carried a merge within minutes).
- **`.gitea/workflows/cut-release.yml`** — tag-cutting from the Gitea UI.
  Unlike onkp there is NO version-bump commit: luxel's version is bumped
  in the shipping PR (firmware/Cargo.toml), so cut-release only validates
  tag == Cargo.toml and pushes the tag. `tools/release.sh` is the
  local/agent fallback (tea API path — the agent has no direct push).
- **`.github/workflows/release.yml`** — GitHub-only (server_url guard,
  since Gitea Actions also reads .github/workflows). On a mirrored
  vX.Y.Z tag: nix-builds all four board variants (the flake's existing
  luxel-fw-* packages: ELF + merged full image + OTA image), builds the
  web app + LUXA, composes full images (LUXA dd'd at 0x310000, same as
  build-esp32.sh image), guards OTA size ≤ 1 MiB and LUXA ≤ the assets
  partition, and publishes: per-board `-ota.bin` + `-full.bin`, the
  `.luxa`, a static `web-dist` tarball (issues #10/#11 fodder), an ELF
  bundle for backtrace decoding, and sha256sums.

Two properties fell out for free and are now documented invariants:
release firmware is **credless by construction** (pure nix eval can't see
creds.env → AP-mode provisioning is the setup path) and release web
bundles are **corpus-free by construction** (fresh clone has no corpus/ →
gallery builds from the clean-room library/ only; rehearsed: 5-file LUXA,
615 KB vs the dev checkout's 6-file 930 KB).

Everything rehearsed locally before the workflows were written: all four
`nix build .#luxel-fw-*` variants build (Athom OTA 916 KB, fits), a
pristine clone builds the web app (after `mkdir -p web/public` — the
fresh-worktree gotcha, now in the workflow), the dd-composition
byte-verified, actionlint clean.

## 2026-08-15 (evening) — v0.1.36: flash-access fairness under playlist
## churn — the driver never leaves the global

The "flash churn starves flash users" finding from this morning's session
is fixed, and the root cause turned out to be one design flaw with three
disguises: `patterns::store_current` (the per-swap read-back persist,
v0.1.34) **took the flash driver out of the global** (`take_flash`) for
its entire multi-page erase/write burst (~200-500 ms per swap). Every
`with_flash` user read busy for that whole window — asset pushes failed
("flash write failed", assets.rs), served assets truncated mid-body
(`read_chunk` → None mid-stream), and `/api/ota` returned the misleading
"update already in progress" (`ota::begin` finds no driver and can't tell
absent-because-store from absent-because-OTA).

**The fix** (firmware v0.1.36): `write_raw` now borrows the driver per
erase/write op via `with_flash` — the same borrow-per-op shape the OTA
and assets writers already soak-proved (2026-07-27: take-for-the-burst
crashed 5/5, borrow-per-op clean 4/4) — with the existing 1 ms yields
between ops now doubling as real windows for waiting HTTP tasks to grab
the driver. `store_current` also skips while an OTA is active (new
`ota::ota_active()` accessor; `with_flash` deliberately doesn't check it
because OTA's own writes go through it). Contention inverts correctly
now: a store transaction (`take_flash` — user-initiated save/playlist
edit) stealing the driver mid-burst aborts the background persist
(read-back degraded until next swap, never a panic), not the user's op.

**Verified on the Athom under a 5 s three-item playlist** (the exact
morning repro): asset pushes **6/6** (was 1/6), **20/20** identical
sha256 on a 315 KB served asset (was truncating), **OTA accepted and
clean-rebooted mid-churn** (was rejected), coldload.mjs **5/5 clean**,
playlist auto-resumed after the OTA reboot, zero panics on live serial
(/dev/ttyUSB0 captured throughout). Deploy tooling's
stop-playlist→push→resume dance is no longer needed on v0.1.36+
(deploy-device skill updated; keep it for 0.1.34/35 devices — the dev
unit is still on v0.1.34 and OFFLINE, push v0.1.36 when it's back).
Stack-check clean; clippy delta zero (the 5 pre-existing `bufs` macro
errors on the c3 target are untouched).

**New finding while verifying — per-swap flash WEAR** (ideas.md): the
persist erases the same slot sectors every swap; a 5 s playlist is ~17k
erase cycles/day against ~100k NOR spec. Fix sketch (carry the library
id in Msg::Code/Crossfade; Library read-back variants; slot write only
for ad-hoc pushes) is written up in ideas.md — deliberately NOT bolted
onto this change (touches all six Msg senders + resume/sync semantics).

Also repaired on the Athom: stored pattern "Doom Fire" (5eed1e55) had
lost its bytecode chunks (playlist skipped it every cycle with
"has no bytecode"; confirmed at idle flash = pre-existing data damage
from the morning's contention chaos, not the new code). Re-saved via
lxp.mjs — same id, plays clean now.

Device end state: Athom on ota_0 = v0.1.36, current assets, empty
playlist stopped (as found), zero panics.

## 2026-08-15 (later) — v0.1.35: cold-load hardening end to end — fetch
## gate + slot reclaim + a pattern-store OOM found by serial; pool stays
## 3 (Chromium needs it), 2 becomes the small-chip profile

The agreed "web pool 3→2 via webui tolerance" follow-up (2026-07-29)
ran its full course and ended somewhere better than planned: the client
tolerance shipped, three real firmware bugs fell out of the verification
gauntlet (one a crash-on-every-page-load), and the pool question got a
definitive answer — **Chromium needs 3 sockets at cold navigation**, so
2 slots is now a `small-chip` cargo feature rather than the default.
Acceptance: **10/10 clean cold chromium loads** on the Athom (fresh
profile, cache off, ~8.6 s to a fully-booted device console, zero failed
requests, zero panics), plus 5/5 with a 5-second playlist churning flash
the whole time. Harness: new `web/tools/coldload.mjs` (docs/tools.md).

**Web (both e2e suites green):** new `web/src/lib/fetchgate.ts` — one
global gate for every fetch the app fires (assets AND API): 2 in-flight
max, backoff-retry on refused (6 tries ≈ 10 s), a 30 s per-attempt
deadline, and the slot is held until the BODY completes, not just
headers — fetch() resolves at headers, and 300 KB gallery bodies kept
device sockets busy for ~6 s afterwards, starving the boot handshake
(caught by per-request tracing). Bodies are buffered inside the gate and
returned as a detached Response. DeviceSession now delegates to it; the
device probe abort went 1.5 → 8 s.

**Firmware fix 1 — pattern-store OOM panic (the big one, found via
serial):** `patterns::read_source`/`read_bc` allocated
`count × CHUNK` bytes INFALLIBLY from a stored TOC record. The Athom had
a corrupt record (playlist wire-format bytes as its name, chunk count 32
= 4× the writer's cap) → every `GET /api/patterns/<id>` tried a 120 KB
alloc → OOM panic → reboot, i.e. **the device crash-rebooted on every
web-app cold load**, on both slots, and the boot guard ping-ponged the
slots — which masqueraded as everything else for hours. Both readers now
reject counts beyond the writer's own caps (MC/MC_BC) and try_reserve.
The corrupt record was deleted (id 2112e1ab). How it got there is
unproven — likely a torn write during the flash-contention chaos below.

**Firmware fix 2 — pool-slot reclaim (`QuickCloseSocket`):** picoserve's
graceful shutdown waits for the CLIENT's FIN bounded by
`timeouts.read_request` — our 45 s, sized for OTA bodies — and browser
socket pools sit on connections after our FIN. Measured: two idle raw
TCP connections wedged BOTH slots ≥60 s. server.rs now wraps the socket
in `QuickCloseSocket`: inline staged shutdown (close → 2 s bounded
discard → 2 s bounded flush → terminal abort), per-slot lifecycle stages
exposed as `"web":[…]` in /api/status, and serial lines on grace
expiry/serve errors. Full 2-slot wedge now self-heals in ~12 s; a
teardown is typically 50 ms–2 s.

**The pool answer:** with everything above fixed, 2-slot cold loads
still failed ~randomly at the NAVIGATION: serial-correlated to Chromium
opening ~2 sockets at cold nav (speculative preconnect + the nav; the
preconnects win both slots, the nav SYN is refused — and
`--disable-features=NetworkPrediction` doesn't stop it). No page code
can fix what happens before the page exists. So `WEB_TASK_POOL_SIZE`
stays 3 by default (esp32 heap static back at 80 KB, .stack 29,716 B
measured) and the new **`small-chip` feature** takes pool 2 + 88 KB heap
(.stack 30,540 B) — reclaiming ~17 KB for the S2/C2 tier at the cost of
an occasionally-refused FIRST navigation (reload works). Both variants
stack-check clean; C3 builds.

**Finding, documented not fixed — playlist flash churn starves flash
users** (v0.1.34's per-swap flash persist): with a 5 s playlist running,
asset pushes failed 5/6 ("flash write failed"), /api/ota rejected
("update already in progress"), and served assets truncated mid-body.
Deploy procedure now: stop playlist → push → resume (in the
deploy-device skill). With the client's retry+deadline the cold-load UX
under churn is clean (5/5), but the write-path starvation stands — a
fairness mechanism (or swap-write backoff while a request is active) is
the real fix, backlogged.

Device end state: Athom on ota_0 = v0.1.35 default (3 slots, all fixes),
current assets, playlist playing as found. The dev unit was OFFLINE at
session end and still runs v0.1.34 — push v0.1.35 to it when it's back
(the pattern-store OOM fix matters everywhere). /api/status gained
`"web"` slot stages. New tool: web/tools/coldload.mjs.

## 2026-08-15 — Claude Code setup bootstrapped from session history

The repo now carries its own agent configuration, mined from six weeks of
memories, session transcripts, and UPDATES.md itself:

- **CLAUDE.md** — orientation, toolchain norms (nix-flake-only, Rust-first,
  strict TS), environment boundaries (container/no-serial, one device + one
  oracle), autonomy grants, hard rules (clean-room corpus, black-box oracle,
  no secrets in tracked files), verification norms, and tripwires.
- **.claude/rules/** — path-scoped must-know risks: `firmware.md` (stack/heap
  footguns), `vm-bytecode.md` (BUILTINS append-only), `web.md` (browser
  verification, Svelte/e2e gotchas, terminology), `corpus-cleanroom.md`,
  `oracle.md` (websocket-wedge et al.).
- **.claude/skills/** — procedures: `deploy-device`, `athom-rig`,
  `verify-webui`, `worktree-setup`, `cleanroom-port`, plus meta-skills
  `fetch-work` / `unblock` / `reflect`.
- **docs/firmware.md** gains a "Stack & heap invariants" section — the
  permanent home for the v0.1.4 / v0.1.19 / v0.1.31-33 memory-model lessons
  (leftover-DRAM stack, task-futures-are-statics, FlashStorage::read bounce
  buffer, measure-don't-estimate, heap economics, WS2812-needs-DMA).
- **docs/wled-migration.md** gains the serial-rig facts (ttyUSB0 ownership,
  single-reader rule, no-DTR/RTS → `--before no-reset --after no-reset`,
  verify dumps twice) and the restore command now carries both no-reset
  flags. Flag spelling verified against the installed esptool v5.3.1 /
  espflash v4.4.0 (hyphenated).
- Cleanup: deleted the untracked `tools/corpus/cleanroom/` scratch dir after
  verifying all 283 specs are byte-identical to the committed
  `docs/pattern-specs/`; gitignored `.claude/settings.local.json`.

Everything cited was verified against the tree during writing; writer
subagents corrected several stale memories along the way (gen-gallery
doesn't read `last-report.json`; the corpus symlink is no longer needed for
the e2e tile assertion; esptool flag spelling).

## 2026-07-27 — v0.1.34: current pattern lives in flash (~40 KB heap
## back) + decode churn fix — Music Sequencer V3 runs at 300 px

Per-allocation profiling (new host harness, below) showed the biggest
pattern's RAM footprint was 62% bookkeeping: the PATTERN_SRC/PATTERN_BC
read-back copies (22.3 + 17.8 KB for "Music Sequencer - for V3 ONLY"),
which nothing on the render path ever reads. Three changes, all
verified on the Athom at 300 px:

- **Decode pre-pass** (luxel-core): `prog_code` is reserved once from a
  header-only sum instead of per function. try_reserve_exact per
  function reallocs the whole buffer each time — 157 KB of copy churn
  for 9 KB of tables on the big pattern (measured), and the realloc
  ladder fragments the heap enough to starve later 17–22 KB contiguous
  reservations (the silent src/bc shedding seen on-device). Now: one
  9,170 B allocation; total decode churn 213 → 65 KB; resident
  byte-identical.
- **Flash-resident current pattern**: on swap the source + blob are
  written as RAW PAGES into the reserved upper half of the storage
  partition (header page written last), and only tiny location enums
  stay in RAM (shared::SrcLoc/BcLoc; boot default = rodata, zero heap
  and zero flash). GET /api/pattern and /api/pattern.lxp stream from
  flash with the FlashAsset discipline (4 KiB reads + Timer yields);
  the engine rebuild reads a transient fallible Vec. A first cut as
  reserved-key map items made every read a 512 KiB NoCache scan whose
  cache-off bursts starved WiFi — raw pages fixed that. Byte-integrity
  verified on-device (src and envelope sections cmp-exact against the
  uploaded originals). /api/status gains `"src"/"bc"` booleans and a
  serial log line when a swap's flash write fails — the shedding that
  used to be silent is now observable.
- **Envelope dropped before the engine builds** (main.rs): the ~40 KB
  upload buffer is freed after the flash persist, so it no longer
  counts against the array budget or the post-load floor check.
  On-device before/after at 300 px: Music Sequencer V3 was REJECTED
  456 B under the floor (v0.1.33 stock: 13 KB under); now it RUNS with
  ~70 KB free. Steady-state heap while running it: was impossible,
  now 70,608 B free.

Known issue (documented, not fixed): back-to-back big readbacks
(/api/pattern twice with no gap) intermittently time out (000, retry
succeeds) — NOT present on v0.1.33's RAM path (A/B'd on hardware).
Slot/keep-alive recycling under load is suspected; a live DDP stream
(LedFx?) was hitting the bench device during later tests, which muddies
attribution. Mid-stream flash failures now PAD the body to the promised
Content-Length instead of truncating — a short body desyncs the
connection and wedges the pool slot until the write timeout (observed
as cascading dead requests; the padding closed that class).

New host tooling: `crates/luxel-cli/tests/allocprof.rs` — dhat-based
per-allocation profile of the device lifecycle (resident / peak / churn
per callsite, driven by AP_SRC/AP_PIXELS/AP_BUDGET env vars), validated
against live hardware to ~2% (Doom Fire predicted 14.7 KB resident
delta, device measured 14.4 KB). `examples/mkenvelope.rs` packs LXP1
envelopes for /api/code. The playlist boot-resume path now also writes
the flash slot on its first swap — soak big-pattern resume before
trusting it hard.

## 2026-07-27 — v0.1.33: main-task stack was 18 KB, not 27 — measured,
## fixed (deterministic /api/wifi + page-load panics)

Jeremy hit a hard-reproducible stack-guard panic on the Athom: every
`GET /api/wifi` (and every load of `/`, whose page JS calls it) died
with "write to the stack guard value on ProCpu". The panic registers
told the whole story: SP = `0x3FFDBA60`, 60 bytes above the `.stack`
section's floor (`0x3FFDBA24`), PC inside
`esp_rom_spiflash_read_status` — main-task stack exhaustion during a
request-context flash read (`read_wifi` → `assets::read_chunk`), the
classic failure mode, back again.

Root cause: v0.1.31's heap retune was arithmetic on an estimate that
was wrong by ~17 KB. The comment budgeted the 3-slot web pool at
"~9 KB of static task arena" and claimed ~27 KB of leftover stack;
`readelf -S` on the shipped ELFs says `server::web_task::POOL` is
25,968 bytes (~8.6 KB **per slot** — each slot embeds picoserve's
whole response-path future) and `.stack` was 18,140 B in v0.1.31 /
17,884 B in v0.1.32. That's ~2 KB above the empirically measured
15.6 KB overflow point — one WiFi NMI frame landing on top of a flash
read at picoserve depth eats it. (This also retroactively explains
v0.1.31's 5/5 `/api/ota` crash-mid-erase-burst on this device.)

- **esp32 heap static 92 KB → 80 KB**: `.stack` measured 30,172 B in
  the new image (the "31 KB ran clean for weeks" zone). Runtime
  heap_free on the Athom: ~95 KB — comfortably above resume.rs's
  `stored×2 + 24 KB` pre-flight and the soak's observed peak.
- **Comment rewritten around the measurement**, with the rule that
  should have been there all along: `.stack` in `readelf -S` is the
  ground truth — measure, don't estimate. `tools/stack-check.sh` now
  enforces it: it prints the linked `.stack` size and fails below a
  24 KB floor (per-frame budget check unchanged).
- **Verified on the Athom**: OTA'd 908 KB to ota_0, then 5/5 clean
  `GET /api/wifi`, repeated `/` loads, api/output/clock/brightness
  all stable. Also pushed the 930 KB playground bundle (the assets
  partition was empty after the serial full-flash — the minimal
  fallback page was what Jeremy's browser was loading), which
  doubles as a clean 227-sector flash-write soak on the new stack.

## 2026-07-27 — v0.1.32: WS2812 goes DMA (fixes erratic colors) + OTA
## writer parity

Jeremy's first real WS2812b test (300 px on the Athom) showed erratic
colors on a plain rainbow. Root cause, confirmed in esp-hal source: the
blocking `Spi::write` splits every frame into 64-byte FIFO transactions
with a busy-wait between them. 64 B = 512 SPI bits, and WS2812 encodes
each LED bit as 3 SPI bits — so every chunk boundary lands mid-symbol
and corrupts a bit (43 boundaries per 300-px frame), and a WiFi
interrupt in the gap stretches it past the strip's latch threshold
(partial-frame latch, rest of the frame re-addresses from pixel 0).
SK9822 has a clock line and never cared — which is why nothing showed
until the first single-wire strip.

- **SPI output is now DMA** (`SpiDma`, blocking mode): one continuous
  gap-free transfer per frame on both chips (esp32: `DMA_SPI2`, c3:
  `DMA_CH0`). The encode buffer became a `u32`-backed `EncodeBuf` —
  the DMA driver only streams a slice zero-copy when it's 4-byte
  aligned with a length that's a multiple of 4 (classic-ESP32 rule);
  anything else bounces through a 4-byte internal buffer, i.e. the
  exact re-chunking the DMA is here to prevent. Max frame (2048 px
  WS2812 = 18.5 KB) fits one transfer (driver cap 32,736 B).
- **OTA writer rebuilt to match the assets writer** (borrow-per-op via
  `with_flash` instead of taking the driver for the whole upload; an
  `OTA_ACTIVE` flag now provides the in-progress guard). Motivation:
  on the Athom, `/api/ota` crashed the device (CPU exception or silent
  lockup mid-erase-burst, panic-reboot or power-cycle to recover)
  **5/5 attempts**, while the line-for-line-identical assets upload
  path was clean 4/4 (930 KB, 227 sector erases each — including with
  the engine frozen). Exonerated by experiment: the Freeze (assets
  push with engine provably frozen at 20 fps = clean), `ota::begin`'s
  partition-table reads (junk-image OTA runs them and rejects cleanly,
  device stays healthy), esp-storage's per-op critical section (active
  in both paths, verified via cargo tree), executor-idle/WAITI
  interleave (frozen assets push = clean), upload pacing (8 KB/s
  throttle still crashed). The one structural delta left was
  taken-bare vs borrowed-per-op flash access, so the OTA writer now
  uses the empirically-bulletproof shape. **Verified on hardware**:
  after a serial full-flash install (which also upgraded the Athom
  off the Arduino-2019 bootloader to espflash's ESP-IDF v5.5.1 one),
  a full 908 KB OTA self-push wrote all 222 sectors, activated, and
  rebooted into ota_1 cleanly — the first successful /api/ota on this
  device ever. (Caveat: bootloader and writer changed together, so
  the fix isn't isolated to one of them.) Residual cosmetic flaw:
  the success response still often dies in the reboot window, so
  ota-push.sh reports failure on a push that actually landed — check
  /api/status version/slot.
- **Known hole, documented not fixed**: `commit` activates on
  `written == Content-Length` alone, so a *truncated prefix* of a real
  image (valid header magic + app-desc) activates and hands the
  problem to the bootloader's image validation / boot-loop guard. Real
  espflash images from ota-push.sh are never truncated; my probe files
  were. A structural end-of-image check would close it.
- Rig lesson recorded: /dev/ttyUSB0 serial works fine — but only ONE
  reader at a time; a forgotten background `cat` silently steals every
  byte and looks exactly like dead serial.

## 2026-07-26 — v0.1.31: takeover always-on + WiFi inheritance + the
## browser-starvation fix

Follow-through on the takeover work below, all verified on the Athom:

- **Takeover is now always compiled in** (feature flag removed): a no-op
  256-byte table check per boot, ~17 KB of image (module + littlefs
  reader + embedded table), and it turns partition-layout changes into
  ordinary OTAs — any future partitions.csv change self-installs on the
  device's next boot. Two new guards for that generality: a flash-size
  preflight (never write a table past the end of the chip) and a
  src/dest overlap check (a resized app slot must never erase the code
  it's running from).
- **WiFi inheritance**: during a takeover the device now mounts the
  outgoing WLED's littlefs read-only (`src/wledfs.rs`, a dependency-free
  ~330-line littlefs v2 reader) and carries SSID (cfg.json) + password
  (wsec.json) into Luxel's own creds record — the device reappears on
  the user's network without provisioning. Factory-fresh WLED (nothing
  to inherit) falls through to the provisioning AP as before. The reader
  is host-tested against real device dumps via `tools/wledfs-check`
  (cfg.json read back byte-identical to an HTTP-fetched reference).
- **"Cannot reach device" in the web UI, root-caused and fixed twice
  over**: the ESP32's 2-socket HTTP pool meant a browser's parallel
  fetches TCP-refused *each other* (headless-chromium repro: 8/10
  parallel API calls refused; even `luxel.wasm` failed during page
  load). Fix 1: web pool back to 3 slots — slots cost ~8 KB heap now,
  not the old 32 KB static (the ~9 KB task-arena growth is repaid by
  trimming the esp32 heap 96→92 KB, keeping the main stack ≈ 27 KB;
  stack-check clean). Fix 2: `device.ts` routes every API call through
  a wrapper capping in-flight requests at 2 with backoff-retry on
  connection-refused. Verified: 3× cold loads on hardware in real
  chromium, zero failed requests, editor synced.
- docs/wled-migration.md: mechanism walkthrough + working notes for the
  future installer page (chip detection via WLED's /json/info, its CORS
  limitation, artifact layout, the open first-boot-panic issue).

## 2026-07-26 — WLED → Luxel OTA takeover (proven on the Athom)

New `wled-takeover` firmware feature (`TAKEOVER=1 ./build-esp32.sh`,
`firmware/src/takeover.rs`): a Luxel app image uploaded through **WLED's
own OTA updater** self-installs the Luxel partition layout. WLED writes
the image into one of its 1.5 MB app slots and boots it (ESP32 apps are
slot-position-independent; WLED's updater only checks the 0xE9 magic); on
boot the module notices the foreign partition table, locates itself by
comparing its `esp_app_desc` against each app slot, copies itself to
0x10000 (sector-by-sector, read-back verified), wipes the nvs/otadata
sectors, rewrites the table (the single ~ms non-re-runnable window), and
reboots. WLED's Arduino-era bootloader is kept and boots our image fine
(proven: "ets Jul 29 2019", DOUT). Crash-safety: everything before the
table write re-runs under WLED's intact table, and since `boot_guard`
runs first (WLED's table also has ota_0/ota_1 + otadata), a crash-looping
takeover build rolls itself back to stock WLED after 3 strikes.
build.rs now serializes partitions.csv via esp-idf-part (byte-identical
to espflash's output, MD5 row included) for the embedded table.

First live run (Athom LS8P music controller, WLED 0.13.2 → Luxel
v0.1.30): upload → self-copy (910 KB) → repartition → clean boot on
ota_0, same DHCP lease (192.168.0.183), storage self-formatted, assets
pushed via `deploy.sh --assets-only`, web UI + engine live at 123 fps.
OPEN ISSUE: the very first boot (from the WLED slot) panicked once with
`esp-alloc: Exceeded the maximum of 3 heap memory regions` *before*
`ota::init`, then self-healed via the panic-reboot handler and never
recurred. A second full takeover run later the same day did NOT reproduce
it — intermittent, 1-in-2 so far. Pre-guard panic loops would never arm
the rollback — understand before advertising this as a public migration
path.

Same-day follow-up — credential/settings inheritance VALIDATED offline:
a configured WLED 0.13.2 stores the WiFi SSID in `cfg.json` and the
password in `wsec.json`, both on littlefs v2 (4 KiB blocks) in the old
spiffs partition — mounted the real device's dump and matched both
against known-good creds. The factory-fresh dump (never provisioned) has
both empty, and WLED's captive-portal config writes NOTHING to NVS (its
`nvs.net80211` blobs stay unprogrammed — WLED runs WiFi.persistent(false)),
so littlefs is the only credential source. cfg.json also carries the
board wiring (relay/IR/mic pins, LED outputs) for the future settings
import. Migration design settled: takeover attempts littlefs inheritance
(creds + pins + name) BEFORE wiping anything; if creds are absent
(factory-default devices) it falls back to the provisioning AP — which is
therefore mandatory, not optional, for the public migration path. Local
corpus (git-ignored, contains real creds): athom-wled-fs-configured.bin,
athom-wled-nvs-configured.bin.

## 2026-07-19 — v0.1.30: boot-resume heap pre-flight (found on hardware)

The v0.1.29 hardware pass caught a real boot-loop: with 2048 px + a large
pattern persisted, boot-time resume loaded source + bytecode + envelope
(all infallible allocations) into the boot heap trough while WiFi — whose
mallocs don't null-check — was still initializing. OOM panic, three
strikes, and the boot-loop guard (correctly) flipped the OTA slot back to
v0.1.28. Two-part fix, found iteratively on the wall (a heap pre-flight
alone still flipped — measured before WiFi had allocated anything, the
heap looked deceptively roomy):
- `resume_task` now spawns **after `wait_config_up()`** — resume always
  runs against post-WiFi steady-state heap instead of racing radio
  bring-up (no network means no resume, but also nothing to persist);
- `apply_stored` pre-flights the heap using a new
  `patterns::stored_size_hint` (TOC chunk counts — no flash reads, no
  allocation), waits up to 20 s for `2× stored bytes + 24 KB` of headroom,
  and skips resume gracefully if it never appears (the default pattern
  keeps rendering; the library copy is untouched).
Verified on the wall: the same 2048 px scenario now reboots cleanly on
the same slot. The playlist task still spawns pre-WiFi as it always has
(soak-proven at 300 px) — worth revisiting if heavy-config playlists ever
misbehave at boot.

Also verified on hardware from the v0.1.29 checklist: single-pattern
resume at 300 px (pattern + controls back after power-cycle), live
sk9822↔ws2812 switches with no outage, and the back-to-back
`/api/config` + `/api/protocol` persistence race (both values survive a
reboot — the WANT_* fix works).

## 2026-07-19 — First full-library hardware soak: 321/322 clean, 0 panics — the render2D holdouts are closed

The first soak since library/ became the gallery source, and it covers the
whole thing: all **322** patterns (the old 195 plus the completed clean-room
corpus), each run on the wall unit's strip.

- **321/322 clean, 0 panics, 0 rejections, 0 reboots** (serial monitored
  end to end); lowest heap seen 53.1 KB; fps at 300 px median 56, p90 123.
  Report regenerated at docs/bench-report.md.
- **The Breakout/Crosstown/Frogger/Swirlpool OOB class is closed.** Root
  cause of the old 4 errors: the pre-clean-room gallery sources carried the
  originals' `sqrt(pixelCount)`-square-rig assumption (fails identically on
  a real PB at 300 px — oracle-verified earlier). The clean-room
  reimplementations dropped that assumption by design (fixed 16×16 virtual
  canvases / map-driven normalized coordinates), so with the oracle-derived
  default ceil(√n) grid they run at any pixel count. Verified natively
  (10 simulated minutes each + seed/fps sweeps), in the shipped playground
  wasm (6000 frames each), and on-device. No engine change was needed;
  the PB-semantics reasoning is recorded in
  docs/research/04-oracle-findings.md ("Maps and render2D" → Resolution).
- **Emoji Animation #2 confirmed on-device**: 20 fps, no vmerr, 87 KB free
  (the LXBC v3 const-array fix holding in practice).
- The one remaining holdout is new territory from the expanded library:
  **"Music Sequencer - for V3 ONLY"** — a true capacity failure, and a
  near miss. 663 lines, 17.8 KB blob, ~71 KB total engine footprint
  (heapstat's largest); it loads at idle heap but leaves **19 KB** free,
  1 KB under the firmware's 20 KB floor, and is cleanly rejected with the
  user-facing "pattern too large for this device" error (mid-soak, with
  less heap, it fails at decode instead). It runs fine in the playground.
  Closing it means flash-mapped execution or WiFi-blob tuning
  (docs/ideas.md) — not worth a risky radio-stack gamble for one pattern.
- No firmware change; device stays on v0.1.28, restored to rainbow/300 px
  and healthy (103.8 KB idle free) after the soak.

## 2026-07-19 — v0.1.29: single-pattern reboot resume + LED-protocol re-init hardening

Two device-robustness features, both off-hardware so far (compile-checked;
the hardware pass has a checklist in docs/webui.md).

- **Single-pattern reboot persistence** (the deferred half of playlist
  resume): an *activated saved* pattern + its explicitly-set slider values
  now survive a reboot. New `firmware/src/resume.rs`; record under reserved
  storage key `0x7FFF_FFFB` (next to the playlist's), line format matching
  playlist.rs (`P <id>` + `C <name> <raw…>`). Rules:
  - only library patterns persist — an ad-hoc `/api/code` push has no saved
    source, so the record is left alone and a reboot resumes the last
    *saved* state;
  - **playlist precedence**: the record is neither written while a playlist
    plays nor applied at boot when the playlist's was-playing flag resumes;
    stopping a playlist marks the item that was showing as the resume state;
  - **flash-wear discipline**: writes debounce (3 s of quiet after the last
    activation/slider event) and identical records are never rewritten;
  - resume is graceful about deleted patterns and stale-format bytecode
    (post-OTA LXBC bump) — it just skips, leaving the built-in default.
- **LED-protocol re-init edge cases** (the last Phase-4 stragglers):
  - `Msg::Protocol` reconfigures the SPI clock *first* and only commits the
    protocol if that succeeded (no more encode-format/wire-clock mismatch on
    a failed apply);
  - the encode buffer is reallocated old-buffer-freed-first and *fallibly*
    (ws2812@2048px ≈ 18 KB; the infallible re-alloc could OOM-panic → reboot
    on a tight heap); on failure the engines freeze and it retries; both
    encode paths length-check the buffer (lazily re-allocating once heap
    frees up, else skipping output) instead of indexing out of bounds;
  - `Msg::Config` frees the engines *before* resizing the buffer (peak-heap
    ordering);
  - **requested-vs-applied persistence fix**: `/api/config` + `/api/protocol`
    persist from new `WANT_PIXEL_COUNT`/`WANT_PROTOCOL` atomics (stored at
    enqueue) instead of the applied atomics, which lag until the render task
    drains the message — back-to-back POSTs could previously persist a stale
    value for the other field and lose one setting across a reboot.

## 2026-07-08 — v0.1.28: `assert()` — invariants become real code; playlists pre-flight against the config

Jeremy's redesign of the hours-old `//# require` directive, and it's
strictly better: **`assert(cond[, "message"])` is a real statement** that
runs inline in top-level init, so invariants can use anything initialized
above them — derived vars, user function calls, array contents — not just
`pixelCount` arithmetic. The comment-directive form is gone (it shipped
yesterday; nothing depended on it).

```js
var w = sqrt(pixelCount)
assert(floor(w) == w, "needs a square number of pixels")
```

- A failed assert **aborts init on the spot** (code above it ran, code
  below didn't) and blocks rendering with
  `pattern requires: needs a square number of pixels (pixelCount = 300)`.
  Changing the pixel count rebuilds the engine → re-runs init → re-checks
  every assert: the settings-page workflow is self-healing, live-verified
  both directions on the wall unit.
- Top-level only, by compile error: inside a function it would fire per
  frame; nested in a branch it isn't an invariant. The quoted message is
  the language's first (and only) string literal, legal only there. A
  runtime error inside the condition stays an ordinary vmerr.
- **LXBC v4**: deduplicated assert-message table + `Assert` opcode, so the
  message survives to compiler-less devices (lean decode keeps it — it's
  user-facing error text, not debug info). `bc-version` auto-heal covers
  v3 blobs; the dev unit's library was upsert-healed to v4.
- **Playlist pre-flight** (the workflow gap that motivated all this): the
  render task re-validates every playlist entry's asserts between frames
  whenever config or content changes (boot, playlist edit, pattern
  save/delete, pixel-count change) — free for assert-less patterns, the
  message table gates it. `GET /api/playlist` reports per-item
  `"invalid":"<msg>"` and the web UI badges the row (⚠ won't run). The
  native mirror computes the same field inline, and device-e2e covers the
  whole loop (API verdict + rendered badge in real chromium).
- One deliberate compatibility note: `assert` is the first extension that
  makes a pattern Luxel-only when used (on a real PB it's an unknown
  identifier). Zero corpus collisions (293/293 clean of `assert`); corpus
  report unchanged at 291/293.
- Hardware: v0.1.28 on the dev unit; assert + config-flip + playlist
  badge verified live; pixel-count sweep 300→600→1024→2048→300 with zero
  panics (heap ≥ 90 KB throughout).

## 2026-07-08 — `//# require` invariants + PB-faithful default grid — the last 2D failures explained

The Breakout/Crosstown/Frogger/Swirlpool class is now understood end to
end, and patterns get a language-level way to state their assumptions.

- **Default map** (PB-as-experienced, oracle-verified): a pattern that
  exports only `render2D`/`render3D` with no installed map now gets an
  automatic ceil(√n)×ceil(√n) row-major grid instead of erroring with
  "no map". Probing the real PB showed a stronger fact: a PB that has
  ever saved a map *cannot be returned to maplessness* via its public
  interface — blank map source and `[]` both keep the old compiled map
  (documented in docs/research/04-oracle-findings.md, plus
  tools/oracle/mapdump.mjs to snapshot/restore a PB's map losslessly).
- With that grid, the four holdouts fail **identically on a real PB** at
  non-square pixel counts (verified live: clean at 17×17, same
  out-of-bounds at 10×30). They're square-rig patterns; 300 isn't square.
  At 289 px Breakout runs on the device at 45 fps. Luxel's 191/195 is
  every pattern PB itself could run on this rig.
- **`//# require <expr> ["message"]`**: patterns can declare invariants in
  a comment directive — `//# require pixelCount % 2 == 0`, or the
  Breakout fix, `//# require floor(sqrt(pixelCount)) == sqrt(pixelCount)
  "needs a square number of pixels"`. Checked before `init` ever runs; a
  violation blocks rendering (black frame) and surfaces as
  `pattern requires: needs a square number of pixels (pixelCount = 300)`.
  Compiled by the frontend into ordinary hidden exported fns (name-tagged
  `require …`), so the wire format is unchanged and the compiler-less
  firmware enforces them by just calling functions. PB-compatible: on a
  real PB the directive is a comment. Documented in docs/lang.md.
- vmerr polish: errors without a source location (require violations,
  lean-decoded blobs) no longer carry a noisy `line 0:0:` prefix.

## 2026-07-08 — LXBC v3: const-array data section — 192/195, capacity failures extinct

Jeremy's idea, straight out of mainline compilers: array literals are
constants — put them in a data section instead of building them at runtime.
The measurement made it a slam dunk: Emoji Animation #2's 768 `[r,g,b]`
literals contain FOUR unique triplets.

- The compiler interns every all-numeric array literal into a
  **deduplicated const pool** in the blob (the pattern's `.rodata`); a new
  `ConstArr` opcode replaces the per-element push/NewArray stream.
- Mutability preserved by **copy-on-write**: each literal occurrence keeps
  its own arena identity as a plain index into the pool (`ArrRepr::
  Const(u32)` — no Rc/Arc, no new dependencies; the arena can never
  outlive the Program, so ids suffice, same as fn/global/builtin ids), and
  materializes an owned copy only on first write. Never written → never
  copied. Identity-preservation is unit-tested (two identical literals
  don't alias after a write).
- Element budget (PB-compat 10,240) still counts const arrays; the byte
  ledger charges only the 32 B entry.
- Emoji Animation #2: blob 17.3 → 5.8 KB, decoded program 19.3 → 8 KB,
  full engine 70.7 → 41 KB — **runs on the device at 34 fps with ~67 KB
  free**. It was the last capacity holdout.

**Certification soak: 192/195 clean, 0 panics, 0 rejections, lowest heap
observed 60.6 KB free.** The 3 remaining errors are the no-map `render2D`
index bugs (Breakout/Crosstown/Frogger — oracle question in ideas.md);
Rainbow Smiley and Rainbow Comet cleared too (their errors were array-
degradation side effects). Day's full arc: 134 → 176 → 182 → 189 → 192,
and idle free heap 50 → 107 KB.

Also: docs/tools.md — a one-page index of every script/harness (soak,
oracle, corpus, e2e, deploy, heapstat), linked from the README.

## 2026-07-08 — v0.1.26/27: the RAM reclaim — 189/195 gallery patterns run; idle free heap 50 → 107 KB

Follow-through on "find something systemic": four structural changes, each
soak-verified on the wall unit, stacking to a device that runs almost the
whole gallery where the morning's build ran two-thirds of it.

- **In-place bytecode execution (LXBC v2)**: the VM now interprets the
  flat LXBC bytes directly — `pc` and jump operands are byte offsets, and
  nothing is materialized per instruction. A decoded Program went from
  ~5× its blob to ~1.2–2.5× (Emoji Animation's 17 KB blob: 60.7 → 19.3 KB
  decoded). One interpreter everywhere (firmware, wasm, native), so
  browser preview and strip can't drift; the decoder additionally proves
  every jump lands on an instruction boundary. Same speed (fps curve
  within noise: 123/84/49 fps at 300/600/1024 px). Format bump v1→v2 —
  stored patterns auto-heal via the bc-version recompile path. The old
  `Insn` enum survives only as compiler IR.
- **Streaming pattern uploads**: /api/code and /api/patterns now stream
  their bodies like OTA/assets do, into an exact-size fallibly-reserved
  Vec. The per-connection HTTP buffer dropped 24 KB → 4 KB (big uploads
  used to dictate its size for every connection) and the upload-size cap
  is gone — the "invalid bytecode: truncated" failures with it.
- **Per-connection buffers + engine freeze**: HTTP buffers are allocated
  per connection, not held for the server's lifetime (the 3rd pool slot
  had already died with the /ws stream). If an upload can't get memory
  because the running pattern owns the heap, the engine is FROZEN (heap
  released, strip holds its last frame) and the reservation retried — and
  OTA freezes the engine up front (a reboot follows anyway). No more
  "can't reach the device because a big pattern is running".
- **Byte-accurate array budget** (elements × 8 + per-array overhead
  against free heap, PB's 10,240-element compat cap on top) and
  pattern-cell hygiene (PATTERN_SRC/_BC no longer retain a past giant's
  capacity forever).

**Definitive soak (docs/bench-report.md): 189/195 clean, 0 panics or
reboots.** The 6 remaining: Emoji Animation #2 (genuinely beyond ~107 KB
free — cleanly rejected), and five 2D patterns hitting `array index out
of bounds` with no map installed (Breakout/Crosstown/Frogger/Rainbow
Smiley/Rainbow Comet) — a map-semantics question for the PB oracle, on
the backlog, not a memory problem. Day's arc: 134 clean (with dozens of
disguised OOM reboots) → 176 → 182 → **189, zero reboots**.

## 2026-07-08 — v0.1.25: the OOM hunt — soak v5 finally runs the whole gallery with zero reboots

Soak v5 on v0.1.24 kept OOM-panicking the device ("memory allocation of N
bytes failed" → reboot). Root cause wasn't one bug but a heap-economics
problem: ~50 KB free for patterns, and several paths that allocated
infallibly while a heavy pattern legitimately held most of it. Fixed over
six soak iterations, each verified on the wall unit via serial:

- **One decoded Program at a time.** The render task kept a resident
  second copy (`current_prog`) + per-rebuild clones — a decoded Program is
  2–3× its blob. Now the only Program lives inside the engine; rebuilds
  re-decode from the running blob, and the outgoing engine is freed
  *before* the new one decodes (peak lands where the most is free).
- **`bytecode::validate()`** — every check the decoder does, near-zero
  allocation (equivalence-tested against `deserialize` on truncations and
  corruptions). Upload/activate/MQTT/sync handlers no longer build a
  throwaway Program in request context.
- **`deserialize_lean`** — devices decode without debug info (pos +
  local names): ~half the Program RAM; vmerrs keep fn/pc, lose line:col.
- **Envelope passthrough** — `Msg::Code`/`Crossfade` carry the received
  LXP1 buffer verbatim; producers (HTTP/MQTT/sync/playlist) make ZERO
  source/blob copies (a copy in the HTTP task OOM'd while amoeba ran).
- **Fallible everything in the swap path** — decoder Vecs, the request
  body copy, PATTERN_SRC/_BC updates all `try_reserve`; array allocation
  is budget-checked BEFORE reserving and the reservation itself is
  fallible (also fixes a pre-existing VM hole: `array(50000)` allocated
  first, budget-checked after = panic even pre-bytecode).
- **Heap-aware array budget + runtime floor** — arrays may fill free heap
  down to a 20 KB floor (÷12 per element, min 1024 so ordinary strip
  patterns never trip it); a pattern whose engine leaves less than the
  floor is REJECTED with a "pattern too large for this device" vmerr.
  Rejected patterns idle properly (a busy-spin showed up as "150825 fps").
- **DRAM rebalance** — the third HTTP pool slot existed for the removed
  preview websocket: pool 3→2 (each slot = 32 KB of buffers), request
  buffer 16→24 KB (envelopes are src+bytecode now), main heap region
  88→96 KB (the ~31 KB stack was sized for the deleted on-device
  compiler; the pool's static shrink pays the stack back). Idle free
  heap: ~50 → ~66 KB.
- vmerr reporting deduped per error site (was per frame: serial flood +
  format! churn at 120 fps).

**Soak v5 final (docs/bench-report.md): 176/195 clean, 0 panics/reboots**
across all 195 patterns + the pixel-count curve — vs the old committed
baseline of 134 clean/61 errors (most of those "timeouts" were OOM
reboots in disguise). The 19 remaining: 7 genuinely-too-large patterns
(amoeba, tixy, bustle, Bouncy Boxes, DBZBattleFinal, neutronorbit,
StarGen polar 2D — clean rejections), ~6 array-budget degradations, 3
2D-game patterns with map-related index errors (separate issue), and a
dozen heavy patterns now run *slow* (20–29 fps) that previously crashed
the device outright. Closing the rest wants in-place bytecode execution
(skip the `Vec<Insn>` materialization — the PB way); tracked in ideas.

## 2026-07-08 — v0.1.24: devices execute LXBC bytecode; compiler out of firmware

The browser (wasm) and CLI now compile patterns to a serialized bytecode
(**LXBC**, docs/spec/bytecode.md) and upload it alongside the source in an
LXP1 envelope; the device stores both and only ever *decodes + executes* —
the lexer/parser/compiler are no longer linked into the firmware (a
default-on `frontend` cargo feature in luxel-core).

- **Flash**: app text+data 929.4 KB → 876.8 KB (−52.6 KB net, decoder
  included) against the 1 MiB OTA slot.
- **Robustness**: the on-device compile path — the deep-recursion,
  alloc-spiky thing behind the v0.1.21 stack-overflow saga — is gone by
  construction. The LXBC decoder fully validates untrusted blobs (the VM
  trusts `Program`, so indices/jumps/argc are proven at decode time).
- **ABI**: builtins are referenced by name via a per-blob import table, so
  growing the builtin table never invalidates stored patterns; real format
  changes bump `FORMAT_VERSION` and devices answer `"code":"bc-version"` —
  the web app then recompiles from the stored source and re-saves
  automatically.
- **Sync**: followers adopt the leader via `GET /api/pattern.lxp`
  (source + bytecode envelope) instead of compiling `/api/pattern`.
- **Storage**: pattern store format v3 (bytecode chunks next to source
  chunks; bc gets 6×3840 B ≈ 23 KB — corpus max blob is 17.5 KB). ⚠️ the
  v2→v3 bump wipes the on-device library + playlist on first boot after
  OTA; re-save from the app.
- **API break**: `POST /api/code` and `POST /api/patterns` take the binary
  LXP1 envelope now — raw-source curls need `luxel compile` (new
  subcommand) or the web app. Mirror (`luxel serve`) matches, and executes
  stored bytecode via the same decode path as the device.
- Verified: corpus 291/293 compile+run with byte-identical LXBC round-trip
  and pixel-identical source-vs-bytecode rendering; luxel-core tests (135)
  incl. decoder corruption tests; device/sync/playground e2e all green;
  ESP32 (Xtensa + C3) builds with zero frontend symbols.
- Drive-bys: `<init>` now keeps line info for init-time vmerrs;
  MAX_LOCALS off-by-one (256th local wrapped a u8) fixed at 255.

## 2026-07-07 — the v0.1.23 batch (catch-up entry): HA polish, sync v2, SNTP, output pipeline, panic fix, boards

Written after the fact — this batch shipped across 2026-07-06/07 in commits
e04f661…012d71b and was verified piecemeal; collecting it here.

- **HA polish** (e04f661): diagnostics sensors (fps/heap/rssi) + playlist
  switch and next/prev buttons via MQTT discovery. Verified live on the
  real broker + wall unit — the HA integration task (#54) is closed.
- **Sync v2** (0908341): followers adopt the leader's *pattern*, not just
  its timebase — beacon carries a source hash; on change the follower
  pulls and swaps. (Since v0.1.24 the pull is `/api/pattern.lxp` bytecode.)
- **SNTP wall clock + timezone** (7fe7aa7): `clockHour()`-family builtins
  work unplugged from a browser; tz persisted, settable via /api/clock.
- **Output pipeline** (3a29bfd): wire color order, global gamma LUT, and a
  current-estimate power cap — applied between blend and protocol encode,
  all live-settable + persisted (/api/output).
- **Compile-panic fix, part 1** (61c4266): parser/compiler recursion
  bounded at depth 60 → "nesting too deep" diagnostic instead of a device
  stack overflow (the soak-v4 crash). Deepest real gallery pattern is 16.
  Part 2 (a stack byte budget) was designed but became moot — v0.1.24
  removed the compiler from the device entirely.
- **Socket hardening + soak harness** (bd8d258): 45 s read_request timeout
  (kills the pinned-socket cascade) and tools/hw-bench.mjs, the on-hardware
  gallery soak + fps/pixel-count bench behind docs/bench-report.md.
- **Board portability** (012d71b): board.rs identity module, four board
  features incl. `board-esp32-generic`, wiring isolated to one marked
  section of main.rs. All three Xtensa variants + C3 build clean as of
  2026-07-08; docs/boards.md has the add-a-board recipe.
- Plus: README front-door rewrite (9781563).

## 2026-07-08 — device recovered ✅: v0.1.21 verified on hardware + tools/deploy.sh

You serial-flashed the fix — thanks! Verified on the wall unit right after:

- **Boot + OTA path healthy**: v0.1.21 up at ~124 fps, and a fresh OTA cycle
  (ota_0 → ota_1) worked, so the wedge is fully behind us (boot guard armed).
- **Sensors on hardware**: injected a frame via POST /api/sensors — exported
  vars carried the exact values and the strip lit from `energyAverage`.
- **Sync on hardware**: put the device in follower mode and fed it beacons
  from the dev container — it hard-jumped onto the fake leader clock in <1 s
  and held **±6 ms**. (Two-device sync awaits a second Luxel.)
- **DDP/E1.31 re-verified** on the new build.
- **MQTT/HA verified on your live broker** (192.168.1.2, user root): the
  device connected, published retained discovery, and shows up as device
  `luxel-4ae0d4` in HA — a **Light** and a **Pattern** select. Exercised
  from the MQTT side against the real wall: brightness 66/255 → device 8/31
  (state echoed back), power OFF blanked the strip with the engine still
  running, ON restored, and selecting "Rainbow" from the (auto-announced)
  library options switched the running pattern with the state topic
  following. Brightness restored to your 4/31 afterwards. The device
  library was empty post-recovery — I saved "Rainbow" into it so the
  select has an option; check Settings → Devices in HA for the new device.

**Your question — does flashing firmware also flash the asset bundle? No.**
`espflash flash` writes only the app image; the web app lives in the
`assets` partition (0x310000), deployed separately via POST /api/assets.
Your recovery left the device serving a stale playground — fixed (pushed the
current bundle), and now there's **`tools/deploy.sh <ip>`**: builds + OTAs
the firmware, then builds + packs + pushes the assets, one command
(`--fw-only` / `--assets-only` to split; validated live end-to-end).

## 2026-07-08 — Mic-to-strip, oracle sweep, Luxel-to-Luxel sync (v0.1.21)

Per your picks (all without the wedged device; everything below rides the
same recovery flash):

- **Mic → device forwarding**: with the playground's *sound* toggle on in
  device mode, mic frames also stream to the strip via POST /api/sensors
  (~20 Hz) — your laptop mic IS the sensor board. e2e-verified on the mirror.
- **Oracle sweep vs the real PB (fw 3.67)**: 130/165 exact + new probes.
  Transforms fully verified (composition order, cross-frame accumulation,
  rotate direction — pinned as tests); found + fixed one real divergence
  (`pow(negative, fractional)` now returns the PB's raw 0x80000000 instead
  of 0); several TODO(oracle) markers settled (log2(≤0), refs-as-0, ref
  identity equality, builtin shadowing aborts, div/0 family). New documented
  supersets: builtins as first-class values, lenient arity.
- **Luxel-to-Luxel sync v1**: one device leads, broadcasting its engine
  timebase on UDP :4049 (4×/s, sensor frame piggybacked when fresh);
  followers hard-jump when >1 s off, then slew smoothly by stretching frame
  deltas. Same pattern on several Luxels = phase-locked, one mic drives all.
  Role select in Settings (persisted; device-settings record → v4 with a
  compatible v3 fallback). **Proven with two mirrors** — tools/sync-e2e.mjs
  desyncs them 2.5 s and watches them converge to single-digit ms + the
  sensor relay land. On-device: needs two recovered Luxels someday.

## 2026-07-07 — ⚠️ DEV DEVICE NEEDS A BENCH RECOVERY (serial reflash)

The v0.1.19 OTA (MQTT) **bricked the boot**: the first cut put ~12 KB of
MQTT/netin buffers into embassy task futures — which are *statics*, and
statics eat the DRAM that becomes the main task stack (the v0.1.4 lesson,
re-learned). Main stack fell to ~10.7 KB (known-fatal is <15.6 KB) and the
device now crash-loops before WiFi comes up, so OTA can't reach it.

**To recover at the bench** (bad image is in ota_0; USB/serial):

```
cd firmware && ./build-esp32.sh board-pixelblaze-v3
nix develop --command espflash save-image --chip esp32 \
    target/xtensa-esp32-none-elf/release/luxel-fw /tmp/luxel-fix.bin
nix develop --command espflash write-bin 0x10000 /tmp/luxel-fix.bin
```

(That writes the current fixed build — now v0.1.21, which also picks up the
sensor-board/sound/sync work above — over the bad one; otadata already
points at ota_0. The fix moves all big task buffers to the heap and trims
the heap static 96→88 KB — main stack is back to ~31 KB.)

**So this can never happen again:** the firmware now has a **boot-loop
guard** — it counts boot attempts in flash before the risky part of boot,
and on the 3rd consecutive boot that never reached "healthy" (60 s of
serving) it flips otadata back to the other OTA slot by itself. A future
bad OTA self-heals in ~30 s instead of wedging the device.

## 2026-07-07 — Sound-reactive groundwork: playground mic + sensor-board support (v0.1.20) 🔶 partly device-blocked

Per your pick (audio next, engine+playground first, sensor board too):

- **Sensor bindings live in the engine** — `export var frequencyData` /
  `energyAverage` / `maxFrequencyMagnitude` / `maxFrequency` / `light` /
  `accelerometer` / `analogInputs` now receive data (they were zero-stubs).
- **Playground "sound" toggle** (next to *debug*): feeds your microphone
  through a WebAudio analyser reshaped to the PB sensor board's 32 log-spaced
  bands (37 Hz–10 kHz) — sound-reactive patterns run in the browser today.
  e2e-covered with chromium's fake mic.
- **PB sensor expansion board support (firmware)** — the official board's
  115200-baud `SB1.0` frames are parsed off UART0 RX (the expansion header's
  RX0, where the board plugs in PB-style). Parser is shared with the mirror
  and unit-tested; 🔶 hardware verification needs the recovered device (and a
  physical board, if you have one).
- **`POST /api/sensors`** (firmware + mirror) — accepts a raw sensor-board
  frame over HTTP, so a desktop script can stream audio/motion data to the
  device with no extra hardware. e2e-verified against the mirror.
- Also: the **firmware size report** you asked for is in
  [docs/size-report.md](docs/size-report.md) — TLDR: ~⅓ is Espressif's
  closed radio stack; found + fixed one real lump (Fx printed via f64 →
  −14.4 KB); regenerate with `tools/size-report.py`. And the **C3 devkit
  build** compiles again (two rv32imc atomic-RMW slips).

The onboard PB v3 mic (your "SPI audio") stays open: closed hardware,
undocumented pinout — that's a bench session with the recovered device
(I2S/PDM driver + FFT prep can happen any time; say the word).

## 2026-07-07 — MQTT + Home Assistant discovery (firmware v0.1.19) 🔶 device-blocked

Point the device at an MQTT broker (Settings → "MQTT / Home Assistant":
host/port/user/pass, applied live, persisted in flash) and it announces
itself to Home Assistant automatically via MQTT discovery:

- **Light** — power + brightness as a normal HA light. Power off blanks the
  strip while the engine keeps running, so ON resumes mid-animation.
- **Pattern select** — the device pattern library by name; picking one runs
  it (and re-announces when the library changes).
- Availability (`luxel/<id>/status`) with an offline LWT; state topics echo
  changes made anywhere (web UI slider moves show up in HA within ~5 s).

Topics/payloads live in `luxel_core::hamqtt` (unit-tested), shared between
firmware (rust-mqtt over embassy-net, with a tiny embedded-io 0.7→0.6
adapter) and the mirror (rumqttc). **Verified end-to-end against a real
mosquitto** with the mirror: retained discovery configs, brightness
round-trip (HA 128 → device 16 → state 132), pattern select switched the
running pattern. Also: `/api/mqtt` GET/POST, broker creds in the third nvs
sector, `connected` status in Settings. e2e: 68 device checks green.

🔶 On-device verification is blocked on the bench recovery above; after
that, OTA the fixed image and point it at a broker (HA's Mosquitto add-on
or any broker; note the dev container's broker may not be reachable from
the device's subnet — use the HA one).

## 2026-07-07 — DDP + E1.31 network input (firmware v0.1.18) ✅

xLights / LedFx / Resolume can now drive Luxel as a network pixel output:
the device listens for **DDP on UDP :4048** and **E1.31/sACN on UDP :5568**
(universe 1 up, 170 px each). Incoming frames paint the strip directly,
bypassing the engine; ~2.5 s after the stream stops, the running pattern
takes back over — so a LedFx session ends and the wall just resumes its
playlist. `/api/status` gained a `live` field (`"ddp"`/`"e131"`/`null`) and
Settings shows a "Network input" status row.

Packet parsing lives in `luxel_core::netin` (unit-tested, shared by firmware
and mirror). Verified end-to-end on the wall unit: DDP red/green/blue +
offset writes, E1.31 magenta, and the timeout-resume — all over real WiFi.
Multicast sACN groups are joined at boot but only unicast was verifiable
from the dev container (bridges/APs commonly filter multicast); xLights and
LedFx default to unicast anyway. Image is 887 KB — still 161 KB under the
1 MiB OTA slot. Both e2e suites green (65 device checks); v0.1.18 OTA'd.

Also: **springy easings** — `easeOutBack`, `easeOutElastic`, `easeOutBounce`
join the builtins (that completes the entire builtins backlog in ideas.md).

## 2026-07-07 — Playlist crossfade transitions (firmware v0.1.17) ✅

Playlists can now **crossfade** between items instead of hard-cutting. The
Playlist tab has a new "crossfade" field (seconds; blank/0 = hard cut). During
a transition the render task keeps the outgoing pattern alive and linearly
blends it into the incoming one over the set time — verified on the wall unit:
a red→blue item change ramps through `a3005b`→`62009c` mid-fade rather than
snapping. The crossfade time persists in flash (playlist wire format gained an
`X <ms>` line) and applies on both auto-advance and manual next/prev.

Also: the app crossed the **1 MiB OTA-slot** boundary at this version, so the
release profile moved to `opt-level = "s"` (size) — reclaimed ~177 KB (image
1,052 KB → 875 KB) with no visible render-rate hit (still 125 fps). Both e2e
suites green; v0.1.17 OTA'd to the wall unit.

## 2026-07-07 overnight — batch: gallery search, playlist polish, 3D preview, WiFi form, device map ✅

While you slept (you picked web features + WiFi + device map, OTA authorized):

- **Gallery search** — a search box filters the 190+ patterns by name.
- **Playlist polish** — a Clear button, a "N items · loop ≈ Xm Ys" total, and
  playlist entries whose pattern was deleted now show "(deleted)" and can be
  removed (the scheduler skips past them).
- **3D map preview** — a map whose z varies now renders as a slowly
  auto-rotating point cloud (depth-sorted, nearer points larger/brighter)
  instead of flat.
- **WiFi settings form** — the Settings tab shows the network the device will
  join and lets you change the credentials (it reboots to apply).
- **Device map upload** (firmware v0.1.16) — install a computed 2D/3D map onto
  the device so its patterns render with real geometry (render2D). Verified on
  the wall unit: a reversed map really does drive the pixels (and it survives a
  reboot). "Install on device" / "clear device map" live in the map sub-tab.

All committed + deployed (v0.1.16 OTA'd; web hot-reloaded); the wall is back to
the rainbow with a clean slate. Both e2e suites green (61 device checks). Left
alone per your note: persistence of a single ad-hoc (non-playlist) pattern.

## 2026-07-06 night — Playlist + "untitled" fix ✅

**Playlist** (firmware v0.1.15): a new device tab that plays your saved patterns
in order. Each entry carries its own **parameters**, so the same pattern can
appear multiple times with different looks — "+ playlist" in the editor captures
the current slider values. Durations are flexible: a playlist-level default,
each item can override it, and default-blank = manual advance (so you get global
timed, per-item timed, or manual, even mixed). It's **saved on the device and
resumes across reboots** — verified on the wall unit (after a reboot the playlist
and its params were intact and still playing). Built across the native mirror,
the web UI (reorder / remove / inline params), and the firmware scheduler.

**"untitled pattern" fix:** on a device the header showed "untitled" because the
device streams only source, not which saved pattern it is. Now the editor
recognizes the running pattern as its saved library entry and shows the name.
(Along the way, fixed the editor spuriously marking freshly-loaded patterns as
edited — which also makes the resume-your-edits behavior more reliable.)

Re: your reboot question — **playlists** now persist and resume across reboots.
A single ad-hoc (non-playlist) pattern still resets to the default on reboot;
I can add persistence for that too — say the word.

## 2026-07-06 night — clean device load (no flash, clear "running on device") ✅

You saw the window flash the playground before a connecting spinner appeared. On
a device the app now probes for the device in parallel with the wasm load and
holds a **full-screen boot cover** until it's decided device-vs-playground and
loaded the device's running pattern — so nothing flashes first. The cover reads
**"opening the pattern running on the device…"**, making it clear the pattern is
the one already on the device. Web-only hot reload; both e2e suites green.

## 2026-07-06 night — device mode = local preview + push (no streaming) ✅

Per your call: the live pixel stream from the device wasn't helpful, so it's
gone — along with the connect/disconnect buttons (it's always connected for the
API). Device mode is now **the playground that also drives the strip**:

- The preview runs on the **local WASM engine**, instantly — no device round-trip
  and no ws/HTTP pixel polling. You watch the real strip for the real thing.
- **Editing pushes to the device**: typing recompiles locally (fast) and pushes
  the code over WiFi (throttled; a broken pattern is never sent). Controls drive
  both the preview and the strip. The **step-debugger now works in device mode**
  (it's genuine local compute).
- **No connect/disconnect/reconnect buttons or badge** — the wordmark shows the
  device; a failed connect just shows an error (reload to retry).
- It **still waits for the device on load** so it opens whatever pattern is
  running, exactly as you asked.

Web-only (the firmware still can stream — the UI just stopped asking), so it went
out as a hot asset reload, no reflash. Both e2e suites green (43 device checks);
code-push verified on the wall unit.

## 2026-07-06 night — live LED-protocol switch (firmware v0.1.14, no reboot) ✅

You asked about protocol — different strips use different ones. You can now
switch **SK9822 (APA102) ↔ WS2812 (WS281x)** at runtime, no reboot, from a
Settings dropdown. esp-hal's blocking SPI has `apply_config()`, so the render
task just changes the clock (8 MHz ↔ 2.4 MHz) and re-sizes the encode buffer
between frames — same live-swap trick as pixel count.

- `GET/POST /api/protocol` (accepts sk9822/ws2812 plus aliases apa102/ws2811/
  ws2815). Persisted alongside brightness + pixel count (settings record → v3).
- Verified on the wall unit: sk9822→ws2812→sk9822 live, no crash, SPI restored
  to full speed (a trivial pattern is back to 124 fps). Left on sk9822 (its real
  strip), rainbow running.
- Firmware v0.1.14 OTA'd; mirror + web + e2e updated (device suite now 49
  checks); assets hot-reloaded.

**All three device settings — brightness, pixel count, LED protocol — are now
live, persisted, and reboot-free.** Phase 3 is down to just the Settings WiFi
form (the endpoint already exists).

## 2026-07-06 night — live pixel-count resize (firmware v0.1.13, no reboot) ✅

You asked for `/api/config` pixel count next and said a reboot isn't ideal — it
turned out a **live resize with no reboot** is feasible, so that's what shipped.
The SPI is Blocking (no DMA) and the encode buffer is a plain heap Vec, so the
render task (which already rebuilds the engine on every code upload) just
reallocs and recompiles at the new count between frames.

- `GET/POST /api/config` — POST a pixel count (1–2048), the strip resizes
  instantly. Persisted alongside brightness in flash (the settings record went
  v2). The **Settings Pixels field is now editable** and re-anchors the preview.
- Verified on the wall unit: 300→150→300 with **no reboot** (same OTA slot, fps
  never dropped, heap freed at 150 and came back at 300), out-of-range rejected,
  and it persists. Left at 300 (the physical strip length).
- Firmware v0.1.13 OTA'd; mirror + web + e2e all updated (device suite now 46
  checks); assets hot-reloaded.

Still open in Phase 3: a runtime LED-protocol switch and the Settings WiFi form.

## 2026-07-06 night — device-editor polish + real brightness (firmware v0.1.12) ✅

Continued from your feedback, then took the plan's next step.

- **Waterfall clears on every open**, and the **save/⋯ actions moved** out of the
  header into a toolbar fixed above the editor (`1945a73`).
- **Device editor fixes** (`6db7f03`): opening a device pattern now shows a
  **loading screen** until its source is fetched (no more flash of the last
  script); **Device Patterns get live preview thumbnails**; the **strip/grid/2D
  map dropdown is back on the device** (pixel count fixed by hardware; map is a
  local preview aid); and a **dirty-aware resume** — on load we only resume the
  last file if it had unsaved changes (and then push it so the device runs it
  too), otherwise we open whatever pattern is active on the device.
- **Brightness is real** (`6143352`, **firmware v0.1.12**, OTA'd to the dev
  device): a runtime `GET/POST /api/brightness`, applied every frame in the
  encode path (SK9822 current field + a WS2812 software scale) and **persisted
  in flash** (new `LXDV` nvs record, its own sector so it never touches WiFi
  creds) so it survives reboot. The Settings **Brightness slider is now live**
  (was a placeholder). Verified on hardware: live apply, out-of-range rejected,
  flash-persist ok. Note: the web preview stays at full range — brightness dims
  the physical strip only. Remaining Phase-3 item is `/api/config` (pixel
  count/protocol), which needs a runtime pixel count + reboot-to-apply.

Both e2e suites green (device suite now 42 checks incl. layout/thumbnail/
dirty-resume/brightness); assets hot-reloaded to 192.168.0.205.

## 2026-07-06 night — navigation redesign: library-first, editor-on-demand ✅

Restructured the app around how you actually work with patterns, per your
direction:

- **The Editor is no longer a tab.** It opens **full-screen** (its own bar:
  ← back · name · save/⋯) when you create a pattern or pick one to inspect —
  which "really gives you the feel that you are editing something." On load it
  **resumes your last edit** (falls back to the Patterns Library); on a device
  it opens on the **running pattern**.
- **"Patterns" → "Patterns Library"**, with a **+ New pattern** button. It holds
  the examples, community corpus, and your saved patterns (chips). Picking a
  tile opens it in the editor.
- **New "Device Patterns" tab** (device mode) — the patterns in the device's
  memory, each opens/activates in the editor; its own **+ New pattern**.
- **The examples dropdown is gone entirely.** Pattern selection is the Library /
  Device Patterns lists now.
- **Mapping is clearly optional**: the layout selector gained a **"2D map"**
  option — choosing it is the *only* enable/disable for mapping. It reveals the
  pattern·map editor sub-tabs and runs the map program; any other layout turns
  mapping off. (Replaces the old "back to strip" button.)

Both e2e suites were rewritten for the new flow (editor entered via New / tile /
device pattern; no picker) and are green in real chromium: 58 playground + 28
device checks. Pushed to the dev device.

## 2026-07-06 night — control-picker layout, no device-URL field, no share on device ✅

Three UI fixes from your feedback:

- **Color-picker controls no longer run off-screen.** An `hsvPicker`/`rgbPicker`
  laid its three channels out in one horizontal row, so the 2nd/3rd sliders
  overflowed the narrow right rail and were unreachable — and the channels had
  no numeric field. They now **stack vertically**, each channel with a slider
  **and** an editable number box (like the scalar sliders). Verified: 0 rail
  overflow.
- **No more "device url" field.** The address is always known — a real device
  serves the UI from its own flash (auto-connect to same origin), and reconnect
  just reuses that. So disconnect → **"reconnect" button, no URL to type**. A
  hosted/standalone playground has *no* device support at all (that's what
  playground mode is for), so it shows no connect UI. (Dev/e2e point the built
  UI at a device or the mirror with `?device=<base>`.)
- **Share is gone in device mode.** Those links carry the pattern in the URL —
  on a device that's a LAN address that won't work for anyone else. Share is now
  shown only in the playground (keyed off the same "is this a playground?"
  state, so it's hidden whether you're connected or just served from a device).

The map sub-tab is likewise gated to playground mode (device map upload is a
later firmware item). device-e2e updated (auto-connect via `?device=`, asserts
no URL field / no share button in device mode, and reconnect-without-URL); both
suites green. Pushed to the dev device.

## 2026-07-06 night — clean device connect-on-load (no more waterfall garbage) ✅

Fixed the connect weirdness you flagged: on load the waterfall skipped and kept
stale/old data after the connection stabilized, because the async handshake
(status → source → controls → ws) leaked pre-stabilization frames.

- **Connection phase state** (`idle → connecting → live`). `connectDevice` enters
  **connecting** and holds the preview blank; a `markLive()` helper flips to
  **live** the moment the *real* stream delivers its first datum (ws pixels or
  status, or the HTTP-fallback poll) and clears the preview **once** at that
  transition. So leftover playground content, canvas-resize artifacts, and
  HTTP/WS cadence jitter can never end up in the waterfall.
- **Visible connecting state** — a "connecting…" pill in the header and a
  spinner overlay on the preview, so the async handshake reads as intentional
  instead of showing stale frames.

Verified in the device-e2e (against the native mirror): on connect, the
waterfall is held **blank (0 lit)** all the way through the handshake, then
streams cleanly — plus the existing 19 device checks and 55 playground checks
stay green. Pushed to the dev device.

## 2026-07-06 night — mapper is a debuggable Luxel program in its own editor tab ✅

Re-read the feedback and fixed the thing I'd under-heard: "the mapping function
should be a tab on the script editor **and debuggable as well**." Per your call,
the map is now a **Luxel program** (not JS), which makes it debuggable for free
by reusing the pattern VM + debugger.

- **New `plot(x, y[, z])` builtin** + engine "map mode": a map program exports
  `render(index)` and calls `plot()` once per pixel. A second engine runs it
  through the *same* per-pixel `drive` loop as patterns, but stores the plotted
  coordinate instead of a color. Collected coords install into the pattern
  engine as a 2D/3D map.
- **Editor sub-tab (pattern · map).** The map is edited in the same CodeMirror
  as patterns — luxel highlighting, completions, hover — not a bare textarea.
- **Debuggable exactly like a pattern.** Gutter breakpoints, step over/into/out,
  the call stack, locals, and globals all work on the map program because it's
  real VM code. (The debugger panel is now a shared `Debugger.svelte`.) A live
  scatter preview renders the pattern on the computed map.
- Spans all layers: luxel-core (`plot`, `enable_map_mode`/`run_map`/`map` +
  4 unit tests), luxel-wasm (`lx_run_map`/`lx_map_*`), luxel.ts
  (`compileMap`/`runMap`), App.svelte. Playground-only for now (installing a
  computed map on the device is a later firmware item).

Also, two small fixes you flagged: the page **`<title>` is now just "Luxel"**
(was "Luxel Playground"), and the **Patterns tab shows a loading state** — the
built-in examples render immediately and a spinner reads "loading patterns…"
while the corpus streams in, instead of a bare "0 patterns."

Verified in real chromium: 55 playground checks (incl. map runs, installs,
scatter renders, **breakpoint pauses the map run at pixel 0**, compile errors,
back-to-strip) + 19 device-mode checks, all green; 65 Rust tests pass. Pushed to
the dev device as a hot asset reload (the map tab is playground-only, so it's
hidden in device mode).

## 2026-07-06 night — web UI Phase 2: tabs + decluttered header ✅

Phase 2 of the web-UI redesign — a real tabbed app instead of one crowded
header. Web-only (no firmware / no reflash); pushed to the dev device as a hot
asset reload, so http://192.168.0.205/ is already running it.

- **Tab bar in both modes.** `Editor · Patterns` in playground; device mode adds
  `Settings`. Panels stay mounted and hide via CSS, so the render loop, editor
  state, and gallery tile-engines all survive a tab switch.
- **Header decluttered** to just wordmark · tabs · status (fps + streaming/
  polling) · connection. Everything else moved out:
  - *Editor toolbar* (above the editor): pattern picker + `save`/`delete`,
    `share` (playground only, prominent), and a `⋯` overflow with import/export.
  - *Playback bar* (below the editor): layout (strip/grid/px — playground only),
    fps, pause, debug.
- **Patterns tab** — the gallery, promoted from a modal overlay to a first-class
  inline tab. Lazy-mounted on first visit then kept alive; picking a pattern
  jumps back to the Editor tab.
- **Settings tab** (device mode) — device address, live pixel-count readout, and
  status; brightness / pixel-count editing / WiFi are honest **Phase-3
  placeholders** (labeled as needing firmware — only `/api/wifi` exists today).
- **Share** hidden in device mode (it's a playground affordance), prominent in
  playground mode — per the feedback.

Both e2e suites were rewired to the new `data-role` hooks (`pattern-picker`,
`tab-*`, `overflow`, `map-badge`, `layout-*`, `cfg-pixels`, `pause`, `fps`) and
are **green in real chromium** — 51 playground checks + 19 device-mode checks
against the native mirror. Verified the served bundle on hardware: index.html
(revalidated) points at the new immutable hashed JS/CSS.

Still ahead (docs/webui.md): Phase 3 (firmware `/api/brightness` + `/api/config`,
wire the Settings fields, fix the connect-on-load race), Phase 4 (mapper as a
CodeMirror tab + debuggable, 3D preview, Playlist tab + firmware storage,
MQTT/HA, AP-mode).

## 2026-07-06 evening — HTTP caching: assets only re-download when changed (v0.1.11) ✅

You asked the device to take advantage of browser caching — no redownload of
unchanged assets. Done and verified live on hardware:

- **Content-hashed bundle files** (`/assets/index-<hash>.js`/`.css`) now serve
  `Cache-Control: public, max-age=31536000, immutable` — the browser reuses
  them with **zero network** until their hash (and thus URL) changes.
- **The unhashed files** (`index.html`, `luxel.wasm`, `gallery.json`) serve a
  **strong ETag** + `Cache-Control: no-cache`, so the browser revalidates with
  `If-None-Match` and gets a **304 Not Modified** (empty body) when unchanged —
  a full download only when the content actually changed.

Implementation: the LUXA archive gained a v2 format (`LUX2`) carrying an
8-byte SHA-256 content hash per file (packed host-side); the firmware serves it
as the ETag and answers `If-None-Match` with a 304. The firmware still reads
legacy `LUXA` archives (those assets just revalidate to a 200). No new stack
cost (stack-check green at 12 KB; clippy clean).

Verified on the device (v0.1.11, OTA'd onto ota_0): every asset returns its own
correct ETag; a matching `If-None-Match` → `304` with no body; a stale one →
`200`; `/assets/*` carries `immutable`, the rest `no-cache`. So a second visit
to http://192.168.0.205/ re-fetches nothing unless a file changed.

## 2026-07-06 evening — web UI feedback sorted + Phase-1 fixes ✅

You gave a big batch of web-UI feedback. First, **sorted into the docs so
nothing is forgotten**: the full redesign backlog now lives in
[docs/webui.md](docs/webui.md) (two modes: device console vs. playground;
tabs; settings page; header declutter — each item tagged with effort and
firmware-dependency), cross-referenced from ideas.md. Notable finding: a real
settings page needs new firmware (brightness/pixel-count/config endpoints,
MQTT) — only `/api/wifi` exists today.

Then **Phase 1** (web-only, no reflash — both e2e suites green in chromium):

- **Not a "playground" on a device.** The wordmark now shows the device
  (name/URL) in device mode, "playground" only when standalone. A `data-mode`
  hook is in place for the Phase-2 restructure.
- **Reconnect remembers the device.** Disconnect → connect no longer needs the
  URL re-typed; the last successful base is remembered (and persisted to
  localStorage), reused automatically.
- **"ws push" → "streaming"** (and "polling · N ms" for the HTTP fallback) —
  the old jargon meant nothing to users.
- **Debugger no longer lies on a live device.** A gutter breakpoint used to arm
  debug mode even when connected (the button was disabled but the gutter path
  bypassed it); it's now fully gated off in device mode.
- **Share hidden in device mode** — it only makes sense for the hosted
  playground.
- **Pattern-browser spinners** while a tile's preview is still computing.

The heavier items (tab restructure + header overflow menu, firmware settings
endpoints, connect-on-load race, mapper-as-editor-tab, 3D preview, playlists)
are phased in docs/webui.md as Phases 2–4.

## 2026-07-06 late — chunked patterns: larger than one flash page (v0.1.10) ✅

You asked for chunking so patterns aren't capped at one 4 KB flash page.
Done and hardware-verified:

- A pattern's source now splits across up to 4 chunk items (~3.8 KB each,
  ~15 KB total — 4x the old limit, and the practical ceiling since the 16 KB
  HTTP buffer bounds a POST there anyway). Each pattern also has a small meta
  item (name + chunk count + generation).
- **Atomic updates via generation flip:** an update writes new chunks to the
  *other* generation, then rewrites the meta (the single-item commit point) —
  a power loss before that leaves the old version fully intact. A **RAM
  index** (built at boot) keeps list/lookup off the flash.
- **Format-version marker:** boot wipes `storage` if the on-flash layout
  isn't the current one, so the incompatible pre-chunking items auto-migrate
  (saw `format 0 != 2, wiping storage` on the upgrade boot).

**Two bugs caught in hardware testing, both fixed:**
- A GET of a >4 KB pattern OOM'd (`allocation of 21880 bytes failed`): the
  old `format!(json_escape(..))` path built an ~11 KB intermediate plus a
  doubling result buffer → a ~22 KB contiguous request that fragmentation
  couldn't satisfy. Now escapes into one pre-sized string; `read_source`
  pre-sizes too.

Verified on hardware: byte-exact round-trip of 2-, 3-, and 4-chunk patterns
(7.7 / 10.8 / 15.1 KB, md5-matched), upsert returns the latest, small +
large patterns coexist, persistence across reboot. Larger-than-15 KB
patterns stay in the browser library (source stays there regardless).

## 2026-07-06 afternoon — pattern library verified on hardware (v0.1.7) ✅

You serial-flashed v0.1.6 (new factory-less table). Boot was textbook:
`Erasing storage (Data(Spiffs))…`, table shows `storage @ 0x210000`,
`booted from: ota_0`, `patterns: 0 stored`, flash creds + assets both
survived. Then full pattern CRUD verified on the real device:

- list/save/get/activate/delete all round-trip through flash; activate
  drives the pixels; missing-delete → "no such pattern".
- **Found + fixed a bug in testing (v0.1.7):** sequential-storage's
  `fetch_all_items` returns superseded item versions after an upsert, so
  the list showed a duplicate id — now deduped by key (`fetch_item` already
  returns the latest for reads).
- **Persistence:** OTA'd both slot directions on the factory-less table
  (ota_0↔ota_1, clean) and confirmed saved patterns survive the reboot —
  serial `patterns: 2 stored`, NEXT_SEQ correctly reseeded from stored ids.

Left two curated patterns in the device library ("simplex aurora", "beat
pulse", both using the new builtins) with aurora running. **Task #9 done.**

## 2026-07-06 midday — firmware pattern storage (v0.1.6); dropped factory; stack guardrails

Working with you awake. Two design calls you made, both shipped:

**Dropped the factory partition → dedicated `storage` partition.** You have
no distinct golden image (serial flash = the same build that ships OTA), so
factory was 1 MB of dead weight. New table (partitions.csv): pure A/B
(ota_0/ota_1, bootloader falls back to ota_0 when both fail) + `storage`
(0x210000, 1 MB) slotted ahead of the unmoved assets region. `ota.rs`
already read the table dynamically, so no logic change. **Applying it needs
one serial flash** — your normal `build-esp32.sh flash` runner already does
`--partition-table=partitions.csv --erase-parts otadata`, so it lays down
the new table, clears otadata (clean boot into ota_0), and preserves
nvs/assets.

**Firmware pattern library — task #9, the CRUD contract's device half
(v0.1.6).** New `patterns.rs`, built on **`sequential-storage`** (your call
over my first hand-rolled blob — it's the PLAN's storage model and matches
your "established crates" preference). Each pattern is one KV item (key =
u32 id, value = name+source) in the `storage` partition: a store/remove is
atomic, so a power loss mid-save loses at most the one in-flight pattern,
never the library — and it's wear-leveled. Routes match the mirror exactly:
`GET/POST /api/patterns`, `GET/DELETE /api/patterns/<id>`,
`POST /api/patterns/<id>/activate` — POST compile-checks before storing.

- **Safety guard:** `patterns::init` resolves the `storage` partition from
  the *live* table and disables the store (writes refuse, reads empty) if
  it is absent — so v0.1.6 is safe even on the current (old) table, where
  0x210000 is the live ota_1 app slot. No corruption; it lights up after
  your reflash.
- **Async/blocking bridge:** sequential-storage is async, esp-storage is
  blocking — a small `AsyncFlash` adapter forwards to the blocking methods,
  driven by `block_on` (it never truly pends). The flash driver is *leased*
  out of the OTA module per transaction (Drop-guarded) so its critical-
  section mutex is never held across the erases.
- **One caveat (FYI):** sequential-storage items must fit one 4 KB flash
  page, so a single pattern's source is capped at ~3.5 KB (clear API error
  above that; larger patterns stay in the browser library). If you want
  unlimited device-side pattern size later, chunking across keys would lift
  it — say the word. A serial reflash clears device patterns (they survive
  OTA updates; `--erase-parts` now includes `storage` for a clean region).

**Stack guardrails (so the original OTA-crash class can't recur):**
- `#![deny(clippy::large_stack_arrays)]` in the firmware (threshold 1 KB,
  `firmware/clippy.toml`) — turns a stray `[0u8; 4096]` on the stack into a
  hard `cargo clippy` error. Caught exactly this while writing patterns.rs.
- `tools/stack-check.{sh,py}` — builds with `-Z emit-stack-sizes` and fails
  if any function's frame exceeds a budget. Unlike clippy it sees *library*
  frames too (esp-storage's `FlashStorage::read` bounce buffer — the actual
  original culprit). Added `python3` to the flake for it.

Requires one serial flash of v0.1.6 (new table). Then I'll verify pattern
CRUD, creds, assets, and an OTA round-trip on hardware.

## 2026-07-06 morning — device back on v0.1.5; full checklist verified on hardware ✅

You reflashed v0.1.5 with creds. Device came up on `factory`, joined WiFi
via **compile-time creds** (no flash record yet), 300 px, 123 fps, no
vmerr. Then I ran the promised morning checklist end-to-end — all four
steps passed on real hardware, zero panics:

1. **WiFi creds → flash** — `POST /api/wifi` stored them in the `nvs`
   partition and rebooted. It came back with
   `wifi: joining "MOMCorp Intranet" (flash-stored creds)`. **Your
   partition ask is done and live**: creds now boot from flash, so future
   OTA images need no baked-in creds. Compile-time creds remain only as a
   last-resort fallback (a flash-wipe can't lock you out).
2. **Assets push** — `POST /api/assets` streamed the 431,755-byte LUXA
   archive (5 files) into flash in ~10 s, hot-reloaded, no reboot. The
   full playground now serves gzip'd at http://192.168.0.205/.
3. **New builtins on hardware** — live-coded a `beatSin`+`simplex2`+
   `setGamma` pattern via `/api/code`; pixels animate, `vmerr:null`. (FPS
   eases to ~87–99 under per-pixel simplex across 300 px — expected.)
4. **OTA round-trip** — pushed the 926 KB app image; device switched
   `factory` → `ota_0`, rebooted, came back clean at 124 fps. This is the
   exact path that used to crash 100% of the time. Serial log: **no panic
   since the reflash** (last crash in the log predates tonight's session).

The beatSin/simplex/setGamma demo is left running on the wall. Remaining
open item: **firmware pattern-library storage** (task #9) — still a
genuine flash-layout decision; notes below.

## 2026-07-06 ~01:30 — second overnight batch: simplex, setGamma, **, mapper, pattern library

All committed, tested (106 core tests, 40 e2e checks in real chromium),
and already in the v0.1.5 image + packed assets waiting for the device:

- **`simplex2`/`simplex3`** (`f1b7abc`) — fixed-point simplex noise,
  smoother than perlin, seeded, deterministic.
- **`setGamma(g)`** (`f95d805`) — output gamma as a cached LUT (zero
  per-pixel cost); `setGamma(2.2)` makes LED fades perceptually even.
- **`**` exponent operator** (`16d6f68`) — right-assoc, tighter than `*`.
- **Mapper (M3)** (`4f157d3`) — write a PB-style JS map function, apply,
  and the playground installs a real 2D map into the engine (new
  `lx_set_map` wasm export) with a live scatter preview. The ring-map
  rainbow screenshot is worth opening: e2e-6-mapper.png.
- **Pattern library + autosave** (`11d43a9`) — edits survive closed tabs;
  named saves live in a picker optgroup. This is the prototype UI for
  device pattern CRUD.
- Corpus re-run with everything new: **288/293 ok, zero regressions**
  (user globals correctly shadow new builtin names — checked explicitly).

**Morning checklist (after your one serial flash of v0.1.5):** I'll
detect the device and, automatically: POST /api/wifi (creds → flash,
lockout-proof forever), push the ~432 KB assets archive (new playground +
gallery + mapper + library UI, /api/assets, no reboot), live-code a
beatSin/simplex/setGamma pattern to verify the new builtins on hardware,
and run one OTA round-trip. Nothing needed from you beyond the flash
command in the note below.

Also shipped since that list (all mirror/playground-verified, waiting on
the device only for the firmware storage half): **mapper**, **pattern
library + autosave**, **device pattern CRUD** (save/load/delete patterns
*on the device* — mirror + playground done, firmware storage queued as
the one remaining piece), and a real **CORS preflight fix** (cross-origin
DELETE from the hosted playground to a device by IP). Everything is in
git; `git log 94e8c52..HEAD` is the full night. Full workspace tests +
both browser e2e suites are green.

## 2026-07-06 ~11:30 — overnight progress: builtins batch 2, .epe UI, pattern browser

All hardware-independent, all committed, all verified locally (tests +
real chromium):

- **Builtins batch 2** (`6745a38`): `beat`/`beatSin` (tempo without
  audio), `hash`/`hash2` (stable per-pixel randomness, pinned lowbias32),
  `blur1D`/`feedback` (the trails/fire idioms as builtins),
  `dot`/`dot3`/`angleBetween`, and value-returning color —
  `hsv2rgb`/`rgb2hsv`/`mixColors(..., out)` with mixColors blending in
  OKLab. 101 core tests green. Also: array literals turned out to already
  work — ideas.md refreshed.
- **.epe import/export in the playground** (`463fa54`): import button +
  drag-drop anywhere + export download (PB-compatible shape). e2e covers
  a real chromium download round-trip.
- **Pattern browser** (`0d87f8f`): your gallery idea — 192 live tiles
  (built-in examples + every corpus pattern that compiles clean), 1D as
  bars / render2D as 16×16 rectangles exactly per your spec, viewport-
  lazy so it stays light. Click a tile to open it. It looks genuinely
  delightful — see e2e-5-gallery.png in the scratchpad shots.

Web bundle: 194 KB gz JS + 491 KB gallery.json (raw; ~150 KB gz over the
wire, lazy-fetched only when the browser opens — device flash region has
plenty of room). The new builtins + wasm + gallery need the next assets
push / firmware flash to reach the device; your morning serial flash
picks up ALL of it in one go.

## 2026-07-06 ~10:00 — ⚠ MY MISTAKE: device is offline on ota_0 (needs one serial touch when you're up)

Right after proving OTA works, I pushed a build of the new builtins from
MY shell — which doesn't have your `LUXEL_SSID`/`LUXEL_PASS` exported —
so the image baked **no WiFi creds** and booted into offline render-only
mode. The device is healthy (rainbow on ota_0, serial confirms "no wifi
credentials; offline mode") but unreachable over the network, so I can't
fix it remotely. This is the exact "hard lesson" from 2026-07-05 that I
had recorded and failed to apply. Sorry.

**Recovery (one step, when you're up):**
```
cd firmware && BOARD=board-pixelblaze-v3 ./build-esp32.sh flash
```
That flashes the current build (**v0.1.5**) — which now includes the
batch-2 builtins, auto-baked creds (see below), AND flash-stored WiFi
credentials: the moment it's up I'll `POST /api/wifi` once and the creds
live in the nvs partition forever — after that even a credless image
joins the network, so tonight's failure mode is structurally dead. Alternative if you prefer not to flash:
`espflash erase-region 0xd000 0x2000` clears otadata → boots factory
(v0.1.4 with creds) and I'll OTA the rest myself.

**So it can't happen again (committed):**
- `firmware/creds.env` (git-ignored) now holds the dev creds;
  `build-esp32.sh` auto-sources it — every build bakes creds no matter
  whose shell runs it.
- `tools/ota-push.sh` now **refuses to push any image that doesn't
  contain the SSID string** (grep -a for it in the binary).
- The real cure stays queued: NVS-stored credentials so images never
  carry creds at all.

## 2026-07-06 ~09:40 — v0.1.4 flashed (thanks!) — OTA fully verified, remote work unblocked

After your serial flash, hardware-verified the fix end to end:
- The 190 KB JS + 87 KB wasm (the files that used to stall/crash) served
  repeatedly, cleanly, ~1.5 s / 0.8 s each. Zero panics.
- Full OTA round-trip: factory → ota_0 (904 KB, ok) → then a second OTA
  **while a loop hammered asset + status requests** → ota_1, ok. The
  worst-case combined load that used to be instant death now just works.
- Serial log: zero new panics since the flash.

Device: ota_1, v0.1.4, ~120 fps, heap_free ~72 KB. The A/B loop is
proven again and I can push everything else tonight over OTA. Carrying on
with M3 + the ideas backlog; log entries below as they land.

## 2026-07-06 ~09:00 — OTA crash ROOT CAUSE found + fixed (v0.1.4) — this supersedes the 08:15 note

You were right that something was fundamentally wrong. The serial log had
24 panics and every single one is the same signature: **stack overflow on
the main task** ("write to the stack guard value on ProCpu"), caught during
a flash read in the HTTP serving path.

Three compounding causes:

1. **The main task stack was 15.6 KB, by accident.** esp-hal gives the
   main task "whatever RWDATA is left after .data/.bss" — and our 120 KB
   heap static ate almost all of it. That one stack runs the entire
   embassy executor (every task's poll), picoserve's deep response path,
   AND the WiFi level-6 NMI frames, which land on whatever stack is
   current. Measured in the ELF: 15,596 bytes. The captured panic's SP was
   already 1.7 KB *past* the stack end.
2. **esp-storage's `FlashStorage::read` puts a 4 KiB sector bounce-buffer
   on the caller's stack** — every asset chunk we served pushed 4 KB onto
   that already-deep stack at maximum depth. (This is the "library
   function" trap: the convenient `Storage::read` API is the wrong one;
   `read_nor` with aligned offset/len/buffer reads directly, zero stack.)
3. WiFi NMI on top of 1+2 → guard hit → panic → reboot. It looked like
   "OTA crashes" because OTA sessions are exactly when the page/status/
   asset traffic and flash ops coincide; the erase-on-write fix (v0.1.2)
   was real but treated a different, secondary hazard.

The v0.1.4 fix (firmware only, two small changes): heap 120→96 KB so the
main stack is now **60 KB** (measured in the ELF: 61,712 bytes; heap_free
still ~70 KB), and `assets::read_chunk` now uses `read_nor` through a
word-aligned heap staging buffer — no more stack bounce-buffer in the
serving path.

On your "reinventing the wheel" hunch: partially right, wrong culprit.
ota.rs does hand-roll erase/write at raw offsets where
esp-bootloader-esp-idf's `OtaUpdater::next_partition()` hands you a
bounds-checked `FlashRegion` — worth cleaning up later — but that code
wasn't the crash; the reads/stack were. Queued the cleanup.

**I attempted OTA of v0.1.4 myself** (fails safe: a crash mid-upload just
reboots to the current slot). Check /api/status — if it says 0.1.4, go to
bed, nothing needed. If it still says 0.1.3, one serial flash:
```
cd firmware && BOARD=board-pixelblaze-v3 ./build-esp32.sh flash
```

## 2026-07-06 ~08:15 — ⚡ PLEASE FLASH THIS before bed (v0.1.2, task-16 fix)

You asked what I need flashed to unblock M3 remote work. This is it:
**`67f2111` fixes the OTA-vs-flash hazard** (erase-on-write interleaved
with network reads instead of a watchdog-tripping pre-erase burst; plus
a Timer-yielded asset-serving path). Once this is on the device, I can
OTA everything else remotely — settings, AP provisioning refinements,
all the ideas.md work — without needing you.

Flash command (creds already baked, so it just connects):
```
cd firmware && BOARD=board-pixelblaze-v3 ./build-esp32.sh flash
```
(the runner erases otadata → boots the flashed image; leave the monitor
running as usual). It's v0.1.2 — `/api/status` will show that once up.

Fails safe as always: if the erase-on-write change has any issue, a bad
OTA just reboots to the current slot; the flashed build itself connects
and live-codes regardless (compile-time creds unchanged). **AP-mode
provisioning still needs your phone to verify** (a device with no creds
leaves WiFi, so I can't reach it) — I'll build it and OTA it, but final
sign-off is yours when you're up.

Correction to my earlier note: the build you flashed a few messages ago
was HEAD *without* a task-16 fix (I'd described it but hadn't written it
yet). THIS commit is the actual fix.

Also shipped tonight while you read this: OKLCH/OKLab perceptual color
builtins (top visual-quality item from ideas.md).

## 2026-07-06 ~07:45 — extension builtins + idea backlog

Acting on your "extend beyond PB" note: wrote `docs/ideas.md` (a ranked
backlog across builtins / language / engine / audio / integration /
playground, with your animated-pattern-browser idea and its
1D-bar-vs-2D-rectangle preview distinction), and shipped the first
cheap-but-high-value batch of builtins: `map()`, `sign`, `step`,
`saturate`, `dist`/`dist3`, and the six `easeIn/Out/InOut` `Quad`/`Cubic`
curves. All pure math, so no PB pattern is affected (new names PB
doesn't have). They're in the VM, autocomplete, and language docs, with
tests. My top-5 picks are in ideas.md — `map`+easing+oklch color helpers
and array literals are the highest authoring-quality-per-effort; I2S
mic/FFT and MQTT/HA are the headline features when we want them.

## 2026-07-06 ~07:15 — const/let landed; caught a real playground bug

`const` and `let` now work (your request). `let` == `var` for now
(function-scoped; block scoping later); `const` needs an initializer and
errors on reassignment, correctly scoped (a local const doesn't lock a
same-named global). Full stack: lexer → AST → parser → compiler, 3 new
tests, editor keyword completion, and docs/lang.md updated with a
"Luxel extensions" section. All 32 semantics tests green.

Writing the e2e for it surfaced a genuine bug I'd introduced with the
device-served playground: the auto-connect probe fetched `/api/status`,
and a dev server answers that with its SPA fallback (200 HTML), so the
local playground tried to "connect to a device", failed, and left a
stuck "device unreachable" banner. Fixed to require real device JSON.
Good catch — it would've bitten the normal local playground, not just
the test.

Queued your other note (extend builtins/features/language — the "not
limited to PB" thesis) as a design task for a docs/ideas.md brainstorm.

The device is back (you re-flashed it) and healthy; I've deliberately
kept my remote testing light after the earlier WiFi-wedge, and the
flash-vs-WiFi fix (task 16) is the thing that unlocks safe remote
iteration again.

## 2026-07-06 ~06:30 — full playground serves from flash (works!) + an OTA regression to fix, and I've STOPPED touching the device

Big win and an honest problem, both real.

**Works, browser-verified against the real PB:** the whole Svelte
playground now lives in the device's flash and serves from it. New
pieces: an "assets" flash partition (0x310000, 960 KB) behind the OTA
slots; a `LUXA` archive format (`web/tools/pack-assets.mjs` packs the
gzipped `dist/` — 279 KB, 4 files); `POST /api/assets` streams it in
(independent of firmware OTA, hot-reloads the TOC, no reboot); the
server serves `/`, `/luxel.wasm`, etc. from flash with
`Content-Encoding: gzip`, embedded minimal page demoted to `/min`; and
the playground auto-enters device mode when same-origin `/api/status`
answers. Chromium loaded `http://192.168.0.205/` end to end: editor up,
device badge, 300-px preview, ~100 fps. Bumped the connection pool to 3
(safe now the stack overflow is fixed) so the preview socket doesn't
starve page/asset loads.

**Two real bugs, same root cause — the ESP32 flash-vs-WiFi hazard:**
1. Serving a *large* asset (JS 190 KB, wasm 87 KB — multi-chunk) stalls:
   interleaving esp-storage flash reads with WiFi TCP writes starves the
   executor, so the second `write_all` never drains. Single-chunk files
   (html, css) serve fine. Tried 8 KiB chunks + `yield_now` between
   writes — not enough.
2. **OTA now trips a hardware watchdog** during the erase phase when
   assets are installed: the device resets (no panic — a watchdog, not a
   Rust panic) ~14 s in, connection reset. It **fails safe every time** —
   always reboots back into the working build — but it means I can no
   longer push updates remotely, and I can't fix a running firmware's OTA
   path *via* OTA (chicken/egg).

**Decision: I stopped experimenting on the device.** It's in a stable,
fully-working state (serves the page, WS, live-code, small assets — all
good). Continuing to hammer it risked leaving it wedged for you, and I
was starting to see transient wedges under my own back-to-back tests.
The right fix for BOTH bugs is the standard one: **memory-map the assets
flash region** and read via the cached data bus instead of esp-storage
flash-controller ops — no cache-off windows, no WiFi starvation. That's
a clean next-session task (needs a serial flash to land, since it
changes the running OTA path). The source for everything above is
committed and sound; only the runtime flash-timing needs that rework.

**To get back to a fully-updatable device:** one serial flash of a build
with the mmap fix. Until then the PB happily runs what's on it.

Everything else tonight (debugger fixes, autocomplete, bidirectional WS,
language docs) is landed, hardware-verified where relevant, and
independent of this.

## 2026-07-06 ~05:15 — WS verdict + bidirectional multiplexing live on hardware (8daf311)

After your last flash the fixed build held: five status hammers, zero
panics, heap 121 KB free. Then the A/B you asked for, on the real PB:

|            | HTTP polling | WS push |
|------------|--------------|---------|
| rate       | 10.0 fps     | 9.7 fps |
| gap p50    | 87 ms        | 96 ms   |
| gap p90    | 143 ms       | 130 ms  |
| gap p99    | **394 ms**   | **185 ms** |

Same average rate, but the tail — the visible stutter — is halved.
Combined with freeing a connection slot, WS stays.

Your bidirectional suggestion then proved *necessary*, not optional:
with the push socket pinning one of the chip's two connections, extra
HTTP callers starved (reproduced: "fetch failed" under mixed load). Now
one socket carries everything — pixel push down, API calls up
(`"<id> call\nbody"` → `{"id":…,"r":…}`), playground multiplexes
transparently with HTTP fallback. The native mirror was rewritten from
tiny_http to a hand-rolled std HTTP layer so its /ws loop is
structurally identical to the firmware's (single-threaded full-duplex).

Hardware-verified end-to-end: 6 live-code pushes + control sets +
pattern fetch over one socket while pixels streamed and a concurrent
plain-HTTP request succeeded; zero panics. 12/12 device e2e, 25/25
local e2e, all tests green. Serial flashing also hardened along the
way: `flash` now erases otadata (the image you flash is the image that
boots — the mismatch that caused the 4 AM round).

## 2026-07-06 ~02:20 — INCIDENT: device down after WS OTA (my fault); recovery staged

The WS-push build OTA'd fine but the device never came back. Doing the
arithmetic I should have done first: bumping the server pool to 3 tasks ×
~32 KB buffers on top of WiFi's ~90 KB almost certainly exhausted the
ESP32's 184 KB heap at boot → allocation panic → and esp-backtrace's
default behavior is to HALT, so one panic = a brick until someone touches
power. You were asleep, serial detached; nothing I can do remotely — the
one failure mode OTA can't recover, found the hard way.

**When you're up: power-cycle the PB once.** A persistent watcher pushes
the staged recovery build (pool back to 2, smaller buffers, and — the
real fix — panics now reboot after 3 s instead of halting forever)
automatically the moment the device answers. If a power cycle alone
doesn't bring it back (crash loop), it'll still recover: each loop
iteration reboots through a WiFi window. Absolute worst case:
`espflash erase-region 0xd000 0x2000` over serial clears otadata → boots
factory.

Meanwhile: docs/lang.md (full language reference) written; continuing
with hardware-independent work (bidirectional WS protocol + remote
debugger against the native mirror, const/let).

## 2026-07-06 ~01:45 — WS pixel push implemented, hardware A/B in progress

Implemented across all four surfaces (firmware /ws, native mirror,
playground, minimal page): binary frames = pixels ~15 Hz, text frames =
typed JSON (status+controls, vars/readouts). 12/12 device e2e green.

Per your note, measuring on the real PB before keeping it. Baseline
captured first, HTTP polling against the previous firmware:
**10.0 fps, gap p50 87 ms / p90 143 ms / p99 394 ms** — that jitter is
the choppiness you saw. WS build OTA'd (861 KB, no size tricks needed
now); push measurement running.

Your bidirectional-WS point is sound and the groundwork exists:
picoserve's next_frame takes a cancel signal that safely unblocks
between frames, so one socket can carry API calls + push concurrently.
If the numbers favor WS, I'll multiplex code/control/var (and later the
remote-debug protocol) onto the same connection.

## 2026-07-06 ~01:00 — editor autocomplete (140a8c6)

Typing pops completions for all 86 builtins with full signatures and
one-line docs (new `web/src/lib/builtins.ts`, kept in sync with the VM
table — it'll also seed the language reference). Predefined constants
(PI, pixelCount, GPIO modes…), keywords, and identifiers you've already
used in the pattern complete too. Verified in chromium; both e2e suites
still green.

Also queued your `const`/`let` language-extensions request as a task —
it needs compiler work (block scoping, assignment-to-const errors), so
it's sequenced after the device-facing items tonight.

## 2026-07-06 ~00:30 — debugger notes resolved (060884b)

**Your `heat` question: the debugger is right.** In Palette Fire 2D, `heat`
is assigned inside `render2D` *without* `var` — in PB's JS-derived scoping
(and ours), undeclared assignment creates a global. `var` inside a function
would make it local. Pinned with a test so it stays true.

**Breakpoint line mismatch: real bug, fixed.** Resolution only matched
exact lines, so a breakpoint on a blank/comment/brace line silently
installed *nothing* (and the dot pointed at a line that could never stop).
Now it snaps forward to the nearest executable line and the gutter dot
moves there — dot and stop-line always agree. While hunting this I
verified the rest of the chain is correct: pause happens *before* the
stopped line executes (so its variables show pre-execution values),
stepping tracks across beforeRender→render→pixels, and the paused line
scrolls into view.

Running log of autonomous work, newest first. Started the night of
2026-07-05 when Jeremy went to sleep ("keep going with less intervention").

## 2026-07-06 — overnight session begins

Queue for tonight, from Jeremy's list + existing plan, in intended order:

1. Debugger correctness pass: `heat`-as-global question + stopped-line
   highlight (his two debugger notes).
2. Editor autocomplete for builtins.
3. WebSocket pixel push (device + native mirror + playground + minimal page),
   then OTA to the PB and measure against HTTP polling.
4. NVS-stored WiFi credentials (kills the creds-baked-image lockout risk).
5. Pattern language docs.
6. Remote debugger design (+ implementation as far as the night allows).

Device state at session start: PB v3 at 192.168.0.205, slot ota_1, v0.1.1,
124 fps, rainbow restored after the live-code green test. OTA loop fully
proven (factory → ota_0 → ota_1 + bootloader-fallback test passed).
