import { contextBridge, ipcRenderer } from 'electron';
import type { OpenPayload } from '../main/overlay';
import type { WindowChip, WindowItem } from '../shared/windows';

const api = {
  onMessage: (cb: (message: string) => void): void => { ipcRenderer.on('action:message', (_e, message) => cb(message)); },
  onUpdate: (cb: (p: OpenPayload) => void): void => { ipcRenderer.on('wheel:update', (_e, p) => cb(p)); },
  onWindows: (cb: (p: { windows: WindowItem[]; chips: WindowChip[] }) => void): void => { ipcRenderer.on('wheel:windows', (_e, p) => cb(p)); },
  onWindowHover: (cb: (index: number | null) => void): void => { ipcRenderer.on('wheel:window-hover', (_e, index) => cb(index)); },
  onPreview: (cb: (p: { title: string; image: string; cached: boolean } | null) => void): void => { ipcRenderer.on('wheel:preview', (_e, p) => cb(p)); },
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
