// Session statistics and rule-based feedback.
// The numbers are always computed here, never by the AI model: small language
// models are unreliable with maths, so they only rephrase what we measured.

import { hzToNote, semitones } from './notes.js';

function percentile(sorted, p) {
  if (!sorted.length) return NaN;
  const idx = Math.min(sorted.length - 1, Math.max(0, (sorted.length - 1) * p));
  const lo = Math.floor(idx);
  const hi = Math.ceil(idx);
  return sorted[lo] + (sorted[hi] - sorted[lo]) * (idx - lo);
}

/**
 * @param {{t: number, hz: number|null, res: number|null}[]} frames
 * @param {{minHz: number, maxHz: number}} target
 */
export function computeStats(frames, target) {
  const voiced = frames.filter((f) => f.hz);
  const hzs = voiced.map((f) => f.hz).sort((a, b) => a - b);
  const res = frames.filter((f) => f.res != null).map((f) => f.res);
  const duration = frames.length ? frames[frames.length - 1].t - frames[0].t : 0;
  // Frames arrive at a steady rate, so voiced share of frames ~ voiced share of time.
  const voicedSeconds = frames.length ? (duration * voiced.length) / frames.length : 0;

  if (hzs.length < 5) {
    return { enough: false, duration, voicedSeconds };
  }

  const median = percentile(hzs, 0.5);
  const low = percentile(hzs, 0.05);
  const high = percentile(hzs, 0.95);
  const inTarget = hzs.filter((h) => h >= target.minHz && h <= target.maxHz).length / hzs.length;
  const below = hzs.filter((h) => h < target.minHz).length / hzs.length;
  const above = hzs.filter((h) => h > target.maxHz).length / hzs.length;

  // Stability: median absolute deviation from the median, in semitones.
  const devs = hzs.map((h) => Math.abs(semitones(median, h))).sort((a, b) => a - b);
  const spread = percentile(devs, 0.5);

  const resSorted = [...res].sort((a, b) => a - b);
  const resonance = res.length >= 5 ? percentile(resSorted, 0.5) : null;

  // Resonance trend: compare first and second half of the session.
  let resonanceDrift = null;
  if (res.length >= 20) {
    const half = Math.floor(res.length / 2);
    const avg = (arr) => arr.reduce((s, v) => s + v, 0) / arr.length;
    resonanceDrift = avg(res.slice(half)) - avg(res.slice(0, half));
  }

  return {
    enough: true,
    duration,
    voicedSeconds,
    medianHz: median,
    medianNote: hzToNote(median),
    lowHz: low,
    highHz: high,
    lowNote: hzToNote(low).name,
    highNote: hzToNote(high).name,
    rangeSemitones: semitones(low, high),
    inTarget,
    below,
    above,
    spread,
    resonance,
    resonanceDrift,
  };
}

const pct = (v) => Math.round(v * 100) + '%';

/** Short rule-based tips. Always available, no model needed. */
export function basicFeedback(stats, target) {
  if (!stats.enough) {
    return ['Not enough voiced sound to analyse yet. Try speaking or reading aloud for at least 10 seconds.'];
  }
  const tips = [];

  if (stats.inTarget >= 0.7) {
    tips.push(`Lovely - ${pct(stats.inTarget)} of your voiced time sat in your ${target.label.toLowerCase()} pitch range.`);
  } else if (stats.below > stats.above) {
    tips.push(`${pct(stats.below)} of the time you dipped below ${Math.round(target.minHz)} Hz. Try anchoring your sentences a little higher, especially at the ends of phrases.`);
  } else {
    tips.push(`${pct(stats.above)} of the time you went above ${Math.round(target.maxHz)} Hz. You can relax down a bit - a strained high pitch tends to read less natural than a comfortable one.`);
  }

  if (stats.spread < 0.8) {
    tips.push('Your pitch was quite flat. Natural speech moves around - try letting your intonation rise and fall more, like you are telling an exciting story.');
  } else if (stats.spread > 3.5) {
    tips.push('Your pitch jumped around a lot. That can be great for expressiveness, but if it felt unsteady, try slower, smoother phrases.');
  } else {
    tips.push('Your intonation had a healthy, natural amount of movement.');
  }

  const goal = target.resonance;
  if (stats.resonance != null && goal) {
    if (stats.resonance < goal.min) {
      tips.push(`Resonance read darker than your goal. Small-dog/big-dog panting drills or a gentle "ee" smile shape can help brighten it.`);
    } else if (stats.resonance > goal.max) {
      tips.push('Resonance read brighter than your goal. Try a more open, relaxed throat - think of a yawn-like space.');
    } else {
      tips.push(`Resonance landed in your ${goal.word} target zone. Nice!`);
    }
    if (stats.resonanceDrift != null && Math.abs(stats.resonanceDrift) > 0.12) {
      tips.push(
        stats.resonanceDrift < 0
          ? 'Your resonance got darker as the session went on - that often happens when you get tired. Short breaks help.'
          : 'Your resonance got brighter as you warmed up. Good sign!'
      );
    }
  }

  tips.push('Reminder: rest if your throat feels tight or scratchy. Voice training should never hurt.');
  return tips;
}

/** Compact, factual summary handed to the optional AI coach. */
export function buildCoachPrompt(stats, target) {
  const lines = [
    `Goal: ${target.label} voice, pitch range ${Math.round(target.minHz)}-${Math.round(target.maxHz)} Hz.`,
    `Voiced speaking time: ${Math.round(stats.voicedSeconds)} seconds.`,
    `Median pitch: ${Math.round(stats.medianHz)} Hz (${stats.medianNote.name}).`,
    `Typical range: ${stats.lowNote} to ${stats.highNote} (${stats.rangeSemitones.toFixed(1)} semitones).`,
    `Time inside the goal pitch range: ${pct(stats.inTarget)}. Below: ${pct(stats.below)}. Above: ${pct(stats.above)}.`,
    `Intonation movement: ${stats.spread < 0.8 ? 'flat' : stats.spread > 3.5 ? 'very jumpy' : 'natural'}.`,
  ];
  if (stats.resonance != null) {
    const r = stats.resonance;
    lines.push(`Resonance: ${r < 0.35 ? 'dark' : r < 0.6 ? 'medium' : 'bright'} (score ${r.toFixed(2)} on 0=dark to 1=bright).`);
    if (target.resonance) lines.push(`Resonance goal: ${target.resonance.word}.`);
    if (stats.resonanceDrift != null) {
      lines.push(`Resonance change during session: ${stats.resonanceDrift > 0.05 ? 'got brighter' : stats.resonanceDrift < -0.05 ? 'got darker' : 'steady'}.`);
    }
  }
  return lines.join('\n');
}

export const COACH_SYSTEM_PROMPT =
  'You are a warm, encouraging voice training coach. You receive measured statistics from a practice session. ' +
  'Write 3 to 5 short sentences of feedback in plain, friendly language: one thing that went well, one or two things to work on, and one concrete exercise to try next. ' +
  'Only use the numbers given. Do not invent measurements. Never comment on the person\'s identity or whether they "pass". ' +
  'Do not use em dashes.';
