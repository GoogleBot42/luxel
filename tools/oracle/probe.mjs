// Quick probe: connect to a Pixelblaze websocket, request config, print
// what comes back. Usage: node probe.mjs <ip>
const ip = process.argv[2] ?? "192.168.0.140";
const ws = new WebSocket(`ws://${ip}:81`);
ws.binaryType = "arraybuffer";

const timer = setTimeout(() => {
  console.error("timeout");
  process.exit(1);
}, 8000);

ws.onopen = () => {
  console.error(`connected to ${ip}:81`);
  ws.send(JSON.stringify({ getConfig: true }));
};
let seen = 0;
ws.onmessage = (ev) => {
  if (typeof ev.data === "string") {
    const obj = JSON.parse(ev.data);
    console.log(JSON.stringify(obj));
    if (obj.ver !== undefined || obj.name !== undefined) seen |= 1;
    if (obj.activeProgram !== undefined) seen |= 2;
  } else {
    const bytes = new Uint8Array(ev.data);
    console.log(`binary frame: type=${bytes[0]} len=${bytes.length}`);
    if (bytes[0] === 9) seen |= 4;
  }
  if (seen >= 3) {
    clearTimeout(timer);
    ws.onclose = () => process.exit(0);
    ws.close();
    setTimeout(() => process.exit(0), 2000);
  }
};
ws.onerror = (e) => {
  console.error("ws error", e.message ?? e);
  process.exit(1);
};
