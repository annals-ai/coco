//! macOS application discovery using Launch Services.
//!
//! This module uses the undocumented `LSCopyAllApplicationURLs` API to enumerate
//! all registered applications on the system. This private API has been stable
//! since macOS 10.5 and is widely used by launcher applications (Alfred, Raycast, etc.).
//!
//! Since the symbol is not exported in Apple's `.tbd` stub files (which only list
//! documented APIs), we load it at runtime via `dlsym` from the LaunchServices
//! framework. If loading fails, we fall back to the cross-platform directory
//! scanning approach.

use core::{
    ffi::{CStr, c_void},
    mem,
    ptr::{self, NonNull},
};
use std::{
    env,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use objc2_core_foundation::{CFArray, CFRetained, CFURL};
use objc2_foundation::{NSBundle, NSNumber, NSString, NSURL, ns_string};
use rayon::iter::{IntoParallelIterator, ParallelIterator as _};

use crate::{
    app::apps::{App, AppCommand},
    commands::Function,
    utils::{icon_from_app_bundle, log_error},
};

use super::super::cross;

/// Function signature for `LSCopyAllApplicationURLs`.
///
/// This undocumented Launch Services function retrieves URLs for all applications
/// registered with the system. It follows Core Foundation's "Copy Rule" - the
/// caller owns the returned `CFArray` and is responsible for releasing it.
///
/// # Parameters
/// - `out`: Pointer to receive the `CFArray<CFURL>` of application URLs
///
/// # Returns
/// - `0` (`noErr`) on success
/// - Non-zero `OSStatus` error code on failure
type LSCopyAllApplicationURLsFn = unsafe extern "C" fn(out: *mut *const CFArray<CFURL>) -> i32;

/// Path to the LaunchServices framework binary within CoreServices.
const LAUNCHSERVICES_PATH: &CStr =
    c"/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/LaunchServices";

/// Logs the last `dlerror` message with a prefix.
///
/// # Safety
///
/// Must be called immediately after a failed `dlopen`/`dlsym` call,
/// before any other dl* functions are invoked.
unsafe fn log_dlerror(prefix: &str) {
    let error = unsafe { libc::dlerror() };
    let message = if error.is_null() {
        "unknown error".into()
    } else {
        unsafe { CStr::from_ptr(error) }.to_string_lossy()
    };

    log_error(&format!("{prefix}: {message}"));
}

/// Dynamically loads `LSCopyAllApplicationURLs` from the LaunchServices framework.
///
/// This function is called once and cached via `LazyLock`. We use dynamic loading
/// because the symbol is undocumented and not present in Apple's `.tbd` stub files,
/// which prevents static linking on modern macOS.
///
/// The library handle is intentionally kept open for the process lifetime since
/// we cache the function pointer.
///
/// # Returns
///
/// The function pointer if successfully loaded, `None` otherwise.
fn load_symbol() -> Option<LSCopyAllApplicationURLsFn> {
    // SAFETY: We pass a valid null-terminated path string to dlopen.
    // RTLD_NOW resolves symbols immediately; RTLD_LOCAL keeps them private.
    let lib = unsafe {
        libc::dlopen(
            LAUNCHSERVICES_PATH.as_ptr(),
            libc::RTLD_NOW | libc::RTLD_LOCAL,
        )
    };

    let Some(lib) = NonNull::new(lib) else {
        // SAFETY: dlopen has returned a null pointer, indicating failure.
        unsafe { log_dlerror("failed to load LaunchServices framework") };
        return None;
    };

    // Clear any prior error before checking dlsym result.
    unsafe { libc::dlerror() };

    // SAFETY: We pass a valid library handle and null-terminated symbol name.
    let sym = unsafe { libc::dlsym(lib.as_ptr(), c"_LSCopyAllApplicationURLs".as_ptr()) };
    let Some(sym) = NonNull::new(sym) else {
        // SAFETY: dlsym has returned a null pointer, indicating failure.
        unsafe { log_dlerror("failed to find symbol `LSCopyAllApplicationURLs`") };

        // SAFETY: lib is a valid handle from successful dlopen.
        unsafe { libc::dlclose(lib.as_ptr()) };
        return None;
    };

    // SAFETY: We've verified the symbol exists. The function signature matches
    // the known (though undocumented) API based on reverse engineering and
    // widespread usage in other applications.
    Some(unsafe { mem::transmute::<*mut c_void, LSCopyAllApplicationURLsFn>(sym.as_ptr()) })
}

/// Retrieves URLs for all applications registered with Launch Services.
///
/// Uses the cached function pointer from [`load_symbol`] to call the
/// undocumented `LSCopyAllApplicationURLs` API.
///
/// # Returns
///
/// `Some(CFRetained<CFArray<CFURL>>)` containing application URLs on success,
/// `None` if the symbol couldn't be loaded or the API call failed.
fn registered_app_urls() -> Option<CFRetained<CFArray<CFURL>>> {
    static SYM: LazyLock<Option<LSCopyAllApplicationURLsFn>> = LazyLock::new(load_symbol);

    let sym = (*SYM)?;
    let mut urls_ptr = ptr::null();

    // SAFETY: We've verified `sym` is a valid function pointer. We pass a valid
    // mutable pointer to receive the output. The function follows the "Copy Rule"
    // so we take ownership of the returned CFArray.
    let err = unsafe { sym(&mut urls_ptr) };

    if err != 0 {
        log_error(&format!(
            "LSCopyAllApplicationURLs failed with error code: {err}"
        ));
        return None;
    }

    let Some(url_ptr) = NonNull::new(urls_ptr.cast_mut()) else {
        log_error("LSCopyAllApplicationURLs returned null on success");
        return None;
    };

    // SAFETY: LSCopyAllApplicationURLs returns a +1 retained CFArray on success.
    // We transfer ownership to CFRetained which will call CFRelease when dropped.
    Some(unsafe { CFRetained::from_raw(url_ptr) })
}

/// Directories that contain user-facing applications.
/// Apps in these directories are included by default (after LSUIElement check).
static USER_APP_DIRECTORIES: LazyLock<&'static [&'static Path]> = LazyLock::new(|| {
    // These strings live for the lifetime of the program, so are safe to leak.
    let items = [
        Path::new("/Applications/"),
        Path::new("/System/Applications/"),
    ];

    let Some(home) = env::var_os("HOME") else {
        return Box::leak(Box::new(items));
    };

    let home_apps = Path::new(&home).join("Applications/");
    let home_apps = PathBuf::leak(home_apps);

    Box::leak(Box::new([items[0], items[1], home_apps]))
});

