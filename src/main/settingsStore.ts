import { app } from 'electron';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { DEFAULT_SETTINGS, sanitize, type Settings } from '../shared/settings';

const file = (): string => join(app.getPath('userData'), 'settings.json');

export function loadSettings(): Settings {
  try {
    return sanitize(JSON.parse(readFileSync(file(), 'utf8')));
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

export function saveSettings(s: Settings): void {
  mkdirSync(dirname(file()), { recursive: true });
  writeFileSync(file(), `${JSON.stringify(s, null, 2)}\n`, 'utf8');
}
