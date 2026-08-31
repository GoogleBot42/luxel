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
prompts for Local Network Access, but the playground's `gatedFetch` does
not yet send the `targetAddressSpace: "local"` hint the installer page
uses (Gitea #162).

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
