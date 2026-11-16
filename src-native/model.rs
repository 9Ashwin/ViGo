use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};
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
    pub hidden_apps: BTreeSet<String>,
    pub added_apps: Vec<String>,
    pub app_order: Vec<String>,
    pub mascot_id: Option<String>,
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
            hidden_apps: BTreeSet::new(),
            added_apps: vec![],
            app_order: vec![],
            mascot_id: None,
            launch_at_login: false,
            enabled_on_start: true,
            wheel_size_px: 520.,
            appearance: "system".into(),
        }
    }
}
impl Settings {
    pub fn visible_apps(&self) -> Vec<String> {
        self.all_apps()
            .into_iter()
            .filter(|path| !self.hidden_apps.contains(path))
            .collect()
    }
    pub fn all_apps(&self) -> Vec<String> {
        let mut seen = BTreeSet::new();
        let mut apps: Vec<_> = self
            .wheel
            .iter()
            .chain(&self.added_apps)
            .filter(|p| seen.insert((*p).clone()))
            .cloned()
            .collect();
        let rank: BTreeMap<_, _> = self
            .app_order
            .iter()
            .enumerate()
            .map(|(i, p)| (p, i))
            .collect();
        apps.sort_by_key(|p| rank.get(p).copied().unwrap_or(usize::MAX));
        apps
    }
    pub fn add_application(&mut self, path: &str) {
        if !self.added_apps.iter().any(|p| p == path) {
            self.added_apps.push(path.to_owned());
        }
        self.hidden_apps.remove(path);
    }
    pub fn remove_added_application(&mut self, path: &str) {
        self.added_apps.retain(|p| p != path);
        if !self.wheel.iter().any(|p| p == path) {
            self.app_order.retain(|p| p != path);
            self.hidden_apps.remove(path);
        }
    }
    pub fn move_application(&mut self, path: &str, delta: isize) {
        let mut apps = self.all_apps();
        if let Some(index) = apps.iter().position(|p| p == path) {
            let next = index.saturating_add_signed(delta).min(apps.len() - 1);
            apps.swap(index, next);
            self.app_order = apps;
        }
    }
    pub fn swap_applications(&mut self, a: &str, b: &str) {
        let mut apps = self.all_apps();
        if let (Some(a), Some(b)) = (
            apps.iter().position(|p| p == a),
            apps.iter().position(|p| p == b),
        ) {
            apps.swap(a, b);
            self.app_order = apps;
        }
    }
    /// Insert a dropped app before the targeted app, keeping all other apps and hidden choices.
    pub fn insert_application(&mut self, path: &str, before: Option<&str>) {
        self.add_application(path);
        if before == Some(path) {
            return;
        }
        let mut apps = self.all_apps();
        apps.retain(|p| p != path);
        let at = before
            .and_then(|target| apps.iter().position(|p| p == target))
            .unwrap_or(apps.len());
        apps.insert(at, path.to_owned());
        self.app_order = apps;
    }
    pub fn drop_application(&mut self, path: &str, target: Option<&str>) {
        if self.visible_apps().iter().any(|p| p == path) {
            if let Some(target) =
                target.filter(|target| self.all_apps().iter().any(|p| p == *target))
            {
                self.swap_applications(path, target);
            } else {
                let mut apps = self.all_apps();
                apps.retain(|p| p != path);
                apps.push(path.to_owned());
                self.app_order = apps;
            }
        } else {
            self.insert_application(path, target);
        }
    }
    pub fn set_app_visible(&mut self, path: &str, visible: bool) {
        if visible {
            self.hidden_apps.remove(path);
        } else {
            self.hidden_apps.insert(path.to_owned());
        }
    }
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
    fn dragging_an_existing_gallery_app_lands_in_the_target_slot_without_pinning_or_duplication() {
        let mut s = Settings::default();
        s.wheel = vec!["/A.app".into(), "/B.app".into(), "/C.app".into()];
        s.drop_application("/A.app", Some("/C.app"));
        assert_eq!(s.visible_apps(), vec!["/C.app", "/B.app", "/A.app"]);
        assert!(s.added_apps.is_empty());
        s.drop_application("/C.app", None);
        assert_eq!(s.visible_apps(), vec!["/B.app", "/A.app", "/C.app"]);
        s.drop_application("/New.app", Some("/A.app"));
        assert_eq!(
            s.visible_apps(),
            vec!["/B.app", "/New.app", "/A.app", "/C.app"]
        );
    }
    #[test]
    fn gallery_drop_inserts_without_replacing_apps_or_duplicating_existing_entries() {
        let mut s = Settings::default();
        s.wheel = vec!["/A.app".into(), "/B.app".into(), "/C.app".into()];
        s.insert_application("/New.app", Some("/B.app"));
        assert_eq!(
            s.visible_apps(),
            vec!["/A.app", "/New.app", "/B.app", "/C.app"]
        );
        s.insert_application("/C.app", Some("/A.app"));
        assert_eq!(
            s.visible_apps(),
            vec!["/C.app", "/A.app", "/New.app", "/B.app"]
        );
        assert_eq!(
            s.added_apps
                .iter()
                .filter(|p| p.as_str() == "/C.app")
                .count(),
            1
        );
    }
    #[test]
    fn dropped_apps_unhide_and_missing_targets_append_safely() {
        let mut s = Settings::default();
        s.wheel = vec!["/A.app".into(), "/B.app".into()];
        s.set_app_visible("/B.app", false);
        s.insert_application("/B.app", Some("/A.app"));
        assert_eq!(s.visible_apps(), vec!["/B.app", "/A.app"]);
        s.insert_application("/New.app", Some("/Gone.app"));
        assert_eq!(s.visible_apps(), vec!["/B.app", "/A.app", "/New.app"]);
        s.insert_application("/B.app", None);
        assert_eq!(s.visible_apps(), vec!["/A.app", "/New.app", "/B.app"]);
        let before = s.app_order.clone();
        s.insert_application("/New.app", Some("/New.app"));
        assert_eq!(s.app_order, before);
    }
    #[test]
    fn wheel_editor_swaps_by_app_identity_and_keeps_hidden_apps_and_other_slots() {
        let mut s = Settings::default();
        s.wheel = vec!["/A.app".into(), "/Hidden.app".into(), "/B.app".into()];
        s.set_app_visible("/Hidden.app", false);
        s.add_application("/Manual.app");
        s.swap_applications("/A.app", "/Manual.app");
        assert_eq!(s.visible_apps(), vec!["/Manual.app", "/B.app", "/A.app"]);
        assert_eq!(s.all_apps()[1], "/Hidden.app");
        let before = s.app_order.clone();
        s.swap_applications("/Gone.app", "/B.app");
        assert_eq!(s.app_order, before);
    }
    #[test]
    fn manual_apps_are_deduplicated_with_dock_and_keep_the_saved_order() {
        let mut s = Settings::default();
        s.wheel = vec!["/A.app".into(), "/B.app".into()];
        s.add_application("/B.app");
        s.add_application("/Manual.app");
        s.add_application("/Manual.app");
        assert_eq!(s.visible_apps(), vec!["/A.app", "/B.app", "/Manual.app"]);
        s.move_application("/Manual.app", -1);
        s.move_application("/Manual.app", -1);
        let mut loaded: Settings =
            serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        loaded.wheel = vec!["/B.app".into(), "/New.app".into(), "/A.app".into()];
        assert_eq!(
            loaded.visible_apps(),
            vec!["/Manual.app", "/A.app", "/B.app", "/New.app"]
        );
        loaded.app_order.clear();
        assert_eq!(
            loaded.visible_apps(),
            vec!["/B.app", "/New.app", "/A.app", "/Manual.app"]
        );
    }
    #[test]
    fn removing_a_manual_reference_never_removes_the_dock_entry() {
        let mut s = Settings::default();
        s.wheel = vec!["/A.app".into()];
        s.add_application("/A.app");
        s.add_application("/Manual.app");
        s.remove_added_application("/A.app");
        assert_eq!(s.visible_apps(), vec!["/A.app", "/Manual.app"]);
        s.remove_added_application("/Manual.app");
        assert_eq!(s.visible_apps(), vec!["/A.app"]);
        s.set_app_visible("/A.app", false);
        s.add_application("/A.app");
        assert_eq!(s.visible_apps(), vec!["/A.app"]);
    }
    #[test]
    fn exclusions_survive_save_and_old_settings_show_everything() {
        let mut s = Settings::default();
        s.wheel = vec!["/A.app".into(), "/B.app".into()];
        s.set_app_visible("/B.app", false);
        let json = serde_json::to_string(&s).unwrap();
        let mut loaded: Settings = serde_json::from_str(&json).unwrap();
        loaded.wheel = s.wheel.clone();
        assert_eq!(loaded.visible_apps(), vec!["/A.app"]);
        let mut old: Settings = serde_json::from_str(r#"{"version":3}"#).unwrap();
        old.wheel = s.wheel;
        assert_eq!(old.visible_apps().len(), 2);
    }
    #[test]
    fn live_dock_order_and_new_apps_follow_exclusions_without_forgetting_removed_apps() {
        let mut s = Settings::default();
        s.set_app_visible("/B.app", false);
        s.wheel = vec![
            "/C.app".into(),
            "/B.app".into(),
            "/A.app".into(),
            "/New.app".into(),
        ];
        assert_eq!(s.visible_apps(), vec!["/C.app", "/A.app", "/New.app"]);
        s.wheel.retain(|p| p != "/B.app");
        s.normalize();
        s.wheel.insert(0, "/B.app".into());
        assert_eq!(s.visible_apps(), vec!["/C.app", "/A.app", "/New.app"]);
        s.hidden_apps.clear();
        assert_eq!(s.visible_apps(), s.wheel);
    }
    #[test]
    fn hiding_the_selected_app_cancels_identity_and_all_apps_can_be_restored() {
        let mut s = Settings::default();
        s.wheel = vec!["/A.app".into(), "/B.app".into()];
        let old = s.visible_apps();
        s.set_app_visible("/A.app", false);
        assert_eq!(remap_selection(&old, &s.visible_apps(), Some(0)), None);
        assert_eq!(remap_selection(&old, &s.visible_apps(), Some(1)), Some(0));
        s.set_app_visible("/B.app", false);
        assert!(s.visible_apps().is_empty());
        s.set_app_visible("/A.app", true);
        assert_eq!(s.visible_apps(), vec!["/A.app"]);
    }
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
