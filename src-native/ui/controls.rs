use super::*;

impl State {
    pub(super) unsafe fn dialog(
        &self,
        title: &str,
        values: &[(&str, &str)],
    ) -> Option<Vec<String>> {
        let a: Retained<AnyObject> = msg_send![class("NSAlert"), new];
        let _: () = msg_send![&*a,setMessageText:&*NSString::from_str(title)];
        let _: Retained<AnyObject> = msg_send![&*a,addButtonWithTitle:ns_string!("保存")];
        let _: Retained<AnyObject> = msg_send![&*a,addButtonWithTitle:ns_string!("取消")];
        let root = native("NSView", r(0., 0., 360., values.len() as f64 * 62.));
        let mut fields = vec![];
        for (i, (name, value)) in values.iter().enumerate() {
            let y = (values.len() - i - 1) as f64 * 62.;
            label(&root, name, r(0., y + 36., 360., 20.), 11., false, true);
            let f = native("NSTextField", r(0., y + 5., 360., 26.));
            let _: () = msg_send![&*f,setStringValue:&*NSString::from_str(value)];
            root.addSubview(&f);
            fields.push(f);
        }
        let _: () = msg_send![&*a,setAccessoryView:&*root];
        let response: isize = msg_send![&*a, runModal];
        if response == 1000 {
            Some(fields.iter().map(|f| string(f)).collect())
        } else {
            None
        }
    }
    pub(super) unsafe fn browse(&self) -> Vec<String> {
        let p: NSRetainedPanel = msg_send![class("NSOpenPanel"), openPanel];
        let _: () = msg_send![&*p, setTitle: ns_string!("添加文件或文件夹")];
        let _: () = msg_send![&*p, setPrompt: ns_string!("添加")];
        let _: () = msg_send![&*p,setCanChooseFiles:true];
        let _: () = msg_send![&*p,setCanChooseDirectories:true];
        let _: () = msg_send![&*p,setAllowsMultipleSelection:true];
        let result: isize = msg_send![&*p, runModal];
        if result != 1 {
            return vec![];
        }
        let urls: Retained<NSArray<NSURL>> = msg_send![&*p, URLs];
        urls.iter()
            .filter_map(|u| u.path().map(|p| p.to_string()))
            .collect()
    }
    pub(super) unsafe fn picker_window(
        &mut self,
        title: &str,
        size: (f64, f64),
    ) -> Retained<NSView> {
        if let Some(w) = self.picker.as_ref() {
            w.orderOut(None);
        }
        self.search = None;
        self.picker_buttons.clear();
        let w = NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(MainThreadMarker::new().unwrap()),
            NSRect::new(NSPoint::ZERO, NSSize::new(size.0, size.1)),
            NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
            NSBackingStoreType::Buffered,
            false,
        );
        w.setReleasedWhenClosed(false);
        w.setTitle(&NSString::from_str(title));
        w.setDelegate(Some(ProtocolObject::from_ref(&*self.delegate)));
        w.center();
        let root = native("NSView", r(0., 0., size.0, size.1));
        w.setContentView(Some(&root));
        self.picker = Some(w);
        root
    }
    pub(super) unsafe fn show_picker(&mut self) {
        let root = self.picker_window("选择软件", (620., 520.));
        let search = native("NSSearchField", r(20., 466., 580., 30.));
        let _: () =
            msg_send![&*search,setPlaceholderString:ns_string!("搜索软件，中英文名称都可以")];
        let _: () = msg_send![&*search,setSendsWholeSearchString:false];
        let _: () = msg_send![&*search,setSendsSearchStringImmediately:true];
        self.control(&search, 501);
        root.addSubview(&search);
        self.search = Some(search);
        let scroll = native("NSScrollView", r(20., 20., 580., 430.));
        let _: () = msg_send![&*scroll,setHasVerticalScroller:true];
        let _: () = msg_send![&*scroll,setDrawsBackground:false];
        root.addSubview(&scroll);
        let doc = native("NSView", r(0., 0., 560., 430.));
        let _: () = msg_send![&*scroll,setDocumentView:&*doc];
        self.picker_doc = Some(doc);
        self.filter_apps();
        self.picker.as_ref().unwrap().makeKeyAndOrderFront(None);
        let _: bool =
            msg_send![&**self.picker.as_ref().unwrap(),makeFirstResponder:self.search.as_deref()];
    }
    pub(super) unsafe fn filter_apps(&mut self) {
        let Some(doc) = self.picker_doc.clone() else {
            return;
        };
        let Some(search) = &self.search else { return };
        let query = string(search).to_lowercase();
        let indices: Vec<usize> = self
            .catalog
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                query.is_empty()
                    || a.name.to_lowercase().contains(&query)
                    || a.path.to_lowercase().contains(&query)
                    || a.aliases.iter().any(|v| v.to_lowercase().contains(&query))
            })
            .map(|(i, _)| i)
            .collect();
        clear(&doc);
        self.picker_buttons.clear();
        let height = (indices.len() as f64 * 46.).max(430.);
        doc.setFrameSize(NSSize::new(560., height));
        for (row, index) in indices.iter().enumerate() {
            let a = &self.catalog[*index];
            let b = self.button(
                &doc,
                &a.name,
                1000 + *index as isize,
                r(6., height - (row + 1) as f64 * 46., 540., 42.),
            );
            let _: () = msg_send![&*b,setBordered:false];
            let _: () = msg_send![&*b,setAlignment:0isize];
            self.picker_buttons.push((a.path.clone(), b));
        }
        if indices.is_empty() {
            label(
                &doc,
                if self.catalog.is_empty() {
                    "正在读取软件目录…"
                } else {
                    "没有匹配的软件"
                },
                r(18., 380., 500., 24.),
                13.,
                false,
                true,
            );
        }
        if let Some(clip) = doc.superview() {
            let _: () = msg_send![&*clip,scrollToPoint:NSPoint::new(0.,height-430.)];
        }
        self.picker_icons();
    }
    pub(super) unsafe fn picker_icons(&mut self) {
        let Some(doc) = &self.picker_doc else { return };
        let visible = doc.visibleRect();
        let mut loaded = 0;
        for (path, b) in &self.picker_buttons {
            let f = b.frame();
            if f.origin.y + f.size.height < visible.origin.y
                || f.origin.y > visible.origin.y + visible.size.height
            {
                continue;
            }
            if !self.icons.contains_key(path) {
                if loaded >= 4 {
                    continue;
                }
                self.icons.insert(
                    path.clone(),
                    NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(path)),
                );
                loaded += 1;
            }
            let _: () = msg_send![&**b,setImage:self.icons.get(path).map(|i|&**i)];
            let _: () = msg_send![&**b,setImagePosition:2usize];
            let _: () = msg_send![&**b,setImageScaling:3usize];
        }
    }
    pub(super) unsafe fn add_path(&mut self, path: String) {
        if !self.settings.assign(self.edit_slot, path) {
            self.notice = "最多固定 24 个动作，请先移除一个".into();
        }
        self.save();
        self.settings_page();
    }
    pub(super) unsafe fn add_paths(&mut self, paths: Vec<String>) -> bool {
        let total = paths.len();
        let (last, added) = self.settings.assign_many(self.edit_slot, paths);
        self.edit_slot = last;
        if added < total {
            self.notice = format!("已添加 {added} 项，轮盘已满，其余项目未添加");
        }
        self.save();
        self.settings_page();
        added > 0
    }
    pub(super) unsafe fn show_outer(&mut self) {
        let path = self.outer_owner.clone();
        if !self.settings.wheel.contains(&path) {
            return;
        }
        if path.is_empty() {
            return;
        }
        let root = self.picker_window(&format!("{} · 外环内容", self.title(&path)), (700., 540.));
        label(
            &root,
            "常用内容，跟随软件展开。",
            r(20., 490., 660., 22.),
            17.,
            true,
            false,
        );
        self.button(&root, "添加文件或文件夹…", 600, r(20., 446., 180., 30.));
        self.button(&root, "添加网址…", 601, r(212., 446., 130., 30.));
        label(
            &root,
            "固定项目最多 24 个；窗口与浏览器标签自动加入。",
            r(20., 412., 650., 23.),
            11.,
            false,
            true,
        );
        let scroll = native("NSScrollView", r(16., 20., 668., 385.));
        let _: () = msg_send![&*scroll,setHasVerticalScroller:true];
        let _: () = msg_send![&*scroll,setDrawsBackground:false];
        root.addSubview(&scroll);
        let doc = native("NSView", r(0., 0., 648., 385.));
        let _: () = msg_send![&*scroll,setDocumentView:&*doc];
        self.picker_doc = Some(doc);
        self.outer_rows();
        self.picker.as_ref().unwrap().makeKeyAndOrderFront(None);
        self.scan();
    }
    pub(super) unsafe fn outer_rows(&mut self) {
        let Some(doc) = self.picker_doc.clone() else {
            return;
        };
        clear(&doc);
        let path = &self.outer_owner;
        if !self.settings.wheel.contains(path) {
            label(
                &doc,
                "所属软件已移除，请重新选择。",
                r(10., 330., 620., 25.),
                12.,
                false,
                true,
            );
            return;
        }
        let pins = self.settings.content.get(path).cloned().unwrap_or_default();
        let tabs: Vec<_> = self.tabs.iter().filter(|t| &t.app_path == path).collect();
        let h = ((pins.len() + tabs.len()) as f64 * 48. + 40.).max(385.);
        doc.setFrameSize(NSSize::new(648., h));
        for (i, pin) in pins.iter().enumerate() {
            let y = h - (i + 1) as f64 * 48.;
            label(
                &doc,
                &pin.name,
                r(8., y + 22., 340., 20.),
                13.,
                false,
                false,
            );
            label(
                &doc,
                &pin.target,
                r(8., y + 4., 370., 18.),
                10.,
                false,
                true,
            );
            self.button(&doc, "改名", 610 + i as isize, r(382., y + 10., 60., 26.));
            self.button(&doc, "↑", 800 + i as isize, r(448., y + 10., 38., 26.));
            self.button(&doc, "↓", 900 + i as isize, r(491., y + 10., 38., 26.));
            self.button(&doc, "移除", 700 + i as isize, r(536., y + 10., 75., 26.));
        }
        for (i, tab) in tabs.iter().enumerate() {
            let y = h - (pins.len() + i + 1) as f64 * 48.;
            label(
                &doc,
                &format!("{} {}", if tab.selected { "●" } else { "○" }, tab.title),
                r(8., y + 15., 620., 25.),
                12.,
                false,
                true,
            );
        }
        if pins.is_empty() && tabs.is_empty() {
            label(
                &doc,
                "添加文件夹或网址；浏览器启动后会自动读取标签。",
                r(10., 330., 620., 25.),
                12.,
                false,
                true,
            );
        }
    }
    pub(super) unsafe fn action(&mut self, tag: isize, sender: &AnyObject) {
        match tag {
            0 | 1 => self.show_settings(),
            2 => {
                self.settings.enabled_on_start = !self.settings.enabled_on_start;
                self.gesture.cancel();
                self.close_wheel();
                self.save();
                self.settings_page();
            }
            3 => self.app.terminate(None),
            10..=12 => {
                self.tab = (tag - 10) as usize;
                self.capturing = false;
                self.settings_page();
            }
            20 => {
                let p = NSEvent::mouseLocation();
                self.open_wheel(p.x, p.y);
                self.trial = Some(self.now() + 3.);
            }
            21 => {
                let opts = NSDictionary::<NSString, AnyObject>::from_slices(
                    &[ns_string!("AXTrustedCheckOptionPrompt")],
                    &[&*NSNumber::numberWithBool(true)],
                );
                platform::AXIsProcessTrustedWithOptions(Retained::as_ptr(&opts) as *mut c_void);
                open_url(
                    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
                );
            }
            22 => {
                self.tap_attempted = false;
                self.notice.clear();
                self.install_hook();
                self.settings_page();
            }
            30 => {
                self.close_wheel();
                self.capturing = !self.capturing;
                self.settings_page();
            }
            31 => {
                let i: isize = msg_send![sender, indexOfSelectedItem];
                self.settings.resize_slots(i.clamp(0, 8) as usize + 4);
                self.edit_slot = 0;
                self.page = 0;
                self.save();
                self.settings_page();
            }
            33 => {
                let i: isize = msg_send![sender, indexOfSelectedItem];
                self.settings.trigger = match i {
                    1 => Trigger::Mouse("mouse4".into()),
                    2 => Trigger::Mouse("mouse5".into()),
                    _ => Trigger::default(),
                };
                self.save();
                self.settings_page();
            }
            32 => {
                let value: f64 = msg_send![sender, doubleValue];
                self.settings.wheel_size_px = (value / 20.).round() * 20.;
                self.save();
                self.settings_page();
            }
            40 => {
                let i: isize = msg_send![sender, indexOfSelectedItem];
                self.settings.appearance =
                    ["system", "light", "dark"][i.clamp(0, 2) as usize].into();
                self.save();
                self.settings_page();
            }
            41 => {
                let value: isize = msg_send![sender, state];
                self.settings.enabled_on_start = value == 1;
                self.gesture.cancel();
                self.close_wheel();
                self.save();
                self.settings_page();
            }
            42 => {
                let _ = libc::dlopen(
                    c"/System/Library/Frameworks/ServiceManagement.framework/ServiceManagement"
                        .as_ptr(),
                    libc::RTLD_LAZY,
                );
                let on: isize = msg_send![sender, state];
                if let Some(service) = AnyClass::get(c"SMAppService") {
                    let s: Retained<AnyObject> = msg_send![service, mainAppService];
                    let mut error: *mut AnyObject = ptr::null_mut();
                    let ok: bool = if on == 1 {
                        msg_send![&*s,registerAndReturnError:&mut error]
                    } else {
                        msg_send![&*s,unregisterAndReturnError:&mut error]
                    };
                    if ok {
                        self.settings.launch_at_login = on == 1
                    } else {
                        self.notice = "登录项未能更新，请在系统设置的登录项中检查".into();
                    }
                }
                self.save();
                self.settings_page();
            }
            43 => {
                let on: isize = msg_send![sender, state];
                self.settings.window_previews = on == 1;
                if on == 1 && !platform::CGPreflightScreenCaptureAccess() {
                    platform::CGRequestScreenCaptureAccess();
                }
                self.save();
            }
            44 => {
                if !platform::CGPreflightScreenCaptureAccess() {
                    platform::CGRequestScreenCaptureAccess();
                    open_url("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture");
                }
            }
            50 => self.show_picker(),
            51 => {
                let paths = self.browse();
                self.add_paths(paths);
            }
            52 => {
                if let Some(p) = self
                    .settings
                    .wheel
                    .get(self.edit_slot)
                    .cloned()
                    .filter(|p| !p.is_empty())
                {
                    if let Some(v) = self.dialog("重命名", &[("名称", &self.title(&p))]) {
                        if !v[0].trim().is_empty() {
                            self.settings.names.insert(p, v[0].trim().into());
                            self.save();
                            self.settings_page();
                        }
                    }
                }
            }
            53 => {
                if let Some(p) = self.settings.wheel.get_mut(self.edit_slot) {
                    let old = std::mem::take(p);
                    self.settings.content.remove(&old);
                    self.settings.names.remove(&old);
                }
                self.save();
                self.settings_page();
            }
            54 | 55 => {
                let n = self.settings.wheel.len();
                let target = if tag == 54 {
                    (self.edit_slot + n - 1) % n
                } else {
                    (self.edit_slot + 1) % n
                };
                self.settings.wheel.swap(target, self.edit_slot);
                self.edit_slot = target;
                self.page = target / self.settings.slots_per_page;
                self.save();
                self.settings_page();
            }
            56 | 57 => {
                let pages = self
                    .settings
                    .wheel
                    .len()
                    .div_ceil(self.settings.slots_per_page);
                self.page = if tag == 56 {
                    (self.page + pages - 1) % pages
                } else {
                    (self.page + 1) % pages
                };
                self.edit_slot = self.page * self.settings.slots_per_page;
                self.settings_page();
            }
            58 => {
                self.outer_owner = self
                    .settings
                    .wheel
                    .get(self.edit_slot)
                    .cloned()
                    .unwrap_or_default();
                self.show_outer();
            }
            501 => self.filter_apps(),
            600 | 601 => {
                let path = self.outer_owner.clone();
                if !self.settings.wheel.contains(&path) {
                    return;
                }
                let additions = if tag == 600 {
                    self.browse()
                        .into_iter()
                        .map(|target| Pin {
                            id: format!("{}-{}", self.now(), target),
                            name: std::path::Path::new(&target)
                                .file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into(),
                            target,
                        })
                        .collect::<Vec<_>>()
                } else if let Some(values) =
                    self.dialog("添加网址", &[("名称", ""), ("网址", "https://")])
                {
                    if (values[1].starts_with("https://") || values[1].starts_with("http://"))
                        && !values[0].trim().is_empty()
                    {
                        vec![Pin {
                            id: format!("url-{}", self.now()),
                            name: values[0].clone(),
                            target: values[1].clone(),
                        }]
                    } else {
                        self.notice = "请输入名称与 http / https 网址".into();
                        vec![]
                    }
                } else {
                    vec![]
                };
                let entries = self.settings.content.entry(path).or_default();
                let room = 24usize.saturating_sub(entries.len());
                entries.extend(additions.into_iter().take(room));
                self.save();
                self.outer_rows();
            }
            610..=633 | 700..=723 | 800..=823 | 900..=923 => {
                let path = self.outer_owner.clone();
                if !self.settings.wheel.contains(&path) {
                    return;
                }
                let i = (tag % 100) as usize;
                let i = if tag < 700 { i - 10 } else { i };
                let mut entries = self
                    .settings
                    .content
                    .get(&path)
                    .cloned()
                    .unwrap_or_default();
                if i < entries.len() {
                    match tag {
                        610..=633 => {
                            if let Some(values) =
                                self.dialog("重命名内容", &[("名称", &entries[i].name)])
                            {
                                if !values[0].trim().is_empty() {
                                    entries[i].name = values[0].trim().into();
                                }
                            }
                        }
                        700..=723 => {
                            entries.remove(i);
                        }
                        800..=823 if i > 0 => entries.swap(i, i - 1),
                        900..=923 if i + 1 < entries.len() => entries.swap(i, i + 1),
                        _ => {}
                    }
                    self.settings.content.insert(path, entries);
                    self.save();
                    self.outer_rows();
                }
            }
            1000.. => {
                if let Some(a) = self.catalog.get((tag - 1000) as usize) {
                    let path = a.path.clone();
                    self.add_path(path);
                    if let Some(w) = self.picker.as_ref() {
                        w.orderOut(None);
                    }
                }
            }
            _ => {}
        }
    }
}
