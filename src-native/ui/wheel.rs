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
        let ink = color("labelColor");
        self.draw_tether();
        // Let the system glass and icon artwork carry the interface; selection has no colored tile.
        let shadow = NSShadow::new();
        let indicator = color("secondaryLabelColor").colorWithAlphaComponent(0.42);
        for i in 0..self.active_paths.len() {
            let path = self.path_at(i);
            if path.is_empty() {
                continue;
            }
            let (x, y, side) = self.node_pose(i);
            let emphasis = self.hover_mix[i];
            if let Some(icon) = self.icons.get(&path) {
                NSGraphicsContext::saveGraphicsState_class();
                shadow.setShadowOffset(NSSize::new(0., -1.5 - 2. * emphasis));
                shadow.setShadowBlurRadius(3. + 7. * emphasis);
                shadow.setShadowColor(Some(&NSColor::colorWithWhite_alpha(
                    0.,
                    0.13 + 0.09 * emphasis,
                )));
                shadow.set();
                image(icon, r(x - side / 2., y - side / 2., side, side));
                NSGraphicsContext::restoreGraphicsState_class();
            }
            if self.running.contains(&path) {
                fill(
                    &NSBezierPath::bezierPathWithOvalInRect(rect(r(
                        x - 1.5,
                        y + side / 2. + 5.,
                        3.,
                        3.,
                    ))),
                    &indicator,
                );
            }
        }
        let dx = self.pointer.0 - cx;
        let dy = self.pointer.1 - cy;
        let reduced = NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion();
        let row = if self.selected.is_none() && self.tether.alpha() < 0.2 {
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
        let (name, frame, selected) = if let Some(index) = self.selected.filter(|_| self.armed) {
            let (x, y, side) = self.node_pose(index);
            let size = self.canvas.bounds().size;
            let width = 144f64.min(size.width - 16.);
            (
                self.title(&self.path_at(index)),
                r(
                    (x - width / 2.).clamp(8., size.width - width - 8.),
                    (y + side / 2. + 10.).clamp(8., size.height - 26.),
                    width,
                    20.,
                ),
                true,
            )
        } else {
            (
                "松开取消".into(),
                r(
                    cx - 64.,
                    cy + self.settings.wheel_size_px * 0.063,
                    128.,
                    22.,
                ),
                false,
            )
        };
        NSGraphicsContext::saveGraphicsState_class();
        let shadow = NSShadow::new();
        shadow.setShadowOffset(NSSize::new(0., -1.));
        shadow.setShadowBlurRadius(3.);
        shadow.setShadowColor(Some(&NSColor::colorWithWhite_alpha(
            if self.dark_appearance() { 0. } else { 1. },
            0.85,
        )));
        shadow.set();
        center_text(
            &name,
            frame,
            self.settings.wheel_size_px * 0.025,
            selected,
            &ink,
        );
        NSGraphicsContext::restoreGraphicsState_class();
    }
    pub(super) fn node_pose(&self, index: usize) -> (f64, f64, f64) {
        let node = &self.ring.nodes[index];
        let t = ((self.now() - self.opened) / 0.18).clamp(0., 1.);
        let intro = if self.reduce_motion {
            1.
        } else {
            0.95 + 0.05 * (1. - (1. - t).powi(3))
        };
        (
            self.center.0 + node.angle.cos() * node.radius * intro,
            self.center.1 + node.angle.sin() * node.radius * intro - self.hover_mix[index] * 3.5,
            self.ring.icon * (1. + 0.16 * self.hover_mix[index]),
        )
    }
    pub(super) unsafe fn dark_appearance(&self) -> bool {
        let names = NSArray::from_slice(&[
            ns_string!("NSAppearanceNameAqua"),
            ns_string!("NSAppearanceNameDarkAqua"),
        ]);
        self.canvas
            .effectiveAppearance()
            .bestMatchFromAppearancesWithNames(&names)
            .is_some_and(|name| name.isEqualToString(ns_string!("NSAppearanceNameDarkAqua")))
    }
    pub(super) unsafe fn draw_tether(&self) {
        let alpha = self.tether.alpha();
        if alpha < 0.006 {
            return;
        }
        let origin = Point::new(
            self.center.0,
            self.center.1 - self.settings.wheel_size_px * 0.022,
        );
        let tip = self.tether.tip();
        let length = origin.distance(tip);
        if length < 22. {
            return;
        }
        let (ux, uy) = ((tip.x - origin.x) / length, (tip.y - origin.y) / length);
        let start = NSPoint::new(origin.x + ux * 16., origin.y + uy * 16.);
        let control = NSPoint::new(
            (start.x + tip.x) / 2. - uy * self.tether.bend(),
            (start.y + tip.y) / 2. + ux * self.tether.bend() + self.tether.sag(),
        );
        let path = NSBezierPath::bezierPath();
        path.moveToPoint(start);
        path.curveToPoint_controlPoint1_controlPoint2(
            NSPoint::new(tip.x, tip.y),
            NSPoint::new(
                start.x + (control.x - start.x) * 2. / 3.,
                start.y + (control.y - start.y) * 2. / 3.,
            ),
            NSPoint::new(
                tip.x + (control.x - tip.x) * 2. / 3.,
                tip.y + (control.y - tip.y) * 2. / 3.,
            ),
        );
        path.setLineCapStyle(NSLineCapStyle::Round);
        NSColor::colorWithWhite_alpha(1., 0.5 * alpha).setStroke();
        path.setLineWidth(5.);
        path.stroke();
        let c = if self.dark_appearance() {
            NSColor::colorWithSRGBRed_green_blue_alpha(0.86, 0.74, 0.59, 0.86 * alpha)
        } else {
            NSColor::colorWithSRGBRed_green_blue_alpha(0.60, 0.47, 0.32, 0.82 * alpha)
        };
        c.setStroke();
        path.setLineWidth(3.8);
        path.stroke();
        // A few low-contrast diagonal threads read as webbing without adding an image asset.
        let weave = NSBezierPath::bezierPath();
        let steps = (length / 6.).floor().max(1.) as usize;
        for i in 1..steps {
            let t = i as f64 / steps as f64;
            let q = 1. - t;
            let x = q * q * start.x + 2. * q * t * control.x + t * t * tip.x;
            let y = q * q * start.y + 2. * q * t * control.y + t * t * tip.y;
            let tx = 2. * q * (control.x - start.x) + 2. * t * (tip.x - control.x);
            let ty = 2. * q * (control.y - start.y) + 2. * t * (tip.y - control.y);
            let d = tx.hypot(ty).max(1.);
            let (tx, ty) = (tx / d, ty / d);
            weave.moveToPoint(NSPoint::new(
                x - tx * 0.7 + ty * 1.1,
                y - ty * 0.7 - tx * 1.1,
            ));
            weave.lineToPoint(NSPoint::new(
                x + tx * 0.7 - ty * 1.1,
                y + ty * 0.7 + tx * 1.1,
            ));
        }
        NSColor::colorWithSRGBRed_green_blue_alpha(1., 0.94, 0.79, 0.52 * alpha).setStroke();
        weave.setLineWidth(0.65);
        weave.stroke();
        let direction = Point::new(ux, uy);
        Self::cord_loop(
            Point::new(origin.x + ux * 27., origin.y + uy * 27.),
            direction,
            5.2,
            3.4,
            alpha,
            &c,
        );
        Self::cord_loop(
            tip,
            direction,
            if self.tether.snapped { 5.6 } else { 10. },
            if self.tether.snapped { 4. } else { 6. },
            alpha,
            &c,
        );
    }
    unsafe fn cord_loop(
        center: Point,
        direction: Point,
        width: f64,
        height: f64,
        alpha: f64,
        ink: &NSColor,
    ) {
        let loop_path = NSBezierPath::bezierPath();
        for step in 0..=24 {
            let angle = std::f64::consts::TAU * step as f64 / 24.;
            let a = angle.cos() * width / 2.;
            let b = angle.sin() * height / 2.;
            let point = NSPoint::new(
                center.x + direction.x * a - direction.y * b,
                center.y + direction.y * a + direction.x * b,
            );
            if step == 0 {
                loop_path.moveToPoint(point)
            } else {
                loop_path.lineToPoint(point)
            }
        }
        loop_path.closePath();
        loop_path.setLineJoinStyle(NSLineJoinStyle::Round);
        NSColor::colorWithWhite_alpha(1., 0.65 * alpha).setStroke();
        loop_path.setLineWidth(3.2);
        loop_path.stroke();
        ink.setStroke();
        loop_path.setLineWidth(1.4);
        loop_path.stroke();
    }
    pub(super) unsafe fn sync_glass(&mut self) {
        let selected = self
            .selected
            .filter(|_| self.active && self.armed && self.tether.snapped);
        if selected.is_none() {
            for view in &self.glasses {
                view.setHidden(true);
            }
            return;
        }
        let index = selected.unwrap();
        let (x, y, _) = self.node_pose(index);
        let radius = self.ring.icon * 0.7;
        let frame = r(x - radius, y - radius, radius * 2., radius * 2.);
        let style = if self.dark_appearance() {
            0isize
        } else {
            1isize
        };
        if self.glasses.is_empty() {
            let glass = if AnyClass::get(c"NSGlassEffectView").is_some() {
                let view = native("NSGlassEffectView", frame);
                let _: () = msg_send![&*view,setStyle:style];
                view
            } else {
                let view = native("NSVisualEffectView", frame);
                let _: () = msg_send![&*view,setMaterial:6usize];
                let _: () = msg_send![&*view,setBlendingMode:0usize];
                let _: () = msg_send![&*view,setState:1usize];
                view
            };
            let root = self.overlay.contentView().unwrap();
            let _: () =
                msg_send![&*root,addSubview:&*glass,positioned:-1isize,relativeTo:&*self.canvas];
            self.glasses.push(glass);
        }
        let height = self.overlay.contentView().unwrap().bounds().size.height;
        let glass = &self.glasses[0];
        let next = NSRect::new(
            NSPoint::new(frame.x, height - frame.y - frame.h),
            NSSize::new(frame.w, frame.h),
        );
        if glass.frame() != next {
            if AnyClass::get(c"NSGlassEffectView").is_some() {
                let _: () = msg_send![&**glass,setCornerRadius:0.0f64];
            }
            glass.setFrame(next);
        }
        if AnyClass::get(c"NSGlassEffectView").is_some() {
            let old: isize = msg_send![&**glass, style];
            if old != style {
                let _: () = msg_send![&**glass,setStyle:style];
            }
            let _: () = msg_send![&**glass,setCornerRadius:radius-0.25];
        }
        if AnyClass::get(c"NSGlassEffectView").is_none() {
            glass.setWantsLayer(true);
            if let Some(layer) = glass.layer() {
                let _: () = msg_send![&*layer,setCornerRadius:radius];
                let _: () = msg_send![&*layer,setMasksToBounds:true];
            }
        }
        glass.setAlphaValue(self.hover_mix[index]);
        glass.setHidden(false);
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
        self.last_snap = None;
        self.tether.reset(
            Point::new(
                self.center.0,
                self.center.1 - self.settings.wheel_size_px * 0.022,
            ),
            self.now(),
        );
        self.overlay.setAlphaValue(0.);
        self.sync_glass();
        self.canvas.setNeedsDisplay(true);
        self.overlay.orderFrontRegardless();
        self.start_animation();
        self.refresh_running();
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
    pub(super) unsafe fn refresh_running(&mut self) {
        self.running = NSWorkspace::sharedWorkspace()
            .runningApplications()
            .iter()
            .filter_map(|app| {
                app.bundleURL()
                    .and_then(|url| url.path())
                    .map(|path| path.to_string())
            })
            .collect();
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
            self.selected = None;
            self.canvas.setNeedsDisplay(true);
            return;
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
        self.selected = next;
        self.canvas.setNeedsDisplay(true);
    }
    pub(super) unsafe fn execute_selection(&mut self) {
        if self.locked || !self.armed {
            return;
        }
        if let Some(i) = self.selected {
            self.open_target(&self.path_at(i));
        }
    }
    pub(super) fn open_target(&self, target: &str) {
        platform::activate("activate-app", target.to_string(), self.tx.clone());
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
        let trusted = platform::accessibility_trusted();
        if trusted != self.trusted {
            self.trusted = trusted;
            self.tap_attempted = false;
            self.tap_failures = 0;
            self.tap_retry_at = now;
            self.observer_retry = 0.;
            self.observer_failures = 0;
            self.dock_observer_pid = 0;
            if !trusted {
                self.remove_hook();
                self.gesture.reset();
                self.close_wheel();
                self.permission_guide_pending = true;
            }
            self.dock_due = now;
        }
        if self.permission_guide_pending && !self.locked {
            self.permission_guide_pending = false;
            let _: () = msg_send![&*self.delegate,performSelector:sel!(permissionGuide:),withObject:None::<&AnyObject>,afterDelay:0.0f64];
        }
        if trusted && self.tap.is_null() && !self.tap_attempted && now >= self.tap_retry_at {
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
                    self.refresh_running();
                    let previous_paths = self.active_paths.clone();
                    let previous_slot = self.selected;
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
                            self.center = self.ring.center(self.anchor, view.width, view.height);
                            self.hover_mix.resize(self.active_paths.len(), 0.);
                            self.selected = slot;
                            if slot.is_none() {
                                self.armed = false;
                            }
                            self.sync_glass();
                            self.canvas.setNeedsDisplay(true);
                        }
                        self.icons.retain(|p, _| {
                            self.settings.wheel.contains(p)
                                || (self.active && self.active_paths.contains(p))
                        });
                        self.load_icons();
                    } else {
                        self.settings.names = names;
                        if self.active {
                            self.active_names = self.settings.names.clone();
                            self.canvas.setNeedsDisplay(true);
                        }
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
            let frame = (now * 6.) as usize;
            if frame != self.last_frame {
                self.last_frame = frame;
                self.canvas.setNeedsDisplay(true);
            }
            if self.trial.is_some_and(|d| now >= d) {
                self.close_wheel();
            }
        }
        if self.active || self.closing.is_some() {
            let size = self.canvas.bounds().size;
            let target = self
                .selected
                .filter(|_| self.armed && self.active)
                .map(|i| {
                    let (x, y, side) = self.node_pose(i);
                    Target {
                        center: Point::new(x, y),
                        half_icon: side / 2.,
                        detour: if self.ring.nodes[i].ring > 0 {
                            let origin = Point::new(
                                self.center.0,
                                self.center.1 - self.settings.wheel_size_px * 0.022,
                            );
                            crate::tether::route_around(
                                origin,
                                Point::new(x, y),
                                self.ring
                                    .nodes
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, node)| node.ring < self.ring.nodes[i].ring)
                                    .map(|(j, _)| {
                                        let (x, y, side) = self.node_pose(j);
                                        (Point::new(x, y), side / 2.)
                                    }),
                            )
                        } else {
                            0.
                        },
                    }
                });
            let origin = Point::new(
                self.center.0,
                self.center.1 - self.settings.wheel_size_px * 0.022,
            );
            let enabled = self.active
                && (self.pointer.0 - self.anchor.0).hypot(self.pointer.1 - self.anchor.1) >= 12.;
            let changed = self.tether.advance(TetherFrame {
                now,
                origin,
                pointer: Point::new(
                    self.pointer.0.clamp(8., size.width - 8.),
                    self.pointer.1.clamp(8., size.height - 8.),
                ),
                target,
                max_length: self.ring.outer + 35.,
                enabled,
                reduced: self.reduce_motion,
            });
            if changed {
                self.canvas.setNeedsDisplay(true);
            }
            self.sync_glass();
            if self.active && self.tether.snapped {
                if let Some(index) = self.selected {
                    let key = self.path_at(index);
                    if self.last_snap.as_ref() != Some(&key) {
                        self.last_snap = Some(key);
                        if !self.reduce_motion {
                            NSHapticFeedbackManager::defaultPerformer()
                                .performFeedbackPattern_performanceTime(
                                    NSHapticFeedbackPattern::Alignment,
                                    NSHapticFeedbackPerformanceTime::DrawCompleted,
                                );
                        }
                    }
                }
            } else if !enabled {
                self.last_snap = None;
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
        if !self.trusted || !self.tap.is_null() {
            return;
        }
        let types = [1u32, 3, 5, 6, 7, 10, 11, 12, 22, 25, 26, 27];
        let mask = types.iter().fold(0u64, |m, t| m | (1u64 << t));
        self.tap_attempted = true;
        // A user-session filter avoids depending on the privileged HID tap location.
        self.tap = platform::CGEventTapCreate(
            platform::SESSION_TAP,
            0,
            0,
            mask,
            event_hook,
            ptr::null_mut(),
        );
        if !self.tap.is_null() {
            self.source = platform::CFMachPortCreateRunLoopSource(ptr::null_mut(), self.tap, 0);
            if !self.source.is_null() {
                platform::CFRunLoopAddSource(
                    platform::CFRunLoopGetMain(),
                    self.source,
                    platform::kCFRunLoopCommonModes,
                );
                platform::CGEventTapEnable(self.tap, true);
                self.tap_keyboard = platform::keyboard_tap_registered();
                if self.tap_keyboard == Some(false) {
                    self.remove_hook();
                }
            } else {
                self.remove_hook();
            }
        }
        if self.tap.is_null() {
            self.tap_failures = self.tap_failures.saturating_add(1);
            if self.tap_failures < 5 {
                self.tap_attempted = false;
                self.tap_retry_at = self.now() + 0.25 * 2f64.powi(self.tap_failures as i32 - 1);
            }
            self.notice = if self.tap_attempted {
                "键盘监听未连接，请从菜单检查授权或重新连接"
            } else {
                "键盘监听暂时不可用，正在重新连接"
            }
            .into();
            if self.tap_attempted && !self.locked {
                self.show_failure();
            }
        } else {
            self.tap_failures = 0;
            self.notice.clear();
        }
    }
    pub(super) unsafe fn remove_hook(&mut self) {
        self.tap_keyboard = None;
        if !self.source.is_null() {
            platform::CFRunLoopRemoveSource(
                platform::CFRunLoopGetMain(),
                self.source,
                platform::kCFRunLoopCommonModes,
            );
            platform::CFRelease(self.source);
            self.source = ptr::null_mut();
        }
        if !self.tap.is_null() {
            platform::CGEventTapEnable(self.tap, false);
            platform::CFMachPortInvalidate(self.tap);
            platform::CFRelease(self.tap);
            self.tap = ptr::null_mut();
        }
    }
    pub(super) fn stop(&mut self) {
        self.gesture.reset();
        unsafe {
            self.remove_hook();
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
