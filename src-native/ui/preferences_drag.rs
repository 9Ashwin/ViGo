//! AppKit drag source for the installed-app gallery. Clicks keep their original action.
use super::*;

define_class!(#[unsafe(super(NSButton))]#[thread_kind=MainThreadOnly]
 pub(super) struct CatalogButton;
 unsafe impl NSObjectProtocol for CatalogButton{}
 unsafe impl NSDraggingSource for CatalogButton{
  #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
  fn operation(&self,_session:&NSDraggingSession,context:NSDraggingContext)->NSDragOperation{if context==NSDraggingContext::WithinApplication{NSDragOperation::Copy}else{NSDragOperation::None}}
  #[unsafe(method(draggingSession:endedAtPoint:operation:))]
  fn ended(&self,_session:&NSDraggingSession,_point:NSPoint,_operation:NSDragOperation){unsafe{let _keep=Retained::retain(self as *const Self as *mut Self);with(|s|{s.catalog_drag_source=None;s.preferences_interacting=false;s.refresh_preferences_apps();});}}
 }
 impl CatalogButton{
  #[unsafe(method(hitTest:))]fn hit(&self,point:NSPoint)->*mut NSView{unsafe{let hit:*mut NSView=msg_send![super(self),hitTest:point];if hit.is_null(){ptr::null_mut()}else{self as *const Self as *mut NSView}}}
  #[unsafe(method(acceptsFirstMouse:))]fn first_mouse(&self,_event:Option<&NSEvent>)->bool{true}
  #[unsafe(method(mouseDown:))]fn down(&self,event:&NSEvent){unsafe{self.track(event);}}
 }
);
impl CatalogButton {
    pub(super) unsafe fn new(frame: Rect) -> Retained<Self> {
        msg_send![Self::alloc(MainThreadMarker::new().unwrap()),initWithFrame:rect(frame)]
    }
    unsafe fn track(&self, down: &NSEvent) {
        if !self.isEnabled() {
            return;
        }
        let _keep = Retained::retain(self as *const Self as *mut Self);
        with(|s| {
            s.close_wheel();
            s.gesture.reset();
            s.preferences_interacting = true;
        });
        self.highlight(true);
        let app = NSApplication::sharedApplication(MainThreadMarker::new().unwrap());
        let until = NSDate::distantFuture();
        while let Some(event) = app.nextEventMatchingMask_untilDate_inMode_dequeue(
            NSEventMask::LeftMouseDragged | NSEventMask::LeftMouseUp | NSEventMask::KeyDown,
            Some(&until),
            NSEventTrackingRunLoopMode,
            true,
        ) {
            if event.r#type() == NSEventType::KeyDown {
                if event.keyCode() == 53 {
                    break;
                }
                app.sendEvent(&event);
                if self.window().is_none() {
                    break;
                }
                continue;
            }
            if event.r#type() == NSEventType::LeftMouseUp {
                self.highlight(false);
                let point = self.convertPoint_fromView(event.locationInWindow(), None);
                if NSPointInRect(point, self.bounds()) {
                    self.performClick(None);
                }
                with(|s| {
                    s.preferences_interacting = false;
                    s.refresh_preferences_apps();
                });
                return;
            }
            let p = event.locationInWindow();
            let start = down.locationInWindow();
            if (p.x - start.x).hypot(p.y - start.y) < 4. {
                continue;
            }
            self.highlight(false);
            let path: Option<Retained<NSString>> = msg_send![self, identifier];
            let Some(path) = path else { break };
            let url = NSURL::fileURLWithPath(&path);
            let item = NSPasteboardItem::new();
            let Some(value) = url.absoluteString() else {
                break;
            };
            item.setString_forType(&value, NSPasteboardTypeFileURL);
            let drag = NSDraggingItem::initWithPasteboardWriter(
                NSDraggingItem::alloc(),
                ProtocolObject::from_ref(&*item),
            );
            let point = self.convertPoint_fromView(down.locationInWindow(), None);
            if let Some(image) = self.image() {
                drag.setDraggingFrame_contents(
                    rect(r(point.x - 22., point.y - 22., 44., 44.)),
                    Some(&image),
                );
            }
            with(|s| {
                s.catalog_drag_source = Retained::retain(self as *const Self as *mut Self);
            });
            let session = self.beginDraggingSessionWithItems_event_source(
                &NSArray::from_retained_slice(&[drag]),
                down,
                ProtocolObject::from_ref(self),
            );
            session.setAnimatesToStartingPositionsOnCancelOrFail(true);
            return;
        }
        self.highlight(false);
        with(|s| {
            s.preferences_interacting = false;
            s.refresh_preferences_apps();
        });
    }
}

pub(super) fn application_paths(board: &NSPasteboard, validate: bool) -> Vec<String> {
    let mut paths = vec![];
    if let Some(items) = board.pasteboardItems() {
        for item in &items {
            let Some(value) = item.stringForType(unsafe { NSPasteboardTypeFileURL }) else {
                continue;
            };
            let Some(url) = NSURL::URLWithString(&value) else {
                continue;
            };
            if !url.isFileURL() {
                continue;
            }
            let Some(path) = url.path() else { continue };
            let path = std::path::PathBuf::from(path.to_string());
            if !path
                .extension()
                .is_some_and(|p| p.eq_ignore_ascii_case("app"))
            {
                continue;
            }
            if validate {
                let Ok(path) = path.canonicalize() else {
                    continue;
                };
                let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
                if NSBundle::bundleWithURL(&url)
                    .and_then(|b| b.executableURL())
                    .is_none()
                {
                    continue;
                }
                paths.push(path.to_string_lossy().into_owned());
            } else {
                paths.push(path.to_string_lossy().into_owned());
            }
        }
    }
    paths
}
impl State {
    pub(super) unsafe fn preferences_drop(
        &mut self,
        paths: &[String],
        before: Option<&str>,
    ) -> bool {
        if self
            .preferences
            .as_ref()
            .is_none_or(|p| !p.window.isVisible())
        {
            return false;
        }
        for path in paths {
            self.settings.drop_application(path, before);
        }
        self.save();
        self.refresh_preferences_apps();
        true
    }
}
