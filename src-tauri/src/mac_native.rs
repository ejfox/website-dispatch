// Native macOS NSWindow hooks Tauri 2 doesn't surface itself.
//
// We use these for two real-Mac-app touches:
//
//   - **Proxy icon**  — `NSWindow.setRepresentedFilename:` puts the file's
//     icon in the titlebar. Cmd-clicking the title then shows the full path
//     stack (Finder, every text editor, Mail's attachment compose window).
//
//   - **Dirty dot**   — `NSWindow.setDocumentEdited:` puts a black dot in
//     the red close button to signal "this window has unsaved changes."
//     We map it to "the selected post has been modified since publish."
//
// Both are macOS-only AppKit methods. Tauri 2.10 / muda 0.17 don't expose
// them, so we drop down to objc2 and send the messages ourselves. The
// `*mut c_void` we get from `Window::ns_window()` is a real `NSWindow *`,
// so this is just normal Cocoa from there.

#[cfg(target_os = "macos")]
mod imp {
    use objc2::msg_send;
    use objc2_app_kit::NSWindow;
    use objc2_foundation::NSString;
    use std::ffi::c_void;

    /// Set the dot-in-close-button "document edited" flag on the window.
    /// Safe no-op on a null pointer.
    pub fn set_document_edited(ns_window: *mut c_void, edited: bool) {
        if ns_window.is_null() {
            return;
        }
        unsafe {
            let win: &NSWindow = &*(ns_window as *const NSWindow);
            let _: () = msg_send![win, setDocumentEdited: edited];
        }
    }

    /// Set the proxy icon / represented filename in the titlebar.
    /// Pass an absolute path; pass `""` to clear it.
    pub fn set_represented_filename(ns_window: *mut c_void, path: &str) {
        if ns_window.is_null() {
            return;
        }
        unsafe {
            let win: &NSWindow = &*(ns_window as *const NSWindow);
            let nspath = NSString::from_str(path);
            let _: () = msg_send![win, setRepresentedFilename: &*nspath];
        }
    }
}

#[cfg(target_os = "macos")]
pub use imp::*;

#[cfg(not(target_os = "macos"))]
pub fn set_document_edited(_ns_window: *mut std::ffi::c_void, _edited: bool) {}

#[cfg(not(target_os = "macos"))]
pub fn set_represented_filename(_ns_window: *mut std::ffi::c_void, _path: &str) {}
