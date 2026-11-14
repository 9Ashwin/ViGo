use std::f64::consts::{PI, TAU};
#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}
#[derive(Clone, Debug)]
pub struct DockNode {
    pub radius: f64,
    pub angle: f64,
    pub ring: usize,
    pub offset: usize,
    pub count: usize,
}
#[derive(Clone, Debug)]
pub struct DockLayout {
    pub nodes: Vec<DockNode>,
    pub icon: f64,
    pub hub: f64,
    pub outer: f64,
    pub surface: f64,
}
impl DockLayout {
    pub fn new(count: usize, base: f64, width: f64, height: f64) -> Self {
        let preferred = (base * 0.088).clamp(36., 54.);
        let hub = (base * 0.135).clamp(56., 84.);
        if count == 0 {
            return Self {
                nodes: vec![],
                icon: preferred,
                hub,
                outer: hub + 50.,
                surface: hub + 50.,
            };
        }
        // The main glass disc stays compact. Only the first eight Dock items live inside it.
        let surface = (base * 188. / 600.)
            .min(width.min(height) / 2. - 20.)
            .max(hub + 50.);
        let inner_count = count.min(8);
        let inner_radius = (base * 0.23).min(surface - preferred / 2. - 12.);
        let mut icon = preferred;
        let mut rings = vec![(inner_count, inner_radius)];
        let mut remaining = count - inner_count;
        let mut radius = surface + preferred / 2. + 18.;
        let maximum = width.min(height) / 2. - preferred / 2. - 16.;
        while remaining > 0 && radius <= maximum {
            let capacity = (TAU * radius / (preferred * 1.35)).floor() as usize;
            let n = remaining.min(capacity.max(1));
            rings.push((n, radius));
            remaining -= n;
            radius += preferred + 18.;
        }
        if remaining > 0 {
            // Dense Docks keep every item visible by fitting overflow rings, never enlarging the glass.
            rings.truncate(1);
            let available = (maximum - surface).max(20.);
            let overflow = count - inner_count;
            let layers = (available / (preferred + 14.)).floor().max(1.) as usize;
            let capacity_radius = (1..=layers)
                .map(|i| surface + available * i as f64 / layers as f64)
                .sum::<f64>();
            icon = preferred
                .min(TAU * capacity_radius / (overflow as f64 * 1.25))
                .max(1.);
            let mut left = overflow;
            for i in 1..=layers {
                let r = surface + available * i as f64 / layers as f64;
                let n = if i == layers {
                    left
                } else {
                    ((overflow as f64 * r / capacity_radius).floor() as usize).min(left)
                };
                rings.push((n, r));
                left -= n;
            }
        }
        let mut nodes = vec![];
        let mut outer = surface;
        for (ring, (n, r)) in rings.into_iter().enumerate() {
            if n == 0 {
                continue;
            }
            outer = outer.max(r + icon / 2. + 12.);
            for offset in 0..n {
                nodes.push(DockNode {
                    radius: r,
                    angle: -PI / 2. + offset as f64 * TAU / n as f64,
                    ring,
                    offset,
                    count: n,
                });
            }
        }
        Self {
            nodes,
            icon,
            hub,
            outer,
            surface,
        }
    }
    pub fn hit(&self, x: f64, y: f64, previous: Option<usize>) -> Option<usize> {
        let radius = x.hypot(y);
        if radius < self.hub || radius > self.outer + 70. {
            return None;
        }
        let node = self.nodes.iter().min_by(|a, b| {
            (radius - a.radius)
                .abs()
                .total_cmp(&(radius - b.radius).abs())
        })?;
        let angle = y.atan2(x);
        let offset = (((angle + PI / 2. + TAU + PI / node.count as f64) % TAU) / TAU
            * node.count as f64)
            .floor() as usize
            % node.count;
        let candidate = self
            .nodes
            .iter()
            .position(|n| n.ring == node.ring && n.offset == offset)?;
        if let Some(old) = previous.and_then(|i| self.nodes.get(i).map(|n| (i, n))) {
            if old.1.ring != node.ring
                && (radius - old.1.radius).abs() < (radius - node.radius).abs() + 5.
            {
                return Some(old.0);
            }
            let d = (angle - old.1.angle)
                .sin()
                .atan2((angle - old.1.angle).cos())
                .abs();
            if old.1.ring == node.ring
                && d <= PI / node.count as f64 + (PI / 60.).min(PI / node.count as f64 * 0.12)
            {
                return Some(old.0);
            }
        }
        Some(candidate)
    }
    pub fn center(&self, anchor: (f64, f64), width: f64, height: f64) -> (f64, f64) {
        let margin = self.outer + 8.;
        (
            anchor.0.clamp(margin, width - margin),
            anchor.1.clamp(margin, height - margin),
        )
    }
}
#[cfg(test)]
mod dock_layout_tests {
    use super::*;
    #[test]
    fn every_dock_item_is_present_and_hittable() {
        for count in [1, 8, 18, 30, 60, 100, 180] {
            let l = DockLayout::new(count, 520., 1470., 956.);
            assert_eq!(l.nodes.len(), count);
            assert!(l.outer < 478.);
            for (i, n) in l.nodes.iter().enumerate() {
                assert_eq!(
                    l.hit(n.angle.cos() * n.radius, n.angle.sin() * n.radius, None),
                    Some(i)
                );
            }
        }
    }
    #[test]
    fn dense_docks_keep_multiple_rings_and_spacing() {
        for (count, w, h) in [(500, 1024., 768.), (180, 640., 480.)] {
            let l = DockLayout::new(count, 640., w, h);
            assert_eq!(l.nodes.len(), count);
            assert!(l.nodes.last().unwrap().ring > 0);
            for (i, a) in l.nodes.iter().enumerate() {
                for b in l.nodes.iter().skip(i + 1) {
                    let d = (a.radius * a.angle.cos() - b.radius * b.angle.cos())
                        .hypot(a.radius * a.angle.sin() - b.radius * b.angle.sin());
                    assert!(d >= l.icon, "overlapping dense Dock icons");
                }
            }
        }
    }
    #[test]
    fn edge_layout_keeps_all_icons_visible() {
        let l = DockLayout::new(30, 520., 1470., 956.);
        for anchor in [(5., 450.), (1465., 950.), (5., 5.)] {
            let (c, d) = l.center(anchor, 1470., 956.);
            for n in &l.nodes {
                let x = c + n.radius * n.angle.cos();
                let y = d + n.radius * n.angle.sin();
                assert!(
                    x - l.icon / 2. >= 0.
                        && x + l.icon / 2. <= 1470.
                        && y - l.icon / 2. >= 0.
                        && y + l.icon / 2. <= 956.
                );
            }
        }
    }
}