/// Checks if an app path is in a trusted user-facing application directory.
fn is_in_user_app_directory(path: &Path) -> bool {
    USER_APP_DIRECTORIES
        .iter()
        .any(|directory| path.starts_with(directory))
}

/// Extracts application metadata from a bundle URL.
///
/// Queries the bundle's `Info.plist` for display name and icon, with the
/// following fallback chain for the app name:
/// 1. `CFBundleDisplayName` - localized display name
/// 2. `CFBundleName` - short bundle name
/// 3. File stem from path (e.g., "Safari" from "Safari.app")
///
/// # Returns
///
/// `Some(App)` if the bundle is valid and has a determinable name, `None` otherwise.
fn query_app(url: impl AsRef<NSURL>, store_icons: bool) -> Option<App> {
    let url = url.as_ref();
    let path = url.to_file_path()?;
    if is_nested_inside_another_app(&path) || is_helper_location(&path) {
        return None;
    }

    let bundle = NSBundle::bundleWithURL(url)?;
    let info = bundle.infoDictionary()?;

    let get_string = |key: &NSString| -> Option<String> {
        info.objectForKey(key)?
            .downcast::<NSString>()
            .ok()
            .map(|s| s.to_string())
    };

    let is_truthy = |key: &NSString| -> bool {
        info.objectForKey(key)
            .map(|v| {
                // Check for boolean true or string "1"/"YES"
                v.downcast_ref::<NSNumber>().is_some_and(|n| n.boolValue())
                    || v.downcast_ref::<NSString>().is_some_and(|s| {
                        s.to_string() == "1" || s.to_string().eq_ignore_ascii_case("YES")
                    })
            })
            .unwrap_or(false)
    };

    // Filter out background-only apps (daemons, agents, internal system apps)
    if is_truthy(ns_string!("LSBackgroundOnly")) {
        return None;
    }

    // For apps outside trusted directories, require LSApplicationCategoryType to be set.
    // This filters out internal system apps (SCIM, ShortcutsActions, etc.) while keeping
    // user-facing apps like Finder that happen to live in /System/Library/CoreServices/.
    if !is_in_user_app_directory(&path)
        && get_string(ns_string!("LSApplicationCategoryType")).is_none()
    {
        return None;
    }

    let name = get_string(ns_string!("CFBundleDisplayName"))
        .or_else(|| get_string(ns_string!("CFBundleName")))
        .or_else(|| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })?;

    let icons = store_icons.then(|| icon_from_app_bundle(&path)).flatten();

    let localized_name = read_zh_hans_display_name(&path).filter(|ln| ln != &name);

    // 有中文本地化名时：显示中文名，desc 显示英文原名
    let (display_name, desc) = match &localized_name {
        Some(ln) => (ln.clone(), name.clone()),
        None => (name.clone(), "Application".to_string()),
    };

    Some(App {
        name: display_name,
        name_lc: name.to_lowercase(),
        localized_name,
        desc,
        icons,
        open_command: AppCommand::Function(Function::OpenApp(path.to_string_lossy().into_owned())),
        category: None,
        bundle_path: Some(path.to_string_lossy().into_owned()),
        bundle_id: None,
        pid: None,
    })
}

