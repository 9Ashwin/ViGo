//! AppKit is confined to this module and the main thread. Workers communicate by owned messages.
mod controls;
mod settings;
mod wheel;

use crate::{
    input::{self, Effect, Gesture},
    layout::{self, Rect},
    model::{Content, Pin, Settings, Tab, Trigger, Window},
    platform::{self, AppInfo, Message},
};
use objc2::{
    define_class, msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyClass, AnyObject, ProtocolObject},
    sel, AllocAnyThread, ClassType, DefinedClass, MainThreadOnly,
};
use objc2_app_kit::*;
use objc2_foundation::*;
use std::{
    cell::RefCell,
    collections::HashMap,
    ffi::{c_void, CString},
    ptr,
    sync::mpsc::{self, Receiver, Sender},
    time::Instant,
};

thread_local! {static UI:RefCell<Option<State>>=const{RefCell::new(None)};}
fn with(f: impl FnOnce(&mut State)) {
    UI.with(|cell| {
        if let Ok(mut s) = cell.try_borrow_mut() {
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
unsafe fn string(v: &NSView) -> String {
    let t: Retained<NSString> = msg_send![v, stringValue];
    t.to_string()
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
unsafe fn clear(parent: &NSView) {
    for child in parent.subviews() {
        child.removeFromSuperview();
    }
}

define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind=MainThreadOnly]
    #[ivars=usize]
    struct Canvas;
    impl Canvas {
        #[unsafe(method(isFlipped))] fn flipped(&self)->bool{true}
        #[unsafe(method(drawRect:))] fn draw(&self,_rect:NSRect){with(|s|unsafe{if *self.ivars()==0{s.draw_wheel()}else{s.draw_editor(self)}});}
        #[unsafe(method(mouseDown:))] fn mouse(&self,event:&NSEvent){if *self.ivars()==1{let p=self.convertPoint_fromView(event.locationInWindow(),None);with(|s|{s.drag_from=s.editor_click(p.x,p.y);s.drag_to=None;});self.setNeedsDisplay(true);}}
        #[unsafe(method(mouseDragged:))] fn dragged(&self,event:&NSEvent){if *self.ivars()==1{let p=self.convertPoint_fromView(event.locationInWindow(),None);with(|s|{s.drag_to=layout::editor_slot(p.x,p.y,s.settings.slots_per_page).map(|i|s.page*s.settings.slots_per_page+i);});self.setNeedsDisplay(true);}}
        #[unsafe(method(mouseUp:))] fn mouse_up(&self,_event:&NSEvent){let _keep=unsafe{Retained::retain(self as *const Self as *mut Self)};if *self.ivars()==1{with(|s|unsafe{if let (Some(a),Some(b))=(s.drag_from.take(),s.drag_to.take()){s.settings.wheel.swap(a,b);s.edit_slot=b;s.save();}s.settings_page();});}}
        #[unsafe(method(draggingEntered:))] fn drag_enter(&self,_sender:&AnyObject)->usize{if *self.ivars()==1{1}else{0}}
        #[unsafe(method(performDragOperation:))] fn file_drop(&self,sender:&AnyObject)->objc2::runtime::Bool {
            let _keep=unsafe{Retained::retain(self as *const Self as *mut Self)};
            if *self.ivars()!=1{return false.into()}
            unsafe {let pasteboard:Retained<AnyObject>=msg_send![sender,draggingPasteboard];let classes:Retained<AnyObject>=msg_send![class("NSArray"),arrayWithObject:NSURL::class()];let urls:Option<Retained<NSArray<NSURL>>>=msg_send![&*pasteboard,readObjectsForClasses:&*classes,options:ptr::null::<AnyObject>()];let point:NSPoint=msg_send![sender,draggingLocation];let p=self.convertPoint_fromView(point,None);let paths:Vec<String>=urls.into_iter().flat_map(|urls|urls.iter().filter_map(|u|u.path().map(|p|p.to_string())).collect::<Vec<_>>()).collect();let mut accepted=false;with(|s|{let target=s.editor_click(p.x,p.y).or_else(||s.settings.wheel.iter().position(|p|p.is_empty()));if let Some(target)=target{s.edit_slot=target;accepted=s.add_paths(paths);}else{s.notice="轮盘已满，请拖到要替换的位置".into();}});accepted.into()}
        }

    }
);
impl Canvas {
    unsafe fn new(kind: usize, frame: Rect) -> Retained<Self> {
        let v = Self::alloc(MainThreadMarker::new().unwrap()).set_ivars(kind);
        let v: Retained<Self> = msg_send![super(v),initWithFrame:rect(frame)];
        if kind == 1 {
            v.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
            v.registerForDraggedTypes(&NSArray::from_slice(&[ns_string!("public.file-url")]));
        }
        v
    }
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind=MainThreadOnly]
    struct Delegate;
    unsafe impl NSObjectProtocol for Delegate{}
    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationShouldHandleReopen:hasVisibleWindows:))]
        fn reopen(&self,_app:&NSApplication,_visible:bool)->bool{with(|s|unsafe{s.show_settings()});true}
        #[unsafe(method(applicationWillTerminate:))] fn quit(&self,_n:&NSNotification){with(|s|s.stop());}
    }
    unsafe impl NSWindowDelegate for Delegate {
        #[unsafe(method(windowShouldClose:))]fn close(&self,window:&NSWindow)->bool{with(|s|unsafe{if s.settings_window.as_ref().is_some_and(|w|std::ptr::eq(&**w,window)){s.capturing=false;s.hide_settings();}else{window.orderOut(None);}});false}
        #[unsafe(method(windowDidEndLiveResize:))]fn resized(&self,_n:&NSNotification){with(|s|unsafe{s.settings_page()});}
        #[unsafe(method(windowDidResignKey:))]fn resign(&self,_n:&NSNotification){with(|s|{if s.capturing{s.capturing=false;unsafe{s.settings_page();}}});}
    }
    impl Delegate {
        #[unsafe(method(action:))]fn action(&self,sender:&AnyObject){let tag:isize=unsafe{msg_send![sender,tag]};let popup=UI.with(|cell|{let Ok(state)=cell.try_borrow()else{return None};state.as_ref().filter(|s|tag==0&&s.app.currentEvent().is_some_and(|e|e.r#type()==NSEventType::RightMouseUp)).map(|s|(s.status.clone(),s.status_menu.clone()))});if let Some((status,menu))=popup{#[allow(deprecated)]status.popUpStatusItemMenu(&menu);return;}with(|s|unsafe{s.action(tag,sender)});}
        #[unsafe(method(tick:))]fn tick(&self,_timer:&NSTimer){with(|s|unsafe{s.tick()});}
    }
);

