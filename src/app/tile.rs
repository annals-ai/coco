//! This module handles the logic for the tile, AKA Coco's main window
pub mod elm;
pub mod update;

use crate::agent::types::{AgentSession, AgentStatus, ChatMessage};
use crate::app::{ArrowKey, Message, Move, Page};
use crate::clipboard::ClipBoardContentType;
use crate::clipboard_store::ClipboardStore;
use crate::config::Config;
use crate::favorite_store::FavoriteStore;
use crate::search;
use crate::utils::open_settings;
use crate::{app::apps::App, platform::default_app_paths};

use arboard::Clipboard;
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, HotKeyState};

use iced::futures::SinkExt;
use iced::futures::channel::mpsc::{Sender, channel};
use iced::keyboard::Modifiers;
use iced::{
    Subscription, Theme, futures,
    keyboard::{self, key::Named},
    stream,
};
use iced::{event, window};

use nucleo_matcher::Matcher;
use objc2::rc::Retained;
use objc2_app_kit::NSRunningApplication;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use tray_icon::TrayIcon;

use std::cell::RefCell;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Flag set by keyboard::listen when a ⌘+key shortcut fires.
/// Checked in SearchQueryChanged to swallow the spurious character
/// that iced's text_input may insert alongside the shortcut.
pub static CMD_SHORTCUT_SWALLOW: AtomicBool = AtomicBool::new(false);

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
                "[{:.3}] [tile] {}",
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

/// This is a wrapper around the sender to disable dropping
#[derive(Clone, Debug)]
pub struct ExtSender(pub Sender<Message>);

/// Disable dropping the sender
impl Drop for ExtSender {
    fn drop(&mut self) {}
}

/// Re-export AppIndex from the search module
pub type AppIndex = search::AppIndex;

#[derive(Debug, Clone)]
pub struct CalculatorHistoryEntry {
    pub expression: String,
    pub result: String,
}

/// This is the base window, and its a "Tile"
/// Its fields are:
/// - Theme ([`iced::Theme`])
/// - Query (String)
/// - Query Lowercase (String, but lowercase)
/// - Previous Query Lowercase (String)
/// - Results (Vec<[`App`]>) the results of the search
/// - Options (Vec<[`App`]>) the options to search through
/// - Visible (bool) whether the window is visible or not
/// - Focused (bool) whether the window is focused or not
/// - Frontmost ([`Option<Retained<NSRunningApplication>>`]) the frontmost application before the window was opened
/// - Config ([`Config`]) the app's config
/// - Open Hotkey ID (`u32`) the id of the hotkey that opens the window
/// - Clipboard Content (`Vec<`[`ClipBoardContentType`]`>`) all of the cliboard contents
/// - Page ([`Page`]) the current page of the window (main or clipboard history)
pub struct Tile {
    pub theme: iced::Theme,
    pub focus_id: u32,
    pub query: String,
    query_lc: String,
    results: Vec<App>,
    options: AppIndex,
    emoji_apps: AppIndex,
    visible: bool,
    focused: bool,
    frontmost: Option<Retained<NSRunningApplication>>,
    pub config: Config,
    /// The opening hotkey
    hotkey: HotKey,
    clipboard_hotkey: Option<HotKey>,
    pub clipboard_store: ClipboardStore,
    pub clipboard_filtered: Vec<usize>,
    pub clipboard_quick_preview_open: bool,
    tray_icon: Option<TrayIcon>,
    sender: Option<ExtSender>,
    page: Page,
    fuzzy_matcher: RefCell<Matcher>,
    // Agent mode fields
    pub agent_sessions: Vec<AgentSession>,
    pub agent_filtered: Vec<usize>,
    pub agent_window_id: Option<window::Id>,
    pub agent_session_id: Option<String>,
    pub agent_messages: Vec<ChatMessage>,
    pub agent_input: String,
    pub agent_status: AgentStatus,
    pub agent_markdown: iced::widget::markdown::Content,
    // Permission state
    pub permissions_ok: bool,
    pub missing_accessibility: bool,
    pub missing_input_monitoring: bool,
    pub missing_paste_permission: bool,
    // Zero-query cache (built once on window open, reused on empty query)
    pub zero_query_cache: Vec<App>,
    // Actions overlay (⌘K)
    pub show_actions: bool,
    pub actions: Vec<crate::app::actions::ActionItem>,
    pub action_focus_id: u32,
    pub action_target_name: String,
    // Window switcher
    pub window_list: Vec<crate::platform::WindowInfo>,
    // Main window ID for window::resize
    pub main_window_id: Option<window::Id>,
    /// The last target blur height, used to avoid redundant resizes.
    pub target_blur_height: f64,
    /// The last target main-window height, used to debounce wgpu resizes.
    pub target_window_height: f64,
    /// Last scheduled shrink target for the main window (if debounced).
    pub pending_window_height: Option<f64>,
    /// Monotonic token to ignore stale delayed window resize tasks.
    pub window_resize_token: u64,
    // ── Native show/hide animation state ──
    pub show_animating: bool,
    pub hide_animating: bool,
    /// Last hotkey press instant for debouncing rapid presses.
    pub last_hotkey_time: Option<std::time::Instant>,
    /// Last search text edit instant (used to gate shrink resizes until idle).
    pub last_query_edit_time: Option<std::time::Instant>,
    /// When true, row hover events are ignored until the mouse moves again.
    pub suppress_row_hover_focus: bool,
    /// When true, trigger Cmd+V to the restored frontmost app after hide completes.
    pub pending_paste_after_hide: bool,
    /// In-memory calculator history shown in the calculator UI.
    pub calculator_history: Vec<CalculatorHistoryEntry>,
    // Clipboard favorites
    pub favorite_store: FavoriteStore,
    pub favorite_filtered: Vec<usize>,
    /// (fav_id, saved_query) — editing title state
    pub editing_favorite_title: Option<(u64, String)>,
}

