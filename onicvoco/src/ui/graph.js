// Scrolling pitch-over-time graph on a canvas, log-frequency axis.

import { hzToMidi, midiToHz, midiName } from '../notes.js';

const WINDOW_S = 12;
const MIN_MIDI = 36; // C2  ~65 Hz
const MAX_MIDI = 72; // C5 ~523 Hz

export class PitchGraph {
  constructor(canvas) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d');
    this.points = [];
    this.target = null;
    this.colors = {};
    this.refreshColors();
    new ResizeObserver(() => this.draw()).observe(canvas);
  }

  refreshColors() {
    const cs = getComputedStyle(document.documentElement);
    const v = (n) => cs.getPropertyValue(n).trim();
    this.colors = {
      text: v('--muted'),
      grid: v('--border'),
      band: v('--accent-soft'),
      line: v('--accent'),
      good: v('--good'),
    };
    this.draw();
  }

  setTarget(target) {
    this.target = target;
    this.draw();
  }

  clear() {
    this.points = [];
    this.draw();
  }

  push(t, hz) {
    this.points.push({ t, hz });
    const cutoff = t - WINDOW_S - 1;
    while (this.points.length && this.points[0].t < cutoff) this.points.shift();
  }

  draw() {
    const { canvas, ctx } = this;
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    if (!w || !h) return;
    if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
      canvas.width = Math.round(w * dpr);
      canvas.height = Math.round(h * dpr);
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);

    const left = 38;
    const right = w - 6;
    const top = 8;
    const bottom = h - 8;
    const y = (hz) => {
      const m = Math.min(MAX_MIDI, Math.max(MIN_MIDI, hzToMidi(hz)));
      return bottom - ((m - MIN_MIDI) / (MAX_MIDI - MIN_MIDI)) * (bottom - top);
    };

    // target band
    if (this.target) {
      const y1 = y(this.target.maxHz);
      const y2 = y(this.target.minHz);
      ctx.fillStyle = this.colors.band;
      roundRect(ctx, left, y1, right - left, y2 - y1, 10);
      ctx.fill();
    }

    // note grid: every C, E and G
    ctx.font = '600 10px ' + getComputedStyle(document.body).fontFamily;
    ctx.textBaseline = 'middle';
    for (let m = MIN_MIDI; m <= MAX_MIDI; m++) {
      const pc = m % 12;
      if (pc !== 0 && pc !== 4 && pc !== 7) continue;
      const yy = y(midiToHz(m));
      ctx.strokeStyle = this.colors.grid;
      ctx.lineWidth = pc === 0 ? 1.2 : 0.6;
      ctx.beginPath();
      ctx.moveTo(left, yy);
      ctx.lineTo(right, yy);
      ctx.stroke();
      ctx.fillStyle = this.colors.text;
      ctx.fillText(midiName(m), 4, yy);
    }

    // pitch line, broken across unvoiced gaps
    if (!this.points.length) return;
    const now = this.points[this.points.length - 1].t;
    const x = (t) => right - ((now - t) / WINDOW_S) * (right - left);
    ctx.lineWidth = 3;
    ctx.lineJoin = 'round';
    ctx.lineCap = 'round';
    let prev = null;
    for (const p of this.points) {
      if (!p.hz || p.t < now - WINDOW_S) {
        prev = null;
        continue;
      }
      const inBand = this.target && p.hz >= this.target.minHz && p.hz <= this.target.maxHz;
      if (prev) {
        ctx.strokeStyle = inBand ? this.colors.good : this.colors.line;
        ctx.beginPath();
        ctx.moveTo(x(prev.t), y(prev.hz));
        ctx.lineTo(x(p.t), y(p.hz));
        ctx.stroke();
      }
      prev = p;
    }
  }
}

function roundRect(ctx, x, y, w, h, r) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}
