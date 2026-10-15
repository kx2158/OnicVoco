// YIN pitch detection (de Cheveigne & Kawahara, 2002).
// Pure function, no browser APIs, so it can be unit tested in Node.

/**
 * @param {Float32Array} buf mono samples in [-1, 1]
 * @param {number} sampleRate
 * @param {{minHz?: number, maxHz?: number, threshold?: number}} [opts]
 * @returns {{hz: number, clarity: number} | null}
 */
export function detectPitch(buf, sampleRate, opts = {}) {
  const minHz = opts.minHz ?? 60;
  const maxHz = opts.maxHz ?? 1000;
  const threshold = opts.threshold ?? 0.15;

  const maxTau = Math.min(Math.floor(sampleRate / minHz), Math.floor(buf.length / 2));
  const minTau = Math.max(2, Math.floor(sampleRate / maxHz));
  const W = buf.length - maxTau;
  if (W <= 0 || maxTau <= minTau) return null;

  // Step 2: difference function
  const d = new Float32Array(maxTau + 1);
  for (let tau = 1; tau <= maxTau; tau++) {
    let sum = 0;
    for (let i = 0; i < W; i++) {
      const delta = buf[i] - buf[i + tau];
      sum += delta * delta;
    }
    d[tau] = sum;
  }

  // Step 3: cumulative mean normalized difference
  const cmnd = new Float32Array(maxTau + 1);
  cmnd[0] = 1;
  let running = 0;
  for (let tau = 1; tau <= maxTau; tau++) {
    running += d[tau];
    cmnd[tau] = running > 0 ? (d[tau] * tau) / running : 1;
  }

  // Step 4: absolute threshold, then walk down to the local minimum
  let tau = -1;
  for (let t = minTau; t <= maxTau; t++) {
    if (cmnd[t] < threshold) {
      while (t + 1 <= maxTau && cmnd[t + 1] < cmnd[t]) t++;
      tau = t;
      break;
    }
  }
  if (tau === -1) return null;

  // Step 5: parabolic interpolation for sub-sample accuracy
  let betterTau = tau;
  if (tau > 1 && tau < maxTau) {
    const s0 = cmnd[tau - 1];
    const s1 = cmnd[tau];
    const s2 = cmnd[tau + 1];
    const denom = 2 * (2 * s1 - s2 - s0);
    if (denom !== 0) betterTau = tau + (s2 - s0) / denom;
  }

  const hz = sampleRate / betterTau;
  if (hz < minHz || hz > maxHz) return null;
  return { hz, clarity: Math.max(0, Math.min(1, 1 - cmnd[tau])) };
}

/** Root mean square level of a buffer. */
export function rms(buf) {
  let s = 0;
  for (let i = 0; i < buf.length; i++) s += buf[i] * buf[i];
  return Math.sqrt(s / buf.length);
}

/** RMS converted to dBFS, floored at -100. */
export function rmsDb(buf) {
  const r = rms(buf);
  return r > 0 ? Math.max(-100, 20 * Math.log10(r)) : -100;
}
