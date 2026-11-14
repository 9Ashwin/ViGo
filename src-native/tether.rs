//! Presentation-only spring. The real app identity and release decision never depend on this tip.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn distance(self, other: Self) -> f64 {
        (self.x - other.x).hypot(self.y - other.y)
    }
}
#[derive(Default, Debug)]
struct Spring {
    value: f64,
    velocity: f64,
}
impl Spring {
    fn set(&mut self, value: f64) {
        self.value = value;
        self.velocity = 0.;
    }
    fn step(&mut self, target: f64, dt: f64, speed: f64) {
        let delta = self.value - target;
        let impulse = (self.velocity + speed * delta) * dt;
        let decay = (-speed * dt).exp();
        self.value = target + (delta + impulse) * decay;
        self.velocity = (self.velocity - speed * impulse) * decay;
    }
}
#[derive(Default, Debug)]
pub struct Tether {
    x: Spring,
    y: Spring,
    curve: Spring,
    routing: Spring,
    slack: Spring,
    opacity: Spring,
    last_time: f64,
    last_pointer: Point,
    pub snapped: bool,
    last_center: Option<Point>,
}
#[derive(Clone, Copy, Debug)]
pub struct Target {
    pub center: Point,
    pub half_icon: f64,
    pub detour: f64,
}
#[derive(Clone, Copy)]
pub struct Frame {
    pub now: f64,
    pub origin: Point,
    pub pointer: Point,
    pub target: Option<Target>,
    pub max_length: f64,
    pub enabled: bool,
    pub reduced: bool,
}
impl Tether {
    pub fn reset(&mut self, origin: Point, now: f64) {
        self.x.set(origin.x);
        self.y.set(origin.y);
        self.curve.set(0.);
        self.routing.set(0.);
        self.slack.set(0.);
        self.opacity.set(0.);
        self.last_pointer = origin;
        self.last_time = now;
        self.snapped = false;
        self.last_center = None;
    }
    pub fn tip(&self) -> Point {
        Point::new(self.x.value, self.y.value)
    }
    pub fn bend(&self) -> f64 {
        (self.curve.value + self.routing.value).clamp(-96., 96.)
    }
    pub fn sag(&self) -> f64 {
        self.slack.value.clamp(0., 12.)
    }
    pub fn alpha(&self) -> f64 {
        self.opacity.value.clamp(0., 1.)
    }
    pub fn advance(&mut self, frame: Frame) -> bool {
        let Frame {
            now,
            origin,
            pointer,
            target,
            max_length,
            enabled,
            reduced,
        } = frame;
        let before = (
            self.tip(),
            self.bend(),
            self.alpha(),
            self.snapped,
            self.sag(),
        );
        let dt = (now - self.last_time).clamp(0., 0.05);
        self.last_time = now;
        let distance = origin.distance(pointer);
        let show = enabled && distance > 45.;
        let mut goal = origin;
        let was_snapped = self.snapped;
        self.snapped = false;
        if show {
            goal = pointer;
            if let Some(target) = target {
                let mut capture = (target.half_icon * 1.65 + 6.).clamp(24., 48.);
                if was_snapped
                    && self
                        .last_center
                        .is_some_and(|point| point.distance(target.center) < target.half_icon + 3.)
                {
                    capture += 10.;
                }
                if pointer.distance(target.center) <= capture {
                    self.snapped = true;
                    let d = origin.distance(target.center).max(1.);
                    let inset = target.half_icon + 3.;
                    goal = Point::new(
                        target.center.x - (target.center.x - origin.x) / d * inset,
                        target.center.y - (target.center.y - origin.y) / d * inset,
                    );
                }
            }
            let d = origin.distance(goal);
            if d > max_length && d > 0. {
                goal = Point::new(
                    origin.x + (goal.x - origin.x) / d * max_length,
                    origin.y + (goal.y - origin.y) / d * max_length,
                );
            }
        }
        self.last_center = target.map(|target| target.center);
        let desired_alpha = if show {
            ((distance - 45.) / 22.).clamp(0., 1.)
        } else {
            0.
        };
        let tangent = if show && distance > 0. {
            ((pointer.x - self.last_pointer.x) * (-(pointer.y - origin.y) / distance)
                + (pointer.y - self.last_pointer.y) * (pointer.x - origin.x) / distance)
                .clamp(-24., 24.)
                * 0.7
        } else {
            0.
        };
        self.last_pointer = pointer;
        let detour = if show {
            target.map(|target| target.detour).unwrap_or(0.)
        } else {
            0.
        };
        let slack = if show && !self.snapped {
            ((115. - distance) / 70.).clamp(0., 1.) * 12.
        } else {
            0.
        };
        if reduced {
            self.x.set(goal.x);
            self.y.set(goal.y);
            self.curve.set(0.);
            self.routing.set(detour);
            self.slack.set(0.);
            self.opacity.set(desired_alpha);
        } else {
            self.x.step(goal.x, dt, 48.);
            self.y.step(goal.y, dt, 48.);
            self.curve.step(tangent, dt, 32.);
            self.routing.step(detour, dt, 32.);
            self.slack.step(slack, dt, 32.);
            self.opacity.step(desired_alpha, dt, 36.);
        }
        before.0.distance(self.tip()) > 0.04
            || (before.1 - self.bend()).abs() > 0.02
            || (before.2 - self.alpha()).abs() > 0.002
            || before.3 != self.snapped
            || (before.4 - self.sag()).abs() > 0.02
    }
}
/// Keep an outer link in the open gap beside an intervening inner icon.
pub fn route_around(
    origin: Point,
    end: Point,
    obstacles: impl IntoIterator<Item = (Point, f64)>,
) -> f64 {
    let distance = origin.distance(end);
    if distance < 1. {
        return 0.;
    }
    let ux = (end.x - origin.x) / distance;
    let uy = (end.y - origin.y) / distance;
    let mut chosen = 0f64;
    for (center, radius) in obstacles {
        let along = (center.x - origin.x) * ux + (center.y - origin.y) * uy;
        let t = along / distance;
        if !(0.12..0.88).contains(&t) {
            continue;
        }
        let across = (center.x - origin.x) * (-uy) + (center.y - origin.y) * ux;
        let clearance = radius + 9.;
        if across.abs() < clearance {
            let needed = ((clearance - across.abs() + 3.) / (2. * t * (1. - t))).min(84.);
            let bend = if across > 0. { -needed } else { needed };
            if bend.abs() > chosen.abs() {
                chosen = bend;
            }
        }
    }
    chosen
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn outer_links_leave_clearance_for_the_inner_icon() {
        let origin = Point::new(0., 0.);
        let end = Point::new(200., 0.);
        let obstacle = Point::new(120., 0.);
        let bend = route_around(origin, end, [(obstacle, 24.)]);
        let min = (0..=100)
            .map(|i| {
                let t = i as f64 / 100.;
                Point::new(200. * t, 2. * t * (1. - t) * bend).distance(obstacle)
            })
            .fold(f64::INFINITY, f64::min);
        assert!(min > 30.);
        assert_eq!(
            route_around(origin, end, [(Point::new(120., 70.), 24.)]),
            0.
        );
    }
    #[test]
    fn cursor_stretches_then_snaps_to_the_icon_edge() {
        let origin = Point::new(0., 0.);
        let target = Target {
            center: Point::new(120., 0.),
            half_icon: 23.,
            detour: 0.,
        };
        let mut t = Tether::default();
        t.reset(origin, 0.);
        for i in 1..40 {
            t.advance(Frame {
                now: i as f64 / 60.,
                origin,
                pointer: Point::new(65., 0.),
                target: Some(target),
                max_length: 250.,
                enabled: true,
                reduced: false,
            });
        }
        assert!(!t.snapped);
        assert!((t.tip().x - 65.).abs() < 0.1);
        assert!(t.sag() > 5.);
        for i in 40..80 {
            t.advance(Frame {
                now: i as f64 / 60.,
                origin,
                pointer: Point::new(112., 0.),
                target: Some(target),
                max_length: 250.,
                enabled: true,
                reduced: false,
            });
        }
        assert!(t.snapped);
        assert!((t.tip().x - 94.).abs() < 0.1);
        assert!(t.alpha() > 0.99);
    }
    #[test]
    fn captured_targets_do_not_flicker_at_the_boundary_or_capture_a_far_neighbor() {
        let mut tether = Tether::default();
        let origin = Point::new(0., 0.);
        tether.reset(origin, 0.);
        let target = Target {
            center: Point::new(120., 0.),
            half_icon: 23.,
            detour: 0.,
        };
        let mut frame = Frame {
            now: 0.1,
            origin,
            pointer: Point::new(110., 0.),
            target: Some(target),
            max_length: 250.,
            enabled: true,
            reduced: true,
        };
        tether.advance(frame);
        assert!(tether.snapped);
        frame = Frame {
            now: 0.2,
            origin,
            pointer: Point::new(70., 0.),
            target: Some(target),
            max_length: 250.,
            enabled: true,
            reduced: true,
        };
        tether.advance(frame);
        assert!(tether.snapped);
        frame.target = Some(Target {
            center: Point::new(240., 0.),
            half_icon: 23.,
            detour: 0.,
        });
        frame.pointer = Point::new(190., 0.);
        frame.now = 0.3;
        tether.advance(frame);
        assert!(!tether.snapped);
    }
    #[test]
    fn cancel_retracts_and_reopen_drops_the_old_tip() {
        let mut t = Tether::default();
        let p = Point::new(0., 0.);
        t.reset(p, 0.);
        for i in 1..40 {
            t.advance(Frame {
                now: i as f64 / 60.,
                origin: p,
                pointer: Point::new(180., 70.),
                target: None,
                max_length: 250.,
                enabled: true,
                reduced: false,
            });
        }
        for i in 40..90 {
            t.advance(Frame {
                now: i as f64 / 60.,
                origin: p,
                pointer: p,
                target: None,
                max_length: 250.,
                enabled: false,
                reduced: false,
            });
        }
        assert!(t.alpha() < 0.001 && t.tip().distance(p) < 0.01 && !t.snapped);
        let next = Point::new(900., 500.);
        t.reset(next, 3.);
        assert_eq!(t.tip(), next);
        assert_eq!(t.alpha(), 0.);
    }
    #[test]
    fn reduced_motion_is_immediate_and_extreme_input_is_bounded() {
        let mut t = Tether::default();
        let p = Point::new(10., 10.);
        t.reset(p, 0.);
        t.advance(Frame {
            now: 100.,
            origin: p,
            pointer: Point::new(100000., -100000.),
            target: None,
            max_length: 220.,
            enabled: true,
            reduced: true,
        });
        assert!((t.tip().distance(p) - 220.).abs() < 0.001);
        assert_eq!(t.bend(), 0.);
        assert_eq!(t.sag(), 0.);
        t.advance(Frame {
            now: 101.,
            origin: p,
            pointer: p,
            target: None,
            max_length: 220.,
            enabled: true,
            reduced: true,
        });
        assert_eq!(t.tip(), p);
        assert_eq!(t.alpha(), 0.);
    }
}