/// Returns all installed applications discovered via Launch Services.
///
/// Attempts to use the native `LSCopyAllApplicationURLs` API for comprehensive
/// app discovery. If the API is unavailable (symbol not found or call fails),
/// falls back to the cross-platform directory scanning approach.
///
/// # Arguments
///
/// * `store_icons` - Whether to load application icons (slower but needed for display)
pub(crate) fn get_installed_apps(store_icons: bool) -> Vec<App> {
    let Some(registered_app_urls) = registered_app_urls() else {
        log_error("native app discovery unavailable, falling back to directory scan");
        return cross::get_installed_apps(store_icons);
    };

    // Intermediate allocation into a vec allows us to parallelize the iteration, speeding up discovery by ~5x.
    let urls: Vec<_> = registered_app_urls.into_iter().collect();

    let mut apps: Vec<App> = urls
        .into_par_iter()
        .filter_map(|url| query_app(url, store_icons))
        .collect();

    // Dedup by name_lc: Launch Services can return the same app from
    // multiple paths (e.g. /Applications/Foo.app and a dev build copy).
    // Keep the first occurrence (which is typically the /Applications one).
    let mut seen = std::collections::HashSet::new();
    apps.retain(|app| seen.insert(app.name_lc.clone()));

    apps
}

fn is_nested_inside_another_app(app_path: &Path) -> bool {
    // Walk up ancestors; if we find an *.app component that is NOT the last component,
    // then this app is nested inside another app bundle.
    let comps: Vec<_> = app_path.components().collect();
    // Normalize: if path ends with ".../Foo.app", we look for any earlier "*.app".
    for component in comps.iter().take(comps.len().saturating_sub(1)) {
        if let std::path::Component::Normal(name) = component
            && name.to_string_lossy().ends_with(".app")
        {
            return true;
        }
    }
    false
}

fn is_helper_location(path: &Path) -> bool {
    let s = path.to_string_lossy();
    s.contains("/Contents/Library/LoginItems/")
        || s.contains("/Contents/XPCServices/")
        || s.contains("/Contents/Helpers/")
        || s.contains("/Contents/Frameworks/")
        || s.contains("/Library/PrivilegedHelperTools/")
}

/// Reads the Chinese localized CFBundleDisplayName for an app.
///
/// Tries 3 sources in order:
/// 1. `zh-Hans.lproj/InfoPlist.strings` — third-party apps (WeChat, Lark, etc.)
/// 2. `zh_CN.lproj/InfoPlist.strings` — some third-party apps use zh_CN
/// 3. `InfoPlist.loctable` — Apple system apps (Weather, Notes, Calendar, etc.)
fn read_zh_hans_display_name(app_path: &Path) -> Option<String> {
    let resources = app_path.join("Contents/Resources");

    // Try .strings files (zh-Hans and zh_CN)
    for lproj in &["zh-Hans.lproj", "zh_CN.lproj"] {
        let strings_path = resources.join(lproj).join("InfoPlist.strings");
        if let Some(name) = read_strings_file(&strings_path) {
            return Some(name);
        }
    }

    // Try .loctable (Apple system apps)
    let loctable_path = resources.join("InfoPlist.loctable");
    if let Some(name) = read_loctable_display_name(&loctable_path) {
        return Some(name);
    }

    None
}

/// Read localized display name from a .strings file (binary plist, XML plist, or text).
/// Tries CFBundleDisplayName first, then falls back to CFBundleName.
fn read_strings_file(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;

    // Try binary/XML plist first
    if let Ok(dict) = plist::from_bytes::<plist::Dictionary>(&bytes) {
        for key in &["CFBundleDisplayName", "CFBundleName"] {
            if let Some(val) = dict.get(*key) {
                if let Some(s) = val.as_string() {
                    return Some(s.to_owned());
                }
            }
        }
    }

    // Fall back to old-style .strings text format (UTF-16 LE/BE or UTF-8)
    let text = decode_strings_file(&bytes)?;
    parse_strings_value(&text, "CFBundleDisplayName")
        .or_else(|| parse_strings_value(&text, "CFBundleName"))
}

