// Runs the optional AI coach off the main thread so the UI stays smooth.
// Model files are stored by the main thread (see model-store.js); this worker
// asks for them through messages via transformers.js' custom cache hook.

import { pipeline, env, TextStreamer } from '@huggingface/transformers';

const MODEL_ID = 'onnx-community/Qwen2.5-0.5B-Instruct';

env.allowLocalModels = false;
env.useBrowserCache = false;
env.useCustomCache = true;

// Use the ONNX Runtime files bundled with the app when available (works offline).
// eslint-disable-next-line no-undef
if (typeof __ORT_LOCAL__ !== 'undefined' && __ORT_LOCAL__) {
  env.backends.onnx.wasm.wasmPaths = new URL('/ort/', self.location.href).href;
}
env.backends.onnx.wasm.proxy = false;

let nextId = 0;
const pending = new Map();

function askMain(op, payload, transfer = []) {
  return new Promise((resolve, reject) => {
    const id = ++nextId;
    pending.set(id, { resolve, reject });
    self.postMessage({ kind: 'store', id, op, ...payload }, transfer);
  });
}

env.customCache = {
  async match(key) {
    const k = typeof key === 'string' ? key : key.url;
    const buf = await askMain('get', { key: k });
    return buf ? new Response(buf) : undefined;
  },
  async put(key, response) {
    const k = typeof key === 'string' ? key : key.url;
    const buf = await response.arrayBuffer();
    await askMain('put', { key: k, buf }, [buf]);
  },
};

let generator = null;

async function load() {
  if (generator) return generator;
  generator = await pipeline('text-generation', MODEL_ID, {
    dtype: 'q8',
    device: 'wasm',
    progress_callback: (p) => self.postMessage({ kind: 'progress', p }),
  });
  return generator;
}

async function generate(system, user) {
  const gen = await load();
  const streamer = new TextStreamer(gen.tokenizer, {
    skip_prompt: true,
    skip_special_tokens: true,
    callback_function: (text) => self.postMessage({ kind: 'token', text }),
  });
  const messages = [
    { role: 'system', content: system },
    { role: 'user', content: user },
  ];
  const out = await gen(messages, {
    max_new_tokens: 200,
    do_sample: false,
    repetition_penalty: 1.1,
    streamer,
  });
  const last = out[0].generated_text.at(-1);
  return typeof last === 'string' ? last : last.content;
}

self.onmessage = async (e) => {
  const msg = e.data;
  if (msg.kind === 'store-reply') {
    const p = pending.get(msg.id);
    pending.delete(msg.id);
    if (!p) return;
    msg.error ? p.reject(new Error(msg.error)) : p.resolve(msg.buf ?? null);
    return;
  }
  try {
    if (msg.kind === 'load') {
      await load();
      self.postMessage({ kind: 'ready' });
    } else if (msg.kind === 'generate') {
      const text = await generate(msg.system, msg.user);
      self.postMessage({ kind: 'done', text });
    }
  } catch (err) {
    self.postMessage({ kind: 'error', message: String(err?.message || err) });
  }
};
