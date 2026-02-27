//! Macos specific logic, such as window settings, etc.
mod discovery;
mod haptics;
pub(super) mod windows;

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use iced::wgpu::rwh::WindowHandle;
use objc2_core_graphics::CGColor;

pub(super) use self::discovery::get_installed_apps;
pub(super) use self::haptics::perform_haptic;

macro_rules! coco_log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("/Users/kcsx/coco_debug.log")
        {
            let _ = writeln!(
                f,
                "[{:.3}] [macos] {}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64()
                    % 10000.0,
                format!($($arg)*)
            );
        }
    }};
}

/// Raw pointer to the blur child NSWindow, stored as usize for Send/Sync.
static BLUR_WINDOW: AtomicUsize = AtomicUsize::new(0);

/// Raw pointer to the agent blur child NSWindow.
static AGENT_BLUR_WINDOW: AtomicUsize = AtomicUsize::new(0);

/// Raw pointers for native clipboard preview panel and its subviews.
static CLIPBOARD_PREVIEW_PANEL: AtomicUsize = AtomicUsize::new(0);
static CLIPBOARD_PREVIEW_TEXT_SCROLL: AtomicUsize = AtomicUsize::new(0);
static CLIPBOARD_PREVIEW_TEXT_VIEW: AtomicUsize = AtomicUsize::new(0);
static CLIPBOARD_PREVIEW_IMAGE_VIEW: AtomicUsize = AtomicUsize::new(0);
static CLIPBOARD_PREVIEW_VIDEO_VIEW: AtomicUsize = AtomicUsize::new(0);
static CLIPBOARD_PREVIEW_PLAYER: AtomicUsize = AtomicUsize::new(0);
static PASTE_PERMISSION_WARNING: AtomicBool = AtomicBool::new(false);
static AX_PROMPT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Raw pointer to the main NSWindow, for show/hide animation.
static MAIN_WINDOW: AtomicUsize = AtomicUsize::new(0);
/// Raw pointer to the iced/wgpu NSView used as the main render view.
static MAIN_RENDER_VIEW: AtomicUsize = AtomicUsize::new(0);
static SHOW_ANIM_DONE: AtomicBool = AtomicBool::new(false);
static HIDE_ANIM_DONE: AtomicBool = AtomicBool::new(false);
static SHOW_ANIM_ACTIVE_TOKEN: AtomicU64 = AtomicU64::new(0);
static HIDE_ANIM_ACTIVE_TOKEN: AtomicU64 = AtomicU64::new(0);
static SHOW_ANIM_DONE_TOKEN: AtomicU64 = AtomicU64::new(0);
static HIDE_ANIM_DONE_TOKEN: AtomicU64 = AtomicU64::new(0);
const BLUR_CORNER_RADIUS: f64 = 22.0;
const MAIN_BLUR_VIEW_ALPHA: f64 = 1.0;
const MAIN_GLASS_TINT_ALPHA: f64 = 0.0;
const MAIN_GLASS_BLACK_OVERLAY_ALPHA: f64 = 0.0;
const EDGE_GLASS_HIGHLIGHT_ALPHA: f64 = 0.10;
const EDGE_RING_ALPHA: f64 = 1.0;
const EDGE_RING_WIDTH: f64 = 1.0;
const BLUR_SHADOW_CARRIER_ALPHA: f64 = 0.001;
const BLUR_SHADOW_OPACITY: f32 = 0.36;
const BLUR_SHADOW_RADIUS: f64 = 22.0;
const BLUR_SHADOW_OFFSET_Y: f64 = -2.0;
const CLIPBOARD_PREVIEW_PANEL_WIDTH: f64 = 560.0;
const CLIPBOARD_PREVIEW_PANEL_HEIGHT: f64 = 520.0;
const CLIPBOARD_PREVIEW_PANEL_MARGIN: f64 = 14.0;
const CLIPBOARD_PREVIEW_PANEL_RADIUS: f64 = 16.0;
const CLIPBOARD_PREVIEW_PANEL_PADDING: f64 = 14.0;

/// This sets the activation policy of the app to Accessory, allowing Coco to be visible ontop
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
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
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
            ns_window.setHasShadow(true);

            // Prevent flickering during window resize:
            // Set window background to fully transparent so resize intermediate frames
            // don't flash a default opaque background color.
            fix_resize_flash(&ns_window);
            MAIN_RENDER_VIEW.store(
                Retained::<NSView>::as_ptr(&ns_view)
                    .cast_mut()
                    .cast::<std::ffi::c_void>() as usize,
                Ordering::Relaxed,
            );
            force_main_render_view_transparency(
                &*ns_window as *const _ as *mut AnyObject,
                Retained::<NSView>::as_ptr(&ns_view).cast_mut().cast(),
            );

            // Start slightly lower than default center to better match Spotlight's
            // visual position on screen.
            unsafe {
                let _: () = msg_send![&*ns_window, center];
                let frame: NSRect = msg_send![&*ns_window, frame];
                let origin = objc2_foundation::NSPoint {
                    x: frame.origin.x,
                    y: frame.origin.y - 72.0,
                };
                let _: () =
                    msg_send![(&*ns_window as *const _ as *mut AnyObject), setFrameOrigin: origin];
            }
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

/// Force the NSWindow content view and the iced/wgpu render view layers to be
/// non-opaque, otherwise a black CAMetalLayer background can hide the blur
/// child-window even when the window itself is transparent.
///
fn force_main_render_view_transparency(
    window: *mut objc2::runtime::AnyObject,
    render_view: *mut objc2::runtime::AnyObject,
) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    unsafe fn clear_layer_tree(layer: *mut AnyObject) {
        use objc2::msg_send;
        use objc2::runtime::{AnyClass, AnyObject};
        if layer.is_null() {
            return;
        }

        let no: bool = false;
        let ns_color_class = AnyClass::get(c"NSColor").expect("NSColor class not found");
        let clear: *mut AnyObject = msg_send![ns_color_class, clearColor];
        let cg_clear: *mut CGColor = msg_send![clear, CGColor];
        let _: () = msg_send![layer, setOpaque: no];
        let _: () = msg_send![layer, setBackgroundColor: cg_clear];

        let sublayers: *mut AnyObject = msg_send![layer, sublayers];
        if !sublayers.is_null() {
            let count: usize = msg_send![sublayers, count];
            for i in 0..count {
                let sub: *mut AnyObject = msg_send![sublayers, objectAtIndex: i];
                unsafe { clear_layer_tree(sub) };
            }
        }
    }

    unsafe fn clear_view_layer(view: *mut AnyObject) {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        if view.is_null() {
            return;
        }

        let yes: bool = true;
        // If the view is not layer-backed, this asks AppKit to back it with one.
        // For the wgpu view it is usually already layer-backed.
        let _: () = msg_send![view, setWantsLayer: yes];

        let layer: *mut AnyObject = msg_send![view, layer];
        if layer.is_null() {
            return;
        }

        unsafe { clear_layer_tree(layer) };
    }

    unsafe {
        let content_view: *mut AnyObject = msg_send![window, contentView];
        clear_view_layer(content_view);
        clear_view_layer(render_view);
    }
}

fn refresh_main_render_view_transparency() {
    use objc2::runtime::AnyObject;

    let window_ptr = MAIN_WINDOW.load(Ordering::Relaxed);
    let render_ptr = MAIN_RENDER_VIEW.load(Ordering::Relaxed);
    if window_ptr == 0 || render_ptr == 0 {
        return;
    }

    force_main_render_view_transparency(window_ptr as *mut AnyObject, render_ptr as *mut AnyObject);
}

/// This is the function that forces focus onto Coco
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
        size: objc2_foundation::NSSize {
            width: w,
            height: h,
        },
    }
}

