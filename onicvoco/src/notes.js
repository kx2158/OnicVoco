// Scientific pitch notation helpers (A4 = 440 Hz, middle C = C4).

export const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];

export const hzToMidi = (hz) => 69 + 12 * Math.log2(hz / 440);
export const midiToHz = (midi) => 440 * Math.pow(2, (midi - 69) / 12);

export function midiName(midi) {
  const n = Math.round(midi);
  return NOTE_NAMES[((n % 12) + 12) % 12] + (Math.floor(n / 12) - 1);
}

/**
 * @param {number} hz
 * @returns {{name: string, midi: number, cents: number}}
 */
export function hzToNote(hz) {
  const exact = hzToMidi(hz);
  const midi = Math.round(exact);
  const cents = Math.round((exact - midi) * 100);
  return { name: midiName(midi), midi, cents };
}

export function formatCents(c) {
  if (c === 0) return '±0¢';
  return (c > 0 ? '+' : '−') + Math.abs(c) + '¢';
}

/** Semitone distance between two frequencies. */
export const semitones = (a, b) => 12 * Math.log2(b / a);

export const isSharp = (midi) => NOTE_NAMES[((midi % 12) + 12) % 12].includes('#');
