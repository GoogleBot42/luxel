// Unit tests for the device-origin probe (src/lib/probe.ts) — the acceptance
// test `detectDeviceBase()` runs at boot.
//
// The bug these exist for (the 2026-09-26 64x64 panel): the probe was ONE
// fetch, and the panel answers `/api/status` with an empty 200 body, a 503 or
// a reset for seconds at a time when it is short of heap. A probe that landed
// in that window threw inside `r.json()` and the console spent the rest of its
// life as a PLAYGROUND — `deviceBase` is written once, at boot. So: a bad body
// is "ask again", and only an ANSWER that says this origin serves something
// else (a 4xx, or HTML) is allowed to be quick.
//
// Run: `npm test` from web/.
import test from "node:test";
import assert from "node:assert/strict";
import { classifyProbe, probeDeviceOrigin } from "../src/lib/probe.ts";

const JSON_HEAD = { "content-type": "application/json" };

/** A `/api/status` body the console accepts as a device. */
function deviceBody() {
  return new Response(JSON.stringify({ pixels: 4096, fps: 120 }), {
    status: 200,
    headers: JSON_HEAD,
  });
}

/** A probe driver with a FAKE clock: the backoff is skipped but still counted
 *  against the budget, so the ladder's shape is what is under test, not the
 *  wall clock. */
function driver(responses) {
  let t = 0;
  const calls = [];
  return {
    calls,
    deps: {
      fetch: async (url) => {
        calls.push(url);
        const next = responses[Math.min(calls.length - 1, responses.length - 1)];
        return typeof next === "function" ? next() : next;
      },
      sleep: async (ms) => {
        t += ms;
      },
      now: () => t,
    },
    elapsed: () => t,
  };
}

test("classifyProbe: a genuine status body is a device", async () => {
  assert.equal(await classifyProbe(deviceBody()), "device");
});

test("classifyProbe: an empty 200 is busy, not a verdict", async () => {
  assert.equal(await classifyProbe(new Response("", { status: 200, headers: JSON_HEAD })), "busy");
});

test("classifyProbe: a truncated body is busy", async () => {
  assert.equal(
    await classifyProbe(new Response('{"pixels":40', { status: 200, headers: JSON_HEAD })),
    "busy",
  );
});

test("classifyProbe: the firmware's low-memory 200 is busy", async () => {
  // `{"ok":false,"error":"out of memory"}` with a 200 — valid JSON, wrong
  // shape. Reading that as "not a device" costs the whole session.
  const r = new Response(JSON.stringify({ ok: false, error: "out of memory" }), {
    status: 200,
    headers: JSON_HEAD,
  });
  assert.equal(await classifyProbe(r), "busy");
});

test("classifyProbe: a 503 is busy and a 404 is not a device", async () => {
  assert.equal(await classifyProbe(new Response("", { status: 503 })), "busy");
  assert.equal(await classifyProbe(new Response("", { status: 404 })), "absent");
});

test("classifyProbe: a dev server's HTML fallback is not a device", async () => {
  const r = new Response("<!doctype html><title>luxel</title>", {
    status: 200,
    headers: { "content-type": "text/html; charset=utf-8" },
  });
  assert.equal(await classifyProbe(r), "absent");
});

test("probeDeviceOrigin: retries a bad body and then accepts the device", async () => {
  const empty = () => new Response("", { status: 200, headers: JSON_HEAD });
  const d = driver([empty, empty, () => deviceBody()]);
  const r = await probeDeviceOrigin(d.deps);
  assert.equal(r.verdict, "device");
  assert.equal(r.attempts, 3);
  assert.equal(d.calls.length, 3);
});

test("probeDeviceOrigin: retries a refused connection", async () => {
  let n = 0;
  const d = driver([
    () => {
      n++;
      if (n < 3) throw new TypeError("Failed to fetch");
      return deviceBody();
    },
  ]);
  assert.equal((await probeDeviceOrigin(d.deps)).verdict, "device");
  assert.equal(d.calls.length, 3);
});

test("probeDeviceOrigin: a 404 origin is the playground, on the FIRST try", async () => {
  const d = driver([() => new Response("", { status: 404 })]);
  const r = await probeDeviceOrigin(d.deps);
  assert.equal(r.verdict, "absent");
  assert.equal(r.attempts, 1);
  assert.equal(d.elapsed(), 0); // no playground waits for this
});

test("probeDeviceOrigin: a board that never answers is busy, never absent", async () => {
  // The caller (`detectDeviceBase`) treats this as "device, unreachable right
  // now" — the console boots bound, with the unreachable banner and the
  // handshake retry, instead of silently becoming a playground.
  const d = driver([() => new Response("", { status: 503 })]);
  const r = await probeDeviceOrigin(d.deps);
  assert.equal(r.verdict, "busy");
  assert.ok(r.attempts >= 5, `gave up after ${r.attempts} attempts`);
  assert.ok(d.elapsed() >= 20000, `only spent ${d.elapsed()} ms of budget`);
});

test("probeDeviceOrigin: the budget is bounded", async () => {
  const d = driver([() => new Response("", { status: 503 })]);
  await probeDeviceOrigin(d.deps);
  assert.ok(d.elapsed() < 30000, `spent ${d.elapsed()} ms`);
});
