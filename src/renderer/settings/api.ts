import type { SettingsApi } from '../../preload/settings';
import type { Platform } from '../../shared/platform';
declare global { interface Window { vigo: SettingsApi; } }
export const api = { ...window.vigo, platform: 'mac' as Platform };
