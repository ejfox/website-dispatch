use std::sync::Mutex;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager};

use super::preview;

/// Key equivalents for every custom menu item, in one table so a test can
/// check that no two collide (with each other or with the system's own
/// Edit/Window/App items). On macOS a key equivalent fires before the
/// webview sees the key, so a duplicate here silently steals a shortcut.
pub const ACCELERATORS: &[(&str, &str)] = &[
    // App
    ("settings", "CmdOrCtrl+,"),
    // File
    ("new_post", "CmdOrCtrl+N"),
    ("open_in_obsidian", "CmdOrCtrl+Shift+O"),
    ("reveal_in_finder", "CmdOrCtrl+Shift+R"),
    ("copy_path", "CmdOrCtrl+Alt+C"), // Finder's "Copy as Pathname"
    ("refresh", "CmdOrCtrl+R"),
    // View
    ("toggle_sidebar", "CmdOrCtrl+0"),
    ("toggle_inspector", "CmdOrCtrl+Alt+I"),
    ("toggle_compact", "CmdOrCtrl+Shift+C"),
    ("panel_preview", "CmdOrCtrl+1"),
    ("panel_media", "CmdOrCtrl+2"),
    ("panel_journal", "CmdOrCtrl+3"),
    ("panel_changed", "CmdOrCtrl+4"),
    ("panel_activity", "CmdOrCtrl+5"),
    ("search", "CmdOrCtrl+K"),
    // Publish
    ("publish", "CmdOrCtrl+Return"),
    ("publish_unlisted", "CmdOrCtrl+Shift+Return"),
    ("copy_public_url", "CmdOrCtrl+Shift+L"),
    // Help
    ("show_help", "CmdOrCtrl+/"),
];

/// Shortcuts the system menu items own (Edit, Window, App menus) plus
/// ⇧⌘/ (⌘?), which AppKit binds to the Help menu's search field once a
/// Help menu is registered. Custom items must not reuse any of these.
#[cfg(test)]
const SYSTEM_ACCELERATORS: &[&str] = &[
    "CmdOrCtrl+Z",
    "CmdOrCtrl+Shift+Z",
    "CmdOrCtrl+X",
    "CmdOrCtrl+C",
    "CmdOrCtrl+V",
    "CmdOrCtrl+A",
    "CmdOrCtrl+H",
    "CmdOrCtrl+Alt+H",
    "CmdOrCtrl+Q",
    "CmdOrCtrl+M",
    "CmdOrCtrl+W",
    "CmdOrCtrl+Ctrl+F",
    "CmdOrCtrl+Shift+/",
    "CmdOrCtrl+`",
];

fn accel(id: &str) -> Option<&'static str> {
    ACCELERATORS.iter().find(|(i, _)| *i == id).map(|(_, a)| *a)
}

/// A custom item whose key equivalent (if any) comes from `ACCELERATORS`.
fn item(handle: &tauri::AppHandle, id: &str, text: &str) -> tauri::Result<MenuItem<tauri::Wry>> {
    MenuItem::with_id(handle, id, text, true, accel(id))
}

// ---------- Context: what's selected, what's showing ----------
//
// The frontend pushes this on every relevant change (`set_menu_context`).
// It lives in a static, not managed state, because `build_app_menu` runs
// before `manage()` (see `recents_path`). And the menubar is rebuilt from
// scratch whenever Open Recent changes, which happens on every file
// selection, so the build applies the stored context too; otherwise a
// rebuild would re-enable everything at exactly the wrong moment.

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuContext {
    /// A post is selected.
    pub has_file: bool,
    /// The selected post is live on the site.
    pub is_live: bool,
    /// Publish as Unlisted makes sense (not live, not already unlisted).
    pub can_publish_unlisted: bool,
    /// Publish Later makes sense (not live, safe, not already scheduled).
    pub can_schedule: bool,
    /// Social cards: blog posts only (dispatch pieces use their chart).
    pub can_generate_og: bool,
    pub sidebar_visible: bool,
    pub inspector_visible: bool,
    pub compact: bool,
    /// The default external editor's name, for "Open in <editor>".
    pub editor: String,
}

