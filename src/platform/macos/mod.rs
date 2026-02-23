//! Macos specific logic, such as window settings, etc.
mod discovery;
mod haptics;

use std::sync::atomic::{AtomicUsize, Ordering};

use iced::wgpu::rwh::WindowHandle;

pub(super) use self::discovery::get_installed_apps;
pub(super) use self::haptics::perform_haptic;

/// Raw pointer to the blur child NSWindow, stored as usize for Send/Sync.
static BLUR_WINDOW: AtomicUsize = AtomicUsize::new(0);

/// This sets the activation policy of the app to Accessory, allowing rustcast to be visible ontop
/// of fullscreen apps
pub(super) fn set_activation_policy_accessory() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApp, NSApplicationActivationPolicy};

    let mtm = MainThreadMarker::new().expect("must be on main thread");
    let app = NSApp(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
}

/// This carries out the window configuration for the macos window (only things that are macos specific)
pub(super) fn macos_window_config(handle: &WindowHandle) {
    use iced::wgpu::rwh::RawWindowHandle;
    use objc2::rc::Retained;
    use objc2_app_kit::NSView;

    match handle.as_raw() {
        RawWindowHandle::AppKit(handle) => {
            let ns_view = handle.ns_view.as_ptr();
            let ns_view: Retained<NSView> = unsafe { Retained::retain(ns_view.cast()) }.unwrap();
            let ns_window = ns_view
                .window()
                .expect("view was not installed in a window");

            use objc2_app_kit::{NSFloatingWindowLevel, NSWindowCollectionBehavior};
            ns_window.setLevel(NSFloatingWindowLevel);
            ns_window.setCollectionBehavior(NSWindowCollectionBehavior::CanJoinAllSpaces);

            // Prevent flickering during window resize:
            // Set window background to fully transparent so resize intermediate frames
            // don't flash a default opaque background color.
            fix_resize_flash(&ns_window);
        }
        _ => {
            panic!(
                "Why are you running this as a non-appkit window? this is a macos only app as of now"
            );
        }
    }
}

/// Set the NSWindow's background to clear and non-opaque to prevent
/// flashing during dynamic resize (e.g. when search results change).
fn fix_resize_flash(window: &objc2_app_kit::NSWindow) {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};

    unsafe {
        // [NSColor clearColor]
        let ns_color_class = AnyClass::get(c"NSColor").expect("NSColor class not found");
        let clear_color: *mut AnyObject = msg_send![ns_color_class, clearColor];

        // [window setBackgroundColor: clearColor]
        let _: () = msg_send![window, setBackgroundColor: clear_color];

        // [window setOpaque: NO]
        let no: bool = false;
        let _: () = msg_send![window, setOpaque: no];
    }
}

/// This is the function that forces focus onto rustcast
#[allow(deprecated)]
pub(super) fn focus_this_app() {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApp;

    let mtm = MainThreadMarker::new().expect("must be on main thread");
    let app = NSApp(mtm);

    app.activateIgnoringOtherApps(true);
}

/// This is the struct that represents the process serial number, allowing us to transform the process to a UI element
#[repr(C)]
struct ProcessSerialNumber {
    low: u32,
    hi: u32,
}

/// This is the function that transforms the process to a UI element, and hides the dock icon
///
/// see mostly <https://github.com/electron/electron/blob/e181fd040f72becd135db1fa977622b81da21643/shell/browser/browser_mac.mm#L512C1-L532C2>
///
/// returns ApplicationServices OSStatus (u32)
///
/// doesn't seem to do anything if you haven't opened a window yet, so wait to call it until after that.
pub(super) fn transform_process_to_ui_element() -> u32 {
    use objc2_application_services::{
        TransformProcessType, kCurrentProcess, kProcessTransformToUIElementApplication,
    };
    use std::ptr;

    let psn = ProcessSerialNumber {
        low: 0,
        hi: kCurrentProcess,
    };

    unsafe {
        TransformProcessType(
            ptr::from_ref(&psn).cast(),
            kProcessTransformToUIElementApplication,
        )
    }
}

// ── Blur child window ────────────────────────────────────────────────────

use objc2_foundation::NSRect;

fn make_rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect {
        origin: objc2_foundation::NSPoint { x, y },
        size: objc2_foundation::NSSize { width: w, height: h },
    }
}

