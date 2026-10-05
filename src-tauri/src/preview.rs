//! The preview and its QA profile are opt-in; ordinary builds retain their identity.
use anyhow::{ensure, Result};
use std::{path::PathBuf, sync::OnceLock};
use tauri::Manager;

pub fn qa() -> bool {
    static QA: OnceLock<bool> = OnceLock::new();
    *QA.get_or_init(|| std::env::var("RHINE_PREVIEW_QA").as_deref() == Ok("1"))
}
fn profile(qa: bool) -> (&'static str, [u8; 16]) {
    if qa {
        ("com.rhine.music.preview.qa", [0x5c,0x46,0xfa,0x23,0x6a,0x5b,0x4f,0x36,0xa2,0x8c,0x0d,0x9a,0x48,0xc3,0x0f,0x72])
    } else {
        ("com.rhine.music.preview", [0x95,0x3e,0xf4,0x1c,0x7a,0x04,0x42,0xda,0xb9,0x89,0x3b,0xb2,0xe6,0x71,0x05,0xa8])
    }
}
pub fn identity() -> (&'static str, [u8; 16]) { profile(qa()) }
fn store_uuid(bytes: [u8; 16]) -> String {
    let hex = bytes.iter().map(|b|format!("{b:02X}")).collect::<String>();
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

// Read back the actual WKWebsiteDataStore, not merely the requested config.
#[link(name = "objc")]
extern "C" {
    fn sel_registerName(name: *const std::ffi::c_char) -> *mut std::ffi::c_void;
    #[link_name = "objc_msgSend"]
    fn message(receiver: *mut std::ffi::c_void, selector: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
}
unsafe fn actual_store(mut object: *mut std::ffi::c_void) -> Option<String> {
    for name in [c"configuration", c"websiteDataStore", c"identifier", c"UUIDString", c"UTF8String"] {
        object = message(object, sel_registerName(name.as_ptr()));
        if object.is_null() { return None }
    }
    Some(std::ffi::CStr::from_ptr(object.cast()).to_string_lossy().into_owned())
}

pub fn setup(app: &tauri::App) -> Result<PathBuf> {
    ensure!(app.config().identifier == "com.rhine.music.preview", "Preview requires tauri.preview.conf.json");
    ensure!(app.config().app.windows.iter().all(|w| !w.create), "Preview windows must be created with isolated WebKit storage");
    // Do not inherit another app's bootstrap account or data-directory overrides.
    std::env::remove_var("RHINE_QQ_SESSION");
    std::env::remove_var("QQMUSIC_API_KEY");
    let (service, store) = identity();
    let data = app.path().app_data_dir()?.with_file_name(service);
    std::fs::create_dir_all(&data)?;
    for config in &app.config().app.windows {
        let window = tauri::WebviewWindowBuilder::from_config(app.handle(), config)?
            .data_store_identifier(store).build()?;
        if qa() { window.set_title("Rhine Music Preview · QA")?; }
        let report = data.join(format!("preview-runtime-{}.json", config.label));
        let expected = store_uuid(store);
        let data = data.clone();
        window.with_webview(move |view| {
            let actual = unsafe { actual_store(view.inner()) };
            let verified = actual.as_deref() == Some(expected.as_str());
            let identity = serde_json::json!({"bundleIdentifier":"com.rhine.music.preview","qa":qa(),
                "dataDirectory":data,"keychainService":service,"keychainAccount":"connection",
                "expectedWebkitStore":expected,"actualWebkitStore":actual,"isolated":verified});
            std::fs::write(report, serde_json::to_vec_pretty(&identity).unwrap()).expect("write preview identity");
            assert!(verified, "Preview WebKit isolation requires macOS 14 or later");
        })?;
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_and_qa_have_distinct_data_keychain_and_webkit_identities() {
        assert_ne!(profile(false).0, "com.rhine.music.qq");
        assert_ne!(profile(true).0, profile(false).0);
        assert_ne!(profile(true).1, profile(false).1);
        assert_eq!(store_uuid(profile(false).1), "953EF41C-7A04-42DA-B989-3BB2E67105A8");
        let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.preview.conf.json")).unwrap();
        assert_eq!(config["identifier"], profile(false).0);
        assert_eq!(config["app"]["windows"][0]["create"], false);
        assert_eq!(config["bundle"]["macOS"]["minimumSystemVersion"], "14.0");
        let ordinary: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(ordinary["identifier"], "com.rhine.music.qq");
        assert_eq!(ordinary["bundle"]["macOS"]["minimumSystemVersion"], "12.0");
    }
}