impl MenuContext {
    /// Before the webview reports in: nothing selected.
    const fn initial() -> Self {
        Self {
            has_file: false,
            is_live: false,
            can_publish_unlisted: false,
            can_schedule: false,
            can_generate_og: false,
            sidebar_visible: true,
            inspector_visible: true,
            compact: false,
            editor: String::new(),
        }
    }
}

static CONTEXT: Mutex<MenuContext> = Mutex::new(MenuContext::initial());

/// Depth-first lookup; tauri's `Menu::get` only searches one level.
fn find(items: Vec<MenuItemKind<tauri::Wry>>, id: &str) -> Option<MenuItemKind<tauri::Wry>> {
    for kind in items {
        if kind.id().as_ref() == id {
            return Some(kind);
        }
        if let MenuItemKind::Submenu(sub) = &kind {
            if let Some(hit) = find(sub.items().unwrap_or_default(), id) {
                return Some(hit);
            }
        }
    }
    None
}

/// Enable/disable and relabel items to match `ctx`.
fn apply_context(menu: &Menu<tauri::Wry>, ctx: &MenuContext) {
    let items = menu.items().unwrap_or_default();
    let get = |id: &str| find(items.clone(), id);
    let enable = |id: &str, on: bool| {
        if let Some(MenuItemKind::MenuItem(i)) = get(id) {
            let _ = i.set_enabled(on);
        }
    };
    let relabel = |id: &str, text: &str| {
        if let Some(MenuItemKind::MenuItem(i)) = get(id) {
            let _ = i.set_text(text);
        }
    };

    for id in [
        "open_in_obsidian",
        "open_in_editor",
        "reveal_in_finder",
        "copy_path",
        "publish",
    ] {
        enable(id, ctx.has_file);
    }
    for id in ["unpublish", "view_on_site", "copy_public_url", "syndicate"] {
        enable(id, ctx.has_file && ctx.is_live);
    }
    enable("publish_unlisted", ctx.has_file && ctx.can_publish_unlisted);
    enable("publish_later", ctx.has_file && ctx.can_schedule);
    enable("generate_og", ctx.has_file && ctx.can_generate_og);

    relabel(
        "publish",
        if ctx.is_live {
            "Republish\u{2026}"
        } else {
            "Publish\u{2026}"
        },
    );
    let editor = if ctx.editor.is_empty() {
        "Editor"
    } else {
        ctx.editor.as_str()
    };
    relabel("open_in_editor", &format!("Open in {editor}"));
    relabel(
        "toggle_sidebar",
        if ctx.sidebar_visible {
            "Hide Sidebar"
        } else {
            "Show Sidebar"
        },
    );
    relabel(
        "toggle_inspector",
        if ctx.inspector_visible {
            "Hide Inspector"
        } else {
            "Show Inspector"
        },
    );
    if let Some(MenuItemKind::Check(c)) = get("toggle_compact") {
        let _ = c.set_checked(ctx.compact);
    }
}

/// Frontend → menubar: what's selected and what's on screen.
#[tauri::command]
pub fn set_menu_context(app: tauri::AppHandle, ctx: MenuContext) {
    if let Ok(mut slot) = CONTEXT.lock() {
        *slot = ctx.clone();
    }
    if let Some(menu) = app.menu() {
        apply_context(&menu, &ctx);
    }
}

