//! This handles the update logic for the tile (AKA rustcast's main window)
use std::fs;
use std::path::Path;
use std::thread;

use iced::Task;
use iced::widget::image::Handle;
use iced::widget::operation;
use iced::widget::operation::AbsoluteOffset;
use iced::window;
use rayon::slice::ParallelSliceMut;

use crate::agent::process::spawn_claude;
use crate::agent::types::{AgentStatus, ChatMessage, ClaudeEvent};
use crate::app::apps::App;
use crate::app::apps::AppCommand;
use crate::app::default_settings;
use crate::app::menubar::menu_icon;
use crate::app::{Message, Page, agent_window_settings, tile::{AppIndex, Tile}};
use crate::calculator::Expr;
use crate::clipboard::ClipBoardContentType;
use crate::commands::Function;
use crate::config::Config;
use crate::currency_conversion;
use crate::unit_conversion;
use crate::utils::is_valid_url;
use crate::app::WINDOW_WIDTH;
use crate::{app::ArrowKey, platform::focus_this_app};
use crate::platform::{self, perform_haptic};
use crate::{app::Move, platform::HapticPattern};
use crate::{app::RUSTCAST_DESC_NAME, platform::get_installed_apps};

pub fn handle_update(tile: &mut Tile, message: Message) -> Task<Message> {
    match message {
        Message::OpenWindow => {
            tile.capture_frontmost();
            focus_this_app();
            tile.focused = true;
            tile.visible = true;
            // Refresh permission state each time the window is shown
            tile.missing_accessibility = !platform::check_accessibility();
            tile.missing_input_monitoring = !platform::check_input_monitoring();
            tile.permissions_ok = !tile.missing_accessibility && !tile.missing_input_monitoring;
            // Resize blur window for the current page (e.g. clipboard opened via hotkey)
            let banner_h = permission_banner_height(tile);
            if tile.page == Page::ClipboardHistory && !tile.clipboard_content.is_empty() {
                platform::resize_blur_window(52.0 + banner_h + 1.0 + 360.0 + 38.0, WINDOW_WIDTH as f64);
            } else {
                platform::resize_blur_window(52.0 + banner_h, WINDOW_WIDTH as f64);
            }
            Task::none()
        }
        Message::HideTrayIcon => {
            tile.tray_icon = None;
            tile.config.show_trayicon = false;
            let home = std::env::var("HOME").unwrap();
            let confg_str = toml::to_string(&tile.config).unwrap();
            thread::spawn(move || fs::write(home + "/.config/rustcast/config.toml", confg_str));
            Task::none()
        }

        Message::SetSender(sender) => {
            tile.sender = Some(sender.clone());
            if tile.config.show_trayicon {
                tile.tray_icon = Some(menu_icon(tile.hotkey, sender));
            }
            Task::none()
        }

        Message::EscKeyPressed(id) => {
            // Agent window: ESC closes it
            if tile.agent_window_id == Some(id) {
                tile.agent_window_id = None;
                platform::clear_agent_blur_window();
                return window::close(id);
            }

            if tile.page == Page::EmojiSearch && !tile.query_lc.is_empty() {
                return Task::none();
            }

            // AgentList: ESC goes back to Main
            if tile.page == Page::AgentList {
                tile.page = Page::Main;
                let banner_h = permission_banner_height(tile);
                platform::resize_blur_window(52.0 + banner_h, WINDOW_WIDTH as f64);
                return Task::batch([
                    Task::done(Message::ClearSearchQuery),
                    Task::done(Message::ClearSearchResults),
                ]);
            }

            if tile.query_lc.is_empty() {
                Task::batch([
                    Task::done(Message::HideWindow(id)),
                    Task::done(Message::ReturnFocus),
                ])
            } else {
                tile.page = Page::Main;
                let banner_h = permission_banner_height(tile);
                platform::resize_blur_window(52.0 + banner_h, WINDOW_WIDTH as f64);

                Task::batch(vec![
                    Task::done(Message::ClearSearchQuery),
                    Task::done(Message::ClearSearchResults),
                ])
            }
        }

        Message::ClearSearchQuery => {
            tile.query_lc = String::new();
            tile.query = String::new();
            Task::none()
        }

        Message::ChangeFocus(key) => {
            let len = match tile.page {
                Page::ClipboardHistory => tile.clipboard_content.len() as u32,
                Page::EmojiSearch => tile.results.len() as u32,
                Page::AgentList => (1 + tile.agent_sessions.len()) as u32,
                _ => tile.results.len() as u32,
            };

            let old_focus_id = tile.focus_id;

            if len == 0 {
                return Task::none();
            }

            let change_by = match tile.page {
                Page::EmojiSearch => 6,
                _ => 1,
            };

            let task = match (&key, &tile.page) {
                (ArrowKey::Down, _) => {
                    tile.focus_id = (tile.focus_id + change_by) % len;
                    Task::none()
                }
                (ArrowKey::Up, _) => {
                    tile.focus_id = (tile.focus_id + len - change_by) % len;
                    Task::none()
                }
                (ArrowKey::Left, Page::EmojiSearch) => {
                    tile.focus_id = (tile.focus_id + len - 1) % len;
                    operation::focus("results")
                }
                (ArrowKey::Right, Page::EmojiSearch) => {
                    tile.focus_id = (tile.focus_id + 1) % len;
                    operation::focus("results")
                }
                _ => Task::none(),
            };

            let quantity = match tile.page {
                Page::Main | Page::AgentList => 52.0,
                Page::ClipboardHistory => 50.,
                Page::EmojiSearch => 5.,
            };

            let (wrapped_up, wrapped_down) = match &key {
                ArrowKey::Up => (tile.focus_id > old_focus_id, false),
                ArrowKey::Down => (false, tile.focus_id < old_focus_id),
                _ => (false, false),
            };

            let y = if wrapped_down {
                0.0
            } else if wrapped_up {
                (len.saturating_sub(1)) as f32 * quantity
            } else {
                tile.focus_id as f32 * quantity
            };

            Task::batch([
                task,
                operation::scroll_to(
                    "results",
                    AbsoluteOffset {
                        x: None,
                        y: Some(y),
                    },
                ),
            ])
        }

        Message::OpenFocused => {
            if tile.page == Page::AgentList {
                if tile.focus_id == 0 {
                    // New conversation — use current query as prompt or empty
                    let prompt = if tile.query.trim().is_empty() {
                        String::new()
                    } else {
                        tile.query.clone()
                    };
                    return Task::done(Message::NewAgentSession(prompt));
                } else {
                    let idx = tile.focus_id as usize - 1;
                    if let Some(session) = tile.agent_sessions.get(idx) {
                        return Task::done(Message::AgentSessionSelected(session.session_id.clone()));
                    }
                    return Task::none();
                }
            }
            match tile.results.get(tile.focus_id as usize) {
                Some(App {
                    open_command: AppCommand::Function(func),
                    ..
                }) => Task::done(Message::RunFunction(func.to_owned())),
                Some(App {
                    open_command: AppCommand::Message(msg),
                    ..
                }) => Task::done(msg.to_owned()),
                Some(App {
                    open_command: AppCommand::Display,
                    ..
                }) => Task::done(Message::ReturnFocus),
                None => Task::none(),
            }
        }

        Message::ReloadConfig => {
            let new_config: Config = match toml::from_str(
                &fs::read_to_string(
                    std::env::var("HOME").unwrap_or("".to_owned())
                        + "/.config/rustcast/config.toml",
                )
                .unwrap_or("".to_owned()),
            ) {
                Ok(a) => a,
                Err(_) => return Task::none(),
            };

            let mut new_options = get_installed_apps(new_config.theme.show_icons);
            new_options.extend(new_config.shells.iter().map(|x| x.to_app()));
            new_options.extend(App::basic_apps());
            new_options.par_sort_by_key(|x| x.name.len());

            tile.theme = new_config.theme.to_owned().into();
            tile.config = new_config;
            tile.options = AppIndex::from_apps(new_options);
            Task::none()
        }

        Message::KeyPressed(hk_id) => {
            let is_clipboard_hotkey = tile
                .clipboard_hotkey
                .map(|hotkey| hotkey.id == hk_id)
                .unwrap_or(false);
            let is_open_hotkey = hk_id == tile.hotkey.id;

            let clipboard_page_task = if is_clipboard_hotkey {
                Task::done(Message::SwitchToPage(Page::ClipboardHistory))
            } else if is_open_hotkey {
                Task::done(Message::SwitchToPage(Page::Main))
            } else {
                Task::none()
            };

            if is_open_hotkey || is_clipboard_hotkey {
                if !tile.visible {
                    return Task::batch([open_window(), clipboard_page_task]);
                }

                tile.visible = !tile.visible;

                let clear_search_query = if tile.config.buffer_rules.clear_on_hide {
                    Task::done(Message::ClearSearchQuery)
                } else {
                    Task::none()
                };

                let to_close = window::latest().map(|x| x.unwrap());
                Task::batch([
                    to_close.map(Message::HideWindow),
                    clear_search_query,
                    Task::done(Message::ReturnFocus),
                ])
            } else {
                Task::none()
            }
        }

        Message::SwitchToPage(page) => {
            tile.page = page.clone();
            let banner_h = permission_banner_height(tile);
            // Resize blur to match the target page content height
            match &tile.page {
                Page::ClipboardHistory if !tile.clipboard_content.is_empty() => {
                    platform::resize_blur_window(52.0 + banner_h + 1.0 + 360.0 + 38.0, WINDOW_WIDTH as f64);
                }
                Page::AgentList => {
                    tile.agent_sessions = crate::agent::session::list_sessions();
                    let rows = std::cmp::min(1 + tile.agent_sessions.len(), 7) as f64;
                    platform::resize_blur_window(52.0 + banner_h + 1.0 + rows * 52.0 + 38.0, WINDOW_WIDTH as f64);
                }
                _ => {
                    platform::resize_blur_window(52.0 + banner_h, WINDOW_WIDTH as f64);
                }
            }
            Task::batch([
                Task::done(Message::ClearSearchQuery),
                Task::done(Message::ClearSearchResults),
            ])
        }

        Message::RunFunction(command) => {
            command.execute(&tile.config, &tile.query);

            let return_focus_task = match &command {
                Function::OpenApp(_) | Function::OpenPrefPane | Function::GoogleSearch(_) => {
                    Task::none()
                }
                _ => Task::done(Message::ReturnFocus),
            };

            if tile.config.buffer_rules.clear_on_enter {
                window::latest()
                    .map(|x| x.unwrap())
                    .map(Message::HideWindow)
                    .chain(Task::done(Message::ClearSearchQuery))
                    .chain(return_focus_task)
            } else {
                Task::none()
            }
        }

        Message::HideWindow(a) => {
            // If this is the agent window, handle it separately
            if tile.agent_window_id == Some(a) {
                tile.agent_window_id = None;
                platform::clear_agent_blur_window();
                return window::close(a);
            }
            tile.visible = false;
            tile.focused = false;
            tile.page = Page::Main;
            platform::clear_blur_window();
            Task::batch([window::close(a), Task::done(Message::ClearSearchResults)])
        }

        Message::ReturnFocus => {
            tile.restore_frontmost();
            Task::none()
        }

        Message::FocusTextInput(update_query_char) => {
            match update_query_char {
                Move::Forwards(query_char) => {
                    tile.query += &query_char.clone();
                    tile.query_lc += &query_char.clone().to_lowercase();
                }
                Move::Back => {
                    tile.query.pop();
                    tile.query_lc.pop();
                }
            }
            let updated_query = tile.query.clone();
            Task::batch([
                operation::focus("query"),
                window::latest()
                    .map(|x| x.unwrap())
                    .map(move |x| Message::SearchQueryChanged(updated_query.clone(), x)),
            ])
        }

        Message::ClearSearchResults => {
            tile.results = vec![];
            Task::none()
        }
        Message::WindowFocusChanged(wid, focused) => {
            // Agent window should not auto-hide on unfocus
            if tile.agent_window_id == Some(wid) {
                return Task::none();
            }
            tile.focused = focused;
            if !focused {
                Task::done(Message::HideWindow(wid)).chain(Task::done(Message::ClearSearchQuery))
            } else {
                Task::none()
            }
        }

        Message::ClipboardHistory(content) => {
            tile.clipboard_content.insert(0, content);
            Task::none()
        }

        Message::ToggleAgentMode => {
            if !tile.visible {
                // Launcher not shown: open it and switch to agent list
                return Task::batch([
                    open_window(),
                    Task::done(Message::SwitchToPage(Page::AgentList)),
                ]);
            }
            if tile.page == Page::AgentList {
                tile.page = Page::Main;
                let banner_h = permission_banner_height(tile);
                platform::resize_blur_window(52.0 + banner_h, WINDOW_WIDTH as f64);
                Task::batch([
                    Task::done(Message::ClearSearchQuery),
                    Task::done(Message::ClearSearchResults),
                ])
            } else {
                Task::done(Message::SwitchToPage(Page::AgentList))
            }
        }

        Message::AgentSessionSelected(sid) => {
            // Resume an existing session: open agent window and spawn claude --resume
            tile.agent_messages.clear();
            tile.agent_input.clear();
            tile.agent_session_id = Some(sid.clone());
            tile.agent_status = AgentStatus::Thinking;
            tile.agent_markdown = iced::widget::markdown::Content::new();

            let (wid, open_task) = window::open(agent_window_settings());
            tile.agent_window_id = Some(wid);

            let configure = window::run(wid, |handle| {
                let wh = handle.window_handle().expect("Unable to get window handle");
                crate::platform::window_config(&wh);
                crate::platform::create_agent_blur_window(&wh, 720.0, 520.0);
            });

            // Hide the launcher
            let hide = window::latest()
                .map(|x| x.unwrap())
                .map(Message::HideWindow);

            // Note: we don't spawn claude here — the user needs to send a new message first.
            // Just open the window and wait.
            tile.agent_status = AgentStatus::Idle;

            Task::batch([
                open_task.discard().chain(configure).discard(),
                hide,
            ])
        }

        Message::NewAgentSession(prompt) => {
            tile.agent_messages.clear();
            tile.agent_input.clear();
            tile.agent_session_id = None;
            tile.agent_markdown = iced::widget::markdown::Content::new();

            let (wid, open_task) = window::open(agent_window_settings());
            tile.agent_window_id = Some(wid);

            let configure = window::run(wid, |handle| {
                let wh = handle.window_handle().expect("Unable to get window handle");
                crate::platform::window_config(&wh);
                crate::platform::create_agent_blur_window(&wh, 720.0, 520.0);
            });

            // Hide the launcher
            let hide = window::latest()
                .map(|x| x.unwrap())
                .map(Message::HideWindow);

            if prompt.is_empty() {
                tile.agent_status = AgentStatus::Idle;
                Task::batch([
                    open_task.discard().chain(configure).discard(),
                    hide,
                ])
            } else {
                // Spawn claude immediately with the prompt
                tile.agent_messages.push(ChatMessage::User(prompt.clone()));
                tile.agent_messages.push(ChatMessage::Assistant(String::new()));
                tile.agent_status = AgentStatus::Thinking;

                if let Some(ref sender) = tile.sender {
                    spawn_claude(prompt, None, sender.0.clone());
                }

                Task::batch([
                    open_task.discard().chain(configure).discard(),
                    hide,
                ])
            }
        }

        Message::AgentInput(text) => {
            tile.agent_input = text;
            Task::none()
        }

        Message::AgentSubmit => {
            let input = tile.agent_input.trim().to_string();
            if input.is_empty() {
                return Task::none();
            }

            tile.agent_messages.push(ChatMessage::User(input.clone()));
            tile.agent_messages.push(ChatMessage::Assistant(String::new()));
            tile.agent_input.clear();
            tile.agent_status = AgentStatus::Thinking;
            tile.agent_markdown = iced::widget::markdown::Content::new();

            if let Some(ref sender) = tile.sender {
                spawn_claude(
                    input,
                    tile.agent_session_id.clone(),
                    sender.0.clone(),
                );
            }

            Task::none()
        }

        Message::AgentEvent(event) => {
            match event {
                ClaudeEvent::SessionStarted(sid) => {
                    tile.agent_session_id = Some(sid);
                    tile.agent_status = AgentStatus::Streaming;
                }
                ClaudeEvent::TextDelta(delta) => {
                    tile.agent_status = AgentStatus::Streaming;
                    if let Some(ChatMessage::Assistant(text)) = tile.agent_messages.last_mut() {
                        text.push_str(&delta);
                        // Incremental markdown parsing
                        tile.agent_markdown.push_str(&delta);
                    }
                }
                ClaudeEvent::ToolUse { name } => {
                    tile.agent_status = AgentStatus::Streaming;
                    let snippet = format!("\n\n> Running: `{}`\n\n", name);
                    if let Some(ChatMessage::Assistant(text)) = tile.agent_messages.last_mut() {
                        text.push_str(&snippet);
                        tile.agent_markdown.push_str(&snippet);
                    }
                }
                ClaudeEvent::ToolResult(result) => {
                    let snippet = format!("\n```\n{}\n```\n", result);
                    if let Some(ChatMessage::Assistant(text)) = tile.agent_messages.last_mut() {
                        text.push_str(&snippet);
                        tile.agent_markdown.push_str(&snippet);
                    }
                }
                ClaudeEvent::Finished => {
                    tile.agent_status = AgentStatus::Idle;
                }
                ClaudeEvent::Error(err) => {
                    tile.agent_status = AgentStatus::Idle;
                    let snippet = format!("\n\n**Error:** {}\n", err);
                    if let Some(ChatMessage::Assistant(text)) = tile.agent_messages.last_mut() {
                        text.push_str(&snippet);
                        tile.agent_markdown.push_str(&snippet);
                    }
                }
            }
            Task::none()
        }

        Message::AgentWindowClosed(wid) => {
            if tile.agent_window_id == Some(wid) {
                tile.agent_window_id = None;
                platform::clear_agent_blur_window();
            }
            Task::none()
        }

        Message::OpenAccessibilitySettings => {
            platform::open_accessibility_settings();
            Task::none()
        }

        Message::OpenInputMonitoringSettings => {
            platform::open_input_monitoring_settings();
            Task::none()
        }

        Message::RefreshPermissions => {
            tile.missing_accessibility = !platform::check_accessibility();
            tile.missing_input_monitoring = !platform::check_input_monitoring();
            tile.permissions_ok = !tile.missing_accessibility && !tile.missing_input_monitoring;
            Task::none()
        }

        Message::SearchQueryChanged(input, _id) => {
            tile.focus_id = 0;

            if tile.config.haptic_feedback {
                perform_haptic(HapticPattern::Alignment);
            }

            tile.query_lc = input.trim().to_lowercase();
            tile.query = input;
            if tile.query_lc.is_empty() && tile.page != Page::ClipboardHistory {
                tile.results = vec![];
                let banner_h = permission_banner_height(tile);
                platform::resize_blur_window(52.0 + banner_h, WINDOW_WIDTH as f64);
                return Task::none();
            } else if tile.query_lc == "randomvar" {
                let rand_num = rand::random_range(0..100);
                tile.results = vec![App {
                    open_command: AppCommand::Function(Function::RandomVar(rand_num)),
                    desc: "Easter egg".to_string(),
                    icons: None,
                    name: rand_num.to_string(),
                    name_lc: String::new(),
                    localized_name: None,
                }];
                // Don't early-return with resize; fall through to the
                // unified resize logic at the bottom.
            } else if tile.query_lc == "67" {
                tile.results = vec![App {
                    open_command: AppCommand::Function(Function::RandomVar(67)),
                    desc: "Easter egg".to_string(),
                    icons: None,
                    name: 67.to_string(),
                    name_lc: String::new(),
                    localized_name: None,
                }];
            } else if tile.query_lc.ends_with("?") {
                tile.results = vec![App {
                    open_command: AppCommand::Function(Function::GoogleSearch(tile.query.clone())),
                    icons: None,
                    desc: "Web Search".to_string(),
                    name: format!("Search for: {}", tile.query),
                    name_lc: String::new(),
                    localized_name: None,
                }];
            } else if tile.query_lc == "cbhist" {
                tile.page = Page::ClipboardHistory
            } else if tile.query_lc == "main" {
                tile.page = Page::Main
            }
            tile.handle_search_query_changed();

            if tile.results.is_empty()
                && let Some(res) = Expr::from_str(&tile.query).ok()
            {
                tile.results.push(App {
                    open_command: AppCommand::Function(Function::Calculate(res.clone())),
                    desc: RUSTCAST_DESC_NAME.to_string(),
                    icons: None,
                    name: res.eval().map(|x| x.to_string()).unwrap_or("".to_string()),
                    name_lc: "".to_string(),
                    localized_name: None,
                });
            } else if tile.results.is_empty()
                && let Some(conversions) = unit_conversion::convert_query(&tile.query)
            {
                tile.results = conversions
                    .into_iter()
                    .map(|conversion| {
                        let source = format!(
                            "{} {}",
                            unit_conversion::format_number(conversion.source_value),
                            conversion.source_unit.name
                        );
                        let target = format!(
                            "{} {}",
                            unit_conversion::format_number(conversion.target_value),
                            conversion.target_unit.name
                        );
                        App {
                            open_command: AppCommand::Function(Function::CopyToClipboard(
                                ClipBoardContentType::Text(target.clone()),
                            )),
                            desc: source,
                            icons: None,
                            name: target,
                            name_lc: String::new(),
                            localized_name: None,
                        }
                    })
                    .collect();
            } else if tile.results.is_empty()
                && let Some(conversions) = currency_conversion::convert_query(&tile.query)
            {
                tile.results = conversions
                    .into_iter()
                    .map(|c| {
                        let formatted = currency_conversion::format_currency(c.target_value, c.target_code);
                        let target_name = currency_conversion::currency_name_cn(c.target_code);
                        let source_name = currency_conversion::currency_name_cn(c.source_code);
                        let copy_text = formatted.clone();
                        App {
                            open_command: AppCommand::Function(Function::CopyToClipboard(
                                ClipBoardContentType::Text(copy_text),
                            )),
                            desc: format!(
                                "{} {}{} {} → {} · 汇率 {:.4} · {}",
                                currency_conversion::currency_flag(c.source_code),
                                currency_conversion::currency_symbol(c.source_code),
                                currency_conversion::format_currency(c.source_value, c.source_code),
                                source_name,
                                target_name,
                                c.rate,
                                c.updated_at,
                            ),
                            icons: None,
                            name: format!(
                                "{} {}{} {}",
                                currency_conversion::currency_flag(c.target_code),
                                currency_conversion::currency_symbol(c.target_code),
                                formatted,
                                c.target_code,
                            ),
                            name_lc: String::new(),
                            localized_name: None,
                        }
                    })
                    .collect();
            } else if tile.results.is_empty() && is_valid_url(&tile.query) {
                tile.results.push(App {
                    open_command: AppCommand::Function(Function::OpenWebsite(tile.query.clone())),
                    desc: "Web Browsing".to_string(),
                    icons: None,
                    name: "Open Website: ".to_string() + &tile.query,
                    name_lc: "".to_string(),
                    localized_name: None,
                });
            } else if tile.query_lc.split(' ').count() > 1 {
                tile.results.push(App {
                    open_command: AppCommand::Function(Function::GoogleSearch(tile.query.clone())),
                    icons: None,
                    desc: "Web Search".to_string(),
                    name: format!("Search for: {}", tile.query),
                    name_lc: String::new(),
                    localized_name: None,
                });
            } else if tile.results.is_empty() && tile.query_lc == "lemon" {
                tile.results.push(App {
                    open_command: AppCommand::Display,
                    desc: "Easter Egg".to_string(),
                    icons: Some(Handle::from_path(Path::new(
                        "/Applications/Rustcast.app/Contents/Resources/lemon.png",
                    ))),
                    name: "Lemon".to_string(),
                    name_lc: "".to_string(),
                    localized_name: None,
                });
            }
            if !tile.query_lc.is_empty() && tile.page == Page::EmojiSearch {
                tile.results = tile.emoji_apps.all();
            }

            let has_results_now = !tile.results.is_empty();
            let banner_h = permission_banner_height(tile);

            // 2-state resize: only on 0↔non-zero transitions.
            // Resize the blur child window to match content — no wgpu flicker
            // since only the native child NSWindow is resized, not the main window.
            let content_h = if tile.page == Page::ClipboardHistory
                && !tile.clipboard_content.is_empty()
            {
                // search(52) + banner + separator(1) + content(360) + footer(38)
                52.0 + banner_h + 1.0 + 360.0 + 38.0
            } else if has_results_now {
                let rows = std::cmp::min(tile.results.len(), 7) as f64;
                52.0 + banner_h + 1.0 + rows * 52.0 + 38.0
            } else {
                52.0 + banner_h
            };
            platform::resize_blur_window(content_h, WINDOW_WIDTH as f64);

            if has_results_now {
                Task::done(Message::ChangeFocus(ArrowKey::Left))
            } else {
                Task::none()
            }
        }
    }
}

/// Calculate the total height of the permission banner area.
/// Each missing permission contributes 28px row + 4px spacing, plus 8px padding (top+bottom).
fn permission_banner_height(tile: &Tile) -> f64 {
    if tile.permissions_ok {
        return 0.0;
    }
    let count = tile.missing_accessibility as u32 + tile.missing_input_monitoring as u32;
    if count == 0 {
        return 0.0;
    }
    // 4px top padding + (count * 28px rows) + ((count-1) * 4px spacing) + 4px bottom padding
    4.0 + (count as f64 * 28.0) + ((count - 1) as f64 * 4.0) + 4.0
}

fn open_window() -> Task<Message> {
    let (id, open_task) = window::open(default_settings());
    let configure = window::run(id, |handle| {
        let wh = handle.window_handle().expect("Unable to get window handle");
        crate::platform::window_config(&wh);
        crate::platform::create_blur_child_window(
            &wh,
            crate::app::WINDOW_WIDTH as f64,
            52.0,
        );
    });
    Task::chain(
        open_task
            .discard()
            .chain(configure)
            .map(|_| Message::OpenWindow),
        operation::focus("query"),
    )
}
