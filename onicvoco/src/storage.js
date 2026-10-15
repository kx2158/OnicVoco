// Settings + progress history, kept in localStorage on this device only.

const SETTINGS_KEY = 'onicvoco.settings';
const HISTORY_KEY = 'onicvoco.history';

const DEFAULTS = {
  theme: 'system',
  goal: 'feminine',
  custom: { minHz: 160, maxHz: 220 },
  gateDb: -50,
  record: true,
  micId: '',
  aiEnabled: false,
};

function read(key, fallback) {
  try {
    const v = JSON.parse(localStorage.getItem(key));
    return v ?? fallback;
  } catch {
    return fallback;
  }
}

function write(key, value) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* storage unavailable: keep working in memory */
  }
}

export function loadSettings() {
  return { ...DEFAULTS, ...read(SETTINGS_KEY, {}) };
}

export function saveSettings(s) {
  write(SETTINGS_KEY, s);
}

export function loadHistory() {
  const h = read(HISTORY_KEY, []);
  return Array.isArray(h) ? h : [];
}

export function addHistory(entry) {
  const h = loadHistory();
  h.unshift(entry);
  write(HISTORY_KEY, h.slice(0, 200));
  return h;
}

export function clearHistory() {
  write(HISTORY_KEY, []);
}
