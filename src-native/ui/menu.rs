use super::*;
impl State {
    pub(super) fn now(&self) -> f64 {
        self.birth.elapsed().as_secs_f64()
    }
    pub(super) fn title(&self, path: &str) -> String {
        self.active_names
            .get(path)
            .cloned()
            .or_else(|| self.settings.names.get(path).cloned())
            .unwrap_or_else(|| self.settings.name(path))
    }
    pub(super) fn path_at(&self, i: usize) -> String {
        self.active_paths.get(i).cloned().unwrap_or_default()
    }
    pub(super) unsafe fn load_icons(&mut self) {
        for p in &self.settings.all_apps() {
            if !self.icons.contains_key(p) {
                self.icons.insert(
                    p.clone(),
                    NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(p)),
                );
            }
        }
    }
    pub(super) unsafe fn apply_theme(&self) {
        let name = match self.settings.appearance.as_str() {
            "light" => Some(ns_string!("NSAppearanceNameAqua")),
            "dark" => Some(ns_string!("NSAppearanceNameDarkAqua")),
            _ => None,
        };
        let appearance = name.and_then(NSAppearance::appearanceNamed);
        self.app.setAppearance(appearance.as_deref());
    }
    pub(super) unsafe fn save(&mut self) {
        self.gesture.reset();
        self.close_wheel();
        self.settings.normalize();
        if let Err(e) = self.settings.save() {
            self.notice = format!("设置保存失败：{e}");
            self.show_failure();
        }
        self.apply_theme();
        let keep = self.settings.all_apps();
        self.icons.retain(|path, _| {
            keep.contains(path) || (self.closing.is_some() && self.active_paths.contains(path))
        });
        self.load_icons();
        self.refresh_preferences_status();
    }
    pub(super) fn refresh_dock(&mut self) {
        if self.dock_pending || self.locked {
            return;
        }
        self.dock_pending = true;
        platform::dock(self.tx.clone());
    }
    pub(super) unsafe fn item(
        &self,
        menu: &NSMenu,
        title: &str,
        tag: isize,
        checked: bool,
        enabled: bool,
    ) -> Retained<NSMenuItem> {
        let i = NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(MainThreadMarker::new().unwrap()),
            &NSString::from_str(title),
            if tag >= 0 { Some(sel!(action:)) } else { None },
            ns_string!(""),
        );
        i.setTarget(Some(&*self.delegate));
        i.setTag(tag);
        i.setEnabled(enabled);
        i.setState(if checked { 1 } else { 0 });
        menu.addItem(&i);
        i
    }
    pub(super) unsafe fn submenu(&self, menu: &NSMenu, title: &str) -> Retained<NSMenu> {
        let i = self.item(menu, title, -1, false, true);
        let sub = NSMenu::new(MainThreadMarker::new().unwrap());
        sub.setAutoenablesItems(false);
        i.setSubmenu(Some(&sub));
        sub
    }
    pub(super) fn dock_summary(&self) -> String {
        format!(
            "{} 个项目 · {}",
            self.settings.visible_apps().len(),
            if self.dock_pending {
                "自动更新中"
            } else if self.dock_exact {
                "实时跟随 Dock"
            } else {
                "等待辅助功能授权"
            }
        )
    }
    pub(super) unsafe fn rebuild_menu(&mut self) {
        let m = &self.status_menu;
        m.removeAllItems();
        m.setAutoenablesItems(false);
        let header = native("NSView", r(0., 0., 290., 82.));
        let icon = NSImageView::new(MainThreadMarker::new().unwrap());
        icon.setImage(self.app.applicationIconImage().as_deref());
        icon.setFrame(rect(r(14., 42., 28., 28.)));
        header.addSubview(&icon);
        label(
            &header,
            "ViGo · 软件轮盘",
            r(50., 47., 224., 23.),
            14.,
            true,
            false,
        );
        let info = NSTextField::labelWithString(
            &NSString::from_str(&self.dock_summary()),
            MainThreadMarker::new().unwrap(),
        );
        info.setFrame(rect(r(14., 25., 262., 19.)));
        info.setFont(Some(&NSFont::systemFontOfSize(11.)));
        info.setTextColor(Some(&color("secondaryLabelColor")));
        header.addSubview(&info);
        self.menu_dock_label = Some(info);
        label(
            &header,
            &format!("按住 {} · 移动选择 · 松开执行", self.trigger_label()),
            r(14., 6., 262., 18.),
            10.,
            false,
            true,
        );
        let item = self.item(m, "ViGo", -1, false, false);
        item.setView(Some(&header));
        m.addItem(&NSMenuItem::separatorItem(MainThreadMarker::new().unwrap()));
        self.item(
            m,
            if self.settings.enabled_on_start {
                "暂停 ViGo"
            } else {
                "启用 ViGo"
            },
            2,
            false,
            true,
        );
        self.item(
            m,
            "试一下轮盘",
            20,
            false,
            !self.settings.visible_apps().is_empty(),
        );
        let settings_item = self.item(m, "设置…", 500, false, true);
        settings_item.setKeyEquivalent(ns_string!(","));
        m.addItem(&NSMenuItem::separatorItem(MainThreadMarker::new().unwrap()));
        if !self.trusted {
            self.item(m, "开启辅助功能权限…", 21, false, true);
        } else if self.tap.is_null() || !platform::CGEventTapIsEnabled(self.tap) {
            self.item(m, "重新连接输入监听", 22, false, true);
            self.item(m, "检查辅助功能授权…", 21, false, true);
        } else if platform::secure_input_enabled() {
            self.item(m, "安全键盘输入已开启，快捷键暂不可用", -1, false, false);
        } else if self.tap_keyboard != Some(true) {
            self.item(m, "监听已连接，键盘状态待确认", -1, false, false);
        } else {
            self.item(m, "输入监听已就绪", -1, false, false);
        }
        self.item(m, "复制诊断信息", 23, false, true);
        self.item(
            m,
            &format!("关于 ViGo {}", env!("CARGO_PKG_VERSION")),
            50,
            false,
            true,
        );
        let q = self.item(m, "退出 ViGo", 3, false, true);
        q.setKeyEquivalent(ns_string!("q"));
    }
    pub(super) fn trigger_label(&self) -> String {
        match &self.settings.trigger {
            Trigger::Key { vk } => input::key_label(*vk),
            Trigger::Mouse(n) => {
                if n == "mouse4" {
                    "侧键 4".into()
                } else {
                    "侧键 5".into()
                }
            }
        }
    }
    pub(super) unsafe fn dismiss_capture(&mut self) {
        if let Some(w) = self.capture_window.take() {
            w.orderOut(None);
        }
    }
    unsafe fn capture_key(&mut self) {
        self.close_wheel();
        self.gesture.reset();
        self.dismiss_capture();
        self.capturing = true;
        let w = NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(MainThreadMarker::new().unwrap()),
            NSRect::new(NSPoint::ZERO, NSSize::new(370., 100.)),
            NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
            NSBackingStoreType::Buffered,
            false,
        );
        w.setReleasedWhenClosed(false);
        w.setTitle(ns_string!("设置呼出键"));
        w.setDelegate(Some(ProtocolObject::from_ref(&*self.delegate)));
        let root = native("NSView", r(0., 0., 370., 100.));
        label(
            &root,
            "按下一个键作为呼出键",
            r(22., 55., 326., 24.),
            17.,
            true,
            false,
        );
        label(
            &root,
            "Esc 取消；切换到其他软件也会取消。",
            r(22., 24., 326., 20.),
            12.,
            false,
            true,
        );
        w.setContentView(Some(&root));
        w.center();
        self.capture_window = Some(w.clone());
        w.makeKeyAndOrderFront(None);
        #[allow(deprecated)]
        self.app.activateIgnoringOtherApps(true);
    }
    pub(super) unsafe fn action(&mut self, tag: isize, _sender: &AnyObject) {
        match tag {
            810 => {
                if let Err(error) = self.select_mascot(None) {
                    self.notice = error;
                    self.show_failure();
                }
            }
            811 | 812 => self.copy_mascot_prompt(tag == 812),
            813 => open_url("https://codex-pets.net/"),
            814 => {
                if self
                    .mascot
                    .asset
                    .source_url
                    .starts_with("https://codex-pets.net/")
                {
                    open_url(&self.mascot.asset.source_url);
                }
            }
            500 => self.show_preferences(),
            501 => {
                self.settings.hidden_apps.clear();
                self.save();
            }
            502 => {
                self.settings.app_order.clear();
                self.save();
                self.refresh_preferences_apps();
            }
            2 => {
                self.settings.enabled_on_start = !self.settings.enabled_on_start;
                self.save();
            }
            3 => self.app.terminate(None),
            20 => {
                let f = NSScreen::mainScreen(MainThreadMarker::new().unwrap())
                    .unwrap()
                    .frame();
                self.open_wheel(
                    f.origin.x + f.size.width / 2.,
                    f.origin.y + f.size.height / 2.,
                );
                self.trial = Some(self.now() + 3.);
            }
            21 => {
                platform::request_accessibility();
                open_url(
                    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
                );
            }
            22 => {
                self.tap_failures = 0;
                self.tap_retry_at = 0.;
                self.tap_attempted = false;
                self.notice.clear();
                self.remove_hook();
                self.install_hook();
            }
            23 => {
                let report = self.input_report();
                let clipboard = NSPasteboard::generalPasteboard();
                clipboard.clearContents();
                clipboard.setString_forType(
                    &NSString::from_str(&report.to_string()),
                    NSPasteboardTypeString,
                );
            }
            30 => self.refresh_dock(),
            31 => open_url("x-apple.systempreferences:com.apple.preference.dock"),
            40 => {
                let _ = libc::dlopen(
                    c"/System/Library/Frameworks/ServiceManagement.framework/ServiceManagement"
                        .as_ptr(),
                    libc::RTLD_LAZY,
                );
                if let Some(service) = AnyClass::get(c"SMAppService") {
                    let s: Retained<AnyObject> = msg_send![service, mainAppService];
                    let mut error: *mut AnyObject = ptr::null_mut();
                    let ok: bool = if self.settings.launch_at_login {
                        msg_send![&*s,unregisterAndReturnError:&mut error]
                    } else {
                        msg_send![&*s,registerAndReturnError:&mut error]
                    };
                    if ok {
                        self.settings.launch_at_login = !self.settings.launch_at_login;
                        self.save();
                    } else {
                        self.notice = "登录项未更新，请检查系统设置中的登录项。".into();
                        self.show_failure();
                    }
                }
            }
            50 => {
                self.app.orderFrontStandardAboutPanel(None);
                #[allow(deprecated)]
                self.app.activateIgnoringOtherApps(true);
            }
            100 => {
                self.settings.trigger = Trigger::default();
                self.save();
            }
            101 => self.capture_key(),
            102 | 103 => {
                self.settings.trigger =
                    Trigger::Mouse(if tag == 102 { "mouse4" } else { "mouse5" }.into());
                self.save();
            }
            1420 | 1520 | 1640 => {
                self.settings.wheel_size_px = (tag - 1000) as f64;
                self.save();
            }
            300..=302 => {
                self.settings.appearance = ["system", "light", "dark"][(tag - 300) as usize].into();
                self.save();
            }
            41 => {
                self.settings.show_window_list = !self.settings.show_window_list;
                self.windows.reset();
                self.sync_window_glass();
                self.canvas.setNeedsDisplay(true);
                self.save();
            }
            _ => {}
        }
    }
    pub(super) unsafe fn input_report(&self) -> serde_json::Value {
        serde_json::json!({
            "version":env!("CARGO_PKG_VERSION"),
            "macOS":NSProcessInfo::processInfo().operatingSystemVersionString().to_string(),
            "arch":std::env::consts::ARCH,
            "enabled":self.settings.enabled_on_start,
            "trigger":self.trigger_label(),
            "accessibility":platform::accessibility_trusted(),
            "sessionLocked":session_locked(),
            "secureInput":platform::secure_input_enabled(),
            "tapLocation":"session",
            "tapConnected":!self.tap.is_null(),
            "tapEnabled":!self.tap.is_null() && platform::CGEventTapIsEnabled(self.tap),
            "keyboardSubscribed":self.tap_keyboard,
            "keyboardEventsSeen":self.key_events_seen,
            "dockApps":self.settings.wheel.len(),
            "wheelApps":self.settings.visible_apps().len(),
            "notice":self.notice,
        })
    }
}

