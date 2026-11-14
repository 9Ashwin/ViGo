import type { WindowItem } from './windows';
export interface PinnedContent { id: string; name: string; target: string; }
export interface BrowserTab { id: string; kind: 'tab'; appPath: string; pid: number; windowId: number; index: number; identifier?: string; title: string; selected: boolean; }
export type OuterItem = { kind: 'pin'; id: string; title: string; target: string }
  | { kind: 'tab'; id: string; title: string; tab: BrowserTab }
  | { kind: 'window'; id: string; title: string; window: WindowItem };
export function validContentTarget(target: string): boolean {
  if (target.startsWith('/')) return true;
  try { const url = new URL(target); return url.protocol === 'https:' || url.protocol === 'http:'; } catch { return false; }
}
export function outerItems(pins: readonly PinnedContent[], tabs: readonly BrowserTab[], windows: readonly WindowItem[]): OuterItem[] {
  return [...pins.map((pin): OuterItem => ({ kind: 'pin', id: pin.id, title: pin.name, target: pin.target })),
    ...tabs.map((tab): OuterItem => ({ kind: 'tab', id: tab.id, title: tab.title, tab })),
    ...windows.map((window): OuterItem => ({ kind: 'window', id: window.id, title: window.title, window }))];
}

/** Browser tabs share their parent window; its current picture must keep its own window title. */
export function previewWindowFor(owner: { path: string; name: string; windows?: WindowItem[]; tabs?: BrowserTab[] } | undefined, content: OuterItem | null): WindowItem | null {
  if (content?.kind === 'window') return content.window;
  if (!owner?.path) return null;
  const tab = content?.kind === 'tab' ? content.tab : undefined;
  const window = tab ? owner.windows?.find((w) => w.pid === tab.pid && w.windowId === tab.windowId)
    : owner.windows?.find((w) => !w.minimized) ?? owner.windows?.[0];
  if (window) return window;
  const current = tab ?? owner.tabs?.find((t) => t.selected) ?? owner.tabs?.[0];
  return current ? { id: `${current.pid}:${current.windowId}`, appPath: owner.path, pid: current.pid, windowId: current.windowId,
    title: `${owner.name} · 当前窗口`, rawTitle: '', index: -1, minimized: false, bounds: [] } : null;
}
