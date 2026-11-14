//! Native Windows 11 utility. UIA and low-level hooks use dedicated threads; rendering never owns input.
mod render;
mod taskbar;
use crate::{
    input::{Effect, Gesture},
    layout::{self, DockLayout},
    model::Settings,
};
use std::{
    cell::RefCell,
    ffi::c_void,
    ptr,
    sync::{
        atomic::{AtomicBool, AtomicIsize, AtomicU64, AtomicUsize, Ordering},
        mpsc, OnceLock,
    },
    time::{Duration, Instant},
};
use windows::{
    core::{w, PCWSTR, PWSTR},
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        Storage::FileSystem::*,
        System::{LibraryLoader::*, Threading::*},
        UI::{Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
    },
};
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
const REPLAY: usize = 0x5649474f;
const WAKE: u32 = WM_APP + 1;
const TRAY: u32 = WM_APP + 2;
static HWND_UI: AtomicIsize = AtomicIsize::new(0);
static ENABLED: AtomicBool = AtomicBool::new(true);
static SESSION: AtomicU64 = AtomicU64::new(0);
static ACTIVE: AtomicBool = AtomicBool::new(false);
static REPLAY_COUNT: AtomicUsize = AtomicUsize::new(0);
static LOCKED: AtomicBool = AtomicBool::new(false);
static BUS: OnceLock<mpsc::Sender<Event>> = OnceLock::new();
static INJECT: OnceLock<mpsc::Sender<Vec<Key>>> = OnceLock::new();
#[derive(Clone)]
struct Key {
    vk: u16,
    scan: u16,
    up: bool,
    extended: bool,
}
enum Event {
    Open(POINT),
    Move(POINT),
    Commit,
    Cancel,
}
fn send(e: Event) {
    if let Some(tx) = BUS.get() {
        let _ = tx.send(e);
        unsafe {
            let _ = PostMessageW(
                HWND(HWND_UI.load(Ordering::Relaxed) as *mut c_void),
                WAKE,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}
#[derive(Default)]
struct Hook {
    gesture: Gesture,
    start: Option<Instant>,
    pending: Option<Key>,
    timer: usize,
    modifiers: std::collections::BTreeSet<u32>,
}
thread_local! {static HOOK:RefCell<Hook>=RefCell::new(Hook::default());static APP:RefCell<Option<App>>=const{RefCell::new(None)};}
fn with(f: impl FnOnce(&mut App)) {
    APP.with(|a| {
        if let Ok(mut s) = a.try_borrow_mut() {
            if let Some(s) = s.as_mut() {
                f(s)
            }
        }
    });
}
unsafe fn replay_allowed() -> bool {
    use windows::Win32::Security::*;
    let fg = GetForegroundWindow();
    let mut pid = 0;
    GetWindowThreadProcessId(fg, Some(&mut pid));
    let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
        return false;
    };
    let mut token = HANDLE::default();
    let mut elevation = TOKEN_ELEVATION::default();
    let mut bytes = 0;
    let ok = OpenProcessToken(process, TOKEN_QUERY, &mut token).is_ok()
        && GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut bytes,
        )
        .is_ok();
    let _ = CloseHandle(token);
    let _ = CloseHandle(process);
    ok && elevation.TokenIsElevated == 0
}
unsafe extern "system" fn keyboard(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code < 0 {
        return CallNextHookEx(None, code, w, l);
    }
    let raw = &*(l.0 as *const KBDLLHOOKSTRUCT);
    if raw.dwExtraInfo == REPLAY {
        let _ = REPLAY_COUNT.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1));
        return CallNextHookEx(None, code, w, l);
    }
    let down = w.0 as u32 == WM_KEYDOWN || w.0 as u32 == WM_SYSKEYDOWN;
    let up = w.0 as u32 == WM_KEYUP || w.0 as u32 == WM_SYSKEYUP;
    let key = Key {
        vk: raw.vkCode as u16,
        scan: raw.scanCode as u16,
        up,
        extended: raw.flags.0 & 1 != 0,
    };
    let mut swallow = false;
    HOOK.with(|cell| {
        let mut h = cell.borrow_mut();
        // Track physical modifiers before replay; async OS state lags suppressed input.
        if [
            0x10, 0x11, 0x12, 0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0x5b, 0x5c,
        ]
        .contains(&raw.vkCode)
        {
            if down {
                h.modifiers.insert(raw.vkCode);
            }
            if up {
                h.modifiers.remove(&raw.vkCode);
            }
        }
        if !ENABLED.load(Ordering::Relaxed) || LOCKED.load(Ordering::Relaxed) {
            h.gesture.reset();
            h.pending = None;
            h.start = None;
            if h.timer != 0 {
                let _ = KillTimer(None, h.timer);
                h.timer = 0;
            }
            send(Event::Cancel);
            return;
        }
        if raw.vkCode == 86 {
            if down {
                if h.gesture.held || (h.modifiers.is_empty() && replay_allowed()) {
                    if !h.gesture.held {
                        h.start = Some(Instant::now());
                        h.pending = Some(key.clone());
                    }
                    h.gesture.down(0.);
                    if h.timer == 0 && !h.gesture.open && !h.gesture.cancelled {
                        h.timer = SetTimer(None, 0, 16, None);
                    }
                    swallow = true;
                }
            } else if up && h.gesture.held {
                let effect = h.gesture.up();
                if effect == Effect::Replay {
                    if let Some(mut v) = h.pending.take() {
                        v.up = false;
                        let mut u = v.clone();
                        u.up = true;
                        replay(vec![v, u]);
                    }
                } else if effect == Effect::Select {
                    send(Event::Commit);
                }
                swallow = true;
            }
        } else if down || up {
            if raw.vkCode == 27 && h.gesture.open {
                h.gesture.cancel();
                send(Event::Cancel);
                swallow = true;
            } else if h.gesture.held && !h.gesture.cancelled {
                let effect = h.gesture.interrupt();
                if effect == Effect::Replay {
                    if let Some(mut v) = h.pending.take() {
                        v.up = false;
                        let mut u = v.clone();
                        u.up = true;
                        replay(vec![v, u, key.clone()]);
                        swallow = true;
                    }
                } else if effect == Effect::Cancel {
                    send(Event::Cancel);
                }
            }
        }
        if (!h.gesture.held || h.gesture.cancelled) && h.timer != 0 {
            let _ = KillTimer(None, h.timer);
            h.timer = 0;
        }
    });
    // Apply the replay barrier after deciding the gesture. Include modified V and key-ups.
    if !swallow
        && REPLAY_COUNT.load(Ordering::SeqCst) > 0
        && (down || up)
        && !LOCKED.load(Ordering::Relaxed)
    {
        replay(vec![key]);
        swallow = true;
    }
    if swallow {
        LRESULT(1)
    } else {
        CallNextHookEx(None, code, w, l)
    }
}
unsafe extern "system" fn mouse(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code >= 0 {
        let p = (*(l.0 as *const MSLLHOOKSTRUCT)).pt;
        if w.0 as u32 == WM_MOUSEMOVE && ACTIVE.load(Ordering::Relaxed) {
            send(Event::Move(p));
        }
        if w.0 as u32 == WM_RBUTTONDOWN {
            let open = HOOK.with(|h| h.borrow().gesture.open);
            if open {
                HOOK.with(|h| h.borrow_mut().gesture.cancel());
                send(Event::Cancel);
                return LRESULT(1);
            }
        }
    }
    CallNextHookEx(None, code, w, l)
}
fn replay(keys: Vec<Key>) {
    if let Some(tx) = INJECT.get() {
        let count = keys.len();
        REPLAY_COUNT.fetch_add(count, Ordering::SeqCst);
        if tx.send(keys).is_err() {
            REPLAY_COUNT.store(0, Ordering::SeqCst);
        }
    }
}
fn hooks() {
    let (tx, rx) = mpsc::channel::<Vec<Key>>();
    let _ = INJECT.set(tx);
    std::thread::spawn(move || {
        while let Ok(keys) = rx.recv() {
            if LOCKED.load(Ordering::Relaxed) {
                REPLAY_COUNT.store(0, Ordering::SeqCst);
                continue;
            }
            let inputs: Vec<INPUT> = keys
                .into_iter()
                .map(|k| INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: VIRTUAL_KEY(k.vk),
                            wScan: k.scan,
                            dwFlags: if k.up {
                                KEYEVENTF_KEYUP
                            } else {
                                KEYBD_EVENT_FLAGS(0)
                            } | if k.extended {
                                KEYEVENTF_EXTENDEDKEY
                            } else {
                                KEYBD_EVENT_FLAGS(0)
                            },
                            time: 0,
                            dwExtraInfo: REPLAY,
                        },
                    },
                })
                .collect();
            unsafe {
                let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
                if sent != inputs.len() as u32 {
                    // UIPI/focus can change after initial validation; stop intercepting on failure.
                    REPLAY_COUNT.store(0, Ordering::SeqCst);
                    ENABLED.store(false, Ordering::Relaxed);
                    send(Event::Cancel);
                }
            }
        }
    });
    std::thread::spawn(move || unsafe {
        let instance = GetModuleHandleW(None).unwrap_or_default();
        HOOK.with(|h| {
            let mut h = h.borrow_mut();
            for vk in [0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0x5b, 0x5c] {
                if GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0 {
                    h.modifiers.insert(vk);
                }
            }
        });
        let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), instance, 0);
        let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), instance, 0);
        if keyboard.is_err() || mouse.is_err() {
            send(Event::Cancel);
            return;
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if msg.message == WM_TIMER {
                HOOK.with(|h| {
                    let mut h = h.borrow_mut();
                    if !ENABLED.load(Ordering::Relaxed) || LOCKED.load(Ordering::Relaxed) {
                        h.gesture.reset();
                        h.start = None;
                        h.pending = None;
                        if h.timer != 0 {
                            let _ = KillTimer(None, h.timer);
                            h.timer = 0;
                        }
                        send(Event::Cancel);
                        return;
                    }
                    if let Some(start) = h.start {
                        if h.gesture.tick(start.elapsed().as_secs_f64()) == Effect::Open {
                            let mut p = POINT::default();
                            let _ = GetCursorPos(&mut p);
                            if h.timer != 0 {
                                let _ = KillTimer(None, h.timer);
                                h.timer = 0;
                            }
                            send(Event::Open(p));
                        }
                    }
                });
            } else {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        if let Ok(h) = keyboard {
            let _ = UnhookWindowsHookEx(h);
        }
        if let Ok(h) = mouse {
            let _ = UnhookWindowsHookEx(h);
        }
    });
}
struct App {
    hwnd: HWND,
    items: Vec<taskbar::Item>,
    active_items: Vec<taskbar::Item>,
    selected: Option<usize>,
    details: Vec<taskbar::Detail>,
    chips: Vec<layout::Rect>,
    selected_detail: Option<usize>,
    layout: DockLayout,
    anchor: (f64, f64),
    center: (f64, f64),
    position: POINT,
    pointer: (f64, f64),
    surface: Option<render::Surface>,
    sheet: render::Sheet,
    opened: Instant,
    active: bool,
    notice: String,
    requests: mpsc::Sender<taskbar::Request>,
    updates: mpsc::Receiver<taskbar::Update>,
    events: mpsc::Receiver<Event>,
    tray: NOTIFYICONDATAW,
    settings: Settings,
    icons: render::Icons,
    presented: bool,
}
impl App {
    unsafe fn open(&mut self, p: POINT) {
        if LOCKED.load(Ordering::Relaxed) || !ENABLED.load(Ordering::Relaxed) {
            return;
        }
        let monitor = MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(monitor, &mut info);
        let w = (info.rcWork.right - info.rcWork.left) as f64;
        let h = (info.rcWork.bottom - info.rcWork.top) as f64;
        self.active_items = self.items.clone();
        self.layout = DockLayout::new(self.items.len(), self.settings.wheel_size_px, w, h);
        self.anchor = (
            (p.x - info.rcWork.left) as f64,
            (p.y - info.rcWork.top) as f64,
        );
        self.center = self.layout.center(self.anchor, w, h);
        self.position = POINT {
            x: info.rcWork.left,
            y: info.rcWork.top,
        };
        self.pointer = self.anchor;
        self.surface = render::Surface::new(w as i32, h as i32);
        self.opened = Instant::now();
        self.active = true;
        ACTIVE.store(true, Ordering::Relaxed);
        self.selected = None;
        self.details.clear();
        self.chips.clear();
        self.selected_detail = None;
        self.draw();
        SetTimer(self.hwnd, 1, 33, None);
        let _ = SetWindowPos(
            self.hwnd,
            HWND_TOPMOST,
            self.position.x,
            self.position.y,
            w as i32,
            h as i32,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
    unsafe fn close(&mut self) {
        self.active = false;
        ACTIVE.store(false, Ordering::Relaxed);
        let _ = KillTimer(self.hwnd, 1);
        self.selected = None;
        self.selected_detail = None;
        let _ = ShowWindow(self.hwnd, SW_HIDE);
        self.surface.take();
    }
    unsafe fn move_to(&mut self, p: POINT) {
        if !self.active {
            return;
        }
        let (x, y) = (
            (p.x - self.position.x) as f64,
            (p.y - self.position.y) as f64,
        );
        self.pointer = (x, y);
        let dx = x - self.center.0;
        let dy = y - self.center.1;
        if (x - self.anchor.0).hypot(y - self.anchor.1) < 12. || dx.hypot(dy) < self.layout.hub {
            self.selected = None;
            self.selected_detail = None;
            self.details.clear();
            self.chips.clear();
            self.draw();
            return;
        }
        if let Some(i) = self.chips.iter().position(|r| r.has(x, y)) {
            self.selected_detail = Some(i);
            self.draw();
            return;
        }
        self.selected_detail = None;
        if self.selected.is_some() && dx.hypot(dy) > self.layout.outer - 4. {
            if dx.hypot(dy) > self.layout.outer + 70. {
                self.selected = None;
            }
            self.draw();
            return;
        }
        let next = self.layout.hit(dx, dy, self.selected);
        if next != self.selected {
            self.selected = next;
            self.details.clear();
            self.chips.clear();
            if let Some(i) = next {
                if let Some(item) = self.active_items.get(i) {
                    let _ = self
                        .requests
                        .send(taskbar::Request::Details(item.key.clone()));
                }
            }
        }
        self.draw();
    }
    unsafe fn commit(&mut self) {
        if LOCKED.load(Ordering::Relaxed) || !ENABLED.load(Ordering::Relaxed) {
            self.close();
            return;
        }
        if let Some(d) = self
            .selected_detail
            .and_then(|i| self.details.get(i))
            .cloned()
        {
            let _ = self
                .requests
                .send(taskbar::Request::Select(d, SESSION.load(Ordering::SeqCst)));
        } else if let Some(item) = self.selected.and_then(|i| self.active_items.get(i)) {
            let _ = self.requests.send(taskbar::Request::Activate(
                item.key.clone(),
                SESSION.load(Ordering::SeqCst),
            ));
        }
        self.close();
    }
    unsafe fn tick(&mut self) {
        while let Ok(e) = self.events.try_recv() {
            match e {
                Event::Open(p) => self.open(p),
                Event::Move(p) => self.move_to(p),
                Event::Commit => self.commit(),
                Event::Cancel => self.close(),
            }
        }
        while let Ok(u) = self.updates.try_recv() {
            match u {
                taskbar::Update::Items(items) => {
                    if items == self.items {
                        continue;
                    }
                    if self.active {
                        let key = self
                            .selected
                            .and_then(|i| self.active_items.get(i))
                            .map(|i| i.key.clone());
                        self.active_items = items.clone();
                        // An external change must never turn a chosen child window into app activation.
                        self.selected = if self.selected_detail.is_some() {
                            None
                        } else {
                            key.and_then(|k| items.iter().position(|i| i.key == k))
                        };
                        self.details.clear();
                        self.chips.clear();
                        self.selected_detail = None;
                        let dimensions = self
                            .surface
                            .as_ref()
                            .map(|s| (s.width, s.height))
                            .unwrap_or((1920, 1080));
                        self.layout = DockLayout::new(
                            items.len(),
                            self.settings.wheel_size_px,
                            dimensions.0 as f64,
                            dimensions.1 as f64,
                        );
                        self.center = self.layout.center(
                            self.anchor,
                            dimensions.0 as f64,
                            dimensions.1 as f64,
                        );
                    }
                    self.items = items;
                }
                taskbar::Update::Details(key, details) => {
                    if self.active
                        && self
                            .selected
                            .and_then(|i| self.active_items.get(i))
                            .is_some_and(|i| i.key == key)
                    {
                        self.details = details;
                        if let Some(surface) = &self.surface {
                            self.chips = layout::content(
                                self.center.0,
                                self.center.1,
                                self.layout.outer * 600. / 188.,
                                self.details.len(),
                                surface.width as f64,
                                surface.height as f64,
                                true,
                            );
                        }
                    }
                }
                taskbar::Update::Error(e) => self.notice = e,
            }
        }
        if self.active {
            self.draw();
        }
    }
    unsafe fn draw(&mut self) {
        let Some(surface) = self.surface.as_mut() else {
            return;
        };
        surface.buffer().fill(0);
        let (cx, cy) = self.center;
        surface.circle(cx, cy, self.layout.surface, [236, 242, 250, 226]);
        surface.circle(cx, cy, self.layout.hub, [248, 250, 255, 80]);
        for (i, n) in self.layout.nodes.iter().enumerate() {
            let x = cx + n.angle.cos() * n.radius;
            let y = cy + n.angle.sin() * n.radius;
            let hot = self.selected == Some(i);
            if hot {
                surface.circle(x, y, self.layout.icon * 0.67, [75, 143, 220, 100]);
            }
            let size = (self.layout.icon * if hot { 1.12 } else { 1. }) as i32;
            if let Some(item) = self.active_items.get(i) {
                self.icons.draw(surface, &item.path, &item.name, x, y, size);
            }
        }
        let row = if self.selected.is_some() {
            if self.pointer.0 < cx {
                2
            } else {
                1
            }
        } else {
            3
        };
        let frame = (self.opened.elapsed().as_secs_f64() * 4.) as usize;
        surface.sprite(&self.sheet, row, frame, cx, cy - 12., 72.);
        let title = self
            .selected
            .and_then(|i| self.active_items.get(i))
            .map(|i| i.name.as_str())
            .unwrap_or("松开取消");
        surface.text(title, cx as i32 - 90, cy as i32 + 34, 180, 24, [27, 39, 52]);
        surface.text(
            "Esc",
            cx as i32 - 30,
            cy as i32 + 63,
            60,
            20,
            [92, 108, 122],
        );
        for (i, (d, r)) in self.details.iter().zip(&self.chips).enumerate() {
            let c = if self.selected_detail == Some(i) {
                [124, 174, 230, 240]
            } else {
                [235, 242, 250, 245]
            };
            for y in r.y as i32..(r.y + r.h) as i32 {
                for x in r.x as i32..(r.x + r.w) as i32 {
                    surface.pixel(x, y, c);
                }
            }
            surface.text(
                &d.title,
                r.x as i32 + 5,
                r.y as i32,
                r.w as i32 - 10,
                r.h as i32,
                [27, 39, 52],
            );
        }
        let t = (self.opened.elapsed().as_secs_f64() / 0.14).clamp(0., 1.);
        self.presented = surface.present(
            self.hwnd,
            self.position.x,
            self.position.y,
            ((1. - (1. - t).powi(3)) * 255.) as u8,
        );
    }
    unsafe fn menu(&mut self) {
        let Ok(menu) = CreatePopupMenu() else { return };
        for (id, title) in [
            (
                1,
                if ENABLED.load(Ordering::Relaxed) {
                    "暂停 ViGo"
                } else {
                    "启用 ViGo"
                },
            ),
            (2, "任务栏状态"),
            (3, "关于 ViGo"),
            (4, "退出 ViGo"),
        ] {
            let _ = AppendMenuW(menu, MF_STRING, id, PCWSTR(wide(title).as_ptr()));
        }
        let mut p = POINT::default();
        let _ = GetCursorPos(&mut p);
        let _ = SetForegroundWindow(self.hwnd);
        let id = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_NONOTIFY,
            p.x,
            p.y,
            0,
            self.hwnd,
            None,
        )
        .0;
        match id {
            1 => {
                let enabled = !ENABLED.load(Ordering::Relaxed);
                SESSION.fetch_add(1, Ordering::SeqCst);
                ENABLED.store(enabled, Ordering::Relaxed);
                self.settings.enabled_on_start = enabled;
                let _ = self.settings.save();
                self.close();
            }
            2 => {
                let text = if self.notice.is_empty() {
                    format!(
                        "已同步 {} 个任务栏项目。\n任务栏变化会自动同步。",
                        self.items.len()
                    )
                } else {
                    self.notice.clone()
                };
                let _ = MessageBoxW(
                    self.hwnd,
                    PCWSTR(wide(&text).as_ptr()),
                    w!("任务栏状态"),
                    MB_OK,
                );
            }
            3 => {
                let _ = MessageBoxW(
                    self.hwnd,
                    w!("ViGo · Rust 原生任务栏轮盘\n按住 V · 移动 · 松开\nWindows 11 x64 初版"),
                    w!("关于 ViGo"),
                    MB_OK,
                );
            }
            4 => {
                let _ = DestroyWindow(self.hwnd);
            }
            _ => {}
        }
        let _ = DestroyMenu(menu);
    }
}
unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WAKE | WM_TIMER => {
            with(|a| a.tick());
            LRESULT(0)
        }
        TRAY => {
            if l.0 as u32 == WM_RBUTTONUP || l.0 as u32 == WM_LBUTTONUP {
                with(|a| a.menu());
            }
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_DESTROY => {
            with(|a| {
                let _ = a.requests.send(taskbar::Request::Stop);
                let _ = Shell_NotifyIconW(NIM_DELETE, &a.tray);
            });
            PostQuitMessage(0);
            LRESULT(0)
        }
        0x02b1 => {
            // Unlock only on WTS_SESSION_UNLOCK; disconnect/logoff are not an unlock.
            let locked = match w.0 {
                8 => false,
                2 | 4 | 6 | 7 => true,
                _ => return LRESULT(0),
            };
            SESSION.fetch_add(1, Ordering::SeqCst);
            LOCKED.store(locked, Ordering::Relaxed);
            if locked {
                with(|a| a.close());
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}
pub fn run() {
    unsafe {
        let smoke = std::env::args().any(|a| a == "--smoke-test");
        let _ = windows::Win32::System::Com::CoInitializeEx(
            None,
            windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
        );
        let instance = GetModuleHandleW(None).unwrap();
        let class = wide("ViGoNativeWheel");
        let icon = LoadIconW(instance, PCWSTR(std::ptr::without_provenance(1)))
            .or_else(|_| LoadIconW(None, IDI_APPLICATION))
            .unwrap_or_default();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: PCWSTR(class.as_ptr()),
            hIcon: icon,
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            std::process::exit(2)
        }
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            PCWSTR(class.as_ptr()),
            w!("ViGo"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            instance,
            None,
        )
        .expect("native window");
        HWND_UI.store(hwnd.0 as isize, Ordering::Relaxed);
        let _ = windows::Win32::System::RemoteDesktop::WTSRegisterSessionNotification(
            hwnd,
            windows::Win32::System::RemoteDesktop::NOTIFY_FOR_THIS_SESSION,
        );
        let (settings, _) = Settings::load();
        ENABLED.store(settings.enabled_on_start, Ordering::Relaxed);
        let (requests, updates) = taskbar::start();
        let (tx, events) = mpsc::channel();
        let _ = BUS.set(tx);
        let mut tray = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: TRAY,
            hIcon: icon,
            ..Default::default()
        };
        let tip = wide("ViGo · 任务栏轮盘");
        tray.szTip[..tip.len()].copy_from_slice(&tip);
        let tray_ok = Shell_NotifyIconW(NIM_ADD, &tray).as_bool();
        let sheet = render::Sheet::load().expect("embedded cat sprite");
        let mut app = App {
            hwnd,
            items: vec![],
            active_items: vec![],
            selected: None,
            details: vec![],
            chips: vec![],
            selected_detail: None,
            layout: DockLayout::new(0, 520., 1920., 1080.),
            anchor: (0., 0.),
            center: (0., 0.),
            position: POINT::default(),
            pointer: (0., 0.),
            surface: None,
            sheet,
            opened: Instant::now(),
            active: false,
            notice: String::new(),
            requests,
            updates,
            events,
            tray,
            settings,
            icons: render::Icons::default(),
            presented: false,
        };
        if smoke {
            app.items = (0..18)
                .map(|i| taskbar::Item {
                    key: format!("fixture-{i}"),
                    name: format!("Application {i}"),
                    path: "C:\\Windows\\System32\\notepad.exe".into(),
                })
                .collect();
            app.open(POINT { x: 640, y: 420 });
            // Let the entrance reach a visible alpha, then verify the real GDI presentation result.
            std::thread::sleep(Duration::from_millis(160));
            app.draw();
            let visible = IsWindowVisible(hwnd).as_bool();
            let surface_created = app.surface.is_some();
            let pixels = app
                .surface
                .as_mut()
                .map(|s| s.buffer().chunks_exact(4).filter(|p| p[3] != 0).count())
                .unwrap_or(0);
            let presented = app.presented;
            app.close();
            let result = serde_json::json!({"platform":"windows","version":env!("CARGO_PKG_VERSION"),"windowCreated":true,"windowVisible":visible,"surfaceCreated":surface_created,"presentSucceeded":presented,"nonzeroAlphaPixels":pixels,"rendered":visible && presented && pixels>0,"spriteDecoded":true,"trayAdded":tray_ok,"taskbarInteractionTested":false,"fixtureItems":18});
            if let Some(path) = std::env::args().skip_while(|s| s != "--report").nth(1) {
                std::fs::write(path, serde_json::to_vec_pretty(&result).unwrap())
                    .expect("write smoke report");
            }
            let _ = app.requests.send(taskbar::Request::Stop);
            let _ = Shell_NotifyIconW(NIM_DELETE, &app.tray);
            let _ = DestroyWindow(hwnd);
            if !visible || !surface_created || !presented || pixels == 0 {
                std::process::exit(3);
            }
            return;
        }
        APP.with(|s| *s.borrow_mut() = Some(app));
        hooks();
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