/// Wrap a blur view with a rounded CALayer shadow so the panel keeps
/// floating depth without revealing square window-corner shadows.
unsafe fn wrap_blur_effect_with_shadow(
    effect_view: *mut objc2::runtime::AnyObject,
    effect_frame: NSRect,
) -> *mut objc2::runtime::AnyObject {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};

    if effect_view.is_null() {
        return effect_view;
    }

    let view_cls = AnyClass::get(c"NSView").expect("NSView class missing");
    let color_cls = AnyClass::get(c"NSColor").expect("NSColor class missing");

    let host: *mut AnyObject = msg_send![view_cls, alloc];
    let host: *mut AnyObject = msg_send![host, initWithFrame: effect_frame];
    let _: () = msg_send![host, setAutoresizingMask: 18_usize];

    let shadow_carrier: *mut AnyObject = msg_send![view_cls, alloc];
    let shadow_carrier: *mut AnyObject = msg_send![shadow_carrier, initWithFrame: effect_frame];
    let _: () = msg_send![shadow_carrier, setAutoresizingMask: 18_usize];

    let yes: bool = true;
    let _: () = msg_send![shadow_carrier, setWantsLayer: yes];
    let shadow_layer: *mut AnyObject = msg_send![shadow_carrier, layer];
    if !shadow_layer.is_null() {
        let _: () = msg_send![shadow_layer, setCornerRadius: BLUR_CORNER_RADIUS];
        let _: () = msg_send![shadow_layer, setMasksToBounds: false];

        // Tiny fill establishes a rounded silhouette for the layer shadow.
        let fill: *mut AnyObject = msg_send![
            color_cls,
            colorWithWhite: 0.0_f64,
            alpha: BLUR_SHADOW_CARRIER_ALPHA
        ];
        let cg_fill: *mut CGColor = msg_send![fill, CGColor];
        let _: () = msg_send![shadow_layer, setBackgroundColor: cg_fill];

        let shadow_color: *mut AnyObject = msg_send![
            color_cls,
            colorWithWhite: 0.0_f64,
            alpha: 1.0_f64
        ];
        let cg_shadow: *mut CGColor = msg_send![shadow_color, CGColor];
        let _: () = msg_send![shadow_layer, setShadowColor: cg_shadow];
        let _: () = msg_send![shadow_layer, setShadowOpacity: BLUR_SHADOW_OPACITY];
        let _: () = msg_send![shadow_layer, setShadowRadius: BLUR_SHADOW_RADIUS];
        let shadow_offset = objc2_foundation::NSSize {
            width: 0.0,
            height: BLUR_SHADOW_OFFSET_Y,
        };
        let _: () = msg_send![shadow_layer, setShadowOffset: shadow_offset];
    }

    let _: () = msg_send![effect_view, setFrame: effect_frame];
    let _: () = msg_send![effect_view, setAutoresizingMask: 18_usize];

    let _: () = msg_send![host, addSubview: shadow_carrier];
    let _: () = msg_send![host, addSubview: effect_view];
    host
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
            let ns_view: Retained<NSView> = unsafe { Retained::retain(ns_view.cast()) }.unwrap();
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

                // Transparent, non-opaque.
                // Window shadow stays off; we render rounded layer shadow ourselves
                // to avoid square-corner leakage.
                let color_cls = AnyClass::get(c"NSColor").unwrap();
                let clear: *mut AnyObject = msg_send![color_cls, clearColor];
                let _: () = msg_send![child, setBackgroundColor: clear];
                let no: bool = false;
                let _: () = msg_send![child, setOpaque: no];
                let _: () = msg_send![child, setHasShadow: no];

                // Same window level as parent
                let level: isize = msg_send![&*parent, level];
                let _: () = msg_send![child, setLevel: level];

                let effect_frame = make_rect(0.0, 0.0, width, content_height);
                let effect_view: *mut AnyObject = if let Some(glass_cls) =
                    AnyClass::get(c"NSGlassEffectView__DISABLED_FOR_DEBUG")
                {
                    coco_log!(
                        "create_blur_child_window: NSGlassEffectView path h={:.1} w={:.1} alpha={:.2} black={:.2} edge={:.2}",
                        content_height,
                        width,
                        MAIN_BLUR_VIEW_ALPHA,
                        MAIN_GLASS_BLACK_OVERLAY_ALPHA,
                        EDGE_RING_ALPHA
                    );
                    let view_cls = AnyClass::get(c"NSView").unwrap();
                    let color_cls = AnyClass::get(c"NSColor").unwrap();

                    let make_glass = |style: isize,
                                      alpha: f64,
                                      set_black_tint: bool|
                     -> *mut AnyObject {
                        let glass: *mut AnyObject = msg_send![glass_cls, alloc];
                        let glass: *mut AnyObject = msg_send![glass, initWithFrame: effect_frame];
                        let _: () = msg_send![glass, setStyle: style];
                        let _: () = msg_send![glass, setCornerRadius: BLUR_CORNER_RADIUS];
                        let _: () = msg_send![glass, setAlphaValue: alpha];
                        let _: () = msg_send![glass, setAutoresizingMask: 18_usize];
                        if set_black_tint {
                            let tint: *mut AnyObject = msg_send![
                                color_cls,
                                colorWithWhite: 0.0_f64,
                                alpha: MAIN_GLASS_TINT_ALPHA
                            ];
                            let _: () = msg_send![glass, setTintColor: tint];
                        }

                        // NSGlassEffectView is designed around an embedded contentView.
                        // Provide a clear filler view so AppKit renders the glass shell
                        // (including edge highlights) consistently.
                        let filler: *mut AnyObject = msg_send![view_cls, alloc];
                        let filler: *mut AnyObject = msg_send![filler, initWithFrame: effect_frame];
                        let _: () = msg_send![filler, setAutoresizingMask: 18_usize];
                        let _: () = msg_send![glass, setContentView: filler];
                        glass
                    };

                    if let Some(container_cls) = AnyClass::get(c"NSGlassEffectContainerView") {
                        coco_log!("create_blur_child_window: using NSGlassEffectContainerView");
                        let container: *mut AnyObject = msg_send![container_cls, alloc];
                        let container: *mut AnyObject =
                            msg_send![container, initWithFrame: effect_frame];
                        let _: () = msg_send![container, setAutoresizingMask: 18_usize];

                        let host: *mut AnyObject = msg_send![view_cls, alloc];
                        let host: *mut AnyObject = msg_send![host, initWithFrame: effect_frame];
                        let _: () = msg_send![host, setAutoresizingMask: 18_usize];
                        let _: () = msg_send![container, setContentView: host];

                        // Background glass body + native black overlay + clear
                        // glass overlay to recover edge sheen.
                        let bg_glass = make_glass(0_isize, MAIN_BLUR_VIEW_ALPHA, true); // Regular
                        let black_overlay: *mut AnyObject = msg_send![view_cls, alloc];
                        let black_overlay: *mut AnyObject =
                            msg_send![black_overlay, initWithFrame: effect_frame];
                        let _: () = msg_send![black_overlay, setAutoresizingMask: 18_usize];
                        let yes: bool = true;
                        let _: () = msg_send![black_overlay, setWantsLayer: yes];
                        let overlay_layer: *mut AnyObject = msg_send![black_overlay, layer];
                        if !overlay_layer.is_null() {
                            let _: () =
                                msg_send![overlay_layer, setCornerRadius: BLUR_CORNER_RADIUS];
                            let _: () = msg_send![overlay_layer, setMasksToBounds: yes];
                            let black_fill: *mut AnyObject = msg_send![
                                color_cls,
                                colorWithWhite: 0.0_f64,
                                alpha: MAIN_GLASS_BLACK_OVERLAY_ALPHA
                            ];
                            let cg_black: *mut CGColor = msg_send![black_fill, CGColor];
                            let _: () = msg_send![overlay_layer, setBackgroundColor: cg_black];
                            // Stable visible border on top of the black overlay.
                            let border_color: *mut AnyObject = msg_send![
                                color_cls,
                                colorWithWhite: 1.0_f64,
                                alpha: EDGE_RING_ALPHA
                            ];
                            let cg_border: *mut CGColor = msg_send![border_color, CGColor];
                            let _: () = msg_send![overlay_layer, setBorderColor: cg_border];
                            let _: () = msg_send![overlay_layer, setBorderWidth: EDGE_RING_WIDTH];
                        }

                        let edge_glass = make_glass(1_isize, 0.0_f64, false); // Clear (diagnostic off)
                        let edge_tint: *mut AnyObject = msg_send![
                            color_cls,
                            colorWithWhite: 1.0_f64,
                            alpha: EDGE_GLASS_HIGHLIGHT_ALPHA
                        ];
                        let _: () = msg_send![edge_glass, setTintColor: edge_tint];

                        // Explicit edge highlight ring (CoreAnimation layer) to
                        // make the glass border visible in this child-window
                        // composition. The glass body itself remains native.
                        let edge_ring_frame = make_rect(
                            EDGE_RING_WIDTH,
                            EDGE_RING_WIDTH,
                            (width - EDGE_RING_WIDTH * 2.0).max(0.0),
                            (content_height - EDGE_RING_WIDTH * 2.0).max(0.0),
                        );
                        let edge_ring: *mut AnyObject = msg_send![view_cls, alloc];
                        let edge_ring: *mut AnyObject =
                            msg_send![edge_ring, initWithFrame: effect_frame];
                        let _: () = msg_send![edge_ring, setAutoresizingMask: 18_usize];
                        let _: () = msg_send![edge_ring, setWantsLayer: yes];
                        let _: () = msg_send![edge_ring, setFrame: edge_ring_frame];
                        let ring_layer: *mut AnyObject = msg_send![edge_ring, layer];
                        if !ring_layer.is_null() {
                            let ring_radius = (BLUR_CORNER_RADIUS - EDGE_RING_WIDTH).max(0.0);
                            let _: () = msg_send![ring_layer, setCornerRadius: ring_radius];
                            let _: () = msg_send![ring_layer, setMasksToBounds: false];
                            let clear_fill: *mut AnyObject = msg_send![
                                color_cls,
                                colorWithWhite: 0.0_f64,
                                alpha: 0.0_f64
                            ];
                            let cg_clear: *mut CGColor = msg_send![clear_fill, CGColor];
                            let _: () = msg_send![ring_layer, setBackgroundColor: cg_clear];
                            let ring_color: *mut AnyObject = msg_send![
                                color_cls,
                                colorWithWhite: 1.0_f64,
                                alpha: EDGE_RING_ALPHA
                            ];
                            let cg_ring: *mut CGColor = msg_send![ring_color, CGColor];
                            let _: () = msg_send![ring_layer, setBorderColor: cg_ring];
                            let _: () = msg_send![ring_layer, setBorderWidth: EDGE_RING_WIDTH];
                        }

                        let _: () = msg_send![host, addSubview: bg_glass];
                        let _: () = msg_send![host, addSubview: black_overlay];
                        let _: () = msg_send![host, addSubview: edge_glass];
                        let _: () = msg_send![host, addSubview: edge_ring];

                        container
                    } else {
                        coco_log!(
                            "create_blur_child_window: NSGlassEffectContainerView unavailable, single-glass fallback"
                        );
                        // macOS 26 class present but container class unavailable:
                        // single glass fallback.
                        make_glass(0_isize, MAIN_BLUR_VIEW_ALPHA, true)
                    }
                } else {
                    coco_log!("create_blur_child_window: NSVisualEffectView fallback");
                    // Fallback for pre-macOS 26: classic visual effect view.
                    let ve_cls = AnyClass::get(c"NSVisualEffectView").unwrap();
                    let ve: *mut AnyObject = msg_send![ve_cls, alloc];
                    let ve: *mut AnyObject = msg_send![ve, initWithFrame: effect_frame];
                    // Popover (6) + Active (1) gives the lightest native blur in this child-window setup.
                    let _: () = msg_send![ve, setMaterial: 6_isize];
                    let _: () = msg_send![ve, setBlendingMode: 0_isize];
                    let _: () = msg_send![ve, setState: 1_isize];
                    let _: () = msg_send![ve, setEmphasized: no];
                    let _: () = msg_send![ve, setAlphaValue: MAIN_BLUR_VIEW_ALPHA];

                    let yes: bool = true;
                    let _: () = msg_send![ve, setWantsLayer: yes];
                    let layer: *mut AnyObject = msg_send![ve, layer];
                    let _: () = msg_send![layer, setCornerRadius: BLUR_CORNER_RADIUS];
                    let _: () = msg_send![layer, setMasksToBounds: yes];
                    let _: () = msg_send![ve, setAutoresizingMask: 18_usize];
                    ve
                };

                // Set as content view with rounded custom shadow wrapper.
                let wrapped_effect_view = wrap_blur_effect_with_shadow(effect_view, effect_frame);
                let _: () = msg_send![child, setContentView: wrapped_effect_view];

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

