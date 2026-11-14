//! UI Automation objects stay on their MTA worker. Descriptors crossing threads contain no COM pointers.
use super::*;
use windows::{
    core::{Interface, VARIANT},
    Win32::{System::Com::*, UI::Accessibility::*},
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub key: String,
    pub name: String,
    pub path: String,
}
#[derive(Clone, Debug)]
pub struct Detail {
    pub title: String,
    pub hwnd: isize,
    pub pid: u32,
    pub token: u64,
}
pub enum Request {
    Activate(String, u64),
    Details(String),
    Select(Detail, u64),
    Stop,
}
pub enum Update {
    Items(Vec<Item>),
    Details(String, Vec<Detail>),
    Error(String),
}
fn path(key: &str) -> String {
    key.strip_prefix("Appid:").unwrap_or(key).trim().to_string()
}
unsafe fn snapshot(ui: &IUIAutomation) -> Result<Vec<(Item, IUIAutomationElement)>, String> {
    let tray = FindWindowW(w!("Shell_TrayWnd"), None)
        .map_err(|_| "当前会话没有可读取的任务栏".to_string())?;
    let root = ui
        .ElementFromHandle(tray)
        .map_err(|_| "任务栏 UI Automation 不可用")?;
    let condition = ui
        .CreatePropertyCondition(
            UIA_ControlTypePropertyId,
            &VARIANT::from(UIA_ButtonControlTypeId.0),
        )
        .map_err(|_| "任务栏条件失败")?;
    let elements = root
        .FindAll(TreeScope_Descendants, &condition)
        .map_err(|_| "任务栏树暂不可用")?;
    let mut items = vec![];
    for i in 0..elements.Length().unwrap_or(0) {
        let Ok(e) = elements.GetElement(i) else {
            continue;
        };
        let key = e
            .CurrentAutomationId()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let class = e
            .CurrentClassName()
            .map(|s| s.to_string())
            .unwrap_or_default();
        if !key.starts_with("Appid:") && !class.contains("TaskListButton") {
            continue;
        }
        let name = e.CurrentName().map(|s| s.to_string()).unwrap_or_default();
        if key.is_empty() || name.is_empty() {
            continue;
        }
        let bounds = e.CurrentBoundingRectangle().unwrap_or_default();
        if bounds.right <= bounds.left || bounds.bottom <= bounds.top {
            continue;
        }
        let descriptor = Item {
            path: path(&key),
            key,
            name,
        };
        items.push((descriptor, e, bounds.left, bounds.top));
    }
    items.sort_by_key(|(_, _, x, y)| (*y, *x));
    let mut seen = std::collections::HashSet::new();
    let out: Vec<_> = items
        .into_iter()
        .filter(|(i, _, _, _)| seen.insert(i.key.clone()))
        .map(|(i, e, _, _)| (i, e))
        .collect();
    if out.is_empty() {
        Err("未识别到实际任务栏应用按钮；保留上次列表".into())
    } else {
        Ok(out)
    }
}
unsafe fn invoke(element: &IUIAutomationElement) -> bool {
    element
        .GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
        .and_then(|p| p.Invoke())
        .is_ok()
}
unsafe fn windows_for(path: &str) -> Vec<Detail> {
    struct Query<'a> {
        path: &'a str,
        out: Vec<Detail>,
    }
    unsafe extern "system" fn collect(hwnd: HWND, lp: LPARAM) -> BOOL {
        let q = &mut *(lp.0 as *mut Query);
        if !IsWindowVisible(hwnd).as_bool()
            || GetWindowTextLengthW(hwnd) == 0
            || GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0
        {
            return BOOL(1);
        }
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == std::process::id() {
            return BOOL(1);
        }
        if process_path(pid).is_some_and(|p| p.eq_ignore_ascii_case(q.path))
            || window_app_id(hwnd, pid).is_some_and(|id| id.eq_ignore_ascii_case(q.path))
        {
            let mut title = vec![0u16; GetWindowTextLengthW(hwnd) as usize + 1];
            let n = GetWindowTextW(hwnd, &mut title);
            q.out.push(Detail {
                title: String::from_utf16_lossy(&title[..n as usize]),
                hwnd: hwnd.0 as isize,
                pid,
                token: 0,
            });
        }
        BOOL(1)
    }
    let mut q = Query { path, out: vec![] };
    let _ = EnumWindows(Some(collect), LPARAM(&mut q as *mut _ as isize));
    q.out
}
pub(super) fn start() -> (
    std::sync::mpsc::Sender<Request>,
    std::sync::mpsc::Receiver<Update>,
) {
    let (tx, rx) = std::sync::mpsc::channel();
    let (out, updates) = std::sync::mpsc::channel();
    std::thread::spawn(move || unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let ui: IUIAutomation = match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
            Ok(u) => u,
            Err(_) => {
                let _ = publish(&out, Update::Error("UI Automation 初始化失败".into()));
                return;
            }
        };
        if let Ok(v) = ui.cast::<IUIAutomation2>() {
            let _ = v.SetConnectionTimeout(800);
            let _ = v.SetTransactionTimeout(800);
        }
        let mut last = Instant::now() - Duration::from_secs(3);
        let mut tabs: std::collections::HashMap<u64, (IUIAutomationElement, isize, u32, String)> =
            std::collections::HashMap::new();
        let mut serial = 0u64;
        loop {
            let request = rx.recv_timeout(Duration::from_millis(100));
            match request {
                Ok(Request::Stop) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Ok(Request::Activate(key, epoch)) => {
                    if !session_valid(epoch) {
                        continue;
                    }
                    let current = snapshot(&ui).unwrap_or_default();
                    let matches: Vec<_> = current.iter().filter(|(i, _)| i.key == key).collect();
                    if matches.len() == 1 && session_valid(epoch) {
                        let (item, e) = matches[0];
                        let windows = windows_for(&item.path);
                        if let Some(w) = windows.first() {
                            if !focus(w) {
                                let _ = publish(&out, Update::Error("系统暂时拒绝窗口聚焦".into()));
                            }
                        } else if !invoke(e) {
                            let _ =
                                publish(&out, Update::Error("任务栏项目已变化，请重新选择".into()));
                        }
                    } else {
                        let _ = publish(&out, Update::Error("任务栏项目已移除，请重新选择".into()));
                    }
                }
                Ok(Request::Details(key)) => {
                    let p = path(&key);
                    let mut details = windows_for(&p);
                    let windows = details.clone();
                    let condition = ui.CreatePropertyCondition(
                        UIA_ControlTypePropertyId,
                        &VARIANT::from(UIA_TabItemControlTypeId.0),
                    );
                    if let Ok(condition) = condition {
                        for w in windows {
                            if let Ok(root) = ui.ElementFromHandle(HWND(w.hwnd as *mut c_void)) {
                                if let Ok(elements) =
                                    root.FindAll(TreeScope_Descendants, &condition)
                                {
                                    for i in 0..elements.Length().unwrap_or(0).min(120) {
                                        let Ok(e) = elements.GetElement(i) else {
                                            continue;
                                        };
                                        let title = e
                                            .CurrentName()
                                            .map(|s| s.to_string())
                                            .unwrap_or_default();
                                        if title.is_empty() {
                                            continue;
                                        }
                                        serial += 1;
                                        tabs.insert(serial, (e, w.hwnd, w.pid, title.clone()));
                                        details.push(Detail {
                                            title,
                                            hwnd: w.hwnd,
                                            pid: w.pid,
                                            token: serial,
                                        });
                                    }
                                }
                            }
                        }
                    }
                    if tabs.len() > 1024 {
                        let min = serial.saturating_sub(512);
                        tabs.retain(|id, _| *id >= min);
                    }
                    let _ = publish(&out, Update::Details(key, details));
                }
                Ok(Request::Select(d, epoch)) => {
                    if !session_valid(epoch) {
                        continue;
                    }
                    if d.token == 0 {
                        if !focus(&d) {
                            let _ = publish(&out, Update::Error("窗口已关闭或无法切换".into()));
                        }
                    } else if let Some((element, hwnd, pid, title)) = tabs.get(&d.token) {
                        let mut live_pid = 0;
                        let live = HWND(*hwnd as *mut c_void);
                        GetWindowThreadProcessId(live, Some(&mut live_pid));
                        if session_valid(epoch)
                            && live_pid == *pid
                            && IsWindow(live).as_bool()
                            && element.CurrentName().is_ok_and(|s| s == title.as_str())
                        {
                            let selected = element
                                .GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(
                                    UIA_SelectionItemPatternId,
                                )
                                .and_then(|p| p.Select())
                                .is_ok();
                            if selected || invoke(element) {
                                let _ = focus(&d);
                            } else {
                                let _ =
                                    publish(&out, Update::Error("标签已变化，请重新选择".into()));
                            }
                        }
                    }
                }
                Err(_) => {}
            }
            if last.elapsed() >= Duration::from_secs(1) {
                last = Instant::now();
                match snapshot(&ui) {
                    Ok(v) => {
                        let _ =
                            publish(&out, Update::Items(v.into_iter().map(|(i, _)| i).collect()));
                    }
                    Err(e) => {
                        let _ = publish(&out, Update::Error(e));
                    }
                }
            }
        }
        drop(tabs);
        drop(ui);
        CoUninitialize();
    });
    (tx, updates)
}
unsafe fn process_path(pid: u32) -> Option<String> {
    let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buffer = vec![0u16; 32768];
    let mut len = buffer.len() as u32;
    let ok =
        QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, PWSTR(buffer.as_mut_ptr()), &mut len)
            .is_ok();
    let _ = CloseHandle(p);
    ok.then(|| String::from_utf16_lossy(&buffer[..len as usize]))
}
/// Window focus primitives adapted from MIT PiPie; no foreground-lock bypass is attempted.
unsafe fn focus(d: &Detail) -> bool {
    if LOCKED.load(Ordering::Relaxed) || !ENABLED.load(Ordering::Relaxed) {
        return false;
    }
    let hwnd = HWND(d.hwnd as *mut c_void);
    let mut pid = 0;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid != d.pid || !IsWindow(hwnd).as_bool() {
        return false;
    }
    if IsIconic(hwnd).as_bool() {
        let _ = ShowWindow(hwnd, SW_RESTORE);
    }
    let _ = SetForegroundWindow(hwnd);
    GetForegroundWindow() == hwnd
}

