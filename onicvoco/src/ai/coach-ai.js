// Main-thread side of the optional AI coach.

import { storeGet, storePut, storeClear } from './model-store.js';

const READY_KEY = 'onicvoco.ai.ready';

export class AICoach {
  constructor() {
    this.worker = null;
    this.onProgress = null; // (fraction 0..1, label) => void
    this.onToken = null; // (text) => void
    this._waiters = [];
    this._files = new Map();
  }

  get downloaded() {
    try {
      return localStorage.getItem(READY_KEY) === '1';
    } catch {
      return false;
    }
  }

  _ensureWorker() {
    if (this.worker) return;
    this.worker = new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });
    this.worker.onmessage = (e) => this._handle(e.data);
    this.worker.onerror = (e) => this._settle({ kind: 'error', message: e.message || 'AI worker crashed' });
  }

  async _handle(msg) {
    if (msg.kind === 'store') {
      try {
        if (msg.op === 'get') {
          const buf = await storeGet(msg.key);
          this.worker.postMessage({ kind: 'store-reply', id: msg.id, buf }, buf ? [buf] : []);
        } else {
          await storePut(msg.key, msg.buf);
          this.worker.postMessage({ kind: 'store-reply', id: msg.id });
        }
      } catch (err) {
        this.worker.postMessage({ kind: 'store-reply', id: msg.id, error: String(err?.message || err) });
      }
      return;
    }
    if (msg.kind === 'progress') {
      const p = msg.p;
      if (p.status === 'progress' && p.total) {
        this._files.set(p.file, { loaded: p.loaded, total: p.total });
        let loaded = 0;
        let total = 0;
        for (const f of this._files.values()) {
          loaded += f.loaded;
          total += f.total;
        }
        this.onProgress?.(total ? loaded / total : 0, `${(loaded / 1e6).toFixed(0)} / ${(total / 1e6).toFixed(0)} MB`);
      }
      return;
    }
    if (msg.kind === 'token') {
      this.onToken?.(msg.text);
      return;
    }
    this._settle(msg);
  }

  _settle(msg) {
    const w = this._waiters.shift();
    if (!w) return;
    msg.kind === 'error' ? w.reject(new Error(msg.message)) : w.resolve(msg);
  }

  _request(payload) {
    this._ensureWorker();
    return new Promise((resolve, reject) => {
      this._waiters.push({ resolve, reject });
      this.worker.postMessage(payload);
    });
  }

  /** Downloads (first time) and loads the model. */
  async load() {
    this._files.clear();
    await this._request({ kind: 'load' });
    try {
      localStorage.setItem(READY_KEY, '1');
    } catch {}
  }

  async feedback(system, user) {
    const msg = await this._request({ kind: 'generate', system, user });
    return msg.text;
  }

  async remove() {
    this.worker?.terminate();
    this.worker = null;
    this._waiters.forEach((w) => w.reject(new Error('AI coach removed')));
    this._waiters = [];
    await storeClear();
    try {
      localStorage.removeItem(READY_KEY);
    } catch {}
  }
}
