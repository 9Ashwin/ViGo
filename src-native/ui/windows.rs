use super::*;
use crate::window_picker::{HEADER, ROW};

impl State {
    pub(super) unsafe fn tick_windows(&mut self) {
        if !self.active || !self.settings.show_window_list || !self.armed || self.selected.is_none()
        {
            if self.windows.path.is_some() {
                self.windows.reset();
            }
            self.sync_window_glass();
            return;
        }
        let index = self.selected.unwrap();
        let path = self.path_at(index);
        let icon = self.node_pose(index);
        let near = (self.pointer.0 - icon.0).hypot(self.pointer.1 - icon.1) <= icon.2 * 1.1;
        if self.windows.frame.is_none() && !near {
            if self.windows.path.is_some() {
                self.windows.reset();
            }
            return;
        }
        self.windows.hover(&path, self.now());
        if !self.window_query_pending && (self.gesture.held || self.trial.is_some()) {
            if let Some((generation, path)) = self.windows.request(self.now()) {
                self.window_query_pending = true;
                self.window_query_count += 1;
                platform::windows(generation, path, self.tx.clone());
            }
        }
        self.sync_window_glass();
    }
    pub(super) unsafe fn sync_window_glass(&mut self) {
        let frame = self
            .windows
            .frame
            .filter(|_| self.active && self.settings.show_window_list);
        let Some(frame) = frame else {
            if let Some(view) = &self.window_glass {
                view.setHidden(true);
            }
            return;
        };
        if self.window_glass.is_none() {
            let view = native("NSVisualEffectView", frame);
            let _: () = msg_send![&*view,setMaterial:6usize];
            let _: () = msg_send![&*view,setBlendingMode:0usize];
            let _: () = msg_send![&*view,setState:1usize];
            view.setWantsLayer(true);
            if let Some(layer) = view.layer() {
                let _: () = msg_send![&*layer,setCornerRadius:14f64];
                let _: () = msg_send![&*layer,setMasksToBounds:true];
            }
            let root = self.overlay.contentView().unwrap();
            let _: () =
                msg_send![&*root,addSubview:&*view,positioned:-1isize,relativeTo:&*self.canvas];
            self.window_glass = Some(view);
        }
        let view = self.window_glass.as_ref().unwrap();
        view.setFrame(rect(r(
            frame.x,
            self.canvas.bounds().size.height - frame.y - frame.h,
            frame.w,
            frame.h,
        )));
        view.setHidden(false);
    }
    pub(super) unsafe fn draw_windows(&self) {
        let Some(f) = self
            .windows
            .frame
            .filter(|_| self.settings.show_window_list)
        else {
            return;
        };
        // The local glass covers neighbouring icons so rows remain legible on either side.
        let background = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(rect(f), 14., 14.);
        fill(
            &background,
            &color("windowBackgroundColor").colorWithAlphaComponent(0.82),
        );
        color("separatorColor")
            .colorWithAlphaComponent(0.45)
            .setStroke();
        background.setLineWidth(0.6);
        background.stroke();
        let title = self
            .windows
            .path
            .as_ref()
            .map(|p| self.title(p))
            .unwrap_or_default();
        window_text(
            &format!("{title} · {} 个窗口", self.windows.items.len()),
            r(f.x + 12., f.y + 10., f.w - 78., 20.),
            11.,
            false,
            &color("secondaryLabelColor"),
        );
        center_text(
            &format!("{}/{}", self.windows.selected + 1, self.windows.items.len()),
            r(f.x + f.w - 62., f.y + 10., 50., 20.),
            11.,
            false,
            &color("secondaryLabelColor"),
        );
        for (row, item) in self
            .windows
            .items
            .iter()
            .skip(self.windows.first)
            .take(self.windows.rows)
            .enumerate()
        {
            let index = self.windows.first + row;
            let rect = r(
                f.x + 6.,
                f.y + HEADER + row as f64 * ROW,
                f.w - 12.,
                ROW - 2.,
            );
            let chosen = index == self.windows.selected;
            if chosen {
                fill(
                    &NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(
                        super::rect(rect),
                        8.,
                        8.,
                    ),
                    &color("selectedContentBackgroundColor").colorWithAlphaComponent(0.18),
                );
            }
            let mut title = item.title.clone();
            if !item.space.is_empty() {
                title.push_str(" · ");
                title.push_str(&item.space);
            }
            if item.minimized {
                title.push_str(" · 已最小化");
            }
            window_text(
                &title,
                r(rect.x + 10., rect.y + 9., rect.w - 32., 22.),
                12.,
                chosen,
                &color("labelColor"),
            );
            if chosen {
                center_text(
                    "✓",
                    r(rect.x + rect.w - 23., rect.y + 9., 20., 22.),
                    12.,
                    true,
                    &color("labelColor"),
                );
            }
        }
    }
}
unsafe fn window_text(value: &str, frame: Rect, size: f64, bold: bool, ink: &NSColor) {
    let style = NSMutableParagraphStyle::new();
    style.setAlignment(NSTextAlignment::Left);
    style.setLineBreakMode(NSLineBreakMode::ByTruncatingMiddle);
    let font = NSFont::systemFontOfSize_weight(size, if bold { 0.35 } else { 0. });
    let attributes = NSDictionary::<NSString, AnyObject>::from_slices(
        &[
            ns_string!("NSFont"),
            ns_string!("NSColor"),
            ns_string!("NSParagraphStyle"),
        ],
        &[&*font, ink, &*style],
    );
    let text = NSString::from_str(value);
    let _: () = msg_send![&*text,drawInRect:rect(frame),withAttributes:&*attributes];
}
