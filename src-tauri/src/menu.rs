use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};

use super::preview;

/// Build the application menu bar.
///
/// macOS apps need a proper menu bar for Cmd+C/V/Z to work in text fields.
pub fn build_app_menu(handle: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    // App menu ("Dispatch")
    let app_menu = Submenu::with_items(
        handle,
        "Dispatch",
        true,
        &[
            &PredefinedMenuItem::about(handle, Some("About Dispatch"), None)?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::services(handle, None)?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::hide(handle, None)?,
            &PredefinedMenuItem::hide_others(handle, None)?,
            &PredefinedMenuItem::show_all(handle, None)?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::quit(handle, None)?,
        ],
    )?;

    // File menu
    let file_menu = Submenu::with_items(
        handle,
        "File",
        true,
        &[
            &MenuItem::with_id(handle, "new_post", "New Post", true, Some("CmdOrCtrl+N"))?,
            &PredefinedMenuItem::separator(handle)?,
            &MenuItem::with_id(
                handle,
                "open_in_obsidian",
                "Open in Obsidian",
                true,
                Some("CmdOrCtrl+Shift+O"),
            )?,
            &MenuItem::with_id(
                handle,
                "reveal_in_finder",
                "Reveal in Finder",
                true,
                Some("CmdOrCtrl+Shift+R"),
            )?,
            &PredefinedMenuItem::separator(handle)?,
            &MenuItem::with_id(handle, "refresh", "Refresh", true, Some("CmdOrCtrl+R"))?,
        ],
    )?;

    // Publish menu — domain-specific actions, the heart of the app
    let publish_menu = Submenu::with_items(
        handle,
        "Publish",
        true,
        &[
            &MenuItem::with_id(handle, "publish", "Publish", true, Some("CmdOrCtrl+Return"))?,
            &MenuItem::with_id(
                handle,
                "publish_unlisted",
                "Publish as Unlisted",
                true,
                Some("CmdOrCtrl+Shift+Return"),
            )?,
            &PredefinedMenuItem::separator(handle)?,
            &MenuItem::with_id(
                handle,
                "copy_public_url",
                "Copy Public URL",
                true,
                Some("CmdOrCtrl+Shift+L"),
            )?,
            &MenuItem::with_id(
                handle,
                "generate_og",
                "Generate OG Image",
                true,
                None::<&str>,
            )?,
            &PredefinedMenuItem::separator(handle)?,
            &MenuItem::with_id(
                handle,
                "show_journal",
                "Show Publishing Journal",
                true,
                Some("CmdOrCtrl+J"),
            )?,
        ],
    )?;

    // Edit menu -- PredefinedMenuItems map to native macOS Edit actions
    // so Cmd+C/V/Z/X/A work in webview text fields
    let edit_menu = Submenu::with_items(
        handle,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(handle, None)?,
            &PredefinedMenuItem::redo(handle, None)?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::cut(handle, None)?,
            &PredefinedMenuItem::copy(handle, None)?,
            &PredefinedMenuItem::paste(handle, None)?,
            &PredefinedMenuItem::select_all(handle, None)?,
        ],
    )?;

    // View menu
    let view_menu = Submenu::with_items(
        handle,
        "View",
        true,
        &[
            &MenuItem::with_id(
                handle,
                "toggle_sidebar",
                "Toggle Sidebar",
                true,
                Some("CmdOrCtrl+0"),
            )?,
            &MenuItem::with_id(
                handle,
                "toggle_compact",
                "Toggle Compact",
                true,
                Some("CmdOrCtrl+Shift+C"),
            )?,
            &PredefinedMenuItem::separator(handle)?,
            &MenuItem::with_id(
                handle,
                "panel_preview",
                "Show Preview",
                true,
                Some("CmdOrCtrl+1"),
            )?,
            &MenuItem::with_id(
                handle,
                "panel_media",
                "Show Media",
                true,
                Some("CmdOrCtrl+2"),
            )?,
            &MenuItem::with_id(
                handle,
                "panel_journal",
                "Show Journal",
                true,
                Some("CmdOrCtrl+3"),
            )?,
            &MenuItem::with_id(handle, "panel_gear", "Show Gear", true, Some("CmdOrCtrl+4"))?,
            &PredefinedMenuItem::separator(handle)?,
            &MenuItem::with_id(handle, "search", "Search...", true, Some("CmdOrCtrl+K"))?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::fullscreen(handle, None)?,
        ],
    )?;

    // Help menu — every native Mac app has one
    let help_menu = Submenu::with_items(
        handle,
        "Help",
        true,
        &[
            &MenuItem::with_id(
                handle,
                "show_help",
                "Dispatch Help",
                true,
                Some("CmdOrCtrl+/"),
            )?,
            &PredefinedMenuItem::separator(handle)?,
            &MenuItem::with_id(handle, "report_issue", "Report Issue…", true, None::<&str>)?,
            &MenuItem::with_id(
                handle,
                "view_on_github",
                "View on GitHub",
                true,
                None::<&str>,
            )?,
        ],
    )?;

    // Window menu
    let window_menu = Submenu::with_items(
        handle,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(handle, None)?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::close_window(handle, None)?,
        ],
    )?;

    let menu = Menu::with_items(
        handle,
        &[
            &app_menu,
            &file_menu,
            &edit_menu,
            &view_menu,
            &publish_menu,
            &window_menu,
            &help_menu,
        ],
    )?;

    Ok(menu)
}

/// Build the system tray icon with its menu.
pub fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let open_i = MenuItem::with_id(app, "open", "Open Dispatch", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_i, &quit_i])?;

    // DIAGNOSTIC: try the dedicated tray.png as a template first, but fall
    // back to the colored app icon if anything goes sideways. The fallback
    // guarantees *some* visible icon while we figure out which path works.
    let mut tray_builder = TrayIconBuilder::new();
    if let Some(window_icon) = app.default_window_icon() {
        tray_builder = tray_builder.icon(window_icon.clone());
    }
    tray_builder
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "quit" => {
                preview::stop_server();
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

/// Handle custom menu item clicks by emitting events to the frontend.
pub fn handle_menu_event(app: &tauri::AppHandle, event: tauri::menu::MenuEvent) {
    let id = event.id().as_ref();

    // Most items are passthrough emits with a stable naming convention.
    let passthrough = [
        "new_post",
        "refresh",
        "toggle_compact",
        "search",
        "open_in_obsidian",
        "reveal_in_finder",
        "publish",
        "publish_unlisted",
        "copy_public_url",
        "generate_og",
        "show_journal",
        "toggle_sidebar",
        "panel_preview",
        "panel_media",
        "panel_journal",
        "panel_gear",
        "show_help",
    ];
    if passthrough.contains(&id) {
        let _ = app.emit(&format!("menu-{}", id.replace('_', "-")), ());
        return;
    }

    // Items that open URLs directly — no need to round-trip through the frontend.
    match id {
        "report_issue" => {
            let _ = open_url("https://github.com/ejfox/website-dispatch/issues/new");
        }
        "view_on_github" => {
            let _ = open_url("https://github.com/ejfox/website-dispatch");
        }
        _ => {}
    }
}

fn open_url(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/open")
            .arg(url)
            .spawn()?;
    }
    Ok(())
}

// Native context menus for FileList live in the frontend via
// @tauri-apps/api/menu — see FileList.vue:showContextMenu. Cleaner there
// because actions can route through Tauri commands directly without a
// round-trip through Rust event emission.
