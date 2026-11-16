//! Native editor preview: drag apps between directions; runtime selection stays independent.
use super::*;
use objc2::DefinedClass;
use std::cell::Cell;
#[derive(Debug)]
pub(super) struct PreviewData {
    paths: RefCell<Vec<String>>,
    icons: RefCell<Vec<Retained<NSImage>>>,
    mascot: RefCell<std::rc::Rc<super::mascot::MascotImage>>,
    drag: Cell<Option<usize>>,
    drag_path: RefCell<Option<String>>,
    pointer: Cell<NSPoint>,
    external_drop: Cell<bool>,
}
define_class!(#[unsafe(super(NSView))]#[thread_kind=MainThreadOnly]#[ivars=PreviewData]
 pub(super) struct WheelPreview;
 unsafe impl NSObjectProtocol for WheelPreview{}
 unsafe impl NSDraggingDestination for WheelPreview{
  #[unsafe(method(draggingEntered:))]fn entered(&self,sender:&ProtocolObject<dyn NSDraggingInfo>)->NSDragOperation{unsafe{self.drop_feedback(sender)}}
  #[unsafe(method(draggingUpdated:))]fn updated(&self,sender:&ProtocolObject<dyn NSDraggingInfo>)->NSDragOperation{unsafe{self.drop_feedback(sender)}}
  #[unsafe(method(draggingExited:))]fn exited(&self,_sender:Option<&ProtocolObject<dyn NSDraggingInfo>>){self.ivars().external_drop.set(false);self.setNeedsDisplay(true);}
  #[unsafe(method(prepareForDragOperation:))]fn prepare(&self,sender:&ProtocolObject<dyn NSDraggingInfo>)->bool{!super::preferences_drag::application_paths(&sender.draggingPasteboard(),false).is_empty()}
  #[unsafe(method(performDragOperation:))]fn perform_drop(&self,sender:&ProtocolObject<dyn NSDraggingInfo>)->bool{unsafe{
    let paths=super::preferences_drag::application_paths(&sender.draggingPasteboard(),true);
    let point=self.convertPoint_fromView(sender.draggingLocation(),None);
    let before=self.drop_hit(point).and_then(|i|self.ivars().paths.borrow().get(i).cloned());
    self.ivars().external_drop.set(false);self.setNeedsDisplay(true);
    let mut accepted=false;
    if !paths.is_empty(){with(|s|accepted=s.preferences_drop(&paths,before.as_deref()));}accepted
  }}
  #[unsafe(method(draggingEnded:))]fn drag_ended(&self,_sender:&ProtocolObject<dyn NSDraggingInfo>){self.ivars().external_drop.set(false);self.setNeedsDisplay(true);}
 }
 impl WheelPreview{
  #[unsafe(method(drawRect:))]fn draw(&self,_r:NSRect){unsafe{self.paint();}}
  #[unsafe(method(mouseDown:))]fn down(&self,event:&NSEvent){let point=self.convertPoint_fromView(event.locationInWindow(),None);let index=self.hit(point);self.ivars().drag.set(index);*self.ivars().drag_path.borrow_mut()=index.and_then(|i|self.ivars().paths.borrow().get(i).cloned());self.ivars().pointer.set(point);self.setNeedsDisplay(true);}
  #[unsafe(method(mouseDragged:))]fn moved(&self,event:&NSEvent){self.ivars().pointer.set(self.convertPoint_fromView(event.locationInWindow(),None));self.setNeedsDisplay(true);}
  #[unsafe(method(mouseUp:))]fn up(&self,event:&NSEvent){let target=self.hit(self.convertPoint_fromView(event.locationInWindow(),None)).and_then(|i|self.ivars().paths.borrow().get(i).cloned());let source=self.ivars().drag_path.borrow_mut().take();self.ivars().drag.set(None);if let(Some(a),Some(b))=(source,target){with(|s|unsafe{s.settings.swap_applications(&a,&b);s.save();s.refresh_preferences_apps();});}self.setNeedsDisplay(true);}

 }
);
impl WheelPreview {
    pub(super) unsafe fn new(
        frame: Rect,
        mascot: std::rc::Rc<super::mascot::MascotImage>,
    ) -> Retained<Self> {
        let obj = Self::alloc(MainThreadMarker::new().unwrap()).set_ivars(PreviewData {
            paths: RefCell::new(vec![]),
            icons: RefCell::new(vec![]),
            mascot: RefCell::new(mascot),
            drag: Cell::new(None),
            drag_path: RefCell::new(None),
            pointer: Cell::new(NSPoint::ZERO),
            external_drop: Cell::new(false),
        });
        let view: Retained<Self> = msg_send![super(obj),initWithFrame:rect(frame)];
        view.registerForDraggedTypes(&NSArray::from_slice(&[NSPasteboardTypeFileURL]));
        view
    }
    pub(super) fn update(&self, paths: Vec<String>, icons: Vec<Retained<NSImage>>) {
        if let Some(path) = self.ivars().drag_path.borrow().as_ref() {
            self.ivars().drag.set(paths.iter().position(|p| p == path));
        }
        *self.ivars().paths.borrow_mut() = paths;
        *self.ivars().icons.borrow_mut() = icons;
        self.setNeedsDisplay(true);
    }
    pub(super) fn set_mascot(&self, mascot: std::rc::Rc<super::mascot::MascotImage>) {
        *self.ivars().mascot.borrow_mut() = mascot;
        self.setNeedsDisplay(true);
    }
    fn nodes(&self) -> (layout::DockLayout, f64) {
        let bounds = self.bounds();
        let layout = layout::DockLayout::new(self.ivars().paths.borrow().len(), 520., 800., 800.);
        let scale = (bounds.size.width.min(bounds.size.height) / 2. - 12.) / layout.outer;
        (layout, scale)
    }
    pub(super) fn centers(&self) -> Vec<(f64, f64)> {
        let (layout, scale) = self.nodes();
        let size = self.bounds().size;
        layout
            .nodes
            .iter()
            .map(|n| {
                (
                    size.width / 2. + n.angle.cos() * n.radius * scale,
                    size.height / 2. - n.angle.sin() * n.radius * scale,
                )
            })
            .collect()
    }
    fn hit(&self, point: NSPoint) -> Option<usize> {
        let (layout, scale) = self.nodes();
        self.centers()
            .iter()
            .position(|(x, y)| (point.x - x).hypot(point.y - y) < (layout.icon * scale / 2. + 9.))
    }
    fn drop_hit(&self, point: NSPoint) -> Option<usize> {
        let (layout, scale) = self.nodes();
        let size = self.bounds().size;
        layout.hit(
            (point.x - size.width / 2.) / scale,
            (size.height / 2. - point.y) / scale,
            None,
        )
    }
    unsafe fn drop_feedback(&self, sender: &ProtocolObject<dyn NSDraggingInfo>) -> NSDragOperation {
        let valid =
            !super::preferences_drag::application_paths(&sender.draggingPasteboard(), false)
                .is_empty();
        self.ivars().external_drop.set(valid);
        self.ivars()
            .pointer
            .set(self.convertPoint_fromView(sender.draggingLocation(), None));
        self.setNeedsDisplay(true);
        if valid {
            NSDragOperation::Copy
        } else {
            NSDragOperation::None
        }
    }
    unsafe fn paint(&self) {
        let size = self.bounds().size;
        let cx = size.width / 2.;
        let cy = size.height / 2.;
        let (layout, scale) = self.nodes();
        let backdrop = NSBezierPath::bezierPathWithOvalInRect(rect(r(
            10.,
            10.,
            size.width - 20.,
            size.height - 20.,
        )));
        fill(&backdrop, &color("windowBackgroundColor"));
        color("separatorColor").setStroke();
        backdrop.setLineWidth(0.6);
        backdrop.stroke();
        let hub = NSBezierPath::bezierPathWithOvalInRect(rect(r(cx - 42., cy - 42., 84., 84.)));
        fill(&hub, &color("controlBackgroundColor"));
        self.ivars().mascot.borrow().draw_neutral(cx, cy, 60.);
        let icons = self.ivars().icons.borrow();
        let centers = self.centers();
        if self.ivars().external_drop.get() {
            let ring = if let Some(i) = self.drop_hit(self.ivars().pointer.get()) {
                let (x, y) = centers[i];
                let side = (layout.icon * scale + 16.).max(38.);
                NSBezierPath::bezierPathWithOvalInRect(rect(r(
                    x - side / 2.,
                    y - side / 2.,
                    side,
                    side,
                )))
            } else {
                NSBezierPath::bezierPathWithOvalInRect(rect(r(
                    4.,
                    4.,
                    size.width - 8.,
                    size.height - 8.,
                )))
            };
            color("controlAccentColor").setStroke();
            ring.setLineWidth(2.);
            ring.stroke();
        }
        for (i, (x, y)) in centers.iter().enumerate() {
            if let Some(icon) = icons.get(i) {
                let side = layout.icon * scale;
                icon.drawInRect_fromRect_operation_fraction_respectFlipped_hints(
                    rect(r(x - side / 2., y - side / 2., side, side)),
                    NSRect::ZERO,
                    NSCompositingOperation::SourceOver,
                    if self.ivars().drag.get() == Some(i) {
                        0.3
                    } else {
                        1.
                    },
                    true,
                    None,
                );
            }
        }
        if let Some(i) = self.ivars().drag.get() {
            if let Some(icon) = icons.get(i) {
                let point = self.ivars().pointer.get();
                let side = (layout.icon * scale).max(32.);
                image(
                    icon,
                    r(point.x - side / 2., point.y - side / 2., side, side),
                );
            }
        }
        if icons.is_empty() {
            center_text(
                "从右侧选择软件",
                r(20., 24., size.width - 40., 24.),
                12.,
                false,
                &color("secondaryLabelColor"),
            );
        }
    }
}
