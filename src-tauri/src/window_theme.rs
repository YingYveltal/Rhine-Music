use serde::Deserialize;
use tauri::{window::Color, Theme, Window};

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowTheme {
    Day,
    Night,
}

#[tauri::command]
pub fn set_window_theme(window: Window, theme: WindowTheme) -> Result<(), String> {
    let (appearance, background) = match theme {
        WindowTheme::Day => (Theme::Light, Color(234, 229, 225, 255)),
        WindowTheme::Night => (Theme::Dark, Color(10, 18, 32, 255)),
    };
    // On macOS this sets this application's NSAppearance, never the OS theme.
    // Retain the standard titlebar and its native window controls.
    window.set_theme(Some(appearance)).map_err(|e| e.to_string())?;
    window.set_background_color(Some(background)).map_err(|e| e.to_string())
}