impl Tile {
    /// This returns the theme of the window
    pub fn theme(&self, _: window::Id) -> Option<Theme> {
        Some(self.theme.clone())
    }

    /// Snap-resize the iced window AND blur child to match content height.
    ///
    /// Resizes both the iced window (so layout fills exactly) and the blur
    /// child NSWindow (macOS child windows don't auto-resize with parent).
    pub fn snap_resize(&mut self, target_h: f64) -> iced::Task<Message> {
        use crate::app::{FOOTER_HEIGHT, WINDOW_WIDTH};
        use crate::platform;
        let search_resize_debounce_active = self.visible
            && self.page == Page::Main
            && !self.query_lc.is_empty()
            && !self.hide_animating;
        let omit_main_footer =
            self.page == Page::Main && !self.show_actions && !self.results.is_empty();
        let effective_target_h = if omit_main_footer && target_h > FOOTER_HEIGHT + 1.0 {
            target_h - FOOTER_HEIGHT
        } else {
            target_h
        };
        let is_window_shrink = effective_target_h + 1.0 < self.target_window_height;
        let window_height_changed = (self.target_window_height - effective_target_h).abs() >= 1.0;
        coco_log!(
            "snap_resize target={:.1} eff={:.1} win={:.1} blur={:.1} page={:?} qlen={} results={} omit_footer={} debounce={} shrink={}",
            target_h,
            effective_target_h,
            self.target_window_height,
            self.target_blur_height,
            self.page,
            self.query_lc.len(),
            self.results.len(),
            omit_main_footer,
            search_resize_debounce_active,
            is_window_shrink
        );

        let mut tasks: Vec<iced::Task<Message>> = Vec::new();

        // Avoid resizing the wgpu-backed main NSWindow on the Main page while
        // search results are changing. We still resize the native blur child
        // (outer glass shell) and let the view layer clip/fill the black panel
        // to the same visual height. This removes the resize-induced flash
        // while preserving a dynamic visual panel height.
        if self.page == Page::Main {
            if (self.target_blur_height - effective_target_h).abs() >= 1.0 {
                self.target_blur_height = effective_target_h;
                platform::resize_blur_window(effective_target_h, WINDOW_WIDTH as f64);
            }

            if self.pending_window_height.take().is_some() {
                self.window_resize_token = self.window_resize_token.wrapping_add(1);
            }

            // Main window only grows when needed (e.g. reopening from a small
            // persisted height). It does not shrink during Main-page search,
            // which is the primary source of visible flashing.
            let desired_main_h = self.target_window_height.max(effective_target_h);
            if (self.target_window_height - desired_main_h).abs() < 1.0 {
                return iced::Task::none();
            }

            self.target_window_height = desired_main_h;
            if !platform::resize_main_window_top_anchored(desired_main_h, WINDOW_WIDTH as f64) {
                coco_log!(
                    "snap_resize main-page fallback iced resize {:.1}",
                    desired_main_h
                );
                if let Some(id) = self.main_window_id {
                    tasks.push(window::resize::<Message>(
                        id,
                        iced::Size {
                            width: WINDOW_WIDTH,
                            height: desired_main_h as f32,
                        },
                    ));
                }
            } else {
                coco_log!(
                    "snap_resize main-page native top-anchored main resize applied {:.1}",
                    desired_main_h
                );
            }

            return if tasks.is_empty() {
                iced::Task::none()
            } else {
                iced::Task::batch(tasks)
            };
        }

        if search_resize_debounce_active && window_height_changed {
            // Defer all height changes while the user is actively typing.
            // The actual apply is gated in `ApplyDebouncedWindowResize` by
            // `last_query_edit_time`, so the panel only resizes after input
            // has been idle briefly.
            // Keep the blur child frozen too while debouncing. Updating only
            // the blur shell before the main iced window catches up causes a
            // visible height "flash" during search.
            self.pending_window_height = Some(effective_target_h);
            self.window_resize_token = self.window_resize_token.wrapping_add(1);
            let token = self.window_resize_token;
            coco_log!(
                "snap_resize defer main resize pending={:.1} token={}",
                effective_target_h,
                token
            );
            let delay_ms = 60;
            return iced::Task::perform(
                async move {
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    (token, effective_target_h)
                },
                |(token, height)| Message::ApplyDebouncedWindowResize(token, height),
            );
        }

        // Resize blur child immediately (native, no wgpu involvement) once
        // we're not in the active typing-shrink freeze path.
        if (self.target_blur_height - effective_target_h).abs() >= 1.0 {
            self.target_blur_height = effective_target_h;
            platform::resize_blur_window(effective_target_h, WINDOW_WIDTH as f64);
        }

        // Invalidate any pending delayed shrink once we decide to resize
        // immediately (growth, page switch, clear query, etc.).
        if self.pending_window_height.take().is_some() {
            self.window_resize_token = self.window_resize_token.wrapping_add(1);
        }

        // Resize the iced window only when the target changes.
        if (self.target_window_height - effective_target_h).abs() < 1.0 {
            return if tasks.is_empty() {
                iced::Task::none()
            } else {
                iced::Task::batch(tasks)
            };
        }

        self.target_window_height = effective_target_h;
        let avoid_native_main_resize_while_typing = self.visible
            && self.page == Page::Main
            && !self.query_lc.is_empty()
            && !self.show_animating
            && !self.hide_animating;
        if avoid_native_main_resize_while_typing
            || !platform::resize_main_window_top_anchored(effective_target_h, WINDOW_WIDTH as f64)
        {
            if avoid_native_main_resize_while_typing {
                coco_log!(
                    "snap_resize using iced main resize during typing {:.1} (avoid native stretch)",
                    effective_target_h
                );
            } else {
                coco_log!(
                    "snap_resize native main resize unavailable -> iced resize {:.1}",
                    effective_target_h
                );
            }
            if let Some(id) = self.main_window_id {
                tasks.push(window::resize::<Message>(
                    id,
                    iced::Size {
                        width: WINDOW_WIDTH,
                        height: effective_target_h as f32,
                    },
                ));
            } else {
                return if tasks.is_empty() {
                    iced::Task::none()
                } else {
                    iced::Task::batch(tasks)
                };
            }
        } else {
            coco_log!(
                "snap_resize native top-anchored main resize applied {:.1}",
                effective_target_h
            );
        }

        iced::Task::batch(tasks)
    }

