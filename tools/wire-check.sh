#!/usr/bin/env bash
# Wire-level contract check of the device HTTP surface, run against a live
# device after any change to firmware/src/server.rs (see the deploy-device
# skill). Verifies every response family: single Content-Type, exact
# Content-Length (incl. flash-streamed routes), asset 200/304 caching
# headers, all four OPTIONS preflight headers, 404 shape. Plain curl — no
# devshell needed. Written for Gitea #167; the response-shape rules it
# guards live in .claude/rules/firmware.md.
#
# Argument: a device IP/host, with or without a scheme — `192.168.0.183`,
# `http://192.168.0.183` and `http://192.168.0.183/` all work. Passing a URL
# used to build `http://http://…`, which curl refused; with `-s` swallowing
# the error every capture came back empty and every assertion FAILed on a
# perfectly healthy device (Gitea #346). Captures are now checked, and a
# missing one aborts as HARNESS BROKEN instead of masquerading as a firmware
# regression.
set -u
RAW="${1:?usage: wire-check.sh <device-ip-or-url>}"
case "$RAW" in
  http://*|https://*) B="$RAW" ;;
  *)                  B="http://$RAW" ;;
esac
B="${B%/}"
WC=$(mktemp -d) || { echo "HARNESS BROKEN: mktemp -d failed (TMPDIR=${TMPDIR-unset})" >&2; exit 2; }
trap 'rm -rf "$WC"' EXIT
: >"$WC"/wc-probe 2>/dev/null || { echo "HARNESS BROKEN: temp dir $WC is not writable" >&2; exit 2; }
fail=0
say() { printf '\n== %s\n' "$*"; }
chk() { # chk <desc> <cond-result (0 ok)>
  if [ "$2" -eq 0 ]; then echo "PASS: $1"; else echo "FAIL: $1"; fail=1; fi
}

# Anything below is a problem with the harness or the network, NOT with the
# device's response shape — so it aborts loudly rather than emitting FAILs.
broken() { printf '\nHARNESS BROKEN: %s\n' "$*" >&2; printf '  base URL: %s\n' "$B" >&2; exit 2; }

req() { # req <desc> <body-out> <curl args...> -> sets H to the response headers
  local desc="$1" out="$2" rc=0; shift 2
  H=$(curl -sS -D - -o "$out" "$@" 2>"$WC"/wc-err) || rc=$?
  local err; err=$(cat "$WC"/wc-err)
  case "$rc" in
    0) ;;
    6|7|28|35|56) printf '\nDEVICE UNREACHABLE: %s (curl exit %s)\n' "$desc" "$rc" >&2
                  [ -n "$err" ] && printf '  %s\n' "$err" >&2
                  printf '  base URL: %s — is the device up, and is the argument a bare IP?\n' "$B" >&2
                  exit 3 ;;
    *) broken "$desc: curl exit $rc${err:+ — $err}" ;;
  esac
  [ -n "${H//[$'\r\n\t ']/}" ] || broken "$desc: no response headers captured"
}

bodysize() { # bodysize <desc> <file> -> byte count; a missing capture is fatal
  # Call sites MUST end in `|| exit 2`: broken() runs inside the $(...) here,
  # so its exit only kills the subshell.
  [ -f "$2" ] || broken "$1: no response body captured (expected $2)"
  stat -c %s "$2"
}

say "status"
req "GET /api/status (smoke)" "$WC"/wc-smoke -m5 "$B/api/status"
cat "$WC"/wc-smoke; echo

say "GET /api/status headers (single JSON Content-Type + CORS + exact Content-Length)"
req "GET /api/status" "$WC"/wc-body -m5 "$B/api/status"
echo "$H"
ct_count=$(echo "$H" | grep -ci '^content-type:')
chk "exactly one Content-Type" $([ "$ct_count" -eq 1 ]; echo $?)
echo "$H" | grep -qi '^content-type: application/json'; chk "CT is application/json" $?
echo "$H" | grep -qi '^access-control-allow-origin: \*'; chk "CORS present" $?
cl=$(echo "$H" | grep -i '^content-length:' | tr -dc 0-9)
bl=$(bodysize "GET /api/status" "$WC"/wc-body) || exit 2
chk "Content-Length ($cl) == body bytes ($bl)" $([ "$cl" = "$bl" ]; echo $?)

say "OPTIONS preflight (204 + all four CORS headers)"
req "OPTIONS /api/patterns/foo" /dev/null -m5 -X OPTIONS "$B/api/patterns/foo"
echo "$H"
echo "$H" | head -1 | grep -q 204; chk "204" $?
for h in 'access-control-allow-origin' 'access-control-allow-methods' 'access-control-allow-headers' 'access-control-max-age'; do
  echo "$H" | grep -qi "^$h:"; chk "preflight header $h" $?
done

