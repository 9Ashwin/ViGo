use super::*;
use crate::mascot as assets;
use objc2::DefinedClass;
use std::{cell::Cell, rc::Rc};

#[derive(Debug)]
pub(super) struct MascotImage {
    pub asset: assets::Asset,
    image: Retained<NSImage>,
}
impl MascotImage {
    pub(super) unsafe fn load(asset: assets::Asset) -> Result<Rc<Self>, String> {
        let image = NSImage::initWithContentsOfFile(
            NSImage::alloc(),
            &NSString::from_str(&assets::image_path(&asset).to_string_lossy()),
        )
        .ok_or("无法显示这份素材")?;
        image.setSize(NSSize::new(asset.width as f64, asset.height as f64));
        Ok(Rc::new(Self { asset, image }))
    }
    pub(super) unsafe fn draw(&self, cx: f64, cy: f64, size: f64, row: usize, frame: usize) {
        let b = self.asset.frame(row, frame);
        let scale = size / (b.width.max(b.height) as f64);
        let (w, h) = (b.width as f64 * scale, b.height as f64 * scale);
        self.image
            .drawInRect_fromRect_operation_fraction_respectFlipped_hints(
                rect(r(cx - w / 2., cy - h / 2., w, h)),
                rect(r(
                    b.x as f64,
                    (self.asset.height - b.y - b.height) as f64,
                    b.width as f64,
                    b.height as f64,
                )),
                NSCompositingOperation::SourceOver,
                1.,
                true,
                None,
            );
    }
    pub(super) unsafe fn draw_neutral(&self, cx: f64, cy: f64, size: f64) {
        if self.asset.layout == assets::Layout::DefaultCat {
            self.draw(cx, cy, size, 1, 0)
        } else {
            self.draw(cx, cy, size, 0, 6)
        }
    }
}
#[derive(Debug)]
pub(super) struct PreviewData {
    mascot: RefCell<Rc<MascotImage>>,
    birth: Instant,
    animate: Cell<bool>,
}
define_class!(#[unsafe(super(NSView))]#[thread_kind=MainThreadOnly]#[ivars=PreviewData]
 pub(super) struct PetPreview;
 impl PetPreview{#[unsafe(method(drawRect:))]fn draw(&self,_rect:NSRect){unsafe{let size=self.bounds().size;let cx=size.width/2.;let cy=size.height/2.;let circle=NSBezierPath::bezierPathWithOvalInRect(rect(r(8.,8.,size.width-16.,size.height-16.)));fill(&circle,&color("windowBackgroundColor"));let image=self.ivars().mascot.borrow();if self.ivars().animate.get(){let frames=if image.asset.layout==assets::Layout::CodexV2{6}else{7};image.draw(cx,cy,size.width.min(size.height)*0.7,0,((self.ivars().birth.elapsed().as_secs_f64()/0.28)as usize)%frames)}else{image.draw_neutral(cx,cy,size.width.min(size.height)*0.7);}}}}
);
impl PetPreview {
    unsafe fn new(mascot: Rc<MascotImage>, frame: Rect) -> Retained<Self> {
        let obj = Self::alloc(MainThreadMarker::new().unwrap()).set_ivars(PreviewData {
            mascot: RefCell::new(mascot),
            birth: Instant::now(),
            animate: Cell::new(false),
        });
        msg_send![super(obj),initWithFrame:rect(frame)]
    }
    fn set_mascot(&self, mascot: Rc<MascotImage>) {
        *self.ivars().mascot.borrow_mut() = mascot;
        self.setNeedsDisplay(true);
    }
}
pub(super) struct MascotControls {
    preview: Retained<PetPreview>,
    picker: Retained<NSPopUpButton>,
    info: Retained<NSTextField>,
    description: Retained<NSTextField>,
    name: Retained<NSTextField>,
    import: Retained<NSButton>,
    site: Retained<NSButton>,
    reset: Retained<NSButton>,
    generated: Retained<NSButton>,
    source: Retained<NSButton>,
    timer: RefCell<Option<Retained<NSTimer>>>,
}
impl Drop for MascotControls {
    fn drop(&mut self) {
        if let Some(t) = self.timer.borrow_mut().take() {
            t.invalidate();
        }
    }
}
unsafe fn field(
    parent: &NSView,
    title: &str,
    frame: Rect,
    size: f64,
    secondary: bool,
) -> Retained<NSTextField> {
    let f =
        NSTextField::labelWithString(&NSString::from_str(title), MainThreadMarker::new().unwrap());
    f.setFrame(rect(frame));
    f.setFont(Some(&NSFont::systemFontOfSize(size)));
    f.setTextColor(Some(&color(if secondary {
        "secondaryLabelColor"
    } else {
        "labelColor"
    })));
    f.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    parent.addSubview(&f);
    f
}
impl State {
    pub(super) unsafe fn build_mascot_settings(&self, parent: &NSView) -> MascotControls {
        super::preferences::card(parent, r(0., 190., 220., 264.))
            .setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        super::preferences::card(parent, r(238., 190., 422., 264.));
        super::preferences::card(parent, r(0., 0., 660., 174.));
        let preview = PetPreview::new(self.mascot.clone(), r(20., 246., 180., 180.));
        preview.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        parent.addSubview(&preview);
        let name = field(parent, "默认小猫", r(12., 207., 196., 25.), 15., false);
        name.setAlignment(NSTextAlignment::Center);
        field(
            parent,
            "换一位桌面伙伴",
            r(258., 409., 382., 26.),
            18.,
            false,
        );
        field(
            parent,
            "默认小猫会保留，随时可以恢复。",
            r(258., 381., 382., 22.),
            12.,
            true,
        );
        let picker = self.preferences_popup(
            parent,
            &["默认小猫"],
            r(258., 337., 382., 30.),
            sel!(mascotSelected:),
        );
        let import = self.preferences_button(parent, "导入素材…", r(258., 289., 180., 30.), 0);
        import.setAction(Some(sel!(mascotImport:)));
        let site = self.preferences_button(parent, "从网站导入…", r(450., 289., 190., 30.), 0);
        site.setAction(Some(sel!(mascotWebsite:)));
        let reset = self.preferences_button(parent, "恢复默认小猫", r(258., 241., 180., 30.), 810);
        self.preferences_button(parent, "浏览素材库 ↗", r(450., 241., 190., 30.), 813);
        let info = field(parent, "", r(258., 205., 260., 25.), 11., true);
        let source = self.preferences_button(parent, "查看来源 ↗", r(530., 200., 110., 28.), 814);
        source.setBordered(false);
        field(
            parent,
            "用 Codex 生成或修改",
            r(20., 135., 620., 24.),
            15.,
            false,
        );
        field(
            parent,
            "复制提示词 → Codex 生成并自动导入；也可手动选择素材。",
            r(20., 107., 620., 22.),
            11.,
            true,
        );
        let description = NSTextField::initWithFrame(
            NSTextField::alloc(MainThreadMarker::new().unwrap()),
            rect(r(20., 61., 620., 32.)),
        );
        description.setPlaceholderString(Some(ns_string!(
            "例如：白金盔甲的小骑士；或把当前小猫换成红色围巾"
        )));
        description.setFont(Some(&NSFont::systemFontOfSize(13.)));
        description.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewMinYMargin,
        );
        parent.addSubview(&description);
        self.preferences_button(parent, "复制生成提示词", r(20., 14., 180., 30.), 811)
            .setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        self.preferences_button(parent, "复制修改提示词", r(212., 14., 180., 30.), 812)
            .setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
        let generated =
            self.preferences_button(parent, "导入生成结果…", r(404., 14., 236., 30.), 0);
        generated.setAction(Some(sel!(mascotImport:)));
        MascotControls {
            preview,
            picker,
            info,
            description,
            name,
            import,
            site,
            reset,
            generated,
            source,
            timer: RefCell::new(None),
        }
    }
    pub(super) unsafe fn select_mascot(&mut self, id: Option<&str>) -> Result<(), String> {
        let asset = match id {
            Some(id) => assets::load(id)?,
            None => assets::default_asset(),
        };
        let mascot = MascotImage::load(asset)?;
        let previous = self.settings.mascot_id.clone();
        self.settings.mascot_id = id.map(str::to_owned);
        if let Err(error) = self.settings.save() {
            self.settings.mascot_id = previous;
            return Err(format!("桌面伙伴设置未保存：{error}"));
        }
        self.notice.clear();
        self.gesture.reset();
        self.close_wheel();
        self.mascot = mascot;
        self.refresh_preferences_apps();
        self.refresh_mascot_settings();
        Ok(())
    }
    pub(super) unsafe fn mascot_selected(&mut self, sender: &NSPopUpButton) {
        if let Some(item) = sender.selectedItem() {
            let value: Option<Retained<NSString>> = msg_send![&*item, representedObject];
            if let Some(value) = value {
                let id = value.to_string();
                if let Err(error) =
                    self.select_mascot(if id == "default" { None } else { Some(&id) })
                {
                    self.notice = error;
                    self.show_failure();
                }
            }
        }
    }
    pub(super) unsafe fn import_mascot(&mut self, source: String, website: bool) {
        if self.mascot_import_pending {
            return;
        }
        self.notice.clear();
        self.mascot_import_pending = true;
        self.refresh_mascot_settings();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = objc2::rc::autoreleasepool(|_| {
                if website {
                    assets::import_site(&source)
                } else {
                    assets::import_local(std::path::Path::new(&source))
                }
            });
            let _ = tx.send(Message::Mascot(result));
        });
    }
    pub(super) unsafe fn copy_mascot_prompt(&mut self, modify: bool) {
        let Some(p) = &self.preferences else { return };
        let Some(controls) = &p.mascot_controls else {
            return;
        };
        let description = controls.description.stringValue().to_string();
        let prompt = assets::generation_prompt(
            &description,
            if modify {
                Some(&self.mascot.asset)
            } else {
                None
            },
        );
        let clipboard = NSPasteboard::generalPasteboard();
        clipboard.clearContents();
        if clipboard.setString_forType(&NSString::from_str(&prompt), NSPasteboardTypeString) {
            self.notice = "提示词已复制，交给 Codex 生成，完成后会自动导入。".into();
        } else {
            self.notice = "提示词复制失败，请重试。".into();
        }
        self.show_failure();
    }
    pub(super) unsafe fn refresh_mascot_settings(&self) {
        let Some(p) = &self.preferences else { return };
        let Some(c) = &p.mascot_controls else { return };
        let mtm = MainThreadMarker::new().unwrap();
        let menu = NSMenu::new(mtm);
        menu.setAutoenablesItems(false);
        let add = |title: &str, id: &str| {
            let item = NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &NSString::from_str(title),
                None,
                ns_string!(""),
            );
            item.setRepresentedObject(Some(&NSString::from_str(id)));
            menu.addItem(&item);
        };
        add("默认小猫", "default");
        let assets = assets::list();
        let mut counts = std::collections::BTreeMap::new();
        for a in &assets {
            *counts.entry(&a.name).or_insert(0usize) += 1;
        }
        let mut ordinals = std::collections::BTreeMap::new();
        let mut selected = 0;
        for (i, asset) in assets.iter().enumerate() {
            let ordinal = ordinals.entry(&asset.name).or_insert(0usize);
            *ordinal += 1;
            let title = if counts[&asset.name] > 1 {
                format!("{} · {}", asset.name, ordinal)
            } else {
                asset.name.clone()
            };
            add(&title, &asset.id);
            if self.settings.mascot_id.as_deref() == Some(&asset.id) {
                selected = i as isize + 1;
            }
        }
        c.picker.setMenu(Some(&menu));
        c.picker.selectItemAtIndex(selected);
        c.picker.setEnabled(!self.mascot_import_pending);
        c.import.setEnabled(!self.mascot_import_pending);
        c.site.setEnabled(!self.mascot_import_pending);
        c.generated.setEnabled(!self.mascot_import_pending);
        c.source
            .setEnabled(!self.mascot.asset.source_url.is_empty());
        c.reset.setEnabled(self.settings.mascot_id.is_some());
        c.preview.set_mascot(self.mascot.clone());
        c.name
            .setStringValue(&NSString::from_str(&self.mascot.asset.name));
        let info = if self.mascot_import_pending {
            "正在导入并检查素材…".into()
        } else if !self.mascot.asset.author.is_empty() {
            format!("社区作者：{} · 来源保存在本机", self.mascot.asset.author)
        } else if self.settings.mascot_id.is_none() {
            "用户提供的小猫 · 默认内置".into()
        } else {
            "自定义素材 · 仅保存在本机".into()
        };
        c.info.setStringValue(&NSString::from_str(&info));
        let animate = p.page == 3
            && self.mascot.asset.layout != assets::Layout::Static
            && p.window.isVisible()
            && !NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion();
        c.preview.ivars().animate.set(animate);
        if animate && c.timer.borrow().is_none() {
            let timer = NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                0.2,
                &self.delegate,
                sel!(mascotTick:),
                None,
                true,
            );
            NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes);
            *c.timer.borrow_mut() = Some(timer);
        } else if !animate {
            if let Some(timer) = c.timer.borrow_mut().take() {
                timer.invalidate();
            }
        }
    }
    pub(super) unsafe fn mascot_tick(&self) {
        if let Some(p) = &self.preferences {
            if p.page == 3 && p.window.isVisible() && p.window.isKeyWindow() {
                if let Some(c) = &p.mascot_controls {
                    c.preview.setNeedsDisplay(true);
                }
            }
        }
    }
}
pub(super) unsafe fn choose_file() {
    with(|s| {
        s.close_wheel();
        s.gesture.reset();
        s.choosing_apps = true;
    });
    let panel = NSOpenPanel::openPanel(MainThreadMarker::new().unwrap());
    panel.setTitle(Some(ns_string!("导入桌面伙伴")));
    panel.setPrompt(Some(ns_string!("导入并使用")));
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(true);
    panel.setAllowsMultipleSelection(false);
    let path = if panel.runModal() == NSModalResponseOK {
        panel
            .URLs()
            .firstObject()
            .and_then(|url| url.path())
            .map(|p| p.to_string())
    } else {
        None
    };
    with(|s| {
        s.choosing_apps = false;
        if let Some(path) = path {
            s.import_mascot(path, false);
        }
    });
}
pub(super) unsafe fn choose_website() {
    with(|s| {
        s.close_wheel();
        s.gesture.reset();
        s.choosing_apps = true;
    });
    let mtm = MainThreadMarker::new().unwrap();
    let alert = NSAlert::new(mtm);
    alert.setMessageText(ns_string!("从 Codex Pets 导入"));
    alert.setInformativeText(ns_string!(
        "粘贴宠物详情链接。素材会保存在本机，默认小猫仍可随时恢复。"
    ));
    alert.addButtonWithTitle(ns_string!("导入并使用"));
    alert.addButtonWithTitle(ns_string!("取消"));
    let input = NSTextField::initWithFrame(NSTextField::alloc(mtm), rect(r(0., 0., 420., 28.)));
    input.setPlaceholderString(Some(ns_string!(
        "https://codex-pets.net/#/pets/aegis-sentinel"
    )));
    alert.setAccessoryView(Some(&input));
    let accepted = alert.runModal() == NSAlertFirstButtonReturn;
    let value = input.stringValue().to_string();
    with(|s| {
        s.choosing_apps = false;
        if accepted {
            s.import_mascot(value, true);
        }
    });
}