/// Legacy resize — directly sets blur child frame (fallback for first-frame init).
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
        let _: () = msg_send![child, invalidateShadow];
        coco_log!(
            "resize_blur_window -> h={:.1} w={:.1} parent_h={:.1} child_y={:.1}",
            content_height,
            width,
            parent_frame.size.height,
            new_frame.origin.y
        );

        // Also resize the NSVisualEffectView content
        let content: *mut AnyObject = msg_send![child, contentView];
        let ve_frame = make_rect(0.0, 0.0, width, content_height);
        let _: () = msg_send![content, setFrame: ve_frame];
    }
}

/// Resize the main window while keeping its top edge visually fixed.
///
/// Iced's generic resize path can resize around the window origin, which makes
/// the search field appear to jump when result height changes. For Spotlight-
/// style panels we want the top edge to stay pinned and the bottom edge to move.
pub(super) fn resize_main_window_top_anchored(height: f64, width: f64) -> bool {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let ptr = MAIN_WINDOW.load(Ordering::Relaxed);
    if ptr == 0 {
        return false;
    }

    unsafe {
        let window = ptr as *mut AnyObject;
        let old_frame: NSRect = msg_send![window, frame];
        let top_y = old_frame.origin.y + old_frame.size.height;
        let new_frame = make_rect(old_frame.origin.x, top_y - height, width, height);
        let yes: bool = true;
        let _: () = msg_send![window, setFrame: new_frame, display: yes];
        refresh_main_render_view_transparency();
        coco_log!(
            "resize_main_window_top_anchored old=({:.1},{:.1},{:.1},{:.1}) new=({:.1},{:.1},{:.1},{:.1})",
            old_frame.origin.x,
            old_frame.origin.y,
            old_frame.size.width,
            old_frame.size.height,
            new_frame.origin.x,
            new_frame.origin.y,
            new_frame.size.width,
            new_frame.size.height
        );
    }

    true
}

// ── Agent blur child window ──────────────────────────────────────────────

/// Create a blur child window for the agent chat window (same technique as main blur).
pub(super) fn create_agent_blur_window(handle: &WindowHandle, width: f64, height: f64) {
    use iced::wgpu::rwh::RawWindowHandle;
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_app_kit::NSView;

    match handle.as_raw() {
        RawWindowHandle::AppKit(handle) => {
            let ns_view = handle.ns_view.as_ptr();
            let ns_view: Retained<NSView> = unsafe { Retained::retain(ns_view.cast()) }.unwrap();
            let parent = ns_view
                .window()
                .expect("view was not installed in a window");

            unsafe {
                let parent_frame: NSRect = msg_send![&*parent, frame];

                let child_frame =
                    make_rect(parent_frame.origin.x, parent_frame.origin.y, width, height);

                let cls = AnyClass::get(c"NSWindow").unwrap();
                let child: *mut AnyObject = msg_send![cls, alloc];
                let style: usize = 0;
                let backing: usize = 2;
                let defer: bool = false;
                let child: *mut AnyObject = msg_send![
                    child,
                    initWithContentRect: child_frame,
                    styleMask: style,
                    backing: backing,
                    defer: defer
                ];

                let color_cls = AnyClass::get(c"NSColor").unwrap();
                let clear: *mut AnyObject = msg_send![color_cls, clearColor];
                let _: () = msg_send![child, setBackgroundColor: clear];
                let no: bool = false;
                let _: () = msg_send![child, setOpaque: no];
                let yes: bool = true;
                let _: () = msg_send![child, setHasShadow: yes];

                let level: isize = msg_send![&*parent, level];
                let _: () = msg_send![child, setLevel: level];

                let ve_cls = AnyClass::get(c"NSVisualEffectView").unwrap();
                let ve: *mut AnyObject = msg_send![ve_cls, alloc];
                let ve_frame = make_rect(0.0, 0.0, width, height);
                let ve: *mut AnyObject = msg_send![ve, initWithFrame: ve_frame];

                let _: () = msg_send![ve, setMaterial: 13_isize];
                let _: () = msg_send![ve, setBlendingMode: 0_isize];
                let _: () = msg_send![ve, setState: 1_isize];

                let yes: bool = true;
                let _: () = msg_send![ve, setWantsLayer: yes];
                let layer: *mut AnyObject = msg_send![ve, layer];
                let _: () = msg_send![layer, setCornerRadius: 12.0_f64];
                let _: () = msg_send![layer, setMasksToBounds: yes];

                let _: () = msg_send![child, setContentView: ve];
                let _: () = msg_send![&*parent, addChildWindow: child, ordered: -1_isize];

                let null: *const AnyObject = std::ptr::null();
                let _: () = msg_send![child, orderFront: null];

                AGENT_BLUR_WINDOW.store(child as usize, Ordering::Relaxed);
            }
        }
        _ => {}
    }
}

/// Clear the agent blur window pointer.
pub(super) fn clear_agent_blur_window() {
    AGENT_BLUR_WINDOW.store(0, Ordering::Relaxed);
}

// ── Running applications ─────────────────────────────────────────────────

/// Get a list of currently running user applications (excluding background/system processes).
pub(super) fn get_running_apps(store_icons: bool) -> Vec<crate::app::apps::App> {
    use crate::app::apps::{App, AppCommand};
    use crate::commands::Function;
    use crate::utils::icon_from_workspace;
    use objc2_app_kit::{NSApplicationActivationPolicy, NSWorkspace};
    use std::path::Path;

    let workspace = NSWorkspace::sharedWorkspace();
    let running = workspace.runningApplications();

    let mut apps = Vec::new();
    let my_pid = std::process::id() as i32;
    let count = running.count();

    for i in 0..count {
        let ra: objc2::rc::Retained<objc2_app_kit::NSRunningApplication> = running.objectAtIndex(i);

        // Only include regular (foreground) apps
        if ra.activationPolicy() != NSApplicationActivationPolicy::Regular {
            continue;
        }

        let pid = ra.processIdentifier();
        if pid == my_pid {
            continue;
        }

        // Get the app name
        let name: String = match ra.localizedName() {
            Some(n) => n.to_string(),
            None => continue,
        };

        // Filter out system processes
        let skip_names = [
            "Finder",
            "SystemUIServer",
            "loginwindow",
            "Dock",
            "WindowManager",
            "Control Center",
            "Notification Center",
        ];
        if skip_names.contains(&name.as_str()) {
            continue;
        }

        // Get icon
        let icon = if store_icons {
            ra.bundleURL()
                .and_then(|url: objc2::rc::Retained<objc2_foundation::NSURL>| {
                    let path_str = url.path()?.to_string();
                    icon_from_workspace(Path::new(&path_str))
                })
        } else {
            None
        };

        // Get bundle path for the desc
        let bundle_path = ra
            .bundleURL()
            .and_then(|url: objc2::rc::Retained<objc2_foundation::NSURL>| {
                Some(url.path()?.to_string())
            })
            .unwrap_or_default();

        apps.push(App {
            open_command: AppCommand::Function(Function::ActivateApp(pid)),
            desc: bundle_path.clone(),
            icons: icon,
            name,
            name_lc: String::new(),
            localized_name: None,
            category: Some(crate::app::apps::AppCategory::Running),
            bundle_path: Some(bundle_path),
            bundle_id: None,
            pid: Some(pid),
        });
    }

    apps
}

