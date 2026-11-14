import { BrowserWindow, screen } from 'electron';
import { slotAt, wheelBoxAt, WINDOW_SIZE, type WheelBox } from '../shared/geometry';
import type { WheelItem } from '../shared/settings';
import { chipAt, windowChips, type WindowItem } from '../shared/windows';

export interface OpenPayload {
  /** Ring centre inside the window: the cursor, unless the window had to be pushed back onto the screen. */
  cx: number;
  cy: number;
  items: WheelItem[];
  size: number;
  page: number;
}

/**
 * The ring's window: one transparent, click-through panel that is kept alive and merely moved and shown,
 * so summoning it costs no window creation. It never takes focus — the app under the cursor keeps it,
 * and the helper — not this window — is what reports the cursor.
 */
export class Overlay {
  private win: BrowserWindow | null = null;
  private box: WheelBox = { cx: 0, cy: 0, count: 0, inner: 0, outer: 0 };
  private originX = 0;
  private originY = 0;
  private items: WheelItem[] = [];
  private size = 520;
  private owner: number | null = null;

  constructor(private readonly preload: string, private readonly page: string, private readonly devUrl?: string) {}

  /** Builds the window hidden; the first `open` only has to show it. */
  create(): void {
    if (this.win) return;
    this.win = new BrowserWindow({
      width: WINDOW_SIZE,
      height: WINDOW_SIZE,
      show: false,
      frame: false,
      transparent: true,
      hasShadow: false,
      resizable: false,
      movable: false,
      minimizable: false,
      maximizable: false,
      fullscreenable: false,
      skipTaskbar: true,
      focusable: false,
      roundedCorners: false,
      type: 'panel',
      webPreferences: { preload: this.preload, contextIsolation: true, sandbox: false, backgroundThrottling: false },
    });
    this.win.setAlwaysOnTop(true, 'screen-saver');
    this.win.excludedFromShownWindowsMenu = true;
    this.win.setHiddenInMissionControl(true);
    this.win.setVisibleOnAllWorkspaces(true, { visibleOnFullScreen: true });
    this.win.setIgnoreMouseEvents(true);
    if (this.devUrl) void this.win.loadURL(this.devUrl);
    else void this.win.loadFile(this.page);
    this.win.on('closed', () => {
      this.win = null;
    });
  }

  /** Shows the ring centred on `x, y` (global points) and remembers the geometry its renderer is drawing. */
  open(x: number, y: number, items: WheelItem[], size = 520, page = 0): WheelBox {
    this.create();
    this.items = items;
    this.size = size;
    this.owner = null;
    const win = this.win;
    const area = screen.getDisplayNearestPoint({ x, y }).workArea;
    this.originX = area.x;
    this.originY = area.y;
    this.box = wheelBoxAt(x, y, this.originX, this.originY, size);
    if (win) {
      win.setBounds({ x: this.originX, y: this.originY, width: area.width, height: area.height });
      win.showInactive();
      win.webContents.send('wheel:open', { cx: this.box.cx, cy: this.box.cy, items, size, page } satisfies OpenPayload);
    }
    return this.box;
  }

  /** Which slice the global point `x, y` is over, or null over the hub, past the rim, or with no wheel. */
  slotAt(x: number, y: number): number | null {
    return slotAt(x - this.originX, y - this.originY, this.box);
  }

  hover(index: number | null): void {
    this.owner = index;
    this.win?.webContents.send('wheel:hover', index);
    const windows = index === null ? [] : this.items[index]?.windows ?? [];
    this.win?.webContents.send('wheel:windows', { windows, chips: index === null ? [] : windowChips(this.box.cx, this.box.cy, index, windows.length, this.size) });
  }

  selectAt(x: number, y: number): { slot: number | null; window: WindowItem | null } {
    const lx = x - this.originX, ly = y - this.originY;
    const windows = this.owner === null ? [] : this.items[this.owner]?.windows ?? [];
    const chips = this.owner === null ? [] : windowChips(this.box.cx, this.box.cy, this.owner, windows.length, this.size);
    const chip = chipAt(lx, ly, chips);
    if (chip !== null) {
      this.win?.webContents.send('wheel:window-hover', chip);
      return { slot: this.owner, window: windows[chip] ?? null };
    }
    this.win?.webContents.send('wheel:window-hover', null);
    const radius = Math.hypot(lx - this.box.cx, ly - this.box.cy);
    if (windows.length > 0 && this.owner !== null && radius > this.size * 188 / 600) return { slot: radius <= this.box.outer ? this.owner : null, window: null };
    const slot = this.slotAt(x, y);
    const owner = slot !== null && this.items[slot]?.path ? slot : null;
    if (owner !== this.owner) this.hover(owner);
    return { slot, window: null };
  }

  preview(item: WindowItem | null, image = '', cached = false): void {
    this.win?.webContents.send('wheel:preview', item ? { title: item.title, image, cached } : null);
  }

  update(items: WheelItem[], size: number, page: number): void {
    this.items = items;
    this.size = size;
    this.win?.webContents.send('wheel:update', { cx: this.box.cx, cy: this.box.cy, items, size, page } satisfies OpenPayload);
    this.hover(this.owner);
  }

  close(): void {
    if (!this.win) return;
    this.owner = null;
    this.win.hide();
    this.win.webContents.send('wheel:close');
    this.box = { ...this.box, count: 0 };
  }

  toast(message: string): void {
    if (!this.win || this.box.count > 0) return;
    const win = this.win;
    win.webContents.send('action:message', message);
    win.showInactive();
    setTimeout(() => {
      if (!win.isDestroyed()) {
        win.webContents.send('action:message', '');
        if (this.box.count === 0) win.hide();
      }
    }, 2600);
  }

  destroy(): void {
    this.win?.destroy();
    this.win = null;
  }

  /** For the settings window's preview link and for tests: where the last ring was drawn. */
  get geometry(): WheelBox {
    return this.box;
  }
}