    /// This handles the subscriptions of the window
    ///
    /// The subscriptions are:
    /// - Hotkeys
    /// - Hot reloading
    /// - Clipboard history
    /// - Window close events
    /// - Keypresses (escape to close the window)
    /// - Window focus changes
    pub fn subscription(&self) -> Subscription<Message> {
        let keyboard = event::listen_with(|event, _, id| match event {
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                ..
            }) => Some(Message::EscKeyPressed(id)),
            iced::Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Character(cha),
                modifiers,
                ..
            }) if modifiers.command() => {
                let ch = cha.to_string();
                let ch_lc = ch.to_lowercase();
                CMD_SHORTCUT_SWALLOW.store(true, Ordering::Relaxed);
                if ch_lc == "k" {
                    Some(Message::ShowActions)
                } else if ch_lc == "r" {
                    Some(Message::ReloadConfig)
                } else if ch_lc == "p" {
                    Some(Message::ClipboardTogglePinFocused)
                } else if ch_lc == "d" {
                    Some(Message::ClipboardDeleteFocused)
                } else if ch_lc == "s" {
                    Some(Message::ClipboardFavoriteAdd)
                } else if ch_lc == "e" {
                    Some(Message::ClipboardFavoriteStartEdit)
                } else {
                    CMD_SHORTCUT_SWALLOW.store(false, Ordering::Relaxed);
                    if ch == "," {
                        open_settings();
                    }
                    None
                }
            }
            _ => None,
        });
        let needs_anim_poll = self.show_animating || self.hide_animating;
        let anim_completion_poll: Subscription<Message> = if needs_anim_poll {
            Subscription::run(handle_animation_completion)
        } else {
            Subscription::none()
        };

        Subscription::batch([
            Subscription::run(handle_hotkeys),
            keyboard,
            Subscription::run(handle_recipient),
            Subscription::run(handle_hot_reloading),
            Subscription::run(handle_clipboard_history),
            anim_completion_poll,
            window::close_events().map(Message::AgentWindowClosed),
            keyboard::listen().filter_map(|event| {
                if let keyboard::Event::KeyPressed { key, modifiers, .. } = event {
                    match key {
                        keyboard::Key::Named(Named::Escape) => {
                            return Some(Message::KeyPressed(65598));
                        }
                        keyboard::Key::Named(Named::ArrowUp) => {
                            return Some(Message::ChangeFocus(ArrowKey::Up));
                        }
                        keyboard::Key::Named(Named::ArrowLeft) => {
                            return Some(Message::ChangeFocus(ArrowKey::Left));
                        }
                        keyboard::Key::Named(Named::ArrowRight) => {
                            return Some(Message::ChangeFocus(ArrowKey::Right));
                        }
                        keyboard::Key::Named(Named::ArrowDown) => {
                            return Some(Message::ChangeFocus(ArrowKey::Down));
                        }
                        keyboard::Key::Named(Named::Tab) => {
                            return Some(Message::CycleLauncherMode {
                                reverse: modifiers.shift(),
                            });
                        }
                        keyboard::Key::Named(Named::Space) => {
                            coco_log!(
                                "keyboard::listen Named::Space page? main-window event emitted SpacePressed"
                            );
                            return Some(Message::SpacePressed);
                        }
                        keyboard::Key::Character(chr) => {
                            let chr_s = chr.to_string();
                            if chr_s == " " && !modifiers.command() {
                                coco_log!(
                                    "keyboard::listen Character(space) emitted SpacePressed shift={} ctrl={} alt={} cmd={}",
                                    modifiers.shift(),
                                    modifiers.control(),
                                    modifiers.alt(),
                                    modifiers.command()
                                );
                                return Some(Message::SpacePressed);
                            }
                            // ⌘+key shortcuts are handled in event::listen_with above.
                            // Let iced text_input handle normal character input directly.
                            return None;
                        }
                        keyboard::Key::Named(Named::Enter) => return Some(Message::OpenFocused),
                        keyboard::Key::Named(Named::Backspace) => {
                            return Some(Message::FocusTextInput(Move::Back));
                        }
                        _ => {}
                    }
                    None
                } else {
                    None
                }
            }),
            window::events()
                .with(self.focused)
                .filter_map(|(focused, (wid, event))| match event {
                    window::Event::Unfocused => {
                        if focused {
                            Some(Message::WindowFocusChanged(wid, false))
                        } else {
                            None
                        }
                    }
                    window::Event::Focused => Some(Message::WindowFocusChanged(wid, true)),
                    _ => None,
                }),
        ])
    }

    /// Handles the search query changed event.
    ///
    /// This is separate from the `update` function because it has a decent amount of logic, and
    /// should be separated out to make it easier to test. This function is called by the `update`
    /// function to handle the search query changed event.
    pub fn handle_search_query_changed(&mut self) {
        let query = self.query_lc.clone();
        let options = match self.page {
            Page::Main => &self.options,
            Page::EmojiSearch => &self.emoji_apps,
            _ => return, // AgentList / ClipboardHistory / ClipboardFavorites don't use fuzzy search
        };
        let mut matcher = self.fuzzy_matcher.borrow_mut();
        self.results = options.search(&query, &mut matcher);
    }

    /// Returns the indices of clipboard entries to display (filtered or all).
    pub fn clipboard_display_indices(&self) -> &[usize] {
        &self.clipboard_filtered
    }

    /// Returns the count of displayed clipboard entries.
    pub fn clipboard_display_count(&self) -> usize {
        self.clipboard_filtered.len()
    }

    /// Rebuild the filtered list (all entries if no search query).
    pub fn clipboard_rebuild_filtered(&mut self) {
        let query = self.query_lc.clone();
        let mut matcher = self.fuzzy_matcher.borrow_mut();
        self.clipboard_filtered = self.clipboard_store.search(&query, &mut matcher);
    }

    /// Returns the filtered agent session indices.
    pub fn agent_display_indices(&self) -> &[usize] {
        &self.agent_filtered
    }

    /// Returns the display count for AgentList, including the "+ New conversation" row.
    pub fn agent_display_count(&self) -> usize {
        1 + self.agent_filtered.len()
    }

    /// Rebuild filtered agent session indices from the current query.
    pub fn agent_rebuild_filtered(&mut self) {
        if self.query_lc.is_empty() {
            self.agent_filtered = (0..self.agent_sessions.len()).collect();
            return;
        }

        let query = self.query_lc.clone();
        self.agent_filtered = self
            .agent_sessions
            .iter()
            .enumerate()
            .filter_map(|(idx, session)| {
                let title_lc = session.title.to_lowercase();
                if title_lc.contains(&query) {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect();
    }

    /// Returns the indices of favorite entries to display (filtered or all).
    pub fn favorite_display_indices(&self) -> &[usize] {
        &self.favorite_filtered
    }

    /// Returns the count of displayed favorite entries.
    pub fn favorite_display_count(&self) -> usize {
        self.favorite_filtered.len()
    }

    /// Rebuild the filtered favorite list (all entries if no search query).
    pub fn favorite_rebuild_filtered(&mut self) {
        let query = self.query_lc.clone();
        let mut matcher = self.fuzzy_matcher.borrow_mut();
        self.favorite_filtered = self.favorite_store.search(&query, &mut matcher);
    }

    /// Load sessions from disk and rebuild filtered indices using the current query.
    pub fn agent_refresh_sessions(&mut self) {
        self.agent_sessions = crate::agent::session::list_sessions();
        self.agent_rebuild_filtered();
    }

    /// Gets the frontmost application to focus later.
    pub fn capture_frontmost(&mut self) {
        use objc2_app_kit::NSWorkspace;

        let ws = NSWorkspace::sharedWorkspace();
        self.frontmost = ws.frontmostApplication();
        if let Some(app) = self.frontmost.as_ref() {
            let pid = app.processIdentifier();
            let name = app
                .localizedName()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "<unknown>".to_string());
            coco_log!("capture_frontmost pid={} name={}", pid, name);
        } else {
            coco_log!("capture_frontmost none");
        }
    }

    /// Restores the frontmost application.
    #[allow(deprecated)]
    pub fn restore_frontmost(&mut self) -> Option<i32> {
        use objc2_app_kit::NSApplicationActivationOptions;

        if let Some(app) = self.frontmost.take() {
            let pid = app.processIdentifier();
            let name = app
                .localizedName()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "<unknown>".to_string());
            coco_log!("restore_frontmost pid={} name={}", pid, name);
            app.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
            Some(pid)
        } else {
            coco_log!("restore_frontmost skipped (none)");
            None
        }
    }
}

