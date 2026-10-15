// Microphone capture + analysis loop.

import { detectPitch, rmsDb } from './pitch.js';
import { estimateFormants, resonanceScore } from './formants.js';

const FRAME = 4096; // samples analysed per tick (pitch uses the newest 2048)

export class AudioEngine {
  constructor() {
    this.ctx = null;
    this.stream = null;
    this.analyser = null;
    this.buffer = new Float32Array(FRAME);
    this.running = false;
    this.gateDb = -50;
    this.onFrame = null; // (frame) => void
    this.recorder = null;
    this.chunks = [];
    this.recordingMime = '';
    this._recent = [];
    this._resSmooth = null;
    this._raf = 0;
    this._tick = 0;
  }

  static async listInputs() {
    if (!navigator.mediaDevices?.enumerateDevices) return [];
    const all = await navigator.mediaDevices.enumerateDevices();
    return all.filter((d) => d.kind === 'audioinput');
  }

  async start(deviceId, { record = false } = {}) {
    await this.stop();
    this.stream = await navigator.mediaDevices.getUserMedia({
      audio: {
        deviceId: deviceId ? { exact: deviceId } : undefined,
        // Processing would distort pitch and resonance measurements.
        echoCancellation: false,
        noiseSuppression: false,
        autoGainControl: false,
      },
    });
    const Ctx = window.AudioContext || window.webkitAudioContext;
    this.ctx = new Ctx();
    if (this.ctx.state === 'suspended') await this.ctx.resume();
    const src = this.ctx.createMediaStreamSource(this.stream);
    this.analyser = this.ctx.createAnalyser();
    this.analyser.fftSize = FRAME;
    this.analyser.smoothingTimeConstant = 0;
    src.connect(this.analyser);

    this.chunks = [];
    this.recorder = null;
    if (record && typeof MediaRecorder !== 'undefined') {
      const types = ['audio/webm;codecs=opus', 'audio/webm', 'audio/mp4', 'audio/ogg;codecs=opus'];
      this.recordingMime = types.find((t) => MediaRecorder.isTypeSupported?.(t)) || '';
      try {
        this.recorder = new MediaRecorder(this.stream, this.recordingMime ? { mimeType: this.recordingMime } : undefined);
        this.recordingMime = this.recorder.mimeType || this.recordingMime || 'audio/webm';
        this.recorder.ondataavailable = (e) => e.data.size && this.chunks.push(e.data);
        this.recorder.start(1000);
      } catch (err) {
        console.warn('Recording unavailable', err);
        this.recorder = null;
      }
    }

    this._recent = [];
    this._resSmooth = null;
    this.running = true;
    this.startedAt = performance.now();
    this._loop();
    return this.stream.getAudioTracks()[0]?.getSettings?.().deviceId;
  }

  _loop = () => {
    if (!this.running) return;
    this._raf = requestAnimationFrame(this._loop);
    // ~30 analyses per second is plenty and keeps CPU use low.
    if (this._tick++ % 2) return;

    this.analyser.getFloatTimeDomainData(this.buffer);
    const sr = this.ctx.sampleRate;
    const db = rmsDb(this.buffer);
    const t = (performance.now() - this.startedAt) / 1000;

    let hz = null;
    let res = null;
    let formants = null;
    let clarity = 0;

    if (db > this.gateDb) {
      const p = detectPitch(this.buffer.subarray(FRAME - 2048), sr, { minHz: 60, maxHz: 700 });
      if (p && p.clarity > 0.8) {
        hz = this._stabilise(p.hz);
        clarity = p.clarity;
        formants = estimateFormants(this.buffer, sr);
        if (formants) {
          const raw = resonanceScore(formants);
          this._resSmooth = this._resSmooth == null ? raw : this._resSmooth * 0.85 + raw * 0.15;
          res = this._resSmooth;
        }
      } else {
        this._recent = [];
      }
    } else {
      this._recent = [];
    }

    this.onFrame?.({ t, db, hz, clarity, res, formants });
  };

  // Median of the last few readings removes single-frame octave jumps.
  _stabilise(hz) {
    this._recent.push(hz);
    if (this._recent.length > 5) this._recent.shift();
    const s = [...this._recent].sort((a, b) => a - b);
    return s[Math.floor(s.length / 2)];
  }

  /** Stops capture. Resolves with the recording Blob, if one was made. */
  async stop() {
    this.running = false;
    cancelAnimationFrame(this._raf);
    let blob = null;
    if (this.recorder && this.recorder.state !== 'inactive') {
      blob = await new Promise((resolve) => {
        this.recorder.onstop = () => resolve(new Blob(this.chunks, { type: this.recordingMime }));
        this.recorder.stop();
      });
    }
    this.recorder = null;
    this.stream?.getTracks().forEach((tr) => tr.stop());
    this.stream = null;
    if (this.ctx) await this.ctx.close().catch(() => {});
    this.ctx = null;
    return blob;
  }
}