unsafe fn window_app_id(hwnd: HWND, pid: u32) -> Option<String> {
    use windows::Win32::{
        Storage::Packaging::Appx::GetApplicationUserModelId,
        System::Com::StructuredStorage::PropVariantToStringAlloc, UI::Shell::PropertiesSystem::*,
    };
    // Documented System.AppUserModel.ID property; explicit desktop AUMIDs need not be exe paths.
    const KEY: PROPERTYKEY = PROPERTYKEY {
        fmtid: windows::core::GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
        pid: 5,
    };
    if let Ok(store) = SHGetPropertyStoreForWindow::<_, IPropertyStore>(hwnd) {
        if let Ok(value) = store.GetValue(&KEY) {
            if let Ok(text) = PropVariantToStringAlloc(&value) {
                let value = text.to_string().ok().filter(|s| !s.is_empty());
                CoTaskMemFree(Some(text.0 as *const c_void));
                if value.is_some() {
                    return value;
                }
            }
        }
    }
    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut size = 1024;
    let mut text = vec![0u16; size as usize];
    let result = GetApplicationUserModelId(process, &mut size, PWSTR(text.as_mut_ptr()));
    let _ = CloseHandle(process);
    (result == ERROR_SUCCESS)
        .then(|| String::from_utf16_lossy(&text[..size.saturating_sub(1) as usize]))
}

fn session_valid(epoch: u64) -> bool {
    SESSION.load(Ordering::SeqCst) == epoch
        && !LOCKED.load(Ordering::Relaxed)
        && ENABLED.load(Ordering::Relaxed)
}
fn publish(out: &mpsc::Sender<Update>, update: Update) -> Result<(), mpsc::SendError<Update>> {
    let result = out.send(update);
    unsafe {
        let _ = PostMessageW(
            HWND(HWND_UI.load(Ordering::Relaxed) as *mut c_void),
            WAKE,
            WPARAM(0),
            LPARAM(0),
        );
    }
    result
}
