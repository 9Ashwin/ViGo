mod input;
mod install;
mod layout;
mod mascot;
mod model;
mod platform;
mod tether;
mod ui;
fn main() -> std::process::ExitCode {
    if let Some(code) = mascot::command_line(&std::env::args().collect::<Vec<_>>()) {
        return code;
    }
    ui::run();
    std::process::ExitCode::SUCCESS
}
