//! Lazily created AppKit settings. Dock remains the source; exclusions belong to ViGo.
use super::*;

define_class!(#[unsafe(super(NSView))]#[thread_kind=MainThreadOnly]struct AppList;
 impl AppList{#[unsafe(method(isFlipped))]fn flipped(&self)->bool{true}}
);
define_class!(#[unsafe(super(NSView))]#[thread_kind=MainThreadOnly]struct PreferencesRoot;
 impl PreferencesRoot{#[unsafe(method(drawRect:))]fn draw(&self,_r:NSRect){unsafe{let background=color("windowBackgroundColor");let background=background.blendedColorWithFraction_ofColor(0.035,&color("labelColor")).unwrap_or(background);background.setFill();NSBezierPath::fillRect(self.bounds());}}}
);
pub(super) struct Preferences {
    pub window: Retained<NSWindow>,
    page: usize,
    panes: [Retained<NSView>; 3],
    editor: Retained<NSView>,
    preview: Retained<super::preferences_wheel::WheelPreview>,
    catalog_scroll: Retained<NSScrollView>,
    catalog_label: Retained<NSTextField>,
    catalog_icons: RefCell<HashMap<String, Retained<NSImage>>>,
    layout: usize,
    breadcrumb: Retained<NSTextField>,
    nav_status: Retained<NSTextField>,
    gesture_hint: Retained<NSTextField>,
    selection: Retained<NSBox>,
    heading: Retained<NSTextField>,
    description: Retained<NSTextField>,
    search: Retained<NSSearchField>,
    scroll: Retained<NSScrollView>,
    summary: Retained<NSTextField>,
    restore: Retained<NSButton>,
    rows: Vec<AppRow>,
    trigger: Retained<NSPopUpButton>,
    size: Retained<NSSlider>,
    size_label: Retained<NSTextField>,
    appearance: Retained<NSPopUpButton>,
    enabled: Retained<NSButton>,
    login: Retained<NSButton>,
    permission: Retained<NSTextField>,
}
struct AppRow {
    path: String,
    checkbox: Retained<NSButton>,
    status: Retained<NSTextField>,
}
pub(super) unsafe fn choose_applications() {
    with(|s| {
        s.close_wheel();
        s.gesture.reset();
        s.choosing_apps = true;
    });
    let mtm = MainThreadMarker::new().unwrap();
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setTitle(Some(ns_string!("添加软件到 ViGo")));
    panel.setPrompt(Some(ns_string!("添加到轮盘")));
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(false);
    panel.setAllowsMultipleSelection(true);
    panel.setTreatsFilePackagesAsDirectories(false);
    panel.setDirectoryURL(Some(&NSURL::fileURLWithPath(ns_string!("/Applications"))));
    if let Some(kind) = AnyClass::get(c"UTType") {
        let app_type: Option<Retained<AnyObject>> =
            msg_send![kind,typeWithIdentifier:ns_string!("com.apple.application-bundle")];
        if let Some(app_type) = app_type {
            let types = NSArray::from_retained_slice(&[app_type]);
            let _: () = msg_send![&*panel,setAllowedContentTypes:&*types];
        }
    }
    let mut added = vec![];
    if panel.runModal() == NSModalResponseOK {
        for url in panel.URLs() {
            let Some(path) = url.path() else { continue };
            let Ok(path) = std::path::Path::new(&path.to_string()).canonicalize() else {
                continue;
            };
            if !path
                .extension()
                .is_some_and(|p| p.eq_ignore_ascii_case("app"))
            {
                continue;
            }
            let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
            if NSBundle::bundleWithURL(&url)
                .and_then(|b| b.executableURL())
                .is_some()
            {
                added.push(path.to_string_lossy().into_owned());
            }
        }
    }
    with(|s| {
        s.choosing_apps = false;
        if !added.is_empty() {
            for path in &added {
                s.settings.add_application(path);
            }
            s.save();
            s.refresh_preferences_apps();
        }
    });
}
unsafe fn text(
    parent: &NSView,
    title: &str,
    frame: Rect,
    size: f64,
    bold: bool,
    secondary: bool,
) -> Retained<NSTextField> {
    let field =
        NSTextField::labelWithString(&NSString::from_str(title), MainThreadMarker::new().unwrap());
    field.setFrame(rect(frame));
    field.setFont(Some(&NSFont::systemFontOfSize_weight(
        size,
        if bold { 0.4 } else { 0. },
    )));
    field.setTextColor(Some(&color(if secondary {
        "secondaryLabelColor"
    } else {
        "labelColor"
    })));
    if frame.y > 100. {
        field.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
    }
    parent.addSubview(&field);
    field
}
unsafe fn card(parent: &NSView, frame: Rect) -> Retained<NSBox> {
    let box_view =
        NSBox::initWithFrame(NSBox::alloc(MainThreadMarker::new().unwrap()), rect(frame));
    box_view.setBoxType(NSBoxType::Custom);
    box_view.setBorderWidth(0.6);
    box_view.setBorderColor(&color("labelColor").colorWithAlphaComponent(0.09));
    box_view.setCornerRadius(12.);
    box_view.setFillColor(&color("controlBackgroundColor"));
    box_view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewMinYMargin,
    );
    parent.addSubview(&box_view);
    box_view
}
impl State {
    pub(super) unsafe fn preferences_layout(&mut self, layout: usize) {
        if let Some(p) = self.preferences.as_mut() {
            p.layout = layout.min(1);
            p.scroll.setHidden(p.layout == 0);
            p.editor.setHidden(p.layout != 0);
        }
        self.refresh_preferences_apps();
    }
    pub(super) unsafe fn preferences_catalog_app(&mut self, sender: &NSButton) {
        let path: Option<Retained<NSString>> = msg_send![sender, identifier];
        if let Some(path) = path {
            let path = path.to_string();
            if self.settings.visible_apps().contains(&path) {
                self.settings.set_app_visible(&path, false);
            } else {
                if !self.settings.all_apps().contains(&path) {
                    self.settings.add_application(&path);
                }
                self.settings.set_app_visible(&path, true);
            }
            self.save();
            self.refresh_preferences_apps();
        }
    }
    unsafe fn refresh_preferences_editor(&self) {
        let Some(p) = &self.preferences else { return };
        let paths = self.settings.visible_apps();
        let icons = paths
            .iter()
            .map(|path| NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(path)))
            .collect();
        p.preview.update(paths.clone(), icons);
        let query = p.search.stringValue().to_string().trim().to_lowercase();
        let mut apps: std::collections::BTreeMap<String, String> = self
            .catalog
            .iter()
            .map(|a| (a.path.clone(), self.settings.name(&a.path)))
            .collect();
        for path in self.settings.all_apps() {
            apps.entry(path.clone())
                .or_insert_with(|| self.settings.name(&path));
        }
        let mut apps: Vec<_> = apps
            .into_iter()
            .filter(|(path, name)| {
                name.to_lowercase().contains(&query)
                    || std::path::Path::new(path)
                        .file_stem()
                        .is_some_and(|s| s.to_string_lossy().to_lowercase().contains(&query))
            })
            .collect();
        apps.sort_by_key(|a| a.1.to_lowercase());
        p.catalog_label.setStringValue(&NSString::from_str(&format!(
            "已安装的软件 · {}",
            apps.len()
        )));
        let width = p.catalog_scroll.contentSize().width;
        let columns = (width / 90.).floor().max(1.) as usize;
        let tile = width / columns as f64;
        let height =
            (apps.len().div_ceil(columns) as f64 * 86.).max(p.catalog_scroll.contentSize().height);
        let document: Retained<AppList> = msg_send![AppList::alloc(MainThreadMarker::new().unwrap()),initWithFrame:rect(r(0.,0.,width,height))];
        for (i, (path, name)) in apps.into_iter().enumerate() {
            let b = self.preferences_button(
                &document,
                &name,
                r(
                    (i % columns) as f64 * tile,
                    (i / columns) as f64 * 86.,
                    tile,
                    82.,
                ),
                0,
            );
            b.setButtonType(NSButtonType::PushOnPushOff);
            b.setBezelStyle(NSBezelStyle::Push);
            b.setBordered(false);
            b.setImagePosition(NSCellImagePosition::ImageAbove);
            b.setFont(Some(&NSFont::systemFontOfSize(10.)));
            let mut cached = p.catalog_icons.borrow_mut();
            let icon = cached.entry(path.clone()).or_insert_with(|| {
                let original =
                    NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(&path));
                let icon: Retained<NSImage> = msg_send![&*original, copy];
                icon.setSize(NSSize::new(36., 36.));
                icon
            });
            b.setImage(Some(icon));
            if let Some(cell) = b.cell() {
                cell.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
            }
            if paths.contains(&path) {
                text(&b, "✓", r(tile - 23., 6., 18., 18.), 11., true, false)
                    .setTextColor(Some(&color("controlAccentColor")));
            }
            b.setState(if paths.contains(&path) { 1 } else { 0 });
            b.setAction(Some(sel!(settingsCatalogApp:)));
            b.setToolTip(Some(&NSString::from_str(&format!(
                "{} · {}",
                name,
                if paths.contains(&path) {
                    "点击隐藏"
                } else {
                    "点击添加"
                }
            ))));
            b.setAutoresizingMask(NSAutoresizingMaskOptions::empty());
            let _: () = msg_send![&*b,setIdentifier:&*NSString::from_str(&path)];
        }
        let old = p.catalog_scroll.contentView().bounds().origin.y;
        p.catalog_scroll.setDocumentView(Some(&document));
        p.catalog_scroll.contentView().scrollToPoint(NSPoint::new(
            0.,
            old.min((height - p.catalog_scroll.contentSize().height).max(0.)),
        ));
        p.catalog_scroll
            .reflectScrolledClipView(&p.catalog_scroll.contentView());
    }
    #[cfg(feature = "diagnostics")]
    pub(super) unsafe fn preferences_diagnostics(&self) -> serde_json::Value {
        serde_json::json!({"visible":self.preferences.as_ref().is_some_and(|p|p.window.isVisible()),"page":self.preferences.as_ref().map(|p|p.page),"window":self.preferences.as_ref().map(|p|p.window.windowNumber()),"catalogCount":self.catalog.len(),"previewNodes":self.preferences.as_ref().map(|p|p.preview.centers().iter().map(|(x,y)|{let point=p.window.convertPointToScreen(p.preview.convertPoint_toView(NSPoint::new(*x,*y),None));let height=NSScreen::screens(MainThreadMarker::new().unwrap()).firstObject().unwrap().frame().size.height;[point.x,height-point.y]}).collect::<Vec<_>>()),"policy":self.app.activationPolicy().0,"candidates":self.settings.all_apps(),"visibleApps":self.settings.visible_apps(),"hidden":self.settings.hidden_apps,"added":self.settings.added_apps,"order":self.settings.app_order,"choosing":self.choosing_apps})
    }
    #[cfg(feature = "diagnostics")]
    pub(super) unsafe fn preferences_snapshot(&self, dir: &std::path::Path) {
        if let Some(p) = &self.preferences {
            if let Some(view) = p.window.contentView() {
                let bounds = view.bounds();
                if let Some(rep) = view.bitmapImageRepForCachingDisplayInRect(bounds) {
                    view.cacheDisplayInRect_toBitmapImageRep(bounds, &rep);
                    if let Some(data) = rep.representationUsingType_properties(
                        NSBitmapImageFileType::PNG,
                        &NSDictionary::new(),
                    ) {
                        let _ = data.writeToURL_atomically(
                            &NSURL::fileURLWithPath(&NSString::from_str(
                                &dir.join("preferences.png").to_string_lossy(),
                            )),
                            true,
                        );
                    }
                }
            }
        }
    }
    pub(super) unsafe fn build_application_menu(&self) {
        let main = NSMenu::new(MainThreadMarker::new().unwrap());
        let app = self.submenu(&main, "ViGo");
        self.item(&app, "关于 ViGo", 50, false, true);
        let settings = self.item(&app, "设置…", 500, false, true);
        settings.setKeyEquivalent(ns_string!(","));
        app.addItem(&NSMenuItem::separatorItem(MainThreadMarker::new().unwrap()));
        let quit = self.item(&app, "退出 ViGo", 3, false, true);
        quit.setKeyEquivalent(ns_string!("q"));
        let edit = self.submenu(&main, "编辑");
        edit.setAutoenablesItems(true);
        for (title, key, action) in [
            ("撤销", "z", sel!(undo:)),
            ("剪切", "x", sel!(cut:)),
            ("复制", "c", sel!(copy:)),
            ("粘贴", "v", sel!(paste:)),
            ("全选", "a", sel!(selectAll:)),
        ] {
            let item = self.item(&edit, title, -1, false, true);
            item.setTarget(None);
            item.setAction(Some(action));
            item.setKeyEquivalent(&NSString::from_str(key));
        }
        self.app.setMainMenu(Some(&main));
    }
    unsafe fn preferences_button(
        &self,
        parent: &NSView,
        title: &str,
        frame: Rect,
        tag: isize,
    ) -> Retained<NSButton> {
        let b = NSButton::new(MainThreadMarker::new().unwrap());
        b.setTitle(&NSString::from_str(title));
        b.setBezelStyle(NSBezelStyle::Push);
        b.setTarget(Some(&*self.delegate));
        b.setAction(Some(sel!(action:)));
        b.setTag(tag);
        b.setFrame(rect(frame));
        b.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewMinXMargin | NSAutoresizingMaskOptions::ViewMinYMargin,
        );
        parent.addSubview(&b);
        b
    }
    unsafe fn preferences_popup(
        &self,
        parent: &NSView,
        titles: &[&str],
        frame: Rect,
        action: objc2::runtime::Sel,
    ) -> Retained<NSPopUpButton> {
        let b = NSPopUpButton::initWithFrame_pullsDown(
            NSPopUpButton::alloc(MainThreadMarker::new().unwrap()),
            rect(frame),
            false,
        );
        for title in titles {
            b.addItemWithTitle(&NSString::from_str(title));
        }
        b.setTarget(Some(&*self.delegate));
        b.setAction(Some(action));
        b.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewMinXMargin | NSAutoresizingMaskOptions::ViewMinYMargin,
        );
        parent.addSubview(&b);
        b
    }
    pub(super) unsafe fn show_preferences(&mut self) {
        self.close_wheel();
        self.status_menu.cancelTracking();
        if self.preferences.is_none() {
            let mtm = MainThreadMarker::new().unwrap();
            let w = NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                rect(r(0., 0., 900., 640.)),
                NSWindowStyleMask::Titled
                    | NSWindowStyleMask::Closable
                    | NSWindowStyleMask::Miniaturizable
                    | NSWindowStyleMask::Resizable
                    | NSWindowStyleMask::FullSizeContentView,
                NSBackingStoreType::Buffered,
                false,
            );
            w.setTitle(ns_string!("ViGo 设置"));
            w.setTitleVisibility(NSWindowTitleVisibility::Hidden);
            w.setTitlebarAppearsTransparent(true);
            w.setReleasedWhenClosed(false);
            w.setContentMinSize(NSSize::new(900., 640.));
            w.setDelegate(Some(ProtocolObject::from_ref(&*self.delegate)));
            w.setBackgroundColor(Some(&color("windowBackgroundColor")));
            let root: Retained<PreferencesRoot> =
                msg_send![PreferencesRoot::alloc(mtm),initWithFrame:rect(r(0.,0.,900.,640.))];
            root.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            let sidebar = NSVisualEffectView::initWithFrame(
                NSVisualEffectView::alloc(mtm),
                rect(r(0., 0., 184., 640.)),
            );
            sidebar.setMaterial(NSVisualEffectMaterial::Sidebar);
            sidebar.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
            sidebar.setAutoresizingMask(NSAutoresizingMaskOptions::ViewHeightSizable);
            root.addSubview(&sidebar);
            let icon = NSImageView::new(mtm);
            icon.setImage(self.app.applicationIconImage().as_deref());
            icon.setFrame(rect(r(22., 541., 32., 32.)));
            icon.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
            sidebar.addSubview(&icon);
            text(&sidebar, "ViGo", r(66., 553., 110., 24.), 16., true, false);
            text(
                &sidebar,
                "V for your desktop.",
                r(66., 535., 115., 20.),
                11.,
                false,
                true,
            );
            let selection = card(&sidebar, r(12., 476., 160., 34.));
            selection.setBorderWidth(0.);
            selection.setCornerRadius(8.);
            selection.setFillColor(&color("controlAccentColor").colorWithAlphaComponent(0.12));
            for (i, (title, symbol)) in [
                ("我的轮盘", "chart.pie"),
                ("呼出与手势", "cursorarrow"),
                ("通用设置", "gearshape"),
            ]
            .iter()
            .enumerate()
            {
                let b = self.preferences_button(
                    &sidebar,
                    title,
                    r(22., 476. - i as f64 * 38., 140., 34.),
                    i as isize,
                );
                b.setBordered(false);
                b.setAlignment(NSTextAlignment::Left);
                b.setAction(Some(sel!(settingsPage:)));
                b.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
                if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
                    &NSString::from_str(symbol),
                    None,
                ) {
                    b.setImage(Some(&image));
                    b.setImagePosition(NSCellImagePosition::ImageLeft);
                }
            }
            let nav_status = text(
                &sidebar,
                "输入助手已就绪",
                r(24., 52., 180., 20.),
                11.,
                false,
                true,
            );
            let gesture_hint = text(
                &sidebar,
                "按住 V · 移动 · 松开",
                r(24., 28., 190., 20.),
                11.,
                false,
                true,
            );
            let breadcrumb = text(
                &root,
                "设置  /  我的轮盘",
                r(212., 603., 580., 22.),
                12.,
                false,
                true,
            );
            let heading = text(
                &root,
                "我的轮盘",
                r(212., 541., 450., 38.),
                28.,
                true,
                false,
            );
            let description = text(
                &root,
                "跟随 Dock，也能放进你的其他常用软件。",
                r(212., 519., 600., 23.),
                13.,
                false,
                true,
            );
            self.preferences_button(&root, "试一下轮盘", r(754., 538., 116., 30.), 20);
            let panes = [
                native("NSView", r(212., 48., 660., 454.)),
                native("NSView", r(212., 48., 660., 454.)),
                native("NSView", r(212., 48., 660., 454.)),
            ];
            for pane in &panes {
                pane.setAutoresizingMask(
                    NSAutoresizingMaskOptions::ViewWidthSizable
                        | NSAutoresizingMaskOptions::ViewHeightSizable,
                );
                root.addSubview(pane);
            }
            panes[1].setHidden(true);
            panes[2].setHidden(true);
            let summary = text(&panes[0], "", r(0., 426., 660., 22.), 13., true, false);
            let search = NSSearchField::initWithFrame(
                NSSearchField::alloc(mtm),
                rect(r(326., 380., 334., 30.)),
            );
            search.setPlaceholderString(Some(ns_string!("搜索软件")));
            search.setTarget(Some(&*self.delegate));
            search.setAction(Some(sel!(settingsSearch:)));
            search.setSendsSearchStringImmediately(true);
            search.setSendsWholeSearchString(false);
            search.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewMinYMargin,
            );
            panes[0].addSubview(&search);
            for (i, title) in ["轮盘编辑", "顺序列表"].iter().enumerate() {
                let b = self.preferences_button(
                    &panes[0],
                    title,
                    r(i as f64 * 100., 380., 96., 30.),
                    i as isize,
                );
                b.setAction(Some(sel!(settingsLayout:)));
                b.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
            }

            let add = self.preferences_button(&panes[0], "添加 App…", r(512., 421., 148., 30.), 0);
            add.setAction(Some(sel!(chooseApplications:)));
            let reset = self.preferences_button(&panes[0], "默认顺序", r(442., 0., 106., 30.), 502);
            reset.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);

            let scroll =
                NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(r(0., 48., 660., 316.)));
            scroll.setHasVerticalScroller(true);
            scroll.setAutohidesScrollers(true);
            scroll.setBorderType(NSBorderType::NoBorder);
            scroll.setBackgroundColor(&color("controlBackgroundColor"));
            scroll.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            scroll.setWantsLayer(true);
            if let Some(layer) = scroll.layer() {
                let _: () = msg_send![&*layer,setCornerRadius:12.0f64];
                let _: () = msg_send![&*layer,setMasksToBounds:true];
            }
            panes[0].addSubview(&scroll);
            scroll.setHidden(true);
            let editor = native("NSView", r(0., 48., 660., 316.));
            editor.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            card(&editor, r(0., 0., 660., 316.)).setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            let preview = super::preferences_wheel::WheelPreview::new(
                r(8., 8., 300., 300.),
                self.sheet.clone(),
            );
            preview.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
            editor.addSubview(&preview);
            let catalog_label = text(
                &editor,
                "已安装的软件",
                r(328., 285., 318., 22.),
                12.,
                true,
                false,
            );
            let catalog_scroll = NSScrollView::initWithFrame(
                NSScrollView::alloc(mtm),
                rect(r(320., 12., 330., 264.)),
            );
            catalog_scroll.setHasVerticalScroller(true);
            catalog_scroll.setAutohidesScrollers(true);
            catalog_scroll.setDrawsBackground(false);
            catalog_scroll.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            editor.addSubview(&catalog_scroll);
            panes[0].addSubview(&editor);

            text(
                &panes[0],
                "点选软件添加 / 隐藏，拖动轮盘图标调整顺序。",
                r(0., 9., 430., 18.),
                11.,
                false,
                true,
            );
            let restore =
                self.preferences_button(&panes[0], "全部显示", r(554., 0., 106., 30.), 501);
            restore.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);
            let general = &panes[1];
            card(general, r(0., 238., 660., 216.));
            text(general, "呼出键", r(20., 407., 280., 23.), 13., true, false);
            text(
                general,
                "轻按正常输入，按住后移动选择。",
                r(20., 385., 330., 20.),
                11.,
                false,
                true,
            );
            let trigger = self.preferences_popup(
                general,
                &["V（默认）", "自定义按键…", "鼠标侧键 4", "鼠标侧键 5"],
                r(440., 395., 200., 30.),
                sel!(settingsTrigger:),
            );
            text(
                general,
                "轮盘大小",
                r(20., 330., 220., 23.),
                13.,
                true,
                false,
            );
            text(
                general,
                "软件较多时自动排到外圈。",
                r(20., 308., 300., 20.),
                11.,
                false,
                true,
            );
            let size = NSSlider::sliderWithValue_minValue_maxValue_target_action(
                self.settings.wheel_size_px,
                360.,
                720.,
                Some(&*self.delegate),
                Some(sel!(settingsSize:)),
                mtm,
            );
            size.setFrame(rect(r(370., 321., 190., 24.)));
            size.setContinuous(false);
            size.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewMinXMargin
                    | NSAutoresizingMaskOptions::ViewMinYMargin,
            );
            general.addSubview(&size);
            let size_label = text(general, "", r(569., 322., 72., 22.), 12., false, true);
            size_label.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewMinXMargin
                    | NSAutoresizingMaskOptions::ViewMinYMargin,
            );
            card(general, r(0., 130., 660., 80.));
            text(
                general,
                "轻按仍然输入",
                r(20., 174., 550., 22.),
                13.,
                true,
                false,
            );
            text(
                general,
                "轻按呼出键正常输入，组合键照常使用。",
                r(20., 148., 550., 20.),
                12.,
                false,
                true,
            );
            card(general, r(0., 22., 660., 84.));
            text(
                general,
                "取消选择",
                r(20., 69., 550., 22.),
                13.,
                true,
                false,
            );
            text(
                general,
                "移回中心再松手，或按 Esc、鼠标右键。",
                r(20., 42., 550., 20.),
                12.,
                false,
                true,
            );
            let general = &panes[2];
            card(general, r(0., 380., 660., 74.));
            text(general, "外观", r(20., 407., 260., 23.), 13., true, false);
            let appearance = self.preferences_popup(
                general,
                &["跟随系统", "浅色", "深色"],
                r(440., 399., 200., 30.),
                sel!(settingsAppearance:),
            );
            card(general, r(0., 238., 660., 126.));
            let enabled = self.preferences_button(general, "启用 ViGo", r(20., 316., 570., 26.), 2);
            enabled.setButtonType(NSButtonType::Switch);
            enabled.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
            let login = self.preferences_button(general, "登录时启动", r(20., 270., 570., 26.), 40);
            login.setButtonType(NSButtonType::Switch);
            login.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
            let permission = text(general, "", r(0., 190., 660., 22.), 12., false, true);
            permission.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
            self.preferences_button(general, "辅助功能…", r(0., 140., 130., 30.), 21)
                .setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
            self.preferences_button(general, "复制诊断信息", r(144., 140., 140., 30.), 23)
                .setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinYMargin);
            card(general, r(0., 12., 660., 88.));
            text(
                general,
                &format!("ViGo {}", env!("CARGO_PKG_VERSION")),
                r(20., 62., 560., 23.),
                15.,
                true,
                false,
            );
            text(
                general,
                "原生 macOS 轮盘 · 按住 V，把常用软件放到手边。",
                r(20., 34., 560., 21.),
                12.,
                false,
                true,
            );
            w.setContentView(Some(&root));
            w.center();
            self.preferences = Some(Preferences {
                window: w,
                page: 0,
                panes,
                editor,
                preview,
                catalog_scroll,
                catalog_label,
                catalog_icons: RefCell::new(HashMap::new()),
                layout: 0,
                breadcrumb,
                nav_status,
                gesture_hint,
                selection,
                heading,
                description,
                search,
                scroll,
                summary,
                restore,
                rows: vec![],
                trigger,
                size,
                size_label,
                appearance,
                enabled,
                login,
                permission,
            });
        }
        if !self.catalog_pending {
            self.catalog_pending = true;
            platform::catalog(self.tx.clone());
        }
        self.refresh_preferences_apps();
        self.refresh_preferences_status();
        self.app
            .setActivationPolicy(NSApplicationActivationPolicy::Regular);
        let p = self.preferences.as_ref().unwrap();
        p.window.deminiaturize(None);
        p.window.makeKeyAndOrderFront(None);
        #[allow(deprecated)]
        self.app.activateIgnoringOtherApps(true);
    }
    pub(super) unsafe fn preferences_page(&mut self, page: usize) {
        let Some(p) = self.preferences.as_mut() else {
            return;
        };
        p.page = page.min(2);
        for (i, v) in p.panes.iter().enumerate() {
            v.setHidden(i != p.page);
        }
        let mut f = p.selection.frame();
        f.origin.y =
            p.window.contentView().unwrap().bounds().size.height - 164. - p.page as f64 * 38.;
        p.selection.setFrame(f);
        p.heading.setStringValue(&NSString::from_str(
            ["我的轮盘", "呼出与手势", "通用设置"][p.page],
        ));
        p.description.setStringValue(&NSString::from_str(
            [
                "跟随 Dock，也能放进你的其他常用软件。",
                "一次按住，让动作顺着你的习惯。",
                "轻巧待命，融入你的 Mac。",
            ][p.page],
        ));
        p.breadcrumb.setStringValue(&NSString::from_str(&format!(
            "设置  /  {}",
            ["我的轮盘", "呼出与手势", "通用设置"][p.page]
        )));
        self.refresh_preferences_status();
    }
    pub(super) unsafe fn refresh_preferences_apps(&mut self) {
        let Some(p) = self.preferences.as_ref() else {
            return;
        };
        let query = p.search.stringValue().to_string().trim().to_lowercase();
        let all = self.settings.all_apps();
        let paths: Vec<_> = all
            .iter()
            .filter(|path| {
                self.settings.name(path).to_lowercase().contains(&query)
                    || std::path::Path::new(path)
                        .file_stem()
                        .is_some_and(|s| s.to_string_lossy().to_lowercase().contains(&query))
            })
            .cloned()
            .collect();
        let width = p.scroll.contentSize().width;
        let viewport = p.scroll.contentSize().height;
        let old_y = p.scroll.contentView().bounds().origin.y;
        let document: Retained<AppList> = msg_send![AppList::alloc(MainThreadMarker::new().unwrap()),initWithFrame:rect(r(0.,0.,width,(paths.len() as f64*64.).max(viewport)))];
        document.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
        let mut rows = vec![];
        for (i, path) in paths.into_iter().enumerate() {
            let row: Retained<AppList> = msg_send![AppList::alloc(MainThreadMarker::new().unwrap()),initWithFrame:rect(r(0., i as f64 * 64., width, 64.))];
            row.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
            let name = self.settings.name(&path);
            let checkbox = self.preferences_button(&row, "", r(16., 21., 24., 24.), 0);
            checkbox.setButtonType(NSButtonType::Switch);
            checkbox.setAction(Some(sel!(settingsApp:)));
            checkbox.setAutoresizingMask(NSAutoresizingMaskOptions::empty());
            let _: () = msg_send![&*checkbox,setIdentifier:&*NSString::from_str(&path)];
            let _: () = msg_send![&*checkbox,setAccessibilityLabel:&*NSString::from_str(&name)];
            let icon = NSImageView::new(MainThreadMarker::new().unwrap());
            icon.setFrame(rect(r(52., 14., 36., 36.)));
            icon.setImage(Some(
                &NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(&path)),
            ));
            row.addSubview(&icon);
            let title = text(
                &row,
                &name,
                r(104., 12., width - 290., 23.),
                13.,
                true,
                false,
            );
            title.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
            text(
                &row,
                if self.settings.added_apps.contains(&path) {
                    "手动添加 · 保留在轮盘"
                } else {
                    "来自系统 Dock"
                },
                r(104., 35., width - 290., 18.),
                11.,
                false,
                true,
            );
            let status = text(&row, "", r(width - 188., 23., 86., 20.), 11., false, true);
            status.setAlignment(NSTextAlignment::Right);
            status.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);
            for (delta, title, x) in [(-1, "↑", width - 94.), (1, "↓", width - 66.)] {
                let move_button = self.preferences_button(&row, title, r(x, 19., 26., 26.), delta);
                move_button.setToolTip(Some(&NSString::from_str(if !query.is_empty() {
                    "清空搜索后调整顺序"
                } else if delta < 0 {
                    "上移"
                } else {
                    "下移"
                })));
                move_button.setBordered(false);
                move_button.setAction(Some(sel!(settingsMove:)));
                move_button.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);
                let _: () = msg_send![&*move_button,setIdentifier:&*NSString::from_str(&path)];
                let position = all.iter().position(|p| p == &path).unwrap();
                move_button.setEnabled(
                    query.is_empty()
                        && if delta < 0 {
                            position > 0
                        } else {
                            position + 1 < all.len()
                        },
                );
                let accessible = NSString::from_str(&format!(
                    "{}{}",
                    if delta < 0 { "上移 " } else { "下移 " },
                    name
                ));
                let _: () = msg_send![&*move_button,setAccessibilityLabel:&*accessible];
            }
            if self.settings.added_apps.contains(&path) {
                let remove = self.preferences_button(&row, "−", r(width - 38., 19., 26., 26.), 0);
                remove.setBordered(false);
                remove.setAction(Some(sel!(settingsRemove:)));
                remove.setAutoresizingMask(NSAutoresizingMaskOptions::ViewMinXMargin);
                let _: () = msg_send![&*remove,setIdentifier:&*NSString::from_str(&path)];
                let _: () = msg_send![&*remove,setAccessibilityLabel:&*NSString::from_str(&format!("移除手动添加的 {}",name))];
            }
            let line = NSBox::initWithFrame(
                NSBox::alloc(MainThreadMarker::new().unwrap()),
                rect(r(16., 63., width - 32., 1.)),
            );
            line.setBoxType(NSBoxType::Separator);
            line.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
            row.addSubview(&line);
            document.addSubview(&row);
            rows.push(AppRow {
                path,
                checkbox,
                status,
            });
        }
        if rows.is_empty() {
            text(
                &document,
                if all.is_empty() {
                    "从 Dock 同步，或点击“添加 App”开始"
                } else {
                    "没有匹配的软件"
                },
                r(24., viewport / 2. - 18., width - 48., 25.),
                16.,
                true,
                false,
            )
            .setAlignment(NSTextAlignment::Center);
        }
        let p = self.preferences.as_mut().unwrap();
        p.scroll.setDocumentView(Some(&document));
        p.rows = rows;
        p.scroll.contentView().scrollToPoint(NSPoint::new(
            0.,
            old_y.min((document.frame().size.height - viewport).max(0.)),
        ));
        p.scroll.reflectScrolledClipView(&p.scroll.contentView());
        self.refresh_preferences_status();
        self.refresh_preferences_editor();
    }
    pub(super) unsafe fn preferences_app(&mut self, sender: &NSButton) {
        let path: Option<Retained<NSString>> = msg_send![sender, identifier];
        if let Some(path) = path {
            self.settings
                .set_app_visible(&path.to_string(), sender.state() != 0);
            self.save();
        }
    }
    pub(super) unsafe fn preferences_move(&mut self, sender: &NSButton) {
        let path: Option<Retained<NSString>> = msg_send![sender, identifier];
        if let Some(path) = path {
            self.settings
                .move_application(&path.to_string(), sender.tag());
            self.save();
            self.refresh_preferences_apps();
        }
    }
    pub(super) unsafe fn preferences_remove(&mut self, sender: &NSButton) {
        let path: Option<Retained<NSString>> = msg_send![sender, identifier];
        if let Some(path) = path {
            self.settings.remove_added_application(&path.to_string());
            self.save();
            self.refresh_preferences_apps();
        }
    }
    pub(super) unsafe fn refresh_preferences_status(&self) {
        let Some(p) = self.preferences.as_ref() else {
            return;
        };
        let value = NSString::from_str(&format!(
            "轮盘中 {} 个软件  ·  Dock 共 {} 个",
            self.settings.visible_apps().len(),
            self.settings.wheel.len()
        ));
        if !p.summary.stringValue().isEqualToString(&value) {
            p.summary.setStringValue(&value);
        }
        p.restore.setEnabled(!self.settings.hidden_apps.is_empty());
        p.gesture_hint.setStringValue(&NSString::from_str(&format!(
            "按住 {} · 移动 · 松开",
            self.trigger_label()
        )));
        for row in &p.rows {
            let visible = !self.settings.hidden_apps.contains(&row.path);
            row.checkbox.setState(if visible { 1 } else { 0 });
            row.status.setStringValue(&NSString::from_str(if visible {
                "在轮盘中"
            } else {
                "已隐藏"
            }));
        }
        p.enabled
            .setState(if self.settings.enabled_on_start { 1 } else { 0 });
        p.login
            .setState(if self.settings.launch_at_login { 1 } else { 0 });
        p.size.setDoubleValue(self.settings.wheel_size_px);
        p.size_label.setStringValue(&NSString::from_str(&format!(
            "{} pt",
            self.settings.wheel_size_px as i32
        )));
        p.appearance
            .selectItemAtIndex(match self.settings.appearance.as_str() {
                "light" => 1,
                "dark" => 2,
                _ => 0,
            });
        let trigger = match &self.settings.trigger {
            Trigger::Key { vk: 86 } => 0,
            Trigger::Key { .. } => 1,
            Trigger::Mouse(name) if name == "mouse4" => 2,
            _ => 3,
        };
        p.trigger
            .itemAtIndex(1)
            .unwrap()
            .setTitle(&NSString::from_str(&format!(
                "自定义按键（{}）…",
                self.trigger_label()
            )));
        p.trigger.selectItemAtIndex(trigger);
        self.refresh_preferences_permission();
    }
    pub(super) unsafe fn refresh_preferences_permission(&self) {
        let Some(p) = self.preferences.as_ref() else {
            return;
        };
        if !p.window.isVisible() {
            return;
        }
        let nav = if !self.trusted {
            "等待辅助功能授权"
        } else if !self.settings.enabled_on_start {
            "ViGo 已暂停"
        } else if self.tap.is_null() || !platform::CGEventTapIsEnabled(self.tap) {
            "输入监听待连接"
        } else if self.tap_keyboard != Some(true) {
            "输入监听待确认"
        } else {
            "输入助手已就绪"
        };
        let nav = NSString::from_str(nav);
        if !p.nav_status.stringValue().isEqualToString(&nav) {
            p.nav_status.setStringValue(&nav);
        }
        let state = if !self.trusted {
            "需要辅助功能授权，开启后自动生效。"
        } else if platform::secure_input_enabled() {
            "系统安全键盘输入已开启，快捷键暂不可用。"
        } else if self.tap.is_null() || !platform::CGEventTapIsEnabled(self.tap) {
            "辅助功能已授权，输入监听尚未连接。"
        } else {
            "辅助功能已开启 · 输入监听已连接"
        };
        let state = NSString::from_str(state);
        if !p.permission.stringValue().isEqualToString(&state) {
            p.permission.setStringValue(&state);
        }
    }
    pub(super) unsafe fn close_preferences_or_capture(&mut self, w: &NSWindow) -> bool {
        w.orderOut(None);
        if self
            .preferences
            .as_ref()
            .is_some_and(|p| std::ptr::eq(&*p.window, w))
        {
            self.capturing = false;
            self.dismiss_capture();
            self.app
                .setActivationPolicy(NSApplicationActivationPolicy::Accessory);
            let _: () = msg_send![&*self.delegate,performSelector:sel!(releasePreferences:),withObject:None::<&AnyObject>,afterDelay:0.0f64];
            true
        } else {
            self.capturing = false;
            false
        }
    }
}
