// Training goals. Pitch bands are common rough guides from voice training
// communities, not rules: every voice is different and users can customise.
// Resonance goals use the 0..1 brightness score from formants.js.

export const PRESETS = {
  feminine: {
    label: 'Feminine',
    minHz: 165,
    maxHz: 255,
    resonance: { min: 0.6, max: 1, word: 'brighter' },
  },
  androgynous: {
    label: 'Androgynous',
    minHz: 145,
    maxHz: 175,
    resonance: { min: 0.4, max: 0.65, word: 'balanced' },
  },
  masculine: {
    label: 'Masculine',
    minHz: 85,
    maxHz: 155,
    resonance: { min: 0, max: 0.4, word: 'darker' },
  },
};

export function makeTarget(key, custom) {
  if (key === 'custom' && custom) {
    const minHz = Math.min(custom.minHz, custom.maxHz);
    const maxHz = Math.max(custom.minHz, custom.maxHz);
    return {
      key,
      label: 'Custom',
      minHz,
      maxHz,
      resonance: custom.resonance ?? null,
    };
  }
  const p = PRESETS[key] ?? PRESETS.feminine;
  return { key, ...p };
}
