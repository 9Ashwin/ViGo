//! First-run installation only. File replacement is staged and validated before stopping the old app.
use crate::platform;
use objc2::{
    define_class, msg_send, rc::Retained, runtime::ProtocolObject, sel, AnyThread,
    MainThreadMarker, MainThreadOnly,
};
use objc2_app_kit::*;
use objc2_foundation::*;
use std::{
    fs, io,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const ID: &str = "com.vigo.desktop";
const DEST: &str = "/Applications/ViGo.app";
type Result<T> = std::result::Result<T, String>;
static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn unique() -> String {
    format!(
        "{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}
fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
fn fingerprint(path: &Path) -> Result<Option<(u64, u64)>> {
    match fs::symlink_metadata(path) {
        Ok(m) => {
            if m.file_type().is_symlink() || !m.is_dir() {
                return Err("目标位置不是普通应用目录，未覆盖。".into());
            }
            Ok(Some((m.dev(), m.ino())))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}
struct Prepared {
    stage: PathBuf,
    dest: PathBuf,
    snapshot: Option<(u64, u64)>,
    committed: bool,
}
impl Prepared {
    fn new(source: &Path, dest: &Path, validate: impl Fn(&Path) -> Result<()>) -> Result<Self> {
        if same_path(source, dest) {
            return Err("应用已经位于安装目录。".into());
        }
        validate(source)?;
        let snapshot = fingerprint(dest)?;
        if snapshot.is_some() {
            validate(dest)?;
        }
        let stage = dest
            .parent()
            .ok_or("安装目录无效")?
            .join(format!(".ViGo-install-{}.app", unique()));
        let prepared = Self {
            stage,
            dest: dest.to_owned(),
            snapshot,
            committed: false,
        };
        let result = Command::new("/usr/bin/ditto")
            .arg(source)
            .arg(&prepared.stage)
            .output()
            .map_err(|e| e.to_string())?;
        if !result.status.success() {
            return Err(
                "无法写入“应用程序”目录。请使用安装盘中的拖拽安装，由访达处理所需权限。".into(),
            );
        }
        validate(&prepared.stage)?;
        Ok(prepared)
    }
    fn commit(mut self) -> Result<Option<PathBuf>> {
        if fingerprint(&self.dest)? != self.snapshot {
            return Err("安装位置在准备期间发生变化，已停止更新。".into());
        }
        let backup = self
            .dest
            .parent()
            .unwrap()
            .join(format!(".ViGo-backup-{}.saved", unique()));
        if self.snapshot.is_some() {
            fs::rename(&self.dest, &backup).map_err(|e| e.to_string())?;
        }
        if let Err(error) = fs::rename(&self.stage, &self.dest) {
            if self.snapshot.is_some() {
                if let Err(restore) = fs::rename(&backup, &self.dest) {
                    return Err(format!(
                        "更新失败：{error}。原版本保留在 {}（还原失败：{restore}）。",
                        backup.display()
                    ));
                }
            }
            return Err(error.to_string());
        }
        self.committed = true;
        Ok(self.snapshot.map(|_| backup))
    }
}
impl Drop for Prepared {
    fn drop(&mut self) {
        if !self.committed && self.stage.exists() {
            let _ = fs::remove_dir_all(&self.stage);
        }
    }
}
struct Bundle {
    path: PathBuf,
    version: String,
    icon: PathBuf,
}
fn bundle(path: &Path) -> Result<Bundle> {
    let b = NSBundle::bundleWithPath(&NSString::from_str(&path.to_string_lossy()))
        .ok_or("不是完整的 ViGo.app 应用包。")?;
    if b.bundleIdentifier().is_none_or(|id| id.to_string() != ID) {
        return Err("此位置不是 ViGo，未覆盖。".into());
    }
    let value = |key: &str| -> Option<String> {
        let key = NSString::from_str(key);
        let object = b.objectForInfoDictionaryKey(&key)?;
        let description: Retained<NSString> = unsafe { msg_send![&*object, description] };
        Some(description.to_string())
    };
    let name = value("CFBundleIconFile").ok_or("应用图标信息缺失。")?;
    if Path::new(&name)
        .file_name()
        .is_none_or(|file| file.to_string_lossy() != name)
    {
        return Err("应用图标路径无效。".into());
    }
    let icon = path.join("Contents/Resources").join(name);
    if !icon.is_file() || !path.join("Contents/MacOS/ViGo").is_file() {
        return Err("应用包不完整，请重新下载安装包。".into());
    }
    Ok(Bundle {
        path: path.to_owned(),
        version: value("CFBundleVersion").unwrap_or_default(),
        icon,
    })
}
fn verify(path: &Path) -> Result<()> {
    bundle(path)?;
    let ok = Command::new("/usr/bin/codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(path)
        .output()
        .map_err(|e| e.to_string())?
        .status
        .success();
    if !ok {
        return Err("应用签名校验失败，原版本没有被替换。".into());
    }
    Ok(())
}
fn versions(value: &str) -> Vec<u64> {
    value.split('.').map(|n| n.parse().unwrap_or(0)).collect()
}
fn current_location() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    if macos.file_name()? != "MacOS" || contents.file_name()? != "Contents" {
        return None;
    }
    Some(contents.parent()?.to_owned())
}
fn current_bundle() -> Option<Bundle> {
    bundle(&current_location()?).ok()
}
pub unsafe fn set_icon(app: &NSApplication) {
    if let Some(bundle) = current_bundle() {
        if let Some(icon) = NSImage::initWithContentsOfFile(
            NSImage::alloc(),
            &NSString::from_str(&bundle.icon.to_string_lossy()),
        ) {
            app.setApplicationIconImage(Some(&icon));
        }
    }
}
unsafe fn alert(
    app: &NSApplication,
    title: &str,
    message: &str,
    buttons: &[&str],
    mtm: MainThreadMarker,
) -> isize {
    let previous_policy = app.activationPolicy();
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    let dialog = NSAlert::new(mtm);
    dialog.setAlertStyle(NSAlertStyle::Informational);
    dialog.setMessageText(&NSString::from_str(title));
    dialog.setInformativeText(&NSString::from_str(message));
    dialog.setIcon(app.applicationIconImage().as_deref());
    for button in buttons {
        dialog.addButtonWithTitle(&NSString::from_str(button));
    }
    dialog.layout();
    let window = dialog.window();
    window.center();
    window.makeKeyAndOrderFront(None);
    window.orderFrontRegardless();
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
    let response = dialog.runModal();
    app.setActivationPolicy(previous_policy);
    response
}
#[link(name = "CoreServices", kind = "framework")]
extern "C" {
    fn LSRegisterURL(url: platform::Ref, update: bool) -> i32;
}
unsafe fn refresh(path: &Path) {
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let _ = LSRegisterURL(Retained::as_ptr(&url) as platform::Ref, true);
    #[allow(deprecated)]
    NSWorkspace::sharedWorkspace()
        .noteFileSystemChanged_(&NSString::from_str(&path.to_string_lossy()));
}
unsafe fn stop_other_instances() -> bool {
    let current = std::process::id() as i32;
    let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(ns_string!(
        "com.vigo.desktop"
    ));
    for app in &apps {
        if app.processIdentifier() != current {
            app.terminate();
        }
    }
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        if !NSRunningApplication::runningApplicationsWithBundleIdentifier(ns_string!(
            "com.vigo.desktop"
        ))
        .iter()
        .any(|app| app.processIdentifier() != current)
        {
            return true;
        }
        NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.05));
    }
    false
}
struct InstallationLock(fs::File);
impl Drop for InstallationLock {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        // Release at the transaction boundary even if a concurrent spawn inherited the description.
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}
fn lock_installation(path: &Path) -> Result<InstallationLock> {
    use std::os::fd::AsRawFd;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("另一个 ViGo 安装流程正在进行，请先完成或关闭它。".into());
    }
    Ok(InstallationLock(lock))
}
unsafe fn launch(path: &Path, after_install: bool) -> bool {
    let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(ns_string!(
        "com.vigo.desktop"
    ));
    for running in &apps {
        if running
            .bundleURL()
            .and_then(|url| url.path())
            .is_some_and(|value| same_path(Path::new(&value.to_string()), path))
        {
            #[allow(deprecated)]
            running.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
            return true;
        }
    }
    if !stop_other_instances() {
        return false;
    }
    let mut command = Command::new("/usr/bin/open");
    command.args(["-n", "-a"]).arg(path);
    if after_install {
        command.args(["--args", "--after-install"]);
    }
    command.status().is_ok_and(|status| status.success())
}
define_class!(
 #[unsafe(super(NSObject))]
 #[thread_kind=MainThreadOnly]
 struct ViGoInstallerDelegate;
 unsafe impl NSObjectProtocol for ViGoInstallerDelegate{}
 unsafe impl NSApplicationDelegate for ViGoInstallerDelegate{
  #[unsafe(method(applicationDidFinishLaunching:))]
  fn finished(&self,_n:&NSNotification){unsafe{let _:()=msg_send![self,performSelector:sel!(performInstall:),withObject:None::<&objc2::runtime::AnyObject>,afterDelay:0.0f64];}}
 }
 impl ViGoInstallerDelegate{
  #[unsafe(method(performInstall:))]
  fn install(&self,_sender:Option<&objc2::runtime::AnyObject>){unsafe{let mtm=MainThreadMarker::new().unwrap();let app=NSApplication::sharedApplication(mtm);run_installation(&app,mtm);app.terminate(None);}}
 }
);
/// A modal shown after AppKit startup participates in the normal accessibility event loop.
pub unsafe fn bootstrap(app: &NSApplication, mtm: MainThreadMarker) -> bool {
    let Some(path) = current_location() else {
        return true;
    };
    if same_path(&path, Path::new(DEST)) && bundle(&path).is_ok() {
        return run_installation(app, mtm);
    }
    let delegate: Retained<ViGoInstallerDelegate> =
        msg_send![ViGoInstallerDelegate::alloc(mtm), init];
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.run();
    false
}
unsafe fn run_installation(app: &NSApplication, mtm: MainThreadMarker) -> bool {
    let Some(location) = current_location() else {
        return true;
    }; // cargo run is a developer entry.
    let source = match bundle(&location) {
        Ok(bundle) => bundle,
        Err(message) => {
            alert(app, "应用包不完整", &message, &["退出"], mtm);
            return false;
        }
    };
    let dest = Path::new(DEST);
    if same_path(&source.path, dest) {
        let defaults = NSUserDefaults::standardUserDefaults();
        if defaults
            .stringForKey(ns_string!("registeredIconVersion"))
            .is_none_or(|v| v.to_string() != source.version)
        {
            refresh(dest);
            defaults.setObject_forKey(
                Some(&NSString::from_str(&source.version)),
                ns_string!("registeredIconVersion"),
            );
        }
        return true;
    }
    let _install_lock =
        match lock_installation(&crate::model::Settings::path().with_extension("install-lock")) {
            Ok(lock) => lock,
            Err(message) => {
                alert(app, "安装正在进行", &message, &["好"], mtm);
                return false;
            }
        };
    if dest.exists() {
        match bundle(dest) {
            Ok(existing)
                if versions(&existing.version) >= versions(&source.version)
                    && verify(dest).is_ok() =>
            {
                if alert(app,"ViGo 已安装","“应用程序”中已有此版本或更新版本。请使用已安装的 ViGo，避免运行下载目录中的重复副本。",&["打开已安装版本","退出"],mtm)==NSAlertFirstButtonReturn{refresh(dest);launch(dest,false);}
                return false;
            }
            Ok(_) => {}
            Err(message) => {
                alert(app, "无法更新 ViGo", &message, &["好"], mtm);
                return false;
            }
        }
    }
    let updating = dest.exists();
    let title = if updating {
        "更新应用程序中的 ViGo"
    } else {
        "安装 ViGo"
    };
    let message = if updating {
        "将更新 /Applications/ViGo.app，并退出正在运行的旧版。原版本会先保留备份，不会生成“ViGo 2”。"
    } else {
        "将 ViGo 安装到“应用程序”目录，随后从那里启动。以后可在启动台或聚焦搜索中找到它。"
    };
    if alert(
        app,
        title,
        message,
        &[
            if updating {
                "更新并启动"
            } else {
                "安装并启动"
            },
            "退出",
        ],
        mtm,
    ) != NSAlertFirstButtonReturn
    {
        return false;
    }
    let mut installed = false;
    let result = (|| -> Result<()> {
        let prepared = Prepared::new(&source.path, dest, verify)?;
        if !stop_other_instances() {
            return Err("旧版尚未退出。请先关闭旧版 ViGo 的弹窗，再重新安装。".into());
        }
        let backup = prepared.commit()?;
        installed = true;
        refresh(dest);
        if let Some(backup) = backup {
            let url = NSURL::fileURLWithPath(&NSString::from_str(&backup.to_string_lossy()));
            let _ =
                NSFileManager::defaultManager().trashItemAtURL_resultingItemURL_error(&url, None);
        }
        if !launch(dest, true) {
            return Err("ViGo 已安装，但系统尚未允许自动启动。请从“应用程序”打开 ViGo。".into());
        }
        Ok(())
    })();
    if let Err(message) = result {
        let response = alert(
            app,
            if installed {
                "ViGo 已安装"
            } else {
                "安装未完成"
            },
            &message,
            &["在访达中查看", "退出"],
            mtm,
        );
        if response == NSAlertFirstButtonReturn {
            NSWorkspace::sharedWorkspace().activateFileViewerSelectingURLs(
                &NSArray::from_retained_slice(&[NSURL::fileURLWithPath(&NSString::from_str(
                    &if installed {
                        dest.to_path_buf()
                    } else {
                        source.path.clone()
                    }
                    .to_string_lossy(),
                ))]),
            );
        }
    }
    false
}
#[cfg(feature = "diagnostics")]
static GUIDES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(feature = "diagnostics")]
pub fn guide_count() -> usize {
    GUIDES.load(std::sync::atomic::Ordering::Relaxed)
}
pub unsafe fn welcome(app: &NSApplication, mtm: MainThreadMarker) {
    if current_bundle().is_none_or(|bundle| !same_path(&bundle.path, Path::new(DEST)))
        || platform::accessibility_trusted()
        || crate::ui::session_locked()
    {
        return;
    }
    #[cfg(feature = "diagnostics")]
    GUIDES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if alert(app,"ViGo 需要辅助功能","ViGo 已安装到“应用程序”。首次使用需要允许全局按键监听和软件切换。点击下方按钮，在系统设置中开启 ViGo；开启后自动生效，无需重启。",&["打开辅助功能","稍后设置"],mtm)==NSAlertFirstButtonReturn {
        #[cfg(feature="diagnostics")]
        if platform::permission_is_simulated(){return;}
        platform::request_accessibility();
        if let Some(url)=NSURL::URLWithString(ns_string!("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")){NSWorkspace::sharedWorkspace().openURL(&url);}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("vigo-install-test-{}", unique()));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn fake(path: &Path, id: &str) {
        fs::create_dir(path).unwrap();
        fs::write(path.join("identity"), id).unwrap();
    }
    fn guard(path: &Path) -> Result<()> {
        if fs::read_to_string(path.join("identity")).map_err(|e| e.to_string())? == ID {
            Ok(())
        } else {
            Err("foreign app".into())
        }
    }
    #[test]
    fn canonical_update_replaces_in_place_and_keeps_backup() {
        let s = Scratch::new();
        let source = s.0.join("ViGo 2.app");
        let dest = s.0.join("ViGo.app");
        fake(&source, ID);
        fake(&dest, ID);
        fs::write(source.join("version"), "new").unwrap();
        fs::write(dest.join("version"), "old").unwrap();
        let backup = Prepared::new(&source, &dest, guard)
            .unwrap()
            .commit()
            .unwrap()
            .unwrap();
        assert_eq!(fs::read_to_string(dest.join("version")).unwrap(), "new");
        assert_eq!(fs::read_to_string(backup.join("version")).unwrap(), "old");
        assert!(source.exists());
    }
    #[test]
    fn unrelated_destination_and_changed_destination_are_preserved() {
        let s = Scratch::new();
        let source = s.0.join("ViGo.app");
        let dest = s.0.join("Other.app");
        fake(&source, ID);
        fake(&dest, "other");
        assert!(Prepared::new(&source, &dest, guard).is_err());
        assert_eq!(fs::read_to_string(dest.join("identity")).unwrap(), "other");
        fs::remove_dir_all(&dest).unwrap();
        let prepared = Prepared::new(&source, &dest, guard).unwrap();
        fake(&dest, "other");
        assert!(prepared.commit().is_err());
        assert_eq!(fs::read_to_string(dest.join("identity")).unwrap(), "other");
    }
    #[test]
    fn failed_staging_validation_leaves_the_existing_install_untouched() {
        let s = Scratch::new();
        let source = s.0.join("Source.app");
        let dest = s.0.join("ViGo.app");
        fake(&source, ID);
        fake(&dest, ID);
        let result = Prepared::new(&source, &dest, |p| {
            if p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".ViGo-install-")
            {
                Err("signature failure".into())
            } else {
                guard(p)
            }
        });
        assert!(result.is_err());
        assert!(dest.join("identity").exists());
        assert_eq!(fs::read_dir(&s.0).unwrap().count(), 2);
    }
    #[test]
    fn failed_commit_restores_the_previous_installation() {
        let s = Scratch::new();
        let source = s.0.join("Source.app");
        let dest = s.0.join("ViGo.app");
        fake(&source, ID);
        fake(&dest, ID);
        fs::write(dest.join("version"), "old").unwrap();
        let staged = Prepared::new(&source, &dest, guard).unwrap();
        fs::remove_dir_all(&staged.stage).unwrap();
        assert!(staged.commit().is_err());
        assert_eq!(fs::read_to_string(dest.join("version")).unwrap(), "old");
    }
    #[test]
    fn symbolic_link_destinations_are_not_followed() {
        let scratch = Scratch::new();
        let source = scratch.0.join("Source.app");
        let other = scratch.0.join("Other.app");
        let dest = scratch.0.join("ViGo.app");
        fake(&source, ID);
        fake(&other, "other");
        std::os::unix::fs::symlink(&other, &dest).unwrap();
        assert!(Prepared::new(&source, &dest, guard).is_err());
        assert_eq!(fs::read_to_string(other.join("identity")).unwrap(), "other");
    }
    #[test]
    fn only_one_install_transaction_can_hold_the_install_lock() {
        let s = Scratch::new();
        let path = s.0.join("install-lock");
        let first = lock_installation(&path).unwrap();
        assert!(lock_installation(&path).is_err());
        let inherited = first.0.try_clone().unwrap();
        drop(first);
        assert!(lock_installation(&path).is_ok());
        drop(inherited);
    }
    #[test]
    fn release_versions_are_compared_numerically() {
        assert!(versions("510") > versions("504"));
        assert!(versions("0.5.10") > versions("0.5.4"));
    }
}
