use iced::wgpu::rwh::WindowHandle;

pub use self::cross::default_app_paths;
use crate::app::apps::App;

/// Information about a window for the window switcher
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub window_id: u32,
    pub owner_pid: i32,
    pub owner_name: String,
    pub window_title: String,
    pub icon: Option<iced::widget::image::Handle>,
}

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

pub fn resize_main_window_top_anchored(height: f64, width: f64) -> bool {
    #[cfg(target_os = "macos")]
    {
        return self::macos::resize_main_window_top_anchored(height, width);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (height, width);
        false
    }
}

pub fn create_agent_blur_window(handle: &WindowHandle, width: f64, height: f64) {
    #[cfg(target_os = "macos")]
    self::macos::create_agent_blur_window(handle, width, height);
}

pub fn clear_agent_blur_window() {
    #[cfg(target_os = "macos")]
    self::macos::clear_agent_blur_window();
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

pub fn get_running_apps(store_icons: bool) -> Vec<App> {
    #[cfg(target_os = "macos")]
    return self::macos::get_running_apps(store_icons);
    #[cfg(not(target_os = "macos"))]
    vec![]
}

pub fn activate_app_by_pid(pid: i32) {
    #[cfg(target_os = "macos")]
    self::macos::activate_app_by_pid(pid);
}

pub fn hide_app_by_pid(pid: i32) {
    #[cfg(target_os = "macos")]
    self::macos::hide_app_by_pid(pid);
}

pub fn quit_app_by_pid(pid: i32) {
    #[cfg(target_os = "macos")]
    self::macos::quit_app_by_pid(pid);
}

pub fn force_quit_app_by_pid(pid: i32) {
    #[cfg(target_os = "macos")]
    self::macos::force_quit_app_by_pid(pid);
}

pub fn reveal_in_finder(path: &str) {
    #[cfg(target_os = "macos")]
    self::macos::reveal_in_finder(path);
}

pub fn get_window_list() -> Vec<WindowInfo> {
    #[cfg(target_os = "macos")]
    return self::macos::windows::get_window_list();
    #[cfg(not(target_os = "macos"))]
    vec![]
}

pub fn focus_window(pid: i32, window_id: u32) {
    #[cfg(target_os = "macos")]
    self::macos::windows::focus_window(pid, window_id);
}

pub fn prepare_show_animation() {
    #[cfg(target_os = "macos")]
    self::macos::prepare_show_animation();
}

pub fn animate_show() {
    #[cfg(target_os = "macos")]
    self::macos::animate_show();
}

pub fn animate_hide() {
    #[cfg(target_os = "macos")]
    self::macos::animate_hide();
}

pub fn cancel_animation_snap_visible() {
    #[cfg(target_os = "macos")]
    self::macos::cancel_animation_snap_visible();
}

pub fn reset_show_animation() {
    #[cfg(target_os = "macos")]
    self::macos::reset_show_animation();
}

pub fn poll_show_anim_done() -> bool {
    #[cfg(target_os = "macos")]
    return self::macos::poll_show_anim_done();
    #[cfg(not(target_os = "macos"))]
    false
}

pub fn poll_hide_anim_done() -> bool {
    #[cfg(target_os = "macos")]
    return self::macos::poll_hide_anim_done();
    #[cfg(not(target_os = "macos"))]
    false
}

pub fn store_main_window(handle: &WindowHandle) {
    #[cfg(target_os = "macos")]
    self::macos::store_main_window(handle);
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