struct State {
    app: Retained<NSApplication>,
    delegate: Retained<Delegate>,
    settings: Settings,
    settings_window: Option<Retained<NSWindow>>,
    settings_root: Option<Retained<NSView>>,
    content_root: Option<Retained<NSView>>,
    overlay: Retained<NSWindow>,
    canvas: Retained<Canvas>,
    glasses: Vec<Retained<NSView>>,
    status: Retained<NSStatusItem>,
    status_menu: Retained<NSMenu>,
    screen_frame: NSRect,
    primary_height: f64,
    picker: Option<Retained<NSWindow>>,
    picker_doc: Option<Retained<NSView>>,
    search: Option<Retained<NSView>>,
    icons: HashMap<String, Retained<NSImage>>,
    sheet: Retained<NSImage>,
    catalog: Vec<AppInfo>,
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
    page: usize,
    tab: usize,
    edit_slot: usize,
    drag_from: Option<usize>,
    drag_to: Option<usize>,
    outer_owner: String,
    opened: f64,
    closing: Option<f64>,
    trial: Option<f64>,
    scan_pending: bool,
    last_scan: f64,
    last_frame: usize,
    last_motion: f64,
    invocations: usize,
    armed: bool,
    hover_mix: [f64; 12],
    capturing: bool,
    capture_dirty: bool,
    pending_key: platform::Ref,
    capture_release: Option<u16>,
    trusted: bool,
    tap_attempted: bool,
    locked: bool,
    preview: Option<platform::Preview>,
    preview_gen: u64,
    preview_key: String,
    preview_start: f64,
    preview_pending: Option<Window>,
    preview_image: Option<Retained<NSImage>>,
    preview_cache: Vec<(String, Retained<NSImage>)>,
    preview_status: String,
    preview_title: String,
    preview_rect: Rect,
    notice: String,
    toast: Option<(Retained<NSWindow>, f64)>,
    #[cfg(feature = "diagnostics")]
    diag: Option<std::path::PathBuf>,
    #[cfg(feature = "diagnostics")]
    diag_ack: u64,
    #[cfg(feature = "diagnostics")]
    preview_frames: u64,
    #[cfg(feature = "diagnostics")]
    preview_hashes: std::collections::BTreeSet<u64>,
    wake: platform::Ref,
    effects: Vec<Effect>,
    moved: bool,
    layout_dirty: bool,
    picker_buttons: Vec<(String, Retained<NSView>)>,
}

