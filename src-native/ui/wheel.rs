use super::*;

impl State {
    pub(super) unsafe fn draw_cat(&self, cx: f64, cy: f64, size: f64, row: usize, frame: usize) {
        const ROWS: [(usize, f64, f64, f64, f64); 11] = [
            (7, 25., 64., 141., 132.),
            (8, 40., 292., 112., 112.),
            (8, 33., 501., 125., 111.),
            (4, 41., 708., 109., 112.),
            (5, 23., 896., 145., 132.),
            (8, 22., 1132., 148., 104.),
            (6, 30., 1312., 132., 132.),
            (6, 8., 1503., 176., 149.),
            (6, 8., 1714., 176., 146.),
            (8, 51., 1936., 68., 132.),
            (8, 43., 2145., 68., 131.),
        ];
        let row = ROWS[row];
        let (w, h) = if row.3 >= row.4 {
            (size, size * row.4 / row.3)
        } else {
            (size * row.3 / row.4, size)
        };
        self.sheet
            .drawInRect_fromRect_operation_fraction_respectFlipped_hints(
                rect(r(cx - w / 2., cy - h / 2., w, h)),
                NSRect::new(
                    NSPoint::new(row.1 + (frame % row.0) as f64 * 192., 2288. - row.2 - row.4),
                    NSSize::new(row.3, row.4),
                ),
                NSCompositingOperation::SourceOver,
                1.,
                true,
                None,
            );
    }
    pub(super) unsafe fn draw_wheel(&self) {
        let (cx, cy) = self.center;
        let size = self.settings.wheel_size_px;
        let radius = size * 188. / 600.;
        let ink = color("labelColor");
        let secondary = color("secondaryLabelColor");
        let border = NSBezierPath::bezierPathWithOvalInRect(rect(r(
            cx - radius,
            cy - radius,
            radius * 2.,
            radius * 2.,
        )));
        NSColor::colorWithWhite_alpha(1., 0.5).setStroke();
        border.setLineWidth(0.65);
        border.stroke();
        if let Some(i) = self.selected {
            let angle = -std::f64::consts::FRAC_PI_2
                + i as f64 * std::f64::consts::TAU / self.settings.slots_per_page as f64;
            let half = std::f64::consts::PI / self.settings.slots_per_page as f64;
            let path = NSBezierPath::bezierPath();
            for step in 0..=28 {
                let a = angle - half + 2. * half * step as f64 / 28.;
                let p = NSPoint::new(cx + a.cos() * radius, cy + a.sin() * radius);
                if step == 0 {
                    path.moveToPoint(p)
                } else {
                    path.lineToPoint(p)
                }
            }
            let inner = size * 86. / 600.;
            for step in (0..=28).rev() {
                let a = angle - half + 2. * half * step as f64 / 28.;
                path.lineToPoint(NSPoint::new(cx + a.cos() * inner, cy + a.sin() * inner));
            }
            path.closePath();
            fill(
                &path,
                &NSColor::colorWithSRGBRed_green_blue_alpha(0.3, 0.5, 0.8, 0.14),
            );
        }
        let hub = size * 86. / 600.;
        let path =
            NSBezierPath::bezierPathWithOvalInRect(rect(r(cx - hub, cy - hub, hub * 2., hub * 2.)));
        fill(
            &path,
            &NSColor::colorWithWhite_alpha(1., if self.selected.is_none() { 0.16 } else { 0.07 }),
        );
        NSColor::colorWithWhite_alpha(1., 0.45).setStroke();
        path.setLineWidth(0.8);
        path.stroke();
        for i in 0..self.settings.slots_per_page {
            let path = self.path_at(i);
            if path.is_empty() {
                continue;
            }
            let (x, y) = layout::icon(cx, cy, i, self.settings.slots_per_page, size);
            let hot = self.hover_mix[i] > 0.01;
            if hot {
                round(
                    r(
                        x - size * 0.063,
                        y - size * 0.063,
                        size * 0.126,
                        size * 0.126,
                    ),
                    size * 0.03,
                    &NSColor::colorWithWhite_alpha(1., 0.2),
                );
            }
            let side = size * (0.088 + 0.011 * self.hover_mix[i]);
            if let Some(icon) = self.icons.get(&path) {
                image(icon, r(x - side / 2., y - side / 2., side, side));
            }
            if self.running.contains(&path) {
                fill(
                    &NSBezierPath::bezierPathWithOvalInRect(rect(r(
                        x - 2.,
                        y + side / 2. + 3.,
                        4.,
                        4.,
                    ))),
                    &NSColor::controlAccentColor(),
                );
            }
        }
        let dx = self.pointer.0 - cx;
        let dy = self.pointer.1 - cy;
        let reduced = NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion();
        let row = if self.selected.is_none() {
            if self.now() - self.opened < 1. {
                3
            } else {
                [6, 0, 5, 7, 8, 4][self.invocations % 6]
            }
        } else if dy.abs() > dx.abs() * 1.4 {
            if dy < 0. {
                9
            } else {
                10
            }
        } else if dx >= 0. {
            1
        } else {
            2
        };
        let frame = if reduced || self.selected.is_some() && self.now() - self.last_motion > 0.35 {
            0
        } else if self.selected.is_none() {
            let t = (self.now() - self.opened).max(0.);
            if t < 1. {
                (t * 4.) as usize
            } else {
                let cycle = (t - 1.) % 4.;
                if cycle < 2. {
                    (cycle * 4.) as usize
                } else {
                    0
                }
            }
        } else {
            self.last_frame
        };
        self.draw_cat(cx, cy - size * 0.022, size * 0.138, row, frame);
        let name = self
            .selected
            .map(|i| self.title(&self.path_at(i)))
            .unwrap_or_else(|| "松开取消".into());
        center_text(
            &name,
            r(cx - size * 0.115, cy + size * 0.063, size * 0.23, 24.),
            size * 0.025,
            true,
            &ink,
        );
        center_text(
            "◦   ESC",
            r(cx - 25., cy + size * 0.115, 75., 19.),
            10.,
            false,
            &secondary,
        );
        let pages = self
            .settings
            .wheel
            .len()
            .div_ceil(self.settings.slots_per_page);
        if pages > 1 {
            center_text(
                &format!("{} / {}", self.page + 1, pages),
                r(cx - 20., cy + radius + 10., 70., 20.),
                10.,
                false,
                &secondary,
            );
        }
        for (i, (entry, chip)) in self.entries.iter().zip(&self.chips).enumerate() {
            if self.selected_chip == Some(i) {
                round(
                    *chip,
                    9.,
                    &NSColor::colorWithSRGBRed_green_blue_alpha(0.2, 0.55, 0.9, 0.18),
                );
            }
            let glyph = match entry {
                Content::Pin(p) => {
                    if p.target.starts_with("http") {
                        "↗"
                    } else {
                        "▱"
                    }
                }
                Content::Tab(t) => {
                    if t.selected {
                        "●"
                    } else {
                        "○"
                    }
                }
                Content::Window(_) => "▣",
            };
            let key = match entry {
                Content::Pin(p) if p.target.starts_with('/') => p.target.clone(),
                Content::Tab(t) => t.app_path.clone(),
                Content::Window(w) => w.app_path.clone(),
                _ => self.selected.map(|i| self.path_at(i)).unwrap_or_default(),
            };
            if let Some(icon) = self.icons.get(&key) {
                image(icon, r(chip.x + 8., chip.y + 7., 20., 20.));
            } else {
                text(
                    glyph,
                    r(chip.x + 9., chip.y + 7., 18., 22.),
                    13.,
                    false,
                    &NSColor::controlAccentColor(),
                );
            }
            text(
                entry.title(),
                r(chip.x + 32., chip.y + 8., chip.w - 40., 21.),
                11.,
                false,
                &ink,
            );
        }
        if !self.preview_key.is_empty() && self.preview_rect.w > 0. {
            let p = self.preview_rect;
            if let Some(img) = &self.preview_image {
                let source = img.size();
                let scale = ((p.w - 16.) / source.width.max(1.)).min(148. / source.height.max(1.));
                let width = source.width * scale;
                let height = source.height * scale;
                image(
                    img,
                    r(
                        p.x + (p.w - width) / 2.,
                        p.y + 8. + (148. - height) / 2.,
                        width,
                        height,
                    ),
                );
            } else {
                text(
                    &self.preview_status,
                    r(p.x + 14., p.y + 65., p.w - 28., 45.),
                    12.,
                    false,
                    &secondary,
                );
            }
            text(
                &self.preview_title,
                r(p.x + 8., p.y + 161., p.w - 75., 20.),
                11.,
                false,
                &ink,
            );
            text(
                if self.preview_status == "live" {
                    "实时"
                } else if self.preview_image.is_some() {
                    "缓存"
                } else {
                    ""
                },
                r(p.x + p.w - 48., p.y + 161., 45., 20.),
                10.,
                false,
                &secondary,
            );
        }
    }
    pub(super) unsafe fn sync_glass(&mut self) {
        let radius = self.settings.wheel_size_px * 188. / 600.;
        let mut frames = vec![r(
            self.center.0 - radius,
            self.center.1 - radius,
            radius * 2.,
            radius * 2.,
        )];
        let mut columns: Vec<Rect> = vec![];
        for chip in &self.chips {
            if let Some(group) = columns
                .iter_mut()
                .find(|g| (g.x - (chip.x - 6.)).abs() < 1.)
            {
                group.h = group.h.max(chip.y + chip.h + 6. - group.y);
            } else {
                columns.push(r(chip.x - 6., chip.y - 6., chip.w + 12., chip.h + 12.));
            }
        }
        frames.extend(columns);
        if !self.preview_key.is_empty() && self.preview_rect.w > 0. {
            frames.push(self.preview_rect);
        }
        let root = self.overlay.contentView().unwrap();
        while self.glasses.len() > frames.len() {
            self.glasses.pop().unwrap().removeFromSuperview();
        }
        while self.glasses.len() < frames.len() {
            let glass = if AnyClass::get(c"NSGlassEffectView").is_some() {
                let v = native("NSGlassEffectView", r(0., 0., 16., 16.));
                let _: () = msg_send![&*v,setStyle:1isize];
                v
            } else {
                let v = native("NSVisualEffectView", r(0., 0., 16., 16.));
                let _: () = msg_send![&*v,setMaterial:6usize];
                let _: () = msg_send![&*v,setBlendingMode:0usize];
                let _: () = msg_send![&*v,setState:1usize];
                v
            };
            let _: () =
                msg_send![&*root,addSubview:&*glass,positioned:-1isize,relativeTo:&*self.canvas];
            self.glasses.push(glass);
        }
        let height = root.bounds().size.height;
        for (i, (glass, frame)) in self.glasses.iter().zip(frames).enumerate() {
            if ![frame.x, frame.y, frame.w, frame.h]
                .iter()
                .all(|v| v.is_finite())
                || frame.w < 1.
                || frame.h < 1.
            {
                continue;
            }
            if AnyClass::get(c"NSGlassEffectView").is_some() {
                let _: () = msg_send![&**glass,setCornerRadius:0.0f64];
            }
            glass.setFrame(NSRect::new(
                NSPoint::new(frame.x, height - frame.y - frame.h),
                NSSize::new(frame.w, frame.h),
            ));
            let radius = if i == 0 { radius } else { 12. };
            if AnyClass::get(c"NSGlassEffectView").is_some() {
                let _: () =
                    msg_send![&**glass,setCornerRadius:radius.min(frame.w.min(frame.h)/2.-0.25)];
            }
        }
    }
    pub(super) unsafe fn open_wheel(&mut self, x: f64, y: f64) {
        if let Some((window, _)) = self.toast.take() {
            window.orderOut(None);
        }
        let screens = NSScreen::screens(MainThreadMarker::new().unwrap());
        let screen = screens
            .iter()
            .find(|s| {
                let b = s.frame();
                x >= b.origin.x
                    && x <= b.origin.x + b.size.width
                    && y >= b.origin.y
                    && y <= b.origin.y + b.size.height
            })
            .or_else(|| screens.firstObject())
            .unwrap();
        let f = screen.frame();
        self.screen_frame = f;
        self.primary_height = screens.firstObject().unwrap().frame().size.height;
        self.overlay.setFrame_display(f, false);
        let root = self.overlay.contentView().unwrap();
        root.setFrameSize(f.size);
        self.canvas.setFrame(NSRect::new(NSPoint::ZERO, f.size));
        self.center = (x - f.origin.x, f.size.height - (y - f.origin.y));
        self.pointer = self.center;
        self.invocations += 1;
        self.last_motion = self.now();
        self.armed = false;
        self.hover_mix = [0.; 12];
        self.active = true;
        self.closing = None;
        self.opened = self.now();
        self.selected = None;
        self.entries.clear();
        self.chips.clear();
        self.selected_chip = None;
        self.stop_preview();
        self.overlay.setAlphaValue(0.);
        self.sync_glass();
        self.canvas.setNeedsDisplay(true);
        self.overlay.orderFrontRegardless();
        self.start_animation();
        self.scan();
    }
    pub(super) unsafe fn close_wheel(&mut self) {
        self.active = false;
        self.armed = false;
        self.trial = None;
        if self.overlay.isVisible() {
            self.closing = Some(self.now());
        }
        self.stop_preview();
    }
    pub(super) unsafe fn start_animation(&mut self) {
        if self.anim_timer.is_none() {
            let t = NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                1. / 60.,
                &self.delegate,
                sel!(tick:),
                None,
                true,
            );
            let loop_ref = NSRunLoop::mainRunLoop();
            loop_ref.addTimer_forMode(&t, NSRunLoopCommonModes);
            self.anim_timer = Some(t);
        }
    }
    pub(super) fn scan(&mut self) {
        if !self.scan_pending && !self.locked {
            self.scan_pending = true;
            self.last_scan = self.now();
            platform::snapshot(self.settings.wheel.clone(), self.tx.clone());
        }
    }
    pub(super) unsafe fn owner_changed(&mut self) {
        let path = self.selected.map(|i| self.path_at(i)).unwrap_or_default();
        self.entries.clear();
        self.selected_chip = None;
        self.entries.extend(
            self.settings
                .content
                .get(&path)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(Content::Pin),
        );
        self.entries.extend(
            self.tabs
                .iter()
                .filter(|t| t.app_path == path)
                .cloned()
                .map(Content::Tab),
        );
        self.entries.extend(
            self.windows
                .iter()
                .filter(|w| w.app_path == path)
                .cloned()
                .map(Content::Window),
        );
        let targets: Vec<_> = self
            .entries
            .iter()
            .filter_map(|e| {
                if let Content::Pin(p) = e {
                    p.target.starts_with('/').then(|| p.target.clone())
                } else {
                    None
                }
            })
            .collect();
        for target in targets {
            if !self.icons.contains_key(&target) {
                self.icons.insert(
                    target.clone(),
                    NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(&target)),
                );
            }
        }
        let size = self.canvas.bounds().size;
        self.chips = layout::content(
            self.center.0,
            self.center.1,
            self.settings.wheel_size_px,
            self.entries.len(),
            size.width,
            size.height,
            self.selected
                .map(|i| {
                    layout::icon(
                        self.center.0,
                        self.center.1,
                        i,
                        self.settings.slots_per_page,
                        self.settings.wheel_size_px,
                    )
                    .0 >= self.center.0
                })
                .unwrap_or(true),
        );
        self.update_preview();
        self.sync_glass();
        self.canvas.setNeedsDisplay(true);
    }
    pub(super) unsafe fn move_pointer(&mut self) {
        if !self.active {
            return;
        }
        let x = self.pointer.0;
        let y = self.pointer.1;
        let dx = x - self.center.0;
        let dy = y - self.center.1;
        let chip = if dx.hypot(dy) < self.settings.wheel_size_px * 86. / 600. {
            None
        } else {
            self.chips.iter().position(|c| c.has(x, y))
        };
        if let Some(i) = chip {
            self.armed = true;
            if self.selected_chip != Some(i) {
                self.selected_chip = Some(i);
                self.update_preview();
                self.canvas.setNeedsDisplay(true);
            }
            return;
        }
        if self.selected_chip.take().is_some() {
            self.update_preview();
            self.canvas.setNeedsDisplay(true);
        }
        if self.selected.is_some() && dx.hypot(dy) > self.settings.wheel_size_px * 0.28 {
            self.armed = dx.hypot(dy) <= self.settings.wheel_size_px * 0.6;
            return;
        }
        let next = layout::slot(
            dx,
            dy,
            self.settings.slots_per_page,
            self.settings.wheel_size_px,
        )
        .filter(|i| !self.path_at(*i).is_empty());
        self.armed = next.is_some();
        if next != self.selected {
            self.selected = next;
            self.owner_changed();
        }
        self.canvas.setNeedsDisplay(true);
    }
    pub(super) fn stop_preview(&mut self) {
        self.preview.take();
        self.preview_gen += 1;
        #[cfg(feature = "diagnostics")]
        {
            self.preview_frames = 0;
            self.preview_hashes.clear();
        }
        self.preview_key.clear();
        self.preview_status.clear();
        self.preview_title.clear();
        self.preview_pending = None;
        self.preview_image = None;
    }
    pub(super) unsafe fn update_preview(&mut self) {
        let candidate = if !self.settings.window_previews {
            None
        } else {
            self.selected_chip
                .and_then(|i| self.entries.get(i))
                .and_then(Content::preview)
                .or_else(|| {
                    self.selected.and_then(|i| {
                        let path = self.path_at(i);
                        self.windows
                            .iter()
                            .find(|w| w.app_path == path && !w.minimized)
                            .or_else(|| self.windows.iter().find(|w| w.app_path == path))
                            .cloned()
                    })
                })
        };
        let Some(window) = candidate else {
            if !self.preview_key.is_empty() {
                self.stop_preview();
                self.sync_glass();
                self.canvas.setNeedsDisplay(true);
            }
            return;
        };
        let viewport = self.canvas.bounds().size;
        let position = layout::preview(
            self.center.0,
            self.center.1,
            self.settings.wheel_size_px,
            viewport.width,
            viewport.height,
            &self.chips,
        );
        let key = format!("{}:{}", window.id, window.minimized);
        if key == self.preview_key {
            let was_visible = self.preview_rect.w > 0.;
            self.preview_rect = position.unwrap_or_default();
            self.preview_title = window.title.clone();
            if position.is_none() && (self.preview.is_some() || self.preview_pending.is_some()) {
                self.preview.take();
                self.preview_pending = None;
                self.preview_gen += 1;
                self.preview_status = "内容较多，优先显示外环".into();
            } else if !was_visible && position.is_some() {
                self.preview_gen += 1;
                self.preview_start = self.now();
                self.preview_pending = Some(window);
                self.preview_status = if self.preview_image.is_some() {
                    "cached"
                } else {
                    "正在读取画面…"
                }
                .into();
            }
            self.sync_glass();
            self.canvas.setNeedsDisplay(true);
            return;
        }
        self.stop_preview();
        self.preview_key = key;
        self.preview_title = window.title.clone();
        self.preview_start = self.now();
        self.preview_image = self
            .preview_cache
            .iter()
            .find(|(id, _)| id == &window.id)
            .map(|(_, image)| image.clone());
        self.preview_status = if window.minimized {
            "已最小化，暂无缓存画面"
        } else {
            "正在读取画面…"
        }
        .into();
        self.preview_rect = position.unwrap_or_default();
        if position.is_some() {
            self.preview_pending = Some(window);
        } else {
            self.preview_status = "内容较多，优先显示外环".into();
        }
        self.sync_glass();
        self.canvas.setNeedsDisplay(true);
    }
    pub(super) unsafe fn execute_selection(&mut self) {
        if self.locked || !self.armed {
            return;
        }
        if let Some(i) = self.selected_chip {
            if let Some(item) = self.entries.get(i).cloned() {
                match item {
                    Content::Window(w) => platform::activate(
                        "activate",
                        serde_json::to_string(&w).unwrap(),
                        self.tx.clone(),
                    ),
                    Content::Tab(t) => platform::activate(
                        "activate-tab",
                        serde_json::to_string(&t).unwrap(),
                        self.tx.clone(),
                    ),
                    Content::Pin(p) => self
                        .open_target(&p.target, self.selected.map(|i| self.path_at(i)).as_deref()),
                }
                return;
            }
        }
        if let Some(i) = self.selected {
            let path = self.path_at(i);
            if let Some(window) = self
                .windows
                .iter()
                .find(|w| w.app_path == path && !w.minimized)
                .or_else(|| self.windows.iter().find(|w| w.app_path == path))
            {
                platform::activate(
                    "activate",
                    serde_json::to_string(window).unwrap(),
                    self.tx.clone(),
                );
            } else {
                self.open_target(&path, None);
            }
        }
    }
    pub(super) fn open_target(&self, target: &str, owner: Option<&str>) {
        let target = target.to_string();
        let owner = owner
            .filter(|o| self.catalog.iter().any(|a| a.path == *o && a.browser))
            .map(str::to_string);
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let mut command = std::process::Command::new("/usr/bin/open");
            if target.ends_with(".app") {
                command.arg("-a");
            } else if target.starts_with("http") {
                if let Some(owner) = owner {
                    command.args(["-a", &owner]);
                }
            }
            if !command.arg(&target).status().is_ok_and(|s| s.success()) {
                let _ = tx.send(Message::Failure("目标暂时无法打开，请检查路径".into()));
            }
        });
    }
    pub(super) unsafe fn tick(&mut self) {
        #[cfg(feature = "diagnostics")]
        self.diagnostics();
        if !self.gesture.held && !self.pending_key.is_null() {
            platform::CFRelease(self.pending_key);
            self.pending_key = ptr::null_mut();
        }
        if self.capture_dirty {
            self.capture_dirty = false;
            self.save();
            self.settings_page();
        }
        let now = self.now();
        if self
            .toast
            .as_ref()
            .is_some_and(|(_, deadline)| now >= *deadline)
        {
            if let Some((window, _)) = self.toast.take() {
                window.orderOut(None);
            }
        }
        let locked = session_locked();
        if locked != self.locked {
            self.locked = locked;
            self.capturing = false;
            self.gesture.reset();
            self.close_wheel();
            if !locked && self.trusted && !self.tap.is_null() {
                platform::CGEventTapEnable(self.tap, true);
            }
        }
        let trusted = platform::AXIsProcessTrusted();
        if trusted != self.trusted {
            self.trusted = trusted;
            self.tap_attempted = false;
            if !self.tap.is_null() {
                platform::CGEventTapEnable(self.tap, trusted);
            }
            if !trusted {
                self.gesture.reset();
                self.close_wheel();
            }
            self.settings_page();
        }
        if trusted && self.tap.is_null() && !self.tap_attempted {
            self.install_hook();
        }
        while let Ok(message) = self.rx.try_recv() {
            match message {
                Message::Catalog(apps) => {
                    self.catalog = apps;
                    if self.search.is_some() {
                        self.filter_apps();
                    }
                    self.settings_page();
                }
                Message::Snapshot {
                    windows,
                    tabs,
                    running,
                    paths,
                } => {
                    self.scan_pending = false;
                    if paths != self.settings.wheel {
                        self.scan();
                        continue;
                    }
                    self.windows = windows;
                    self.tabs = tabs;
                    self.running = running;
                    if self.active && self.selected_chip.is_none() {
                        self.owner_changed();
                    }
                    if self.search.is_none() && self.picker.as_ref().is_some_and(|w| w.isVisible())
                    {
                        self.outer_rows();
                    }
                }
                Message::Preview {
                    generation,
                    image: encoded,
                } if generation == self.preview_gen && self.active => {
                    #[cfg(feature = "diagnostics")]
                    {
                        use std::hash::{Hash, Hasher};
                        let mut h = std::collections::hash_map::DefaultHasher::new();
                        encoded.hash(&mut h);
                        self.preview_frames += 1;
                        self.preview_hashes.insert(h.finish());
                    }
                    if let Some(data) = decode_image(&encoded) {
                        self.preview_image = Some(data.clone());
                        self.preview_status = "live".into();
                        let key = self
                            .preview_key
                            .split(':')
                            .take(2)
                            .collect::<Vec<_>>()
                            .join(":");
                        self.preview_cache.retain(|(id, _)| id != &key);
                        self.preview_cache.push((key, data));
                        while self.preview_cache.len() > 32 {
                            self.preview_cache.remove(0);
                        }
                        self.canvas.setNeedsDisplay(true);
                    }
                }
                Message::PreviewProblem { generation, reason }
                    if generation == self.preview_gen =>
                {
                    self.preview_status = match reason.as_str() {
                        "permission" => "需要在设置中开启屏幕录制权限",
                        "closed" => "窗口已关闭",
                        "locked" => "锁屏期间停止预览",
                        _ => "这个窗口暂时无法预览",
                    }
                    .into();
                    self.canvas.setNeedsDisplay(true);
                }
                Message::PreviewEnded(generation) if generation == self.preview_gen => {
                    self.preview.take();
                    self.preview_pending = None;
                    self.preview_status = if self.preview_status.starts_with("需要")
                        || self.preview_status == "窗口已关闭"
                    {
                        self.preview_status.as_str()
                    } else if self.preview_image.is_some() {
                        "cached"
                    } else {
                        "这个窗口暂时无法预览"
                    }
                    .into();
                    self.canvas.setNeedsDisplay(true);
                }
                Message::Failure(message) => {
                    self.notice = message;
                    self.settings_page();
                    if !self.active && !self.settings_window.as_ref().is_some_and(|w| w.isVisible())
                    {
                        self.show_failure();
                    }
                }
                _ => {}
            }
        }
        if self.moved {
            self.moved = false;
            self.move_pointer();
        }
        if self.layout_dirty {
            self.layout_dirty = false;
            self.sync_glass();
            self.canvas.setNeedsDisplay(true);
        }
        let effects = std::mem::take(&mut self.effects);
        for effect in effects {
            match effect {
                Effect::Select => {
                    self.execute_selection();
                    self.close_wheel();
                }
                Effect::Cancel => self.close_wheel(),
                _ => {}
            }
        }
        if self.gesture.held && !self.gesture.cancelled && self.anim_timer.is_none() {
            self.start_animation();
        }
        if self.gesture.tick(now) == Effect::Open && !self.locked {
            let p = NSEvent::mouseLocation();
            self.open_wheel(p.x, p.y);
        }
        if self.moved {
            self.moved = false;
            self.move_pointer();
        }
        if self.active {
            for i in 0..self.settings.slots_per_page {
                let target = if self.selected == Some(i) && self.armed {
                    1.
                } else {
                    0.
                };
                let delta = target - self.hover_mix[i];
                if delta.abs() > 0.002 {
                    self.hover_mix[i] += delta * 0.35;
                    self.canvas.setNeedsDisplay(true);
                }
            }
            self.overlay
                .setAlphaValue(((now - self.opened) / 0.12).clamp(0., 1.));
            if now - self.last_scan > 4. {
                self.scan();
            }
            let frame = (now * 6.) as usize;
            if self.last_frame != frame {
                self.last_frame = frame;
                self.canvas.setNeedsDisplay(true);
            }
            if self.preview_pending.is_some()
                && self.preview.is_none()
                && now - self.preview_start > 0.13
            {
                let window = self.preview_pending.take().unwrap();
                self.preview = platform::preview(&window, self.preview_gen, self.tx.clone());
            }
            if self.preview.is_some()
                && self.preview_image.is_none()
                && now - self.preview_start > 1.8
            {
                self.preview_status = "画面暂未就绪，请稍后重试".into();
                self.canvas.setNeedsDisplay(true);
            }
            if self.trial.is_some_and(|deadline| now >= deadline) {
                self.close_wheel();
            }
        }
        if let Some(start) = self.closing {
            let alpha = 1. - (now - start) / 0.10;
            self.overlay.setAlphaValue(alpha.max(0.));
            if alpha <= 0. {
                self.overlay.orderOut(None);
                self.closing = None;
            }
        }
        if !self.active && (!self.gesture.held || self.gesture.cancelled) && self.closing.is_none()
        {
            if let Some(timer) = self.anim_timer.take() {
                timer.invalidate();
            }
        }
        if self.search.is_some() && self.picker.as_ref().is_some_and(|w| w.isVisible()) {
            self.picker_icons();
        }
    }
    pub(super) unsafe fn install_hook(&mut self) {
        let types = [1u32, 3, 5, 6, 7, 10, 11, 12, 22, 25, 26, 27];
        let mask = types.iter().fold(0u64, |m, t| m | (1u64 << t));
        self.tap_attempted = true;
        self.tap = platform::CGEventTapCreate(0, 0, 0, mask, event_hook, ptr::null_mut());
        if self.tap.is_null() {
            self.notice = "系统尚未允许输入监听，请检查辅助功能后点击重新连接".into();
        }
        if !self.tap.is_null() {
            self.source = platform::CFMachPortCreateRunLoopSource(ptr::null_mut(), self.tap, 0);
            platform::CFRunLoopAddSource(
                platform::CFRunLoopGetMain(),
                self.source,
                platform::kCFRunLoopCommonModes,
            );
            platform::CGEventTapEnable(self.tap, true);
        }
    }
    pub(super) fn stop(&mut self) {
        self.preview.take();
        self.gesture.reset();
        unsafe {
            if !self.tap.is_null() {
                platform::CGEventTapEnable(self.tap, false);
            }
        }
    }
}

