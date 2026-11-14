use super::*;

impl State {
    pub(super) fn now(&self) -> f64 {
        self.birth.elapsed().as_secs_f64()
    }
    pub(super) unsafe fn button(
        &self,
        parent: &NSView,
        title: &str,
        tag: isize,
        frame: Rect,
    ) -> Retained<NSView> {
        let b: Retained<NSView> = msg_send![NSButton::class(),buttonWithTitle:&*NSString::from_str(title),target:&*self.delegate,action:sel!(action:)];
        frame_view(&b, frame);
        if frame.y > 100. {
            b.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        }
        let _: () = msg_send![&*b,setTag:tag];
        let _: () = msg_send![&*b,setBezelStyle:1usize];
        parent.addSubview(&b);
        b
    }
    pub(super) unsafe fn popup(
        &self,
        parent: &NSView,
        items: &[&str],
        index: usize,
        tag: isize,
        frame: Rect,
    ) -> Retained<NSView> {
        let b = native("NSPopUpButton", frame);
        for title in items {
            let _: () = msg_send![&*b,addItemWithTitle:&*NSString::from_str(title)];
        }
        let _: () = msg_send![&*b,selectItemAtIndex:index as isize];
        self.control(&b, tag);
        parent.addSubview(&b);
        b
    }
    pub(super) unsafe fn control(&self, v: &NSView, tag: isize) {
        let _: () = msg_send![v,setTag:tag];
        let _: () = msg_send![v,setTarget:&*self.delegate];
        let _: () = msg_send![v,setAction:sel!(action:)];
    }
    pub(super) unsafe fn toggle(&self, parent: &NSView, on: bool, tag: isize, frame: Rect) {
        let b = native("NSSwitch", frame);
        let _: () = msg_send![&*b,setState:if on{1isize}else{0isize}];
        self.control(&b, tag);
        parent.addSubview(&b);
    }
    pub(super) unsafe fn load_icons(&mut self) {
        let workspace = NSWorkspace::sharedWorkspace();
        for path in self.settings.wheel.iter().filter(|p| !p.is_empty()) {
            if !self.icons.contains_key(path) {
                self.icons.insert(
                    path.clone(),
                    workspace.iconForFile(&NSString::from_str(path)),
                );
            }
        }
    }
    pub(super) unsafe fn apply_theme(&self) {
        let name = match self.settings.appearance.as_str() {
            "dark" => Some(ns_string!("NSAppearanceNameDarkAqua")),
            "light" => Some(ns_string!("NSAppearanceNameAqua")),
            _ => None,
        };
        let appearance = name.and_then(NSAppearance::appearanceNamed);
        self.app.setAppearance(appearance.as_deref());
    }
    pub(super) fn title(&self, path: &str) -> String {
        self.settings
            .names
            .get(path)
            .cloned()
            .or_else(|| {
                self.catalog
                    .iter()
                    .find(|a| a.path == path)
                    .map(|a| a.name.clone())
            })
            .unwrap_or_else(|| self.settings.name(path))
    }
    pub(super) fn path_at(&self, index: usize) -> String {
        self.settings
            .wheel
            .get(self.page * self.settings.slots_per_page + index)
            .cloned()
            .unwrap_or_default()
    }
    pub(super) unsafe fn save(&mut self) {
        self.gesture.reset();
        self.close_wheel();
        self.settings.normalize();
        if let Err(e) = self.settings.save() {
            self.notice = format!("设置保存失败：{e}");
        }
        self.load_icons();
        self.apply_theme();
        self.page = self.page.min(
            self.settings
                .wheel
                .len()
                .div_ceil(self.settings.slots_per_page)
                .saturating_sub(1),
        );
    }
    pub(super) unsafe fn show_settings(&mut self) {
        self.close_wheel();
        self.capturing = false;
        self.app
            .setActivationPolicy(NSApplicationActivationPolicy::Regular);
        if self.settings_window.is_none() {
            let w = NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(MainThreadMarker::new().unwrap()),
                NSRect::new(NSPoint::new(0., 0.), NSSize::new(900., 640.)),
                NSWindowStyleMask::Titled
                    | NSWindowStyleMask::Closable
                    | NSWindowStyleMask::Miniaturizable
                    | NSWindowStyleMask::Resizable
                    | NSWindowStyleMask::FullSizeContentView,
                NSBackingStoreType::Buffered,
                false,
            );
            w.setReleasedWhenClosed(false);
            w.setTitle(ns_string!("ViGo"));
            w.setTitlebarAppearsTransparent(true);
            w.setTitleVisibility(NSWindowTitleVisibility::Hidden);
            w.setContentMinSize(NSSize::new(900., 640.));
            w.center();
            w.setDelegate(Some(ProtocolObject::from_ref(&*self.delegate)));
            let root = native("NSView", r(0., 0., 900., 640.));
            let _: () = msg_send![&*root,setAutoresizingMask:18usize];
            w.setContentView(Some(&root));
            self.settings_root = Some(root);
            self.settings_window = Some(w);
        }
        self.settings_page();
        let w = self.settings_window.as_ref().unwrap();
        w.makeKeyAndOrderFront(None);
        #[allow(deprecated)]
        self.app.activateIgnoringOtherApps(true);
    }
    pub(super) unsafe fn hide_settings(&mut self) {
        self.settings_window.as_ref().unwrap().orderOut(None);
        self.app
            .setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    }
    pub(super) unsafe fn settings_page(&mut self) {
        let Some(root) = self.settings_root.clone() else {
            return;
        };
        clear(&root);
        let bounds = root.bounds();
        let height = bounds.size.height;
        let sidebar = native("NSVisualEffectView", r(0., 0., 184., height));
        let _: () = msg_send![&*sidebar,setMaterial:7usize];
        let _: () = msg_send![&*sidebar,setBlendingMode:0usize];
        let _: () = msg_send![&*sidebar,setAutoresizingMask:16usize];
        root.addSubview(&sidebar);
        let icon = NSImageView::new(MainThreadMarker::new().unwrap());
        icon.setImage(self.app.applicationIconImage().as_deref());
        icon.setFrame(NSRect::new(
            NSPoint::new(22., height - 98.),
            NSSize::new(36., 36.),
        ));
        icon.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        sidebar.addSubview(&icon);
        label(
            &sidebar,
            "ViGo",
            r(68., height - 82., 96., 25.),
            21.,
            true,
            false,
        );
        label(
            &sidebar,
            "V for your desktop.",
            r(22., height - 116., 152., 20.),
            11.,
            false,
            true,
        );
        for (i, title) in ["我的轮盘", "呼出与手势", "通用设置"].iter().enumerate() {
            let b = self.button(
                &sidebar,
                title,
                10 + i as isize,
                r(12., height - 176. - i as f64 * 46., 160., 36.),
            );
            let _: () = msg_send![&*b,setBordered:self.tab==i];
            let _: () = msg_send![&*b,setAlignment:0isize];
            let symbol = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                &NSString::from_str(["chart.pie", "cursorarrow", "gearshape"][i]),
                None,
            );
            let _: () = msg_send![&*b,setImage:symbol.as_deref()];
            let _: () = msg_send![&*b,setImagePosition:2usize];
        }
        label(
            &sidebar,
            if self.capturing {
                "●  正在录制按键"
            } else if !self.trusted {
                "○  需要辅助功能"
            } else if self.tap.is_null() {
                "○  连接失败，请重连"
            } else if self.settings.enabled_on_start {
                "●  输入助手运行中"
            } else {
                "○  已暂停"
            },
            r(20., 45., 160., 18.),
            11.,
            false,
            true,
        );
        label(
            &sidebar,
            "按住 V · 移动 · 松开",
            r(20., 20., 160., 18.),
            10.,
            false,
            true,
        );
        let content = native("NSView", r(184., 0., bounds.size.width - 184., height));
        let _: () = msg_send![&*content,setAutoresizingMask:18usize];
        root.addSubview(&content);
        self.content_root = Some(content.clone());
        let titles = ["我的轮盘", "呼出与手势", "通用设置"];
        label(
            &content,
            &format!("设置   /   {}", titles[self.tab]),
            r(30., height - 49., 470., 20.),
            11.,
            false,
            true,
        );
        label(
            &content,
            titles[self.tab],
            r(30., height - 108., 500., 38.),
            28.,
            true,
            false,
        );
        self.button(&content, "▷  试一下", 20, r(555., height - 105., 110., 32.));
        if self.trusted && self.tap.is_null() {
            self.button(
                &content,
                "重新连接输入助手",
                22,
                r(495., height - 147., 170., 30.),
            );
        }
        if !self.trusted {
            label(
                &content,
                "允许辅助功能后，即可按住 V 呼出。",
                r(30., height - 144., 410., 22.),
                13.,
                false,
                true,
            );
            self.button(
                &content,
                "开启辅助功能…",
                21,
                r(500., height - 148., 165., 30.),
            );
        }
        match self.tab {
            0 => self.editor_page(&content, height),
            1 => self.gestures_page(&content, height),
            _ => self.general_page(&content, height),
        }
        if !self.notice.is_empty() {
            label(
                &content,
                &self.notice,
                r(30., 12., 640., 22.),
                11.,
                false,
                true,
            );
        }
    }
    pub(super) unsafe fn card(
        &self,
        parent: &NSView,
        title: &str,
        description: &str,
        y: f64,
    ) -> Retained<NSView> {
        let card = native("NSBox", r(30., y, 635., 66.));
        let _: () = msg_send![&*card,setBoxType:4usize];
        let _: () = msg_send![&*card,setCornerRadius:10.0f64];
        let _: () = msg_send![&*card,setBorderWidth:0.5f64];
        let _: () = msg_send![&*card,setBorderColor:&*color("separatorColor")];
        let _: () = msg_send![&*card,setFillColor:&*color("controlBackgroundColor")];
        card.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        parent.addSubview(&card);
        label(&card, title, r(16., 34., 325., 21.), 14., false, false);
        label(&card, description, r(16., 13., 360., 18.), 11., false, true);
        card
    }
    pub(super) unsafe fn gestures_page(&self, parent: &NSView, height: f64) {
        label(
            parent,
            "一次按住，一次移动。让动作顺着你的习惯。",
            r(30., height - 143., 610., 22.),
            13.,
            false,
            true,
        );
        let c = self.card(
            parent,
            "轮盘呼出键",
            "轻按仍正常输入，组合键保持原来的行为。",
            height - 244.,
        );
        let mode = match &self.settings.trigger {
            Trigger::Mouse(name) => {
                if name == "mouse4" {
                    1
                } else {
                    2
                }
            }
            _ => 0,
        };
        self.popup(
            &c,
            &["键盘按键", "鼠标侧键 4", "鼠标侧键 5"],
            mode,
            33,
            r(400., 17., 126., 32.),
        );
        let key = match &self.settings.trigger {
            Trigger::Key { vk } => crate::input::key_label(*vk),
            _ => "改按键".into(),
        };
        self.button(
            &c,
            if self.capturing { "按键…" } else { &key },
            30,
            r(534., 17., 76., 32.),
        );
        let c = self.card(
            parent,
            "取消选择",
            "移回中心再松手，或按 Esc、鼠标右键。",
            height - 322.,
        );
        label(&c, "esc", r(565., 24., 45., 23.), 13., true, true);
        let c = self.card(
            parent,
            "每组软件数量",
            "4～12 个方向；滚轮切换软件组。",
            height - 400.,
        );
        let options: Vec<String> = (4..=12).map(|v| format!("{v} 个")).collect();
        self.popup(
            &c,
            &options.iter().map(String::as_str).collect::<Vec<_>>(),
            self.settings.slots_per_page - 4,
            31,
            r(490., 18., 120., 30.),
        );
        let c = self.card(
            parent,
            "轮盘大小",
            "图标与命中区域一起缩放。",
            height - 478.,
        );
        let slider = native("NSSlider", r(405., 21., 145., 25.));
        let _: () = msg_send![&*slider,setMinValue:360.0f64];
        let _: () = msg_send![&*slider,setMaxValue:720.0f64];
        let _: () = msg_send![&*slider,setDoubleValue:self.settings.wheel_size_px];
        let _: () = msg_send![&*slider,setContinuous:false];
        self.control(&slider, 32);
        c.addSubview(&slider);
        label(
            &c,
            &format!("{} pt", self.settings.wheel_size_px as i32),
            r(557., 22., 65., 23.),
            12.,
            false,
            true,
        );
    }
    pub(super) unsafe fn general_page(&self, parent: &NSView, height: f64) {
        label(
            parent,
            "轻巧待命，融入你的 Mac。",
            r(30., height - 143., 610., 22.),
            13.,
            false,
            true,
        );
        let c = self.card(
            parent,
            "外观",
            "为当前桌面选择合适的明暗模式。",
            height - 234.,
        );
        self.popup(
            &c,
            &["跟随系统", "浅色", "深色"],
            match self.settings.appearance.as_str() {
                "light" => 1,
                "dark" => 2,
                _ => 0,
            },
            40,
            r(455., 18., 155., 30.),
        );
        let c = self.card(
            parent,
            "启用 ViGo",
            "暂停时，所有输入都交还当前软件。",
            height - 310.,
        );
        self.toggle(
            &c,
            self.settings.enabled_on_start,
            41,
            r(560., 17., 50., 30.),
        );
        let c = self.card(
            parent,
            "登录时启动",
            "登录后在菜单栏静静待命。",
            height - 386.,
        );
        self.toggle(
            &c,
            self.settings.launch_at_login,
            42,
            r(560., 17., 50., 30.),
        );
        let c = self.card(
            parent,
            "窗口预览",
            "悬停软件即可显示，仅捕获所在窗口的当前画面。",
            height - 462.,
        );
        self.toggle(
            &c,
            self.settings.window_previews,
            43,
            r(560., 17., 50., 30.),
        );
        if self.settings.window_previews && !platform::CGPreflightScreenCaptureAccess() {
            self.button(&c, "开启权限…", 44, r(430., 17., 115., 30.));
        }
        label(
            parent,
            "ViGo  0.3.0   ·   Rust + AppKit",
            r(34., height - 512., 610., 22.),
            13.,
            true,
            false,
        );
        label(
            parent,
            "按住 V，把常用动作放到手边。",
            r(34., height - 536., 610., 22.),
            12.,
            false,
            true,
        );
    }
    pub(super) unsafe fn editor_page(&mut self, parent: &NSView, height: f64) {
        label(
            parent,
            "软件放内环，常用内容跟随展开。",
            r(30., height - 143., 610., 22.),
            13.,
            false,
            true,
        );
        let canvas = Canvas::new(1, r(26., height - 480., 330., 320.));
        parent.addSubview(&canvas);
        self.button(parent, "添加软件…", 50, r(365., height - 203., 140., 30.));
        self.button(
            parent,
            "文件或文件夹…",
            51,
            r(515., height - 203., 150., 30.),
        );
        let path = self
            .settings
            .wheel
            .get(self.edit_slot)
            .cloned()
            .unwrap_or_default();
        let title = self.title(&path);
        label(
            parent,
            if path.is_empty() {
                "选择一个位置，添加常用软件。"
            } else {
                &title
            },
            r(370., height - 250., 290., 24.),
            15.,
            true,
            false,
        );
        self.button(parent, "重命名", 52, r(365., height - 294., 94., 30.));
        self.button(parent, "移除", 53, r(465., height - 294., 90., 30.));
        self.button(parent, "←", 54, r(562., height - 294., 45., 30.));
        self.button(parent, "→", 55, r(614., height - 294., 45., 30.));
        let count = self
            .settings
            .wheel
            .len()
            .div_ceil(self.settings.slots_per_page);
        self.button(parent, "‹ 上一组", 56, r(366., height - 344., 115., 30.));
        self.button(parent, "下一组 ›", 57, r(500., height - 344., 115., 30.));
        label(
            parent,
            &format!("第 {} / {} 组 · 最多 24 个动作", self.page + 1, count),
            r(370., height - 378., 285., 22.),
            11.,
            false,
            true,
        );
        self.button(
            parent,
            "编辑这个软件的外环…",
            58,
            r(365., height - 434., 290., 32.),
        );
        label(
            parent,
            "点击位置选择；左右交换可跨组，已有软件会一起换位。",
            r(30., 45., 630., 22.),
            11.,
            false,
            true,
        );
    }
    pub(super) unsafe fn draw_editor(&self, view: &Canvas) {
        let n = self.settings.slots_per_page;
        let ink = color("labelColor");
        let size = 440.;
        round(
            r(20., 10., 290., 290.),
            145.,
            &NSColor::colorWithWhite_alpha(0.5, 0.06),
        );
        for i in 0..n {
            let (x, y) = layout::icon(165., 155., i, n, size);
            let index = self.page * n + i;
            if index == self.drag_to.unwrap_or(self.edit_slot) {
                round(
                    r(x - 31., y - 31., 62., 62.),
                    14.,
                    &NSColor::colorWithSRGBRed_green_blue_alpha(0.2, 0.57, 0.95, 0.16),
                );
            }
            let path = self
                .settings
                .wheel
                .get(index)
                .map(String::as_str)
                .unwrap_or("");
            if let Some(icon) = self.icons.get(path) {
                image(icon, r(x - 23., y - 23., 46., 46.));
            } else {
                text(
                    "＋",
                    r(x - 12., y - 17., 30., 30.),
                    24.,
                    false,
                    &color("tertiaryLabelColor"),
                );
            }
        }
        self.draw_cat(165., 150., 60., 6, 0);
        text(
            &format!("{} 个方向", n),
            r(125., 196., 100., 22.),
            12.,
            false,
            &ink,
        );
        let _ = view;
    }
    pub(super) fn editor_click(&mut self, x: f64, y: f64) -> Option<usize> {
        let index = layout::editor_slot(x, y, self.settings.slots_per_page)
            .map(|i| self.page * self.settings.slots_per_page + i);
        if let Some(i) = index {
            self.edit_slot = i;
        }
        index
    }
}