/// Read localized display name from an InfoPlist.loctable (binary plist with all locales).
/// Tries CFBundleDisplayName first, then falls back to CFBundleName.
fn read_loctable_display_name(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let top: plist::Dictionary = plist::from_bytes(&bytes).ok()?;
    for locale in &["zh_CN", "zh-Hans"] {
        if let Some(plist::Value::Dictionary(locale_dict)) = top.get(*locale) {
            for key in &["CFBundleDisplayName", "CFBundleName"] {
                if let Some(plist::Value::String(name)) = locale_dict.get(*key) {
                    return Some(name.clone());
                }
            }
        }
    }
    None
}

/// Decode .strings file bytes to a String, handling UTF-16 BOM and UTF-8.
fn decode_strings_file(bytes: &[u8]) -> Option<String> {
    if bytes.len() >= 2 {
        // UTF-16 LE BOM: FF FE
        if bytes[0] == 0xFF && bytes[1] == 0xFE {
            let u16s: Vec<u16> = bytes[2..]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            return String::from_utf16(&u16s).ok();
        }
        // UTF-16 BE BOM: FE FF
        if bytes[0] == 0xFE && bytes[1] == 0xFF {
            let u16s: Vec<u16> = bytes[2..]
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            return String::from_utf16(&u16s).ok();
        }
    }
    // UTF-8 (with or without BOM)
    let s = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        bytes
    };
    std::str::from_utf8(s).ok().map(|s| s.to_owned())
}

