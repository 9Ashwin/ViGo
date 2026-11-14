import { app, nativeTheme, type BrowserWindow } from 'electron';
import { join } from 'node:path';
export interface GlassRect { x: number; y: number; width: number; height: number; radius: number; angle?: number; }
interface GlassBridge { update(handle: Buffer, rects: GlassRect[], dark: boolean): number; }
let bridge: GlassBridge | null | undefined;
export function updateGlass(win: BrowserWindow, rects: GlassRect[], dark = nativeTheme.shouldUseDarkColors): void {
  if (bridge === undefined) {
    try {
      bridge = require(app.isPackaged ? join(process.resourcesPath, 'glass.node') : join(app.getAppPath(), 'native/glass/build/glass.node')) as GlassBridge;
    } catch (error) { console.error('Native glass unavailable:', error); bridge = null; }
  }
  if (!win.isDestroyed() && rects.every((r) => [r.x, r.y, r.width, r.height, r.radius].every(Number.isFinite) && r.width > 0 && r.height > 0)) {
    try { bridge?.update(win.getNativeWindowHandle(), rects, dark); }
    catch (error) { console.error('Native glass update skipped:', error); }
  }
}
