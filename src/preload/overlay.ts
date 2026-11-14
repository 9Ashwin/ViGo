import type { OuterItem } from '../shared/content';
import { contextBridge, ipcRenderer } from 'electron';
import type { OpenPayload } from '../main/overlay';
import type { PreviewFrame, WindowChip } from '../shared/windows';

const api = {
  onPointer: (cb: (p: { dx: number; dy: number; hub: boolean }) => void): void => { ipcRenderer.on('wheel:pointer', (_e, p) => cb(p)); },
  onTheme: (cb: (dark: boolean) => void): void => { ipcRenderer.on('wheel:theme', (_e, dark) => cb(dark)); },
  onMessage: (cb: (message: string) => void): void => { ipcRenderer.on('action:message', (_e, message) => cb(message)); },
  onUpdate: (cb: (p: OpenPayload) => void): void => { ipcRenderer.on('wheel:update', (_e, p) => cb(p)); },
  onWindows: (cb: (p: { entries: OuterItem[]; chips: WindowChip[]; total: number }) => void): void => { ipcRenderer.on('wheel:windows', (_e, p) => cb(p)); },
  onWindowHover: (cb: (index: number | null) => void): void => { ipcRenderer.on('wheel:window-hover', (_e, index) => cb(index)); },
  onPreview: (cb: (p: ({ id: string; title: string; x: number; y: number } & PreviewFrame) | null) => void): void => { ipcRenderer.on('wheel:preview', (_e, p) => cb(p)); },
  onOpen: (cb: (p: OpenPayload) => void): void => {
    ipcRenderer.on('wheel:open', (_e, p: OpenPayload) => cb(p));
  },
  onHover: (cb: (index: number | null) => void): void => {
    ipcRenderer.on('wheel:hover', (_e, i: number | null) => cb(i));
  },
  onClose: (cb: () => void): void => {
    ipcRenderer.on('wheel:close', () => cb());
  },
};

contextBridge.exposeInMainWorld('vigo', api);

export type OverlayApi = typeof api;
