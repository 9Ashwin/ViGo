//! macOS event boundary and bounded, isolated system services. No AppKit calls on workers.
use crate::model::{Tab, Window};
use objc2_foundation::NSPoint;
use serde::Deserialize;
use std::{
    ffi::c_void,
    io::{BufRead, BufReader, Read},
    path::PathBuf,
    process::{ChildStdin, Command, Stdio},
    sync::{mpsc::Sender, Arc},
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
    pub fn CGPreflightScreenCaptureAccess() -> bool;
    pub fn CGRequestScreenCaptureAccess() -> bool;
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
    pub fn CFMachPortCreateRunLoopSource(allocator: Ref, port: Ref, order: isize) -> Ref;
    pub fn CFRunLoopGetMain() -> Ref;
    pub fn CFRunLoopAddSource(loop_ref: Ref, source: Ref, mode: Ref);
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
#[derive(Clone, Debug, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub browser: bool,
}
pub enum Message {
    Catalog(Vec<AppInfo>),
    Snapshot {
        windows: Vec<Window>,
        tabs: Vec<Tab>,
        running: Vec<String>,
        paths: Vec<String>,
    },
    Preview {
        generation: u64,
        image: String,
    },
    PreviewEnded(u64),
    PreviewProblem {
        generation: u64,
        reason: String,
    },
    Failure(String),
}
pub fn catalog(tx: Sender<Message>) {
    std::thread::spawn(move || {
        if let Ok(value) = run(&["apps".into()], Duration::from_secs(8)) {
            if let Ok(mut apps) = serde_json::from_value::<Vec<AppInfo>>(value["apps"].clone()) {
                apps.sort_by(|a, b| a.name.cmp(&b.name));
                let _ = tx.send(Message::Catalog(apps));
            }
        }
    });
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
pub struct Preview {
    input: Option<ChildStdin>,
    child: Arc<std::sync::Mutex<std::process::Child>>,
}
impl Drop for Preview {
    fn drop(&mut self) {
        self.input.take();
        let child = self.child.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(500));
            if let Ok(mut child) = child.lock() {
                if matches!(child.try_wait(), Ok(None)) {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        });
    }
}
pub fn preview(window: &Window, generation: u64, tx: Sender<Message>) -> Option<Preview> {
    if window.minimized || window.window_id == 0 {
        return None;
    }
    let mut child = Command::new(helper())
        .args([
            "stream",
            &window.window_id.to_string(),
            &window.pid.to_string(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let input = child.stdin.take();
    let output = child.stdout.take()?;
    let shared = Arc::new(std::sync::Mutex::new(child));
    let process = shared.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(output).lines() {
            let Ok(line) = line else { break };
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                if let Some(image) = value["image"].as_str() {
                    let _ = tx.send(Message::Preview {
                        generation,
                        image: image.into(),
                    });
                } else if let Some(reason) = value["reason"].as_str() {
                    let _ = tx.send(Message::PreviewProblem {
                        generation,
                        reason: reason.into(),
                    });
                }
            }
        }
        loop {
            let exited = process
                .lock()
                .map(|mut child| !matches!(child.try_wait(), Ok(None)))
                .unwrap_or(true);
            if exited {
                break;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        let _ = tx.send(Message::PreviewEnded(generation));
    });
    Some(Preview {
        input,
        child: shared,
    })
}
