import {
  app, BrowserWindow, dialog, ipcMain, Menu, nativeImage, powerMonitor, screen, shell, systemPreferences, Tray,
} from 'electron';
import { join } from 'node:path';
import { HELPER_VERSION, type HelperConfig, type HelperEvent, type HelperStatus } from '../shared/protocol';
import { MAX_SLOTS, STARTER_APPS, sanitize, type Settings, type WheelItem } from '../shared/settings';
import { appIcon, availableShortcuts, installedApps, launchApp, wheelItems, wheelSkeleton } from './apps';
import { InputBridge } from './inputBridge';
import { AccessibilityGate } from './accessibilityGate';
import { Overlay } from './overlay';
import { loadSettings, saveSettings } from './settingsStore';
import { activateWindow, listWindows, requestScreenPreview, watchWindowPreview } from './windows';
import type { WindowState } from '../shared/windows';

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
let windowScan: Promise<void> | null = null;
let windowTimer: NodeJS.Timeout | null = null;
let previewGeneration = 0;
let previewWindow: string | null = null;
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
  return { ...item, running: windowState.running.includes(item.path), windows: windowState.windows.filter((w) => w.appPath === item.path) };
}

function finishGesture(): void {
  wheelGeneration++;
  previewGeneration++;
  stopPreview?.();
  stopPreview = null;
  previewWindow = null;
  hovered = null;
  overlay?.close();
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
  windowScan = listWindows(snapshot.wheel).then((value) => {
    if (snapshot !== settings) return;
    windowState = value;
    items = items.map(withWindows);
    if (overlay && overlay.geometry.count > 0 && hovered === null) {
      activeItems = activeItems.map(withWindows);
      overlay.update(activeItems, settings.wheelSizePx, page);
      overlay.hover(hovered);
    }
    settingsWin?.webContents.send('settings:changed', stateSnapshot());
  }).finally(() => { windowScan = null; if (snapshot !== settings) void refreshWindows(); });
  return windowScan;
}

function applySettings(next: Settings): void {
  finishGesture();
  settings = sanitize(next);
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
    screenPermission: windowState.screen,
  };
}

