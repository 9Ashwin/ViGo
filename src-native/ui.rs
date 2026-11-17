//! All AppKit state stays on the main thread; Dock queries run in isolated workers.
mod mascot;
mod menu;
mod permissions;
mod preferences;
mod preferences_drag;
mod preferences_wheel;
mod wheel;
mod windows;
use crate::{
    input::{self, Effect, Gesture},
    layout::{self, Rect},
    model::{Settings, Trigger},
    platform::{self, Message},
    tether::{Frame as TetherFrame, Point, Target, Tether},
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
pub(crate) unsafe fn repair_accessibility_permissions() {
    permissions::repair();
}
fn rect(r: Rect) -> NSRect {
    NSRect::new(NSPoint::new(r.x, r.y), NSSize::new(r.w, r.h))
}
#[cfg(feature = "diagnostics")]
unsafe fn capture_wheel_snapshot() {
    let mut target = None;
    with(|s| {
        if let Some(dir) = s.diag.clone() {
            let bounds = if let Some(f) = s.windows.frame {
                let (x, y, side) = s.windows.icon;
                let left = f.x.min(x - side / 2.) - 18.;
                let top = f.y.min(y - side / 2.) - 18.;
                rect(r(
                    left,
                    top,
                    f.x.max(x + side / 2.).max(f.x + f.w) - left + 18.,
                    (f.y + f.h).max(y + side / 2.) - top + 18.,
                ))
            } else {
                s.canvas.bounds()
            };
            target = Some((s.canvas.clone(), dir, bounds));
        }
    });
    if let Some((canvas, dir, bounds)) = target {
        if let Some(rep) = canvas.bitmapImageRepForCachingDisplayInRect(bounds) {
            canvas.cacheDisplayInRect_toBitmapImageRep(bounds, &rep);
            if let Some(data) = rep.representationUsingType_properties(
                NSBitmapImageFileType::PNG,
                &NSDictionary::new(),
            ) {
                let _ = data.writeToURL_atomically(
                    &NSURL::fileURLWithPath(&NSString::from_str(
                        &dir.join("wheel.png").to_string_lossy(),
                    )),
                    true,
                );
            }
        }
    }
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

  #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]fn reopen(&self,_a:&NSApplication,_v:bool)->bool{with(|s|unsafe{s.show_preferences();});true}
  #[unsafe(method(applicationWillTerminate:))]fn quit(&self,_n:&NSNotification){with(|s|s.stop());}
  #[unsafe(method(application:openURLs:))]fn open_urls(&self,_app:&NSApplication,urls:&NSArray<NSURL>){mascot::queue_browser_imports(urls);}
 }
 unsafe impl NSMenuDelegate for Delegate{#[unsafe(method(menuNeedsUpdate:))]fn menu_update(&self,_m:&NSMenu){with(|s|unsafe{s.rebuild_menu()});}}
 unsafe impl NSWindowDelegate for Delegate{
  #[unsafe(method(windowDidResize:))]fn resized(&self,n:&NSNotification){let object=n.object();with(|s|{if s.preferences.as_ref().is_some_and(|p|object.as_deref().is_some_and(|o|std::ptr::eq(o as *const AnyObject,Retained::as_ptr(&p.window) as *const AnyObject))){s.preferences_layout_due=s.now()+0.1;}});}

  #[unsafe(method(windowShouldClose:))]fn close(&self,w:&NSWindow)->bool{let mut close=false;with(|s|unsafe{close=s.close_preferences_or_capture(w);});close}
  #[unsafe(method(windowDidResignKey:))]fn resign(&self,n:&NSNotification){let object=n.object();with(|s|unsafe{if s.capturing && s.capture_window.as_ref().is_some_and(|w|object.as_deref().is_some_and(|o|std::ptr::eq(o as *const AnyObject,Retained::as_ptr(w) as *const AnyObject))){s.capturing=false;s.dismiss_capture();}});}
 }
 impl Delegate{
  #[cfg(feature="diagnostics")]
  #[unsafe(method(wheelSnapshot:))]fn wheel_snapshot(&self,_sender:Option<&AnyObject>){unsafe{capture_wheel_snapshot();}}
  #[unsafe(method(releasePreferences:))]fn release_preferences(&self,_sender:Option<&AnyObject>){with(|s|{if s.preferences.as_ref().is_some_and(|p|!p.window.isVisible()){s.preferences=None;}});}
  #[unsafe(method(mascotSelected:))]fn mascot_selected(&self,sender:&NSPopUpButton){with(|s|unsafe{s.mascot_selected(sender);});}
  #[unsafe(method(mascotImport:))]fn mascot_import(&self,_sender:Option<&AnyObject>){unsafe{mascot::choose_file();}}
  #[unsafe(method(mascotWebsite:))]fn mascot_website(&self,_sender:Option<&AnyObject>){unsafe{mascot::choose_website(None);}}
  #[unsafe(method(mascotTick:))]fn mascot_tick(&self,_timer:&NSTimer){with(|s|unsafe{s.mascot_tick();});}
  #[unsafe(method(settingsPage:))]fn settings_page(&self,sender:&NSButton){with(|s|unsafe{s.preferences_page(sender.tag() as usize);});}
  #[unsafe(method(settingsLayout:))]fn settings_layout(&self,sender:&NSButton){with(|s|unsafe{s.preferences_layout(sender.tag() as usize);});}
  #[unsafe(method(settingsCatalogApp:))]fn settings_catalog_app(&self,sender:&NSButton){with(|s|unsafe{s.preferences_catalog_app(sender);});}
  #[unsafe(method(settingsSearch:))]fn settings_search(&self,_sender:&NSSearchField){with(|s|unsafe{s.refresh_preferences_apps();});}
  #[unsafe(method(settingsApp:))]fn settings_app(&self,sender:&NSButton){with(|s|unsafe{s.preferences_app(sender);});}
  #[unsafe(method(settingsMove:))]fn settings_move(&self,sender:&NSButton){with(|s|unsafe{s.preferences_move(sender);});}
  #[unsafe(method(settingsRemove:))]fn settings_remove(&self,sender:&NSButton){with(|s|unsafe{s.preferences_remove(sender);});}
  #[unsafe(method(chooseApplications:))]fn choose_applications(&self,_sender:Option<&AnyObject>){unsafe{preferences::choose_applications();}}
  #[unsafe(method(settingsTrigger:))]fn settings_trigger(&self,sender:&NSPopUpButton){with(|s|unsafe{s.action([100,101,102,103][sender.indexOfSelectedItem().clamp(0,3) as usize],sender);});}
  #[unsafe(method(settingsSize:))]fn settings_size(&self,sender:&NSSlider){with(|s|unsafe{s.settings.wheel_size_px=sender.doubleValue().round();s.save();});}
  #[unsafe(method(settingsAppearance:))]fn settings_appearance(&self,sender:&NSPopUpButton){with(|s|unsafe{s.action(300+sender.indexOfSelectedItem(),sender);});}
  #[unsafe(method(permissionGuide:))]fn permission_guide(&self,_sender:Option<&AnyObject>){unsafe{let mtm=MainThreadMarker::new().unwrap();crate::install::welcome(&NSApplication::sharedApplication(mtm),mtm);}}
  #[unsafe(method(repairPermissions:))]fn repair_permissions(&self,_sender:Option<&AnyObject>){unsafe{permissions::repair();}}
  #[unsafe(method(action:))]fn action(&self,sender:&AnyObject){let tag:isize=unsafe{msg_send![sender,tag]};with(|s|unsafe{s.action(tag,sender)});}
  #[unsafe(method(tick:))]fn tick(&self,_t:&NSTimer){unsafe{mascot::process_browser_import();}with(|s|unsafe{s.tick()});}
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
    _status: Retained<NSStatusItem>,
    status_menu: Retained<NSMenu>,
    capture_window: Option<Retained<NSWindow>>,
    preferences: Option<preferences::Preferences>,
    catalog: Vec<crate::model::DockItem>,
    catalog_pending: bool,
    preferences_layout_due: f64,
    preferences_interacting: bool,
    catalog_drag_source: Option<Retained<preferences_drag::CatalogButton>>,
    menu_dock_label: Option<Retained<NSTextField>>,
    screen_frame: NSRect,
    primary_height: f64,
    icons: HashMap<String, Retained<NSImage>>,
    mascot: std::rc::Rc<mascot::MascotImage>,
    mascot_import_pending: bool,
    mascot_request_due: f64,
    running: Vec<String>,
    tx: Sender<Message>,
    rx: Receiver<Message>,
    gesture: Gesture,
    tap: platform::Ref,
    tap_keyboard: Option<bool>,
    key_events_seen: bool,
    source: platform::Ref,
    anim_timer: Option<Retained<NSTimer>>,
    birth: Instant,
    active: bool,
    center: (f64, f64),
    pointer: (f64, f64),
    selected: Option<usize>,
    windows: crate::window_picker::Picker,
    window_query_pending: bool,
    window_query_count: usize,
    window_titles: HashMap<(i32, u32), String>,
    window_glass: Option<Retained<NSView>>,
    ring: layout::DockLayout,
    anchor: (f64, f64),
    opened: f64,
    closing: Option<f64>,
    trial: Option<f64>,
    last_frame: usize,
    last_motion: f64,
    invocations: usize,
    armed: bool,
    hover_mix: Vec<f64>,
    tether: Tether,
    last_snap: Option<String>,
    capturing: bool,
    choosing_apps: bool,
    capture_dirty: bool,
    pending_key: platform::Ref,
    capture_release: Option<u16>,
    trusted: bool,
    tap_attempted: bool,
    tap_retry_at: f64,
    tap_failures: u8,
    permission_guide_pending: bool,
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
pub(crate) unsafe fn session_locked() -> bool {
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
        if kind == 10 || kind == 11 {
            s.key_events_seen = true;
        }
        if s.choosing_apps || s.preferences_interacting {
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
            if s.settings.show_window_list
                && s.windows.contains(s.pointer)
                && platform::CGEventGetIntegerValueField(event, 123) == 0
            {
                let lines = platform::CGEventGetIntegerValueField(event, 11);
                let delta = if lines != 0 {
                    lines
                } else {
                    platform::CGEventGetIntegerValueField(event, 96)
                } as f64;
                s.windows.scroll(delta, s.now());
                s.canvas.setNeedsDisplay(true);
            }
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
pub fn run() {
    unsafe {
        let mtm = MainThreadMarker::new().unwrap();
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        crate::install::set_icon(&app);
        if !crate::install::bootstrap(&app, mtm) {
            return;
        }
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
        #[cfg(feature = "diagnostics")]
        if std::env::args().any(|arg| arg == "--test-missing-accessibility") {
            platform::simulate_permission(false);
        }
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
            button.setToolTip(Some(ns_string!("ViGo · V 键软件轮盘")));
        }
        let selected = settings
            .mascot_id
            .as_deref()
            .and_then(|id| crate::mascot::load(id).ok());
        let mascot = selected
            .and_then(|asset| mascot::MascotImage::load(asset).ok())
            .unwrap_or_else(|| {
                mascot::MascotImage::load(crate::mascot::default_asset()).expect("Bundled mascot")
            });
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
            _status: status,
            status_menu: menu,
            capture_window: None,
            preferences: None,
            catalog: vec![],
            catalog_pending: false,
            preferences_layout_due: 0.,
            preferences_interacting: false,
            catalog_drag_source: None,
            menu_dock_label: None,
            screen_frame: NSRect::ZERO,
            primary_height: 0.,
            icons: HashMap::new(),
            mascot,
            mascot_import_pending: false,
            mascot_request_due: 0.,
            running: vec![],
            tx,
            rx,
            gesture: Gesture::default(),
            tap: ptr::null_mut(),
            tap_keyboard: None,
            key_events_seen: false,
            source: ptr::null_mut(),
            anim_timer: None,
            birth: Instant::now(),
            active: false,
            center: (0., 0.),
            pointer: (0., 0.),
            selected: None,
            windows: crate::window_picker::Picker::default(),
            window_query_pending: false,
            window_query_count: 0,
            window_titles: HashMap::new(),
            window_glass: None,
            ring: layout::DockLayout::new(0, 520., 1470., 956.),
            anchor: (0., 0.),
            opened: 0.,
            closing: None,
            trial: None,
            last_frame: 0,
            last_motion: 0.,
            invocations: 0,
            armed: false,
            hover_mix: vec![],
            tether: Tether::default(),
            last_snap: None,
            capturing: false,
            choosing_apps: false,
            capture_dirty: false,
            pending_key: ptr::null_mut(),
            capture_release: None,
            trusted: platform::accessibility_trusted(),
            tap_attempted: false,
            tap_retry_at: 0.,
            tap_failures: 0,
            permission_guide_pending: !platform::accessibility_trusted(),
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
        s.load_icons();
        if s.trusted && !s.locked {
            s.install_hook();
        }
        s.refresh_dock();
        s.rebuild_menu();
        s.build_application_menu();
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
                    "mascot-import" => {
                        self.import_mascot(
                            v["path"].as_str().unwrap_or_default().to_owned(),
                            v["website"].as_bool().unwrap_or(false),
                        );
                    }
                    "settings-snapshot" => {
                        self.preferences_snapshot(&dir);
                    }
                    "wheel-snapshot" => {
                        let _: () = msg_send![&*self.delegate,performSelector:sel!(wheelSnapshot:),withObject:None::<&AnyObject>,afterDelay:0.0f64];
                    }
                    "settings-page" => {
                        self.preferences_page(v["page"].as_u64().unwrap_or(0) as usize);
                    }
                    "test-permission" => {
                        platform::simulate_permission(v["trusted"].as_bool().unwrap_or(false))
                    }
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
                        self.trial =
                            Some(self.now() + v["duration"].as_f64().unwrap_or(3.).clamp(1., 12.));
                    }
                    "dismiss" => self.close_wheel(),
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
        let v = serde_json::json!({"preferences":self.preferences_diagnostics(),"input":self.input_report(),"version":env!("CARGO_PKG_VERSION"),"ack":self.diag_ack,"mode":"dock","dock":self.settings.wheel,"dockExact":self.dock_exact,"dockPending":self.dock_pending,"activePaths":self.active_paths,"trusted":self.trusted,"permissionGuides":crate::install::guide_count(),"tap":!self.tap.is_null(),"active":self.active,"capturing":self.capturing,"selected":self.selected,"armed":self.armed,"items":self.active_paths.len(),"nodeCenters":self.ring.nodes.iter().map(|n|[self.center.0+n.angle.cos()*n.radius,self.center.1+n.angle.sin()*n.radius,self.ring.icon]).collect::<Vec<_>>(),"rings":self.ring.nodes.iter().map(|n|n.ring).max().unwrap_or(0)+1,"center":[self.center.0,self.center.1],"softwareOnly":true,"glassCount":self.glasses.iter().filter(|view|!view.isHidden()).count(),"tetherTip":[self.tether.tip().x,self.tether.tip().y],"tetherAlpha":self.tether.alpha(),"tetherSnapped":self.tether.snapped,"emptyBackdrop":true,"notice":self.notice});
        let mut v = v;
        v["windowList"] = serde_json::json!({"enabled":self.settings.show_window_list,"queries":self.window_query_count,"pending":self.window_query_pending,"path":self.windows.path,"items":self.windows.items,"selected":self.windows.selected,"explicit":self.windows.explicit,"first":self.windows.first,"frame":self.windows.frame.map(|f|[f.x,f.y,f.w,f.h])});
        let tmp = dir.join("state.tmp");
        let _ = std::fs::write(&tmp, serde_json::to_vec(&v).unwrap());
        let _ = std::fs::rename(tmp, dir.join("state.json"));
    }
}
