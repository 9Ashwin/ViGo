#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]
mod input;
mod layout;
mod model;
#[cfg(target_os = "macos")]
mod platform;
#[cfg(target_os = "macos")]
mod ui;
#[cfg(target_os = "windows")]
mod windows_app;
fn main() {
    #[cfg(target_os = "macos")]
    ui::run();
    #[cfg(target_os = "windows")]
    windows_app::run();
}