/// Parse a value from old-style .strings text.
/// Supports both `"key" = "value";` and `key = "value";` formats.
fn parse_strings_value(text: &str, key: &str) -> Option<String> {
    let quoted_key = format!("\"{}\"", key);
    for line in text.lines() {
        let trimmed = line.trim();
        // Try quoted key first: "CFBundleDisplayName" = "value";
        let rest = if let Some(r) = trimmed.strip_prefix(&quoted_key) {
            Some(r)
        // Then unquoted key: CFBundleDisplayName = "value";
        } else if let Some(r) = trimmed.strip_prefix(key) {
            // Make sure it's a full key match (next char is whitespace or '=')
            if r.starts_with(|c: char| c == ' ' || c == '\t' || c == '=') {
                Some(r)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(rest) = rest {
            // expect: = "value";
            let rest = rest.trim().strip_prefix('=')?.trim();
            let rest = rest.strip_prefix('"')?;
            let end = rest.find('"')?;
            return Some(rest[..end].to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// 测试 parse_strings_value 支持带引号和不带引号两种格式
    #[test]
    fn test_parse_quoted_key() {
        let text = r#""CFBundleDisplayName" = "微信";"#;
        assert_eq!(
            parse_strings_value(text, "CFBundleDisplayName"),
            Some("微信".to_string())
        );
    }

    #[test]
    fn test_parse_unquoted_key() {
        let text = r#"CFBundleDisplayName = "飞书";"#;
        assert_eq!(
            parse_strings_value(text, "CFBundleDisplayName"),
            Some("飞书".to_string())
        );
    }

    #[test]
    fn test_parse_with_comments() {
        let text = "/* comment */\nCFBundleDisplayName = \"飞书\";\nCFBundleName = \"飞书\";";
        assert_eq!(
            parse_strings_value(text, "CFBundleDisplayName"),
            Some("飞书".to_string())
        );
    }

    #[test]
    fn test_parse_no_match() {
        let text = "CFBundleName = \"Test\";";
        assert_eq!(parse_strings_value(text, "CFBundleDisplayName"), None);
    }

    #[test]
    fn test_parse_no_false_prefix_match() {
        // "CFBundleDisplayNameExtra" should NOT match "CFBundleDisplayName"
        let text = "CFBundleDisplayNameExtra = \"Wrong\";";
        assert_eq!(parse_strings_value(text, "CFBundleDisplayName"), None);
    }

    /// 真实文件系统测试：读取实际安装的 app 的中文名
    /// 这些测试只在对应 app 安装时运行
    #[test]
    fn real_fs_wechat() {
        let path = Path::new("/Applications/WeChat.app");
        if !path.exists() {
            return; // skip if not installed
        }
        let name = read_zh_hans_display_name(path);
        assert_eq!(
            name,
            Some("微信".to_string()),
            "WeChat zh-Hans name should be 微信, got: {name:?}"
        );
    }

    #[test]
    fn real_fs_lark() {
        let path = Path::new("/Applications/Lark.app");
        if !path.exists() {
            return;
        }
        let name = read_zh_hans_display_name(path);
        assert_eq!(
            name,
            Some("飞书".to_string()),
            "Lark zh-Hans name should be 飞书, got: {name:?}"
        );
    }

    #[test]
    fn real_fs_qqmusic() {
        let path = Path::new("/Applications/QQMusic.app");
        if !path.exists() {
            return;
        }
        let name = read_zh_hans_display_name(path);
        assert_eq!(
            name,
            Some("QQ音乐".to_string()),
            "QQMusic zh-Hans name should be QQ音乐, got: {name:?}"
        );
    }

    #[test]
    fn real_fs_neteasemusic() {
        let path = Path::new("/Applications/NeteaseMusic.app");
        if !path.exists() {
            return;
        }
        let name = read_zh_hans_display_name(path);
        assert_eq!(
            name,
            Some("网易云音乐".to_string()),
            "NeteaseMusic zh-Hans name should be 网易云音乐, got: {name:?}"
        );
    }

    #[test]
    fn real_fs_tencent_meeting() {
        let path = Path::new("/Applications/TencentMeeting.app");
        if !path.exists() {
            return;
        }
        let name = read_zh_hans_display_name(path);
        assert_eq!(
            name,
            Some("腾讯会议".to_string()),
            "TencentMeeting zh-Hans name should be 腾讯会议, got: {name:?}"
        );
    }

    // === 系统 app：.loctable 格式 ===

    #[test]
    fn real_fs_weather() {
        let path = Path::new("/System/Applications/Weather.app");
        if !path.exists() {
            return;
        }
        let name = read_zh_hans_display_name(path);
        assert_eq!(
            name,
            Some("天气".to_string()),
            "Weather should be 天气, got: {name:?}"
        );
    }

    #[test]
    fn real_fs_notes() {
        let path = Path::new("/System/Applications/Notes.app");
        if !path.exists() {
            return;
        }
        let name = read_zh_hans_display_name(path);
        assert_eq!(
            name,
            Some("备忘录".to_string()),
            "Notes should be 备忘录, got: {name:?}"
        );
    }

    #[test]
    fn real_fs_calendar() {
        let path = Path::new("/System/Applications/Calendar.app");
        if !path.exists() {
            return;
        }
        let name = read_zh_hans_display_name(path);
        assert_eq!(
            name,
            Some("日历".to_string()),
            "Calendar should be 日历, got: {name:?}"
        );
    }

    #[test]
    fn real_fs_calculator() {
        let path = Path::new("/System/Applications/Calculator.app");
        if !path.exists() {
            return;
        }
        let name = read_zh_hans_display_name(path);
        assert_eq!(
            name,
            Some("计算器".to_string()),
            "Calculator should be 计算器, got: {name:?}"
        );
    }

    #[test]
    fn real_fs_maps() {
        let path = Path::new("/System/Applications/Maps.app");
        if !path.exists() {
            return;
        }
        let name = read_zh_hans_display_name(path);
        assert_eq!(
            name,
            Some("地图".to_string()),
            "Maps should be 地图, got: {name:?}"
        );
    }

    #[test]
    fn real_fs_photos() {
        let path = Path::new("/System/Applications/Photos.app");
        if !path.exists() {
            return;
        }
        let name = read_zh_hans_display_name(path);
        assert_eq!(
            name,
            Some("照片".to_string()),
            "Photos should be 照片, got: {name:?}"
        );
    }

    #[test]
    fn real_fs_reminders() {
        let path = Path::new("/System/Applications/Reminders.app");
        if !path.exists() {
            return;
        }
        let name = read_zh_hans_display_name(path);
        assert_eq!(
            name,
            Some("提醒事项".to_string()),
            "Reminders should be 提醒事项, got: {name:?}"
        );
    }

    #[test]
    fn real_fs_system_settings() {
        let path = Path::new("/System/Applications/System Settings.app");
        if !path.exists() {
            return;
        }
        let name = read_zh_hans_display_name(path);
        assert_eq!(
            name,
            Some("系统设置".to_string()),
            "System Settings should be 系统设置, got: {name:?}"
        );
    }

    #[test]
    fn real_fs_safari_no_zh() {
        let path = Path::new("/Applications/Safari.app");
        if !path.exists() {
            return;
        }
        let name = read_zh_hans_display_name(path);
        if let Some(ref n) = name {
            assert!(!n.is_empty());
        }
    }
}
