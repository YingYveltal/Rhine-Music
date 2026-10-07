use serde::Deserialize;
use tauri::{window::Color, Theme, Window};

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowTheme {
    Day,
    Night,
}

#[tauri::command]
pub async fn set_window_theme(window: Window, theme: WindowTheme) -> Result<(), String> {
    let (appearance, background) = match theme {
        WindowTheme::Day => (Theme::Light, Color(234, 229, 225, 255)),
        WindowTheme::Night => (Theme::Dark, Color(10, 18, 32, 255)),
    };
    // On macOS this sets this application's NSAppearance, never the OS theme.
    // Retain the standard titlebar and its native window controls.
    window
        .set_theme(Some(appearance))
        .map_err(|e| e.to_string())?;
    window
        .set_background_color(Some(background))
        .map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    {
        // Tao sets NSApp.appearance. Give this NSWindow an explicit appearance
        // too, so AppKit resolves native title/controls in the same context.
        let target = window.clone();
        let dark = matches!(theme, WindowTheme::Night);
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        window
            .run_on_main_thread(move || {
                let result = target
                    .ns_window()
                    .map_err(|e| e.to_string())
                    .and_then(|pointer| unsafe { macos::set_appearance(pointer, dark) });
                let _ = send.send(result);
            })
            .map_err(|e| e.to_string())?;
        // Complete the command only after AppKit has applied the request;
        // the frontend's serial queue then preserves rapid toggle ordering.
        tauri::async_runtime::spawn_blocking(move || {
            receive.recv().map_err(|e| e.to_string())?
        })
        .await
        .map_err(|e| e.to_string())??;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::{c_char, c_void};
    type Object = *mut c_void;
    #[link(name = "AppKit", kind = "framework")]
    extern "C" {
        static NSAppearanceNameAqua: Object;
        static NSAppearanceNameDarkAqua: Object;
    }
    #[link(name = "objc")]
    extern "C" {
        fn objc_getClass(name: *const c_char) -> Object;
        fn sel_registerName(name: *const c_char) -> Object;
        #[link_name = "objc_msgSend"]
        fn object_message(receiver: Object, selector: Object, value: Object) -> Object;
        #[link_name = "objc_msgSend"]
        fn void_message(receiver: Object, selector: Object, value: Object);
    }

    // Called only on the AppKit main thread, while the cloned Tauri Window
    // remains alive. These are public NSAppearance/NSWindow APIs; the window
    // retains the named appearance. No native subview or text colour is edited.
    pub unsafe fn set_appearance(window: Object, dark: bool) -> Result<(), String> {
        if window.is_null() {
            return Err("Native window is unavailable".into());
        }
        let class = objc_getClass(c"NSAppearance".as_ptr());
        let name = if dark { NSAppearanceNameDarkAqua } else { NSAppearanceNameAqua };
        let appearance = object_message(class, sel_registerName(c"appearanceNamed:".as_ptr()), name);
        if appearance.is_null() {
            return Err("Native appearance is unavailable".into());
        }
        void_message(window, sel_registerName(c"setAppearance:".as_ptr()), appearance);
        Ok(())
    }
}
