//! macOS event boundary and bounded, isolated system services. No AppKit calls on workers.
use crate::model::DockItem;
use objc2_foundation::NSPoint;
use std::{
    ffi::c_void,
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc::Sender,
    time::{Duration, Instant},
};
pub type Ref = *mut c_void;
#[repr(C)]
pub struct SourceContext {
    pub version: isize,
    pub info: Ref,
    pub retain: Ref,
    pub release: Ref,
    pub description: Ref,
    pub equal: Ref,
    pub hash: Ref,
    pub schedule: Ref,
    pub cancel: Ref,
    pub perform: Ref,
}
pub const REPLAY: i64 = 0x5649474f;
pub const SESSION_TAP: u32 = 1;
pub const KEYBOARD_EVENTS: u64 = (1 << 10) | (1 << 11);
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct TapInformation {
    id: u32,
    point: u32,
    options: u32,
    events: u64,
    process: i32,
    target: i32,
    enabled: bool,
    minimum: f32,
    average: f32,
    maximum: f32,
}
impl TapInformation {
    fn keyboard_filter_for(&self, process: i32) -> bool {
        self.process == process
            && self.point == SESSION_TAP
            && self.options == 0
            && self.events & KEYBOARD_EVENTS == KEYBOARD_EVENTS
    }
}
#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn IsSecureEventInputEnabled() -> u8;
}
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    pub fn AXIsProcessTrusted() -> bool;
    pub fn AXIsProcessTrustedWithOptions(options: Ref) -> bool;
    pub fn AXUIElementCreateApplication(pid: i32) -> Ref;
    pub fn AXUIElementCopyAttributeValue(element: Ref, name: Ref, value: *mut Ref) -> i32;
    pub fn AXUIElementSetMessagingTimeout(element: Ref, timeout: f32) -> i32;
    pub fn AXObserverCreate(
        pid: i32,
        callback: extern "C" fn(Ref, Ref, Ref, Ref),
        observer: *mut Ref,
    ) -> i32;
    pub fn AXObserverAddNotification(observer: Ref, element: Ref, name: Ref, info: Ref) -> i32;
    pub fn AXObserverGetRunLoopSource(observer: Ref) -> Ref;
    pub fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        mask: u64,
        callback: extern "C" fn(Ref, u32, Ref, Ref) -> Ref,
        info: Ref,
    ) -> Ref;
    pub fn CGEventTapEnable(tap: Ref, on: bool);
    pub fn CGEventTapIsEnabled(tap: Ref) -> bool;
    fn CGGetEventTapList(maximum: u32, list: *mut TapInformation, count: *mut u32) -> i32;
    pub fn CGEventGetIntegerValueField(event: Ref, field: u32) -> i64;
    pub fn CGEventSetIntegerValueField(event: Ref, field: u32, value: i64);
    pub fn CGEventGetFlags(event: Ref) -> u64;
    pub fn CGEventGetLocation(event: Ref) -> NSPoint;
    pub fn CGEventCreateCopy(event: Ref) -> Ref;
    pub fn CGEventSetType(event: Ref, kind: u32);
    pub fn CGEventTapPostEvent(proxy: Ref, event: Ref);
}
pub unsafe fn secure_input_enabled() -> bool {
    IsSecureEventInputEnabled() != 0
}