/// Activate a running application by PID.
#[allow(deprecated)]
pub(super) fn activate_app_by_pid(pid: i32) {
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};

    let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid);
    if let Some(app) = app {
        app.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
    }
}

/// Hide a running application by PID.
pub(super) fn hide_app_by_pid(pid: i32) {
    use objc2_app_kit::NSRunningApplication;

    let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid);
    if let Some(app) = app {
        let _ = app.hide();
    }
}

/// Quit (terminate) a running application by PID.
pub(super) fn quit_app_by_pid(pid: i32) {
    use objc2_app_kit::NSRunningApplication;

    let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid);
    if let Some(app) = app {
        let _ = app.terminate();
    }
}

/// Force quit a running application by PID.
pub(super) fn force_quit_app_by_pid(pid: i32) {
    use objc2_app_kit::NSRunningApplication;

    let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid);
    if let Some(app) = app {
        let _ = app.forceTerminate();
    }
}

/// Reveal a path in Finder.
pub(super) fn reveal_in_finder(path: &str) {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::NSString;

    let ws = NSWorkspace::sharedWorkspace();
    let ns_path = NSString::from_str(path);
    let url = objc2_foundation::NSURL::fileURLWithPath(&ns_path);
    ws.activateFileViewerSelectingURLs(&objc2_foundation::NSArray::from_retained_slice(&[url]));
}

/// Paste current clipboard content into the frontmost input target via Cmd+V.
pub(super) fn paste_to_frontmost(target_pid: Option<i32>) {
    std::thread::spawn(move || {
        // Let hide + app activation settle before dispatching Cmd+V.
        std::thread::sleep(std::time::Duration::from_millis(24));
        let ax_trusted = ax_process_trusted();
        coco_log!(
            "paste_to_frontmost start target_pid={:?} ax_trusted={}",
            target_pid,
            ax_trusted
        );
        let mut permission_denied = false;
        let frontmost_ready = wait_for_frontmost_target(target_pid);
        coco_log!(
            "paste_to_frontmost frontmost_ready={} target_pid={:?}",
            frontmost_ready,
            target_pid
        );

        // AX route is much faster than spawning osascript when trusted.
        if ax_trusted {
            let ax_ok = post_cmd_v_with_ax(target_pid);
            coco_log!(
                "paste_to_frontmost ax primary ok={} target_pid={:?}",
                ax_ok,
                target_pid
            );
            if ax_ok {
                PASTE_PERMISSION_WARNING.store(false, Ordering::Relaxed);
                return;
            }
        } else {
            coco_log!("paste_to_frontmost ax primary skipped: AXIsProcessTrusted=false");
            request_accessibility_permission_prompt_once();
        }

        let script = r#"
tell application "System Events"
    repeat 12 times
        try
            set frontName to name of first application process whose frontmost is true
            if frontName is not "coco" and frontName is not "Coco" then
                exit repeat
            end if
        end try
        delay 0.01
    end repeat
    keystroke "v" using command down
end tell
"#;
        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output();

        let osascript_ok = match &output {
            Ok(o) => {
                let stderr = String::from_utf8_lossy(&o.stderr).trim().to_string();
                let stdout = String::from_utf8_lossy(&o.stdout).trim().to_string();
                permission_denied =
                    osascript_permission_denied(&stderr) || osascript_permission_denied(&stdout);
                coco_log!(
                    "paste_to_frontmost osascript code={:?} denied={} target_pid={:?} stdout={:?} stderr={:?}",
                    o.status.code(),
                    permission_denied,
                    target_pid,
                    stdout,
                    stderr
                );
                o.status.success()
            }
            Err(err) => {
                coco_log!("paste_to_frontmost osascript launch failed: {err}");
                false
            }
        };

        if osascript_ok {
            PASTE_PERMISSION_WARNING.store(false, Ordering::Relaxed);
            return;
        }

        let cg_ok = post_cmd_v_with_cg_events();
        coco_log!("paste_to_frontmost cgevent fallback ok={cg_ok}");
        if cg_ok {
            PASTE_PERMISSION_WARNING.store(false, Ordering::Relaxed);
        } else {
            let should_warn = permission_denied || !ax_trusted;
            coco_log!(
                "paste_to_frontmost failed denied={} ax_trusted={} => warning={}",
                permission_denied,
                ax_trusted,
                should_warn
            );
            PASTE_PERMISSION_WARNING.store(should_warn, Ordering::Relaxed);
        }
    });
}

fn ax_process_trusted() -> bool {
    unsafe { objc2_application_services::AXIsProcessTrusted() }
}

fn wait_for_frontmost_target(target_pid: Option<i32>) -> bool {
    use objc2_app_kit::NSWorkspace;

    for attempt in 0..26 {
        let ws = NSWorkspace::sharedWorkspace();
        if let Some(app) = ws.frontmostApplication() {
            let pid = app.processIdentifier();
            let name = app
                .localizedName()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "<unknown>".to_string());
            let is_coco = name.eq_ignore_ascii_case("coco");
            let ready = match target_pid {
                Some(expected) if expected > 0 => pid == expected,
                _ => !is_coco,
            };
            if ready {
                return true;
            }
            if attempt == 0 || attempt == 25 {
                coco_log!(
                    "wait_for_frontmost_target attempt={} frontmost pid={} name={} expected={:?}",
                    attempt,
                    pid,
                    name,
                    target_pid
                );
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    false
}

fn request_accessibility_permission_prompt_once() {
    if AX_PROMPT_REQUESTED.swap(true, Ordering::Relaxed) {
        return;
    }
    request_accessibility_permission_prompt();
}

fn request_accessibility_permission_prompt() {
    use objc2_application_services::{AXIsProcessTrustedWithOptions, kAXTrustedCheckOptionPrompt};
    use objc2_core_foundation::{CFBoolean, CFDictionary, CFString, kCFBooleanTrue};

    let Some(true_value) = (unsafe { kCFBooleanTrue }) else {
        coco_log!("request_accessibility_permission_prompt: kCFBooleanTrue unavailable");
        return;
    };

    let prompt_key: &CFString = unsafe { kAXTrustedCheckOptionPrompt };
    let options = CFDictionary::<CFString, CFBoolean>::from_slices(&[prompt_key], &[true_value]);
    let options_typed: &CFDictionary<CFString, CFBoolean> = &options;
    let options_untyped: &objc2_core_foundation::CFDictionary = options_typed.as_ref();
    let trusted = unsafe { AXIsProcessTrustedWithOptions(Some(options_untyped)) };
    coco_log!("request_accessibility_permission_prompt trusted_after_call={trusted}");
}

fn post_cmd_v_with_ax(_target_pid: Option<i32>) -> bool {
    use objc2_application_services::{AXError, AXUIElement};
    use objc2_core_graphics::{CGCharCode, CGKeyCode};

    const KEY_COMMAND: CGKeyCode = 0x37;
    const KEY_V: CGKeyCode = 0x09;
    const CHAR_NONE: CGCharCode = 0;
    let app = unsafe { AXUIElement::new_system_wide() };

    #[allow(deprecated)]
    fn post(app: &AXUIElement, key_char: CGCharCode, key_code: CGKeyCode, down: bool) -> AXError {
        unsafe { app.post_keyboard_event(key_char, key_code, down) }
    }

    let cmd_down = post(&app, CHAR_NONE, KEY_COMMAND, true);
    let v_down = post(&app, CHAR_NONE, KEY_V, true);
    let v_up = post(&app, CHAR_NONE, KEY_V, false);
    let cmd_up = post(&app, CHAR_NONE, KEY_COMMAND, false);
    coco_log!(
        "post_cmd_v_with_ax errs: cmd_down={:?} v_down={:?} v_up={:?} cmd_up={:?}",
        cmd_down,
        v_down,
        v_up,
        cmd_up
    );

    cmd_down == AXError::Success
        && v_down == AXError::Success
        && v_up == AXError::Success
        && cmd_up == AXError::Success
}

fn post_cmd_v_with_cg_events() -> bool {
    use objc2_core_graphics::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode};

    const KEY_V: CGKeyCode = 0x09;
    let preflight = objc2_core_graphics::CGPreflightPostEventAccess();
    coco_log!("post_cmd_v_with_cg_events preflight_post_access={preflight}");
    if !preflight {
        return false;
    }

    let Some(key_down) = CGEvent::new_keyboard_event(None, KEY_V, true) else {
        coco_log!("post_cmd_v_with_cg_events failed: key_down event create");
        return false;
    };
    let Some(key_up) = CGEvent::new_keyboard_event(None, KEY_V, false) else {
        coco_log!("post_cmd_v_with_cg_events failed: key_up event create");
        return false;
    };

    CGEvent::set_flags(Some(&key_down), CGEventFlags::MaskCommand);
    CGEvent::set_flags(Some(&key_up), CGEventFlags::MaskCommand);
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&key_down));
    std::thread::sleep(std::time::Duration::from_millis(12));
    CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&key_up));
    true
}

