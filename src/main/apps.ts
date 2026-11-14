import { app, nativeImage } from 'electron';
import { execFile } from 'node:child_process';
import { existsSync, statSync } from 'node:fs';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, extname, join } from 'node:path';
import { promisify } from 'node:util';
import type { WheelItem } from '../shared/settings';
import { fallbackIcon } from '../shared/icons';

export interface InstalledApp {
  name: string;
  path: string;
  aliases?: string[];
  browser?: boolean;
}

/** Bundle metadata supplies localized names; containers are scanned without entering app bundles. */
export async function installedApps(): Promise<InstalledApp[]> {
  const binary = app.isPackaged ? join(process.resourcesPath, 'system-helper')
    : join(app.getAppPath(), 'native/system-helper/build/system-helper');
  const { stdout } = await run(binary, ['apps'], { timeout: 8000, maxBuffer: 4 * 1024 * 1024 });
  const apps = JSON.parse(stdout).apps as InstalledApp[];
  for (const item of apps) { appNames.set(item.path, item.name); if (item.browser) browsers.add(item.path); }
  return apps.sort((a, b) => a.name.localeCompare(b.name, 'zh-Hans-CN'));
}

const icons = new Map<string, string>();
const appNames = new Map<string, string>();
const browsers = new Set<string>();
const run = promisify(execFile);

const iconRequests = new Map<string, { promise: Promise<string>; resolve: (image: string) => void }>();
const iconQueue = new Set<string>();
let iconBatchRunning = false;
let iconTimer: NodeJS.Timeout | null = null;
function requestNativeIcon(path: string): Promise<string> {
  const existing = iconRequests.get(path); if (existing) return existing.promise;
  let resolve!: (image: string) => void;
  const promise = new Promise<string>((done) => { resolve = done; });
  iconRequests.set(path, { promise, resolve }); iconQueue.add(path); scheduleIcons(); return promise;
}
function scheduleIcons(): void {
  if (iconTimer || iconBatchRunning) return;
  iconTimer = setTimeout(() => { iconTimer = null; void flushIcons(); }, 12);
}
async function flushIcons(): Promise<void> {
  if (iconBatchRunning || !iconQueue.size) return;
  iconBatchRunning = true;
  const paths = [...iconQueue].slice(0, 32); paths.forEach((path) => iconQueue.delete(path));
  let images: Record<string, string> = {};
  try {
    const binary = app.isPackaged ? join(process.resourcesPath, 'system-helper') : join(app.getAppPath(), 'native/system-helper/build/system-helper');
    const { stdout } = await run(binary, ['icons', ...paths], { timeout: 7000, maxBuffer: 8 * 1024 * 1024 });
    images = JSON.parse(stdout).images ?? {};
  } catch { /* Individual resource decoding remains available below. */ }
  finally {
    for (const path of paths) { iconRequests.get(path)?.resolve(images[path] ?? ''); iconRequests.delete(path); }
    iconBatchRunning = false; if (iconQueue.size) scheduleIcons();
  }
}

/** The bundle's icon as a PNG data URL. Empty when macOS hands us nothing, so the ring can fall back to a letter. */
export async function appIcon(path: string): Promise<string> {
  const cached = icons.get(path);
  if (cached !== undefined) return cached;
  if (app.isPackaged && path === app.getPath('exe').split('/Contents/')[0]) {
    const own = nativeImage.createFromPath(join(process.resourcesPath, 'appIcon.png'));
    if (!own.isEmpty()) { const url = own.resize({ width: 128, height: 128 }).toDataURL(); icons.set(path, url); return url; }
  }
  let url = '';
  let temporary: string | undefined;
  try {
    const image = await requestNativeIcon(path);
    if (image.startsWith('data:image/png;base64,')) { icons.set(path, image); return image; }
    // getFileIcon's background NSImage work crashes natively on macOS 27; a JS catch cannot recover.
    // Read the bundle's declared icon and let the system image converter decode ICNS outside Electron.
    let source: string;
    if (path.endsWith('.app')) {
      const { stdout } = await run('/usr/bin/plutil', [
        '-extract', 'CFBundleIconFile', 'raw', '-o', '-', join(path, 'Contents', 'Info.plist'),
      ], { timeout: 5000 });
      const name = basename(stdout.trim());
      if (!name) return '';
      source = join(path, 'Contents', 'Resources', extname(name) ? name : `${name}.icns`);
    } else {
      source = join('/System/Library/CoreServices/CoreTypes.bundle/Contents/Resources',
        statSync(path).isDirectory() ? 'GenericFolderIcon.icns' : 'GenericDocumentIcon.icns');
    }
    temporary = await mkdtemp(join(tmpdir(), 'vigo-icon-'));
    const png = join(temporary, 'icon.png');
    await run('/usr/bin/sips', ['-s', 'format', 'png', '-Z', '128', source, '--out', png], { timeout: 5000 });
    const icon = nativeImage.createFromBuffer(await readFile(png));
    if (!icon.isEmpty()) {
      const resized = icon.resize({ width: 128, height: 128, quality: 'best' });
      url = resized.toDataURL();
    }
  } catch {
    url = '';
  } finally {
    if (temporary) await rm(temporary, { recursive: true, force: true }).catch(() => undefined);
  }
  icons.set(path, url);
  return url;
}

/** The ring's contents for the paths in settings, in order, skipping anything uninstalled since. */
export function wheelSkeleton(paths: readonly string[], names: Record<string, string> = {}): WheelItem[] {
  return paths.map((path) => path && existsSync(path) ? {
      name: names[path] || appNames.get(path) || basename(path, '.app'),
      path, icon: icons.get(path) || fallbackIcon(),
    } : { name: '', path: '', icon: '' });
}

export async function wheelItems(paths: readonly string[], names: Record<string, string> = {}): Promise<WheelItem[]> {
  return Promise.all(wheelSkeleton(paths, names).map(async (item) => ({ ...item,
    icon: item.path ? await appIcon(item.path) || fallbackIcon() : '',
  })));
}

/** Hands the bundle to LaunchServices: the app comes forward, or starts if it was not running. */
export function launchApp(path: string, failed?: (message: string) => void): void {
  if (!path) return;
  execFile('open', [path], (err) => {
    if (err) { console.error(`open ${path} failed: ${err.message}`); failed?.('无法打开这个项目，请检查它是否仍然存在。'); }
  });
}

/** URLs under a browser use that browser; other owners use the system default. */
export function launchContent(owner: string, target: string, failed: (message: string) => void): void {
  const args = /^https?:\/\//.test(target) && browsers.has(owner) ? ['-a', owner, target] : [target];
  execFile('/usr/bin/open', args, (error) => { if (error) failed('无法打开这个外环项目，请检查路径或网址。'); });
}
