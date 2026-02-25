//! Window listing and focusing for the Window Switcher feature.
//!
//! Uses CGWindowListCopyWindowInfo for listing and AXUIElement for focusing.

use std::ffi::c_void;

use crate::platform::WindowInfo;
use crate::utils::icon_from_workspace;

// ── CoreGraphics FFI ──────────────────────────────────────────────────────

type CFArrayRef = *const c_void;
type CFDictionaryRef = *const c_void;
type CFStringRef = *const c_void;
type CFNumberRef = *const c_void;
type CFTypeRef = *const c_void;

const K_CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY: u32 = 1 << 0;
const K_CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS: u32 = 1 << 4;
const K_CG_NULL_WINDOW_ID: u32 = 0;

#[allow(clashing_extern_declarations)]
unsafe extern "C" {
    fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> CFArrayRef;
    fn CFArrayGetCount(array: CFArrayRef) -> isize;
    fn CFArrayGetValueAtIndex(array: CFArrayRef, idx: isize) -> CFTypeRef;
    fn CFDictionaryGetValue(dict: CFDictionaryRef, key: CFTypeRef) -> CFTypeRef;
    fn CFStringGetCStringPtr(string: CFStringRef, encoding: u32) -> *const i8;
    fn CFNumberGetValue(number: CFNumberRef, the_type: isize, value_ptr: *mut c_void) -> bool;
    fn CFRelease(cf: CFTypeRef);
}

// CFString encoding
const K_CF_STRING_ENCODING_UTF8: u32 = 0x08000100;

// CFNumber types
const K_CF_NUMBER_INT32_TYPE: isize = 3;

// CoreGraphics window dictionary keys — loaded at runtime
unsafe fn cg_key(name: &[u8]) -> CFStringRef {
    unsafe extern "C" {
        fn CFStringCreateWithCString(
            alloc: *const c_void,
            c_str: *const i8,
            encoding: u32,
        ) -> CFStringRef;
    }
    unsafe {
        CFStringCreateWithCString(
            std::ptr::null(),
            name.as_ptr() as *const i8,
            K_CF_STRING_ENCODING_UTF8,
        )
    }
}

fn cf_string_to_string(cf_str: CFStringRef) -> Option<String> {
    if cf_str.is_null() {
        return None;
    }
    unsafe {
        let c_ptr = CFStringGetCStringPtr(cf_str, K_CF_STRING_ENCODING_UTF8);
        if c_ptr.is_null() {
            // Fallback: use CFStringGetCString
            return cf_string_to_string_fallback(cf_str);
        }
        Some(
            std::ffi::CStr::from_ptr(c_ptr)
                .to_string_lossy()
                .into_owned(),
        )
    }
}

fn cf_string_to_string_fallback(cf_str: CFStringRef) -> Option<String> {
    unsafe extern "C" {
        fn CFStringGetLength(string: CFStringRef) -> isize;
        fn CFStringGetCString(
            string: CFStringRef,
            buffer: *mut i8,
            buffer_size: isize,
            encoding: u32,
        ) -> bool;
    }
    unsafe {
        let len = CFStringGetLength(cf_str);
        if len <= 0 {
            return None;
        }
        let buf_size = (len * 4 + 1) as usize;
        let mut buf: Vec<u8> = vec![0; buf_size];
        if CFStringGetCString(
            cf_str,
            buf.as_mut_ptr() as *mut i8,
            buf_size as isize,
            K_CF_STRING_ENCODING_UTF8,
        ) {
            let c_str = std::ffi::CStr::from_ptr(buf.as_ptr() as *const i8);
            Some(c_str.to_string_lossy().into_owned())
        } else {
            None
        }
    }
}

fn cf_number_to_i32(cf_num: CFNumberRef) -> Option<i32> {
    if cf_num.is_null() {
        return None;
    }
    unsafe {
        let mut value: i32 = 0;
        if CFNumberGetValue(
            cf_num,
            K_CF_NUMBER_INT32_TYPE,
            &mut value as *mut i32 as *mut c_void,
        ) {
            Some(value)
        } else {
            None
        }
    }
}

/// Get a list of all visible user windows.
pub fn get_window_list() -> Vec<WindowInfo> {
    let mut results = Vec::new();

    unsafe {
        let key_owner_name = cg_key(b"kCGWindowOwnerName\0");
        let key_window_name = cg_key(b"kCGWindowName\0");
        let key_owner_pid = cg_key(b"kCGWindowOwnerPID\0");
        let key_window_number = cg_key(b"kCGWindowNumber\0");
        let key_window_layer = cg_key(b"kCGWindowLayer\0");

        let window_list = CGWindowListCopyWindowInfo(
            K_CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY | K_CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS,
            K_CG_NULL_WINDOW_ID,
        );

        if window_list.is_null() {
            return results;
        }

        let count = CFArrayGetCount(window_list);
        let my_pid = std::process::id() as i32;

        for i in 0..count {
            let dict = CFArrayGetValueAtIndex(window_list, i) as CFDictionaryRef;
            if dict.is_null() {
                continue;
            }

            // Check window layer (only layer 0 = normal windows)
            let layer_val = CFDictionaryGetValue(dict, key_window_layer);
            if let Some(layer) = cf_number_to_i32(layer_val) {
                if layer != 0 {
                    continue;
                }
            }

            let owner_pid = match cf_number_to_i32(CFDictionaryGetValue(dict, key_owner_pid)) {
                Some(pid) => pid,
                None => continue,
            };

            // Skip our own windows
            if owner_pid == my_pid {
                continue;
            }

            let window_id = match cf_number_to_i32(CFDictionaryGetValue(dict, key_window_number)) {
                Some(id) => id as u32,
                None => continue,
            };

            let owner_name =
                cf_string_to_string(CFDictionaryGetValue(dict, key_owner_name)).unwrap_or_default();

            let window_title = cf_string_to_string(CFDictionaryGetValue(dict, key_window_name))
                .unwrap_or_default();

            // Skip windows without titles or from system processes
            if window_title.is_empty() {
                continue;
            }

            let skip_owners = [
                "Window Server",
                "Dock",
                "WindowManager",
                "Control Center",
                "Notification Center",
                "SystemUIServer",
            ];
            if skip_owners.contains(&owner_name.as_str()) {
                continue;
            }

            // Try to get icon from the app
            let icon = get_icon_for_pid(owner_pid);

            results.push(WindowInfo {
                window_id,
                owner_pid,
                owner_name,
                window_title,
                icon,
            });
        }

        CFRelease(window_list);
        // Don't forget to release the keys
        CFRelease(key_owner_name);
        CFRelease(key_window_name);
        CFRelease(key_owner_pid);
        CFRelease(key_window_number);
        CFRelease(key_window_layer);
    }

    results
}

