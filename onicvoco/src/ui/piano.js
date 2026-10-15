// Chromatic note strip from E2 to C5 with the target range highlighted.

import { hzToMidi, midiName, isSharp } from '../notes.js';

const FIRST = 40; // E2
const LAST = 72; // C5

export class NoteStrip {
  constructor(el) {
    this.el = el;
    this.keys = new Map();
    this.current = null;
    for (let m = FIRST; m <= LAST; m++) {
      const k = document.createElement('div');
      k.className = 'key' + (isSharp(m) ? ' sharp' : '');
      const name = midiName(m);
      k.title = name;
      if (!isSharp(m)) k.textContent = name;
      el.appendChild(k);
      this.keys.set(m, k);
    }
  }

  setTarget(target) {
    const lo = Math.round(hzToMidi(target.minHz));
    const hi = Math.round(hzToMidi(target.maxHz));
    for (const [m, k] of this.keys) k.classList.toggle('target', m >= lo && m <= hi);
  }

  setNote(midi) {
    if (midi === this.current) return;
    this.keys.get(this.current)?.classList.remove('on');
    this.current = midi;
    this.keys.get(midi)?.classList.add('on');
  }
}
