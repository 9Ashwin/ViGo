import { app } from 'electron';
import { execFile, spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { join } from 'node:path';
import { promisify } from 'node:util';
import type { WindowItem, WindowState } from '../shared/windows';
const run = promisify(execFile);
const binary = (): string => app.isPackaged ? join(process.resourcesPath, 'system-helper') : join(app.getAppPath(), 'native/system-helper/build/system-helper');
const empty = (): WindowState => ({ accessibility: false, screen: false, running: [], windows: [] });
let lastWindowState = empty();
export async function listWindows(paths: string[]): Promise<WindowState> {
  try {
    const { stdout } = await run(binary(), ['list', ...paths.filter((path) => path.endsWith('.app'))], { timeout: 3500, maxBuffer: 1024 * 1024 });
    const value = JSON.parse(stdout) as WindowState;
    if (!Array.isArray(value.windows) || !Array.isArray(value.running)) return lastWindowState;
    lastWindowState = value;
    return value;
  } catch { return lastWindowState; }
}
export async function activateWindow(item: WindowItem): Promise<boolean> {
  try {
    const { stdout } = await run(binary(), ['activate', JSON.stringify(item)], { timeout: 2500 });
    return JSON.parse(stdout).ok === true;
  } catch { return false; }
}
const previews = new Map<string, string>();
export function watchWindowPreview(item: WindowItem, frame: (value: { image: string; cached: boolean }) => void): () => void {
  frame({ image: previews.get(item.id) ?? '', cached: previews.has(item.id) });
  if (!item.windowId) return () => undefined;
  const child = spawn(binary(), ['stream', String(item.windowId), String(item.pid)], { stdio: ['pipe', 'pipe', 'ignore'] });
  let stopped = false;
  child.on('error', () => undefined);
  child.stdin.on('error', () => undefined);
  createInterface({ input: child.stdout }).on('line', (line) => {
    if (stopped) return;
    try {
      const value = JSON.parse(line) as { ok: boolean; image?: string };
      if (value.ok && value.image?.startsWith('data:image/png;base64,')) {
        previews.set(item.id, value.image);
        while (previews.size > 32) previews.delete(previews.keys().next().value as string);
        frame({ image: value.image, cached: false });
      }
    } catch { /* Incomplete/invalid helper output cannot replace a valid preview. */ }
  });
  return () => {
    stopped = true;
    child.stdin.end();
    const timer = setTimeout(() => { if (child.exitCode === null) child.kill('SIGKILL'); }, 500);
    timer.unref();
    child.once('exit', () => clearTimeout(timer));
  };
}
export async function requestScreenPreview(): Promise<void> {
  await run(binary(), ['request-screen'], { timeout: 10000 }).catch(() => undefined);
}
