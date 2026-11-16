//! Native editor preview: drag apps between directions; runtime selection stays independent.
use super::*;
use objc2::DefinedClass;
use std::cell::Cell;
#[derive(Debug)]
pub(super) struct PreviewData {
    paths: RefCell<Vec<String>>,
    icons: RefCell<Vec<Retained<NSImage>>>,
    sheet: Retained<NSImage>,
    drag: Cell<Option<usize>>,
    drag_path: RefCell<Option<String>>,
    pointer: Cell<NSPoint>,
}
define_class!(#[unsafe(super(NSView))]#[thread_kind=MainThreadOnly]#[ivars=PreviewData]
 pub(super) struct WheelPreview;
 impl WheelPreview{
  #[unsafe(method(drawRect:))]fn draw(&self,_r:NSRect){unsafe{self.paint();}}
  #[unsafe(method(mouseDown:))]fn down(&self,event:&NSEvent){let point=self.convertPoint_fromView(event.locationInWindow(),None);let index=self.hit(point);self.ivars().drag.set(index);*self.ivars().drag_path.borrow_mut()=index.and_then(|i|self.ivars().paths.borrow().get(i).cloned());self.ivars().pointer.set(point);self.setNeedsDisplay(true);}
  #[unsafe(method(mouseDragged:))]fn moved(&self,event:&NSEvent){self.ivars().pointer.set(self.convertPoint_fromView(event.locationInWindow(),None));self.setNeedsDisplay(true);}
  #[unsafe(method(mouseUp:))]fn up(&self,event:&NSEvent){let target=self.hit(self.convertPoint_fromView(event.locationInWindow(),None)).and_then(|i|self.ivars().paths.borrow().get(i).cloned());let source=self.ivars().drag_path.borrow_mut().take();self.ivars().drag.set(None);if let(Some(a),Some(b))=(source,target){with(|s|unsafe{s.settings.swap_applications(&a,&b);s.save();s.refresh_preferences_apps();});}self.setNeedsDisplay(true);}

 }
);
impl WheelPreview {
    pub(super) unsafe fn new(frame: Rect, sheet: Retained<NSImage>) -> Retained<Self> {
        let obj = Self::alloc(MainThreadMarker::new().unwrap()).set_ivars(PreviewData {
            paths: RefCell::new(vec![]),
            icons: RefCell::new(vec![]),
            sheet,
            drag: Cell::new(None),
            drag_path: RefCell::new(None),
            pointer: Cell::new(NSPoint::ZERO),
        });
        msg_send![super(obj),initWithFrame:rect(frame)]
    }
    pub(super) fn update(&self, paths: Vec<String>, icons: Vec<Retained<NSImage>>) {
        if let Some(path) = self.ivars().drag_path.borrow().as_ref() {
            self.ivars().drag.set(paths.iter().position(|p| p == path));
        }
        *self.ivars().paths.borrow_mut() = paths;
        *self.ivars().icons.borrow_mut() = icons;
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
        self.ivars()
            .sheet
            .drawInRect_fromRect_operation_fraction_respectFlipped_hints(
                rect(r(cx - 30., cy - 30., 60., 60.)),
                rect(r(40., 2288. - 292. - 112., 112., 112.)),
                NSCompositingOperation::SourceOver,
                1.,
                true,
                None,
            );
        let icons = self.ivars().icons.borrow();
        let centers = self.centers();
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
