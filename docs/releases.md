# Releases

How a luxel version becomes downloadable firmware. Modeled on
open-nanokvm-pro's pipeline (same forge, same constraint: the Gitea
source of truth is Tailscale-only, so public artifacts live on GitHub).

## Architecture

```
git.neet.dev/zuckerberg/luxel        github.com/GoogleBot42/luxel
  (source of truth, private LAN)  ──►  (read-only push mirror, public)
        cut a tag vX.Y.Z            tag arrives → .github/workflows/release.yml
                                     builds firmware + web assets (nix)
                                     publishes the GitHub release
```

- **Gitea is the source of truth.** All development, PRs, and tags happen
  there. The push mirror replicates commits and tags to GitHub.
- **GitHub is a read-only mirror + build farm.** Nothing on GitHub ever
  creates commits or tags (that would diverge from the mirror); its only
  job is building release assets when a mirrored tag lands. The release
  workflow is guarded with `github.server_url == 'https://github.com'`
  because Gitea Actions also picks up `.github/workflows/`.

## CI (the test gate)

Separate from the release pipeline above, and the only workflow that runs on
every change: `.gitea/workflows/ci.yml` runs the standing acceptance suite on
every **push to `master`** and every **pull request**.

The steps are not in the YAML — they are in `tools/ci.sh`, so the gate is
runnable locally, byte for byte:

```
nix develop --command tools/ci.sh
```

Four steps, in a load-bearing order (`CI_SKIP="web cargo library firmware"`
drops any of them while iterating; `CI_BOARDS` replaces the firmware board
list and `CI_BOARD` builds just one variant):

1. **web** — `npm ci && npm run build && npm test` (wasm, gen-gallery,
   svelte-check, vite build, then the pure unit tests). First because
   `luxel-cli`'s `heapstat` test reads `web/public/gallery.json`, which the
   web build writes.
