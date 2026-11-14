use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Trigger {
    Key { vk: u16 },
    Mouse(String),
}
impl Default for Trigger {
    fn default() -> Self {
        Self::Key { vk: 86 }
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Pin {
    pub id: String,
    pub name: String,
    pub target: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub version: u8,
    pub trigger: Trigger,
    pub wheel: Vec<String>,
    pub slots_per_page: usize,
    pub names: BTreeMap<String, String>,
    pub content: BTreeMap<String, Vec<Pin>>,
    pub launch_at_login: bool,
    pub enabled_on_start: bool,
    pub wheel_size_px: f64,
    pub window_previews: bool,
    pub appearance: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 2,
            trigger: Trigger::default(),
            wheel: vec![String::new(); 8],
            slots_per_page: 8,
            names: BTreeMap::new(),
            content: BTreeMap::new(),
            launch_at_login: false,
            enabled_on_start: true,
            wheel_size_px: 520.,
            window_previews: false,
            appearance: "system".into(),
        }
    }
}
impl Settings {
    pub fn trigger_supported(&self) -> bool {
        match &self.trigger {
            Trigger::Key { vk } => crate::input::safe_trigger(*vk),
            Trigger::Mouse(name) => name == "mouse4" || name == "mouse5",
        }
    }
    pub fn normalize(&mut self) {
        if !self.trigger_supported() {
            self.trigger = Trigger::default();
        }
        self.slots_per_page = self.slots_per_page.clamp(4, 12);
        self.wheel_size_px = if self.wheel_size_px.is_finite() {
            self.wheel_size_px.clamp(360., 720.)
        } else {
            520.
        };
        let mut seen = std::collections::HashSet::new();
        for p in &mut self.wheel {
            if !p.starts_with('/') || seen.len() >= 24 || !seen.insert(p.clone()) {
                p.clear();
            }
        }
        self.wheel.truncate(36);
        let size = 24usize.div_ceil(self.slots_per_page) * self.slots_per_page;
        self.wheel.resize(size, String::new());
        for pins in self.content.values_mut() {
            pins.retain(|p| {
                !p.id.is_empty()
                    && !p.name.trim().is_empty()
                    && (p.target.starts_with('/')
                        || p.target.starts_with("https://")
                        || p.target.starts_with("http://"))
            });
            pins.truncate(24);
        }
        if !["system", "light", "dark"].contains(&self.appearance.as_str()) {
            self.appearance = "system".into();
        }
    }
    pub fn resize_slots(&mut self, count: usize) {
        self.slots_per_page = count.clamp(4, 12);
        self.wheel.retain(|p| !p.is_empty());
        self.normalize();
    }
    pub fn assign(&mut self, at: usize, path: String) -> bool {
        if at >= self.wheel.len() {
            return false;
        }
        if let Some(index) = self.wheel.iter().position(|p| p == &path) {
            self.wheel.swap(at, index);
            return true;
        }
        if self.wheel[at].is_empty() && self.wheel.iter().filter(|p| !p.is_empty()).count() >= 24 {
            return false;
        }
        self.wheel[at] = path;
        true
    }
    pub fn assign_many(&mut self, at: usize, paths: Vec<String>) -> (usize, usize) {
        let mut last = at;
        let mut count = 0;
        for (i, path) in paths.into_iter().enumerate() {
            let target = if i == 0 {
                Some(at)
            } else {
                self.wheel.iter().position(|p| p.is_empty())
            };
            let Some(target) = target else { break };
            if !self.assign(target, path) {
                break;
            }
            last = target;
            count += 1;
        }
        (last, count)
    }
    pub fn name(&self, path: &str) -> String {
        self.names.get(path).cloned().unwrap_or_else(|| {
            PathBuf::from(path)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
    }
    pub fn path() -> PathBuf {
        PathBuf::from(std::env::var_os("HOME").unwrap())
            .join("Library/Application Support/ViGo/settings.json")
    }
    pub fn load() -> (Self, String) {
        let path = Self::path();
        if path.exists() && !path.with_extension("before-rust.json").exists() {
            let _ = std::fs::copy(&path, path.with_extension("before-rust.json"));
        }
        let mut s: Self = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let notice = if s.trigger_supported() {
            String::new()
        } else {
            "原呼出键当前不可用，已恢复为 V，可在设置里重新选择。".into()
        };
        s.normalize();
        (s, notice)
    }
    pub fn save(&self) -> std::io::Result<()> {
        let p = Self::path();
        std::fs::create_dir_all(p.parent().unwrap())?;
        let temp = p.with_extension("native-tmp");
        std::fs::write(&temp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(temp, p)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Window {
    pub id: String,
    pub app_path: String,
    pub pid: i32,
    pub window_id: u32,
    pub index: i32,
    pub title: String,
    pub raw_title: String,
    pub minimized: bool,
    pub bounds: Vec<f64>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Tab {
    pub id: String,
    pub app_path: String,
    pub pid: i32,
    pub window_id: u32,
    pub index: usize,
    pub identifier: String,
    pub title: String,
    pub selected: bool,
}
#[derive(Clone, Debug)]
pub enum Content {
    Pin(Pin),
    Tab(Tab),
    Window(Window),
}
impl Content {
    pub fn title(&self) -> &str {
        match self {
            Self::Pin(p) => &p.name,
            Self::Tab(t) => &t.title,
            Self::Window(w) => &w.title,
        }
    }
    pub fn preview(&self) -> Option<Window> {
        match self {
            Self::Window(w) => Some(w.clone()),
            Self::Tab(t) => Some(Window {
                id: format!("{}:{}", t.pid, t.window_id),
                pid: t.pid,
                window_id: t.window_id,
                app_path: t.app_path.clone(),
                title: "浏览器 · 当前窗口".into(),
                ..Window::default()
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn count_changes_preserve_actions() {
        for n in 4..=12 {
            let mut s = Settings::default();
            s.wheel = (0..24).map(|i| format!("/App{i}.app")).collect();
            let before = s.wheel.clone();
            s.resize_slots(n);
            assert_eq!(
                s.wheel
                    .iter()
                    .filter(|p| !p.is_empty())
                    .cloned()
                    .collect::<Vec<_>>(),
                before
            );
        }
    }
    #[test]
    fn multi_file_drop_stops_at_capacity_without_replacing_previous_file() {
        let mut s = Settings::default();
        s.wheel = (0..23).map(|i| format!("/Existing{i}.app")).collect();
        s.normalize();
        let (_, added) = s.assign_many(23, vec!["/A.app".into(), "/B.app".into()]);
        assert_eq!(added, 1);
        assert_eq!(s.wheel[23], "/A.app");
        assert_eq!(s.wheel.iter().filter(|p| !p.is_empty()).count(), 24);
        assert!(!s.wheel.contains(&"/B.app".into()));
    }
    #[test]
    fn invalid_geometry_is_normalized() {
        let mut s = Settings::default();
        s.wheel_size_px = f64::NAN;
        s.normalize();
        assert_eq!(s.wheel_size_px, 520.);
    }
}
