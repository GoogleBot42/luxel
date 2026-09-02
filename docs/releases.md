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
drops any of them while iterating; `CI_BOARD` picks a different board):

1. **web** — `npm ci && npm run build && npm test` (wasm, gen-gallery,
   svelte-check, vite build, then the pure unit tests). First because
   `luxel-cli`'s `heapstat` test reads `web/public/gallery.json`, which the
   web build writes.
2. **cargo** — `cargo test --workspace`.
3. **library** — `tools/check-library.sh`, the five-rig library sweep.
4. **firmware** — `BOARD=board-pixelblaze-v3 firmware/build-esp32.sh` (build
   only), which ends in `tools/image-check.sh` on the ELF for the
   load-bearing-feature markers; ci.sh then makes an app image with
   `espflash save-image` and runs image-check over *that* too, because the
   1 MiB OTA-slot margin gate only applies to app images, not ELFs.

What it is **not**: no device, no browser e2e, no soak. The hardware gates in
docs/tools.md still have to be run by hand.

**One build at a time.** The workflow takes a `concurrency` group named
`luxel-ci` — deliberately global, not per-ref — with `cancel-in-progress:
true`. A new build therefore cancels whatever is in flight rather than
queueing behind it. That matters twice over: sessions merge PRs
continuously, and the runner is shared with every other repo on the server,
so a superseded build is somebody else's queue time.

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

## What gets published

Per board (`c3-devkit`, `pixelblaze-v3`, `athom-music`, `esp32-generic`,
and — **untested on metal**, see docs/boards.md — `s3-devkit`,
`c6-devkit`). The untested boards are published as artifacts only; the
installer page (web/flash.html) does not list them:

| asset | what it's for |
|---|---|
| `luxel-<board>-<ver>-ota.bin` | App-only image: `POST /api/ota`, and the image WLED's `/update` page accepts for the WLED→Luxel takeover (docs/wled-migration.md). Size-guarded against the 1 MiB OTA slot. |
| `luxel-<board>-<ver>-full.bin` | Full-flash image (bootloader + partition table + app + **web assets**): `espflash write-bin 0x0 <file>` — new-device bring-up and full restores. Composed exactly like `firmware/build-esp32.sh image`. |

One extra pseudo-board, `c6-devkit-hosted`, ships the same two images built
with the **`hosted-ui`** cargo feature (Gitea #11): no on-device playground
at all — `/` serves the embedded page that links to the hosted playground
with `?device=` prefilled, and its `-full.bin` leaves the assets partition
erased. It exists because the C6 owns the fleet's tightest OTA-slot margin;
any board can be built this way (`EXTRA_FEATURES=hosted-ui`), it just isn't
worth an artifact each. See docs/boards.md for the mode and its numbers.
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
