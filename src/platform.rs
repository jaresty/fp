/// Send a macOS system notification via osascript. Silently ignores errors on non-macOS or
/// headless environments where notifications are unavailable.
pub fn notify_macos_titled(title: &str, msg: &str) {
    let script = format!(r#"display notification "{}" with title "{}""#, msg.replace('"', "'"), title.replace('"', "'"));
    let _ = std::process::Command::new("osascript").args(["-e", &script]).output();
}
