use std::f64::consts::{PI, TAU};
#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}
impl Rect {
    pub fn has(&self, x: f64, y: f64) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}
pub fn slot(dx: f64, dy: f64, count: usize, size: f64) -> Option<usize> {
    if count == 0 || dx.hypot(dy) < size * 86. / 600. || dx.hypot(dy) > size * 0.6 {
        return None;
    }
    Some(
        (((dy.atan2(dx) + PI / 2. + TAU + PI / count as f64) % TAU) / TAU * count as f64).floor()
            as usize
            % count,
    )
}
pub fn icon(cx: f64, cy: f64, i: usize, count: usize, size: f64) -> (f64, f64) {
    let a = -PI / 2. + TAU * i as f64 / count as f64;
    (cx + a.cos() * size * 0.23, cy + a.sin() * size * 0.23)
}
pub fn editor_slot(x: f64, y: f64, count: usize) -> Option<usize> {
    (0..count)
        .map(|i| {
            let (ix, iy) = icon(165., 155., i, count, 440.);
            (i, (x - ix).hypot(y - iy))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .filter(|(_, d)| *d <= 31.)
        .map(|(i, _)| i)
}
pub fn content(
    cx: f64,
    cy: f64,
    size: f64,
    count: usize,
    w: f64,
    h: f64,
    prefer_right: bool,
) -> Vec<Rect> {
    if count == 0 {
        return vec![];
    }
    let rows = ((h - 24.) / 38.).floor().clamp(1., 12.) as usize;
    let columns = count.div_ceil(rows);
    let rows = count.div_ceil(columns);
    let height = rows as f64 * 38. + 8.;
    let hub = size * 86. / 600. + 12.;
    let mut split = 0;
    let mut best = 0.;
    for right in 0..=columns {
        let left = columns - right;
        let rw = if right > 0 {
            (w - cx - hub - 24. - 8. * (right - 1) as f64) / right as f64
        } else {
            f64::INFINITY
        };
        let lw = if left > 0 {
            (cx - hub - 24. - 8. * (left - 1) as f64) / left as f64
        } else {
            f64::INFINITY
        };
        let width = rw.min(lw).min(184.);
        if width > best || (width - best).abs() < 0.01 && prefer_right {
            best = width;
            split = right;
        }
    }
    let width = best.max(1.);
    let top = (cy - height / 2.).clamp(8., (h - height - 8.).max(8.));
    (0..count)
        .map(|i| {
            let col = i / rows;
            let x = if col < split {
                cx + size * 0.32 + 14. + col as f64 * (width + 8.)
            } else {
                cx - size * 0.32 - 14. - (columns - col) as f64 * (width + 8.)
            };
            let x = if col < split {
                x.min(w - 8. - (split - col) as f64 * (width + 8.))
            } else {
                x.max(8. + (col - split) as f64 * (width + 8.))
            };
            Rect {
                x,
                y: top + 4. + (i % rows) as f64 * 38.,
                w: width,
                h: 34.,
            }
        })
        .collect()
}
/// Prefer the opposite side, then free corners; never cover an outer entry or the cancel hub.
pub fn preview(cx: f64, cy: f64, size: f64, w: f64, h: f64, chips: &[Rect]) -> Option<Rect> {
    let radius = size * 188. / 600.;
    let left = cx - radius - 260.;
    let right = cx + radius + 20.;
    let side = if chips.first().is_some_and(|r| r.x > cx) {
        left
    } else {
        right
    };
    let candidates = [
        (side, cy - 94.),
        (side, cy - radius - 210.),
        (side, cy + radius + 20.),
        (8., 40.),
        (w - 248., 40.),
        (8., h - 196.),
        (w - 248., h - 196.),
    ];
    let circle = Rect {
        x: cx - radius - 8.,
        y: cy - radius - 8.,
        w: radius * 2. + 16.,
        h: radius * 2. + 16.,
    };
    let overlaps = |a: Rect, b: Rect| {
        a.x < b.x + b.w + 8. && a.x + a.w + 8. > b.x && a.y < b.y + b.h + 8. && a.y + a.h + 8. > b.y
    };
    candidates
        .into_iter()
        .map(|(x, y)| Rect {
            x,
            y,
            w: 240.,
            h: 188.,
        })
        .find(|r| {
            r.x >= 8.
                && r.y >= 8.
                && r.x + r.w <= w - 8.
                && r.y + r.h <= h - 8.
                && !overlaps(*r, circle)
                && chips.iter().all(|c| !overlaps(*r, *c))
        })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_does_not_cover_large_lists() {
        let chips = content(700., 450., 520., 38, 1470., 956., true);
        let p = preview(700., 450., 520., 1470., 956., &chips).expect("Room in a free corner");
        for c in chips {
            assert!(p.x + p.w <= c.x || p.x >= c.x + c.w || p.y + p.h <= c.y || p.y >= c.y + c.h);
        }
    }
    #[test]
    fn editor_only_hits_actual_slots() {
        for count in 4..=12 {
            assert_eq!(editor_slot(165., 155., count), None);
            assert_eq!(editor_slot(0., 0., count), None);
            for i in 0..count {
                let (x, y) = icon(165., 155., i, count, 440.);
                assert_eq!(editor_slot(x, y, count), Some(i));
            }
        }
    }
    #[test]
    fn directions_and_cancel() {
        assert_eq!(slot(120., 0., 8, 520.), Some(2));
        assert_eq!(slot(0., 0., 8, 520.), None);
        assert_eq!(slot(0., -120., 4, 520.), Some(0));
    }
    #[test]
    fn labels_visible_and_avoid_hub() {
        for count in [1, 9, 24, 36, 60, 120] {
            for cx in [30., 735., 1440.] {
                let rr = content(cx, 450., 520., count, 1470., 956., true);
                assert_eq!(rr.len(), count);
                for r in rr {
                    assert!(r.x >= 0. && r.y >= 0. && r.x + r.w <= 1470. && r.y + r.h <= 956.);
                    assert!(!r.has(cx, 450.));
                }
            }
        }
    }
}