impl State {
    /// Dock exposes Created/Destroyed on its AXList; preferences and workspace events cover reorder/restart.
    pub(super) unsafe fn ensure_dock_observer(&mut self) {
        let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(ns_string!(
            "com.apple.dock"
        ));
        let pid = apps
            .firstObject()
            .map(|a| a.processIdentifier())
            .unwrap_or(0);
        if pid == 0
            || (pid == self.dock_observer_pid && !self.dock_observer.is_null())
            || self.now() < self.observer_retry
        {
            return;
        }
        if !self.dock_observer.is_null() {
            platform::CFRunLoopRemoveSource(
                platform::CFRunLoopGetMain(),
                platform::AXObserverGetRunLoopSource(self.dock_observer),
                platform::kCFRunLoopCommonModes,
            );
            platform::CFRelease(self.dock_observer);
            self.dock_observer = ptr::null_mut();
        }
        for element in self.dock_observed.drain(..) {
            platform::CFRelease(element);
        }
        self.observer_failures = self.observer_failures.saturating_add(1);
        self.observer_retry =
            self.now() + (0.5 * 2f64.powi(self.observer_failures.min(6) as i32)).min(30.);
        let root = platform::AXUIElementCreateApplication(pid);
        platform::AXUIElementSetMessagingTimeout(root, 0.05);
        let mut observer = ptr::null_mut();
        if platform::AXObserverCreate(pid, dock_changed, &mut observer) != 0 {
            platform::CFRelease(root);
            return;
        }
        self.dock_observer = observer;
        let mut registered = 0;
        let mut children = ptr::null_mut();
        if platform::AXUIElementCopyAttributeValue(
            root,
            ns_string!("AXChildren") as *const NSString as platform::Ref,
            &mut children,
        ) == 0
            && !children.is_null()
        {
            for i in 0..platform::CFArrayGetCount(children) {
                let child = platform::CFArrayGetValueAtIndex(children, i);
                let mut role = ptr::null_mut();
                if platform::AXUIElementCopyAttributeValue(
                    child,
                    ns_string!("AXRole") as *const NSString as platform::Ref,
                    &mut role,
                ) == 0
                    && !role.is_null()
                {
                    if platform::CFEqual(
                        role,
                        ns_string!("AXList") as *const NSString as platform::Ref,
                    ) {
                        self.dock_observed.push(platform::CFRetain(child));
                        for name in [ns_string!("AXCreated"), ns_string!("AXUIElementDestroyed")] {
                            if platform::AXObserverAddNotification(
                                observer,
                                child,
                                name as *const NSString as platform::Ref,
                                ptr::null_mut(),
                            ) == 0
                            {
                                registered += 1;
                            }
                        }
                    }
                    platform::CFRelease(role);
                }
            }
            platform::CFRelease(children);
        }
        self.dock_observed.push(root);
        if registered == 0 {
            platform::CFRelease(observer);
            self.dock_observer = ptr::null_mut();
            return;
        }
        self.dock_observer_pid = pid;
        self.observer_failures = 0;
        self.observer_retry = 0.;
        platform::CFRunLoopAddSource(
            platform::CFRunLoopGetMain(),
            platform::AXObserverGetRunLoopSource(observer),
            platform::kCFRunLoopCommonModes,
        );
    }
}