/// Quartz may remove keyboard bits while returning a valid, mouse-only tap.
/// Query failure is unknown, rather than evidence that the keyboard is unavailable.
pub fn keyboard_tap_registered() -> Option<bool> {
    unsafe {
        let mut count = 0;
        if CGGetEventTapList(0, std::ptr::null_mut(), &mut count) != 0 || count > 4096 {
            return None;
        }
        let mut taps = vec![TapInformation::default(); (count + 16) as usize];
        if CGGetEventTapList(taps.len() as u32, taps.as_mut_ptr(), &mut count) != 0 {
            return None;
        }
        Some(
            taps.iter()
                .take(count as usize)
                .any(|t| t.keyboard_filter_for(std::process::id() as i32)),
        )
    }
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    pub fn CFRelease(p: Ref);
    pub fn CFRetain(p: Ref) -> Ref;
    pub fn CFArrayGetCount(array: Ref) -> isize;
    pub fn CFArrayGetValueAtIndex(array: Ref, index: isize) -> Ref;
    pub fn CFEqual(a: Ref, b: Ref) -> bool;
    pub fn CFMachPortCreateRunLoopSource(allocator: Ref, port: Ref, order: isize) -> Ref;
    pub fn CFRunLoopGetMain() -> Ref;
    pub fn CFRunLoopAddSource(loop_ref: Ref, source: Ref, mode: Ref);
    pub fn CFRunLoopRemoveSource(loop_ref: Ref, source: Ref, mode: Ref);
    pub fn CFMachPortInvalidate(port: Ref);
    pub fn CFRunLoopSourceCreate(allocator: Ref, order: isize, context: *mut SourceContext) -> Ref;
    pub fn CFRunLoopSourceSignal(source: Ref);
    pub fn CFRunLoopWakeUp(loop_ref: Ref);
    pub static kCFRunLoopCommonModes: Ref;
    pub fn CGSessionCopyCurrentDictionary() -> Ref;
}
pub unsafe fn replay(proxy: Ref, original: Ref) {
    if original.is_null() {
        return;
    }
    for kind in [10, 11] {
        let e = CGEventCreateCopy(original);
        if !e.is_null() {
            CGEventSetType(e, kind);
            CGEventSetIntegerValueField(e, 42, REPLAY);
            CGEventSetIntegerValueField(e, 8, 0);
            CGEventTapPostEvent(proxy, e);
            CFRelease(e);
        }
    }
    CFRelease(original);
}
pub fn helper() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let path = exe.parent().unwrap().join("../Resources/system-helper");
    if path.exists() {
        path
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("native/system-helper/build/system-helper")
    }
}
pub fn resource(name: &str) -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let p = exe.parent().unwrap().join("../Resources").join(name);
    if p.exists() {
        p
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join(name)
    }
}
pub fn run(args: &[String], timeout: Duration) -> Result<serde_json::Value, String> {
    let mut child = Command::new(helper())
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let out = child.stdout.take().unwrap();
    let reader = std::thread::spawn(move || {
        let mut b = Vec::new();
        out.take(8 * 1024 * 1024).read_to_end(&mut b).map(|_| b)
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(15)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("System query timed out".into());
            }
        }
    }
    let bytes = reader
        .join()
        .map_err(|_| "System query failed")?
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
pub enum Message {
    Dock { items: Vec<DockItem>, exact: bool },
    Catalog(Vec<DockItem>),
    DockFailed,
    Failure(String),
}
pub fn catalog(tx: Sender<Message>) {
    std::thread::spawn(move || {
        objc2::rc::autoreleasepool(|_| {
            use objc2_foundation::{NSBundle, NSString};
            let mut queue = std::collections::VecDeque::from([
                (PathBuf::from("/Applications"), 0),
                (PathBuf::from("/System/Applications"), 0),
                (
                    PathBuf::from(std::env::var_os("HOME").unwrap()).join("Applications"),
                    0,
                ),
            ]);
            let mut seen = std::collections::BTreeSet::new();
            let mut apps = vec![];
            let start = Instant::now();
            while let Some((dir, depth)) = queue.pop_front() {
                if start.elapsed() > Duration::from_secs(4) || apps.len() > 800 {
                    break;
                }
                let Ok(entries) = std::fs::read_dir(dir) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if entry.file_name().to_string_lossy().starts_with('.') {
                        continue;
                    }
                    if path
                        .extension()
                        .is_some_and(|v| v.eq_ignore_ascii_case("app"))
                    {
                        let Ok(path) = path.canonicalize() else {
                            continue;
                        };
                        let path = path.to_string_lossy().into_owned();
                        if !seen.insert(path.clone()) {
                            continue;
                        }
                        if let Some(bundle) = NSBundle::bundleWithPath(&NSString::from_str(&path)) {
                            if bundle.executableURL().is_none() {
                                continue;
                            }
                            let mut name = PathBuf::from(&path)
                                .file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned();
                            for key in ["CFBundleDisplayName", "CFBundleName"] {
                                if let Some(value) =
                                    bundle.objectForInfoDictionaryKey(&NSString::from_str(key))
                                {
                                    let label: objc2::rc::Retained<NSString> =
                                        unsafe { objc2::msg_send![&*value, description] };
                                    if !label.is_empty() {
                                        name = label.to_string();
                                        break;
                                    }
                                }
                            }
                            apps.push(DockItem {
                                path,
                                name,
                                kind: "app".into(),
                            });
                        }
                    } else if depth < 3 && entry.file_type().is_ok_and(|v| v.is_dir()) {
                        queue.push_back((path, depth + 1));
                    }
                }
            }
            apps.sort_by_key(|a| a.name.to_lowercase());
            let _ = tx.send(Message::Catalog(apps));
        })
    });
}
pub fn dock(tx: Sender<Message>) {
    std::thread::spawn(
        move || match run(&["dock".into()], Duration::from_millis(2500)) {
            Ok(v) => {
                let items = serde_json::from_value(v["items"].clone()).unwrap_or_default();
                let _ = tx.send(Message::Dock {
                    items,
                    exact: v["exact"].as_bool().unwrap_or(false),
                });
            }
            Err(_) => {
                let _ = tx.send(Message::DockFailed);
            }
        },
    );
}
pub fn activate(command: &str, item: String, tx: Sender<Message>) {
    let command = command.to_string();
    std::thread::spawn(move || {
        if !run(&[command, item], Duration::from_secs(3))
            .ok()
            .is_some_and(|v| v["ok"] == true)
        {
            let _ = tx.send(Message::Failure(
                "目标已变化或暂时无法切换，请重新选择".into(),
            ));
        }
    });
}

