#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
# Assemble a static library-review page (the 2026-11-02 one Jeremy got):
# every pattern's 32x32 / 64x64 / strip sheet, every scene at 64 and 128,
# every sprite strip, with a keep/fix/drop + note field per item that
# exports as JSON keyed by library path.
#
#   nix develop --command python3 tools/verify/review-site/build-site.py <render-root>
#
# <render-root> holds what the renderers wrote: sheets/32 (sheet.mjs
# --grid 32x32 --scale 4 --strip 144), sheets/64 (--grid 64x64 --scale 2),
# scenes/64 and scenes/128 (scene-sheet.mjs --size N), sprites/
# (scene-sheet.mjs --sprites). Output: <render-root>/site/, ready for
# `scp -r site/* claude@sites.neet.dev:/luxel-library-review/` (publish-site
# skill). The library it lists is the repo this script sits in.
import json, os, re, shutil, glob, html, sys
HERE = os.path.dirname(os.path.abspath(__file__))
W = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
if len(sys.argv) != 2:
    sys.exit("usage: build-site.py <render-root>")
R = os.path.abspath(sys.argv[1])
S = f"{R}/site"
for d in ["p32", "p64", "strip", "s64", "s128", "spr"]:
    shutil.rmtree(f"{S}/img/{d}", ignore_errors=True)
    os.makedirs(f"{S}/img/{d}")

def jl(p):
    return json.load(open(p)) if os.path.exists(p) else {}

def cp(src, dst):
    if os.path.exists(src):
        shutil.copy(src, f"{S}/img/{dst}")
        return f"img/{dst}"
    return None

patterns = []
for f in sorted(glob.glob(f"{W}/library/*.js")):
    slug = os.path.basename(f)[:-3]
    src = open(f).read()
    m = re.search(r"^// name:\s*(.+)$", src, re.M)
    j32, j64 = jl(f"{R}/sheets/32/{slug}.json"), jl(f"{R}/sheets/64/{slug}.json")
    errs = []
    for tag, j in (("32", j32), ("64", j64)):
        if j.get("compileError"):
            errs.append(f"compile@{tag}: {json.dumps(j['compileError'])}")
        for e in j.get("runtimeErrors", []):
            errs.append(f"runtime@{tag}: {e}")
    lit = j32.get("grid_litFrames", "0/0")
    a, b = map(int, lit.split("/"))
    patterns.append({
        "slug": slug, "name": m.group(1).strip() if m else slug,
        "controls": sum(1 for l in src.splitlines() if "//#" in l),
        "bytes": len(src.encode()),
        "ms32": j32.get("grid_msPerFrame"), "ms64": j64.get("grid_msPerFrame"),
        "lit32": lit, "lit64": j64.get("grid_litFrames"),
        "stripMs": j32.get("strip_msPerFrame"), "stripLit": j32.get("strip_litFrames"),
        "errors": errs,
        "flagged": bool(errs) or a < b or b == 0,
        "img32": cp(f"{R}/sheets/32/{slug}.png", f"p32/{slug}.png"),
        "img64": cp(f"{R}/sheets/64/{slug}.png", f"p64/{slug}.png"),
        "strip": cp(f"{R}/sheets/32/{slug}.strip.png", f"strip/{slug}.png")
                 or cp(f"{R}/sheets/strip-bulk/{slug}.png", f"strip/{slug}.png"),
        "stripBulk": not os.path.exists(f"{R}/sheets/32/{slug}.strip.png") and os.path.exists(f"{R}/sheets/strip-bulk/{slug}.png"),
    })
    if patterns[-1]["stripBulk"]:
        patterns[-1]["stripLit"] = jl(f"{R}/sheets/strip-bulk/{slug}.json").get("strip_litFrames")

scenes = []
for f in sorted(glob.glob(f"{W}/library/scenes/*.scene")):
    slug = os.path.basename(f)[:-6]
    j64, j128 = jl(f"{R}/scenes/64/{slug}.json"), jl(f"{R}/scenes/128/{slug}.json")
    errs = [f"@64: {e}" for e in j64.get("errors", [])] + [f"@128: {e}" for e in j128.get("errors", [])]
    scenes.append({
        "slug": slug, "name": j64.get("name", slug), "layers": j64.get("layers", []),
        "periodMs": j64.get("periodMs", 0), "wireBytes": j64.get("wireBytes"),
        "ms64": j64.get("msPerFrame"), "ms128": j128.get("msPerFrame"),
        "lit64": j64.get("litShots"), "lit128": j128.get("litShots"),
        "errors": errs,
        "flagged": bool(errs) or j64.get("litShots", 0) < 6 or j128.get("litShots", 0) < 6,
        "img64": cp(f"{R}/scenes/64/{slug}.png", f"s64/{slug}.png"),
        "img128": cp(f"{R}/scenes/128/{slug}.png", f"s128/{slug}.png"),
    })

sprites = []
for f in sorted(glob.glob(f"{W}/library/sprites/*.sprite")):
    slug = os.path.basename(f)[:-7]
    j = jl(f"{R}/sprites/{slug}.json")
    sprites.append({
        "slug": slug, "name": j.get("name", slug), "w": j.get("w"), "h": j.get("h"),
        "frames": j.get("frames"), "fps": j.get("fps"), "bytes": j.get("bytes"),
        "errors": [j["error"]] if j.get("error") else [], "flagged": bool(j.get("error")),
        "img": cp(f"{R}/sprites/{slug}.png", f"spr/{slug}.png"),
    })

data = {"patterns": patterns, "scenes": scenes, "sprites": sprites,
        "commit": os.popen(f"git -C {W} rev-parse --short HEAD").read().strip()}
blob = json.dumps(data, separators=(",", ":")).replace("</", "<\\/")
page = open(f"{HERE}/index.template.html").read().replace("@@DATA@@", blob)
open(f"{S}/index.html", "w").write(page)
for a in ("review.js", "review.css"):
    shutil.copy(f"{HERE}/{a}", f"{S}/{a}")
print(len(patterns), len(scenes), len(sprites), "flagged:",
      [p["slug"] for p in patterns if p["flagged"]], [s["slug"] for s in scenes if s["flagged"]])