unsafe fn frame_view(v: &NSView, f: Rect) {
    v.setFrame(rect(f));
}

type NSRetainedPanel = Retained<AnyObject>;
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
unsafe fn decode_image(encoded: &str) -> Option<Retained<NSImage>> {
    let encoded = encoded
        .strip_prefix("data:image/jpeg;base64,")
        .or_else(|| encoded.strip_prefix("data:image/png;base64,"))?;
    let d: Option<Retained<NSData>> = msg_send![NSData::alloc(),initWithBase64EncodedString:&*NSString::from_str(encoded),options:0usize];
    d.and_then(|data| NSImage::initWithData(NSImage::alloc(), &data))
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
            if kind == 10 && code == 53 && s.gesture.open {
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
            let dx = s.pointer.0 - s.center.0;
            let dy = s.pointer.1 - s.center.1;
            if s.entries.is_empty() || dx.hypot(dy) < s.settings.wheel_size_px * 0.28 {
                let delta = platform::CGEventGetIntegerValueField(event, 11);
                if delta != 0 {
                    let pages = s.settings.wheel.len().div_ceil(s.settings.slots_per_page);
                    s.page = if delta < 0 {
                        (s.page + 1) % pages
                    } else {
                        (s.page + pages - 1) % pages
                    };
                    s.selected = None;
                    s.entries.clear();
                    s.chips.clear();
                    s.stop_preview();
                    s.layout_dirty = true;
                    s.moved = true;
                }
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
        // Single instance across native launches. Electron is stopped by the installer before replacement.
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
        let mtm = MainThreadMarker::new().expect("AppKit must start on the main thread");
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        let delegate: Retained<Delegate> = msg_send![Delegate::alloc(mtm), init];
        app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
        let main_menu = NSMenu::new(mtm);
        for (heading, commands) in [
            (
                "ViGo",
                vec![
                    ("关于 ViGo", sel!(orderFrontStandardAboutPanel:), ""),
                    ("隐藏 ViGo", sel!(hide:), "h"),
                    ("退出 ViGo", sel!(terminate:), "q"),
                ],
            ),
            (
                "编辑",
                vec![
                    ("撤销", sel!(undo:), "z"),
                    ("剪切", sel!(cut:), "x"),
                    ("复制", sel!(copy:), "c"),
                    ("粘贴", sel!(paste:), "v"),
                    ("全选", sel!(selectAll:), "a"),
                ],
            ),
        ] {
            let root = NSMenuItem::new(mtm);
            let submenu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(heading));
            for (title, action, key) in commands {
                let item = NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(mtm),
                    &NSString::from_str(title),
                    Some(action),
                    &NSString::from_str(key),
                );
                submenu.addItem(&item);
            }
            root.setTitle(&NSString::from_str(heading));
            root.setSubmenu(Some(&submenu));
            main_menu.addItem(&root);
        }
        app.setMainMenu(Some(&main_menu));
        let (settings, load_notice) = Settings::load();
        let panel: Allocated<NSWindow> = msg_send![class("NSPanel"), alloc];
        let overlay = NSWindow::initWithContentRect_styleMask_backing_defer(
            panel,
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
        let canvas = Canvas::new(0, r(0., 0., 800., 800.));
        root.addSubview(&canvas);
        overlay.setContentView(Some(&root));
        let status = NSStatusBar::systemStatusBar().statusItemWithLength(-1.);
        if let Some(button) = status.button(mtm) {
            let img = NSImage::initWithContentsOfFile(
                NSImage::alloc(),
                &NSString::from_str(&platform::resource("trayTemplate.png").to_string_lossy()),
            );
            if let Some(img) = img {
                img.setTemplate(true);
                button.setImage(Some(&img));
            } else {
                button.setTitle(ns_string!("V"));
            }
            button.setTarget(Some(&delegate));
            button.setAction(Some(sel!(action:)));
            button.setTag(0);
            button.setToolTip(Some(ns_string!("ViGo · 按住 V，移动选择，松开执行")));
            let _: usize = msg_send![&*button,sendActionOn:0x14usize];
        }
        let menu = NSMenu::new(mtm);
        for (title, tag) in [("打开 ViGo 设置…", 1), ("暂停 / 启用", 2), ("退出 ViGo", 3)]
        {
            let item = NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(title),
                Some(sel!(action:)),
                ns_string!(""),
            );
            item.setTarget(Some(&delegate));
            item.setTag(tag);
            menu.addItem(&item);
        } // right-click menu is routed by the status button action below.
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
        let mut state = State {
            app: app.clone(),
            delegate: delegate.clone(),
            settings,
            settings_window: None,
            settings_root: None,
            content_root: None,
            overlay,
            canvas,
            glasses: vec![],
            status,
            status_menu: menu.clone(),
            screen_frame: NSRect::ZERO,
            primary_height: 0.,
            picker: None,
            picker_doc: None,
            search: None,
            icons: HashMap::new(),
            sheet,
            catalog: vec![],
            windows: vec![],
            tabs: vec![],
            running: vec![],
            tx: tx.clone(),
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
            page: 0,
            tab: 0,
            edit_slot: 0,
            drag_from: None,
            drag_to: None,
            outer_owner: String::new(),
            opened: 0.,
            closing: None,
            trial: None,
            scan_pending: false,
            last_scan: 0.,
            last_frame: 0,
            last_motion: 0.,
            invocations: 0,
            armed: false,
            hover_mix: [0.; 12],
            capturing: false,
            capture_dirty: false,
            pending_key: ptr::null_mut(),
            capture_release: None,
            trusted: platform::AXIsProcessTrusted(),
            tap_attempted: false,
            locked: session_locked(),
            preview: None,
            preview_gen: 0,
            preview_key: String::new(),
            preview_start: 0.,
            preview_pending: None,
            preview_image: None,
            preview_cache: vec![],
            preview_status: String::new(),
            preview_title: String::new(),
            preview_rect: Rect::default(),
            notice: load_notice,
            toast: None,
            #[cfg(feature = "diagnostics")]
            diag_ack: 0,
            #[cfg(feature = "diagnostics")]
            preview_frames: 0,
            #[cfg(feature = "diagnostics")]
            preview_hashes: std::collections::BTreeSet::new(),
            #[cfg(feature = "diagnostics")]
            diag: std::env::args()
                .collect::<Vec<_>>()
                .windows(2)
                .find(|v| v[0] == "--diagnostic-dir")
                .map(|v| std::path::PathBuf::from(&v[1])),
            wake,
            effects: vec![],
            moved: false,
            layout_dirty: false,
            picker_buttons: vec![],
        };
        state.load_icons();
        state.apply_theme();
        if state.trusted && !state.locked {
            state.install_hook();
        }
        state.scan();
        if std::env::args().any(|a| a == "--settings")
            || state.settings.wheel.iter().all(|p| p.is_empty())
        {
            state.show_settings();
        }
        UI.with(|s| *s.borrow_mut() = Some(state));
        platform::catalog(tx);
        let timer = NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
            0.5,
            &delegate,
            sel!(tick:),
            None,
            true,
        );
        timer.setTolerance(0.1);
        let _keep = (lock, timer, menu);
        app.run();
    }
}

