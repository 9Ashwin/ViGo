use crate::layout::Rect;
use serde::{Deserialize, Serialize};

pub const ROW: f64 = 40.;
pub const HEADER: f64 = 36.;
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Window {
    pub pid: i32,
    pub id: u32,
    pub title: String,
    #[serde(default)]
    pub minimized: bool,
}
#[derive(Default)]
pub struct Picker {
    pub path: Option<String>,
    pub generation: u64,
    since: f64,
    requested: bool,
    pub items: Vec<Window>,
    pub selected: usize,
    pub explicit: bool,
    pub frame: Option<Rect>,
    pub first: usize,
    pub rows: usize,
    pub icon: (f64, f64, f64),
    origin_pointer: (f64, f64),
    last_scroll: f64,
}
impl Picker {
    pub fn reset(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.path = None;
        self.items.clear();
        self.frame = None;
        self.explicit = false;
        self.requested = false;
    }
    pub fn hover(&mut self, path: &str, now: f64) {
        if self.path.as_deref() != Some(path) {
            self.reset();
            self.path = Some(path.into());
            self.since = now;
            self.selected = 0;
            self.first = 0;
            self.last_scroll = f64::NEG_INFINITY;
        }
    }
    pub fn request(&mut self, now: f64) -> Option<(u64, String)> {
        if !self.requested && now - self.since >= 0.18 {
            let path = self.path.clone()?;
            self.requested = true;
            Some((self.generation, path))
        } else {
            None
        }
    }
    pub fn accept(&mut self, generation: u64, items: Vec<Window>) -> bool {
        if generation != self.generation || self.path.is_none() {
            return false;
        }
        self.items = items
            .into_iter()
            .filter(|w| w.pid > 0 && w.id > 0 && !w.title.is_empty())
            .collect();
        self.selected = 0;
        self.explicit = false;
        self.items.len() > 1
    }
    pub fn place(&mut self, icon: (f64, f64, f64), pointer: (f64, f64), width: f64, height: f64) {
        self.icon = icon;
        self.origin_pointer = pointer;
        self.rows = self
            .items
            .len()
            .min(((height - 60. - HEADER) / ROW).floor().max(1.) as usize)
            .min(10);
        let w = 272f64.min((width - 16.).max(1.));
        let h = HEADER + ROW * self.rows as f64 + 8.;
        let right = icon.0 + icon.2 / 2. + 12.;
        let x = if right + w <= width - 8. {
            right
        } else {
            icon.0 - icon.2 / 2. - 12. - w
        };
        self.frame = Some(Rect {
            x: x.clamp(8., (width - w - 8.).max(8.)),
            y: (icon.1 - h / 2.).clamp(8., (height - h - 8.).max(8.)),
            w,
            h,
        });
    }
    pub fn contains(&self, p: (f64, f64)) -> bool {
        let Some(f) = self.frame else { return false };
        let in_menu = p.0 >= f.x && p.0 <= f.x + f.w && p.1 >= f.y && p.1 <= f.y + f.h;
        let near_icon = (p.0 - self.icon.0).hypot(p.1 - self.icon.1) <= self.icon.2 * 0.85;
        // A short bridge keeps the app locked while traversing the gap into its menu.
        let bridge = p.0 >= self.icon.0.min(f.x) - 4.
            && p.0 <= self.icon.0.max(f.x + f.w) + 4.
            && (p.1 - self.icon.1).abs() <= self.icon.2 / 2. + 8.;
        in_menu || near_icon || bridge
    }
    pub fn point(&mut self, p: (f64, f64)) {
        let Some(f) = self.frame else { return };
        if (p.0 - self.origin_pointer.0).hypot(p.1 - self.origin_pointer.1) < 4. {
            return;
        }
        if p.0 < f.x
            || p.0 > f.x + f.w
            || p.1 < f.y + HEADER
            || p.1 >= f.y + HEADER + self.rows as f64 * ROW
        {
            return;
        }
        let i = self.first + ((p.1 - f.y - HEADER) / ROW) as usize;
        if i < self.items.len() {
            self.selected = i;
            self.explicit = true;
        }
    }
    pub fn scroll(&mut self, delta: f64, now: f64) {
        if self.frame.is_none() || delta == 0. || now - self.last_scroll < 0.14 {
            return;
        }
        self.last_scroll = now;
        // Quartz positive vertical deltas mean upward, matching the list's visual order.
        self.selected = if delta > 0. {
            self.selected.saturating_sub(1)
        } else {
            (self.selected + 1).min(self.items.len() - 1)
        };
        self.explicit = true;
        if self.selected < self.first {
            self.first = self.selected;
        }
        if self.selected >= self.first + self.rows {
            self.first = self.selected + 1 - self.rows;
        }
    }
    pub fn choice(&self) -> Option<&Window> {
        if self.explicit {
            self.items.get(self.selected)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn windows(n: usize) -> Vec<Window> {
        (0..n)
            .map(|i| Window {
                pid: 7,
                id: i as u32 + 1,
                title: format!("Project {i}"),
                minimized: false,
            })
            .collect()
    }
    #[test]
    fn only_dwelled_apps_query_once_and_stale_replies_never_open() {
        let mut p = Picker::default();
        p.hover("GoLand", 1.);
        assert!(p.request(1.1).is_none());
        let (g, _) = p.request(1.2).unwrap();
        assert!(p.request(2.).is_none());
        p.hover("Finder", 3.);
        assert!(!p.accept(g, windows(3)));
        assert!(p.items.is_empty());
    }
    #[test]
    fn opening_under_pointer_never_chooses_a_window_without_movement() {
        let mut p = Picker::default();
        p.hover("GoLand", 0.);
        assert!(p.accept(p.generation, windows(3)));
        p.place((350., 170., 48.), (350., 170.), 640., 480.);
        assert!(p.choice().is_none());
        p.point((350., 170.));
        assert!(p.choice().is_none());
        let f = p.frame.unwrap();
        p.point((f.x + 20., f.y + HEADER + ROW * 1.5));
        assert_eq!(p.choice().unwrap().id, 2);
    }
    #[test]
    fn scroll_is_bounded_and_exposes_all_rows_without_wrapping() {
        let mut p = Picker::default();
        p.hover("GoLand", 0.);
        p.accept(p.generation, windows(24));
        p.place((300., 200., 48.), (300., 200.), 800., 480.);
        p.scroll(-1., 1.);
        assert_eq!(p.choice().unwrap().id, 2);
        p.scroll(-1., 1.01);
        assert_eq!(p.selected, 1);
        for i in 0..40 {
            p.scroll(-1., 2. + i as f64);
        }
        assert_eq!(p.selected, 23);
        assert!(p.first + p.rows > p.selected);
        p.reset();
        assert!(p.choice().is_none());
        assert!(p.frame.is_none());
    }
    #[test]
    fn popover_stays_on_screen_and_bridge_is_contiguous() {
        let mut p = Picker::default();
        p.hover("GoLand", 0.);
        p.accept(p.generation, windows(3));
        p.place((780., 470., 48.), (780., 470.), 800., 480.);
        let f = p.frame.unwrap();
        assert!(f.x + f.w <= 792. && f.y + f.h <= 472.);
        assert!(f.x < 780.);
        assert!(p.contains(((f.x + f.w + 780.) / 2., 470.)));
        assert!(!p.contains((20., 20.)));
    }
    #[test]
    fn one_window_does_not_expand_and_old_settings_default_off() {
        let mut p = Picker::default();
        p.hover("GoLand", 0.);
        assert!(!p.accept(p.generation, windows(1)));
        let s: crate::model::Settings = serde_json::from_str("{}").unwrap();
        assert!(!s.show_window_list);
        let mut s = s;
        s.show_window_list = true;
        let restored: crate::model::Settings =
            serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert!(restored.show_window_list);
    }
}