fn osascript_permission_denied(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("not allowed to send keystrokes")
        || lower.contains("1002")
        || stderr.contains("不允许发送按键")
}

pub(super) fn paste_permission_warning_active() -> bool {
    PASTE_PERMISSION_WARNING.load(Ordering::Relaxed)
}

pub(super) fn accessibility_permission_granted() -> bool {
    ax_process_trusted()
}

/// Preview clipboard content with macOS Quick Look (native space-preview style).
pub(super) fn quick_look_clipboard_content(
    entry_id: u64,
    content: &crate::clipboard::ClipBoardContentType,
) {
    use std::process::{Command, Stdio};

    let dir = std::env::temp_dir().join("coco-quicklook");
    if let Err(err) = std::fs::create_dir_all(&dir) {
        coco_log!("quick look: create temp dir failed: {err}");
        return;
    }

    let path = match content {
        crate::clipboard::ClipBoardContentType::Text(text) => {
            let path = dir.join(format!("clipboard-{entry_id}.txt"));
            if let Err(err) = std::fs::write(&path, text) {
                coco_log!("quick look: write text failed: {err}");
                return;
            }
            path
        }
        crate::clipboard::ClipBoardContentType::Image(img) => {
            let path = dir.join(format!("clipboard-{entry_id}.png"));
            if let Err(err) = image::save_buffer_with_format(
                &path,
                img.bytes.as_ref(),
                img.width as u32,
                img.height as u32,
                image::ColorType::Rgba8,
                image::ImageFormat::Png,
            ) {
                coco_log!("quick look: write image failed: {err}");
                return;
            }
            path
        }
    };

    let _ = Command::new("qlmanage")
        .arg("-p")
        .arg(&path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn clipboard_preview_text(text: &str) -> String {
    const MAX_CHARS: usize = 20_000;
    if text.chars().count() > MAX_CHARS {
        let mut clipped = text.chars().take(MAX_CHARS).collect::<String>();
        clipped.push_str("...");
        clipped
    } else {
        text.to_owned()
    }
}

fn media_video_path_from_text(text: &str) -> Option<std::path::PathBuf> {
    let raw = text.lines().find(|line| !line.trim().is_empty())?.trim();
    if raw.is_empty() {
        return None;
    }

    let path = if let Some(rest) = raw.strip_prefix("file://") {
        if let Some(localhost_path) = rest.strip_prefix("localhost/") {
            std::path::PathBuf::from(format!("/{}", localhost_path))
        } else {
            std::path::PathBuf::from(rest)
        }
    } else {
        std::path::PathBuf::from(raw.trim_matches('"'))
    };

    if !path.exists() || !path.is_file() {
        return None;
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())?;
    let is_video = matches!(
        ext.as_str(),
        "mp4" | "mov" | "m4v" | "webm" | "mkv" | "avi" | "flv"
    );
    if !is_video {
        return None;
    }

    std::fs::canonicalize(path).ok()
}

fn clipboard_preview_panel_size(_parent_height: f64) -> (f64, f64) {
    (
        CLIPBOARD_PREVIEW_PANEL_WIDTH,
        CLIPBOARD_PREVIEW_PANEL_HEIGHT,
    )
}

unsafe fn ensure_clipboard_preview_panel() -> bool {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};

    if CLIPBOARD_PREVIEW_PANEL.load(Ordering::Relaxed) != 0 {
        return true;
    }

    let main_ptr = MAIN_WINDOW.load(Ordering::Relaxed);
    if main_ptr == 0 {
        coco_log!("clipboard preview: main window missing");
        return false;
    }
    let parent = main_ptr as *mut AnyObject;
    let parent_frame: NSRect = unsafe { msg_send![parent, frame] };
    let (panel_w, panel_h) = clipboard_preview_panel_size(parent_frame.size.height);
    let panel_frame = make_rect(0.0, 0.0, panel_w, panel_h);

    let Some(panel_cls) = AnyClass::get(c"NSPanel") else {
        coco_log!("clipboard preview: NSPanel class missing");
        return false;
    };
    let panel: *mut AnyObject = unsafe { msg_send![panel_cls, alloc] };
    let style_mask: usize = 0; // borderless
    let backing: usize = 2; // NSBackingStoreBuffered
    let defer: bool = false;
    let panel: *mut AnyObject = unsafe {
        msg_send![
            panel,
            initWithContentRect: panel_frame,
            styleMask: style_mask,
            backing: backing,
            defer: defer
        ]
    };
    if panel.is_null() {
        coco_log!("clipboard preview: init panel failed");
        return false;
    }

    let color_cls = AnyClass::get(c"NSColor").expect("NSColor class missing");
    let clear: *mut AnyObject = unsafe { msg_send![color_cls, clearColor] };
    let yes: bool = true;
    let no: bool = false;
    let _: () = unsafe { msg_send![panel, setBackgroundColor: clear] };
    let _: () = unsafe { msg_send![panel, setOpaque: no] };
    let _: () = unsafe { msg_send![panel, setHasShadow: yes] };
    let _: () = unsafe { msg_send![panel, setIgnoresMouseEvents: no] };
    let _: () = unsafe { msg_send![panel, setMovableByWindowBackground: no] };
    let _: () = unsafe { msg_send![panel, setReleasedWhenClosed: no] };
    let _: () = unsafe { msg_send![panel, setHidesOnDeactivate: yes] };
    let _: () = unsafe { msg_send![panel, setFloatingPanel: yes] };
    let _: () = unsafe { msg_send![panel, setBecomesKeyOnlyIfNeeded: yes] };

    let level: isize = unsafe { msg_send![parent, level] };
    let _: () = unsafe { msg_send![panel, setLevel: level] };

    let effect_cls = AnyClass::get(c"NSVisualEffectView").expect("NSVisualEffectView missing");
    let effect: *mut AnyObject = unsafe { msg_send![effect_cls, alloc] };
    let effect: *mut AnyObject = unsafe { msg_send![effect, initWithFrame: panel_frame] };
    let _: () = unsafe { msg_send![effect, setMaterial: 6_isize] }; // Popover
    let _: () = unsafe { msg_send![effect, setBlendingMode: 0_isize] };
    let _: () = unsafe { msg_send![effect, setState: 1_isize] };
    let _: () = unsafe { msg_send![effect, setEmphasized: no] };
    let _: () = unsafe { msg_send![effect, setAutoresizingMask: 18_usize] };
    let _: () = unsafe { msg_send![effect, setWantsLayer: yes] };
    let layer: *mut AnyObject = unsafe { msg_send![effect, layer] };
    if !layer.is_null() {
        let _: () = unsafe { msg_send![layer, setCornerRadius: CLIPBOARD_PREVIEW_PANEL_RADIUS] };
        let _: () = unsafe { msg_send![layer, setMasksToBounds: yes] };
        let border: *mut AnyObject = unsafe {
            msg_send![
                color_cls,
                colorWithWhite: 1.0_f64,
                alpha: 0.15_f64
            ]
        };
        let cg_border: *mut CGColor = unsafe { msg_send![border, CGColor] };
        let _: () = unsafe { msg_send![layer, setBorderColor: cg_border] };
        let _: () = unsafe { msg_send![layer, setBorderWidth: 1.0_f64] };
    }

    let content_w = (panel_w - CLIPBOARD_PREVIEW_PANEL_PADDING * 2.0).max(1.0);
    let content_h = (panel_h - CLIPBOARD_PREVIEW_PANEL_PADDING * 2.0).max(1.0);
    let content_frame = make_rect(
        CLIPBOARD_PREVIEW_PANEL_PADDING,
        CLIPBOARD_PREVIEW_PANEL_PADDING,
        content_w,
        content_h,
    );

    let scroll_cls = AnyClass::get(c"NSScrollView").expect("NSScrollView missing");
    let text_scroll: *mut AnyObject = unsafe { msg_send![scroll_cls, alloc] };
    let text_scroll: *mut AnyObject =
        unsafe { msg_send![text_scroll, initWithFrame: content_frame] };
    let _: () = unsafe { msg_send![text_scroll, setAutoresizingMask: 18_usize] };
    let _: () = unsafe { msg_send![text_scroll, setHasVerticalScroller: yes] };
    let _: () = unsafe { msg_send![text_scroll, setHasHorizontalScroller: no] };
    let _: () = unsafe { msg_send![text_scroll, setAutohidesScrollers: yes] };
    let _: () = unsafe { msg_send![text_scroll, setBorderType: 0_isize] };
    let _: () = unsafe { msg_send![text_scroll, setDrawsBackground: no] };

    let text_cls = AnyClass::get(c"NSTextView").expect("NSTextView missing");
    let text_view: *mut AnyObject = unsafe { msg_send![text_cls, alloc] };
    let text_view: *mut AnyObject = unsafe {
        msg_send![
            text_view,
            initWithFrame: make_rect(0.0, 0.0, content_w, content_h)
        ]
    };
    let _: () = unsafe { msg_send![text_view, setAutoresizingMask: 18_usize] };
    let _: () = unsafe { msg_send![text_view, setEditable: no] };
    let _: () = unsafe { msg_send![text_view, setSelectable: yes] };
    let _: () = unsafe { msg_send![text_view, setRichText: no] };
    let _: () = unsafe { msg_send![text_view, setImportsGraphics: no] };
    let _: () = unsafe { msg_send![text_view, setDrawsBackground: no] };
    let _: () = unsafe { msg_send![text_view, setHorizontallyResizable: no] };
    let _: () = unsafe { msg_send![text_view, setVerticallyResizable: yes] };
    let _: () = unsafe { msg_send![text_scroll, setDocumentView: text_view] };

    let image_view_cls = AnyClass::get(c"NSImageView").expect("NSImageView missing");
    let image_view: *mut AnyObject = unsafe { msg_send![image_view_cls, alloc] };
    let image_view: *mut AnyObject = unsafe { msg_send![image_view, initWithFrame: content_frame] };
    let _: () = unsafe { msg_send![image_view, setAutoresizingMask: 18_usize] };
    let _: () = unsafe { msg_send![image_view, setImageScaling: 3_isize] };
    let _: () = unsafe { msg_send![image_view, setImageAlignment: 0_isize] };
    let _: () = unsafe { msg_send![image_view, setHidden: yes] };

    let mut video_view: *mut AnyObject = std::ptr::null_mut();
    if let Some(video_view_cls) = AnyClass::get(c"AVPlayerView") {
        let vv_alloc: *mut AnyObject = unsafe { msg_send![video_view_cls, alloc] };
        video_view = unsafe { msg_send![vv_alloc, initWithFrame: content_frame] };
        if !video_view.is_null() {
            let _: () = unsafe { msg_send![video_view, setAutoresizingMask: 18_usize] };
            let _: () = unsafe { msg_send![video_view, setHidden: yes] };
        }
    } else {
        coco_log!("clipboard preview: AVPlayerView unavailable, video preview disabled");
    }

    let _: () = unsafe { msg_send![effect, addSubview: text_scroll] };
    let _: () = unsafe { msg_send![effect, addSubview: image_view] };
    if !video_view.is_null() {
        let _: () = unsafe { msg_send![effect, addSubview: video_view] };
    }
    let _: () = unsafe { msg_send![panel, setContentView: effect] };

    CLIPBOARD_PREVIEW_PANEL.store(panel as usize, Ordering::Relaxed);
    CLIPBOARD_PREVIEW_TEXT_SCROLL.store(text_scroll as usize, Ordering::Relaxed);
    CLIPBOARD_PREVIEW_TEXT_VIEW.store(text_view as usize, Ordering::Relaxed);
    CLIPBOARD_PREVIEW_IMAGE_VIEW.store(image_view as usize, Ordering::Relaxed);
    CLIPBOARD_PREVIEW_VIDEO_VIEW.store(video_view as usize, Ordering::Relaxed);
    CLIPBOARD_PREVIEW_PLAYER.store(0, Ordering::Relaxed);

    true
}

unsafe fn attach_clipboard_preview_panel_to_main(panel: *mut objc2::runtime::AnyObject) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    if panel.is_null() {
        return;
    }
    let main_ptr = MAIN_WINDOW.load(Ordering::Relaxed);
    if main_ptr == 0 {
        return;
    }
    let main_window = main_ptr as *mut AnyObject;
    let parent: *mut AnyObject = unsafe { msg_send![panel, parentWindow] };
    if !parent.is_null() && parent != main_window {
        let _: () = unsafe { msg_send![parent, removeChildWindow: panel] };
    }
    if parent != main_window {
        let _: () = unsafe { msg_send![main_window, addChildWindow: panel, ordered: 1_isize] };
    }
    let level: isize = unsafe { msg_send![main_window, level] };
    let _: () = unsafe { msg_send![panel, setLevel: level] };
}

