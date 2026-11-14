import { previewWindowFor, type OuterItem, type BrowserTab } from '../shared/content';
import {
  app, BrowserWindow, desktopCapturer, dialog, ipcMain, Menu, nativeImage, nativeTheme, powerMonitor, screen, shell, systemPreferences, Tray,
} from 'electron';
import { join } from 'node:path';
import { HELPER_VERSION, type HelperConfig, type HelperEvent, type HelperStatus } from '../shared/protocol';
import { MAX_SLOTS, STARTER_APPS, sanitize, type Settings, type WheelItem } from '../shared/settings';
import { appIcon, installedApps, launchApp, launchContent, wheelItems, wheelSkeleton } from './apps';
import { InputBridge } from './inputBridge';
import { AccessibilityGate } from './accessibilityGate';
import { Overlay } from './overlay';
import { loadSettings, saveSettings } from './settingsStore';
import { activateWindow, activateBrowserTab, listBrowserTabs, listWindows, watchWindowPreview } from './windows';
import type { WindowState, WindowItem } from '../shared/windows';

// This menu bar utility restores its own state. AppKit's crash-recovery dialog can otherwise
// block `ready` after an abnormal exit. Registration is in-memory, not a system preference write.
systemPreferences.registerDefaults({ ApplePersistenceIgnoreState: true });

const ACCESSIBILITY_URL = 'x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility';

const helperPath = (): string =>
  app.isPackaged
    ? join(process.resourcesPath, 'hook-helper')
    : join(app.getAppPath(), 'native', 'hook-helper', 'build', 'hook-helper');

const assetPath = (name: string): string =>
  app.isPackaged ? join(process.resourcesPath, name) : join(app.getAppPath(), 'build', name);

const pageUrl = (page: 'overlay' | 'settings'): { dev: string; file: string } => ({
  dev: `${process.env.ELECTRON_RENDERER_URL ?? ''}/${page}/index.html`,
  file: join(__dirname, `../renderer/${page}/index.html`),
});

const loadPage = (win: BrowserWindow, page: 'overlay' | 'settings'): Promise<void> =>
  process.env.ELECTRON_RENDERER_URL ? win.loadURL(pageUrl(page).dev) : win.loadFile(pageUrl(page).file);

process.on('uncaughtException', (err) => console.error('uncaught:', err));
process.on('unhandledRejection', (err) => console.error('unhandled rejection:', err));

let settings: Settings = loadSettings();
let items: WheelItem[] = [];
let status: HelperStatus = 'starting';
let bridge: InputBridge | null = null;
let overlay: Overlay | null = null;
let settingsWin: BrowserWindow | null = null;
let tray: Tray | null = null;
let trayMenu: Menu | null = null;
let quitting = false;
let hovered: number | null = null;
let page = 0;
let activeItems: WheelItem[] = [];
let wheelGeneration = 0;
let windowState: WindowState = { accessibility: false, screen: false, running: [], windows: [] };
let browserTabs: BrowserTab[] = [];
let windowScan: Promise<void> | null = null;
let windowTimer: NodeJS.Timeout | null = null;
let previewGeneration = 0;
let previewWindow: string | null = null;
let previewSelection: WindowItem | null = null;
let hoveredContent: OuterItem | null = null;
let stopPreview: (() => void) | null = null;
let screenRequested = false;
let screenLocked = false;
let capturing = false;
const accessGate = new AccessibilityGate();
let accessTimer: NodeJS.Timeout | null = null;
let accessRequested = false;
let staleAccess = false;

const helperConfig = (s: Settings): HelperConfig => ({
  type: 'config',
  trigger: typeof s.trigger === 'object' ? 'vk' : s.trigger,
  triggerVk: typeof s.trigger === 'object' ? s.trigger.vk : 0,
  openOnPress: true,
  dragThresholdPx: 8,
  // The ring is a launcher: a plain click on its trigger belongs to the app underneath.
  clickPing: false,
  gameCheck: false,
  emoteTrigger: 'off',
  askTrigger: 'off',
  enabled: s.enabledOnStart,
  toggleVk: 0,
});

/** Re-reads the ring's contents. Icons are cached by path, so this is cheap after the first run. */
function syncSuspended(): void {
  bridge?.send({ type: 'suspend', on: screenLocked || capturing });
}

function withWindows(item: WheelItem): WheelItem {
  return { ...item, content: settings.content[item.path] ?? [], tabs: browserTabs.filter((tab) => tab.appPath === item.path), running: windowState.running.includes(item.path), windows: windowState.windows.filter((w) => w.appPath === item.path) };
}

