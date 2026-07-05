// Minimal Pixel Blaze websocket client for the oracle harness.
// Uses only the public protocol: JSON commands + binary type-3 bytecode
// frames (live-code, nothing is saved to the device's flash).

import zlib from "node:zlib";

const FRAME_FIRST = 0x01;
const FRAME_MIDDLE = 0x02;
const FRAME_LAST = 0x04;
const TYPE_PUT_BYTECODE = 3;
const CHUNK = 4096;

const ID_CHARS = "23456789ABCDEFGHJKLMNPQRSTWXYZabcdefghijkmnopqrstuvwxyz";

export function makeId() {
  let id = "";
  for (let i = 0; i < 17; i++) id += ID_CHARS[Math.floor(Math.random() * ID_CHARS.length)];
  return id;
}

export class PB {
  /** ESP32 websocket slots free up lazily after a disconnect — retry. */
  static async connect(ip, attempts = 5) {
    for (let i = 0; ; i++) {
      try {
        return await PB.connectOnce(ip);
      } catch (e) {
        if (i >= attempts - 1) throw e;
        await sleep(2000);
      }
    }
  }

  static async connectOnce(ip) {
    const pb = new PB();
    pb.ws = new WebSocket(`ws://${ip}:81`);
    pb.ws.binaryType = "arraybuffer";
    pb.queue = [];
    pb.waiters = [];
    pb.ws.onmessage = (ev) => {
      if (typeof ev.data !== "string") return; // ignore binary pushes
      let obj;
      try {
        obj = JSON.parse(ev.data);
      } catch {
        return;
      }
      if (obj.fps !== undefined && obj.vmerr !== undefined) {
        pb.lastStats = obj; // 1 Hz stats — keep the latest, don't queue
        return;
      }
      const w = pb.waiters.findIndex((f) => f.pred(obj));
      if (w >= 0) pb.waiters.splice(w, 1)[0].resolve(obj);
      else pb.queue.push(obj);
    };
    await new Promise((resolve, reject) => {
      pb.ws.onopen = resolve;
      pb.ws.onerror = (e) => reject(new Error(`websocket: ${e.message ?? "error"}`));
    });
    return pb;
  }

  send(obj) {
    this.ws.send(JSON.stringify(obj));
  }

  waitFor(pred, what, timeoutMs = 6000) {
    const i = this.queue.findIndex(pred);
    if (i >= 0) return Promise.resolve(this.queue.splice(i, 1)[0]);
    return new Promise((resolve, reject) => {
      const t = setTimeout(() => {
        this.waiters = this.waiters.filter((w) => w.resolve !== resolve);
        reject(new Error(`timeout waiting for ${what}`));
      }, timeoutMs);
      this.waiters.push({
        pred,
        resolve: (v) => {
          clearTimeout(t);
          resolve(v);
        },
      });
    });
  }

  async getConfig() {
    this.send({ getConfig: true });
    const settings = await this.waitFor((o) => o.ver !== undefined, "settings");
    const seq = await this.waitFor((o) => o.activeProgram !== undefined, "sequencer state");
    return { settings, seq };
  }

  /** Live-code: compile-side bytecode → running renderer. Nothing saved. */
  async setCode(bytecode) {
    const crc = zlib.crc32(bytecode);
    this.send({
      pause: true,
      setCode: { size: bytecode.length, crc, name: "", id: makeId() },
    });
    await this.waitFor((o) => o.ack !== undefined, "setCode ack");
    for (let off = 0; off < bytecode.length; off += CHUNK) {
      const chunk = bytecode.subarray(off, off + CHUNK);
      let flags = 0;
      flags |= off === 0 ? FRAME_FIRST : FRAME_MIDDLE;
      if (off + CHUNK >= bytecode.length) flags = (flags & ~FRAME_MIDDLE) | FRAME_LAST;
      if (off === 0 && off + CHUNK >= bytecode.length) flags = FRAME_FIRST | FRAME_LAST;
      const frame = Buffer.concat([Buffer.from([TYPE_PUT_BYTECODE, flags]), chunk]);
      this.ws.send(frame);
    }
    await this.waitFor((o) => o.ack !== undefined, "bytecode ack");
    this.send({ setControls: {} });
    this.send({ pause: false });
  }

  async getVars() {
    this.send({ getVars: true });
    const msg = await this.waitFor((o) => o.vars !== undefined, "vars");
    return msg.vars;
  }

  async setActivePattern(id) {
    this.send({ activeProgramId: id });
    await this.waitFor((o) => o.activeProgram !== undefined, "pattern switch").catch(() => {});
  }

  /** Close and wait for the close handshake so the device frees the slot. */
  async close() {
    await new Promise((resolve) => {
      const t = setTimeout(resolve, 2000);
      this.ws.onclose = () => {
        clearTimeout(t);
        resolve();
      };
      this.ws.close();
    });
  }
}

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