unsafe fn position_clipboard_preview_panel(panel: *mut objc2::runtime::AnyObject) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    if panel.is_null() {
        return;
    }
    let main_ptr = MAIN_WINDOW.load(Ordering::Relaxed);
    if main_ptr == 0 {
        return;
    }
    let main_window = main_ptr as *mut AnyObject;
    let parent_frame: NSRect = unsafe { msg_send![main_window, frame] };
    let (panel_w, panel_h) = clipboard_preview_panel_size(parent_frame.size.height);

    let x = parent_frame.origin.x + ((parent_frame.size.width - panel_w) * 0.5).round();
    let y = (parent_frame.origin.y + (parent_frame.size.height - panel_h) * 0.5).round();
    let frame = make_rect(x, y.max(CLIPBOARD_PREVIEW_PANEL_MARGIN), panel_w, panel_h);

    let yes: bool = true;
    let _: () = unsafe { msg_send![panel, setFrame: frame, display: yes] };

    let content: *mut AnyObject = unsafe { msg_send![panel, contentView] };
    if !content.is_null() {
        let _: () = unsafe { msg_send![content, setFrame: make_rect(0.0, 0.0, panel_w, panel_h)] };
    }

    let content_w = (panel_w - CLIPBOARD_PREVIEW_PANEL_PADDING * 2.0).max(1.0);
    let content_h = (panel_h - CLIPBOARD_PREVIEW_PANEL_PADDING * 2.0).max(1.0);
    let content_frame = make_rect(
        CLIPBOARD_PREVIEW_PANEL_PADDING,
        CLIPBOARD_PREVIEW_PANEL_PADDING,
        content_w,
        content_h,
    );
    let text_scroll_ptr = CLIPBOARD_PREVIEW_TEXT_SCROLL.load(Ordering::Relaxed);
    if text_scroll_ptr != 0 {
        let text_scroll = text_scroll_ptr as *mut AnyObject;
        let _: () = unsafe { msg_send![text_scroll, setFrame: content_frame] };
    }
    let image_view_ptr = CLIPBOARD_PREVIEW_IMAGE_VIEW.load(Ordering::Relaxed);
    if image_view_ptr != 0 {
        let image_view = image_view_ptr as *mut AnyObject;
        let _: () = unsafe { msg_send![image_view, setFrame: content_frame] };
    }
    let video_view_ptr = CLIPBOARD_PREVIEW_VIDEO_VIEW.load(Ordering::Relaxed);
    if video_view_ptr != 0 {
        let video_view = video_view_ptr as *mut AnyObject;
        let _: () = unsafe { msg_send![video_view, setFrame: content_frame] };
    }
}

unsafe fn stop_video_preview() {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let player_ptr = CLIPBOARD_PREVIEW_PLAYER.swap(0, Ordering::Relaxed);
    if player_ptr != 0 {
        let player = player_ptr as *mut AnyObject;
        let _: () = unsafe { msg_send![player, pause] };
    }

    let video_view_ptr = CLIPBOARD_PREVIEW_VIDEO_VIEW.load(Ordering::Relaxed);
    if video_view_ptr != 0 {
        let video_view = video_view_ptr as *mut AnyObject;
        let null_player: *mut AnyObject = std::ptr::null_mut();
        let yes: bool = true;
        let _: () = unsafe { msg_send![video_view, setPlayer: null_player] };
        let _: () = unsafe { msg_send![video_view, setHidden: yes] };
    }
}

unsafe fn show_video_preview(path: &std::path::Path) -> bool {
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2_foundation::NSString;

    let text_scroll_ptr = CLIPBOARD_PREVIEW_TEXT_SCROLL.load(Ordering::Relaxed);
    let image_view_ptr = CLIPBOARD_PREVIEW_IMAGE_VIEW.load(Ordering::Relaxed);
    let video_view_ptr = CLIPBOARD_PREVIEW_VIDEO_VIEW.load(Ordering::Relaxed);
    if text_scroll_ptr == 0 || image_view_ptr == 0 || video_view_ptr == 0 {
        return false;
    }
    let text_scroll = text_scroll_ptr as *mut AnyObject;
    let image_view = image_view_ptr as *mut AnyObject;
    let video_view = video_view_ptr as *mut AnyObject;

    let Some(url_cls) = AnyClass::get(c"NSURL") else {
        return false;
    };
    let Some(player_cls) = AnyClass::get(c"AVPlayer") else {
        return false;
    };

    let path_string = path.to_string_lossy().to_string();
    let ns_path = NSString::from_str(&path_string);
    let ns_url: *mut AnyObject = unsafe { msg_send![url_cls, fileURLWithPath: &*ns_path] };
    if ns_url.is_null() {
        return false;
    }
    let player: *mut AnyObject = unsafe { msg_send![player_cls, playerWithURL: ns_url] };
    if player.is_null() {
        return false;
    }

    unsafe { stop_video_preview() };

    let yes: bool = true;
    let no: bool = false;
    let _: () = unsafe { msg_send![text_scroll, setHidden: yes] };
    let _: () = unsafe { msg_send![image_view, setHidden: yes] };
    let _: () = unsafe { msg_send![video_view, setHidden: no] };
    let _: () = unsafe { msg_send![video_view, setPlayer: player] };
    let _: () = unsafe { msg_send![player, play] };
    CLIPBOARD_PREVIEW_PLAYER.store(player as usize, Ordering::Relaxed);
    true
}

