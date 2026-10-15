// Where downloaded model files live.
// Desktop app: the app's data folder (via the Tauri fs plugin), so the model
// survives restarts and can be removed cleanly.
// Plain browser (npm run dev): the browser Cache Storage.

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
const DIR = 'models';
const CACHE_NAME = 'onicvoco-models';

const fileName = (key) =>
  key.replace(/^https?:\/\//, '').replace(/[^a-zA-Z0-9._-]/g, '_').slice(-180);

let fsMod = null;
async function fs() {
  fsMod ??= await import('@tauri-apps/plugin-fs');
  return fsMod;
}

export async function storeGet(key) {
  if (isTauri) {
    const { exists, readFile, BaseDirectory } = await fs();
    const path = `${DIR}/${fileName(key)}`;
    if (!(await exists(path, { baseDir: BaseDirectory.AppData }))) return null;
    const bytes = await readFile(path, { baseDir: BaseDirectory.AppData });
    return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
  }
  const cache = await caches.open(CACHE_NAME);
  const res = await cache.match(key);
  return res ? res.arrayBuffer() : null;
}

export async function storePut(key, buffer) {
  if (isTauri) {
    const { mkdir, writeFile, BaseDirectory } = await fs();
    await mkdir(DIR, { baseDir: BaseDirectory.AppData, recursive: true });
    await writeFile(`${DIR}/${fileName(key)}`, new Uint8Array(buffer), { baseDir: BaseDirectory.AppData });
    return;
  }
  const cache = await caches.open(CACHE_NAME);
  await cache.put(key, new Response(buffer));
}

export async function storeClear() {
  if (isTauri) {
    const { exists, remove, BaseDirectory } = await fs();
    if (await exists(DIR, { baseDir: BaseDirectory.AppData })) {
      await remove(DIR, { baseDir: BaseDirectory.AppData, recursive: true });
    }
    return;
  }
  await caches.delete(CACHE_NAME);
}
