import { updateGlass, type GlassRect } from './glass';
import { outerItems, type OuterItem } from '../shared/content';
import { BrowserWindow, nativeTheme, screen } from 'electron';
import { slotAt, stableSlotAt, wheelBoxAt, WINDOW_SIZE, type WheelBox } from '../shared/geometry';
import type { WheelItem } from '../shared/settings';
import { contentPanels, chipAt, windowChips, type WindowItem, type PreviewFrame } from '../shared/windows';

export interface OpenPayload {
  /** Ring centre inside the window: the cursor, unless the window had to be pushed back onto the screen. */
  cx: number;
  cy: number;
  items: WheelItem[];
  size: number;
  page: number;
  pages: number;
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
  private activeChip: number | null = null;
  private closeTimer: NodeJS.Timeout | null = null;
  private dark = false;
  private chips: ReturnType<typeof windowChips> = [];
  private outer: OuterItem[] = [];
  private material: GlassRect[] = [];
  private previewId: string | null = null;

  private glass(): void {
    if (!this.win) return;
    const radius = this.size * 188 / 600;
    updateGlass(this.win, [{ x: this.box.cx - radius, y: this.box.cy - radius, width: radius * 2, height: radius * 2, radius },
      ...this.contentMaterial(), ...this.material], this.dark);
  }

  private contentMaterial(): GlassRect[] {
    return contentPanels(this.chips).map((bounds) => ({ ...bounds, radius: 14 }));
  }

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
  open(x: number, y: number, items: WheelItem[], size = 520, page = 0, appearance: 'system' | 'light' | 'dark' = 'system', windowPreviews = false): WheelBox {
    this.create();
    if (this.closeTimer) { clearTimeout(this.closeTimer); this.closeTimer = null; }
    this.dark = appearance === 'system' ? nativeTheme.shouldUseDarkColors : appearance === 'dark';
    this.chips = []; this.outer = []; this.material = []; this.previewId = null;
    this.items = items;
    this.size = size;
    this.owner = null; this.activeChip = null;
    const win = this.win;
    const display = screen.getDisplayNearestPoint({ x, y });
    const area = display.bounds ?? display.workArea;
    const width = area.width, height = area.height;
    void windowPreviews;
    this.originX = Math.round(Math.max(area.x, Math.min(area.x + area.width - width, x - width / 2)));
    this.originY = Math.round(Math.max(area.y, Math.min(area.y + area.height - height, y - height / 2)));
    this.box = wheelBoxAt(x, y, this.originX, this.originY, size, items.length);
    if (win) {
      win.setBounds({ x: this.originX, y: this.originY, width, height });
      this.glass();
      win.webContents.send('wheel:theme', this.dark);
      win.showInactive();
      win.webContents.send('wheel:open', { cx: this.box.cx, cy: this.box.cy, items, size, page, pages: Math.ceil(24 / items.length) } satisfies OpenPayload);
    }
    return this.box;
  }

  /** Which slice the global point `x, y` is over, or null over the hub, past the rim, or with no wheel. */
  slotAt(x: number, y: number): number | null {
    return slotAt(x - this.originX, y - this.originY, this.box);
  }

  hover(index: number | null): void {
    this.owner = index; this.activeChip = null;
    this.win?.webContents.send('wheel:hover', index);
    const item = index === null ? undefined : this.items[index];
    this.outer = item ? outerItems(item.content ?? [], item.tabs ?? [], item.windows ?? []) : [];
    this.renderOuter();
  }

  private renderOuter(): void {
    this.activeChip = null;
    const entries = this.outer;
    const bounds = this.win?.getBounds();
    this.chips = this.owner === null ? [] : windowChips(this.box.cx, this.box.cy, this.owner, entries.length, this.size, this.items.length, bounds);
    this.win?.webContents.send('wheel:windows', { entries, chips: this.chips, total: this.outer.length });
    this.glass();
  }

  inOuterRegion(x: number, y: number): boolean {
    return this.owner !== null && this.outer.length > 0 && Math.hypot(x - this.originX - this.box.cx, y - this.originY - this.box.cy) > this.size * 161 / 600;
  }