fn get_icon_for_pid(pid: i32) -> Option<iced::widget::image::Handle> {
    use objc2_app_kit::NSRunningApplication;

    let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?;

    let url = app.bundleURL()?;
    let path = url.path()?.to_string();
    icon_from_workspace(std::path::Path::new(&path))
}

// ── AXUIElement FFI for focusing windows ──────────────────────────────────

unsafe extern "C" {
    fn AXUIElementCreateApplication(pid: i32) -> *mut c_void;
    fn AXUIElementCopyAttributeValue(
        element: *mut c_void,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXUIElementPerformAction(element: *mut c_void, action: CFStringRef) -> i32;
}

/// Focus a specific window by activating the app and raising the window via AXUIElement.
#[allow(deprecated)]
pub fn focus_window(pid: i32, window_id: u32) {
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};

    // Step 1: Activate the app
    let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid);
    if let Some(app) = app {
        app.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
    }

    // Step 2: Use AXUIElement to raise the specific window
    unsafe {
        let ax_app = AXUIElementCreateApplication(pid);
        if ax_app.is_null() {
            return;
        }

        let attr_windows = cg_key(b"AXWindows\0");
        let mut windows_value: CFTypeRef = std::ptr::null();
        let result = AXUIElementCopyAttributeValue(ax_app, attr_windows, &mut windows_value);

        if result == 0 && !windows_value.is_null() {
            let count = CFArrayGetCount(windows_value as CFArrayRef);

            // Try to match by window title or just raise the first window
            let attr_title = cg_key(b"AXTitle\0");
            let action_raise = cg_key(b"AXRaise\0");

            // Get the target window title from our window_id
            let target_title = get_window_title_by_id(window_id);

            let mut raised = false;
            for i in 0..count {
                let ax_window =
                    CFArrayGetValueAtIndex(windows_value as CFArrayRef, i) as *mut c_void;

                let mut title_value: CFTypeRef = std::ptr::null();
                let _ = AXUIElementCopyAttributeValue(ax_window, attr_title, &mut title_value);

                if !title_value.is_null() {
                    if let Some(title) = cf_string_to_string(title_value) {
                        if let Some(ref target) = target_title {
                            if title == *target {
                                AXUIElementPerformAction(ax_window, action_raise);
                                raised = true;
                                CFRelease(title_value);
                                break;
                            }
                        }
                    }
                    CFRelease(title_value);
                }
            }

            // Fallback: raise the first window if no exact match
            if !raised && count > 0 {
                let ax_window =
                    CFArrayGetValueAtIndex(windows_value as CFArrayRef, 0) as *mut c_void;
                AXUIElementPerformAction(ax_window, action_raise);
            }

            CFRelease(attr_title);
            CFRelease(action_raise);
            CFRelease(windows_value);
        }

        CFRelease(attr_windows);
        CFRelease(ax_app as CFTypeRef);
    }
}

/// Helper: get window title by CGWindowID (for matching against AXWindows)
fn get_window_title_by_id(target_id: u32) -> Option<String> {
    unsafe {
        let key_window_name = cg_key(b"kCGWindowName\0");
        let key_window_number = cg_key(b"kCGWindowNumber\0");

        let window_list = CGWindowListCopyWindowInfo(
            K_CG_WINDOW_LIST_OPTION_ON_SCREEN_ONLY | K_CG_WINDOW_LIST_EXCLUDE_DESKTOP_ELEMENTS,
            K_CG_NULL_WINDOW_ID,
        );
        if window_list.is_null() {
            return None;
        }

        let count = CFArrayGetCount(window_list);
        let mut result = None;

        for i in 0..count {
            let dict = CFArrayGetValueAtIndex(window_list, i) as CFDictionaryRef;
            if dict.is_null() {
                continue;
            }
            if let Some(wid) = cf_number_to_i32(CFDictionaryGetValue(dict, key_window_number)) {
                if wid as u32 == target_id {
                    result = cf_string_to_string(CFDictionaryGetValue(dict, key_window_name));
                    break;
                }
            }
        }

        CFRelease(window_list);
        CFRelease(key_window_name);
        CFRelease(key_window_number);
        result
    }
}