#[cfg(feature = "diagnostics")]
impl State {
    unsafe fn diagnostics(&mut self) {
        let Some(dir) = self.diag.clone() else { return };
        let command = dir.join("command.json");
        if let Ok(bytes) = std::fs::read(&command) {
            let _ = std::fs::remove_file(&command);
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                self.diag_ack = v["id"].as_u64().unwrap_or(0);
                match v["command"].as_str().unwrap_or("") {
                    "settings" => {
                        self.tab = v["tab"].as_u64().unwrap_or(0).min(2) as usize;
                        self.show_settings();
                    }
                    "hide" => {
                        if self.settings_window.is_some() {
                            self.hide_settings();
                        }
                    }
                    "save" => {
                        if let Ok(next) = serde_json::from_value::<Settings>(v["settings"].clone())
                        {
                            self.close_wheel();
                            self.settings = next;
                            self.save();
                            self.scan();
                            self.settings_page();
                        }
                    }
                    "trial" => {
                        let screens = NSScreen::screens(MainThreadMarker::new().unwrap());
                        let f = screens.firstObject().unwrap().frame();
                        self.open_wheel(
                            v["x"].as_f64().unwrap_or(f.size.width / 2.),
                            v["y"].as_f64().unwrap_or(f.size.height / 2.),
                        );
                        self.trial = Some(self.now() + v["seconds"].as_f64().unwrap_or(5.));
                    }
                    "screenshot" => {
                        if let Some(w) = if v["overlay"].as_bool() == Some(true) {
                            Some(&self.overlay)
                        } else {
                            self.settings_window.as_ref()
                        } {
                            let _ = std::process::Command::new("/usr/sbin/screencapture")
                                .args(["-x", "-l", &w.windowNumber().to_string()])
                                .arg(dir.join(if v["overlay"].as_bool() == Some(true) {
                                    "overlay.png"
                                } else {
                                    "settings.png"
                                }))
                                .spawn();
                        }
                    }
                    "record" => {
                        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                        let _ = std::process::Command::new(
                            root.join(".local/verification/record-capture"),
                        )
                        .arg(root.join(".local/verification/native-demo-frames"))
                        .stdout(std::fs::File::create(dir.join("recording.jsonl")).unwrap())
                        .spawn();
                    }
                    "quit" => self.app.terminate(None),
                    _ => {}
                }
            }
        }
        let _ = std::fs::create_dir_all(&dir);
        let value = serde_json::json!({"version":"0.3.0","ack":self.diag_ack,"previewFrames":self.preview_frames,"previewDistinct":self.preview_hashes.len(),"pid":std::process::id(),"trusted":self.trusted,"locked":self.locked,"tap":!self.tap.is_null(),"capturing":self.capturing,"active":self.active,"selected":self.selected,"selectedChip":self.selected_chip,"pins":self.settings.wheel.iter().filter(|p|!p.is_empty()).count(),"slots":self.settings.slots_per_page,"windows":self.windows.len(),"tabs":self.tabs.len(),"catalog":self.catalog.len(),"screenPermission":platform::CGPreflightScreenCaptureAccess(),"toastVisible":self.toast.as_ref().is_some_and(|(w,_)|w.isVisible()),"preview":self.preview_status,"previewImage":self.preview_image.is_some(),"previewGeneration":self.preview_gen,"settingsWindow":self.settings_window.as_ref().map(|w|w.windowNumber()),"overlayWindow":self.overlay.windowNumber(),"settingsVisible":self.settings_window.as_ref().is_some_and(|w|w.isVisible()),"center":[self.center.0,self.center.1],"armed":self.armed,"entries":self.entries.iter().map(|e|match e{Content::Window(w)=>serde_json::json!({"kind":"window","id":w.id}),Content::Tab(t)=>serde_json::json!({"kind":"tab","id":t.id}),Content::Pin(p)=>serde_json::json!({"kind":"pin","id":p.id})}).collect::<Vec<_>>(),"chips":self.chips.iter().map(|c|[c.x,c.y,c.w,c.h]).collect::<Vec<_>>(),"notice":self.notice});
        if let Ok(data) = serde_json::to_vec(&value) {
            let tmp = dir.join("state.tmp");
            let _ = std::fs::write(&tmp, data);
            let _ = std::fs::rename(tmp, dir.join("state.json"));
        }
    }
}

unsafe fn center_text(value: &str, frame: Rect, size: f64, bold: bool, c: &NSColor) {
    let font = NSFont::systemFontOfSize_weight(size, if bold { 0.45 } else { 0. });
    let para = NSMutableParagraphStyle::new();
    para.setAlignment(NSTextAlignment::Center);
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
