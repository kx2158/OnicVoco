import test from 'node:test';
import assert from 'node:assert/strict';
import { detectPitch } from '../src/audio/pitch.js';
import { estimateFormants, resonanceScore } from '../src/audio/formants.js';
import { hzToNote, formatCents } from '../src/notes.js';
import { computeStats, basicFeedback, buildCoachPrompt } from '../src/coach.js';
import { makeTarget } from '../src/targets.js';

const SR = 48000;

function sine(hz, n = 2048) {
  const b = new Float32Array(n);
  for (let i = 0; i < n; i++) b[i] = 0.5 * Math.sin((2 * Math.PI * hz * i) / SR);
  return b;
}

// Crude vowel: impulse train through two resonators (F1, F2) plus a 3rd.
function vowel(f0, f1, f2, n = 2048, f3 = 2800) {
  const x = new Float64Array(n);
  const period = SR / f0;
  for (let p = 0; p < n; p += period) x[Math.floor(p)] = 1;
  const resonate = (input, fc, bw) => {
    const r = Math.exp((-Math.PI * bw) / SR);
    const c = 2 * r * Math.cos((2 * Math.PI * fc) / SR);
    const out = new Float64Array(input.length);
    for (let i = 0; i < input.length; i++) {
      out[i] = input[i] + c * (out[i - 1] ?? 0) - r * r * (out[i - 2] ?? 0);
    }
    return out;
  };
  let y = resonate(x, f1, 80);
  y = resonate(y, f2, 100);
  y = resonate(y, f3, 150);
  let max = 0;
  for (const v of y) max = Math.max(max, Math.abs(v));
  return Float32Array.from(y, (v) => (0.6 * v) / max);
}

test('pitch: sines across the speaking range', () => {
  for (const hz of [70, 98, 130.8, 196, 220, 261.6, 440]) {
    const r = detectPitch(sine(hz), SR);
    assert.ok(r, `no pitch for ${hz}`);
    assert.ok(Math.abs(r.hz - hz) / hz < 0.01, `${hz} -> ${r.hz}`);
  }
});

test('pitch: harmonic-rich vowels', () => {
  for (const f0 of [110, 150, 200, 240]) {
    const r = detectPitch(vowel(f0, 600, 1700), SR);
    assert.ok(r, `no pitch for ${f0}`);
    assert.ok(Math.abs(r.hz - f0) / f0 < 0.02, `${f0} -> ${r.hz}`);
  }
});

test('pitch: silence and noise return null', () => {
  assert.equal(detectPitch(new Float32Array(2048), SR), null);
  const noise = Float32Array.from({ length: 2048 }, () => Math.random() * 2 - 1);
  assert.equal(detectPitch(noise, SR), null);
});

test('formants: recover F1/F2 of synthetic vowels', () => {
  const cases = [
    [120, 700, 1220], // dark "ah"
    [200, 850, 2000], // bright "a"
    [180, 400, 2100], // "ee"-like
  ];
  for (const [f0, f1, f2] of cases) {
    const est = estimateFormants(vowel(f0, f1, f2, 4096), SR);
    assert.ok(est, `no formants for ${f1}/${f2}`);
    assert.ok(Math.abs(est.f1 - f1) < 120, `F1 ${f1} -> ${est.f1}`);
    assert.ok(Math.abs(est.f2 - f2) < 200, `F2 ${f2} -> ${est.f2}`);
  }
});

test('resonance: brighter formants score higher', () => {
  const dark = resonanceScore({ f1: 450, f2: 1200 });
  const bright = resonanceScore({ f1: 680, f2: 1900 });
  assert.ok(dark < 0.2 && bright > 0.8, `${dark} ${bright}`);
});

test('notes: scientific pitch notation', () => {
  assert.equal(hzToNote(440).name, 'A4');
  assert.equal(hzToNote(261.63).name, 'C4');
  assert.equal(hzToNote(196).name, 'G3');
  assert.equal(hzToNote(82.41).name, 'E2');
  assert.equal(hzToNote(445).cents, 20);
  assert.equal(formatCents(-12), '−12¢');
});

test('coach: stats and feedback', () => {
  const target = makeTarget('feminine');
  const frames = [];
  for (let i = 0; i < 300; i++) {
    frames.push({ t: i * 0.05, hz: i % 4 === 0 ? null : 190 + 20 * Math.sin(i / 10), res: 0.5 });
  }
  const s = computeStats(frames, target);
  assert.ok(s.enough);
  assert.ok(s.inTarget > 0.95);
  assert.ok(Math.abs(s.voicedSeconds - 11.2) < 0.5, String(s.voicedSeconds));
  const tips = basicFeedback(s, target);
  assert.ok(tips.length >= 3);
  assert.ok(tips.some((t) => t.includes('darker')));
  const prompt = buildCoachPrompt(s, target);
  assert.match(prompt, /Median pitch/);
  assert.ok(!/—/.test(tips.join(' ') + prompt), 'no em dashes');
});
