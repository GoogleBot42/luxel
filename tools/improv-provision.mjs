// Provision a stock WLED's WiFi over Improv-serial — zero button holds.
// A restored/factory WLED boots into its captive-portal AP, unreachable
// from the container; WLED 0.13+ listens for Improv RPC on the UART, so
// one packet puts it on the LAN with cfg.json AND wsec.json persisted
// (survives cold power cycles — proven on the Athom 2026-08-16, reused
// 2026-08-30 for the Gitea #53 takeover validation).
//
// Usage:  stty -F /dev/ttyUSB0 115200 raw -echo   # ALWAYS first, or the
//         node tools/improv-provision.mjs /dev/ttyUSB0 <ssid> <pass>
//         # port reads produce zero bytes (athom-rig skill, serial gotchas)
//
// Mind the single-reader rule: kill any serial capture before running —
// this script reads the port for the RPC result (WLED replies with its
// URL once joined).
//
// Packet: "IMPROV" + version 0x01 + type 0x03 (RPC) + len +
// [cmd 0x01 (send wifi settings), data_len, ssid_len, ssid, pass_len,
// pass] + sum-of-all-bytes checksum + '\n'.
import fs from "node:fs";

const [port, ssid, pass] = process.argv.slice(2);
if (!pass) {
  console.error("usage: improv-provision.mjs <port> <ssid> <pass>");
  process.exit(2);
}

const s = Buffer.from(ssid, "utf8"), p = Buffer.from(pass, "utf8");
const rpc = Buffer.concat([
  Buffer.from([0x01, s.length + p.length + 2, s.length]), s,
  Buffer.from([p.length]), p,
]);
const head = Buffer.concat([
  Buffer.from("IMPROV", "ascii"), Buffer.from([1, 0x03, rpc.length]), rpc,
]);
let sum = 0;
for (const b of head) sum = (sum + b) & 0xff;
const pkt = Buffer.concat([head, Buffer.from([sum, 0x0a])]);

const fd = fs.openSync(port, fs.constants.O_RDWR | fs.constants.O_NONBLOCK);
fs.writeSync(fd, pkt);
console.log(`sent ${pkt.length} B Improv RPC (ssid "${ssid}") to ${port}`);

// Read replies for a few seconds: an RPC result carrying the device URL
// (http://<ip>) means it joined and persisted the creds.
const buf = Buffer.alloc(4096);
const deadline = Date.now() + 8000;
let got = Buffer.alloc(0);
while (Date.now() < deadline) {
  try {
    const n = fs.readSync(fd, buf, 0, buf.length, null);
    if (n > 0) got = Buffer.concat([got, buf.subarray(0, n)]);
  } catch (e) {
    if (e.code !== "EAGAIN") throw e;
    await new Promise((r) => setTimeout(r, 100));
  }
}
fs.closeSync(fd);
const text = got.toString("latin1").replace(/[^\x20-\x7e\n]/g, ".");
console.log(`response (${got.length} B):\n${text}`);
const url = text.match(/http:\/\/[0-9.]+/);
console.log(url ? `joined: ${url[0]}` : "no URL in response — check AP/creds (device may still be booting)");
