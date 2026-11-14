/**
 * The hook helper's stdin/stdout protocol, one JSON object per line. Coordinates are macOS global points,
 * which are the same units as Electron's DIPs, so nothing has to be scaled.
 *
 * Third-party license terms are included in THIRD_PARTY_LICENSES.md.
 */

export type WheelKind = 'ping' | 'emote' | 'ask';

export type HelperEvent =
  | { type: 'scroll'; x: number; y: number; delta: number }
  | { type: 'ready'; version: number }
  | { type: 'wheelOpen' | 'wheelMove' | 'wheelRelease'; x: number; y: number; wheel: WheelKind }
  | { type: 'cancel' }
  | { type: 'toggled'; enabled: boolean }
  | { type: 'error'; message: string; code?: string }
  | { type: 'sim'; swallow: boolean; inject: string };

/** `noAccess`: macOS Accessibility access is missing, so no event tap could be created. */
export type HelperStatus = 'starting' | 'running' | 'failed' | 'noAccess';

export const HELPER_EXIT_NO_ACCESS = 3;
/** The `ready` version this build speaks. */
export const HELPER_VERSION = 6;

export type HelperTrigger = 'alt' | 'ctrl' | 'shift' | 'win' | 'capslock' | 'mouse4' | 'mouse5' | 'vk';

export interface HelperConfig {
  type: 'config';
  trigger: HelperTrigger;
  triggerVk?: number;
  /** Open the ring on the press itself rather than after a drag. */
  openOnPress: boolean;
  dragThresholdPx: number;
  clickPing: boolean;
  gameCheck: boolean;
  /** The wheel kinds this build does not use stay off. */
  emoteTrigger: 'off';
  askTrigger: 'off';
  enabled: boolean;
  toggleVk: number;
}

export type HelperCommand = HelperConfig | { type: 'setEnabled'; enabled: boolean } | { type: 'suspend'; on: boolean } | { type: 'shutdown' };

export function serializeCommand(cmd: HelperCommand): string {
  return `${JSON.stringify(cmd)}\n`;
}

const isInt = (x: unknown): x is number => Number.isInteger(x);

export function parseHelperLine(line: string): HelperEvent | null {
  let v: unknown;
  try {
    v = JSON.parse(line);
  } catch {
    return null;
  }
  if (typeof v !== 'object' || v === null || Array.isArray(v)) return null;
  const o = v as Record<string, unknown>;
  switch (o.type) {
    case 'ready':
      return isInt(o.version) ? { type: 'ready', version: o.version } : null;
    case 'cancel':
      return { type: 'cancel' };
    case 'toggled':
      return typeof o.enabled === 'boolean' ? { type: 'toggled', enabled: o.enabled } : null;
    case 'error':
      if (typeof o.message !== 'string') return null;
      return typeof o.code === 'string'
        ? { type: 'error', message: o.message, code: o.code }
        : { type: 'error', message: o.message };
    case 'sim':
      return typeof o.swallow === 'boolean' && typeof o.inject === 'string'
        ? { type: 'sim', swallow: o.swallow, inject: o.inject }
        : null;
    case 'scroll':
      return isInt(o.x) && isInt(o.y) && isInt(o.delta)
        ? { type: 'scroll', x: o.x, y: o.y, delta: o.delta } : null;
    case 'wheelOpen':
    case 'wheelMove':
    case 'wheelRelease':
      if (!isInt(o.x) || !isInt(o.y)) return null;
      return { type: o.type, x: o.x, y: o.y, wheel: 'ping' };
    default:
      return null;
  }
}
