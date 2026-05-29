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

    // "Open Recent" submenu — built fresh from the persisted recents list so
    // it reflects the latest order. See record_recent / clear_recents below;
    // the whole menubar rebuilds on every change (muda menus aren't mutable
    // in place).
    let recents = read_recents(handle);
    let recent_items: Vec<MenuItem<tauri::Wry>> = if recents.is_empty() {
        vec![MenuItem::with_id(
            handle,
            "recent_empty",
            "No Recent Files",
            false,
            None::<&str>,
        )?]
    } else {
        recents
            .iter()
            .map(|path| {
                // Display the file name; the full path lives in the ID so the
                // event handler knows what to open.
                let label = std::path::Path::new(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(path.as_str())
                    .to_string();
                MenuItem::with_id(handle, format!("recent:{}", path), label, true, None::<&str>)
            })
            .collect::<tauri::Result<Vec<_>>>()?
    };
    let recent_sep = PredefinedMenuItem::separator(handle)?;
    let clear_recents_item = MenuItem::with_id(
        handle,
        "clear_recent_files",
        "Clear Menu",
        !recents.is_empty(),
        None::<&str>,
    )?;
    let mut recent_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = recent_items
        .iter()
        .map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>)
        .collect();
    if !recents.is_empty() {
        recent_refs.push(&recent_sep);
        recent_refs.push(&clear_recents_item);
    }
    let open_recent = Submenu::with_items(handle, "Open Recent", true, &recent_refs)?;

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
            &open_recent,
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

    // Window menu — standard macOS Minimize / Zoom / Close bundle.
    // Tauri 2.10 doesn't surface muda's `bring_all_to_front`, but
    // tagging this submenu as the system Windows menu (below, via
    // `set_as_windows_menu_for_nsapp`) makes AppKit append the open-
    // window list automatically — so the menu still feels right.
    let window_menu = Submenu::with_items(
        handle,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(handle, None)?,
            &PredefinedMenuItem::maximize(handle, Some("Zoom"))?,
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

    // Tell AppKit which submenus are the standard "Window" and "Help"
    // menus. NSApplication then appends the open-windows list to Window
    // and routes the "Help > Search" field through Help (matches every
    // first-party Mac app).
    #[cfg(target_os = "macos")]
    {
        let _ = window_menu.set_as_windows_menu_for_nsapp();
        let _ = help_menu.set_as_help_menu_for_nsapp();
    }

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

    // Open Recent: id is "recent:<absolute-path>". Emit the path so the
    // frontend can route through its existing file-select flow.
    if let Some(path) = id.strip_prefix("recent:") {
        let _ = app.emit("menu-open-recent", path.to_string());
        return;
    }
    if id == "clear_recent_files" {
        clear_recents(app);
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

// ---------- Open Recent persistence ----------
//
// Recently opened file paths live in <app_data_dir>/recent_files.json as a
// JSON array of absolute path strings, newest-first, capped to MAX_RECENTS.
// Frontend calls `record_recent_file` whenever the user selects a file; the
// File → Open Recent submenu is rebuilt from this list at app start and on
// every record/clear (muda menus aren't mutable in place).

const MAX_RECENTS: usize = 10;
const RECENTS_FILE: &str = "recent_files.json";

/// Bypass `app.path().app_data_dir()` here — this function is called by
/// `build_app_menu`, which Tauri invokes BEFORE the PathResolver state is
/// registered, causing a panic ("state() called before manage()"). Mirrors
/// the HOME-based path that `config.rs::config_path` already uses, so the
/// `recent_files.json` lands next to `config.json`.
fn recents_path(_app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(
        std::path::PathBuf::from(home)
            .join("Library/Application Support/com.ejfox.dispatch")
            .join(RECENTS_FILE),
    )
}

pub fn read_recents(app: &tauri::AppHandle) -> Vec<String> {
    let Some(p) = recents_path(app) else {
        return vec![];
    };
    let Ok(bytes) = std::fs::read(&p) else {
        return vec![];
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn write_recents(app: &tauri::AppHandle, paths: &[String]) {
    let Some(p) = recents_path(app) else { return };
    if let Some(parent) = p.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_vec_pretty(paths) {
        let _ = std::fs::write(&p, json);
    }
}

pub fn record_recent(app: &tauri::AppHandle, path: String) {
    let mut recents = read_recents(app);
    recents.retain(|p| p != &path);
    recents.insert(0, path);
    recents.truncate(MAX_RECENTS);
    write_recents(app, &recents);
    rebuild_menu(app);
}

pub fn clear_recents(app: &tauri::AppHandle) {
    write_recents(app, &[]);
    rebuild_menu(app);
}

fn rebuild_menu(app: &tauri::AppHandle) {
    if let Ok(menu) = build_app_menu(app) {
        let _ = app.set_menu(menu);
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