/// Query only; never trigger a system prompt from a polling timer.
pub fn accessibility_trusted() -> bool {
    #[cfg(feature = "diagnostics")]
    {
        let value = PERMISSION_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed);
        if value >= 0 {
            return value != 0;
        }
    }
    unsafe { AXIsProcessTrusted() }
}
pub unsafe fn request_accessibility() {
    if AXIsProcessTrusted() {
        return;
    }
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_foundation::{ns_string, NSDictionary, NSNumber, NSString};
    let options = NSDictionary::<NSString, AnyObject>::from_slices(
        &[ns_string!("AXTrustedCheckOptionPrompt")],
        &[&*NSNumber::numberWithBool(true)],
    );
    AXIsProcessTrustedWithOptions(Retained::as_ptr(&options) as Ref);
}
#[cfg(feature = "diagnostics")]
static PERMISSION_OVERRIDE: std::sync::atomic::AtomicI8 = std::sync::atomic::AtomicI8::new(-1);
#[cfg(feature = "diagnostics")]
pub fn simulate_permission(trusted: bool) {
    PERMISSION_OVERRIDE.store(i8::from(trusted), std::sync::atomic::Ordering::Relaxed);
}
#[cfg(feature = "diagnostics")]
pub fn permission_is_simulated() -> bool {
    PERMISSION_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed) >= 0
}

#[cfg(test)]
mod tap_tests {
    use super::*;
    #[test]
    fn mouse_only_or_partial_keyboard_taps_do_not_report_keyboard_ready() {
        let mut tap = TapInformation {
            process: 41,
            point: SESSION_TAP,
            ..Default::default()
        };
        for mask in [1 << 5, 1 << 10, 1 << 11] {
            tap.events = mask;
            assert!(!tap.keyboard_filter_for(41));
        }
        tap.events = KEYBOARD_EVENTS | (1 << 5);
        assert!(tap.keyboard_filter_for(41));
        assert!(!tap.keyboard_filter_for(42));
        tap.options = 1; // A listen-only tap cannot suppress V while the wheel is open.
        assert!(!tap.keyboard_filter_for(41));
    }
}