unsafe fn show_text_preview(text: &str) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_foundation::NSString;

    unsafe { stop_video_preview() };

    let text_view_ptr = CLIPBOARD_PREVIEW_TEXT_VIEW.load(Ordering::Relaxed);
    let text_scroll_ptr = CLIPBOARD_PREVIEW_TEXT_SCROLL.load(Ordering::Relaxed);
    let image_view_ptr = CLIPBOARD_PREVIEW_IMAGE_VIEW.load(Ordering::Relaxed);
    if text_view_ptr == 0 || text_scroll_ptr == 0 || image_view_ptr == 0 {
        return;
    }

    let text_view = text_view_ptr as *mut AnyObject;
    let text_scroll = text_scroll_ptr as *mut AnyObject;
    let image_view = image_view_ptr as *mut AnyObject;

    let ns_text = NSString::from_str(&clipboard_preview_text(text));
    let _: () = unsafe { msg_send![text_view, setString: &*ns_text] };

    let yes: bool = true;
    let no: bool = false;
    let _: () = unsafe { msg_send![text_scroll, setHidden: no] };
    let _: () = unsafe { msg_send![image_view, setHidden: yes] };

    let clip_view: *mut AnyObject = unsafe { msg_send![text_scroll, contentView] };
    if !clip_view.is_null() {
        let origin = objc2_foundation::NSPoint { x: 0.0, y: 0.0 };
        let _: () = unsafe { msg_send![clip_view, scrollToPoint: origin] };
        let _: () = unsafe { msg_send![text_scroll, reflectScrolledClipView: clip_view] };
    }
}

unsafe fn show_image_preview(img: &arboard::ImageData<'static>) {
    use image::ImageEncoder;
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject};

    unsafe { stop_video_preview() };

    let text_scroll_ptr = CLIPBOARD_PREVIEW_TEXT_SCROLL.load(Ordering::Relaxed);
    let image_view_ptr = CLIPBOARD_PREVIEW_IMAGE_VIEW.load(Ordering::Relaxed);
    if text_scroll_ptr == 0 || image_view_ptr == 0 {
        return;
    }

    let text_scroll = text_scroll_ptr as *mut AnyObject;
    let image_view = image_view_ptr as *mut AnyObject;

    let mut png = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut png);
    if let Err(err) = encoder.write_image(
        img.bytes.as_ref(),
        img.width as u32,
        img.height as u32,
        image::ExtendedColorType::Rgba8,
    ) {
        coco_log!("clipboard preview: encode image failed: {err}");
        return;
    }

    let Some(data_cls) = AnyClass::get(c"NSData") else {
        coco_log!("clipboard preview: NSData class missing");
        return;
    };
    let ns_data: *mut AnyObject = unsafe {
        msg_send![
            data_cls,
            dataWithBytes: png.as_ptr().cast::<std::ffi::c_void>(),
            length: png.len()
        ]
    };
    if ns_data.is_null() {
        coco_log!("clipboard preview: NSData creation failed");
        return;
    }

    let Some(image_cls) = AnyClass::get(c"NSImage") else {
        coco_log!("clipboard preview: NSImage class missing");
        return;
    };
    let ns_image: *mut AnyObject = unsafe { msg_send![image_cls, alloc] };
    let ns_image: *mut AnyObject = unsafe { msg_send![ns_image, initWithData: ns_data] };
    if ns_image.is_null() {
        coco_log!("clipboard preview: NSImage initWithData failed");
        return;
    }

    let _: () = unsafe { msg_send![image_view, setImage: ns_image] };
    let yes: bool = true;
    let no: bool = false;
    let _: () = unsafe { msg_send![text_scroll, setHidden: yes] };
    let _: () = unsafe { msg_send![image_view, setHidden: no] };
}

unsafe fn update_clipboard_preview_content(content: &crate::clipboard::ClipBoardContentType) {
    match content {
        crate::clipboard::ClipBoardContentType::Text(text) => unsafe {
            if let Some(path) = media_video_path_from_text(text)
                && show_video_preview(&path)
            {
                return;
            }
            show_text_preview(text);
        },
        crate::clipboard::ClipBoardContentType::Image(img) => unsafe {
            show_image_preview(img);
        },
    }
}

/// Show the native clipboard preview panel above the launcher window.
pub(super) fn show_clipboard_preview_panel(content: &crate::clipboard::ClipBoardContentType) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    unsafe {
        if !ensure_clipboard_preview_panel() {
            return;
        }
        let panel_ptr = CLIPBOARD_PREVIEW_PANEL.load(Ordering::Relaxed);
        if panel_ptr == 0 {
            return;
        }
        let panel = panel_ptr as *mut AnyObject;
        attach_clipboard_preview_panel_to_main(panel);
        position_clipboard_preview_panel(panel);
        update_clipboard_preview_content(content);
        let null: *const AnyObject = std::ptr::null();
        let _: () = msg_send![panel, orderFront: null];
    }
}

/// Update preview panel content while keeping it visible.
pub(super) fn update_clipboard_preview_panel(content: &crate::clipboard::ClipBoardContentType) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    unsafe {
        if CLIPBOARD_PREVIEW_PANEL.load(Ordering::Relaxed) == 0 {
            show_clipboard_preview_panel(content);
            return;
        }
        let panel = CLIPBOARD_PREVIEW_PANEL.load(Ordering::Relaxed) as *mut AnyObject;
        if panel.is_null() {
            show_clipboard_preview_panel(content);
            return;
        }
        attach_clipboard_preview_panel_to_main(panel);
        position_clipboard_preview_panel(panel);
        update_clipboard_preview_content(content);
        let null: *const AnyObject = std::ptr::null();
        let _: () = msg_send![panel, orderFront: null];
    }
}

/// Hide (but keep) the native clipboard preview panel.
pub(super) fn hide_clipboard_preview_panel() {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let panel_ptr = CLIPBOARD_PREVIEW_PANEL.load(Ordering::Relaxed);
    if panel_ptr == 0 {
        return;
    }

    unsafe {
        stop_video_preview();
        let panel = panel_ptr as *mut AnyObject;
        let parent: *mut AnyObject = msg_send![panel, parentWindow];
        if !parent.is_null() {
            let _: () = msg_send![parent, removeChildWindow: panel];
        }
        let null: *const AnyObject = std::ptr::null();
        let _: () = msg_send![panel, orderOut: null];
    }
}

/// Open System Settings to the Accessibility pane.
pub(super) fn open_accessibility_settings() {
    request_accessibility_permission_prompt_once();
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn();
}

/// Open System Settings to the Input Monitoring pane.
pub(super) fn open_input_monitoring_settings() {
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")
        .spawn();
}

// ── Show/hide animation ──────────────────────────────────────────────────

/// CATransform3D struct for GPU-accelerated scale transforms.
/// Matches the CoreAnimation C struct layout exactly.
#[repr(C)]
#[derive(Copy, Clone)]
struct CATransform3D {
    m11: f64,
    m12: f64,
    m13: f64,
    m14: f64,
    m21: f64,
    m22: f64,
    m23: f64,
    m24: f64,
    m31: f64,
    m32: f64,
    m33: f64,
    m34: f64,
    m41: f64,
    m42: f64,
    m43: f64,
    m44: f64,
}

// SAFETY: CATransform3D is a plain C struct of 16 f64s,
// matching the Objective-C {CATransform3D=dddddddddddddddd} encoding.
unsafe impl objc2::Encode for CATransform3D {
    const ENCODING: objc2::Encoding = objc2::Encoding::Struct(
        "CATransform3D",
        &[
            objc2::Encoding::Double,
            objc2::Encoding::Double,
            objc2::Encoding::Double,
            objc2::Encoding::Double,
            objc2::Encoding::Double,
            objc2::Encoding::Double,
            objc2::Encoding::Double,
            objc2::Encoding::Double,
            objc2::Encoding::Double,
            objc2::Encoding::Double,
            objc2::Encoding::Double,
            objc2::Encoding::Double,
            objc2::Encoding::Double,
            objc2::Encoding::Double,
            objc2::Encoding::Double,
            objc2::Encoding::Double,
        ],
    );
}
unsafe impl objc2::RefEncode for CATransform3D {
    const ENCODING_REF: objc2::Encoding =
        objc2::Encoding::Pointer(&<Self as objc2::Encode>::ENCODING);
}

impl CATransform3D {
    fn identity() -> Self {
        Self {
            m11: 1.0,
            m12: 0.0,
            m13: 0.0,
            m14: 0.0,
            m21: 0.0,
            m22: 1.0,
            m23: 0.0,
            m24: 0.0,
            m31: 0.0,
            m32: 0.0,
            m33: 1.0,
            m34: 0.0,
            m41: 0.0,
            m42: 0.0,
            m43: 0.0,
            m44: 1.0,
        }
    }

    fn scale(sx: f64, sy: f64) -> Self {
        let mut t = Self::identity();
        t.m11 = sx;
        t.m22 = sy;
        t
    }
}

