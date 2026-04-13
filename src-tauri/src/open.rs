use crate::config;

/// Open a file in Obsidian using its URL scheme (obsidian://open?vault=...&file=...)
pub fn open_in_obsidian(path: &str) -> Result<(), String> {
    let app_config = config::get()?;

    // Get path relative to vault root by stripping the vault_path prefix
    let relative_path = path
        .strip_prefix(&app_config.vault.path)
        .unwrap_or(path)
        .trim_start_matches('/');

    // Build the Obsidian URL scheme using configured vault name
    let url = format!(
        "obsidian://open?vault={}&file={}",
        app_config.vault.name,
        urlencoding::encode(relative_path)
    );

    // Run macOS `open` command with the URL
    std::process::Command::new("open")
        .arg(&url)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Open a file in any macOS app (e.g., "iA Writer", "VS Code")
pub fn open_in_app(path: &str, app: &str) -> Result<(), String> {
    std::process::Command::new("open")
        .args(["-a", app, path])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Open a terminal (iTerm or Terminal.app) and run a command with the file path
pub fn open_in_terminal(path: &str, cmd: &str) -> Result<(), String> {
    // AppleScript to control iTerm
    let iterm_script = format!(
        r#"tell application "iTerm"
            activate
            try
                set newWindow to (create window with default profile)
                tell current session of newWindow
                    write text "{} \"{}\""
                end tell
            on error
                tell current window
                    create tab with default profile
                    tell current session
                        write text "{} \"{}\""
                    end tell
                end tell
            end try
        end tell"#,
        cmd, path, cmd, path
    );

    // Try iTerm first using osascript (AppleScript runner)
    let result = std::process::Command::new("osascript")
        .args(["-e", &iterm_script])
        .output();

    if matches!(result, Ok(ref output) if output.status.success()) {
        return Ok(());
    }

    // Fallback to Terminal.app if iTerm isn't available
    let terminal_script = format!(
        r#"tell application "Terminal"
            activate
            do script "{} \"{}\""
        end tell"#,
        cmd, path
    );

    std::process::Command::new("osascript")
        .args(["-e", &terminal_script])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}
