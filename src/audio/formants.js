// Formant estimation via LPC (linear predictive coding).
// We fit an all-pole model to a short voiced frame, evaluate its spectral
// envelope and pick the first peaks as F1 and F2.
// This is an estimate: formants depend on the vowel being spoken, so the app
// averages them over time instead of trusting single frames.

const TARGET_RATE = 11025;
const MIN_PROMINENCE_DB = 2;

/** Simple decimation with a box low-pass filter. */
function downsample(buf, sampleRate) {
  const factor = Math.max(1, Math.round(sampleRate / TARGET_RATE));
  if (factor === 1) return { data: Float64Array.from(buf), rate: sampleRate };
  const n = Math.floor(buf.length / factor);
  const out = new Float64Array(n);
  for (let i = 0; i < n; i++) {
    let s = 0;
    for (let k = 0; k < factor; k++) s += buf[i * factor + k];
    out[i] = s / factor;
  }
  return { data: out, rate: sampleRate / factor };
}

/** Levinson-Durbin recursion. Returns LPC coefficients a[0..order], a[0] = 1. */
export function levinson(r, order) {
  const a = new Float64Array(order + 1);
  const tmp = new Float64Array(order + 1);
  a[0] = 1;
  let err = r[0];
  if (err <= 0) return null;
  for (let i = 1; i <= order; i++) {
    let acc = r[i];
    for (let j = 1; j < i; j++) acc += a[j] * r[i - j];
    const k = -acc / err;
    tmp.set(a);
    for (let j = 1; j < i; j++) a[j] = tmp[j] + k * tmp[i - j];
    a[i] = k;
    err *= 1 - k * k;
    if (err <= 0) return null;
  }
  return a;
}

/** LPC coefficients for a frame (already downsampled). */
function lpc(x, order) {
  const n = x.length;
  // pre-emphasis + Hamming window
  const w = new Float64Array(n);
  for (let i = 0; i < n; i++) {
    const pre = i === 0 ? x[0] : x[i] - 0.97 * x[i - 1];
    w[i] = pre * (0.54 - 0.46 * Math.cos((2 * Math.PI * i) / (n - 1)));
  }
  const r = new Float64Array(order + 1);
  for (let lag = 0; lag <= order; lag++) {
    let s = 0;
    for (let i = lag; i < n; i++) s += w[i] * w[i - lag];
    r[lag] = s;
  }
  r[0] *= 1 + 1e-9; // tiny white-noise correction for stability
  return levinson(r, order);
}

/** Magnitude (dB) of the LPC envelope at the given frequencies. */
function envelope(a, rate, freqs) {
  const out = new Float64Array(freqs.length);
  for (let fi = 0; fi < freqs.length; fi++) {
    const w = (2 * Math.PI * freqs[fi]) / rate;
    let re = 0;
    let im = 0;
    for (let k = 0; k < a.length; k++) {
      re += a[k] * Math.cos(w * k);
      im -= a[k] * Math.sin(w * k);
    }
    out[fi] = -10 * Math.log10(re * re + im * im + 1e-12);
  }
  return out;
}

const FREQS = (() => {
  const f = [];
  for (let hz = 150; hz <= 4000; hz += 10) f.push(hz);
  return f;
})();

/**
 * Estimate the first two formants of a voiced frame.
 * @param {Float32Array} buf
 * @param {number} sampleRate
 * @returns {{f1: number, f2: number} | null}
 */
export function estimateFormants(buf, sampleRate) {
  const { data, rate } = downsample(buf, sampleRate);
  if (data.length < 128) return null;
  const order = Math.round(2 + rate / 1000); // ~13 at 11 kHz
  const a = lpc(data, order);
  if (!a) return null;
  const env = envelope(a, rate, FREQS);

  // Local maxima with enough prominence over the neighbouring valleys.
  // High-pitched voices have widely spaced harmonics that can create small
  // ripples in the envelope; prominence filtering ignores those.
  const peaks = [];
  for (let i = 1; i < env.length - 1; i++) {
    if (!(env[i] > env[i - 1] && env[i] >= env[i + 1])) continue;
    let l = i;
    while (l > 0 && env[l - 1] <= env[l]) l--;
    let r = i;
    while (r < env.length - 1 && env[r + 1] <= env[r]) r++;
    const prominence = env[i] - Math.max(env[l], env[r]);
    if (prominence >= MIN_PROMINENCE_DB) peaks.push(FREQS[i]);
  }
  const f1 = peaks.find((f) => f >= 200 && f <= 1100);
  if (f1 === undefined) return null;
  const f2 = peaks.find((f) => f > f1 + 250 && f >= 700 && f <= 3200);
  if (f2 === undefined) return null;
  return { f1, f2 };
}

const clamp01 = (v) => Math.max(0, Math.min(1, v));

/**
 * Map formants to a 0..1 "brightness" score (0 = darker, 1 = brighter).
 * Calibrated loosely on average speaking values: darker voices tend to sit
 * around F1 ~ 450 / F2 ~ 1250 Hz and brighter ones around F1 ~ 650 / F2 ~ 1850 Hz.
 * Treat it as a relative trend line, not a verdict.
 */
export function resonanceScore({ f1, f2 }) {
  const s2 = clamp01((f2 - 1150) / (1950 - 1150));
  const s1 = clamp01((f1 - 380) / (720 - 380));
  return clamp01(0.75 * s2 + 0.25 * s1);
}