  selectAt(x: number, y: number): { slot: number | null; window: WindowItem | null; content: OuterItem | null } {
    if (this.box.count === 0) return { slot: null, window: null, content: null };
    const lx = x - this.originX, ly = y - this.originY;
    const radius = Math.hypot(lx - this.box.cx, ly - this.box.cy);
    this.win?.webContents.send('wheel:pointer', { dx: lx - this.box.cx, dy: ly - this.box.cy, hub: radius < this.box.inner });
    if (radius < this.box.inner) { if (this.owner !== null) this.hover(null); this.win?.webContents.send('wheel:window-hover', null); return { slot: null, window: null, content: null }; }
    const chip = chipAt(lx, ly, this.chips);
    this.activeChip = chip;
    if (chip !== null) {
      this.win?.webContents.send('wheel:window-hover', chip);
      const content = this.outer[chip] ?? null;
      return { slot: this.owner, window: content?.kind === 'window' ? content.window : null, content };
    }
    this.win?.webContents.send('wheel:window-hover', null);
    if (this.outer.length > 0 && this.owner !== null && radius > this.size * (137 + 24) / 600) return { slot: radius <= this.box.outer ? this.owner : null, window: null, content: null };
    const slot = stableSlotAt(lx, ly, this.box, this.owner);
    const owner = slot !== null && this.items[slot]?.path ? slot : null;
    if (owner !== this.owner) this.hover(owner);
    return { slot, window: null, content: null };
  }

  preview(item: WindowItem | null, frame: PreviewFrame = { image: '', cached: false, state: 'loading' }): void {
    const changed = (item?.id ?? null) !== this.previewId;
    this.previewId = item?.id ?? null;
    if (item) {
      const bounds = this.win?.getBounds();
      const radius = this.size * 188 / 600, right = this.box.cx + radius + 16, left = this.box.cx - radius - 256;
      const panels = this.contentMaterial();
      const y = Math.max(8, Math.min((bounds?.height ?? 0) - 196, this.box.cy - 90));
      const sides = panels[0] && panels[0].x + panels[0].width / 2 > this.box.cx ? [left, right] : [right, left];
      const candidates = [...sides.map((x) => ({ x, y })),
        { x: this.box.cx - 120, y: this.box.cy - radius - 204 }, { x: this.box.cx - 120, y: this.box.cy + radius + 16 },
        { x: this.box.cx - 120, y: 8 }, { x: this.box.cx - 120, y: (bounds?.height ?? 0) - 196 }];
      const avoid = [...panels, { x: this.box.cx - radius, y: this.box.cy - radius, width: radius * 2, height: radius * 2 }];
      const free = candidates.find((p) => p.x >= 8 && p.y >= 8 && p.x + 240 <= (bounds?.width ?? 0) - 8 && p.y + 188 <= (bounds?.height ?? 0) - 8
        && avoid.every((r) => p.x + 248 <= r.x || p.x >= r.x + r.width + 8 || p.y + 196 <= r.y || p.y >= r.y + r.height + 8));
      const position = free ?? { x: Math.max(8, Math.min((bounds?.width ?? 0) - 248, sides[0])), y };
      this.material = [{ ...position, width: 240, height: 188, radius: 12 }];
    } else this.material = [];
    if (changed) this.glass();
    this.win?.webContents.send('wheel:preview', item ? { id: item.id, title: item.title, x: this.material[0]?.x ?? 8, y: this.material[0]?.y ?? 8, ...frame } : null);
  }

  update(items: WheelItem[], size: number, page: number): void {
    this.items = items;
    this.size = size;
    this.win?.webContents.send('wheel:update', { cx: this.box.cx, cy: this.box.cy, items, size, page, pages: Math.ceil(24 / items.length) } satisfies OpenPayload);
    this.hover(this.owner);
  }

  close(immediate = false): void {
    if (!this.win) return;
    this.owner = null; this.activeChip = null; this.chips = []; this.outer = []; this.material = []; this.previewId = null;
    this.win.webContents.send('wheel:close');
    this.box = { ...this.box, count: 0 };
    if (this.closeTimer) clearTimeout(this.closeTimer);
    if (immediate) { this.win.hide(); updateGlass(this.win, []); this.closeTimer = null; }
    else this.closeTimer = setTimeout(() => {
      this.closeTimer = null;
      if (this.win && this.box.count === 0) { this.win.hide(); updateGlass(this.win, []); }
    }, 110);
  }

  toast(message: string): void {
    if (!this.win || this.box.count > 0) return;
    if (this.closeTimer) { clearTimeout(this.closeTimer); this.closeTimer = null; }
    updateGlass(this.win, []);
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
    if (this.closeTimer) clearTimeout(this.closeTimer);
    this.win?.destroy();
    this.win = null;
  }

  /** For the settings window's preview link and for tests: where the last ring was drawn. */
  get hoveringContent(): boolean { return this.activeChip !== null; }

  get geometry(): WheelBox {
    return this.box;
  }
}
