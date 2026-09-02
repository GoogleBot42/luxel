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
queueing behind it, and two Xtensa `-Zbuild-std` builds never fight over the
runner's shared `target/`.

**Runner.** `runs-on: nixos`, the host-mode runner: a NixOS container sharing
the host nix-daemon and `/nix/store`, with a persistent
`/var/lib/gitea-runner` home. The persistence is the point — the flake's
toolchain closures stay in the store and the checkout's `target/` and
`web/node_modules` survive between runs, so warm runs are incremental.
(`nixos-podman` discards the workspace per job; `ubuntu-latest` has no nix.)
The workflow-level `env: PATH: /run/current-system/sw/bin/` is required on
this runner — without it not even `nix` or `git` are found.

Measured: RUNTIMES

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