/// Build the application menu bar.
///
/// macOS apps need a proper menu bar for Cmd+C/V/Z to work in text fields.
/// Order and naming follow the HIG: App, File, Edit, View, (app-specific)
/// Publish, Window, Help. "…" marks items that open a sheet or ask first.
pub fn build_app_menu(handle: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    // App menu ("Dispatch")
    let app_menu = Submenu::with_items(
        handle,
        "Dispatch",
        true,
        &[
            &PredefinedMenuItem::about(handle, Some("About Dispatch"), None)?,
            &item(handle, "check_updates", "Check for Updates\u{2026}")?,
            &PredefinedMenuItem::separator(handle)?,
            &item(handle, "settings", "Settings\u{2026}")?,
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
                MenuItem::with_id(
                    handle,
                    format!("recent:{}", path),
                    label,
                    true,
                    None::<&str>,
                )
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

    // File menu. Close Window lives here per the HIG (not in Window).
    let file_menu = Submenu::with_items(
        handle,
        "File",
        true,
        &[
            &item(handle, "new_post", "New Post\u{2026}")?,
            &PredefinedMenuItem::separator(handle)?,
            &item(handle, "open_in_obsidian", "Open in Obsidian")?,
            &item(handle, "open_in_editor", "Open in Editor")?,
            &item(handle, "reveal_in_finder", "Reveal in Finder")?,
            &item(handle, "open_site_preview", "Open Site Preview")?,
            &PredefinedMenuItem::separator(handle)?,
            &open_recent,
            &PredefinedMenuItem::separator(handle)?,
            &item(handle, "copy_path", "Copy File Path")?,
            &PredefinedMenuItem::separator(handle)?,
            &item(handle, "refresh", "Refresh")?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::close_window(handle, None)?,
        ],
    )?;

    // Publish menu — domain-specific actions, the heart of the app. Every
    // publish goes through the review sheet, hence the ellipses.
    let publish_menu = Submenu::with_items(
        handle,
        "Publish",
        true,
        &[
            &item(handle, "publish", "Publish\u{2026}")?,
            &item(handle, "publish_unlisted", "Publish as Unlisted\u{2026}")?,
            &item(handle, "publish_later", "Publish Later\u{2026}")?,
            &PredefinedMenuItem::separator(handle)?,
            &item(handle, "view_on_site", "View on Site")?,
            &item(handle, "copy_public_url", "Copy Public URL")?,
            &item(handle, "syndicate", "Syndicate\u{2026}")?,
            &item(handle, "generate_og", "Generate Social Card\u{2026}")?,
            &PredefinedMenuItem::separator(handle)?,
            &item(handle, "unpublish", "Unpublish\u{2026}")?,
        ],
    )?;

    // Edit menu -- PredefinedMenuItems map to native macOS Edit actions
    // so Cmd+C/V/Z/X/A work in webview text fields. AppKit appends
    // AutoFill / Start Dictation / Emoji & Symbols itself.
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

    // View menu. Show/Hide labels flip with state (apply_context).
    let view_menu = Submenu::with_items(
        handle,
        "View",
        true,
        &[
            &item(handle, "toggle_sidebar", "Hide Sidebar")?,
            // The Preview tab's inspector column (status, actions, details).
            &item(handle, "toggle_inspector", "Hide Inspector")?,
            &CheckMenuItem::with_id(
                handle,
                "toggle_compact",
                "Compact Rows",
                true,
                false,
                accel("toggle_compact"),
            )?,
            &PredefinedMenuItem::separator(handle)?,
            // Tab order in the window matches these numbers, left to right
            // after the Desk (which is `d`, a bare key a menu can't own).
            &item(handle, "panel_desk", "Show Desk")?,
            &item(handle, "panel_preview", "Show Preview")?,
            &item(handle, "panel_media", "Show Media")?,
            &item(handle, "panel_journal", "Show Journal")?,
            &item(handle, "panel_changed", "Show Changed Since Live")?,
            &item(handle, "panel_activity", "Show Activity")?,
            &PredefinedMenuItem::separator(handle)?,
            &item(handle, "search", "Search\u{2026}")?,
            &PredefinedMenuItem::separator(handle)?,
            &PredefinedMenuItem::fullscreen(handle, None)?,
        ],
    )?;

    // Help menu. Registered as NSApp's help menu below, so AppKit adds the
    // search field at the top (⇧⌘/ focuses it) and it searches every menu
    // item by title. That's why no item here may take ⇧⌘/.
    let help_menu = Submenu::with_items(
        handle,
        "Help",
        true,
        &[
            &item(handle, "show_overview", "Dispatch Help")?,
            &item(handle, "show_help", "Keyboard Shortcuts")?,
            &PredefinedMenuItem::separator(handle)?,
            &item(handle, "open_docs", "Dispatch Documentation")?,
            &item(handle, "report_issue", "Report an Issue\u{2026}")?,
        ],
    )?;

    // Window menu — standard macOS Minimize / Zoom.
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

    if let Ok(ctx) = CONTEXT.lock() {
        apply_context(&menu, &ctx);
    }

    Ok(menu)
}

const TRAY_ID: &str = "main";

/// The menu bar extra's menu. Rebuilt with the File menu whenever the recents
/// change (muda menus aren't mutable in place). Ids are `tray_*` so the
/// app-level `handle_menu_event`, which also sees these clicks, ignores them.
fn build_tray_menu(app: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "tray_open", "Open Dispatch", true, None::<&str>)?,
            &MenuItem::with_id(app, "tray_new_post", "New Post", true, None::<&str>)?,
        ],
    )?;

    let recents = read_recents(app);
    if !recents.is_empty() {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        menu.append(&MenuItem::new(app, "Recent", false, None::<&str>)?)?;
        for path in recents.iter().take(5) {
            let name = std::path::Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(path);
            menu.append(&MenuItem::with_id(
                app,
                format!("tray_recent:{}", path),
                name,
                true,
                None::<&str>,
            )?)?;
        }
    }

    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "tray_quit",
        "Quit Dispatch",
        true,
        None::<&str>,
    )?)?;
    Ok(menu)
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Build the menu bar extra: a monochrome template image (so macOS tints it
/// for light/dark and the menu bar's own appearance) and a menu that does work.
pub fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let menu = build_tray_menu(app.handle())?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tauri::include_image!("icons/tray.png"))
        .icon_as_template(true)
        .tooltip("Dispatch")
        .menu(&menu)
        .on_menu_event(|app, event| {
            let id = event.id.as_ref();
            if let Some(path) = id.strip_prefix("tray_recent:") {
                let _ = app.emit("menu-open-recent", path.to_string());
                show_main_window(app);
                return;
            }
            match id {
                "tray_open" => show_main_window(app),
                "tray_new_post" => {
                    let _ = app.emit("menu-new-post", ());
                    show_main_window(app);
                }
                "tray_quit" => {
                    preview::stop_server();
                    app.exit(0);
                }
                _ => {}
            }
        })
        .build(app)?;

    Ok(())
}

