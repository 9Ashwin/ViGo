import type { BrowserTab } from '../shared/content';
import { app } from 'electron';
import { execFile, spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { join } from 'node:path';
import { promisify } from 'node:util';
import type { WindowItem, WindowState, PreviewFrame } from '../shared/windows';
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
export function watchWindowPreview(item: WindowItem, frame: (value: PreviewFrame) => void): () => void {
  const cached = previews.get(item.id) ?? '';
  frame({ image: cached, cached: Boolean(cached), state: cached ? 'cached' : item.minimized ? 'unavailable' : 'loading',
    message: item.minimized && !cached ? '窗口已最小化，暂无缓存画面' : undefined });
  if (!item.windowId || item.minimized) return () => undefined;
  let stopped = false, reported = false;
  let child: ReturnType<typeof spawn> | null = null;
  let firstFrame: NodeJS.Timeout | null = null;
  const unavailable = (message = '这个窗口暂时无法预览') => {
    if (stopped) return;
    reported = true;
    const last = previews.get(item.id) || cached;
    frame({ image: last, cached: Boolean(last), state: last ? 'cached' : 'unavailable', message });
  };
  // A short hover delay avoids spawning capture processes while crossing unrelated windows.
  const begin = setTimeout(() => {
    if (stopped) return;
    child = spawn(binary(), ['stream', String(item.windowId), String(item.pid)], { stdio: ['pipe', 'pipe', 'ignore'] });
    child.on('error', () => unavailable()); child.stdin?.on('error', () => undefined);
    firstFrame = setTimeout(() => { if (!reported) unavailable('画面暂未就绪，请稍后重试'); }, 1600);
    firstFrame.unref();
    createInterface({ input: child.stdout! }).on('line', (line) => {
      if (stopped) return;
      try {
        const value = JSON.parse(line) as { ok: boolean; image?: string; reason?: string };
        if (value.ok && value.image && /^data:image\/(png|jpeg);base64,/.test(value.image)) {
          reported = true; if (firstFrame) clearTimeout(firstFrame);
          previews.delete(item.id); previews.set(item.id, value.image);
          while (previews.size > 32) previews.delete(previews.keys().next().value as string);
          frame({ image: value.image, cached: false, state: 'live' });
        } else if (value.reason) unavailable(value.reason === 'permission' ? '需要开启屏幕录制权限' : value.reason === 'closed' ? '窗口已关闭' : '这个窗口暂时无法预览');
      } catch { /* Invalid output cannot replace a valid preview. */ }
    });
    child.once('exit', () => { if (firstFrame) clearTimeout(firstFrame); if (!stopped) unavailable('捕获已结束，显示上次画面'); });
  }, 130);
  return () => {
    stopped = true; clearTimeout(begin); if (firstFrame) clearTimeout(firstFrame);
    if (!child) return;
    const active = child; active.stdin?.end();
    const timer = setTimeout(() => { if (active.exitCode === null && active.signalCode === null) active.kill('SIGKILL'); }, 500);
    timer.unref(); active.once('exit', () => clearTimeout(timer));
  };
}
export async function listBrowserTabs(paths: string[]): Promise<BrowserTab[]> {
  try {
    const { stdout } = await run(binary(), ['tabs', ...paths.filter((path) => path.endsWith('.app'))], { timeout: 2200, maxBuffer: 1024 * 1024 });
    return JSON.parse(stdout).tabs ?? [];
  } catch { return []; }
}
export async function activateBrowserTab(tab: BrowserTab): Promise<boolean> {
  try {
    const { stdout } = await run(binary(), ['activate-tab', JSON.stringify(tab)], { timeout: 2500 });
    return JSON.parse(stdout).ok === true;
  } catch { return false; }
}
