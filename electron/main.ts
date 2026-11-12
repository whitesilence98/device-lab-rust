import { app, BrowserWindow, ipcMain } from 'electron';
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process';
import { readFile, mkdir, open, rename, unlink } from 'node:fs/promises';
import path from 'node:path';
import { randomUUID } from 'node:crypto';
import { defaults, emptySnapshot, encodePreferences, parsePreferences, parseSnapshot, validPreferences } from './core';
import type { Appearance, Preferences, Snapshot } from './types';

const root = path.resolve(__dirname, '..');
const preferencePath = path.join(root, 'data', 'preferences.txt');
let window: BrowserWindow | null = null;
let child: ChildProcessWithoutNullStreams | null = null;
let inFlight: Promise<Snapshot> | null = null;
let latest: Snapshot = { ...emptySnapshot };
let preferenceSource: string | null = null;
let appearance: Appearance = { preferences: { ...defaults }, error: null };
let saving = false;
let stopping = false;

const message = (error: unknown) => error instanceof Error ? error.message : String(error);
const isMissing = (error: unknown) => (error as NodeJS.ErrnoException).code === 'ENOENT';

async function loadAppearance(): Promise<void> {
  try {
    preferenceSource = await readFile(preferencePath, 'utf8');
    appearance.preferences = parsePreferences(preferenceSource);
  } catch (error) {
    if (!isMissing(error)) appearance.error = `Cannot load appearance: ${message(error)} Original preserved; saving disabled.`;
  }
}

async function saveAppearance(value: unknown): Promise<string> {
  if (!validPreferences(value)) throw new Error('Invalid appearance settings.');
  if (appearance.error) throw new Error(appearance.error);
  if (saving) throw new Error('An appearance save is already in progress.');
  saving = true;
  const temporary = `${preferencePath}.${randomUUID()}.tmp`;
  try {
    let current: string | null = null;
    try { current = await readFile(preferencePath, 'utf8'); }
    catch (error) { if (!isMissing(error)) throw error; }
    if (current !== preferenceSource) {
      appearance.error = 'Preferences changed outside this window; original preserved. Restart before saving.';
      throw new Error(appearance.error);
    }
    const text = encodePreferences(value);
    await mkdir(path.dirname(preferencePath), { recursive: true });
    const file = await open(temporary, 'wx');
    try { await file.writeFile(text, 'utf8'); await file.sync(); }
    finally { await file.close(); }
    await rename(temporary, preferencePath);
    preferenceSource = text;
    appearance.preferences = { light: value.light, scale: value.scale };
    return 'Saved locally. No device inventory is stored.';
  } finally {
    saving = false;
    await unlink(temporary).catch(() => {});
  }
}

function scan(): Promise<Snapshot> {
  if (inFlight) return inFlight;
  if (stopping) return Promise.resolve({ ...latest, error: 'Application is closing.' });
  inFlight = new Promise<Snapshot>((resolve, reject) => {
    if (process.platform !== 'win32') {
      reject(new Error('Device enumeration currently supports Windows only.'));
      return;
    }
    if (!child) {
      child = spawn(path.join(root, 'target', 'debug', 'device-lab.exe'), [], {
        cwd: root, shell: false, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'],
      });
    }
    const helper = child;
    const chunks: Buffer[] = [];
    let bytes = 0;
    let diagnostic = '';
    const timer = setTimeout(() => fail(new Error('Device scan timed out; retry with Refresh.')), 15000);
    function cleanup() {
      clearTimeout(timer);
      helper.stdout.removeListener('data', data);
      helper.stderr.removeListener('data', stderr);
      helper.removeListener('error', fail);
      helper.removeListener('exit', exited);
      helper.stdin.removeListener('error', fail);
    }
    function fail(error: Error) {
      cleanup();
      // Keep a listener through teardown: a failed spawn may also close its pipes.
      helper.on('error', () => {});
      helper.stdin.on('error', () => {});
      if (child === helper) child = null;
      helper.kill();
      reject(error);
    }
    function exited(code: number | null) {
      fail(new Error(`Device helper exited (${code ?? 'signal'}). ${diagnostic}`));
    }
    function stderr(chunk: Buffer) { diagnostic = (diagnostic + chunk.toString('utf8')).slice(-2048); }
    function data(chunk: Buffer) {
      bytes += chunk.length;
      if (bytes > 32 * 1024 * 1024) return fail(new Error('Device helper response exceeded the size limit.'));
      chunks.push(chunk);
      if (!chunk.includes(10)) return;
      try {
        const text = Buffer.concat(chunks).toString('utf8');
        if (!text.endsWith('\n') || text.indexOf('\n') !== text.length - 1) throw new Error('Invalid helper framing.');
        const result = parseSnapshot(text);
        cleanup();
        resolve(result);
      } catch (error) { fail(new Error(message(error))); }
    }
    helper.stdout.on('data', data);
    helper.stderr.on('data', stderr);
    helper.once('error', fail);
    helper.once('exit', exited);
    helper.stdin.once('error', fail);
    helper.stdin.write('scan\n');
  }).then(result => {
    latest = result;
    return latest;
  }).catch(error => {
    const detail = (error as NodeJS.ErrnoException).code === 'ENOENT'
      ? 'Rust device helper is missing. Run npm run build:backend in Developer PowerShell with C++ Build Tools and Windows SDK installed.'
      : message(error);
    latest = { ...latest, error: detail };
    return latest;
  }).finally(() => { inFlight = null; });
  return inFlight;
}

function createWindow(): void {
  window = new BrowserWindow({
    width: 1200, height: 780, minWidth: 760, minHeight: 520,
    title: 'Device Lab — read-only explorer', backgroundColor: '#151819',
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true, nodeIntegration: false, sandbox: true,
      webviewTag: false, webSecurity: true,
    },
  });
  window.removeMenu();
  window.webContents.setWindowOpenHandler(() => ({ action: 'deny' }));
  window.webContents.on('will-navigate', event => event.preventDefault());
  window.webContents.session.setPermissionRequestHandler((_contents, _permission, callback) => callback(false));
  window.webContents.session.setPermissionCheckHandler(() => false);
  window.on('closed', () => { window = null; });
  void window.loadFile(path.join(root, 'dist', 'index.html'));
}

app.whenReady().then(async () => {
  await loadAppearance();
  for (const [channel, handler] of [
    ['devices:scan', () => scan()],
    ['appearance:load', () => appearance],
    ['appearance:save', (value: unknown) => saveAppearance(value)],
  ] as const) {
    ipcMain.handle(channel, (event, value: unknown) => {
      if (!window || event.sender !== window.webContents || event.senderFrame !== window.webContents.mainFrame) {
        throw new Error('Untrusted IPC sender.');
      }
      return handler(value);
    });
  }
  createWindow();
}).catch(error => { console.error(error); app.quit(); });

app.on('window-all-closed', () => app.quit());
app.on('before-quit', () => {
  stopping = true;
  child?.stdin.end();
  child?.kill();
  child = null;
});