/// Menu ids that are forwarded to the frontend as `menu-<id with dashes>`.
/// Kept in one list so a test can check the frontend handles each one.
pub const MENU_EVENT_IDS: &[&str] = &[
    "check_updates",
    "new_post",
    "refresh",
    "toggle_compact",
    "search",
    "open_in_obsidian",
    "open_in_editor",
    "reveal_in_finder",
    "open_site_preview",
    "copy_path",
    "publish",
    "publish_unlisted",
    "publish_later",
    "unpublish",
    "view_on_site",
    "copy_public_url",
    "syndicate",
    "generate_og",
    "toggle_sidebar",
    "toggle_inspector",
    "panel_desk",
    "panel_preview",
    "panel_media",
    "panel_journal",
    "panel_changed",
    "panel_activity",
    "show_help",
    "show_overview",
];

/// Items handled entirely in Rust (no frontend event).
#[cfg(test)]
const RUST_HANDLED_IDS: &[&str] = &[
    "settings",
    "clear_recent_files",
    "open_docs",
    "report_issue",
];

/// Handle custom menu item clicks by emitting events to the frontend.
pub fn handle_menu_event(app: &tauri::AppHandle, event: tauri::menu::MenuEvent) {
    let id = event.id().as_ref();

    // Most items are passthrough emits with a stable naming convention.
    // Every id here MUST have a listener (App.vue or useMenuActions.ts) —
    // an id without one is a dead menu item (and, if it has an accelerator,
    // a dead shortcut, since the menu sees the key before the webview).
    let passthrough = MENU_EVENT_IDS;
    if id == "settings" {
        // The window may be hidden (hide-on-close); Settings lives in it.
        show_main_window(app);
        let _ = app.emit("menu-settings", ());
        return;
    }
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
        "open_docs" => {
            let _ = open_url("https://github.com/ejfox/website-dispatch#readme");
        }
        "report_issue" => {
            let _ = open_url("https://github.com/ejfox/website-dispatch/issues/new");
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
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), build_tray_menu(app)) {
        let _ = tray.set_menu(Some(menu));
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

#[cfg(test)]
mod tests {
    use super::{ACCELERATORS, MENU_EVENT_IDS, RUST_HANDLED_IDS, SYSTEM_ACCELERATORS};

    /// A menu item whose event nobody listens to is dead, and on macOS its
    /// accelerator is dead too (the menu takes the key before the webview).
    /// v0.8.0 shipped eleven of these, including Publish ⌘↩. Each id must be
    /// named in App.vue or useMenuActions.ts.
    #[test]
    fn every_menu_event_has_a_frontend_listener() {
        let frontend = [
            include_str!("../../src/App.vue"),
            include_str!("../../src/composables/useMenuActions.ts"),
        ]
        .concat();
        let mut events: Vec<String> = MENU_EVENT_IDS
            .iter()
            .map(|id| format!("'menu-{}'", id.replace('_', "-")))
            .collect();
        // Emitted by name rather than through the passthrough list.
        events.push("'menu-settings'".into());
        events.push("'menu-open-recent'".into());
        let missing: Vec<String> = events
            .into_iter()
            .filter(|ev| !frontend.contains(ev.as_str()))
            .collect();
        assert!(
            missing.is_empty(),
            "menu events with no listener: {:?}",
            missing
        );
    }

    /// Every custom item built in `build_app_menu` is either forwarded to
    /// the frontend or handled in Rust. Catches an item added to the menu
    /// but to neither list (a click that does nothing). Build custom items
    /// via `item()` so this check sees them.
    #[test]
    fn every_menu_item_is_handled() {
        let src = include_str!("menu.rs");
        let build = src
            .split("pub fn build_app_menu")
            .nth(1)
            .and_then(|s| s.split("const TRAY_ID").next())
            .expect("build_app_menu body");
        let mut unhandled = vec![];
        for chunk in build.split("item(handle, \"").skip(1) {
            let id = chunk.split('"').next().unwrap();
            if !MENU_EVENT_IDS.contains(&id) && !RUST_HANDLED_IDS.contains(&id) {
                unhandled.push(id.to_string());
            }
        }
        assert!(
            unhandled.is_empty(),
            "menu items nobody handles: {:?}",
            unhandled
        );
    }

    /// The exact shape useMenuActions.ts sends. A rename on either side would
    /// otherwise fail silently (the invoke's error is only logged), leaving
    /// every item enabled with default labels.
    #[test]
    fn menu_context_parses_frontend_payload() {
        let ctx: super::MenuContext = serde_json::from_str(
            r#"{"hasFile":true,"isLive":false,"canPublishUnlisted":true,"canSchedule":false,
                "canGenerateOg":true,"sidebarVisible":false,"inspectorVisible":true,
                "compact":true,"editor":"iA Writer"}"#,
        )
        .expect("frontend payload parses");
        assert!(ctx.has_file && !ctx.is_live && !ctx.sidebar_visible && ctx.compact);
        assert_eq!(ctx.editor, "iA Writer");
    }

    /// "Cmd+Shift+O" and "Shift+CmdOrCtrl+O" are the same chord.
    fn chord(a: &str) -> String {
        let mut parts: Vec<String> = a
            .split('+')
            .map(|p| match p {
                "Cmd" | "Command" | "CmdOrCtrl" | "CommandOrControl" => "Cmd".into(),
                "Option" | "Alt" => "Alt".into(),
                other => other.to_ascii_uppercase(),
            })
            .collect();
        parts.sort();
        parts.join("+")
    }

    /// No two items share a shortcut, and none steals a system one.
    #[test]
    fn accelerators_do_not_collide() {
        let mut seen: Vec<(String, &str)> = SYSTEM_ACCELERATORS
            .iter()
            .map(|a| (chord(a), "system"))
            .collect();
        let mut clashes = vec![];
        for (id, a) in ACCELERATORS {
            let c = chord(a);
            if let Some((_, other)) = seen.iter().find(|(s, _)| *s == c) {
                clashes.push(format!("{id} ({a}) vs {other}"));
            }
            seen.push((c, id));
        }
        assert!(clashes.is_empty(), "shortcut collisions: {:?}", clashes);
    }

    /// Every accelerator belongs to a real item (no stale table rows).
    #[test]
    fn accelerators_name_real_items() {
        let src = include_str!("menu.rs");
        let build = src
            .split("pub fn build_app_menu")
            .nth(1)
            .and_then(|s| s.split("const TRAY_ID").next())
            .expect("build_app_menu body");
        let stale: Vec<&str> = ACCELERATORS
            .iter()
            .map(|(id, _)| *id)
            .filter(|id| !build.contains(&format!("\"{id}\"")))
            .collect();
        assert!(
            stale.is_empty(),
            "accelerators for missing items: {:?}",
            stale
        );
    }
}
