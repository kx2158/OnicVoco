import '@fontsource-variable/nunito';
import './styles.css';

import { AudioEngine } from './audio/engine.js';
import { hzToNote, formatCents } from './notes.js';
import { makeTarget } from './targets.js';
import { computeStats, basicFeedback, buildCoachPrompt, COACH_SYSTEM_PROMPT } from './coach.js';
import { loadSettings, saveSettings, loadHistory, addHistory, clearHistory } from './storage.js';
import { PitchGraph } from './ui/graph.js';
import { NoteStrip } from './ui/piano.js';
import { AICoach } from './ai/coach-ai.js';

const $ = (id) => document.getElementById(id);
const isTauri = '__TAURI_INTERNALS__' in window;

const settings = loadSettings();
const engine = new AudioEngine();
const graph = new PitchGraph($('graph'));
const strip = new NoteStrip($('piano'));
const ai = new AICoach();

let target = makeTarget(settings.goal, settings.custom);
let frames = [];
let lastStats = null;
let lastRecording = null;
let aiBusy = false;

/* ---------- theme ---------- */

const ICONS = {
  light: '<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"><circle cx="12" cy="12" r="4"/><path d="M12 3v1.5M12 19.5V21M3 12h1.5M19.5 12H21M5.6 5.6l1.1 1.1M17.3 17.3l1.1 1.1M5.6 18.4l1.1-1.1M17.3 6.7l1.1-1.1"/></svg>',
  dark: '<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z"/></svg>',
};

const systemDark = matchMedia('(prefers-color-scheme: dark)');
const effectiveTheme = () =>
  settings.theme === 'system' ? (systemDark.matches ? 'dark' : 'light') : settings.theme;

function applyTheme() {
  if (settings.theme === 'system') delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = settings.theme;
  $('themeBtn').innerHTML = effectiveTheme() === 'dark' ? ICONS.dark : ICONS.light;
  setPressed($('themeSeg'), 'theme', settings.theme);
  requestAnimationFrame(() => graph.refreshColors());
}
systemDark.addEventListener('change', applyTheme);

$('themeBtn').onclick = () => {
  settings.theme = effectiveTheme() === 'dark' ? 'light' : 'dark';
  saveSettings(settings);
  applyTheme();
};
$('themeSeg').onclick = (e) => {
  const b = e.target.closest('button');
  if (!b) return;
  settings.theme = b.dataset.theme;
  saveSettings(settings);
  applyTheme();
};

function setPressed(group, attr, value) {
  for (const b of group.querySelectorAll('button')) {
    b.setAttribute('aria-checked', String(b.dataset[attr] === value));
  }
}

/* ---------- goal / target ---------- */

function applyTarget() {
  target = makeTarget(settings.goal, settings.custom);
  setPressed($('goalSeg'), 'goal', settings.goal);
  $('customRange').hidden = settings.goal !== 'custom';
  $('customMin').value = settings.custom.minHz;
  $('customMax').value = settings.custom.maxHz;
  $('customNotes').textContent = `(${hzToNote(target.minHz).name} to ${hzToNote(target.maxHz).name})`;
  $('targetText').textContent =
    `Goal: ${target.label.toLowerCase()} range, ${Math.round(target.minHz)} to ${Math.round(target.maxHz)} Hz ` +
    `(${hzToNote(target.minHz).name} to ${hzToNote(target.maxHz).name}).`;
  graph.setTarget(target);
  strip.setTarget(target);
  const zone = $('resZone');
  if (target.resonance) {
    zone.hidden = false;
    zone.style.left = target.resonance.min * 100 + '%';
    zone.style.width = (target.resonance.max - target.resonance.min) * 100 + '%';
  } else {
    zone.hidden = true;
  }
}

$('goalSeg').onclick = (e) => {
  const b = e.target.closest('button');
  if (!b) return;
  settings.goal = b.dataset.goal;
  saveSettings(settings);
  applyTarget();
};
for (const id of ['customMin', 'customMax']) {
  $(id).onchange = () => {
    const lo = Number($('customMin').value);
    const hi = Number($('customMax').value);
    if (lo > 0 && hi > 0) {
      settings.custom = { minHz: lo, maxHz: hi };
      saveSettings(settings);
      applyTarget();
    }
  };
}

/* ---------- microphones ---------- */

async function refreshMics() {
  const sel = $('micSelect');
  const mics = await AudioEngine.listInputs().catch(() => []);
  sel.innerHTML = '<option value="">Default microphone</option>';
  mics.forEach((m, i) => {
    if (!m.deviceId || m.deviceId === 'default') return;
    const o = document.createElement('option');
    o.value = m.deviceId;
    o.textContent = m.label || `Microphone ${i + 1}`;
    sel.appendChild(o);
  });
  sel.value = [...sel.options].some((o) => o.value === settings.micId) ? settings.micId : '';
}
navigator.mediaDevices?.addEventListener?.('devicechange', refreshMics);

$('micSelect').onchange = () => {
  settings.micId = $('micSelect').value;
  saveSettings(settings);
};

/* ---------- live loop ---------- */

