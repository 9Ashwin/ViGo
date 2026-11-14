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
        let size = self.render_size;
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
        for i in 0..self.active_paths.len() {
            if self.hover_mix[i] < 0.005 {
                continue;
            }
            let node = &self.ring.nodes[i];
            let angle = node.angle;
            let half = std::f64::consts::PI / node.count as f64;
            let radius = node.radius + self.ring.icon / 2. + 12.;
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
            let inner = (node.radius - self.ring.icon / 2. - 12.).max(self.ring.hub);
            for step in (0..=28).rev() {
                let a = angle - half + 2. * half * step as f64 / 28.;
                path.lineToPoint(NSPoint::new(cx + a.cos() * inner, cy + a.sin() * inner));
            }
            path.closePath();
            fill(
                &path,
                &NSColor::controlAccentColor().colorWithAlphaComponent(0.14 * self.hover_mix[i]),
            );
        }
        let hub = self.ring.hub;
        let path =
            NSBezierPath::bezierPathWithOvalInRect(rect(r(cx - hub, cy - hub, hub * 2., hub * 2.)));
        fill(
            &path,
            &NSColor::colorWithWhite_alpha(1., if self.selected.is_none() { 0.16 } else { 0.07 }),
        );
        NSColor::colorWithWhite_alpha(1., 0.45).setStroke();
        path.setLineWidth(0.8);
        path.stroke();
        for i in 0..self.active_paths.len() {
            let path = self.path_at(i);
            if path.is_empty() {
                continue;
            }
            let n = &self.ring.nodes[i];
            let t = ((self.now() - self.opened) / 0.18).clamp(0., 1.);
            let intro = if self.reduce_motion {
                1.
            } else {
                0.95 + 0.05 * (1. - (1. - t).powi(3))
            };
            let (x, y) = (
                cx + n.angle.cos() * n.radius * intro,
                cy + n.angle.sin() * n.radius * intro - self.hover_mix[i] * 1.5,
            );
            let hot = self.hover_mix[i] > 0.01;
            if hot {
                round(
                    r(
                        x - self.ring.icon * 0.7,
                        y - self.ring.icon * 0.7,
                        self.ring.icon * 1.4,
                        self.ring.icon * 1.4,
                    ),
                    self.ring.icon * 0.3,
                    &NSColor::colorWithWhite_alpha(1., 0.18 * self.hover_mix[i]),
                );
            }
            let side = self.ring.icon * (1. + 0.12 * self.hover_mix[i]);
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
        let gaze = if reduced {
            (0., 0.)
        } else {
            (
                (dx / 75.).clamp(-1., 1.) * 2.8,
                (dy / 75.).clamp(-1., 1.) * 1.8,
            )
        };
        self.draw_cat(
            cx + gaze.0,
            cy - self.settings.wheel_size_px * 0.022 + gaze.1,
            self.settings.wheel_size_px * 0.138,
            row,
            frame,
        );
        let name = self
            .selected
            .map(|i| self.title(&self.path_at(i)))
            .unwrap_or_else(|| "松开取消".into());
        center_text(
            &name,
            r(
                cx - self.settings.wheel_size_px * 0.115,
                cy + self.settings.wheel_size_px * 0.063,
                self.settings.wheel_size_px * 0.23,
                24.,
            ),
            self.settings.wheel_size_px * 0.025,
            true,
            &ink,
        );
        center_text(
            "◦   ESC",
            r(cx - 25., cy + self.settings.wheel_size_px * 0.115, 75., 19.),
            10.,
            false,
            &secondary,
        );
        for (i, (entry, chip)) in self.entries.iter().zip(&self.chips).enumerate() {
            if self.selected_chip == Some(i) {
                round(
                    *chip,
                    9.,
                    &NSColor::colorWithSRGBRed_green_blue_alpha(0.2, 0.55, 0.9, 0.18),
                );
            }
            let glyph = match entry {
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
                Content::Tab(t) => t.app_path.clone(),
                Content::Window(w) => w.app_path.clone(),
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
    }
    pub(super) unsafe fn sync_glass(&mut self) {
        let radius = self.ring.outer;
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
                let _: () = msg_send![&**glass,setStyle:if i==0{1isize}else{0isize}];
                let _: () =
                    msg_send![&**glass,setCornerRadius:radius.min(frame.w.min(frame.h)/2.-0.25)];
            }
        }
    }
    pub(super) unsafe fn open_wheel(&mut self, x: f64, y: f64) {
        if let Some((window, _)) = self.toast.take() {
            window.orderOut(None);
        }
        self.active_paths = self.settings.wheel.clone();
        self.active_names = self.settings.names.clone();
        self.reduce_motion =
            NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion();
        self.status_menu.cancelTracking();
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
        self.anchor = (x - f.origin.x, f.size.height - (y - f.origin.y));
        self.ring = layout::DockLayout::new(
            self.active_paths.len(),
            self.settings.wheel_size_px,
            f.size.width,
            f.size.height,
        );
        self.render_size = self.ring.outer * 600. / 188.;
        self.center = self.ring.center(self.anchor, f.size.width, f.size.height);
        self.pointer = self.anchor;
        self.invocations += 1;
        self.last_motion = self.now();
        self.armed = false;
        self.hover_mix = vec![0.; self.active_paths.len()];
        self.active = true;
        self.closing = None;
        self.opened = self.now();
        self.selected = None;
        self.entries.clear();
        self.chips.clear();
        self.selected_chip = None;
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
        self.entries = self
            .tabs
            .iter()
            .filter(|t| t.app_path == path)
            .cloned()
            .map(Content::Tab)
            .chain(
                self.windows
                    .iter()
                    .filter(|w| w.app_path == path)
                    .cloned()
                    .map(Content::Window),
            )
            .collect();
        self.selected_chip = None;
        let size = self.canvas.bounds().size;
        self.chips = layout::content(
            self.center.0,
            self.center.1,
            self.render_size,
            self.entries.len(),
            size.width,
            size.height,
            self.selected
                .map(|i| self.ring.nodes[i].angle.cos() >= 0.)
                .unwrap_or(true),
        );
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
        if (x - self.anchor.0).hypot(y - self.anchor.1) < 12. {
            self.armed = false;
            if self.selected.take().is_some() {
                self.owner_changed();
            }
            self.canvas.setNeedsDisplay(true);
            return;
        }
        let chip = if dx.hypot(dy) < self.ring.hub {
            None
        } else {
            self.chips.iter().position(|c| c.has(x, y))
        };
        if let Some(i) = chip {
            self.armed = true;
            if self.selected_chip != Some(i) {
                self.selected_chip = Some(i);

                self.canvas.setNeedsDisplay(true);
            }
            return;
        }
        if self.selected_chip.take().is_some() {
            self.canvas.setNeedsDisplay(true);
        }
        if self.selected.is_some() && dx.hypot(dy) > self.ring.outer - 4. {
            self.armed = dx.hypot(dy) <= self.ring.outer + 70.;
            return;
        }
        let next = if (x - self.anchor.0).hypot(y - self.anchor.1) < 12. {
            None
        } else {
            self.ring.hit(dx, dy, self.selected)
        };
        self.armed = next.is_some();
        if next != self.selected {
            self.selected = next;
            self.owner_changed();
        }
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
                self.open_target(&path);
            }
        }
    }
    pub(super) fn open_target(&self, target: &str) {
        let target = target.to_string();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let mut c = std::process::Command::new("/usr/bin/open");
            if target.ends_with(".app") {
                c.arg("-a");
            }
            if !c.arg(&target).status().is_ok_and(|s| s.success()) {
                let _ = tx.send(Message::Failure("Dock 项目暂时无法打开，请重新同步".into()));
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
            self.dismiss_capture();
        }
        let now = self.now();
        if self.toast.as_ref().is_some_and(|(_, d)| now >= *d) {
            if let Some((w, _)) = self.toast.take() {
                w.orderOut(None);
            }
        }
        let locked = session_locked();
        if locked != self.locked {
            self.locked = locked;
            self.capturing = false;
            self.gesture.reset();
            self.close_wheel();
            self.dismiss_capture();
            if !locked && self.trusted && !self.tap.is_null() {
                platform::CGEventTapEnable(self.tap, true);
            }
        }
        let trusted = platform::AXIsProcessTrusted();
        if trusted != self.trusted {
            self.trusted = trusted;
            self.tap_attempted = false;
            self.observer_retry = 0.;
            self.observer_failures = 0;
            self.dock_observer_pid = 0;
            if !self.tap.is_null() {
                platform::CGEventTapEnable(self.tap, trusted);
            }
            if !trusted {
                self.gesture.reset();
                self.close_wheel();
            }
            self.dock_due = now;
        }
        if trusted && self.tap.is_null() && !self.tap_attempted {
            self.install_hook();
        }
        if trusted && !self.locked {
            self.ensure_dock_observer();
        }
        while let Ok(message) = self.rx.try_recv() {
            match message {
                Message::Dock { items, exact } => {
                    self.dock_pending = false;
                    if !exact && self.dock_exact && !self.settings.wheel.is_empty() {
                        self.dock_due = now + 2.;
                        continue;
                    }
                    self.dock_failures = 0;
                    if self.notice.starts_with("Dock ") {
                        self.notice.clear();
                    }
                    self.dock_exact = exact;
                    self.last_dock = now;
                    let previous_paths = self.active_paths.clone();
                    let previous_slot = self.selected;
                    let previous_content = self
                        .selected_chip
                        .and_then(|i| self.entries.get(i))
                        .cloned();
                    let names = items
                        .iter()
                        .map(|i| (i.path.clone(), i.name.clone()))
                        .collect();
                    let paths = items.into_iter().map(|i| i.path).collect::<Vec<_>>();
                    if paths != self.settings.wheel {
                        self.settings.wheel = paths;
                        self.settings.names = names;
                        if self.active {
                            let slot = crate::model::remap_selection(
                                &previous_paths,
                                &self.settings.wheel,
                                previous_slot,
                            );
                            self.active_paths = self.settings.wheel.clone();
                            self.active_names = self.settings.names.clone();
                            let view = self.canvas.bounds().size;
                            self.ring = layout::DockLayout::new(
                                self.active_paths.len(),
                                self.settings.wheel_size_px,
                                view.width,
                                view.height,
                            );
                            self.render_size = self.ring.outer * 600. / 188.;
                            self.center = self.ring.center(self.anchor, view.width, view.height);
                            self.hover_mix.resize(self.active_paths.len(), 0.);
                            self.selected = slot;
                            if slot.is_none() {
                                self.armed = false;
                            }
                            self.owner_changed();
                            if let Some(previous) = previous_content {
                                self.selected_chip =
                                    crate::model::match_content(&previous, &self.entries);
                                if self.selected_chip.is_none() {
                                    self.armed = false;
                                }
                            }
                        }
                        self.icons.retain(|p, _| {
                            self.settings.wheel.contains(p)
                                || (self.active && self.active_paths.contains(p))
                        });
                        self.load_icons();
                        self.scan();
                    } else {
                        self.settings.names = names;
                        if self.active {
                            self.active_names = self.settings.names.clone();
                            self.canvas.setNeedsDisplay(true);
                        }
                    }
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
                }
                Message::DockFailed => {
                    self.dock_pending = false;
                    self.dock_failures = self.dock_failures.saturating_add(1);
                    self.dock_due =
                        now + (0.3 * 2f64.powi(self.dock_failures.min(5) as i32)).min(8.);
                    self.notice = "Dock 暂时不可用，正在自动恢复".into();
                }
                Message::Failure(text) => {
                    self.notice = text;
                    if !self.active && !self.capturing {
                        self.show_failure();
                    }
                }
            }
        }
        if let Some(label) = &self.menu_dock_label {
            let summary = NSString::from_str(&self.dock_summary());
            if !label.stringValue().isEqualToString(&summary) {
                label.setStringValue(&summary);
            }
        }
        let stamp = std::fs::metadata(
            std::path::PathBuf::from(std::env::var_os("HOME").unwrap())
                .join("Library/Preferences/com.apple.dock.plist"),
        )
        .ok()
        .and_then(|m| m.modified().ok());
        if stamp != self.dock_stamp {
            self.dock_stamp = stamp;
            self.dock_due = now + 0.2;
        }
        if !self.locked
            && ((self.dock_due > 0. && now >= self.dock_due)
                || (self.gesture.held && !self.active && now - self.last_dock > 0.3))
        {
            self.dock_due = 0.;
            self.refresh_dock();
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
        for e in std::mem::take(&mut self.effects) {
            match e {
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
        if self.active {
            for i in 0..self.active_paths.len() {
                let target = if self.selected == Some(i) && self.armed {
                    1.
                } else {
                    0.
                };
                let delta = target - self.hover_mix[i];
                if delta.abs() > 0.002 {
                    self.hover_mix[i] = if self.reduce_motion {
                        target
                    } else {
                        self.hover_mix[i] + delta * 0.35
                    };
                    self.canvas.setNeedsDisplay(true);
                }
            }
            let t = ((now - self.opened) / 0.14).clamp(0., 1.);
            self.overlay.setAlphaValue(if self.reduce_motion {
                1.
            } else {
                1. - (1. - t).powi(3)
            });
            if now - self.last_scan > 4. {
                self.scan();
            }
            let frame = (now * 6.) as usize;
            if frame != self.last_frame {
                self.last_frame = frame;
                self.canvas.setNeedsDisplay(true);
            }
            if self.trial.is_some_and(|d| now >= d) {
                self.close_wheel();
            }
        }
        if let Some(start) = self.closing {
            let alpha = if self.reduce_motion {
                0.
            } else {
                1. - (now - start) / 0.1
            };
            self.overlay.setAlphaValue(alpha.max(0.));
            if alpha <= 0. {
                self.overlay.orderOut(None);
                self.closing = None;
            }
        }
        if !self.active && (!self.gesture.held || self.gesture.cancelled) && self.closing.is_none()
        {
            if let Some(t) = self.anim_timer.take() {
                t.invalidate();
            }
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
    pub(super) unsafe fn show_failure(&mut self) {
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