function pageItems(): WheelItem[] {
  const result = items.slice(page * settings.slotsPerPage, (page + 1) * settings.slotsPerPage);
  while (result.length < settings.slotsPerPage) result.push({ name: '', path: '', icon: '' });
  return result;
}

function cancelKeyCapture(): void {
  if (!capturing) return;
  capturing = false;
  syncSuspended();
  const win = settingsWin;
  if (win && !win.isDestroyed() && !win.webContents.isDestroyed()) {
    win.webContents.send('capture:cancel');
    win.webContents.send('settings:changed', stateSnapshot());
  }
}

function finishGesture(): void {
  wheelGeneration++;
  previewGeneration++;
  stopPreview?.();
  stopPreview = null;
  previewWindow = null; previewSelection = null; hoveredContent = null;
  hovered = null;
  overlay?.close(screenLocked || quitting || capturing);
}

async function refreshItems(): Promise<void> {
  const snapshot = settings;
  items = wheelSkeleton(snapshot.wheel, snapshot.names).map(withWindows);
  const enriched = await wheelItems(snapshot.wheel, snapshot.names);
  if (settings === snapshot) items = enriched.map(withWindows);
}

function refreshWindows(): Promise<void> {
  if (screenLocked || quitting) return Promise.resolve();
  if (windowScan) return windowScan;
  const snapshot = settings;
  windowScan = Promise.all([listWindows(snapshot.wheel), listBrowserTabs(snapshot.wheel)]).then(([value, tabs]) => {
    if (snapshot !== settings) return;
    windowState = value;
    browserTabs = tabs;
    items = items.map(withWindows);
    if (overlay && overlay.geometry.count > 0 && !overlay.hoveringContent) {
      activeItems = activeItems.map(withWindows);
      overlay.update(activeItems, settings.wheelSizePx, page);
      overlay.hover(hovered);
      updateHoveredPreview();
    }
    settingsWin?.webContents.send('settings:changed', stateSnapshot());
  }).finally(() => { windowScan = null; if (snapshot !== settings) void refreshWindows(); });
  return windowScan;
}

function updateHoveredPreview(): void {
  const owner = hovered === null ? undefined : activeItems[hovered];
  const selected = settings.windowPreviews ? previewWindowFor(owner, hoveredContent) : null;
  previewSelection = selected;
  const key = selected ? `${selected.id}:${selected.minimized}` : null;
  if (key === previewWindow) return;
  previewWindow = key;
  const generation = ++previewGeneration;
  stopPreview?.(); stopPreview = null; overlay?.preview(null);
  if (selected) stopPreview = watchWindowPreview(selected, (result) => {
    if (generation === previewGeneration && overlay && overlay.geometry.count > 0 && previewSelection)
      overlay.preview(previewSelection, result);
  });
}

function applySettings(next: Settings): void {
  finishGesture();
  settings = sanitize(next);
  nativeTheme.themeSource = settings.appearance;
  page = Math.min(page, Math.ceil(MAX_SLOTS / settings.slotsPerPage) - 1);
  saveSettings(settings);
  // An unsigned dev build may not register a login item; the packaged app can.
  if (app.isPackaged) app.setLoginItemSettings({ openAtLogin: settings.launchAtLogin });
  bridge?.send(helperConfig(settings));
  void refreshItems().then(() => settingsWin?.webContents.send('settings:changed', stateSnapshot()));
  refreshTray();
  void refreshWindows();
}

interface StateSnapshot {
  settings: Settings;
  status: HelperStatus;
  items: Array<{ name: string; path: string }>;
  maxSlots: number;
  helperVersion: number;
  staleAccess: boolean;
  capturing: boolean;
  screenPermission: boolean;
}

function stateSnapshot(): StateSnapshot {
  return {
    settings,
    status,
    items: items.map((i) => ({ name: i.name, path: i.path })),
    maxSlots: MAX_SLOTS,
    helperVersion: HELPER_VERSION,
    staleAccess,
    capturing,
    screenPermission: systemPreferences.getMediaAccessStatus('screen') === 'granted',
  };
}

