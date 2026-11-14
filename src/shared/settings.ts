import { validContentTarget, type PinnedContent, type BrowserTab } from './content.ts';
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

export interface WheelItem { name: string; path: string; icon: string; running?: boolean; windows?: import('./windows').WindowItem[]; content?: PinnedContent[]; tabs?: BrowserTab[]; }
export interface Settings {
  version: 2;
  trigger: TriggerKey;
  /** Pages of configurable size, including empty strings for unassigned slots; at most 24 actions. */
  wheel: string[];
  slotsPerPage: number;
  names: Record<string, string>;
  content: Record<string, PinnedContent[]>;
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
  version: 2, trigger: { vk: 0x56 }, wheel: Array<string>(8).fill(''), names: {}, content: {}, slotsPerPage: 8,
  launchAtLogin: false, enabledOnStart: true, wheelSizePx: 520, appearance: 'system', windowPreviews: false,
};

export function sanitize(raw: unknown): Settings {
  const o = typeof raw === 'object' && raw !== null ? raw as Record<string, unknown> : {};
  const slotsPerPage = typeof o.slotsPerPage === 'number' && Number.isInteger(o.slotsPerPage) ? Math.max(4, Math.min(12, o.slotsPerPage)) : 8;
  const capacity = Math.ceil(MAX_SLOTS / slotsPerPage) * slotsPerPage;
  const source = Array.isArray(o.wheel) ? o.wheel : [];
  const seen = new Set<string>();
  const normalized = source.slice(0, 36).map((v: unknown) => {
    if (typeof v !== 'string' || !v.startsWith('/') || seen.has(v) || seen.size >= MAX_SLOTS) return '';
    seen.add(v); return v;
  });
  const wheel = normalized.slice(capacity).some(Boolean) ? normalized.filter(Boolean) : normalized.slice(0, capacity);
  const length = Math.max(slotsPerPage, Math.ceil(wheel.length / slotsPerPage) * slotsPerPage);
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
  const content: Record<string, PinnedContent[]> = {};
  if (typeof o.content === 'object' && o.content !== null) {
    for (const [owner, raw] of Object.entries(o.content)) {
      if (!seen.has(owner) || !Array.isArray(raw)) continue;
      const ids = new Set<string>();
      content[owner] = raw.filter((item): item is PinnedContent => {
        if (!item || typeof item !== 'object' || typeof item.id !== 'string' || !item.id || ids.has(item.id)
          || typeof item.name !== 'string' || !item.name.trim() || typeof item.target !== 'string' || !validContentTarget(item.target)) return false;
        ids.add(item.id); return true;
      }).slice(0, 24).map((item) => ({ id: item.id.slice(0, 120), name: item.name.trim().slice(0, 80), target: item.target }));
    }
  }
  const size = typeof o.wheelSizePx === 'number' && Number.isFinite(o.wheelSizePx) ? o.wheelSizePx : 520;
  return { version: 2, trigger, wheel, names, content, slotsPerPage,
    launchAtLogin: typeof o.launchAtLogin === 'boolean' ? o.launchAtLogin : false,
    enabledOnStart: typeof o.enabledOnStart === 'boolean' ? o.enabledOnStart : true,
    windowPreviews: o.windowPreviews === true,
    appearance: o.appearance === 'light' || o.appearance === 'dark' ? o.appearance : 'system',
    wheelSizePx: Math.max(360, Math.min(720, Math.round(size / 20) * 20)),
  };
}