/// This is the subscription function that handles hot reloading of the config
fn handle_hot_reloading() -> impl futures::Stream<Item = Message> {
    stream::channel(100, async |mut output| {
        let mut content = fs::read_to_string(
            std::env::var("HOME").unwrap_or("".to_owned()) + "/.config/coco/config.toml",
        )
        .unwrap_or("".to_string());

        let paths = default_app_paths();
        let mut total_files: usize = paths
            .par_iter()
            .map(|dir| count_dirs_in_dir(Path::new(dir)))
            .sum();

        loop {
            let current_content = fs::read_to_string(
                std::env::var("HOME").unwrap_or("".to_owned()) + "/.config/coco/config.toml",
            )
            .unwrap_or("".to_string());

            let current_total_files: usize = paths
                .par_iter()
                .map(|dir| count_dirs_in_dir(Path::new(dir)))
                .sum();

            if current_content != content {
                content = current_content;
                output.send(Message::ReloadConfig).await.unwrap();
            } else if total_files != current_total_files {
                total_files = current_total_files;
                output.send(Message::ReloadConfig).await.unwrap();
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
}

fn count_dirs_in_dir(dir: impl AsRef<Path>) -> usize {
    // Read the directory; if it fails, treat as empty
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return 0,
    };

    entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .count()
}

/// This is the subscription function that handles hotkeys for hiding / showing the window
fn handle_hotkeys() -> impl futures::Stream<Item = Message> {
    stream::channel(100, async |mut output| {
        let receiver = GlobalHotKeyEvent::receiver();
        loop {
            if let Ok(event) = receiver.recv()
                && event.state == HotKeyState::Pressed
            {
                output.try_send(Message::KeyPressed(event.id)).unwrap();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
}

/// Poll native animation completion flags on a lightweight timer.
fn handle_animation_completion() -> impl futures::Stream<Item = Message> {
    use crate::platform;
    stream::channel(10, async |mut output| {
        loop {
            if platform::poll_hide_anim_done() {
                output.send(Message::NativeHideComplete).await.ok();
            }
            if platform::poll_show_anim_done() {
                output.send(Message::NativeShowComplete).await.ok();
            }
            tokio::time::sleep(Duration::from_millis(16)).await;
        }
    })
}

/// This is the subscription function that handles the change in clipboard history
fn handle_clipboard_history() -> impl futures::Stream<Item = Message> {
    stream::channel(100, async |mut output| {
        let mut clipboard = Clipboard::new().unwrap();
        let mut prev_byte_rep: Option<ClipBoardContentType> = None;

        loop {
            let byte_rep = if let Ok(a) = clipboard.get_image() {
                Some(ClipBoardContentType::Image(a))
            } else if let Ok(a) = clipboard.get_text() {
                Some(ClipBoardContentType::Text(a))
            } else {
                None
            };

            if byte_rep != prev_byte_rep
                && let Some(content) = &byte_rep
            {
                output
                    .send(Message::ClipboardHistory(content.to_owned()))
                    .await
                    .ok();
                prev_byte_rep = byte_rep;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
}

fn handle_recipient() -> impl futures::Stream<Item = Message> {
    stream::channel(100, async |mut output| {
        let (sender, mut recipient) = channel(100);
        output
            .send(Message::SetSender(ExtSender(sender)))
            .await
            .expect("Sender not sent");
        loop {
            let abcd = recipient
                .try_next()
                .map(async |msg| {
                    if let Some(msg) = msg {
                        output.send(msg).await.unwrap();
                    }
                })
                .ok();

            if let Some(abcd) = abcd {
                abcd.await;
            }
            tokio::time::sleep(Duration::from_nanos(10)).await;
        }
    })
}
