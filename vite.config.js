import { defineConfig } from 'vite';
import { existsSync, readdirSync } from 'node:fs';

// scripts/copy-ort.js copies ONNX Runtime's wasm files into public/ort so the
// AI coach works offline once the model is downloaded.
const ortLocal = existsSync('public/ort') && readdirSync('public/ort').some((f) => f.endsWith('.wasm'));

export default defineConfig({
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  define: {
    __ORT_LOCAL__: JSON.stringify(ortLocal),
  },
  worker: { format: 'es' },
  optimizeDeps: { exclude: ['@huggingface/transformers'] },
  build: {
    target: ['es2022', 'safari15', 'chrome105'],
    sourcemap: false,
    chunkSizeWarningLimit: 2000,
  },
});