function onHelperEvent(ev: HelperEvent): void {
  switch (ev.type) {
    case 'scroll': {
      previewGeneration++;
      stopPreview?.();
      stopPreview = null;
      previewWindow = null;
      overlay?.preview(null);
      if (!overlay || overlay.geometry.count === 0 || !ev.delta) break;
      page = (page + (ev.delta < 0 ? 1 : 2)) % 3;
      activeItems = items.slice(page * 8, page * 8 + 8);
      while (activeItems.length < 8) activeItems.push({ name: '', path: '', icon: '' });
      // Keep the centre fixed while switching pages during a held gesture.
      overlay.update(activeItems, settings.wheelSizePx, page);
      hovered = overlay.slotAt(ev.x, ev.y);
      overlay.hover(hovered);
      break;
    }
    case 'wheelOpen': {
      finishGesture();
      if (screenLocked || capturing) return;
      void refreshWindows();
      if (!items.some((item) => item.path)) {
        // Nothing pinned yet: no empty ring to stare at. The tray menu opens settings.
        return;
      }
      hovered = null;
      activeItems = items.slice(page * 8, page * 8 + 8);
      overlay?.open(ev.x, ev.y, activeItems, settings.wheelSizePx, page);
      break;
    }
    case 'wheelMove': {
      const choice = overlay?.selectAt(ev.x, ev.y);
      hovered = choice?.slot ?? null;
      const selected = choice?.window ?? null;
      if ((selected?.id ?? null) !== previewWindow) {
        previewWindow = selected?.id ?? null;
        const generation = ++previewGeneration;
        stopPreview?.();
        stopPreview = null;
        overlay?.preview(null);
        if (selected && settings.windowPreviews) {
          stopPreview = watchWindowPreview(selected, (result) => {
            if (generation === previewGeneration && overlay && overlay.geometry.count > 0)
              overlay.preview(selected, result.image, result.cached);
          });
        }
      }
      break;
    }
    case 'wheelRelease': {
      const choice = overlay?.selectAt(ev.x, ev.y);
      const i = choice?.slot ?? null;
      const picked = i === null ? undefined : activeItems[i];
      finishGesture();
      if (choice?.window) void activateWindow(choice.window).then((ok) => { if (!ok && !screenLocked && !quitting) overlay?.toast('这个窗口已关闭或暂时无法切换，请重新选择。'); });
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
    width: 1040,
    height: 760,
    minWidth: 900,
    minHeight: 640,
    title: 'ViGo',
    titleBarStyle: 'hiddenInset',
    backgroundColor: '#00000000',
    vibrancy: 'sidebar',
    visualEffectState: 'active',
    webPreferences: {
      preload: join(__dirname, '../preload/settings.js'),
      contextIsolation: true,
      sandbox: false,
    },
  });
  void loadPage(settingsWin, 'settings');
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
  page = Math.max(0, Math.min(2, Math.floor(nextPage)));
  if (!items.some((item) => item.path)) {
    void dialog.showMessageBox({ message: '轮盘还是空的', detail: '先在设置里把一个动作拖到轮盘上。' });
    openSettings();
    return;
  }
  const p = screen.getCursorScreenPoint();
  hovered = null;
  activeItems = items.slice(page * 8, page * 8 + 8);
  overlay?.open(p.x, p.y, activeItems, settings.wheelSizePx, page);
  overlay?.hover(activeItems.findIndex((item) => item.path) >= 0 ? activeItems.findIndex((item) => item.path) : null);
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
  ipcMain.handle('settings:get', () => ({ ...stateSnapshot(), installed: installedApps() }));
  ipcMain.handle('settings:save', (_e, next: unknown) => {
    applySettings(next as Settings);
    return stateSnapshot();
  });
  ipcMain.handle('settings:icon', (_e, path: string) => appIcon(path));
  ipcMain.handle('settings:open-accessibility', () => requestAccessibility());
  ipcMain.handle('settings:preview-wheel', (_e, nextPage: unknown) => previewWheel(typeof nextPage === 'number' ? nextPage : 0));
  ipcMain.handle('settings:quit', () => app.quit());
  ipcMain.handle('settings:shortcuts', () => availableShortcuts());
  ipcMain.handle('settings:window-preview-permission', async () => {
    if (!windowState.screen) {
      if (!screenRequested) { screenRequested = true; await requestScreenPreview(); }
      void shell.openExternal('x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture');
    }
    await refreshWindows();
    return stateSnapshot();
  });
  ipcMain.handle('settings:browse', async () => (await dialog.showOpenDialog({
    title: '添加到 ViGo', properties: ['openFile', 'openDirectory', 'multiSelections'],
  })).filePaths);
  ipcMain.handle('settings:capturing', (_e, on: unknown) => {
    if (typeof on === 'boolean') { capturing = on; if (on) finishGesture(); syncSuspended(); }
  });
}

const gotLock = app.requestSingleInstanceLock();
if (!gotLock) {
  app.quit();
} else {
  app.on('second-instance', () => { void app.whenReady().then(openSettings); });
  app.on('activate', () => { if (app.isReady()) openSettings(); });

  void app.whenReady().then(async () => {
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
    await refreshItems();
    createTray();
    screenLocked = powerMonitor.getSystemIdleState(60) === 'locked';
    powerMonitor.on('lock-screen', () => { screenLocked = true; finishGesture(); syncSuspended(); });
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
    windowTimer = setInterval(() => { void refreshWindows(); }, 4000);
    windowTimer.unref();
    if (!items.some((item) => item.path)) {
      // First run: seed the ring with whatever of the usual suspects is installed.
      const installed = new Set(installedApps().map((a) => a.path));
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
