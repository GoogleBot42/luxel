#!/usr/bin/env python3
"""Run every QEMU-dependent test with one command.

The QEMU emulation harness (docs/research/qemu-emulation-spike.md) backs a
growing set of hardware-free tests. Each is a standalone script; this runner
is the single entry point that builds their shared inputs once and executes
them all, so "the emulator tests" is one command, not a checklist.

What it does:
  1. Builds the athom firmware (`.#luxel-fw-athom-music`) to ./result,
     Espressif's patched QEMU (`.#qemu-espressif`) to ./result-qemu, and —
     for the 16 MB layout check — the Seengreat firmware
     (`.#luxel-fw-seengreat-hub75`) to ./result-s3. All nix-cached, seconds
     when warm. Separate out-links on purpose: building one flake output
     reuses the default ./result symlink and would clobber the other
     (worktree gotcha, .claude/skills/worktree-setup).
  2. Locates the two gitignored Athom dumps the takeover/heap tests need
     (athom-wled-stock.bin, athom-wled-fs-configured.bin) — via --stock/--fs,
     the LUXEL_ATHOM_STOCK / LUXEL_ATHOM_FS env vars, or autodetection in the
     repo root and the sibling main checkout.
  3. Runs each test as a subprocess and prints a pass/fail summary.

Tests that need the dumps are skipped (not failed) when the dumps aren't
found, so the runner still works in a checkout without them — it just reports
what it couldn't run. The migration and flashmap tests need no dumps: they
compose their fixtures from the stock merged image plus `tools/storegen`.

The suite's three families:

  takeover-*   WLED -> Luxel self-install (firmware/src/takeover.rs)
  migrate-*    the self-applied partition migration (firmware/src/migrate.rs,
               Gitea #501/#634) — on BOTH layouts: the 4 MB table on the
               esp32 machine and the 16 MB one on esp32s3, each from either
               OTA slot and cut at every re-runnable stage; the refusal path
               (a library too large); a fast assertion-only model of the
               16 MB table; and the two-hop board — 16 MB silicon behind a
               4 MB bootloader falls back to the 4 MB table now and migrates
               again to the 16 MB one once the bootloader is re-flashed
  flashmap /   cache-MMU mapping and the heap-region self-heal
  heap-regions

Usage:
    nix develop -c python3 tools/qemu/run-all.py
    nix develop -c python3 tools/qemu/run-all.py --stock <dump> --fs <fs>
    nix develop -c python3 tools/qemu/run-all.py -k heap   # filter by name
    nix develop -c python3 tools/qemu/run-all.py -k migrate
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
        build("luxel-fw-seengreat-hub75", os.path.join(REPO, "result-s3"))
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

    s3 = os.path.join(REPO, "result-s3")
    common = ["--qemu", qemu, "--result-dir", result_dir]
    dump_args = ["--stock", stock or "", "--fs", fs or ""]

    # (name, script, args, needs_dumps)
    suite = [
        ("takeover-app1", "takeover-test.py", dump_args + ["--slot", "app1"], True),
        ("takeover-app0", "takeover-test.py", dump_args + ["--slot", "app0"], True),
        ("takeover-fault", "takeover-test.py", dump_args + ["--slot", "app1", "--inject-fault"], True),
        # The fixture's own WLED pin is GPIO18 = the Athom's board default, so
        # takeover-app0/app1 only cover the "store no override" arm.  These two
        # rewrite the fixture's pin (see takeover-test.py --wled-pin) to cover
        # the import and the this-board-reserves-it arms (Gitea #154/#273).
        ("takeover-pin-import", "takeover-test.py",
         dump_args + ["--slot", "app0", "--wled-pin", "19"], True),
        ("takeover-pin-reserved", "takeover-test.py",
         dump_args + ["--slot", "app0", "--wled-pin", "10"], True),
        ("heap-regions-selfheal", "heap-regions-test.py", dump_args + ["--mode", "selfheal"], True),
        ("heap-regions-rollback", "heap-regions-test.py", dump_args + ["--mode", "rollback"], True),
        # cache-MMU flash mapping (firmware/src/flashmap.rs) — stock merged
        # image + a synthetic LUX2 archive, no dumps needed
        ("flashmap", "flashmap-test.py", [], False),
        # The #501 partition migration.  No dumps: the pre-#501 flash is
        # composed here (hand-built old table + a tools/storegen store).
        ("migrate-from-ota0", "migrate-test.py", ["--from", "ota_0"], False),
        # The variant that caught the overlap-guard bug: a device whose last
        # OTA landed in ota_1 has to copy itself down before it can stage.
        ("migrate-from-ota1", "migrate-test.py", ["--from", "ota_1"], False),
        # Power cuts, one per re-runnable stage.  `table` is deliberately
        # absent — a cut inside that one sector write is the known
        # unrecoverable window (see migrate-test.py's docstring).
        ("migrate-cut-copy", "migrate-test.py",
         ["--from", "ota_1", "--cut", "copy"], False),
        ("migrate-cut-staged", "migrate-test.py", ["--cut", "staged"], False),
        ("migrate-cut-stored", "migrate-test.py", ["--cut", "stored"], False),
        ("migrate-cut-assets", "migrate-test.py", ["--cut", "assets"], False),
        # A library that cannot fit the 4 MB layout's log: refuse, change
        # nothing, keep working.
        ("migrate-overfill", "migrate-test.py", ["--overfill"], False),
        # The 16 MB layout, assertion-only (table encoding + the host store
        # move).  Fast, and it runs ahead of the emulated S3 cases so that
        # when both fail you can tell the layout from the migrator.
        ("migrate-plan-16mb", "migrate-test.py",
         ["--plan-16mb", "--result-dir-16mb", s3], False),
        # The 16 MB layout FOR REAL, on QEMU's esp32s3 machine (Gitea #634).
        # The only layout whose `assets` partition moves, so the only one
        # that executes migrate::move_assets' copy branch.
        ("migrate-s3-from-ota0", "migrate-test.py",
         ["--board", "s3", "--result-dir-16mb", s3], False),
        ("migrate-s3-from-ota1", "migrate-test.py",
         ["--board", "s3", "--from", "ota_1", "--result-dir-16mb", s3], False),
        ("migrate-s3-cut-copy", "migrate-test.py",
         ["--board", "s3", "--from", "ota_1", "--cut", "copy",
          "--result-dir-16mb", s3], False),
        ("migrate-s3-cut-staged", "migrate-test.py",
         ["--board", "s3", "--cut", "staged", "--result-dir-16mb", s3], False),
        ("migrate-s3-cut-stored", "migrate-test.py",
         ["--board", "s3", "--cut", "stored", "--result-dir-16mb", s3], False),
        # A cut between "the store is recorded" and the table write — on this
        # layout that window contains the 960 KiB asset move, so unlike the
        # 4 MB board it is a wide, re-runnable stage rather than a race.
        ("migrate-s3-cut-assets", "migrate-test.py",
         ["--board", "s3", "--cut", "assets", "--result-dir-16mb", s3], False),
        # The Seengreat as found on 2026-09-21: 16 MB of silicon behind a
        # bootloader serially flashed for 4 MB, which an OTA cannot replace.
        # It migrates to the LARGEST layout that bootloader can back — the
        # 4 MB table, the Athom's path, byte for byte — instead of refusing,
        # and reports that a serial re-flash would unlock the big one
        # (Gitea #634). From both slots, because the self-copy is the same
        # code here as anywhere.
        ("migrate-s3-fallback-ota0", "migrate-test.py",
         ["--board", "s3", "--old-bootloader", "4mb", "--result-dir-16mb", s3], False),
        ("migrate-s3-fallback-ota1", "migrate-test.py",
         ["--board", "s3", "--old-bootloader", "4mb", "--from", "ota_1",
          "--result-dir-16mb", s3], False),
        # …and cut at every re-runnable stage of it. The fallback is an
        # ordinary migration in every respect but which table it targets, so
        # anything the 4 MB board is covered for, this board is too.
        ("migrate-s3-fallback-cut-copy", "migrate-test.py",
         ["--board", "s3", "--old-bootloader", "4mb", "--from", "ota_1",
          "--cut", "copy", "--result-dir-16mb", s3], False),
        ("migrate-s3-fallback-cut-staged", "migrate-test.py",
         ["--board", "s3", "--old-bootloader", "4mb", "--cut", "staged",
          "--result-dir-16mb", s3], False),
        ("migrate-s3-fallback-cut-stored", "migrate-test.py",
         ["--board", "s3", "--old-bootloader", "4mb", "--cut", "stored",
          "--result-dir-16mb", s3], False),
        ("migrate-s3-fallback-cut-assets", "migrate-test.py",
         ["--board", "s3", "--old-bootloader", "4mb", "--cut", "assets",
          "--result-dir-16mb", s3], False),
        # Then Jeremy re-flashes the bootloader over serial and the SAME
        # device migrates a second time, 4 MB layout -> 16 MB: the store
        # moves 0x290000 -> 0x610000 and the bundle 0x310000 -> 0xa10000.
        # This is what stops `migrated: true` from meaning "stop looking".
        ("migrate-s3-fallback-then-16mb", "migrate-test.py",
         ["--board", "s3", "--old-bootloader", "4mb",
          "--reflash-bootloader", "16mb", "--result-dir-16mb", s3], False),
        ("migrate-s3-fallback-then-16mb-ota1", "migrate-test.py",
         ["--board", "s3", "--old-bootloader", "4mb", "--from", "ota_1",
          "--reflash-bootloader", "16mb", "--result-dir-16mb", s3], False),
        # The on-device JIT (Gitea #658): the boot pattern rendered natively
        # and interpreted, compared bit for bit out of guest RAM. Builds its
        # OWN image (`.#luxel-fw-esp32-generic-jit`) — the shipped classic
        # images are interpreter-only — and drives the runtime switch
        # through the gdbstub, because QEMU has no network. ~50 s.
        # `--patterns @five` covers the §7.1 set at a firmware build each;
        # that is minutes, so it is not in the default suite.
        ("jit", "jit-test.py", [], False),
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