const chip = $('statusChip');
function setChip(text, cls = '') {
  chip.textContent = text;
  chip.className = 'chip' + (cls ? ' ' + cls : '');
}

engine.onFrame = (f) => {
  frames.push({ t: f.t, hz: f.hz, res: f.res });
  graph.push(f.t, f.hz);
  graph.draw();

  $('levelFill').style.width = Math.max(0, Math.min(100, ((f.db + 70) / 60) * 100)) + '%';

  $('readout').classList.toggle('stale', !f.hz);
  if (f.hz) {
    const n = hzToNote(f.hz);
    $('hzValue').textContent = Math.round(f.hz);
    $('noteName').textContent = n.name;
    $('noteCents').textContent = formatCents(n.cents);
    strip.setNote(n.midi);
    if (f.hz < target.minHz) setChip('A little low', 'low');
    else if (f.hz > target.maxHz) setChip('A little high', 'high');
    else setChip('In your range', 'good');
  } else {
    setChip('Listening…');
    strip.setNote(null);
  }

  const marker = $('resMarker');
  if (f.res != null) {
    marker.style.left = f.res * 100 + '%';
    marker.classList.add('active');
  } else {
    marker.classList.remove('active');
  }
  if (f.formants) $('formantText').textContent = `F1 ${f.formants.f1} Hz · F2 ${f.formants.f2} Hz`;
};

function showMicError(err) {
  console.error(err);
  const name = err?.name || '';
  setChip(name === 'NotAllowedError' ? 'Microphone blocked' : 'Microphone error', 'high');
  $('targetText').textContent =
    name === 'NotAllowedError'
      ? 'OnicVoco needs microphone permission. Allow it in your system privacy settings, then try again.'
      : `Could not open the microphone (${err?.message || name}).`;
}

async function start() {
  frames = [];
  graph.clear();
  try {
    await engine.start(settings.micId, { record: settings.record });
  } catch (err) {
    showMicError(err);
    return;
  }
  engine.gateDb = settings.gateDb;
  await refreshMics(); // labels become visible after permission is granted
  $('micSelect').disabled = true;
  $('startBtn').classList.add('running');
  $('startBtn').querySelector('.label').textContent = 'Finish session';
  setChip('Listening…');
}

async function stop() {
  const blob = await engine.stop();
  $('micSelect').disabled = false;
  $('startBtn').classList.remove('running');
  $('startBtn').querySelector('.label').textContent = 'Start practice';
  setChip('Ready');
  $('hzValue').textContent = '--';
  $('noteName').textContent = '--';
  $('noteCents').textContent = '';
  $('readout').classList.remove('stale');
  $('levelFill').style.width = '0%';
  $('resMarker').classList.remove('active');
  strip.setNote(null);
  showSummary(blob);
}

$('startBtn').onclick = () => (engine.running ? stop() : start());

/* ---------- summary ---------- */

function stat(value, label) {
  return `<div class="stat"><b>${value}</b><span>${label}</span></div>`;
}

function showSummary(blob) {
  const stats = computeStats(frames, target);
  lastStats = stats;
  $('summaryCard').hidden = false;
  $('summaryWhen').textContent = new Date().toLocaleString();

  if (stats.enough) {
    const resWord = stats.resonance == null ? '--' : stats.resonance < 0.35 ? 'darker' : stats.resonance < 0.6 ? 'balanced' : 'brighter';
    $('statsGrid').innerHTML = [
      stat(`${Math.round(stats.medianHz)} Hz`, `median pitch · ${stats.medianNote.name}`),
      stat(`${stats.lowNote}–${stats.highNote}`, `typical range · ${stats.rangeSemitones.toFixed(1)} semitones`),
      stat(`${Math.round(stats.inTarget * 100)}%`, 'time in goal range'),
      stat(resWord, 'resonance'),
      stat(`${Math.round(stats.voicedSeconds)} s`, 'voiced time'),
    ].join('');
    addHistory({
      at: Date.now(),
      goal: target.label,
      medianHz: Math.round(stats.medianHz),
      note: stats.medianNote.name,
      inTarget: stats.inTarget,
      resonance: stats.resonance,
      seconds: Math.round(stats.voicedSeconds),
    });
    renderHistory();
  } else {
    $('statsGrid').innerHTML = '';
  }

  $('tipsList').innerHTML = basicFeedback(stats, target)
    .map((t) => `<li>${escapeHtml(t)}</li>`)
    .join('');

  $('aiText').textContent = '';
  $('aiBox').hidden = !(settings.aiEnabled && ai.downloaded && stats.enough);

  if (lastRecording) URL.revokeObjectURL(lastRecording.url);
  lastRecording = blob && blob.size ? { blob, url: URL.createObjectURL(blob) } : null;
  $('playback').hidden = !lastRecording;
  if (lastRecording) $('audioEl').src = lastRecording.url;

  $('summaryCard').scrollIntoView({ behavior: 'smooth', block: 'start' });
}