function onHelperEvent(ev: HelperEvent): void {
  switch (ev.type) {
    case 'scroll': {
      if (!overlay || overlay.geometry.count === 0 || !ev.delta || overlay.inOuterRegion(ev.x, ev.y)) break;
      previewGeneration++; stopPreview?.(); stopPreview = null;
      previewWindow = null; previewSelection = null; hoveredContent = null; overlay.preview(null);
      const pages = Math.ceil(MAX_SLOTS / settings.slotsPerPage);
      page = (page + (ev.delta < 0 ? 1 : pages - 1)) % pages;
      activeItems = pageItems();
      // Keep the centre fixed while switching pages during a held gesture.
      overlay.update(activeItems, settings.wheelSizePx, page);
      hovered = overlay.slotAt(ev.x, ev.y);
      overlay.hover(hovered);
      updateHoveredPreview();
      break;
    }
    case 'wheelOpen': {
      finishGesture();
      if (screenLocked || capturing || status !== 'running') return;
      void refreshWindows();
      if (!items.some((item) => item.path)) {
        // Nothing pinned yet: no empty ring to stare at. The tray menu opens settings.
        return;
      }
      hovered = null;
      activeItems = pageItems();
      overlay?.open(ev.x, ev.y, activeItems, settings.wheelSizePx, page, settings.appearance, settings.windowPreviews);
      break;
    }
    case 'wheelMove': {
      if (!overlay || overlay.geometry.count === 0 || screenLocked || capturing || status !== 'running') break;
      const choice = overlay?.selectAt(ev.x, ev.y);
      hovered = choice?.slot ?? null;
      hoveredContent = choice?.content ?? null;
      updateHoveredPreview();
      break;
    }
    case 'wheelRelease': {
      if (!overlay || overlay.geometry.count === 0 || screenLocked || capturing || status !== 'running') break;
      const choice = overlay?.selectAt(ev.x, ev.y);
      const i = choice?.slot ?? null;
      const picked = i === null ? undefined : activeItems[i];
      finishGesture();
      if (choice?.content?.kind === 'pin' && picked) launchContent(picked.path, choice.content.target, (message) => { if (!screenLocked && !quitting) overlay?.toast(message); });
      else if (choice?.content?.kind === 'tab') void activateBrowserTab(choice.content.tab).then((ok) => { if (!ok && !screenLocked && !quitting) overlay?.toast('标签已变化，请重新呼出轮盘选择。'); });
      else if (choice?.window) void activateWindow(choice.window).then((ok) => { if (!ok && !screenLocked && !quitting) overlay?.toast('这个窗口已关闭或暂时无法切换，请重新选择。'); });
      else if (picked) launchApp(picked.path, (message) => { if (!screenLocked && !quitting) overlay?.toast(message); });
      break;
    }
    case 'cancel': {
      finishGesture();
      break;
    }
    case 'error':
      console.error(`hook helper: ${ev.message}`);
      break;
    case 'ready':
    case 'toggled':
    case 'sim':
      break;
  }
}

function setStatus(next: HelperStatus): void {
  if (next === status) return;
  status = next;
  if (next !== 'running') finishGesture();
  settingsWin?.webContents.send('settings:changed', stateSnapshot());
  refreshTray();
}

function requestAccessibility(): void {
  if (systemPreferences.isTrustedAccessibilityClient(false)) {
    const repairEntry = staleAccess;
    accessGate.retry();
    refreshAccess();
    if (repairEntry) void shell.openExternal(ACCESSIBILITY_URL);
    return;
  }
  // Only this explicit user action may prompt, at most once per session. Polls never ask or open windows.
  if (!accessRequested) {
    accessRequested = true;
    systemPreferences.isTrustedAccessibilityClient(true);
  }
  void shell.openExternal(ACCESSIBILITY_URL);
}

function refreshAccess(): void {
  if (!bridge || quitting) return;
  const result = accessGate.observe(systemPreferences.isTrustedAccessibilityClient(false), bridge.status);
  const changed = staleAccess !== result.stale;
  staleAccess = result.stale;
  setStatus(result.status);
  if (changed) settingsWin?.webContents.send('settings:changed', stateSnapshot());
  if (result.action === 'start') bridge.retry();
  if (result.action === 'stop') {
    finishGesture();
    void bridge.stop().then(() => {
      accessGate.retry();
      refreshAccess();
    });
  }
}

