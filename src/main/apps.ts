import { nativeImage } from 'electron';
import { execFile } from 'node:child_process';
import { existsSync, readdirSync, statSync } from 'node:fs';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { homedir, tmpdir } from 'node:os';
import { basename, extname, join } from 'node:path';
import { promisify } from 'node:util';
import type { WheelItem } from '../shared/settings';
import { fallbackIcon } from '../shared/icons';

const APP_DIRS = [
  '/Applications',
  '/System/Applications',
  '/System/Applications/Utilities',
  '/Applications/Utilities',
  join(homedir(), 'Applications'),
];

export interface InstalledApp {
  name: string;
  path: string;
}

/** Every .app bundle in the usual places, one per name, in a stable alphabetical order. */
export function installedApps(): InstalledApp[] {
  const byName = new Map<string, string>();
  if (existsSync('/System/Library/CoreServices/Finder.app')) byName.set('Finder', '/System/Library/CoreServices/Finder.app');
  for (const dir of APP_DIRS) {
    let entries: string[];
    try {
      entries = readdirSync(dir);
    } catch {
      continue;
    }
    for (const entry of entries) {
      if (!entry.endsWith('.app')) continue;
      const name = basename(entry, '.app');
      if (!byName.has(name)) byName.set(name, join(dir, entry));
    }
  }
  return [...byName.entries()]
    .map(([name, path]) => ({ name, path }))
    .sort((a, b) => a.name.localeCompare(b.name, 'zh-Hans-CN'));
}

const icons = new Map<string, string>();
const run = promisify(execFile);

/** The bundle's icon as a PNG data URL. Empty when macOS hands us nothing, so the ring can fall back to a letter. */
export async function appIcon(path: string): Promise<string> {
  const cached = icons.get(path);
  if (cached !== undefined) return cached;
  let url = '';
  let temporary: string | undefined;
  try {
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
  return paths.map((path) => path && (existsSync(path) || path.startsWith('shortcuts://')) ? {
      name: names[path] || (path.startsWith('shortcuts://') ? new URL(path).searchParams.get('name') ?? '快捷指令' : basename(path, '.app')),
      path, icon: icons.get(path) || fallbackIcon(path.startsWith('shortcuts://')),
    } : { name: '', path: '', icon: '' });
}

export async function wheelItems(paths: readonly string[], names: Record<string, string> = {}): Promise<WheelItem[]> {
  return Promise.all(wheelSkeleton(paths, names).map(async (item) => ({ ...item,
    icon: item.path && !item.path.startsWith('shortcuts://') ? await appIcon(item.path) || fallbackIcon(false) : item.path ? fallbackIcon(true) : '',
  })));
}

/** Hands the bundle to LaunchServices: the app comes forward, or starts if it was not running. */
export function launchApp(path: string, failed?: (message: string) => void): void {
  if (!path) return;
  if (path.startsWith('shortcuts://run-shortcut?name=')) {
    const name = new URL(path).searchParams.get('name');
    if (name) execFile('/usr/bin/shortcuts', ['run', name], (err) => {
      if (err) { console.error('Shortcut failed:', err.message); failed?.(`“${name}”未能完成，请在「快捷指令」中检查运行条件。`); }
    });
    return;
  }
  execFile('open', [path], (err) => {
    if (err) { console.error(`open ${path} failed: ${err.message}`); failed?.('无法打开这个项目，请检查它是否仍然存在。'); }
  });
}

/** Catalog only: reading shortcut names never runs a workflow. */
export async function availableShortcuts(): Promise<InstalledApp[]> {
  try {
    const { stdout } = await run('/usr/bin/shortcuts', ['list'], { timeout: 5000 });
    return stdout.split('\n').map((s) => s.trim()).filter(Boolean)
      .map((name) => ({ name, path: `shortcuts://run-shortcut?name=${encodeURIComponent(name)}` }));
  } catch { return []; }
}