$('aiAskBtn').onclick = async () => {
  if (aiBusy || !lastStats?.enough) return;
  aiBusy = true;
  const btn = $('aiAskBtn');
  btn.disabled = true;
  btn.textContent = 'Thinking…';
  const out = $('aiText');
  out.textContent = '';
  ai.onToken = (t) => (out.textContent += t.replace(/—/g, ' - '));
  try {
    const text = await ai.feedback(COACH_SYSTEM_PROMPT, buildCoachPrompt(lastStats, target));
    out.textContent = text.replace(/—/g, ' - ').trim();
  } catch (err) {
    out.textContent = `The AI coach couldn't run: ${err.message}`;
  } finally {
    aiBusy = false;
    btn.disabled = false;
    btn.textContent = 'Ask again';
  }
};

$('saveRecBtn').onclick = async () => {
  if (!lastRecording) return;
  const ext = lastRecording.blob.type.includes('mp4') ? 'm4a' : lastRecording.blob.type.includes('ogg') ? 'ogg' : 'webm';
  const name = `onicvoco-${new Date().toISOString().slice(0, 16).replace(/[:T]/g, '-')}.${ext}`;
  if (isTauri) {
    const { save } = await import('@tauri-apps/plugin-dialog');
    const { writeFile } = await import('@tauri-apps/plugin-fs');
    const path = await save({ defaultPath: name, filters: [{ name: 'Audio', extensions: [ext] }] });
    if (!path) return;
    await writeFile(path, new Uint8Array(await lastRecording.blob.arrayBuffer()));
  } else {
    const a = document.createElement('a');
    a.href = lastRecording.url;
    a.download = name;
    a.click();
  }
};

/* ---------- history ---------- */

function renderHistory() {
  const h = loadHistory();
  const list = $('historyList');
  if (!h.length) {
    list.innerHTML = '<p class="empty">Finish a practice session and your progress will show up here.</p>';
    return;
  }
  list.innerHTML = h
    .slice(0, 30)
    .map((e) => {
      const d = new Date(e.at);
      const res = e.resonance == null ? '--' : e.resonance < 0.35 ? 'darker' : e.resonance < 0.6 ? 'balanced' : 'brighter';
      return `<div class="hist-row">
        <div><b>${d.toLocaleDateString(undefined, { day: 'numeric', month: 'short' })}</b>
          <span class="muted"> ${d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })} · ${escapeHtml(e.goal)}</span></div>
        <div><b>${e.medianHz} Hz</b> <span class="muted">${escapeHtml(e.note)}</span></div>
        <div>${Math.round(e.inTarget * 100)}% in range<div class="hist-bar"><div style="width:${Math.round(e.inTarget * 100)}%"></div></div></div>
        <div class="muted">${res} · ${e.seconds}s</div>
      </div>`;
    })
    .join('');
}

/* ---------- settings ---------- */

$('settingsBtn').onclick = () => {
  syncAiUi();
  $('settingsDialog').showModal();
};
$('settingsDialog').addEventListener('click', (e) => {
  if (e.target === $('settingsDialog')) $('settingsDialog').close();
});

$('gateSlider').value = settings.gateDb;
$('gateValue').textContent = `(${settings.gateDb} dB)`;
$('gateSlider').oninput = () => {
  settings.gateDb = Number($('gateSlider').value);
  engine.gateDb = settings.gateDb;
  $('gateValue').textContent = `(${settings.gateDb} dB)`;
  saveSettings(settings);
};

$('recordToggle').checked = settings.record;
$('recordToggle').onchange = () => {
  settings.record = $('recordToggle').checked;
  saveSettings(settings);
};

$('clearHistoryBtn').onclick = () => {
  if (confirm('Clear all saved progress? This cannot be undone.')) {
    clearHistory();
    renderHistory();
  }
};

/* ---------- AI coach setup ---------- */

function syncAiUi(status) {
  const ready = settings.aiEnabled && ai.downloaded;
  $('aiDownloadBtn').hidden = ready;
  $('aiDeleteBtn').hidden = !ready;
  $('aiStatus').textContent = status ?? (ready ? 'AI coach is installed and ready.' : 'Not installed.');
}

$('aiDownloadBtn').onclick = async () => {
  const btn = $('aiDownloadBtn');
  btn.disabled = true;
  $('aiProgress').hidden = false;
  ai.onProgress = (frac, label) => {
    $('aiProgressFill').style.width = Math.round(frac * 100) + '%';
    $('aiStatus').textContent = `Downloading… ${label}`;
  };
  try {
    syncAiUi('Starting download…');
    await ai.load();
    settings.aiEnabled = true;
    saveSettings(settings);
    syncAiUi('AI coach is installed and ready.');
    if (lastStats?.enough && !$('summaryCard').hidden) $('aiBox').hidden = false;
  } catch (err) {
    syncAiUi(`Download failed: ${err.message}. Check your internet connection and try again.`);
  } finally {
    btn.disabled = false;
    $('aiProgress').hidden = true;
  }
};

$('aiDeleteBtn').onclick = async () => {
  await ai.remove();
  settings.aiEnabled = false;
  saveSettings(settings);
  $('aiBox').hidden = true;
  syncAiUi('Model removed from this computer.');
};

/* ---------- helpers & init ---------- */

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
}

applyTheme();
applyTarget();
renderHistory();
refreshMics();
graph.draw();