function openSettings(): void {
  void app.dock?.show();
  if (settingsWin) {
    if (settingsWin.isMinimized()) settingsWin.restore();
    settingsWin.show();
    settingsWin.focus();
    return;
  }
  settingsWin = new BrowserWindow({
    width: 900,
    height: 640,
    minWidth: 900,
    minHeight: 640,
    title: 'ViGo',
    titleBarStyle: 'hiddenInset',
    backgroundColor: '#00000000',
    vibrancy: 'under-window',
    visualEffectState: 'active',
    webPreferences: {
      preload: join(__dirname, '../preload/settings.js'),
      contextIsolation: true,
      sandbox: false,
    },
  });
  void loadPage(settingsWin, 'settings');
  settingsWin.on('blur', cancelKeyCapture);
  settingsWin.on('hide', cancelKeyCapture);
  settingsWin.on('minimize', cancelKeyCapture);
  settingsWin.on('closed', () => {
    capturing = false;
    syncSuspended();
    settingsWin = null;
    if (!quitting && process.env.VIGO_SHOW_DOCK !== '1') app.dock?.hide();
  });
}

function refreshTray(): void {
  if (!tray) return;
  const label = !settings.enabledOnStart ? '已暂停' : status === 'running' ? '已就绪'
    : status === 'noAccess' ? '等待辅助功能权限' : status === 'failed' ? '输入助手已停止' : '正在连接';
  trayMenu = Menu.buildFromTemplate([
    { label: '打开 ViGo…', click: openSettings },
    { label: settings.enabledOnStart ? '暂停 ViGo' : '启用 ViGo', click: () => applySettings({ ...settings, enabledOnStart: !settings.enabledOnStart }) },
    { type: 'separator' },
    { label: '退出 ViGo', role: 'quit' },
  ]);
  tray.setToolTip(`ViGo · ${label}`);
}

/** Shows the ring at the cursor for a few seconds: a look at the layout before any input permission exists. */
function previewWheel(nextPage = 0): void {
  if (screenLocked) return;
  finishGesture();
  const generation = wheelGeneration;
  page = Math.max(0, Math.min(Math.ceil(MAX_SLOTS / settings.slotsPerPage) - 1, Math.floor(nextPage)));
  if (!items.some((item) => item.path)) {
    void dialog.showMessageBox({ message: '轮盘还是空的', detail: '先在设置里把一个动作拖到轮盘上。' });
    openSettings();
    return;
  }
  const p = screen.getCursorScreenPoint();
  hovered = null;
  activeItems = pageItems();
  overlay?.open(p.x, p.y, activeItems, settings.wheelSizePx, page, settings.appearance, settings.windowPreviews);
  hovered = activeItems.findIndex((item) => item.path) >= 0 ? activeItems.findIndex((item) => item.path) : null;
  hoveredContent = null; overlay?.hover(hovered); updateHoveredPreview();
  setTimeout(() => {
    if (generation !== wheelGeneration) return;
    finishGesture();
  }, 2500);
}

function startHelper(): void {
  bridge = new InputBridge({ command: helperPath() });
  bridge.on('event', onHelperEvent);
  bridge.on('status', () => { refreshAccess(); if (bridge?.status === 'running') { syncSuspended(); void refreshWindows(); } });
  bridge.on('log', (line: string) => { console.log(`[helper] ${line}`); });
  bridge.send(helperConfig(settings));
  refreshAccess();
  accessTimer = setInterval(refreshAccess, 1500);
  accessTimer.unref();
}

function createTray(): void {
  const image = nativeImage.createFromPath(assetPath('trayTemplate.png'));
  image.setTemplateImage(true);
  tray = new Tray(image);
  tray.on('click', openSettings);
  tray.on('right-click', () => { if (trayMenu) tray?.popUpContextMenu(trayMenu); });
  refreshTray();
}

function registerIpc(): void {
  ipcMain.handle('settings:get', async () => ({ ...stateSnapshot(), installed: await installedApps() }));
  ipcMain.handle('settings:save', (_e, next: unknown) => {
    applySettings(next as Settings);
    return stateSnapshot();
  });
  ipcMain.handle('settings:icon', (_e, path: string) => appIcon(path));
  ipcMain.handle('settings:open-accessibility', () => requestAccessibility());
  ipcMain.handle('settings:preview-wheel', (_e, nextPage: unknown) => previewWheel(typeof nextPage === 'number' ? nextPage : 0));
  ipcMain.handle('settings:quit', () => app.quit());
  ipcMain.handle('settings:tabs', (_event, owner: string) => settings.wheel.includes(owner) ? listBrowserTabs([owner]) : []);
  ipcMain.handle('settings:window-preview-permission', async () => {
    if (systemPreferences.getMediaAccessStatus('screen') !== 'granted') {
      if (!screenRequested) {
        screenRequested = true;
        // Request from the signed root application so macOS registers the correct identity.
        await desktopCapturer.getSources({ types: ['window'], thumbnailSize: { width: 0, height: 0 } }).catch(() => []);
      }
      if (systemPreferences.getMediaAccessStatus('screen') !== 'granted')
        void shell.openExternal('x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture');
    }
    await refreshWindows();
    return stateSnapshot();
  });
  ipcMain.handle('settings:browse', async () => (await dialog.showOpenDialog({
    title: '添加到 ViGo', properties: ['openFile', 'openDirectory', 'multiSelections'],
  })).filePaths);
  ipcMain.handle('settings:capturing', (event, on: unknown) => {
    const win = settingsWin;
    if (!win || win.isDestroyed() || event.sender.id !== win.webContents.id) return;
    if (typeof on !== 'boolean') return;
    if (on && !settingsWin?.isFocused()) { settingsWin?.webContents.send('capture:cancel'); return; }
    if (capturing === on) return;
    capturing = on; if (on) finishGesture(); syncSuspended();
    settingsWin?.webContents.send('settings:changed', stateSnapshot());
  });
}

