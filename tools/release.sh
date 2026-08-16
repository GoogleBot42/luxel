#!/usr/bin/env bash
# Cut a release tag on the Gitea source of truth — the local/agent fallback
# for .gitea/workflows/cut-release.yml (same validation, same effect).
# See docs/releases.md for the full pipeline this kicks off.
#
# Usage: tools/release.sh [X.Y.Z] [--dry-run]
#   With no version argument, releases what firmware/Cargo.toml already says.
#
# The version must ALREADY be bumped in firmware/Cargo.toml on master —
# version bumps belong in the PR that ships the change, not here. This
# script only validates and creates the tag; the push mirror carries it to
# GitHub, whose release workflow builds and publishes the assets.
set -euo pipefail
cd "$(dirname "$0")/.."

DRY=0
V=""
for a in "$@"; do
  case "$a" in
    --dry-run) DRY=1 ;;
    *) V="${a#v}" ;;
  esac
done

TOML="$(sed -n 's/^version = "\(.*\)"$/\1/p' firmware/Cargo.toml | head -1)"
V="${V:-$TOML}"

echo "$V" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+([-.][0-9A-Za-z.-]+)?$' \
  || { echo "invalid version '$V' (expected X.Y.Z)" >&2; exit 1; }
[ "$TOML" = "$V" ] \
  || { echo "firmware/Cargo.toml says '$TOML', not '$V' — bump the version in a PR first" >&2; exit 1; }

# tag must be new, and cut from current origin/master (not local drift)
git fetch -q origin master "refs/tags/*:refs/tags/*" 2>/dev/null || true
git rev-parse -q --verify "refs/tags/v$V" >/dev/null \
  && { echo "tag v$V already exists" >&2; exit 1; }
HEAD_SHA="$(git rev-parse origin/master)"
LOCAL_TOML="$(git show "origin/master:firmware/Cargo.toml" | sed -n 's/^version = "\(.*\)"$/\1/p' | head -1)"
[ "$LOCAL_TOML" = "$V" ] \
  || { echo "origin/master's firmware/Cargo.toml says '$LOCAL_TOML', not '$V' — merge the bump first" >&2; exit 1; }

echo "would tag: v$V at origin/master ($HEAD_SHA)"
if [ "$DRY" = 1 ]; then
  echo "DRY RUN — nothing created."
  exit 0
fi

# Prefer the Gitea API (tea) — the agent has no direct push rights on the
# main repo, and a Gitea release-with-tag mirrors to GitHub the same as a
# pushed tag. Fall back to a plain tag push for users with push rights.
if command -v tea >/dev/null 2>&1; then
  tea release create --repo zuckerberg/luxel --tag "v$V" --title "v$V" \
    --note "Release assets (firmware images + web app) are built and published on the GitHub mirror: https://github.com/GoogleBot42/luxel/releases/tag/v$V"
else
  git tag -a "v$V" -m "release: $V" "$HEAD_SHA"
  git push origin "refs/tags/v$V"
fi
echo "Tag v$V cut. The push mirror carries it to GitHub, whose release"
echo "workflow builds and publishes the assets:"
echo "  https://github.com/GoogleBot42/luxel/actions"