say "asset 200 (/, expect ETag + Cache-Control [+ gzip])"
req "GET /" "$WC"/wc-asset -m10 "$B/"
echo "$H" | head -12
echo "$H" | grep -qi '^etag:'; chk "200 has ETag" $?
echo "$H" | grep -qi '^cache-control:'; chk "200 has Cache-Control" $?
ct_count=$(echo "$H" | grep -ci '^content-type:')
chk "asset 200: exactly one Content-Type" $([ "$ct_count" -eq 1 ]; echo $?)
cl200=$(echo "$H" | grep -i '^content-length:' | tr -dc 0-9)
al=$(bodysize "GET /" "$WC"/wc-asset) || exit 2
chk "asset 200 Content-Length ($cl200) == received bytes ($al)" $([ "$cl200" = "$al" ]; echo $?)
ETAG=$(echo "$H" | grep -i '^etag:' | sed 's/^[Ee][Tt][Aa][Gg]: //' | tr -d '\r')
# A missing ETag is a DEVICE failure — the "200 has ETag" check above already
# reported it — not a harness one, so skip the block it would make meaningless
# rather than aborting or asserting against an empty header.
if [ -z "$ETAG" ]; then
  say "asset 304 revalidation — SKIPPED: the 200 carried no ETag (see the FAIL above)"
else

say "asset 304 revalidation (If-None-Match: $ETAG)"
req "GET / (If-None-Match)" "$WC"/wc-304 -m10 -H "If-None-Match: $ETAG" "$B/"
echo "$H"
echo "$H" | head -1 | grep -q 304; chk "304 status" $?
echo "$H" | grep -qi '^etag:'; chk "304 has ETag" $?
echo "$H" | grep -qi '^cache-control:'; chk "304 has Cache-Control" $?
cl304=$(echo "$H" | grep -i '^content-length:' | tr -dc 0-9)
chk "304 Content-Length ($cl304) == 200's ($cl200)" $([ "$cl304" = "$cl200" ]; echo $?)
# curl never creates the -o file for a body-less 304, so a missing file IS
# the empty-body pass condition (bitten on the first #167 device run) — this
# is the one capture that is legitimately allowed to be absent.
sz=$(stat -c %s "$WC"/wc-304 2>/dev/null || echo 0)
chk "304 body is empty ($sz B)" $([ "$sz" = "0" ]; echo $?)

fi

say "/api/pixels (octet-stream, 3B/px)"
req "GET /api/pixels" "$WC"/wc-px -m5 "$B/api/pixels"
echo "$H"
echo "$H" | grep -qi '^content-type: application/octet-stream'; chk "octet-stream CT" $?
ct_count=$(echo "$H" | grep -ci '^content-type:')
chk "pixels: exactly one Content-Type" $([ "$ct_count" -eq 1 ]; echo $?)
cl=$(echo "$H" | grep -i '^content-length:' | tr -dc 0-9)
bl=$(bodysize "GET /api/pixels" "$WC"/wc-px) || exit 2
chk "pixels Content-Length ($cl) == body ($bl)" $([ "$cl" = "$bl" ]; echo $?)

say "/api/pattern + /api/pattern.lxp streaming (length == wire bytes)"
for p in /api/pattern /api/pattern.lxp; do
  req "GET $p" "$WC"/wc-stream -m15 "$B$p"
  cl=$(echo "$H" | grep -i '^content-length:' | tr -dc 0-9)
  bl=$(bodysize "GET $p" "$WC"/wc-stream) || exit 2
  chk "$p Content-Length ($cl) == body ($bl)" $([ "$cl" = "$bl" ]; echo $?)
done

say "404 (text/plain)"
req "GET /nope" "$WC"/wc-404 -m5 "$B/nope-$RANDOM"
echo "$H" | head -4
echo "$H" | head -1 | grep -q 404; chk "404 status" $?
echo "$H" | grep -qi '^content-type: text/plain'; chk "404 text/plain" $?
bodysize "GET /nope" "$WC"/wc-404 >/dev/null || exit 2
grep -q "not found" "$WC"/wc-404; chk "404 body" $?

say "/min (single text/html CT)"
req "GET /min" /dev/null -m5 "$B/min"
ct_count=$(echo "$H" | grep -ci '^content-type:')
echo "$H" | grep -qi '^content-type: text/html'; chk "/min text/html" $?
chk "/min exactly one Content-Type" $([ "$ct_count" -eq 1 ]; echo $?)

say "DELETE reaches the JSON path (informational)"
code=$(curl -sSm10 -o "$WC"/wc-del -w '%{http_code}' -X DELETE "$B/api/patterns/wire-check-nonexistent") \
  || broken "DELETE /api/patterns/... : curl failed"
echo "DELETE nonexistent -> $code ($(cat "$WC"/wc-del))"

say "RESULT"
[ "$fail" -eq 0 ] && echo "ALL PASS" || echo "FAILURES PRESENT"
exit $fail
