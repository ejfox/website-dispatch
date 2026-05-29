// Native macOS Dock menu — right-click the Dispatch icon in the Dock,
// get "New Post", "Open Search", and the Open Recent list.
//
// Tauri 2.10 / muda 0.17 don't expose `applicationDockMenu:`, and tao
// (the underlying windowing layer) doesn't either. To wire it up without
// replacing tao's delegate (which would break the app), we inject the
// method into tao's delegate class at runtime via
// `objc_sys::class_addMethod`.
//
// On non-macOS this whole module is a no-op stub.

#[cfg(target_os = "macos")]
mod imp {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2::sel;
    use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};
    use objc2_foundation::{MainThreadMarker, NSString};
    use objc_sys::{class_addMethod, objc_class, objc_selector};
    use std::ffi::c_void;
    use std::os::raw::c_char;
    use std::sync::{Mutex, OnceLock};
    use tauri::{AppHandle, Emitter, Manager};

    // Global storage for the AppHandle so the C-callback can reach back into
    // Tauri to emit events / show the window.
    static APP_HANDLE: OnceLock<Mutex<Option<AppHandle>>> = OnceLock::new();

    // The NSMenu pointer we hand back from `applicationDockMenu:`.
    // Stored as `usize` for `Send + Sync`, leaked at install time so AppKit
    // can hold onto it for the process lifetime.
    static DOCK_MENU_PTR: OnceLock<Mutex<usize>> = OnceLock::new();

    // ----- C callbacks injected into the AppDelegate class -----

    // applicationDockMenu: → NSMenu*
    // Signature: id (*)(id self, SEL _cmd, NSApplication *sender)
    unsafe extern "C" fn application_dock_menu(
        _this: *mut AnyObject,
        _cmd: *const objc_selector,
        _sender: *mut AnyObject,
    ) -> *mut AnyObject {
        DOCK_MENU_PTR
            .get()
            .and_then(|m| m.lock().ok().map(|g| *g))
            .unwrap_or(0) as *mut AnyObject
    }

    // dispatchDockAction: — read the menu item's tag, route it.
    // Signature: void (*)(id self, SEL _cmd, NSMenuItem *sender)
    unsafe extern "C" fn dispatch_dock_action(
        _this: *mut AnyObject,
        _cmd: *const objc_selector,
        sender: *mut AnyObject,
    ) {
        if sender.is_null() {
            return;
        }
        let tag: isize = msg_send![sender, tag];
        let handle = APP_HANDLE
            .get()
            .and_then(|m| m.lock().ok().and_then(|g| g.clone()));
        let Some(h) = handle else { return };

        let bring_to_front = || {
            if let Some(w) = h.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
                let _ = w.unminimize();
            }
        };

        match tag {
            1 => {
                let _ = h.emit("menu-new-post", ());
                bring_to_front();
            }
            2 => bring_to_front(),
            3 => {
                let _ = h.emit("menu-search", ());
                bring_to_front();
            }
            t if t >= 10 => {
                let idx = (t - 10) as usize;
                let recents = crate::menu::read_recents(&h);
                if let Some(path) = recents.get(idx).cloned() {
                    let _ = h.emit("menu-open-recent", path);
                    bring_to_front();
                }
            }
            _ => {}
        }
    }

    // ----- Menu construction -----

    fn build_menu(app: &AppHandle, mtm: MainThreadMarker) -> Retained<NSMenu> {
        let menu = NSMenu::new(mtm);

        let action = sel!(dispatchDockAction:);

        // Helper to make a clickable item with a tag.
        let make_item = |title: &str, tag: isize| -> Retained<NSMenuItem> {
            let item = NSMenuItem::new(mtm);
            unsafe {
                item.setTitle(&NSString::from_str(title));
                item.setTag(tag);
                item.setAction(Some(action));
            }
            item
        };

        menu.addItem(&make_item("New Post", 1));
        menu.addItem(&make_item("Open Search\u{2026}", 3));
        menu.addItem(&make_item("Show Dispatch", 2));

        // Recent Files — pulled live at build time.
        let recents = crate::menu::read_recents(app);
        if !recents.is_empty() {
            menu.addItem(&NSMenuItem::separatorItem(mtm));
            let header = NSMenuItem::new(mtm);
            unsafe {
                header.setTitle(&NSString::from_str("Recent"));
                header.setEnabled(false);
            }
            menu.addItem(&header);

            for (i, path) in recents.iter().enumerate().take(8) {
                let name = std::path::Path::new(path)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or(path)
                    .to_string();
                menu.addItem(&make_item(&format!("   {}", name), 10 + i as isize));
            }
        }

        menu
    }

    // ----- Public install -----

    pub fn install(app: &AppHandle) {
        let cell = APP_HANDLE.get_or_init(|| Mutex::new(None));
        if let Ok(mut g) = cell.lock() {
            *g = Some(app.clone());
        }

        // SAFETY: Tauri's setup() runs on the main thread, which is where
        // install() is called from. If that contract changes, NSMenu APIs
        // would abort anyway with a clearer error than this would.
        let mtm = unsafe { MainThreadMarker::new_unchecked() };

        // 1. Build the menu and leak it.
        let menu = build_menu(app, mtm);

        // Get the NSApp delegate; we'll set it as the target of each item
        // AND inject the two methods into its class.
        let nsapp = NSApplication::sharedApplication(mtm);
        let delegate = unsafe {
            // delegate() returns Option<Retained<ProtocolObject<...>>>;
            // we only need the raw pointer to set as target.
            let d: *mut AnyObject = msg_send![&*nsapp, delegate];
            d
        };
        if delegate.is_null() {
            eprintln!("[dock_menu] NSApp has no delegate yet — skipping");
            return;
        }

        // 2. Set each top-level item's target = delegate.
        let items = unsafe { menu.itemArray() };
        let count = items.count();
        for i in 0..count {
            let item = unsafe { items.objectAtIndex(i) };
            unsafe {
                item.setTarget(Some(&*(delegate as *const AnyObject)));
            }
        }

        // 3. Stash the menu pointer for the callback.
        let raw = Retained::into_raw(menu) as *const c_void as usize;
        let cell_m = DOCK_MENU_PTR.get_or_init(|| Mutex::new(0));
        if let Ok(mut g) = cell_m.lock() {
            *g = raw;
        }

        // 4. Inject methods into the delegate's class.
        unsafe {
            let delegate_class: &AnyClass = (*delegate).class();
            let class_ptr: *mut objc_class =
                (delegate_class as *const AnyClass).cast::<objc_class>() as *mut objc_class;

            let imp_dock: unsafe extern "C" fn(
                *mut AnyObject,
                *const objc_selector,
                *mut AnyObject,
            ) -> *mut AnyObject = application_dock_menu;
            // `objc_sys::BOOL` is `bool` on aarch64-apple-darwin but `i8` on
            // x86_64-apple-darwin — the universal build needs both arches to
            // compile. Compare against the platform-correct truth constant
            // (`objc_sys::YES`) so we don't lock to either type.
            let added = class_addMethod(
                class_ptr,
                sel!(applicationDockMenu:).as_ptr(),
                Some(std::mem::transmute::<_, unsafe extern "C" fn()>(imp_dock)),
                b"@@:@\0".as_ptr() as *const c_char,
            );
            if added != objc_sys::YES {
                eprintln!(
                    "[dock_menu] applicationDockMenu: already present on delegate \
                    (later Tauri may add it). Skipping override."
                );
            }

            let imp_act: unsafe extern "C" fn(
                *mut AnyObject,
                *const objc_selector,
                *mut AnyObject,
            ) = dispatch_dock_action;
            class_addMethod(
                class_ptr,
                sel!(dispatchDockAction:).as_ptr(),
                Some(std::mem::transmute::<_, unsafe extern "C" fn()>(imp_act)),
                b"v@:@\0".as_ptr() as *const c_char,
            );
        }
    }
}

#[cfg(target_os = "macos")]
pub use imp::install;

#[cfg(not(target_os = "macos"))]
pub fn install(_app: &tauri::AppHandle) {}
