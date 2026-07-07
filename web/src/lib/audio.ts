// Browser-microphone sensor source: WebAudio AnalyserNode reshaped into the
// PB sensor board's audio surface — 32 bands spanning ~37 Hz–10 kHz
// (log-spaced like the board's output), everything normalized 0..1, plus
// energyAverage / maxFrequency(Hz) / maxFrequencyMagnitude.

import type { SensorFrame } from "./luxel";

const BANDS = 32;
const LO_HZ = 37;
const HI_HZ = 10_000;

/** Encode a frame as the sensor board's 98-byte serial format
 *  ("SB1.0\0" … "END\0") — what POST /api/sensors expects, so the browser
 *  mic can stand in for the physical board. */
export function toSensorBoardFrame(s: SensorFrame): Uint8Array {
  const buf = new Uint8Array(98);
  const view = new DataView(buf.buffer);
  const ascii = (at: number, str: string): void => {
    for (let i = 0; i < str.length; i++) buf[at + i] = str.charCodeAt(i);
  };
  const u16 = (at: number, v: number): void =>
    view.setUint16(at, Math.max(0, Math.min(0xffff, Math.round(v * 0x10000))), true);
  ascii(0, "SB1.0\0");
  for (let i = 0; i < BANDS; i++) u16(6 + i * 2, s.frequencyData[i] ?? 0);
  u16(70, s.energyAverage);
  u16(72, s.maxFrequencyMagnitude);
  view.setUint16(74, Math.max(0, Math.min(0xffff, Math.round(s.maxFrequency))), true);
  for (let i = 0; i < 3; i++)
    view.setInt16(
      76 + i * 2,
      Math.max(-0x8000, Math.min(0x7fff, Math.round((s.accelerometer?.[i] ?? 0) * 0x10000))),
      true,
    );
  u16(82, s.light ?? 0);
  for (let i = 0; i < 5; i++) u16(84 + i * 2, s.analogInputs?.[i] ?? 0);
  ascii(94, "END\0");
  return buf;
}

export class MicSource {
  private ctx: AudioContext | null = null;
  private analyser: AnalyserNode | null = null;
  private stream: MediaStream | null = null;
  private bytes = new Uint8Array(0);
  /** analyser-bin [start, end) per band */
  private bands: Array<[number, number]> = [];

  get running(): boolean {
    return this.ctx !== null;
  }

  /** Ask for the microphone and start analysing. Throws if denied. */
  async start(): Promise<void> {
    if (this.ctx) return;
    this.stream = await navigator.mediaDevices.getUserMedia({
      audio: { echoCancellation: false, autoGainControl: false, noiseSuppression: false },
    });
    const ctx = new AudioContext();
    const analyser = ctx.createAnalyser();
    analyser.fftSize = 2048;
    analyser.smoothingTimeConstant = 0.5; // a little decay, like the board
    ctx.createMediaStreamSource(this.stream).connect(analyser);
    this.bytes = new Uint8Array(analyser.frequencyBinCount);
    // log-spaced band edges over the FFT bins (≥1 bin per band)
    const hzPerBin = ctx.sampleRate / analyser.fftSize;
    this.bands = [];
    for (let b = 0; b < BANDS; b++) {
      const f0 = LO_HZ * Math.pow(HI_HZ / LO_HZ, b / BANDS);
      const f1 = LO_HZ * Math.pow(HI_HZ / LO_HZ, (b + 1) / BANDS);
      const s = Math.max(1, Math.floor(f0 / hzPerBin));
      const e = Math.max(s + 1, Math.ceil(f1 / hzPerBin));
      this.bands.push([s, Math.min(e, this.bytes.length)]);
    }
    // autoplay policy can start the context suspended (e.g. a synthetic
    // click) — resume explicitly so the analyser actually runs
    await ctx.resume();
    this.ctx = ctx;
    this.analyser = analyser;
  }

  stop(): void {
    this.stream?.getTracks().forEach((t) => t.stop());
    void this.ctx?.close();
    this.ctx = null;
    this.analyser = null;
    this.stream = null;
  }

  /** Snapshot the current spectrum as a sensor frame. */
  frame(): SensorFrame {
    const freq = new Array<number>(BANDS).fill(0);
    let energy = 0;
    let maxV = 0;
    let maxHz = 0;
    if (this.ctx && this.analyser) {
      this.analyser.getByteFrequencyData(this.bytes);
      const hzPerBin = this.ctx.sampleRate / this.analyser.fftSize;
      for (let b = 0; b < BANDS; b++) {
        const [s, e] = this.bands[b] ?? [0, 0];
        let sum = 0;
        for (let i = s; i < e; i++) sum += this.bytes[i] ?? 0;
        const v = e > s ? sum / (e - s) / 255 : 0;
        freq[b] = v;
        energy += v;
      }
      // loudest raw bin inside the band range → its center frequency
      const lo = this.bands[0]?.[0] ?? 1;
      const hi = this.bands[BANDS - 1]?.[1] ?? this.bytes.length;
      for (let i = lo; i < hi; i++) {
        const v = this.bytes[i] ?? 0;
        if (v > maxV) {
          maxV = v;
          maxHz = i * hzPerBin;
        }
      }
    }
    return {
      frequencyData: freq,
      energyAverage: energy / BANDS,
      maxFrequencyMagnitude: maxV / 255,
      maxFrequency: maxHz,
    };
  }
}
