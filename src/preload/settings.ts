import { contextBridge, ipcRenderer, webUtils } from 'electron';
import type { Settings } from '../shared/settings';

export interface InstalledApp {
  name: string;
  path: string;
}

export interface SettingsState {
  settings: Settings;
  status: 'starting' | 'running' | 'failed' | 'noAccess';
  items: Array<{ name: string; path: string }>;
  maxSlots: number;
  helperVersion: number;
  staleAccess: boolean;
  screenPermission: boolean;
  installed?: InstalledApp[];
}

const api = {
  get: (): Promise<SettingsState> => ipcRenderer.invoke('settings:get'),
  save: (settings: Settings): Promise<SettingsState> => ipcRenderer.invoke('settings:save', settings),
  icon: (path: string): Promise<string> => ipcRenderer.invoke('settings:icon', path),
  requestAccessibility: (): Promise<void> => ipcRenderer.invoke('settings:open-accessibility'),
  previewWheel: (page = 0): Promise<void> => ipcRenderer.invoke('settings:preview-wheel', page),
  quit: (): Promise<void> => ipcRenderer.invoke('settings:quit'),
  requestWindowPreview: (): Promise<SettingsState> => ipcRenderer.invoke('settings:window-preview-permission'),
  shortcuts: (): Promise<InstalledApp[]> => ipcRenderer.invoke('settings:shortcuts'),
  browse: (): Promise<string[]> => ipcRenderer.invoke('settings:browse'),
  setCapturing: (on: boolean): Promise<void> => ipcRenderer.invoke('settings:capturing', on),
  filePaths: (files: File[]): string[] => files.map((file) => webUtils.getPathForFile(file)).filter(Boolean),
  onChange: (cb: (s: SettingsState) => void): (() => void) => {
    const listener = (_e: unknown, s: SettingsState): void => cb(s);
    ipcRenderer.on('settings:changed', listener);
    return () => { ipcRenderer.removeListener('settings:changed', listener); };
  },
};

contextBridge.exposeInMainWorld('vigo', api);

export type SettingsApi = typeof api;
