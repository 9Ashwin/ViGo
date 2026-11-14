//! All AppKit state stays on the main thread; Dock and window queries run in isolated workers.
mod menu;
mod wheel;
use crate::{
    input::{self, Effect, Gesture},
    layout::{self, Rect},
    model::{Content, Settings, Tab, Trigger, Window},
    platform::{self, Message},
};
use objc2::{
    define_class, msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyClass, AnyObject, ProtocolObject},
    sel, AllocAnyThread, ClassType, MainThreadOnly,
};
use objc2_app_kit::*;
use objc2_foundation::*;
use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::{c_void, CString},
    ptr,
    sync::mpsc::{self, Receiver, Sender},
    time::{Instant, SystemTime},
};
thread_local! {static UI:RefCell<Option<State>>=const{RefCell::new(None)};}
fn with(f: impl FnOnce(&mut State)) {
    UI.with(|c| {
        if let Ok(mut s) = c.try_borrow_mut() {
            if let Some(s) = s.as_mut() {
                f(s);
            }
        }
    });
}
fn rect(r: Rect) -> NSRect {
    NSRect::new(NSPoint::new(r.x, r.y), NSSize::new(r.w, r.h))
}
fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect { x, y, w, h }
}
unsafe fn class(name: &str) -> &'static AnyClass {
    AnyClass::get(&CString::new(name).unwrap()).expect("macOS class")
}
unsafe fn native(name: &str, frame: Rect) -> Retained<NSView> {
    let v: Allocated<NSView> = msg_send![class(name), alloc];
    msg_send![v,initWithFrame:rect(frame)]
}
unsafe fn color(name: &str) -> Retained<NSColor> {
    let selector = objc2::runtime::Sel::register(&CString::new(name).unwrap());
    msg_send![NSColor::class(),performSelector:selector]
}
unsafe fn fill(path: &NSBezierPath, c: &NSColor) {
    c.setFill();
    path.fill();
}
unsafe fn round(frame: Rect, radius: f64, c: &NSColor) {
    fill(
        &NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(rect(frame), radius, radius),
        c,
    );
}
unsafe fn text(value: &str, frame: Rect, size: f64, bold: bool, c: &NSColor) {
    let font = NSFont::systemFontOfSize_weight(size, if bold { 0.45 } else { 0. });
    let para = NSMutableParagraphStyle::new();
    para.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    let attrs = NSDictionary::<NSString, AnyObject>::from_slices(
        &[
            ns_string!("NSFont"),
            ns_string!("NSColor"),
            ns_string!("NSParagraphStyle"),
        ],
        &[&*font, &**c, &*para],
    );
    let s = NSString::from_str(value);
    let _: () = msg_send![&*s,drawInRect:rect(frame),withAttributes:&*attrs];
}
unsafe fn image(img: &NSImage, frame: Rect) {
    img.drawInRect_fromRect_operation_fraction_respectFlipped_hints(
        rect(frame),
        NSRect::ZERO,
        NSCompositingOperation::SourceOver,
        1.,
        true,
        None,
    );
}
unsafe fn label(parent: &NSView, value: &str, frame: Rect, size: f64, bold: bool, secondary: bool) {
    let mtm = MainThreadMarker::new().unwrap();
    let v = NSTextField::labelWithString(&NSString::from_str(value), mtm);
    v.setFrame(rect(frame));
    if frame.y > 100. {
        v.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    }
    v.setFont(Some(&NSFont::systemFontOfSize_weight(
        size,
        if bold { 0.45 } else { 0. },
    )));
    v.setTextColor(Some(&color(if secondary {
        "secondaryLabelColor"
    } else {
        "labelColor"
    })));
    parent.addSubview(&v);
}
define_class!(#[unsafe(super(NSView))]#[thread_kind=MainThreadOnly]struct Canvas;impl Canvas{#[unsafe(method(isFlipped))]fn flipped(&self)->bool{true}#[unsafe(method(drawRect:))]fn draw(&self,_r:NSRect){with(|s|unsafe{s.draw_wheel()});}});
impl Canvas {
    unsafe fn new(frame: Rect) -> Retained<Self> {
        let v = Self::alloc(MainThreadMarker::new().unwrap());
        msg_send![v,initWithFrame:rect(frame)]
    }
}
define_class!(#[unsafe(super(NSObject))]#[thread_kind=MainThreadOnly]struct Delegate;
 unsafe impl NSObjectProtocol for Delegate{}
 unsafe impl NSApplicationDelegate for Delegate{
  #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]fn reopen(&self,_a:&NSApplication,_v:bool)->bool{open_menu();true}
  #[unsafe(method(applicationWillTerminate:))]fn quit(&self,_n:&NSNotification){with(|s|s.stop());}
 }
 unsafe impl NSMenuDelegate for Delegate{#[unsafe(method(menuNeedsUpdate:))]fn menu_update(&self,_m:&NSMenu){with(|s|unsafe{s.rebuild_menu()});}}
 unsafe impl NSWindowDelegate for Delegate{
  #[unsafe(method(windowShouldClose:))]fn close(&self,w:&NSWindow)->bool{with(|s|{s.capturing=false;w.orderOut(None);});false}
  #[unsafe(method(windowDidResignKey:))]fn resign(&self,_n:&NSNotification){with(|s|unsafe{if s.capturing{s.capturing=false;s.dismiss_capture();}});}
 }
 impl Delegate{
  #[unsafe(method(action:))]fn action(&self,sender:&AnyObject){let tag:isize=unsafe{msg_send![sender,tag]};with(|s|unsafe{s.action(tag,sender)});}
  #[unsafe(method(tick:))]fn tick(&self,_t:&NSTimer){with(|s|unsafe{s.tick()});}
  #[unsafe(method(dockChanged:))]fn dock_changed(&self,n:&NSNotification){let regular=unsafe{n.userInfo().and_then(|d|d.objectForKey(NSWorkspaceApplicationKey)).is_some_and(|a|{let policy:isize=msg_send![&*a,activationPolicy];policy==0})};if regular{with(|s|{s.dock_due=s.now()+0.35;});}}
 }
);
struct State {
    app: Retained<NSApplication>,
    delegate: Retained<Delegate>,
    settings: Settings,
    overlay: Retained<NSWindow>,
    canvas: Retained<Canvas>,
    glasses: Vec<Retained<NSView>>,
    status: Retained<NSStatusItem>,
    status_menu: Retained<NSMenu>,
    capture_window: Option<Retained<NSWindow>>,
    menu_dock_label: Option<Retained<NSTextField>>,
    screen_frame: NSRect,
    primary_height: f64,
    icons: HashMap<String, Retained<NSImage>>,
    sheet: Retained<NSImage>,
    windows: Vec<Window>,
    tabs: Vec<Tab>,
    running: Vec<String>,
    tx: Sender<Message>,
    rx: Receiver<Message>,
    gesture: Gesture,
    tap: platform::Ref,
    source: platform::Ref,
    anim_timer: Option<Retained<NSTimer>>,
    birth: Instant,
    active: bool,
    center: (f64, f64),
    pointer: (f64, f64),
    selected: Option<usize>,
    entries: Vec<Content>,
    chips: Vec<Rect>,
    selected_chip: Option<usize>,
    ring: layout::DockLayout,
    anchor: (f64, f64),
    render_size: f64,
    opened: f64,
    closing: Option<f64>,
    trial: Option<f64>,
    scan_pending: bool,
    last_scan: f64,
    last_frame: usize,
    last_motion: f64,
    invocations: usize,
    armed: bool,
    hover_mix: Vec<f64>,
    capturing: bool,
    capture_dirty: bool,
    pending_key: platform::Ref,
    capture_release: Option<u16>,
    trusted: bool,
    tap_attempted: bool,
    locked: bool,
    notice: String,
    toast: Option<(Retained<NSWindow>, f64)>,
    wake: platform::Ref,
    effects: Vec<Effect>,
    moved: bool,
    layout_dirty: bool,
    dock_observer: platform::Ref,
    dock_observer_pid: i32,
    observer_retry: f64,
    observer_failures: u32,
    dock_failures: u32,
    dock_observed: Vec<platform::Ref>,
    reduce_motion: bool,
    dock_pending: bool,
    dock_exact: bool,
    last_dock: f64,
    dock_due: f64,
    dock_stamp: Option<SystemTime>,
    active_paths: Vec<String>,
    active_names: std::collections::BTreeMap<String, String>,
    #[cfg(feature = "diagnostics")]
    diag: Option<std::path::PathBuf>,
    #[cfg(feature = "diagnostics")]
    diag_ack: u64,
}
unsafe fn open_url(url: &str) {
    if let Some(url) = NSURL::URLWithString(&NSString::from_str(url)) {
        NSWorkspace::sharedWorkspace().openURL(&url);
    }
}
unsafe fn session_locked() -> bool {
    let raw = platform::CGSessionCopyCurrentDictionary();
    if raw.is_null() {
        return true;
    }
    let dict = &*(raw as *const NSDictionary<NSString, AnyObject>);
    let get = |key: &str| -> bool {
        dict.objectForKey(&NSString::from_str(key))
            .is_some_and(|v| {
                let b: bool = msg_send![&*v, boolValue];
                b
            })
    };
    let locked = get("CGSSessionScreenIsLocked")
        || !get("kCGSSessionOnConsoleKey")
        || !get("kCGSessionLoginDoneKey");
    platform::CFRelease(raw);
    locked
}
extern "C" fn dock_changed(
    _observer: platform::Ref,
    _element: platform::Ref,
    _name: platform::Ref,
    _info: platform::Ref,
) {
    with(|s| unsafe {
        s.dock_due = s.now();
        wake(s);
    });
}
unsafe fn wake(s: &State) {
    platform::CFRunLoopSourceSignal(s.wake);
    platform::CFRunLoopWakeUp(platform::CFRunLoopGetMain());
}
extern "C" fn pump(_info: *mut c_void) {
    with(|s| unsafe { s.tick() });
}
extern "C" fn event_hook(
    proxy: platform::Ref,
    kind: u32,
    event: platform::Ref,
    _info: platform::Ref,
) -> platform::Ref {
    let mut suppress = false;
    with(|s| unsafe {
        if kind >= 0xffff_fffe {
            if s.trusted && !s.locked {
                platform::CGEventTapEnable(s.tap, true);
            }
            s.gesture.reset();
            s.effects.push(Effect::Cancel);
            wake(s);
            return;
        }
        if platform::CGEventGetIntegerValueField(event, 42) == platform::REPLAY {
            return;
        }
        let code = platform::CGEventGetIntegerValueField(event, 9) as u16;
        let flags = platform::CGEventGetFlags(event);
        let modifiers = flags & 0x9e0000 != 0;
        if let Some(release) = s.capture_release {
            if code == release && (kind == 10 || kind == 11) {
                if kind == 11 {
                    s.capture_release = None;
                    s.gesture.reset();
                }
                suppress = true;
                return;
            }
        }
        if s.capturing && kind == 10 && code == 53 {
            s.capturing = false;
            s.capture_release = Some(code);
            s.gesture.reset();
            s.capture_dirty = true;
            suppress = true;
            wake(s);
            return;
        }
        if s.capturing && kind == 10 {
            if let Some(vk) = input::mac_to_vk(code).filter(|v| input::safe_trigger(*v)) {
                s.settings.trigger = Trigger::Key { vk };
                s.capturing = false;
                s.capture_release = Some(code);
                s.effects.push(Effect::Cancel);
                s.notice = "呼出键已更新".into();
                s.capture_dirty = true;
            }
            suppress = true;
            wake(s);
            return;
        }
        if !s.settings.enabled_on_start || s.locked {
            return;
        }
        let trigger = match &s.settings.trigger {
            Trigger::Key { vk } => {
                input::vk_to_mac(*vk) == Some(code) && (kind == 10 || kind == 11)
            }
            Trigger::Mouse(name) => {
                (kind == 25 || kind == 26)
                    && platform::CGEventGetIntegerValueField(event, 3)
                        == if name == "mouse4" { 3 } else { 4 }
            }
        };
        if trigger {
            // Repeats after a modal, pause or modified chord are ordinary input.
            if kind == 10 && !s.gesture.held && platform::CGEventGetIntegerValueField(event, 8) != 0
            {
                return;
            }
            if kind == 10 || kind == 25 {
                if !modifiers || s.gesture.held {
                    if !s.gesture.held && kind == 10 {
                        if !s.pending_key.is_null() {
                            platform::CFRelease(s.pending_key);
                        }
                        s.pending_key = platform::CGEventCreateCopy(event);
                    }
                    s.gesture.down(s.now());
                    suppress = true;
                }
            } else if s.gesture.held {
                let effect = s.gesture.up();
                if effect == Effect::Replay {
                    platform::replay(
                        proxy,
                        std::mem::replace(&mut s.pending_key, ptr::null_mut()),
                    );
                } else {
                    s.effects.push(effect);
                }
                suppress = true;
            }
        } else if kind == 10 || kind == 12 {
            if kind == 10 && code == 53 && s.active {
                s.gesture.cancel();
                s.effects.push(Effect::Cancel);
                suppress = true;
            } else if s.gesture.held {
                let effect = s.gesture.interrupt();
                if effect == Effect::Replay {
                    platform::replay(
                        proxy,
                        std::mem::replace(&mut s.pending_key, ptr::null_mut()),
                    );
                } else {
                    s.effects.push(effect);
                }
            }
        }
        if kind == 3 && s.active {
            s.gesture.cancel();
            s.effects.push(Effect::Cancel);
            suppress = true;
        }
        if matches!(kind, 5 | 6 | 7 | 27) && s.active {
            let p = platform::CGEventGetLocation(event);
            let bounds = s.screen_frame;
            s.pointer = (
                p.x - bounds.origin.x,
                bounds.size.height - (s.primary_height - p.y - bounds.origin.y),
            );
            s.last_motion = s.now();
            s.moved = true;
        }
        if kind == 22 && s.active {
            suppress = true;
        }
        if s.gesture.held || s.active || !s.effects.is_empty() || s.capture_dirty || s.moved {
            wake(s);
        }
    });
    if suppress {
        ptr::null_mut()
    } else {
        event
    }
}

#[link(name = "ServiceManagement", kind = "framework")]
extern "C" {}
fn open_menu() {
    let state = UI.with(|c| {
        let mut state = c.try_borrow_mut().ok()?;
        let s = state.as_mut()?;
        unsafe {
            s.rebuild_menu();
        }
        Some((s.status.clone(), s.status_menu.clone()))
    });
    if let Some((status, menu)) = state {
        #[allow(deprecated)]
        status.popUpStatusItemMenu(&menu);
    }
}
pub fn run() {
    unsafe {
        let _ = std::fs::create_dir_all(Settings::path().parent().unwrap());
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(Settings::path().with_extension("native-lock"))
            .ok();
        use std::os::fd::AsRawFd;
        if let Some(lock) = &lock {
            if libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) != 0 {
                return;
            }
        }
        let mtm = MainThreadMarker::new().unwrap();
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        let delegate: Retained<Delegate> = msg_send![Delegate::alloc(mtm), init];
        app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        let (settings, notice) = Settings::load();
        let allocation: Allocated<NSWindow> = msg_send![class("NSPanel"), alloc];
        let overlay = NSWindow::initWithContentRect_styleMask_backing_defer(
            allocation,
            NSRect::new(NSPoint::ZERO, NSSize::new(800., 800.)),
            NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
            NSBackingStoreType::Buffered,
            false,
        );
        overlay.setReleasedWhenClosed(false);
        overlay.setOpaque(false);
        overlay.setBackgroundColor(Some(&NSColor::clearColor()));
        overlay.setHasShadow(false);
        overlay.setIgnoresMouseEvents(true);
        overlay.setLevel(1000);
        overlay.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::CanJoinAllApplications,
        );
        let root = native("NSView", r(0., 0., 800., 800.));
        let canvas = Canvas::new(r(0., 0., 800., 800.));
        root.addSubview(&canvas);
        overlay.setContentView(Some(&root));
        let status = NSStatusBar::systemStatusBar().statusItemWithLength(-1.);
        let menu = NSMenu::new(mtm);
        menu.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        status.setMenu(Some(&menu));
        if let Some(button) = status.button(mtm) {
            if let Some(icon) = NSImage::initWithContentsOfFile(
                NSImage::alloc(),
                &NSString::from_str(&platform::resource("trayTemplate.png").to_string_lossy()),
            ) {
                icon.setTemplate(true);
                button.setImage(Some(&icon));
            }
            button.setToolTip(Some(ns_string!("ViGo · Dock 同步轮盘")));
        }
        let sheet = NSImage::initWithContentsOfFile(
            NSImage::alloc(),
            &NSString::from_str(&platform::resource("mascot-sheet.png").to_string_lossy()),
        )
        .expect("Bundled mascot");
        let (tx, rx) = mpsc::channel();
        let mut context = platform::SourceContext {
            version: 0,
            info: ptr::null_mut(),
            retain: ptr::null_mut(),
            release: ptr::null_mut(),
            description: ptr::null_mut(),
            equal: ptr::null_mut(),
            hash: ptr::null_mut(),
            schedule: ptr::null_mut(),
            cancel: ptr::null_mut(),
            perform: pump as *mut c_void,
        };
        let wake = platform::CFRunLoopSourceCreate(ptr::null_mut(), 0, &mut context);
        platform::CFRunLoopAddSource(
            platform::CFRunLoopGetMain(),
            wake,
            platform::kCFRunLoopCommonModes,
        );
        let mut s = State {
            app: app.clone(),
            delegate: delegate.clone(),
            settings,
            overlay,
            canvas,
            glasses: vec![],
            status,
            status_menu: menu,
            capture_window: None,
            menu_dock_label: None,
            screen_frame: NSRect::ZERO,
            primary_height: 0.,
            icons: HashMap::new(),
            sheet,
            windows: vec![],
            tabs: vec![],
            running: vec![],
            tx,
            rx,
            gesture: Gesture::default(),
            tap: ptr::null_mut(),
            source: ptr::null_mut(),
            anim_timer: None,
            birth: Instant::now(),
            active: false,
            center: (0., 0.),
            pointer: (0., 0.),
            selected: None,
            entries: vec![],
            chips: vec![],
            selected_chip: None,
            ring: layout::DockLayout::new(0, 520., 1470., 956.),
            anchor: (0., 0.),
            render_size: 520.,
            opened: 0.,
            closing: None,
            trial: None,
            scan_pending: false,
            last_scan: 0.,
            last_frame: 0,
            last_motion: 0.,
            invocations: 0,
            armed: false,
            hover_mix: vec![],
            capturing: false,
            capture_dirty: false,
            pending_key: ptr::null_mut(),
            capture_release: None,
            trusted: platform::AXIsProcessTrusted(),
            tap_attempted: false,
            locked: session_locked(),
            notice,
            toast: None,
            wake,
            effects: vec![],
            moved: false,
            layout_dirty: false,
            dock_observer: ptr::null_mut(),
            dock_observer_pid: 0,
            observer_retry: 0.,
            observer_failures: 0,
            dock_failures: 0,
            dock_observed: vec![],
            reduce_motion: false,
            dock_pending: false,
            dock_exact: false,
            last_dock: -10.,
            dock_due: 0.,
            dock_stamp: None,
            active_paths: vec![],
            active_names: std::collections::BTreeMap::new(),
            #[cfg(feature = "diagnostics")]
            diag: std::env::args()
                .collect::<Vec<_>>()
                .windows(2)
                .find(|v| v[0] == "--diagnostic-dir")
                .map(|v| std::path::PathBuf::from(&v[1])),
            #[cfg(feature = "diagnostics")]
            diag_ack: 0,
        };
        s.apply_theme();
        if s.trusted && !s.locked {
            s.install_hook();
        }
        s.refresh_dock();
        s.rebuild_menu();
        let center = NSWorkspace::sharedWorkspace().notificationCenter();
        for name in [
            NSWorkspaceDidLaunchApplicationNotification,
            NSWorkspaceDidTerminateApplicationNotification,
        ] {
            center.addObserver_selector_name_object(
                &delegate,
                sel!(dockChanged:),
                Some(name),
                None,
            );
        }
        UI.with(|c| *c.borrow_mut() = Some(s));
        let timer = NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
            0.5,
            &delegate,
            sel!(tick:),
            None,
            true,
        );
        timer.setTolerance(0.1);
        NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes);
        let _keep = (lock, timer);
        app.run();
    }
}
unsafe fn center_text(value: &str, frame: Rect, size: f64, bold: bool, c: &NSColor) {
    let font = NSFont::systemFontOfSize_weight(size, if bold { 0.35 } else { 0. });
    let p = NSMutableParagraphStyle::new();
    p.setAlignment(NSTextAlignment::Center);
    p.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
    let a = NSDictionary::<NSString, AnyObject>::from_slices(
        &[
            ns_string!("NSFont"),
            ns_string!("NSColor"),
            ns_string!("NSParagraphStyle"),
        ],
        &[&*font, &**c, &*p],
    );
    let s = NSString::from_str(value);
    let _: () = msg_send![&*s,drawInRect:rect(frame),withAttributes:&*a];
}
#[cfg(feature = "diagnostics")]
impl State {
    unsafe fn diagnostics(&mut self) {
        let Some(dir) = self.diag.clone() else { return };
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join("command.json");
        if let Ok(b) = std::fs::read(&p) {
            let _ = std::fs::remove_file(p);
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&b) {
                self.diag_ack = v["id"].as_u64().unwrap_or(0);
                match v["command"].as_str().unwrap_or("") {
                    "refresh" => self.refresh_dock(),
                    "action" => self.action(
                        v["tag"].as_i64().unwrap_or(-1) as isize,
                        &self.delegate.clone(),
                    ),
                    "trial" => {
                        let f = NSScreen::mainScreen(MainThreadMarker::new().unwrap())
                            .unwrap()
                            .frame();
                        self.open_wheel(
                            v["x"].as_f64().unwrap_or(f.size.width / 2.),
                            v["y"].as_f64().unwrap_or(f.size.height / 2.),
                        );
                        self.trial = Some(self.now() + 3.);
                    }
                    "screenshot" => {
                        let rect = v["rect"].as_str().unwrap_or("940,28,380,640");
                        let _ = std::process::Command::new("/usr/sbin/screencapture")
                            .args(["-x", "-R", rect])
                            .arg(dir.join("menu.png"))
                            .spawn();
                    }
                    "record" => {
                        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                        let _ = std::process::Command::new(
                            root.join(".local/verification/dock-record-capture"),
                        )
                        .arg(root.join(".local/verification/dock-demo-frames"))
                        .stdout(std::fs::File::create(dir.join("recording.jsonl")).unwrap())
                        .spawn();
                    }
                    "quit" => self.app.terminate(None),
                    _ => {}
                }
            }
        }
        let v = serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"ack":self.diag_ack,"mode":"dock","dock":self.settings.wheel,"dockExact":self.dock_exact,"dockPending":self.dock_pending,"activePaths":self.active_paths,"trusted":self.trusted,"tap":!self.tap.is_null(),"active":self.active,"capturing":self.capturing,"selected":self.selected,"selectedChip":self.selected_chip,"armed":self.armed,"items":self.active_paths.len(),"nodeCenters":self.ring.nodes.iter().map(|n|[self.center.0+n.angle.cos()*n.radius,self.center.1+n.angle.sin()*n.radius,self.ring.icon]).collect::<Vec<_>>(),"rings":self.ring.nodes.iter().map(|n|n.ring).max().unwrap_or(0)+1,"center":[self.center.0,self.center.1],"chips":self.chips.iter().map(|c|[c.x,c.y,c.w,c.h]).collect::<Vec<_>>(),"entries":self.entries.iter().map(|e|match e{Content::Window(w)=>&w.id,Content::Tab(t)=>&t.id}).collect::<Vec<_>>(),"notice":self.notice});
        let tmp = dir.join("state.tmp");
        let _ = std::fs::write(&tmp, serde_json::to_vec(&v).unwrap());
        let _ = std::fs::rename(tmp, dir.join("state.json"));
    }
}
