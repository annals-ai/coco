use iced::wgpu::rwh::WindowHandle;

pub use self::cross::default_app_paths;
use crate::app::apps::App;

mod cross;
#[cfg(target_os = "macos")]
mod macos;

pub fn set_activation_policy_accessory() {
    #[cfg(target_os = "macos")]
    self::macos::set_activation_policy_accessory();
}

pub fn window_config(handle: &WindowHandle) {
    #[cfg(target_os = "macos")]
    self::macos::macos_window_config(handle);
}

pub fn focus_this_app() {
    #[cfg(target_os = "macos")]
    self::macos::focus_this_app();
}

pub fn transform_process_to_ui_element() {
    #[cfg(target_os = "macos")]
    self::macos::transform_process_to_ui_element();
}

pub fn create_blur_child_window(handle: &WindowHandle, width: f64, content_height: f64) {
    #[cfg(target_os = "macos")]
    self::macos::create_blur_child_window(handle, width, content_height);
}

pub fn resize_blur_window(content_height: f64, width: f64) {
    #[cfg(target_os = "macos")]
    self::macos::resize_blur_window(content_height, width);
}

pub fn clear_blur_window() {
    #[cfg(target_os = "macos")]
    self::macos::clear_blur_window();
}

pub fn create_agent_blur_window(handle: &WindowHandle, width: f64, height: f64) {
    #[cfg(target_os = "macos")]
    self::macos::create_agent_blur_window(handle, width, height);
}

pub fn clear_agent_blur_window() {
    #[cfg(target_os = "macos")]
    self::macos::clear_agent_blur_window();
}

pub fn check_accessibility() -> bool {
    #[cfg(target_os = "macos")]
    return self::macos::check_accessibility();
    #[cfg(not(target_os = "macos"))]
    true
}

pub fn check_input_monitoring() -> bool {
    #[cfg(target_os = "macos")]
    return self::macos::check_input_monitoring();
    #[cfg(not(target_os = "macos"))]
    true
}

pub fn open_accessibility_settings() {
    #[cfg(target_os = "macos")]
    self::macos::open_accessibility_settings();
}

pub fn open_input_monitoring_settings() {
    #[cfg(target_os = "macos")]
    self::macos::open_input_monitoring_settings();
}

pub fn install_double_tap_option_monitor() {
    #[cfg(target_os = "macos")]
    self::macos::install_double_tap_option_monitor();
}

pub fn poll_double_tap_option() -> bool {
    #[cfg(target_os = "macos")]
    return self::macos::poll_double_tap_option();
    #[cfg(not(target_os = "macos"))]
    false
}

/// The kinds of haptic patterns that can be performed
#[allow(dead_code)]
#[derive(Copy, Clone, Debug)]
pub enum HapticPattern {
    Generic,
    Alignment,
    LevelChange,
}

#[cfg(target_os = "macos")]
pub fn perform_haptic(pattern: HapticPattern) -> bool {
    self::macos::perform_haptic(pattern)
}

#[cfg(not(target_os = "macos"))]
pub fn perform_haptic(_: HapticPattern) -> bool {
    false
}

#[cfg(target_os = "macos")]
pub fn get_installed_apps(store_icons: bool) -> Vec<App> {
    self::macos::get_installed_apps(store_icons)
}

#[cfg(not(target_os = "macos"))]
pub fn get_installed_apps(store_icons: bool) -> Vec<App> {
    self::cross::get_installed_apps(store_icons)
}