2. **cargo** — `cargo test --workspace`.
3. **library** — `tools/check-library.sh`, the five-rig library sweep.
4. **firmware** — two halves since 2026-09-08 (Gitea #413/#438).

   **4a, the devshell build.** `BOARD=$CI_BOARD firmware/build-esp32.sh`
   (default `board-pixelblaze-v3`, build only), which ends in
   `tools/image-check.sh` on the ELF for the load-bearing-feature markers.
   This is what keeps `build-esp32.sh` itself — the script every developer
   and every device deploy runs — covered.

   **4b, the release images.** `nix build .#luxel-fw-<variant>` plus
   `tools/image-check.sh` on the resulting `luxel-fw-ota.bin`, for three
   variants:

   | variant | why it is in the gate |
   |---|---|
   | `pixelblaze-v3` | the Xtensa toolchain; stands in for athom-music, esp32-generic, s3-devkit, s3-hub75 and seengreat-hub75. Since #501 it is also the one gated variant with `wled-takeover` **off**, so image-check's absent-marker half is exercised |
   | `c6-devkit-hosted` | the tightest image in the fleet, and the exact C6 variant `release.yml` ships — the one that trips image-check's 3 % OTA-slot floor first |
   | `c3-devkit` | the only `riscv32imc` target: no A extension, so no atomic read-modify-write |

   (All three build `core`/`alloc` from source since #501 — the RISC-V pair
   through `RUSTC_BOOTSTRAP=1` — so a std-side codegen break shows up on both
   toolchain axes, not just the Xtensa one.)

   Gating one board was how two release-gate breakages merged green on the
   same day (2026-09-07): `board-c3-devkit` stopped compiling outright
   (Gitea #413/#422) and the shipped C6 image slipped under the margin floor
   (Gitea #438). Nothing between a merge and a `v*` tag noticed, because the
   eight-image matrix lives in this repo's *release* workflow and only runs
   at a tag. The three above cover the toolchain axis, the margin axis and
   the atomics axis; the other five images differ only in pin maps and
   features these already compile.

   **Why the flake image and not the `espflash save-image` one ci.sh used to
   make.** They are not the same artifact, and the difference is bigger than
   the thing being measured. A devshell build bakes WiFi credentials and
   embeds the *absolute path* of every dependency source file in its panic
   `Location`s (~13.5 KB of `…/.cargo/registry/src/index.crates.io-<hash>/…`
   strings), so it reads about **2.8 KB larger** than the credless flake
   image — and by a *different* amount on every machine, because those paths
   are a different length there. Measured on the same commit: 1,014,400 B
   from the flake, 1,015,568 B in this repo's devshell, 1,017,168 B on the
   CI runner (which builds under `/var/lib/gitea-runner/inst/.cache/act/…`).
   Against a 3 %-of-1-MiB floor that is a 0.27-percentage-point swing decided
   by the checkout path. `nix build .#luxel-fw-<variant>` is the derivation
   release.yml builds, so what image-check weighs here is what gets
   published. Trimming those paths out of the image is Gitea #441.

   Knobs: `CI_BOARD` picks the devshell board; `CI_VARIANTS` replaces the
   image list (spelled as in release.yml's matrix), and `CI_VARIANTS=`
   skips the image half entirely.

What it is **not**: no device, no browser e2e, no soak. The hardware gates in
docs/tools.md still have to be run by hand.

**One build per ref.** The workflow takes a `concurrency` group named
`luxel-ci-<ref>` with `cancel-in-progress: true`: a new push to a branch
cancels that branch's own stale build, and runs for different refs queue
behind each other. The group used to be global (one build repo-wide, so a
superseded build never cost the shared runner time), but once several agent
sessions were opening PRs in parallel every push on any branch killed every
other branch's run — the cancelled run reports as commit-status `failure`,
and each PR needed two or three manual re-queues before it could merge
(2026-09-19). Queue time is the price; wasted reruns were dearer.

**Runner.** `runs-on: nixos` — the host-mode runner (label `nixos:host`): a
NixOS container that shares the host nix-daemon and `/nix/store` and keeps a
persistent `/var/lib/gitea-runner` home. The shared store is what makes a
second run cheap (see the numbers below); the build tree is *not* carried
over, because `actions/checkout` cleans it. The workflow-level `env: PATH:
/run/current-system/sw/bin/` is required here — without it the job's PATH has
neither `nix` nor `git` and even `actions/checkout` fails. (`ubuntu-latest`
exists on the same server but has no nix.)

**This runner is legacy and is being retired.** The replacement is
`nixos-podman`, which was broken as of 2026-09-01. When the switch happens:

- Delete the `PATH:` env — the podman job image has its own `PATH=/bin`, and
  overriding it hides even `bash`.
- Expect **every run to be cold**. Each job gets a fresh
  `localhost/gitea-runner-nix` container with the host `/nix` mounted
  read-only under a throwaway overlay and no persistent workspace: no
  `target/`, no `node_modules` (the `nixos` runner does not really carry
  those either), and nothing the job realizes survives — the nix store is
  the part that stops being free.
- The container ships no `/usr/bin`, so the repo's `#!/usr/bin/env bash`
  shebangs need a `ln -s "$(command -v env)" /usr/bin/env` step, and it
  starts with `HOME` unset (nix then computes a *relative* cache dir and
  dies with `not an absolute path: ".cache/nix"`) and with no build-users
  group (`the group 'nixbld' ... does not exist` on the first derivation it
  has to realize).

A rehearsal on `nixos-podman` did pass end to end on 2026-09-01 (run 1441):
10 min 45 s wall, of which ~8 min was realizing the devshell — the gate
itself was 125 s.

The lever against that cold start is Jeremy's **attic** binary cache, the
same one nix-config's `build-and-cache.sh` uses. The workflow's two attic
steps are conditional: with no `ATTIC_TOKEN` they print "attic not
configured … running uncached" and skip; with credentials they
`attic login` + `attic use` before the gate, and afterwards push the devshell
closure (realized into a profile, since a `mkShell` derivation can't be
`nix build`-ed directly). Turning it on is two repo settings — an
`ATTIC_ENDPOINT` Actions **variable** and an `ATTIC_TOKEN` Actions
**secret** on `zuckerberg/luxel` (Gitea #246, which also tracks the podman
migration and making this check required on master).

**Measured** on the `nixos` runner, 2026-09-02 — run 1443 (the first ever
for this repo) and run 1448 straight after it:

| | wall | `nix develop` | the gate |
|---|---|---|---|
| 1443, cold | 5 min 43 s | 3 min 44 s | 110 s |
| 1448, warm | 2 min 58 s | 21 s | 143 s |

Read that carefully: **the warm saving is entirely the nix store, not the
build tree.** Entering the devshell drops from 3 min 44 s to 21 s because the
closures stay in the host store — but the gate itself does not speed up at
all (it re-`npm ci`'d 177 packages and recompiled everything both times),
because `actions/checkout` defaults to `clean: true`, i.e. `git clean -ffdx`,
which deletes the gitignored `target/` and `web/node_modules` at the top of
every job. That is left as is on purpose: two and a half minutes of honest
rebuild beats a correctness gate that can go green on stale artifacts (the
repo has been bitten by stale `web/public` and `target/` more than once —
see CLAUDE.md's tripwires). Per-step on the warm run: web 32 s · cargo 25 s ·
library 19 s · firmware 67 s.

The runner is single-slot and shared with every other repo on the server, so
**queue time dwarfs run time** — 1443 waited 27 minutes and 1448 waited 46,
both behind the same nix-config flake check. Read the job's own duration, not
the wall clock from your push.

No guard is needed on this file. Gitea Actions also picks up
`.github/workflows/`, which is why release.yml and pages.yml carry
`github.server_url == 'https://github.com'`; `.gitea/workflows/` is invisible
to GitHub, so ci.yml can only ever run on Gitea.

CI writes a placeholder `firmware/creds.env` (`ci-placeholder`) because
`build-esp32.sh` sources it. Real credentials never reach CI, and CI never
publishes or flashes an image.

## Cutting a release

The version lives in `firmware/Cargo.toml` and is bumped **in the PR that
ships the change** (existing practice — e.g. the v0.1.36 bump rode the
flash-fairness PR). Cutting a release is then just creating the matching
tag; every path validates tag == Cargo.toml before anything is pushed:

1. **Gitea web UI**: Actions → cut-release → Run workflow → enter the
   version. (`.gitea/workflows/cut-release.yml`; has a dry-run option.)
2. **Locally / agent**: `tools/release.sh [X.Y.Z] [--dry-run]` — same
   validation; creates the tag via the Gitea API (`tea`) or a direct tag
   push if you have push rights.

Then watch https://github.com/GoogleBot42/luxel/actions — the release
appears at https://github.com/GoogleBot42/luxel/releases when the build
finishes (~30–60 min; four of the six boards are slow Xtensa
`-Zbuild-std` builds). The job is idempotent: re-run it to recover from a
transient failure — it upserts the release and re-uploads missing assets.

### The migrating release (Gitea #501) — and taking the switch back out

The 2026-09-20 repartition gave the fleet bigger OTA slots (docs/firmware.md,
"Partition tables"). Devices move themselves, on the first boot of ONE
release, and that release has a different size gate from every other:

- **`MIGRATING_RELEASE=1`** makes `tools/image-check.sh` weigh every board's
  image against the **old 1,048,576 B slot** instead of the board's new one,
  with the margin floor relaxed to **0 %**. The reason is not a preference:
  a device still on the pre-#501 table is what installs this image, and its
  own running firmware writes it into a 1 MiB slot. An image that only fits
  the new 1.25 MiB slot would be rejected at `/api/ota` and nobody could
  migrate. The floor comes down because the repartition is the thing that
  ends the squeeze — holding 3 % of the old slot would block the release
  that makes the slot bigger. Margins as measured are in docs/boards.md;
  the tightest is 2,992 B (0.28 %) on `board-c6-devkit`.
- **`.github/workflows/release.yml` carries it workflow-wide**
  (`env: MIGRATING_RELEASE: "1"`), and `tools/ci.sh` exports it through to
  image-check when it is set in the environment.
- **It must be REMOVED in the release after this one.** Leaving it in
  silently keeps gating the whole fleet at 1 MiB and throws away the
  headroom the repartition bought; the floor then goes back to 3 % of the
  per-board slot, where there is finally room under it (20–25 % free on the
  4 MB boards, 68.6 % on the Seengreat). The block in release.yml says so
  above itself; this is the second copy of that reminder.
- **The release notes for this version must say it is a prerequisite for
  every later one.** A release after this one may exceed 1 MiB and therefore
  cannot be installed on a device that has not migrated. `/api/ota` refuses
  such an upload up front, before erasing a sector, and on an un-migrated
  device the error names the migrating release — but a user reading release
  notes should not have to discover that from an error string.
- **Later, `migrate-off`** (tracked with the switch removal as Gitea #635).
  Once the fleet has moved, a release can be built
  with that cargo feature and get the migrator's ~12 KB of OTA slot back.
  It is deliberate by construction: image-check asserts the migrator's
  marker is linked unless the feature is named, and absent when it is. Do
  not combine it with anything a device on the old table might be handed.

## What gets published

Per board (`c3-devkit`, `pixelblaze-v3`, `athom-music`, `esp32-generic`,
and — **untested on metal**, see docs/boards.md — `s3-devkit`,
`s3-hub75`, `seengreat-hub75`). The untested boards are published as
artifacts only; the installer page (web/flash.html) does not list them.
The C6 is the exception: since 2026-09-06 it ships **only** as
`c6-devkit-hosted` (below) — the on-device-UI build fell under the 3 %
OTA-slot floor with the pattern extent allocator (Gitea #281), and
restoring it is Gitea #291:

| asset | what it's for |
|---|---|
| `luxel-<board>-<ver>-ota.bin` | App-only image: `POST /api/ota`, and the image WLED's `/update` page accepts for the WLED→Luxel takeover (docs/wled-migration.md). Size-guarded against the board's OTA slot — or against the old 1 MiB one while `MIGRATING_RELEASE=1` is set, see above. |
| `luxel-<board>-<ver>-full.bin` | Full-flash image (bootloader + partition table + app + **web assets**): `espflash write-bin 0x0 <file>` — new-device bring-up and full restores. Composed exactly like `firmware/build-esp32.sh image`. |
| `luxel-<board>-<ver>.luxr` | **Release package** (Gitea #643): the app image AND that release's web assets in one container, installed as a single action from an already-running device's own console — Settings → Advanced → Firmware & recovery → **Update…**. This is the recommended way to update an installed device. Not built for `c6-devkit-hosted`, which serves no on-device console. |

### The `.luxr` container

A small header then the two payloads, little-endian:

    off  len  field
      0    4  magic "LUXR"
      4    2  container format (u16), currently 1
      6    1  board-name length (bytes)
      7    1  firmware-version length (bytes)
      8    4  app image length (u32)
     12    4  assets archive length (u32; 0 = firmware only)
     16   32  sha256 of the app image
     48   32  sha256 of the assets archive
     80    n  board name, UTF-8 — the image's `board::NAME`
     ..    m  firmware version, UTF-8
     ..    .  app image bytes
     ..    .  LUXA/LUX2 assets archive bytes

Both hashes are verified on parse, because what follows them is written into
an OTA slot. The board name is the same string `/api/status` reports as
`board` and `tools/ota-push.sh` greps an image for, so the console refuses a
package built for another board before streaming a byte (#389's lesson).

One codec, three callers: `web/src/lib/luxr.ts` is shared by the browser, by
`web/tools/pack-luxr.mjs` (which this workflow and `tools/deploy.sh --package
<out.luxr>` both run) and by `web/tests/luxr.test.mjs`. The bench and CI
therefore cannot produce different containers.

**Why the package exists.** Firmware and the on-device console are versioned
together and were shipped separately, and nothing made anyone install the
second. A device that took a firmware OTA across an LXBC format bump kept
serving the console that came with its OLD firmware — which could not compile
anything the new engine would run, on a store the new engine could not read.
The Athom went dark exactly that way on 2026-09-20 (Gitea #643). See
docs/firmware.md, "Bytecode format bumps", for the policy that goes with it.

One extra pseudo-board, `c6-devkit-hosted`, ships the same two images built
with the **`hosted-ui`** cargo feature (Gitea #11): no on-device playground
at all — `/` serves the embedded page that links to the hosted playground
with `?device=` prefilled, and its `-full.bin` leaves the assets partition
erased. It exists because the C6 owns the fleet's tightest OTA-slot margin,
and since 2026-09-06 it is the *only* C6 artifact; any board can be built
this way (`EXTRA_FEATURES=hosted-ui`), it just isn't worth an artifact
each. See docs/boards.md for the mode and its numbers.
The installer page skips it like any other board id it doesn't know.

Plus, once per release:

| asset | what it's for |
|---|---|
| `luxel-web-assets-<ver>.luxa` | The packed web app alone: `POST /api/assets`. |
| `luxel-web-dist-<ver>.tar.gz` | The playground as plain static files — host anywhere (playground mode needs no device). |
| `luxel-elfs-<ver>.tar.gz` | Per-board ELFs for symbolicating panic backtraces (`tools/decode-backtrace.sh`). |
| `sha256sums.txt` | Checksums of everything above. |

## Two properties release builds guarantee

- **Credential-free by construction.** The firmware images come from the
  flake's *pure* builds (`nix build .#luxel-fw-<board>`), which cannot see
  the environment — no WiFi creds are baked (contrast dev builds, which
  source `firmware/creds.env`). A credless device boots the AP-mode
  provisioning flow (open AP `luxel-xxxx`, captive portal) — that IS the
  public setup path. Never wire credentials into the release workflow.
- **Corpus-free by construction.** A fresh clone has no `corpus/`
  (gitignored, never redistributed), so the packed gallery is built from
  the clean-room `library/` alone and `pixelblaze-library.json` simply
  isn't produced. Release web bundles are license-clean without any
  filtering step.

## The installer site (GitHub Pages)

`.github/workflows/pages.yml` (separate from release.yml) composes and
deploys `site/` = the whole web dist (playground + the WLED→Luxel
installer, `flash.html`) + `firmware/` (the **latest release's**
per-board OTA images and LUXA bundle, downloaded via the GitHub API,
plus `manifest.json` via `web/tools/gen-flash-manifest.mjs`). Triggers:
mirrored master pushes touching `web/`/`library/`, every published
release, and manual dispatch — so installer/web changes go live without
waiting for a firmware release, and new firmware refreshes the site's
binaries. Reason for co-hosting: GitHub's release-asset downloads send
no CORS headers (verified 2026-08-15), so a browser page can only fetch
firmware binaries same-origin — see docs/wled-migration.md. The site
lands at `https://googlebot42.github.io/luxel/` (installer at
`/flash.html`).

That URL is also hardcoded in the firmware's embedded fallback page
(`firmware/src/index.html`, served at `/` when no assets are installed —
always, in a `hosted-ui` build — and at `/min`): it links to
`https://googlebot42.github.io/luxel/?device=http://<this device's
host>`, built client-side from `location.host`, so a device with no
on-flash UI is still one click from a working console (the playground
honours `?device=` and the firmware serves `Access-Control-Allow-Origin:
*`). If the Pages URL ever moves, that anchor moves with it. Caveat: the
Pages copy is https and devices are http — Chromium exempts
recognised-local targets (RFC1918 / `.local`) from mixed content and
prompts for Local Network Access. The playground now sends the same
`targetAddressSpace: "local"` hint the installer page does (both go
through `web/src/lib/lna.ts`), and when the request is refused anyway it
says so and points at the manual routes — open the console from the
device itself, or host the UI on a plain-http LAN address. Whether the
*granted* path actually works end to end from the Pages URL is still
unproven: headless chromium won't run the permission flow, so it needs a
headful browser against a real device (Gitea #162).

## One-time infrastructure (state as of 2026-08-15)

- Push mirror Gitea → GitHub: **already configured and syncing** (the
  GitHub repo tracked a Gitea merge within minutes). Whether the mirror
  carries *tags* gets proven by the first release — if a tag doesn't show
  up on GitHub, check the mirror's "sync all refs / tags" setting in
  Gitea repo Settings → Mirror.
- Gitea Actions must be enabled on the repo for the cut-release UI path
  (open-nanokvm-pro uses it on the same server, so a runner exists);
  `tools/release.sh` works regardless.
- GitHub Pages: Settings → Pages → Source: **GitHub Actions** — enabled
  by Jeremy 2026-08-15.
