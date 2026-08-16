// A fake WLED device for exercising the installer page (flash.html)
// without touching real hardware. Speaks just enough of both sides:
//
//   phase "wled":  GET /json/info  → WLED-shaped info (arch configurable)
//                  POST /update    → accepts a multipart OTA upload, then
//                                    "reboots": goes dark for REBOOT_MS,
//                                    then switches to phase "luxel"
//   phase "luxel": GET /api/status → Luxel-shaped status (version + slot)
//                  POST /api/assets→ accepts the LUXA bundle
//
// CORS behavior is configurable to mimic WLED 0.13 (no headers) vs 0.14+:
//   FAKE_WLED_CORS=1 adds Access-Control-Allow-Origin: * to WLED routes.
// Luxel routes always send CORS headers, like the real firmware.
//
// Env: FAKE_WLED_PORT (default 4189), FAKE_WLED_ARCH (default esp32),
//      FAKE_WLED_CORS (default 0), FAKE_WLED_REBOOT_MS (default 4000).
// Prints state-change lines to stdout for the harness to assert on.

import http from "node:http";

const PORT = Number(process.env.FAKE_WLED_PORT ?? 4189);
const ARCH = process.env.FAKE_WLED_ARCH ?? "esp32";
const WLED_CORS = process.env.FAKE_WLED_CORS === "1";
const REBOOT_MS = Number(process.env.FAKE_WLED_REBOOT_MS ?? 4000);

let phase = "wled"; // wled → rebooting → luxel
let updateBytes = 0;
let assetsBytes = 0;

const drain = (req) =>
  new Promise((resolve) => {
    let n = 0;
    req.on("data", (c) => (n += c.length));
    req.on("end", () => resolve(n));
  });

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, `http://localhost:${PORT}`);
  const luxelHeaders = {
    "Access-Control-Allow-Origin": "*",
    "Access-Control-Allow-Methods": "GET, POST, DELETE, OPTIONS",
    "Access-Control-Allow-Headers": "Content-Type",
  };

  if (phase === "rebooting") {
    req.destroy(); // nobody home during the reboot window
    return;
  }

  if (phase === "wled") {
    const h = WLED_CORS ? { "Access-Control-Allow-Origin": "*" } : {};
    if (req.method === "GET" && url.pathname === "/json/info") {
      res.writeHead(200, { "Content-Type": "application/json", ...h });
      res.end(JSON.stringify({ ver: "0.13.2", arch: ARCH, name: "Fake WLED", brand: "WLED" }));
      return;
    }
    if (req.method === "POST" && url.pathname === "/update") {
      updateBytes = await drain(req);
      console.log(`fake-wled: /update received ${updateBytes} bytes; rebooting`);
      res.writeHead(200, { "Content-Type": "text/html", ...h });
      res.end("<html>Update loaded. Rebooting...</html>");
      phase = "rebooting";
      setTimeout(() => {
        phase = "luxel";
        console.log("fake-wled: now luxel");
      }, REBOOT_MS);
      return;
    }
    res.writeHead(404, h);
    res.end();
    return;
  }

  // phase === "luxel"
  if (req.method === "OPTIONS") {
    res.writeHead(204, luxelHeaders);
    res.end();
    return;
  }
  if (req.method === "GET" && url.pathname === "/api/status") {
    res.writeHead(200, { "Content-Type": "application/json", ...luxelHeaders });
    res.end(JSON.stringify({ version: "9.9.9", slot: "ota_0", fps: 60, heap_free: 90000 }));
    return;
  }
  if (req.method === "POST" && url.pathname === "/api/assets") {
    assetsBytes = await drain(req);
    console.log(`fake-wled: /api/assets received ${assetsBytes} bytes`);
    res.writeHead(200, { "Content-Type": "application/json", ...luxelHeaders });
    res.end(JSON.stringify({ ok: true, bytes: assetsBytes }));
    return;
  }
  res.writeHead(404, luxelHeaders);
  res.end();
});

server.listen(PORT, () => console.log(`fake-wled: listening on ${PORT} (arch=${ARCH}, cors=${WLED_CORS}, reboot=${REBOOT_MS}ms)`));
