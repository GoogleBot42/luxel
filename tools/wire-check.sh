#!/usr/bin/env bash
# Wire-level contract check of the device HTTP surface, run against a live
# device after any change to firmware/src/server.rs (see the deploy-device
# skill). Verifies every response family: single Content-Type, exact
# Content-Length (incl. flash-streamed routes), asset 200/304 caching
# headers, all four OPTIONS preflight headers, 404 shape. Plain curl — no
# devshell needed. Written for Gitea #167; the response-shape rules it
# guards live in .claude/rules/firmware.md.
set -u
IP="${1:?usage: wire-check.sh <device-ip>}"
B="http://$IP"
WC=$(mktemp -d)
trap 'rm -rf "$WC"' EXIT
fail=0
say() { printf '\n== %s\n' "$*"; }
chk() { # chk <desc> <cond-result (0 ok)>
  if [ "$2" -eq 0 ]; then echo "PASS: $1"; else echo "FAIL: $1"; fail=1; fi
}

say "status"
curl -sm5 "$B/api/status"; echo

say "GET /api/status headers (single JSON Content-Type + CORS + exact Content-Length)"
H=$(curl -sm5 -D - -o "$WC"/wc-body "$B/api/status")
echo "$H"
ct_count=$(echo "$H" | grep -ci '^content-type:')
chk "exactly one Content-Type" $([ "$ct_count" -eq 1 ]; echo $?)
echo "$H" | grep -qi '^content-type: application/json'; chk "CT is application/json" $?
echo "$H" | grep -qi '^access-control-allow-origin: \*'; chk "CORS present" $?
cl=$(echo "$H" | grep -i '^content-length:' | tr -dc 0-9)
bl=$(stat -c %s "$WC"/wc-body)
chk "Content-Length ($cl) == body bytes ($bl)" $([ "$cl" = "$bl" ]; echo $?)

say "OPTIONS preflight (204 + all four CORS headers)"
H=$(curl -sm5 -X OPTIONS -D - -o /dev/null "$B/api/patterns/foo")
echo "$H"
echo "$H" | head -1 | grep -q 204; chk "204" $?
for h in 'access-control-allow-origin' 'access-control-allow-methods' 'access-control-allow-headers' 'access-control-max-age'; do
  echo "$H" | grep -qi "^$h:"; chk "preflight header $h" $?
done

say "asset 200 (/, expect ETag + Cache-Control [+ gzip])"
H=$(curl -sm10 -D - -o "$WC"/wc-asset "$B/")
echo "$H" | head -12
echo "$H" | grep -qi '^etag:'; chk "200 has ETag" $?
echo "$H" | grep -qi '^cache-control:'; chk "200 has Cache-Control" $?
ct_count=$(echo "$H" | grep -ci '^content-type:')
chk "asset 200: exactly one Content-Type" $([ "$ct_count" -eq 1 ]; echo $?)
cl200=$(echo "$H" | grep -i '^content-length:' | tr -dc 0-9)
al=$(stat -c %s "$WC"/wc-asset)
chk "asset 200 Content-Length ($cl200) == received bytes ($al)" $([ "$cl200" = "$al" ]; echo $?)
ETAG=$(echo "$H" | grep -i '^etag:' | sed 's/^[Ee][Tt][Aa][Gg]: //' | tr -d '\r')

say "asset 304 revalidation (If-None-Match: $ETAG)"
H=$(curl -sm10 -D - -o "$WC"/wc-304 -H "If-None-Match: $ETAG" "$B/")
echo "$H"
echo "$H" | head -1 | grep -q 304; chk "304 status" $?
echo "$H" | grep -qi '^etag:'; chk "304 has ETag" $?
echo "$H" | grep -qi '^cache-control:'; chk "304 has Cache-Control" $?
cl304=$(echo "$H" | grep -i '^content-length:' | tr -dc 0-9)
chk "304 Content-Length ($cl304) == 200's ($cl200)" $([ "$cl304" = "$cl200" ]; echo $?)
# curl never creates the -o file for a body-less 304, so a missing file IS
# the empty-body pass condition (bitten on the first #167 device run)
sz=$(stat -c %s "$WC"/wc-304 2>/dev/null || echo 0)
chk "304 body is empty ($sz B)" $([ "$sz" = "0" ]; echo $?)

say "/api/pixels (octet-stream, 3B/px)"
H=$(curl -sm5 -D - -o "$WC"/wc-px "$B/api/pixels")
echo "$H"
echo "$H" | grep -qi '^content-type: application/octet-stream'; chk "octet-stream CT" $?
ct_count=$(echo "$H" | grep -ci '^content-type:')
chk "pixels: exactly one Content-Type" $([ "$ct_count" -eq 1 ]; echo $?)
cl=$(echo "$H" | grep -i '^content-length:' | tr -dc 0-9)
bl=$(stat -c %s "$WC"/wc-px)
chk "pixels Content-Length ($cl) == body ($bl)" $([ "$cl" = "$bl" ]; echo $?)

say "/api/pattern + /api/pattern.lxp streaming (length == wire bytes)"
for p in /api/pattern /api/pattern.lxp; do
  H=$(curl -sm15 -D - -o "$WC"/wc-stream "$B$p")
  cl=$(echo "$H" | grep -i '^content-length:' | tr -dc 0-9)
  bl=$(stat -c %s "$WC"/wc-stream)
  chk "$p Content-Length ($cl) == body ($bl)" $([ "$cl" = "$bl" ]; echo $?)
done

say "404 (text/plain)"
H=$(curl -sm5 -D - -o "$WC"/wc-404 "$B/nope-$RANDOM")
echo "$H" | head -4
echo "$H" | head -1 | grep -q 404; chk "404 status" $?
echo "$H" | grep -qi '^content-type: text/plain'; chk "404 text/plain" $?
grep -q "not found" "$WC"/wc-404; chk "404 body" $?

say "/min (single text/html CT)"
H=$(curl -sm5 -D - -o /dev/null "$B/min")
ct_count=$(echo "$H" | grep -ci '^content-type:')
echo "$H" | grep -qi '^content-type: text/html'; chk "/min text/html" $?
chk "/min exactly one Content-Type" $([ "$ct_count" -eq 1 ]; echo $?)

say "DELETE reaches the JSON path (informational)"
code=$(curl -sm10 -o "$WC"/wc-del -w '%{http_code}' -X DELETE "$B/api/patterns/wire-check-nonexistent")
echo "DELETE nonexistent -> $code ($(cat "$WC"/wc-del))"

say "RESULT"
[ "$fail" -eq 0 ] && echo "ALL PASS" || echo "FAILURES PRESENT"
exit $fail