/// Create a borderless child NSWindow with NSVisualEffectView, positioned
/// behind the main window. Only the child window provides blur, so
/// resizing it does NOT trigger wgpu surface recreation (zero flicker).
pub(super) fn create_blur_child_window(handle: &WindowHandle, width: f64, content_height: f64) {
    use iced::wgpu::rwh::RawWindowHandle;
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_app_kit::NSView;

    match handle.as_raw() {
        RawWindowHandle::AppKit(handle) => {
            let ns_view = handle.ns_view.as_ptr();
            let ns_view: Retained<NSView> =
                unsafe { Retained::retain(ns_view.cast()) }.unwrap();
            let parent = ns_view
                .window()
                .expect("view was not installed in a window");

            unsafe {
                let parent_frame: NSRect = msg_send![&*parent, frame];

                // Child covers the TOP of the parent (macOS origin = bottom-left)
                let child_frame = make_rect(
                    parent_frame.origin.x,
                    parent_frame.origin.y + parent_frame.size.height - content_height,
                    width,
                    content_height,
                );

                // NSWindow alloc + initWithContentRect:styleMask:backing:defer:
                let cls = AnyClass::get(c"NSWindow").unwrap();
                let child: *mut AnyObject = msg_send![cls, alloc];
                let style: usize = 0; // NSWindowStyleMaskBorderless
                let backing: usize = 2; // NSBackingStoreBuffered
                let defer: bool = false;
                let child: *mut AnyObject = msg_send![
                    child,
                    initWithContentRect: child_frame,
                    styleMask: style,
                    backing: backing,
                    defer: defer
                ];

                // Transparent, no shadow, non-opaque
                let color_cls = AnyClass::get(c"NSColor").unwrap();
                let clear: *mut AnyObject = msg_send![color_cls, clearColor];
                let _: () = msg_send![child, setBackgroundColor: clear];
                let no: bool = false;
                let _: () = msg_send![child, setOpaque: no];
                let _: () = msg_send![child, setHasShadow: no];

                // Same window level as parent
                let level: isize = msg_send![&*parent, level];
                let _: () = msg_send![child, setLevel: level];

                // NSVisualEffectView
                let ve_cls = AnyClass::get(c"NSVisualEffectView").unwrap();
                let ve: *mut AnyObject = msg_send![ve_cls, alloc];
                let ve_frame = make_rect(0.0, 0.0, width, content_height);
                let ve: *mut AnyObject = msg_send![ve, initWithFrame: ve_frame];

                // Material: HUDWindow = 13
                let _: () = msg_send![ve, setMaterial: 13_isize];
                // BlendingMode: BehindWindow = 0
                let _: () = msg_send![ve, setBlendingMode: 0_isize];
                // State: Active = 1
                let _: () = msg_send![ve, setState: 1_isize];

                // Round corners (match contents_style radius)
                let yes: bool = true;
                let _: () = msg_send![ve, setWantsLayer: yes];
                let layer: *mut AnyObject = msg_send![ve, layer];
                let _: () = msg_send![layer, setCornerRadius: 14.0_f64];
                let _: () = msg_send![layer, setMasksToBounds: yes];

                // Set as content view
                let _: () = msg_send![child, setContentView: ve];

                // Add as child window, ordered Below (-1)
                let _: () = msg_send![&*parent, addChildWindow: child, ordered: -1_isize];

                // Show
                let null: *const AnyObject = std::ptr::null();
                let _: () = msg_send![child, orderFront: null];

                // Store for later resize
                BLUR_WINDOW.store(child as usize, Ordering::Relaxed);
            }
        }
        _ => {}
    }
}

/// Resize the blur child window to match the new content height.
/// Called from the update function — synchronous, main-thread safe.
pub(super) fn resize_blur_window(content_height: f64, width: f64) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let ptr = BLUR_WINDOW.load(Ordering::Relaxed);
    if ptr == 0 {
        return;
    }

    unsafe {
        let child = ptr as *mut AnyObject;

        // Get parent frame to position child at the top
        let parent: *mut AnyObject = msg_send![child, parentWindow];
        if parent.is_null() {
            return;
        }
        let parent_frame: NSRect = msg_send![parent, frame];

        let new_frame = make_rect(
            parent_frame.origin.x,
            parent_frame.origin.y + parent_frame.size.height - content_height,
            width,
            content_height,
        );

        let yes: bool = true;
        let _: () = msg_send![child, setFrame: new_frame, display: yes];

        // Also resize the NSVisualEffectView content
        let content: *mut AnyObject = msg_send![child, contentView];
        let ve_frame = make_rect(0.0, 0.0, width, content_height);
        let _: () = msg_send![content, setFrame: ve_frame];
    }
}

/// Clear the stored blur window pointer (call when hiding/closing main window).
pub(super) fn clear_blur_window() {
    BLUR_WINDOW.store(0, Ordering::Relaxed);
}
