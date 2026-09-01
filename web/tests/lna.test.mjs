// Unit tests for the Local Network Access classifier (src/lib/lna.ts) — the
// rules that decide whether a fetch carries `targetAddressSpace: "local"` and
// whether a failure should be reported as "the browser refused" (Gitea #162).
//
// Run: `npm test` from web/ (node's built-in runner + type stripping, so the
// .ts module is imported directly — no build step, no test dependency).
//
// Why this is worth pinning: a hint that DISAGREES with the target's real
// address space hard-fails the request in Chromium (measured against the live
// site, docs/wled-migration.md). Over-hinting is not a harmless default — it
// breaks fetches that would otherwise have worked, and the real grant flow
// can't be exercised headlessly to catch it.
import test from "node:test";
import assert from "node:assert/strict";
import { browserBlocked, classifyHost, lnaHint } from "../src/lib/lna.ts";

/** Stand-in for window.location. */
const page = (href) => ({ protocol: new URL(href).protocol, href });

const PAGES = "https://googlebot42.github.io/luxel/"; // the hosted copy
const DEV = "http://localhost:4179/"; // vite dev/preview
const LAN_HTTP = "http://192.168.0.183/"; // UI served from the device itself

test("classifyHost: loopback", () => {
  for (const h of ["localhost", "LOCALHOST", "app.localhost", "127.0.0.1", "127.1.2.3", "[::1]"])
    assert.equal(classifyHost(h), "loopback", h);
});

test("classifyHost: local (RFC1918, link-local, mDNS, IPv6 ULA)", () => {
  for (const h of [
    "10.0.0.1",
    "192.168.0.183",
    "172.16.0.1",
    "172.31.255.254",
    "169.254.7.7",
    "luxel-4ae0d4.local",
    "[fd00::1]",
    "[fe80::1]",
  ])
    assert.equal(classifyHost(h), "local", h);
});

test("classifyHost: public — including near-misses on the private ranges", () => {
  for (const h of [
    "example.com",
    "googlebot42.github.io",
    "8.8.8.8",
    "172.15.0.1", // just below the 172.16/12 block
    "172.32.0.1", // just above it
    "192.169.0.1",
    "11.0.0.1",
    "999.1.1.1", // not a valid IPv4 literal — a name, and an unknown one
    "[2001:db8::1]",
  ])
    assert.equal(classifyHost(h), "public", h);
});

test("hint is sent only from https, and only to local-space targets", () => {
  assert.deepEqual(lnaHint("http://192.168.0.183/api/status", page(PAGES)), {
    targetAddressSpace: "local",
  });
  assert.deepEqual(lnaHint("http://luxel.local/api/status", page(PAGES)), {
    targetAddressSpace: "local",
  });
});

test("no hint for loopback or public targets — a mismatch hard-fails", () => {
  assert.deepEqual(lnaHint("http://127.0.0.1:8080/api/status", page(PAGES)), {});
  assert.deepEqual(lnaHint("http://localhost:8080/api/status", page(PAGES)), {});
  assert.deepEqual(lnaHint("https://example.com/api/status", page(PAGES)), {});
});

test("no hint from an http page — the dev server, and the device-served UI", () => {
  // Sending it here is the case that HARD-FAILS working setups: plain-http
  // origins have no mixed-content problem and no LNA gate to open.
  assert.deepEqual(lnaHint("http://192.168.0.183/api/status", page(DEV)), {});
  assert.deepEqual(lnaHint("/api/status", page(DEV)), {});
  assert.deepEqual(lnaHint("/api/status", page(LAN_HTTP)), {});
  assert.deepEqual(lnaHint("http://192.168.0.183/api/status", page(LAN_HTTP)), {});
});

test("relative URLs resolve against the page", () => {
  // The playground's own assets (luxel.wasm, gallery.json) go through the same
  // gate: same-origin https means a public-space target and no hint.
  assert.deepEqual(lnaHint("./luxel.wasm", page(PAGES)), {});
  // …but an https page hosted ON the LAN is a local-space target: matching.
  assert.deepEqual(lnaHint("/api/status", page("https://192.168.0.9/")), {
    targetAddressSpace: "local",
  });
});

test("unparseable targets get no hint rather than a guess", () => {
  assert.deepEqual(lnaHint("http://:::/", page(PAGES)), {});
});

test("browser-blocked: https page + http target, whatever the space", () => {
  assert.equal(browserBlocked("http://192.168.0.183", page(PAGES)), true);
  assert.equal(browserBlocked("http://luxel.local", page(PAGES)), true);
  assert.equal(browserBlocked("http://127.0.0.1:8080", page(PAGES)), true);
});

test("browser-blocked: nothing an http page or an https target does is blocked", () => {
  assert.equal(browserBlocked("http://192.168.0.183", page(DEV)), false);
  assert.equal(browserBlocked("http://192.168.0.183", page(LAN_HTTP)), false);
  assert.equal(browserBlocked("https://192.168.0.183", page(PAGES)), false);
  // "" is the served-from-device base: same origin, never blocked.
  assert.equal(browserBlocked("", page(PAGES)), false);
  assert.equal(browserBlocked("", page(LAN_HTTP)), false);
});

test("outside a browser nothing is hinted or blocked", () => {
  // No window.location in node: the defaults must not throw or invent a page.
  assert.deepEqual(lnaHint("http://192.168.0.183/api/status"), {});
  assert.equal(browserBlocked("http://192.168.0.183"), false);
});
