use super::*;

pub(super) unsafe fn repair() {
    let mut ready = false;
    with(|s| {
        if !s.choosing_apps && !s.locked {
            s.close_wheel();
            s.gesture.reset();
            s.choosing_apps = true;
            ready = true;
        }
    });
    if !ready {
        return;
    }
    let mtm = MainThreadMarker::new().unwrap();
    let dialog = NSAlert::new(mtm);
    dialog.setMessageText(ns_string!("修复辅助功能授权"));
    let trusted = platform::accessibility_trusted();
    if trusted {
        dialog.setInformativeText(ns_string!("当前授权已经生效，不需要清理。覆盖安装后若出现“开关已开但按键无反应”，可从新版本再次进入这里修复。"));
        dialog.addButtonWithTitle(ns_string!("好"));
    } else {
        dialog.setInformativeText(ns_string!("清理 ViGo 已失效的辅助功能记录，再打开系统设置。之后需要你为 ViGo 开启一次授权，无需手动删除、重新添加应用。只处理 ViGo 的辅助功能，不修改其他权限。"));
        dialog.addButtonWithTitle(ns_string!("取消"));
        dialog.addButtonWithTitle(ns_string!("清理并重新授权"));
    }
    let app = NSApplication::sharedApplication(mtm);
    dialog.setIcon(app.applicationIconImage().as_deref());
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
    let accepted = dialog.runModal() == NSAlertSecondButtonReturn;
    with(|s| {
        if !trusted && accepted {
            s.notice = "正在清理 ViGo 的旧授权…".into();
            platform::repair_accessibility(s.tx.clone());
        } else {
            s.choosing_apps = false;
        }
    });
}