/// Save the main NSWindow pointer for animation. Call from window::run callback.
pub(super) fn store_main_window(handle: &WindowHandle) {
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
            let ptr = &*ns_window as *const _ as usize;
            MAIN_WINDOW.store(ptr, Ordering::Relaxed);
        }
        _ => {}
    }
}

// Fade-only show/hide (no scale) to avoid clipping/desync artifacts.
const SHOW_SCALE_HIDDEN: f64 = 1.0;
const SHOW_ANIM_DURATION_SECS: f64 = 0.200;
const HIDE_ANIM_DURATION_SECS: f64 = 0.150;

unsafe fn for_main_and_blur_windows(mut f: impl FnMut(*mut objc2::runtime::AnyObject)) {
    use objc2::runtime::AnyObject;

    let main_ptr = MAIN_WINDOW.load(Ordering::Relaxed);
    if main_ptr != 0 {
        f(main_ptr as *mut AnyObject);
    }

    let blur_ptr = BLUR_WINDOW.load(Ordering::Relaxed);
    if blur_ptr != 0 {
        f(blur_ptr as *mut AnyObject);
    }
}

unsafe fn window_content_layer(
    window: *mut objc2::runtime::AnyObject,
) -> *mut objc2::runtime::AnyObject {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let content_view: *mut AnyObject = msg_send![window, contentView];
    if content_view.is_null() {
        return std::ptr::null_mut();
    }

    let yes: bool = true;
    let _: () = msg_send![content_view, setWantsLayer: yes];
    let layer: *mut AnyObject = msg_send![content_view, layer];
    if layer.is_null() {
        return std::ptr::null_mut();
    }

    layer
}

unsafe fn remove_window_layer_animations(window: *mut objc2::runtime::AnyObject) {
    use objc2::msg_send;

    let layer = unsafe { window_content_layer(window) };
    if layer.is_null() {
        return;
    }
    let _: () = msg_send![layer, removeAllAnimations];
}

unsafe fn set_window_alpha_immediate(window: *mut objc2::runtime::AnyObject, alpha: f64) {
    use objc2::msg_send;
    let _: () = msg_send![window, setAlphaValue: alpha];
}

unsafe fn set_window_alpha_animated(window: *mut objc2::runtime::AnyObject, alpha: f64) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let animator: *mut AnyObject = msg_send![window, animator];
    if animator.is_null() {
        let _: () = msg_send![window, setAlphaValue: alpha];
        return;
    }
    let _: () = msg_send![animator, setAlphaValue: alpha];
}

unsafe fn snap_all_windows(alpha: f64, scale: f64, clear_layer_animations: bool) {
    let transform = if (scale - 1.0).abs() < f64::EPSILON {
        CATransform3D::identity()
    } else {
        CATransform3D::scale(scale, scale)
    };

    unsafe {
        for_main_and_blur_windows(|window| {
            if clear_layer_animations {
                remove_window_layer_animations(window);
            }
            set_window_alpha_immediate(window, alpha);
            apply_centered_transform(window, &transform);
        });
    }
}

fn run_native_window_animation(
    duration_secs: f64,
    target_alpha: f64,
    target_scale: f64,
    done_flag: &'static AtomicBool,
    active_token: &'static AtomicU64,
    done_token: &'static AtomicU64,
) {
    use block2::StackBlock;
    use core::ptr::NonNull;
    use objc2_app_kit::NSAnimationContext;

    if MAIN_WINDOW.load(Ordering::Relaxed) == 0 {
        done_flag.store(false, Ordering::Relaxed);
        done_token.store(0, Ordering::Relaxed);
        return;
    }

    done_flag.store(false, Ordering::Relaxed);
    done_token.store(0, Ordering::Relaxed);
    let token = active_token.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
    let transform = if (target_scale - 1.0).abs() < f64::EPSILON {
        CATransform3D::identity()
    } else {
        CATransform3D::scale(target_scale, target_scale)
    };

    let changes = StackBlock::new(move |ctx_ptr: NonNull<NSAnimationContext>| {
        let ctx = unsafe { ctx_ptr.as_ref() };
        ctx.setDuration(duration_secs);
        ctx.setAllowsImplicitAnimation(true);
        unsafe {
            for_main_and_blur_windows(|window| {
                set_window_alpha_animated(window, target_alpha);
                apply_centered_transform(window, &transform);
            });
        }
    });

    let completion = StackBlock::new(move || {
        done_token.store(token, Ordering::Relaxed);
        done_flag.store(true, Ordering::Relaxed);
    });

    NSAnimationContext::runAnimationGroup_completionHandler(&*changes, Some(&*completion));
}

/// Apply a CATransform3D to a window's content view layer,
/// ensuring the anchorPoint is centered (0.5, 0.5).
unsafe fn apply_centered_transform(
    window: *mut objc2::runtime::AnyObject,
    transform: &CATransform3D,
) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let content_view: *mut AnyObject = msg_send![window, contentView];
    if content_view.is_null() {
        return;
    }
    let yes: bool = true;
    let _: () = msg_send![content_view, setWantsLayer: yes];
    let layer: *mut AnyObject = msg_send![content_view, layer];
    if layer.is_null() {
        return;
    }

    // With Spotlight-style small-scale animation, keep rounded clipping enabled
    // on both the main window and blur child to avoid square-corner overflow
    // during animation.
    let yes: bool = true;
    let _: () = msg_send![content_view, setClipsToBounds: yes];
    let _: () = msg_send![layer, setMasksToBounds: yes];
    let _: () = msg_send![layer, setCornerRadius: BLUR_CORNER_RADIUS];

    // Ensure anchorPoint is at center (default, but be explicit)
    let center = objc2_foundation::NSPoint { x: 0.5, y: 0.5 };
    let _: () = msg_send![layer, setAnchorPoint: center];

    // The anchorPoint change moves the layer visually; fix by setting
    // position to the center of its bounds.
    let bounds: NSRect = msg_send![layer, bounds];
    let pos = objc2_foundation::NSPoint {
        x: bounds.size.width * 0.5,
        y: bounds.size.height * 0.5,
    };
    let _: () = msg_send![layer, setPosition: pos];

    let _: () = msg_send![layer, setTransform: *transform];
}

/// Set alpha=0 and scale=SHOW_SCALE_HIDDEN before ordering the window front.
pub(super) fn prepare_show_animation() {
    SHOW_ANIM_DONE.store(false, Ordering::Relaxed);
    SHOW_ANIM_DONE_TOKEN.store(0, Ordering::Relaxed);
    unsafe {
        snap_all_windows(0.0, SHOW_SCALE_HIDDEN, true);
    }
}

/// Animate alpha 0→1 and scale SHOW_SCALE_HIDDEN→1.0 using NSAnimationContext.
pub(super) fn animate_show() {
    run_native_window_animation(
        SHOW_ANIM_DURATION_SECS,
        1.0,
        1.0,
        &SHOW_ANIM_DONE,
        &SHOW_ANIM_ACTIVE_TOKEN,
        &SHOW_ANIM_DONE_TOKEN,
    );
}

/// Animate alpha 1→0 and scale 1.0→SHOW_SCALE_HIDDEN using NSAnimationContext.
pub(super) fn animate_hide() {
    run_native_window_animation(
        HIDE_ANIM_DURATION_SECS,
        0.0,
        SHOW_SCALE_HIDDEN,
        &HIDE_ANIM_DONE,
        &HIDE_ANIM_ACTIVE_TOKEN,
        &HIDE_ANIM_DONE_TOKEN,
    );
}

/// Cancel any running animations and snap to fully visible state.
pub(super) fn cancel_animation_snap_visible() {
    SHOW_ANIM_ACTIVE_TOKEN.fetch_add(1, Ordering::Relaxed);
    HIDE_ANIM_ACTIVE_TOKEN.fetch_add(1, Ordering::Relaxed);
    SHOW_ANIM_DONE.store(false, Ordering::Relaxed);
    HIDE_ANIM_DONE.store(false, Ordering::Relaxed);
    SHOW_ANIM_DONE_TOKEN.store(0, Ordering::Relaxed);
    HIDE_ANIM_DONE_TOKEN.store(0, Ordering::Relaxed);
    unsafe {
        snap_all_windows(1.0, 1.0, true);
    }
}

/// Cleanup after show animation completes (clear transforms, ensure alpha=1).
pub(super) fn reset_show_animation() {
    unsafe {
        snap_all_windows(1.0, 1.0, true);
    }
}

pub(super) fn poll_show_anim_done() -> bool {
    if !SHOW_ANIM_DONE.swap(false, Ordering::Relaxed) {
        return false;
    }
    let done = SHOW_ANIM_DONE_TOKEN.swap(0, Ordering::Relaxed);
    done != 0 && done == SHOW_ANIM_ACTIVE_TOKEN.load(Ordering::Relaxed)
}

pub(super) fn poll_hide_anim_done() -> bool {
    if !HIDE_ANIM_DONE.swap(false, Ordering::Relaxed) {
        return false;
    }
    let done = HIDE_ANIM_DONE_TOKEN.swap(0, Ordering::Relaxed);
    done != 0 && done == HIDE_ANIM_ACTIVE_TOKEN.load(Ordering::Relaxed)
}
