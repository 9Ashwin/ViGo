//! macOS event boundary and bounded, isolated system services. No AppKit calls on workers.
use crate::model::{DockItem, Tab, Window};
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
    pub fn CGEventGetIntegerValueField(event: Ref, field: u32) -> i64;
    pub fn CGEventSetIntegerValueField(event: Ref, field: u32, value: i64);
    pub fn CGEventGetFlags(event: Ref) -> u64;
    pub fn CGEventGetLocation(event: Ref) -> NSPoint;
    pub fn CGEventCreateCopy(event: Ref) -> Ref;
    pub fn CGEventSetType(event: Ref, kind: u32);
    pub fn CGEventTapPostEvent(proxy: Ref, event: Ref);
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
    Dock {
        items: Vec<DockItem>,
        exact: bool,
    },
    Snapshot {
        windows: Vec<Window>,
        tabs: Vec<Tab>,
        running: Vec<String>,
        paths: Vec<String>,
    },
    DockFailed,
    Failure(String),
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
pub fn snapshot(paths: Vec<String>, tx: Sender<Message>) {
    std::thread::spawn(move || {
        let args = |cmd: &str| {
            let mut a = vec![cmd.into()];
            a.extend(paths.iter().filter(|p| p.ends_with(".app")).cloned());
            a
        };
        let value = run(&args("list"), Duration::from_millis(3500)).unwrap_or_default();
        let tabs = run(&args("tabs"), Duration::from_millis(2200)).unwrap_or_default();
        let _ = tx.send(Message::Snapshot {
            paths,
            windows: serde_json::from_value(value["windows"].clone()).unwrap_or_default(),
            tabs: serde_json::from_value(tabs["tabs"].clone()).unwrap_or_default(),
            running: serde_json::from_value(value["running"].clone()).unwrap_or_default(),
        });
    });
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
