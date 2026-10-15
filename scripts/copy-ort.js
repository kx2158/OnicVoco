// Copies ONNX Runtime Web's wasm runtime files into public/ort so they ship
// inside the app instead of being fetched from a CDN at runtime.
import { existsSync, mkdirSync, readdirSync, copyFileSync } from 'node:fs';
import { join } from 'node:path';

const candidates = [
  'node_modules/onnxruntime-web/dist',
  'node_modules/@huggingface/transformers/node_modules/onnxruntime-web/dist',
];
const src = candidates.find((p) => existsSync(p));
if (!src) {
  console.warn('[copy-ort] onnxruntime-web not found; the AI coach will load its runtime from the CDN.');
  process.exit(0);
}

const out = 'public/ort';
mkdirSync(out, { recursive: true });
let n = 0;
for (const f of readdirSync(src)) {
  // transformers.js uses the WebGPU/JSEP build of ONNX Runtime; only that pair is needed.
  if (/^ort-wasm-simd-threaded\.jsep\.(wasm|mjs)$/.test(f)) {
    copyFileSync(join(src, f), join(out, f));
    n++;
  }
}
console.log(`[copy-ort] copied ${n} files from ${src}`);
