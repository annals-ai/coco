//! This module handles the logic for the tile, AKA rustcast's main window
pub mod elm;
pub mod update;

use crate::agent::types::{AgentSession, AgentStatus, ChatMessage};
use crate::app::{ArrowKey, Message, Move, Page};
use crate::clipboard::ClipBoardContentType;
use crate::config::Config;
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
use std::time::Duration;
use std::path::Path;

/// This is a wrapper around the sender to disable dropping
#[derive(Clone, Debug)]
pub struct ExtSender(pub Sender<Message>);

/// Disable dropping the sender
impl Drop for ExtSender {
    fn drop(&mut self) {}
}

/// Re-export AppIndex from the search module
pub type AppIndex = search::AppIndex;

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
    clipboard_content: Vec<ClipBoardContentType>,
    tray_icon: Option<TrayIcon>,
    sender: Option<ExtSender>,
    page: Page,
    fuzzy_matcher: RefCell<Matcher>,
    // Agent mode fields
    pub agent_sessions: Vec<AgentSession>,
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
}

impl Tile {
    /// This returns the theme of the window
    pub fn theme(&self, _: window::Id) -> Option<Theme> {
        Some(self.theme.clone())
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
                modifiers: Modifiers::LOGO,
                ..
            }) => {
                if cha.to_string() == "," {
                    open_settings();
                }
                None
            }
            _ => None,
        });
        Subscription::batch([
            Subscription::run(handle_hotkeys),
            keyboard,
            Subscription::run(handle_recipient),
            Subscription::run(handle_hot_reloading),
            Subscription::run(handle_clipboard_history),
            Subscription::run(handle_double_tap_option),
            window::close_events().map(Message::HideWindow),
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
                        keyboard::Key::Character(chr) => {
                            if modifiers.command() && chr.to_string().to_lowercase() == "r" {
                                return Some(Message::ReloadConfig);
                            } else if modifiers.command() && chr.to_string() == "," {
                                open_settings();
                            } else {
                                return Some(Message::FocusTextInput(Move::Forwards(
                                    chr.to_string(),
                                )));
                            }
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
            _ => return, // AgentList / ClipboardHistory don't use fuzzy search
        };
        let mut matcher = self.fuzzy_matcher.borrow_mut();
        self.results = options.search(&query, &mut matcher);
    }

    /// Gets the frontmost application to focus later.
    pub fn capture_frontmost(&mut self) {
        use objc2_app_kit::NSWorkspace;

        let ws = NSWorkspace::sharedWorkspace();
        self.frontmost = ws.frontmostApplication();
    }

    /// Restores the frontmost application.
    #[allow(deprecated)]
    pub fn restore_frontmost(&mut self) {
        use objc2_app_kit::NSApplicationActivationOptions;

        if let Some(app) = self.frontmost.take() {
            app.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
        }
    }
}

/// This is the subscription function that handles hot reloading of the config
fn handle_hot_reloading() -> impl futures::Stream<Item = Message> {
    stream::channel(100, async |mut output| {
        let mut content = fs::read_to_string(
            std::env::var("HOME").unwrap_or("".to_owned()) + "/.config/rustcast/config.toml",
        )
        .unwrap_or("".to_string());

        let paths = default_app_paths();
        let mut total_files: usize = paths
            .par_iter()
            .map(|dir| count_dirs_in_dir(Path::new(dir)))
            .sum();

        loop {
            let current_content = fs::read_to_string(
                std::env::var("HOME").unwrap_or("".to_owned()) + "/.config/rustcast/config.toml",
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

/// Poll for double-tap Option key events from the native macOS monitor.
fn handle_double_tap_option() -> impl futures::Stream<Item = Message> {
    use crate::platform::poll_double_tap_option;
    stream::channel(10, async |mut output| {
        loop {
            if poll_double_tap_option() {
                output.send(Message::ToggleAgentMode).await.ok();
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
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
