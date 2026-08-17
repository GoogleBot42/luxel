#!/usr/bin/env python3
"""Run every QEMU-dependent test with one command.

The QEMU emulation harness (docs/research/qemu-emulation-spike.md) backs a
growing set of hardware-free tests. Each is a standalone script; this runner
is the single entry point that builds their shared inputs once and executes
them all, so "the emulator tests" is one command, not a checklist.

What it does:
  1. Builds the athom firmware (`.#luxel-fw-athom-music`) to ./result and
     Espressif's patched QEMU (`.#qemu-espressif`) to ./result-qemu — both
     nix-cached, seconds when warm. Separate out-links on purpose: building
     one flake output reuses the default ./result symlink and would clobber
     the other (worktree gotcha, .claude/skills/worktree-setup).
  2. Locates the two gitignored Athom dumps the takeover/heap tests need
     (athom-wled-stock.bin, athom-wled-fs-configured.bin) — via --stock/--fs,
     the LUXEL_ATHOM_STOCK / LUXEL_ATHOM_FS env vars, or autodetection in the
     repo root and the sibling main checkout.
  3. Runs each test as a subprocess and prints a pass/fail summary.

Tests that need the dumps are skipped (not failed) when the dumps aren't
found, so the runner still works in a checkout without them — it just reports
what it couldn't run.

Usage:
    nix develop -c python3 tools/qemu/run-all.py
    nix develop -c python3 tools/qemu/run-all.py --stock <dump> --fs <fs>
    nix develop -c python3 tools/qemu/run-all.py -k heap   # filter by name
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))


def build(flake_attr: str, out_link: str) -> str:
    """nix build <flake_attr> to <out_link>; return the out-link path."""
    print(f"  building .#{flake_attr} -> {out_link} …", flush=True)
    p = subprocess.run(
        ["nix", "build", f"{REPO}#{flake_attr}", "--out-link", out_link],
        cwd=REPO, capture_output=True, text=True)
    if p.returncode != 0:
        raise SystemExit(f"nix build .#{flake_attr} failed:\n{p.stderr.strip()}")
    return out_link


def find_dump(explicit: str | None, env: str, *names: str) -> str | None:
    if explicit:
        return explicit if os.path.exists(explicit) else None
    if os.environ.get(env) and os.path.exists(os.environ[env]):
        return os.environ[env]
    # repo root, then the sibling main checkout ("pixler")
    roots = [REPO, os.path.join(os.path.dirname(REPO), "pixler")]
    for root in roots:
        for name in names:
            cand = os.path.join(root, name)
            if os.path.exists(cand):
                return cand
    return None


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--stock", help="athom-wled-stock.bin (else env/autodetect)")
    ap.add_argument("--fs", help="athom-wled-fs-configured.bin (else env/autodetect)")
    ap.add_argument("--result-dir", default=os.path.join(REPO, "result"),
                    help="firmware build out-link (default ./result)")
    ap.add_argument("-k", "--filter", default="",
                    help="only run tests whose name contains this substring")
    ap.add_argument("--no-build", action="store_true",
                    help="skip nix builds; use existing ./result and ./result-qemu")
    args = ap.parse_args()

    print("== QEMU test harness ==")
    if args.no_build:
        result_dir = args.result_dir
        qemu = os.path.join(REPO, "result-qemu")
    else:
        result_dir = build("luxel-fw-athom-music", args.result_dir)
        qemu = build("qemu-espressif", os.path.join(REPO, "result-qemu"))
    if not os.path.exists(os.path.join(qemu, "bin", "qemu-system-xtensa")):
        raise SystemExit(f"no qemu-system-xtensa under {qemu} (run without --no-build?)")

    stock = find_dump(args.stock, "LUXEL_ATHOM_STOCK", "athom-wled-stock.bin")
    fs = find_dump(args.fs, "LUXEL_ATHOM_FS", "athom-wled-fs-configured.bin")
    have_dumps = bool(stock and fs)
    if have_dumps:
        print(f"  dumps: {stock}\n         {fs}")
    else:
        print("  dumps: NOT FOUND — takeover/heap tests will be skipped "
              "(pass --stock/--fs or set LUXEL_ATHOM_STOCK/_FS)")

    common = ["--qemu", qemu, "--result-dir", result_dir]
    dump_args = ["--stock", stock or "", "--fs", fs or ""]

    # (name, script, args, needs_dumps)
    suite = [
        ("takeover-app1", "takeover-test.py", dump_args + ["--slot", "app1"], True),
        ("takeover-app0", "takeover-test.py", dump_args + ["--slot", "app0"], True),
        ("takeover-fault", "takeover-test.py", dump_args + ["--slot", "app1", "--inject-fault"], True),
        ("heap-regions-selfheal", "heap-regions-test.py", dump_args + ["--mode", "selfheal"], True),
        ("heap-regions-rollback", "heap-regions-test.py", dump_args + ["--mode", "rollback"], True),
    ]

    results: list[tuple[str, str, float]] = []
    for name, script, extra, needs_dumps in suite:
        if args.filter and args.filter not in name:
            continue
        if needs_dumps and not have_dumps:
            results.append((name, "SKIP", 0.0))
            print(f"\n-- {name}: SKIP (no dumps)")
            continue
        print(f"\n-- {name} --", flush=True)
        cmd = [sys.executable, os.path.join(HERE, script)] + common + extra
        t0 = time.monotonic()
        rc = subprocess.run(cmd, cwd=REPO).returncode
        dt = time.monotonic() - t0
        results.append((name, "PASS" if rc == 0 else "FAIL", dt))

    print("\n" + "=" * 56)
    print("QEMU harness summary")
    print("=" * 56)
    width = max(len(n) for n, _, _ in results) if results else 0
    for name, status, dt in results:
        secs = f"{dt:6.1f}s" if dt else "   -- "
        print(f"  {status:4}  {name:<{width}}  {secs}")
    failed = [n for n, s, _ in results if s == "FAIL"]
    skipped = [n for n, s, _ in results if s == "SKIP"]
    print("=" * 56)
    if failed:
        print(f"FAILED: {', '.join(failed)}")
        return 1
    if skipped:
        print(f"all run tests passed; skipped (no dumps): {', '.join(skipped)}")
        return 0
    print("all QEMU tests passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
