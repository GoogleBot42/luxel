// WiFi RX-pool stress: saturate a real device's inbound path with DDP
// frames while hammering its HTTP API, and watch for the failure mode that
// an undersized esp-radio RX pool actually produces — a StoreProhibited
// panic / reboot, not a clean error (the blob's allocations do not
// null-check). This is the acceptance harness for any change to
// `static_rx_buf_num` / `dynamic_rx_buf_num` / `ampdu_rx_enable` in
// firmware/src/main.rs (Gitea #60, and the 2026-08-22 small-chip tuning
// before it).
//
// Usage (repo root, nix develop):
//   node tools/rx-stress.mjs <device-ip> [seconds] [pixels]
// Defaults: 180 s, 300 px (~245 pkt/s x 900 B payload = ~220 KB/s inbound
// UDP, which is roughly a real xLights/LedFx feed).
//
// It sets the device to `pixels` for the run and restores the pixel count
// it found. Run a serial capture alongside it — the HTTP-side view of a
// reboot is nearly invisible (the device is back in ~5 s), so the
// watchdog's slot/uptime checks and the serial log are the real evidence.
//
// Reports: DDP frames sent, HTTP requests served vs refused/reset, min
// heap_free, any vmerr, and whether `slot` ever changed (a boot-loop
// rollback flips slots silently — see .claude/skills/deploy-device).

import dgram from "node:dgram";

const IP = process.argv[2] ?? "192.168.0.183";
const SECONDS = Number(process.argv[3] ?? 180);
const PIXELS = Number(process.argv[4] ?? 300);
const DEV = `http://${IP}`;
const PPS = 245; // DDP packets per second
const HTTP_WORKERS = 6;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function api(path, body) {
  const opts = {
    headers: { connection: "close" },
    signal: AbortSignal.timeout(15_000),
  };
  const r = await fetch(
    DEV + path,
    body === undefined ? opts : { ...opts, method: "POST", body },
  );
  return await r.json();
}

function ddpFrame(pixels, phase) {
  // v1 | push, type RGB, dest 1 (default output), offset 0.
  const buf = Buffer.alloc(10 + pixels * 3);
  buf[0] = 0x41;
  buf[2] = 1;
  buf[3] = 1;
  buf.writeUInt16BE(pixels * 3, 8);
  for (let i = 0; i < pixels; i++) {
    buf[10 + i * 3] = (i + phase) & 0xff;
    buf[11 + i * 3] = (i * 3 + phase) & 0xff;
    buf[12 + i * 3] = (phase * 5) & 0xff;
  }
  return buf;
}

const main = async () => {
  const before = await api("/api/status");
  const cfg = await api("/api/config");
  console.log(`device: ${JSON.stringify(before)}`);
  console.log(`config: ${JSON.stringify(cfg)}`);
  const restorePixels = cfg.pixels;
  if (cfg.pixels !== PIXELS) {
    // POST /api/config takes a bare integer, not JSON.
    const r = await api("/api/config", String(PIXELS));
    if (!r.ok) throw new Error(`could not set pixels: ${JSON.stringify(r)}`);
    await sleep(1500);
  }

  const stop = Date.now() + SECONDS * 1000;
  const stats = {
    ddpSent: 0,
    ddpErr: 0,
    httpOk: 0,
    httpFail: 0,
    minHeap: Infinity,
    liveDdp: 0,
    vmerr: null,
    slots: new Set([before.slot]),
    statusMisses: 0,
    samples: 0,
  };

  // --- DDP flood -----------------------------------------------------
  const udp = dgram.createSocket("udp4");
  const frames = Array.from({ length: 16 }, (_, i) => ddpFrame(PIXELS, i * 17));
  const ddpTask = (async () => {
    let phase = 0;
    const period = 1000 / PPS;
    let next = Date.now();
    while (Date.now() < stop) {
      const burst = 8; // send in small bursts, then re-sync to the clock
      for (let i = 0; i < burst; i++) {
        udp.send(frames[phase++ & 15], 4048, IP, (e) => {
          if (e) stats.ddpErr++;
          else stats.ddpSent++;
        });
      }
      next += period * burst;
      const wait = next - Date.now();
      if (wait > 0) await sleep(wait);
      else next = Date.now();
    }
    await new Promise((r) => udp.close(r));
  })();

  // --- concurrent HTTP hammer ---------------------------------------
  const paths = [
    "/api/status",
    "/api/config",
    "/api/brightness",
    "/api/controls",
    "/api/playlist",
    "/api/output",
  ];
  const httpTasks = Array.from({ length: HTTP_WORKERS }, (_, w) =>
    (async () => {
      while (Date.now() < stop) {
        try {
          await api(paths[(w + stats.httpOk) % paths.length]);
          stats.httpOk++;
        } catch {
          stats.httpFail++;
          await sleep(50);
        }
      }
    })(),
  );

  // --- watchdog: heap / vmerr / slot / reachability ------------------
  const watchdog = (async () => {
    while (Date.now() < stop) {
      try {
        const s = await api("/api/status");
        stats.samples++;
        stats.slots.add(s.slot);
        // `live: "ddp"` proves the frames are being RECEIVED, not just sent —
        // without it a silently-dropping RX pool looks like a clean pass.
        if (s.live === "ddp") stats.liveDdp++;
        if (s.heap_free < stats.minHeap) stats.minHeap = s.heap_free;
        if (s.vmerr) stats.vmerr = s.vmerr;
      } catch {
        stats.statusMisses++;
      }
      await sleep(1000);
    }
  })();

  const t0 = Date.now();
  await Promise.all([ddpTask, ...httpTasks, watchdog]);
  const secs = (Date.now() - t0) / 1000;

  // let the live-input timeout lapse, then restore
  await sleep(3000);
  const after = await api("/api/status");
  if (restorePixels !== PIXELS) {
    await api("/api/config", String(restorePixels));
  }

  const slots = [...stats.slots];
  const rebooted = slots.length > 1;
  console.log(
    `\nDDP:   ${stats.ddpSent} frames sent (${(stats.ddpSent / secs).toFixed(0)} pkt/s, ` +
      `~${((stats.ddpSent * (PIXELS * 3 + 10)) / secs / 1024).toFixed(0)} KB/s), ${stats.ddpErr} send errors`,
  );
  console.log(
    `HTTP:  ${stats.httpOk} served, ${stats.httpFail} refused/reset ` +
      `(${HTTP_WORKERS} workers; the device's web pool is 2-3 slots, so refusals are expected)`,
  );
  console.log(
    `watch: ${stats.samples} status samples, ${stats.statusMisses} missed, ` +
      `${stats.liveDdp} showed live="ddp" (frames actually received), ` +
      `min heap_free ${stats.minHeap}, vmerr ${stats.vmerr ?? "null"}`,
  );
  console.log(`slot:  ${slots.join(" -> ")}${rebooted ? "  ** CHANGED **" : " (held)"}`);
  console.log(`after: ${JSON.stringify(after)}`);
  // liveDdp === 0 means the device never showed a received frame — a
  // silently-dropping RX path would otherwise read as a clean pass.
  const bad = rebooted || stats.vmerr || stats.ddpSent === 0 || stats.liveDdp === 0;
  console.log(bad ? "\nFAIL" : "\nOK — no reboot, no rollback, no vmerr");
  process.exit(bad ? 1 : 0);
};

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
