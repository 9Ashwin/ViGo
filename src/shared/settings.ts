import { UNSAFE_TRIGGER_VKS, vkLabel } from './keys.ts';
import type { Platform } from './platform';

export type NamedTrigger = 'alt' | 'ctrl' | 'shift' | 'win' | 'capslock' | 'mouse4' | 'mouse5';
export type TriggerKey = NamedTrigger | { vk: number };
export type EmoteTriggerKey = TriggerKey | 'off';
export const keyboardTriggers = (_platform: Platform): NamedTrigger[] => [];
export const triggerVk = (t: EmoteTriggerKey): number | null => typeof t === 'object' ? t.vk : t === 'capslock' ? 0x14 : null;
export const sameTrigger = (a: EmoteTriggerKey, b: EmoteTriggerKey): boolean =>
  triggerVk(a) !== null && triggerVk(b) !== null ? triggerVk(a) === triggerVk(b) : a === b;
export function triggerLabel(t: TriggerKey, _lang = 'zh', platform: Platform = 'mac'): string {
  if (typeof t === 'object') return vkLabel(t.vk, platform);
  return { alt: '⌥ Option', ctrl: '⌃ Control', shift: '⇧ Shift', win: '⌘ Command', capslock: 'Caps Lock', mouse4: '鼠标侧键 X1', mouse5: '鼠标侧键 X2' }[t];
}

export interface WheelItem { name: string; path: string; icon: string; running?: boolean; windows?: import('./windows').WindowItem[]; }
export interface Settings {
  version: 2;
  trigger: TriggerKey;
  /** Three pages of eight slots, including empty strings for unassigned slots. */
  wheel: string[];
  names: Record<string, string>;
  launchAtLogin: boolean;
  enabledOnStart: boolean;
  wheelSizePx: number;
  windowPreviews: boolean;
  appearance: 'system' | 'light' | 'dark';
}
export const PAGE_SLOTS = 8;
export const MAX_SLOTS = 24;
export const LIMITS = { wheelSizePx: { min: 360, max: 720, step: 20 } };

export const STARTER_APPS = [
  '/Applications/Safari.app',
  '/System/Applications/Mail.app',
  '/Applications/Google Chrome.app',
  '/Applications/Visual Studio Code.app',
  '/System/Applications/Utilities/Terminal.app',
  '/Applications/iTerm.app',
  '/System/Applications/Notes.app',
  '/Applications/Spotify.app',
];


export const DEFAULT_SETTINGS: Settings = {
  version: 2, trigger: { vk: 0x56 }, wheel: Array<string>(8).fill(''), names: {},
  launchAtLogin: false, enabledOnStart: true, wheelSizePx: 520, appearance: 'system', windowPreviews: false,
};

export function sanitize(raw: unknown): Settings {
  const o = typeof raw === 'object' && raw !== null ? raw as Record<string, unknown> : {};
  const source = Array.isArray(o.wheel) ? o.wheel : [];
  const seen = new Set<string>();
  const wheel = source.slice(0, MAX_SLOTS).map((v: unknown) => {
    if (typeof v !== 'string' || (!v.startsWith('/') && !v.startsWith('shortcuts://run-shortcut?name=')) || seen.has(v)) return '';
    seen.add(v); return v;
  });
  const length = Math.max(PAGE_SLOTS, Math.ceil(wheel.length / PAGE_SLOTS) * PAGE_SLOTS);
  while (wheel.length < length) wheel.push('');
  const named = ['mouse4', 'mouse5'];
  const custom = typeof o.trigger === 'object' && o.trigger !== null ? (o.trigger as Record<string, unknown>).vk : null;
  const trigger: TriggerKey = typeof o.trigger === 'string' && named.includes(o.trigger) ? o.trigger as NamedTrigger
    : typeof custom === 'number' && Number.isInteger(custom) && custom > 0 && custom < 256 && !UNSAFE_TRIGGER_VKS.includes(custom)
      ? { vk: custom } : { vk: 0x56 };
  const names: Record<string, string> = {};
  if (typeof o.names === 'object' && o.names !== null) {
    for (const [key, v] of Object.entries(o.names)) if (seen.has(key) && typeof v === 'string' && v.trim()) names[key] = v.trim().slice(0, 40);
  }
  const size = typeof o.wheelSizePx === 'number' && Number.isFinite(o.wheelSizePx) ? o.wheelSizePx : 520;
  return { version: 2, trigger, wheel, names,
    launchAtLogin: typeof o.launchAtLogin === 'boolean' ? o.launchAtLogin : false,
    enabledOnStart: typeof o.enabledOnStart === 'boolean' ? o.enabledOnStart : true,
    windowPreviews: o.windowPreviews === true,
    appearance: o.appearance === 'light' || o.appearance === 'dark' ? o.appearance : 'system',
    wheelSizePx: Math.max(360, Math.min(720, Math.round(size / 20) * 20)),
  };
}
