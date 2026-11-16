mod companion_link;
mod input;
mod install;
mod layout;
mod mascot;
mod model;
mod network;
mod platform;
mod tether;
mod ui;
fn main() -> std::process::ExitCode {
    #[cfg(feature = "diagnostics")]
    {
        let args = std::env::args().collect::<Vec<_>>();
        if args
            .get(1)
            .is_some_and(|a| a == "--check-companion-download")
        {
            return objc2::rc::autoreleasepool(|_| {
                match network::download(
                    args.get(2).map(String::as_str).unwrap_or_default(),
                    24 * 1024 * 1024,
                ) {
                    Ok(data) => {
                        println!(
                            "{}",
                            serde_json::json!({"ok":true,"bytes":data.len(),"transport":"NSURLSession/system-settings"})
                        );
                        std::process::ExitCode::SUCCESS
                    }
                    Err(error) => {
                        eprintln!("{}", serde_json::json!({"ok":false,"error":error}));
                        std::process::ExitCode::FAILURE
                    }
                }
            });
        }
    }
    if let Some(code) = mascot::command_line(&std::env::args().collect::<Vec<_>>()) {
        return code;
    }
    ui::run();
    std::process::ExitCode::SUCCESS
}
