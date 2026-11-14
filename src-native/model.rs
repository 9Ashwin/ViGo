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
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub version: u8,
    pub trigger: Trigger,
    #[serde(skip)]
    pub wheel: Vec<String>,
    #[serde(skip)]
    pub names: BTreeMap<String, String>,
    pub launch_at_login: bool,
    pub enabled_on_start: bool,
    pub wheel_size_px: f64,
    pub appearance: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 3,
            trigger: Trigger::default(),
            wheel: vec![],
            names: BTreeMap::new(),
            launch_at_login: false,
            enabled_on_start: true,
            wheel_size_px: 520.,
            appearance: "system".into(),
        }
    }
}
impl Settings {
    pub fn normalize(&mut self) {
        self.version = 3;
        if !self.trigger_supported() {
            self.trigger = Trigger::default();
        }
        self.wheel_size_px = if self.wheel_size_px.is_finite() {
            self.wheel_size_px.clamp(360., 720.)
        } else {
            520.
        };
        if !["system", "light", "dark"].contains(&self.appearance.as_str()) {
            self.appearance = "system".into();
        }
    }
    pub fn trigger_supported(&self) -> bool {
        match &self.trigger {
            Trigger::Key { vk } => crate::input::safe_trigger(*vk),
            Trigger::Mouse(n) => n == "mouse4" || n == "mouse5",
        }
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
        let root = PathBuf::from(std::env::var_os("HOME").unwrap())
            .join("Library/Application Support/ViGo");
        root.join("settings.json")
    }
    pub fn load() -> (Self, String) {
        let p = Self::path();
        let backup = p.with_extension("before-dock.json");
        if p.exists() && !backup.exists() {
            let _ = std::fs::copy(&p, backup);
        }
        let mut s: Self = std::fs::read(p)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let notice = if s.trigger_supported() {
            String::new()
        } else {
            "原呼出键不可用，已恢复为 V。".into()
        };
        s.normalize();
        (s, notice)
    }
    pub fn save(&self) -> std::io::Result<()> {
        let p = Self::path();
        std::fs::create_dir_all(p.parent().unwrap())?;
        let tmp = p.with_extension("native-tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, p)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DockItem {
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub kind: String,
}
pub fn remap_selection(old: &[String], new: &[String], slot: Option<usize>) -> Option<usize> {
    slot.and_then(|i| old.get(i))
        .and_then(|p| new.iter().position(|n| n == p))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_lists_are_not_imported_or_saved() {
        let s: Settings = serde_json::from_str(
            r#"{"wheel":["/Old.app"],"content":{"/Old.app":[]},"names":{"/Old.app":"Old"}}"#,
        )
        .unwrap();
        assert!(s.wheel.is_empty());
        let v = serde_json::to_value(s).unwrap();
        assert!(v.get("wheel").is_none());
        assert!(v.get("content").is_none());
    }
    #[test]
    fn live_dock_changes_keep_identity_not_index() {
        let old = vec!["/A.app".into(), "/B.app".into(), "/C.app".into()];
        let new = vec!["/C.app".into(), "/A.app".into(), "/B.app".into()];
        assert_eq!(remap_selection(&old, &new, Some(1)), Some(2));
        assert_eq!(remap_selection(&old, &vec!["/A.app".into()], Some(1)), None);
    }
    #[test]
    fn normalizes_display_only() {
        let mut s = Settings::default();
        s.wheel = (0..45).map(|i| format!("/App{i}.app")).collect();
        s.wheel_size_px = f64::NAN;
        s.normalize();
        assert_eq!(s.wheel.len(), 45);
        assert_eq!(s.wheel_size_px, 520.);
    }
}