const gotLock = app.requestSingleInstanceLock();
if (!gotLock) {
  app.quit();
} else {
  app.on('second-instance', () => { void app.whenReady().then(openSettings); });
  app.on('activate', () => { if (app.isReady()) openSettings(); });

  void app.whenReady().then(async () => {
    nativeTheme.themeSource = settings.appearance;
    app.dock?.setIcon(assetPath(app.isPackaged ? 'appIcon.png' : 'icon.png'));
    if (process.env.VIGO_SHOW_DOCK !== '1') app.dock?.hide();
    registerIpc();
    Menu.setApplicationMenu(Menu.buildFromTemplate([
      { label: 'ViGo', submenu: [
        { label: '设置…', accelerator: 'CmdOrCtrl+,', click: openSettings },
        { type: 'separator' }, { role: 'hide', label: '隐藏 ViGo' }, { role: 'hideOthers', label: '隐藏其它应用' }, { role: 'unhide', label: '显示全部' },
        { type: 'separator' }, { role: 'quit', label: '退出 ViGo' },
      ] }, { label: '编辑', submenu: [
        { role: 'undo', label: '撤销' }, { role: 'redo', label: '重做' }, { type: 'separator' },
        { role: 'cut', label: '剪切' }, { role: 'copy', label: '复制' }, { role: 'paste', label: '粘贴' }, { role: 'selectAll', label: '全选' },
      ] }, { role: 'windowMenu', label: '窗口', submenu: [
        { role: 'minimize', label: '最小化' }, { role: 'close', label: '关闭窗口' }, { type: 'separator' }, { role: 'front', label: '全部置于前台' },
      ] },
    ]));
    overlay = new Overlay(join(__dirname, '../preload/overlay.js'), pageUrl('overlay').file, process.env.ELECTRON_RENDERER_URL ? pageUrl('overlay').dev : undefined);
    overlay.create();
    await installedApps();
    await refreshItems();
    createTray();
    screenLocked = powerMonitor.getSystemIdleState(60) === 'locked';
    powerMonitor.on('lock-screen', () => { screenLocked = true; cancelKeyCapture(); finishGesture(); syncSuspended(); });
    powerMonitor.on('unlock-screen', () => {
      if (quitting) return;
      screenLocked = false;
      finishGesture();
      setStatus('starting');
      // Keys released on the secure desktop never reached the tap; restart with fresh input state.
      void bridge?.stop().then(() => { if (!quitting) { accessGate.retry(); refreshAccess(); void refreshWindows(); } });
    });
    startHelper();
    void refreshWindows();
    windowTimer = setInterval(() => { if ((overlay?.geometry.count ?? 0) > 0) void refreshWindows(); }, 4000);
    windowTimer.unref();
    if (!items.some((item) => item.path)) {
      // First run: seed the ring with whatever of the usual suspects is installed.
      const installed = new Set((await installedApps()).map((a) => a.path));
      applySettings({ ...settings, wheel: STARTER_APPS.filter((p) => installed.has(p)).slice(0, 8) });
      openSettings();
    }
  }).catch((err: unknown) => {
    console.error('startup failed:', err);
  });

  app.on('window-all-closed', () => {
    // A menu bar app keeps running with no windows.
  });

  app.on('before-quit', () => {
    quitting = true;
    finishGesture();
    if (accessTimer) clearInterval(accessTimer);
    if (windowTimer) clearInterval(windowTimer);
    void bridge?.stop();
    overlay?.destroy();
  });
}