impl State {
    /// A short, non-activating notice makes a rejected stale target visible without opening Settings.
    unsafe fn show_failure(&mut self) {
        if self.locked {
            return;
        }
        if let Some((window, _)) = self.toast.take() {
            window.orderOut(None);
        }
        let screens = NSScreen::screens(MainThreadMarker::new().unwrap());
        let point = NSEvent::mouseLocation();
        let screen = screens
            .iter()
            .find(|s| {
                let f = s.frame();
                point.x >= f.origin.x
                    && point.x <= f.origin.x + f.size.width
                    && point.y >= f.origin.y
                    && point.y <= f.origin.y + f.size.height
            })
            .or_else(|| screens.firstObject())
            .unwrap();
        let f = screen.visibleFrame();
        let allocation: Allocated<NSWindow> = msg_send![class("NSPanel"), alloc];
        let window = NSWindow::initWithContentRect_styleMask_backing_defer(
            allocation,
            NSRect::new(
                NSPoint::new(
                    f.origin.x + (f.size.width - 360.) / 2.,
                    f.origin.y + f.size.height - 100.,
                ),
                NSSize::new(360., 56.),
            ),
            NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel,
            NSBackingStoreType::Buffered,
            false,
        );
        window.setReleasedWhenClosed(false);
        window.setOpaque(false);
        window.setBackgroundColor(Some(&NSColor::clearColor()));
        window.setHasShadow(true);
        window.setIgnoresMouseEvents(true);
        window.setLevel(1000);
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::IgnoresCycle
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::CanJoinAllApplications,
        );
        let root = if AnyClass::get(c"NSGlassEffectView").is_some() {
            let root = native("NSGlassEffectView", r(0., 0., 360., 56.));
            let _: () = msg_send![&*root,setStyle:1isize];
            let _: () = msg_send![&*root,setCornerRadius:12.0f64];
            root
        } else {
            let root = native("NSVisualEffectView", r(0., 0., 360., 56.));
            let _: () = msg_send![&*root,setMaterial:6usize];
            let _: () = msg_send![&*root,setBlendingMode:0usize];
            root
        };
        label(
            &root,
            &self.notice,
            r(16., 18., 328., 22.),
            13.,
            false,
            false,
        );
        window.setContentView(Some(&root));
        window.orderFrontRegardless();
        self.toast = Some((window, self.now() + 2.));
    }
}
