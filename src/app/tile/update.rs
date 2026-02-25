//! This handles the update logic for the tile (AKA Coco's main window)

macro_rules! coco_log {
    ($($arg:tt)*) => {{
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true).append(true).open("/Users/kcsx/coco_debug.log")
        {
            let _ = writeln!(f, "[{:.3}] {}", std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs_f64() % 10000.0,
                format!($($arg)*));
        }
    }};
}

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
use crate::app::menubar::menu_icon;
use crate::app::{
    Message, Page, agent_window_settings,
    tile::{AppIndex, Tile},
};
use crate::app::{ROW_HEIGHT, WINDOW_WIDTH};
use crate::calculator::Expr;
use crate::clipboard::ClipBoardContentType;
use crate::commands::Function;
use crate::config::Config;
use crate::currency_conversion;
use crate::platform::{self, perform_haptic};
use crate::unit_conversion;
use crate::utils::is_valid_url;
use crate::{app::ArrowKey, platform::focus_this_app};
use crate::{app::COCO_DESC_NAME, platform::get_installed_apps};
use crate::{app::Move, platform::HapticPattern};

pub fn handle_update(tile: &mut Tile, message: Message) -> Task<Message> {
    match message {
        Message::OpenWindow(opt_id) => {
            coco_log!(
                "OpenWindow({opt_id:?}): prev main_wid={:?} visible={} show_anim={} hide_anim={}",
                tile.main_window_id,
                tile.visible,
                tile.show_animating,
                tile.hide_animating
            );
            if let Some(wid) = opt_id {
                tile.main_window_id = Some(wid);
            }
            tile.capture_frontmost();
            focus_this_app();
            tile.focused = true;
            tile.visible = true;
            tile.hide_animating = false;
            tile.show_animating = true;
            tile.pending_window_height = None;
            tile.window_resize_token = tile.window_resize_token.wrapping_add(1);
            // NOTE: On macOS 26 (Tahoe), the AXIsProcessTrusted() and
            // CGPreflightListenEventAccess() APIs return false even when
            // the user has granted permissions in System Settings. This is
            // a known issue with the new OS. We skip the permission banner
            // entirely to avoid confusing users.
            // TODO: Re-enable when Apple fixes these APIs on macOS 26+.
            tile.permissions_ok = true;
            tile.missing_accessibility = false;
            tile.missing_input_monitoring = false;
            // Always refresh zero-query cache on open
            let zq = build_zero_query_results_inner(&tile.options, tile.config.theme.show_icons);
            tile.zero_query_cache = zq;
            // Compute target height for content
            let banner_h = permission_banner_height(tile);
            let target_h = if tile.page == Page::ClipboardHistory && tile.clipboard_store.len() > 0
            {
                tile.clipboard_rebuild_filtered();
                blur_height(banner_h, 360.0)
            } else if tile.page == Page::Main && tile.query_lc.is_empty() {
                // Zero-query state
                tile.results = tile.zero_query_cache.clone();
                tile.focus_id = 0;
                if !tile.results.is_empty() {
                    let content_h = zero_query_scrollable_height(&tile.results)
                        .min(crate::app::MAX_RESULTS_SCROLL_HEIGHT);
                    blur_height(banner_h, content_h)
                } else {
                    blur_height(banner_h, 0.0)
                }
            } else if tile.page == Page::Main && !tile.query_lc.is_empty() {
                // Restore previous search results
                tile.handle_search_query_changed();
                if !tile.results.is_empty() {
                    let rows = std::cmp::min(tile.results.len(), 7) as f64;
                    blur_height(banner_h, rows * ROW_HEIGHT)
                } else {
                    blur_height(banner_h, 0.0)
                }
            } else {
                blur_height(banner_h, 0.0)
            };
            // For the very first frame, directly resize the blur child window
            // (window::resize won't take effect until next event loop iteration).
            platform::resize_blur_window(target_h, WINDOW_WIDTH as f64);
            // Also resize the iced window so it matches content going forward.
            // Reset cached targets first so snap_resize doesn't skip.
            tile.target_blur_height = 0.0;
            tile.target_window_height = 0.0;

            platform::animate_show();

            Task::batch([tile.snap_resize(target_h), operation::focus("query")])
        }
        Message::HideTrayIcon => {
            tile.tray_icon = None;
            tile.config.show_trayicon = false;
            let home = std::env::var("HOME").unwrap();
            let confg_str = toml::to_string(&tile.config).unwrap();
            thread::spawn(move || fs::write(home + "/.config/coco/config.toml", confg_str));
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

            // Actions overlay: ESC closes it
            if tile.show_actions {
                tile.show_actions = false;
                tile.actions.clear();
                return Task::none();
            }

            if tile.page == Page::EmojiSearch && !tile.query_lc.is_empty() {
                return Task::none();
            }

            // ClipboardHistory: ESC clears search first, then goes back to Main
            if tile.page == Page::ClipboardHistory {
                if !tile.query_lc.is_empty() {
                    tile.query.clear();
                    tile.query_lc.clear();
                    tile.focus_id = 0;
                    tile.clipboard_rebuild_filtered();
                    let banner_h = permission_banner_height(tile);
                    let content_h = if tile.clipboard_display_count() > 0 {
                        360.0
                    } else {
                        0.0
                    };
                    return tile.snap_resize(blur_height(banner_h, content_h));
                } else {
                    tile.page = Page::Main;
                    tile.results = tile.zero_query_cache.clone();
                    tile.focus_id = 0;
                    let banner_h = permission_banner_height(tile);
                    let content_h = if !tile.results.is_empty() {
                        zero_query_scrollable_height(&tile.results)
                            .min(crate::app::MAX_RESULTS_SCROLL_HEIGHT)
                    } else {
                        0.0
                    };
                    let resize = tile.snap_resize(blur_height(banner_h, content_h));
                    return Task::batch([resize, Task::done(Message::ClearSearchQuery)]);
                }
            }

            // AgentList / WindowSwitcher: ESC goes back to Main
            if tile.page == Page::AgentList || tile.page == Page::WindowSwitcher {
                tile.page = Page::Main;
                // Repopulate zero-query state from cache
                tile.results = tile.zero_query_cache.clone();
                tile.focus_id = 0;
                let banner_h = permission_banner_height(tile);
                let content_h = if !tile.results.is_empty() {
                    zero_query_scrollable_height(&tile.results)
                        .min(crate::app::MAX_RESULTS_SCROLL_HEIGHT)
                } else {
                    0.0
                };
                let resize = tile.snap_resize(blur_height(banner_h, content_h));
                return Task::batch([resize, Task::done(Message::ClearSearchQuery)]);
            }

            if tile.query_lc.is_empty() {
                Task::batch([
                    Task::done(Message::HideWindow(id)),
                    Task::done(Message::ReturnFocus),
                ])
            } else {
                // First ESC: clear query and show zero-query state
                tile.page = Page::Main;
                tile.query.clear();
                tile.query_lc.clear();
                tile.results = tile.zero_query_cache.clone();
                tile.focus_id = 0;
                let banner_h = permission_banner_height(tile);
                let content_h = if !tile.results.is_empty() {
                    zero_query_scrollable_height(&tile.results)
                        .min(crate::app::MAX_RESULTS_SCROLL_HEIGHT)
                } else {
                    0.0
                };
                tile.snap_resize(blur_height(banner_h, content_h))
            }
        }

        Message::ClearSearchQuery => {
            tile.query_lc = String::new();
            tile.query = String::new();
            tile.last_query_edit_time = None;
            Task::none()
        }

        Message::ChangeFocus(key) => {
            // Redirect to actions overlay if open
            if tile.show_actions {
                return Task::done(Message::ActionFocusChanged(key));
            }

            let len = match tile.page {
                Page::ClipboardHistory => tile.clipboard_display_count() as u32,
                Page::EmojiSearch => tile.results.len() as u32,
                Page::AgentList => (1 + tile.agent_sessions.len()) as u32,
                Page::WindowSwitcher => tile.window_list.len() as u32,
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

            let (wrapped_up, wrapped_down) = match &key {
                ArrowKey::Up => (tile.focus_id > old_focus_id, false),
                ArrowKey::Down => (false, tile.focus_id < old_focus_id),
                _ => (false, false),
            };

            let y = match tile.page {
                Page::EmojiSearch => {
                    let quantity = 5.0_f32;
                    if wrapped_down {
                        0.0
                    } else if wrapped_up {
                        (len.saturating_sub(1)) as f32 * quantity
                    } else {
                        tile.focus_id as f32 * quantity
                    }
                }
                Page::ClipboardHistory => {
                    let row_h = 32.0_f32;
                    let viewport_h = 360.0_f32;
                    let total_h = len as f32 * row_h;
                    scroll_offset_with_mid_threshold(
                        tile.focus_id,
                        len,
                        row_h,
                        viewport_h,
                        total_h,
                        wrapped_up,
                        wrapped_down,
                    )
                }
                Page::Main | Page::AgentList | Page::WindowSwitcher => {
                    let row_h = ROW_HEIGHT as f32;
                    let (focus_y, total_h, viewport_h) = if tile.page == Page::Main
                        && tile.query_lc.is_empty()
                        && tile.results.iter().any(|a| a.category.is_some())
                    {
                        (
                            zero_query_focus_offset(&tile.results, tile.focus_id as usize) as f32,
                            zero_query_scrollable_height(&tile.results) as f32,
                            zero_query_scrollable_height(&tile.results)
                                .min(crate::app::MAX_RESULTS_SCROLL_HEIGHT)
                                as f32,
                        )
                    } else {
                        let total_h = len as f32 * row_h;
                        let viewport_h = total_h.min(crate::app::MAX_RESULTS_SCROLL_HEIGHT as f32);
                        (tile.focus_id as f32 * row_h, total_h, viewport_h)
                    };

                    scroll_offset_with_mid_threshold_from_focus_y(
                        focus_y,
                        len,
                        row_h,
                        viewport_h,
                        total_h,
                        wrapped_up,
                        wrapped_down,
                    )
                }
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
            // Execute focused action if overlay is open
            if tile.show_actions {
                if let Some(item) = tile.actions.get(tile.action_focus_id as usize) {
                    return Task::done(Message::ExecuteAction(item.action.clone()));
                }
                return Task::none();
            }

            if tile.page == Page::ClipboardHistory {
                let indices = tile.clipboard_display_indices();
                eprintln!(
                    "[clipboard] OpenFocused: focus_id={}, indices.len={}, store.len={}",
                    tile.focus_id,
                    indices.len(),
                    tile.clipboard_store.len()
                );
                if let Some(&entry_idx) = indices.get(tile.focus_id as usize) {
                    eprintln!("[clipboard] entry_idx={}", entry_idx);
                    if let Some(entry) = tile.clipboard_store.get(entry_idx) {
                        eprintln!("[clipboard] copying: {:?}", entry.preview_title);
                        // Copy content to system clipboard
                        match &entry.content {
                            ClipBoardContentType::Text(text) => {
                                arboard::Clipboard::new()
                                    .unwrap()
                                    .set_text(text.clone())
                                    .ok();
                            }
                            ClipBoardContentType::Image(img) => {
                                arboard::Clipboard::new()
                                    .unwrap()
                                    .set_image(img.to_owned_img())
                                    .ok();
                            }
                        }
                        // Hide window and return focus
                        let hide = if let Some(wid) = tile.main_window_id {
                            Task::done(Message::HideWindow(wid))
                        } else {
                            Task::none()
                        };
                        return Task::batch([hide, Task::done(Message::ReturnFocus)]);
                    } else {
                        eprintln!("[clipboard] entry not found at idx {}", entry_idx);
                    }
                } else {
                    eprintln!(
                        "[clipboard] focus_id {} out of range for indices",
                        tile.focus_id
                    );
                }
                return Task::none();
            }

            if tile.page == Page::WindowSwitcher {
                if let Some(win) = tile.window_list.get(tile.focus_id as usize) {
                    return Task::done(Message::FocusWindow(win.owner_pid, win.window_id));
                }
                return Task::none();
            }

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
                        return Task::done(Message::AgentSessionSelected(
                            session.session_id.clone(),
                        ));
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
                    std::env::var("HOME").unwrap_or("".to_owned()) + "/.config/coco/config.toml",
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

            if is_open_hotkey || is_clipboard_hotkey {
                coco_log!(
                    "KeyPressed: visible={} focused={} show_anim={} hide_anim={} main_wid={:?}",
                    tile.visible,
                    tile.focused,
                    tile.show_animating,
                    tile.hide_animating,
                    tile.main_window_id
                );

                // During SHOW animation: snap to visible, then fall through to hide.
                if tile.show_animating {
                    coco_log!("KeyPressed: interrupting show animation -> hide");
                    platform::cancel_animation_snap_visible();
                    tile.show_animating = false;
                    tile.hide_animating = false;
                    tile.visible = true;
                } else if tile.hide_animating {
                    // During HIDE animation: cancel hide and restore visible state.
                    coco_log!("KeyPressed: cancelling hide animation, snap to visible");
                    platform::cancel_animation_snap_visible();
                    tile.hide_animating = false;
                    tile.show_animating = false;
                    tile.visible = true;
                    tile.focused = true;
                    focus_this_app();
                    return Task::none();
                } else {
                    // Debounce only when not animating, so users can interrupt animations.
                    let now_inst = std::time::Instant::now();
                    if let Some(last) = tile.last_hotkey_time {
                        if now_inst.duration_since(last) < std::time::Duration::from_millis(300) {
                            coco_log!(
                                "KeyPressed: debounce, ignoring ({}ms since last)",
                                now_inst.duration_since(last).as_millis()
                            );
                            return Task::none();
                        }
                    }
                    tile.last_hotkey_time = Some(now_inst);
                }

                if !tile.visible {
                    coco_log!("KeyPressed: showing hidden main window");
                    // Always reset to Main when opening fresh;
                    // clipboard hotkey will switch page AFTER open.
                    tile.page = Page::Main;
                    tile.focus_id = 0;
                    if let Some(wid) = tile.main_window_id {
                        platform::prepare_show_animation();
                        let show = window::set_mode::<Message>(wid, window::Mode::Windowed)
                            .chain(Task::done(Message::OpenWindow(Some(wid))));
                        if is_clipboard_hotkey {
                            return show
                                .chain(Task::done(Message::SwitchToPage(Page::ClipboardHistory)));
                        }
                        return show;
                    }
                    return Task::none();
                }

                coco_log!("KeyPressed: triggering HideWindow via main_wid");
                // Use main_window_id directly instead of window::latest()
                // to avoid races with stale window IDs.
                if let Some(wid) = tile.main_window_id {
                    Task::done(Message::HideWindow(wid))
                } else {
                    Task::none()
                }
            } else {
                Task::none()
            }
        }

        Message::SwitchToPage(page) => {
            tile.page = page.clone();
            let banner_h = permission_banner_height(tile);
            let resize = match &tile.page {
                Page::ClipboardHistory => {
                    tile.clipboard_rebuild_filtered();
                    let content_h = if tile.clipboard_display_count() > 0 {
                        360.0
                    } else {
                        0.0
                    };
                    tile.snap_resize(blur_height(banner_h, content_h))
                }
                Page::AgentList => {
                    tile.agent_sessions = crate::agent::session::list_sessions();
                    let rows = std::cmp::min(1 + tile.agent_sessions.len(), 7) as f64;
                    tile.snap_resize(blur_height(banner_h, rows * ROW_HEIGHT))
                }
                Page::WindowSwitcher => {
                    tile.window_list = platform::get_window_list();
                    let rows = std::cmp::min(tile.window_list.len(), 7) as f64;
                    tile.snap_resize(blur_height(
                        banner_h,
                        if rows > 0.0 { rows * ROW_HEIGHT } else { 0.0 },
                    ))
                }
                _ => tile.snap_resize(blur_height(banner_h, 0.0)),
            };
            Task::batch([
                resize,
                Task::done(Message::ClearSearchQuery),
                Task::done(Message::ClearSearchResults),
            ])
        }

        Message::RunFunction(command) => {
            command.execute(&tile.config, &tile.query);

            let return_focus_task = match &command {
                Function::OpenApp(_)
                | Function::ActivateApp(_)
                | Function::OpenPrefPane
                | Function::GoogleSearch(_)
                | Function::ShowInFinder(_) => Task::none(),
                _ => Task::done(Message::ReturnFocus),
            };

            if tile.config.buffer_rules.clear_on_enter {
                if let Some(wid) = tile.main_window_id {
                    Task::done(Message::HideWindow(wid))
                        .chain(Task::done(Message::ClearSearchQuery))
                        .chain(return_focus_task)
                } else {
                    Task::none()
                }
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

            coco_log!(
                "HideWindow({a:?}): visible={} show_anim={} hide_anim={} main_wid={:?}",
                tile.visible,
                tile.show_animating,
                tile.hide_animating,
                tile.main_window_id
            );

            // Ignore stale/non-main requests.
            if tile.main_window_id != Some(a) {
                coco_log!("HideWindow: non-main window, ignoring");
                return Task::none();
            }

            // Already hidden or already hiding.
            if !tile.visible && !tile.hide_animating {
                coco_log!("HideWindow: already hidden, ignoring");
                return Task::none();
            }
            if tile.hide_animating {
                coco_log!("HideWindow: duplicate, ignoring");
                return Task::none();
            }

            // If a show animation is active, snap to visible first, then hide.
            if tile.show_animating {
                platform::cancel_animation_snap_visible();
                tile.show_animating = false;
            }

            tile.visible = false;
            tile.focused = false;
            tile.show_animating = false;
            tile.hide_animating = true;
            tile.pending_window_height = None;
            tile.window_resize_token = tile.window_resize_token.wrapping_add(1);
            // Always reset to Main so next open starts fresh
            tile.page = Page::Main;

            // Start hide animation instead of closing immediately
            coco_log!("HideWindow: starting hide animation for {:?}", a);
            platform::animate_hide();
            Task::none()
        }

        Message::NativeHideComplete => {
            if !tile.hide_animating {
                coco_log!("NativeHideComplete: stale completion ignored");
                return Task::none();
            }
            coco_log!(
                "NativeHideComplete: visible={} main_wid={:?}",
                tile.visible,
                tile.main_window_id
            );
            tile.hide_animating = false;
            tile.show_animating = false;
            if tile.visible {
                coco_log!("NativeHideComplete: visible=true, hide was cancelled");
                return Task::none();
            }
            tile.restore_frontmost();

            let hide_task = if let Some(wid) = tile.main_window_id {
                window::set_mode::<Message>(wid, window::Mode::Hidden)
            } else {
                Task::none()
            };

            if tile.config.buffer_rules.clear_on_hide {
                Task::batch([
                    hide_task,
                    Task::done(Message::ClearSearchQuery),
                    Task::done(Message::ClearSearchResults),
                ])
            } else {
                hide_task
            }
        }

        Message::NativeShowComplete => {
            if !tile.show_animating {
                coco_log!("NativeShowComplete: stale completion ignored");
                return Task::none();
            }
            coco_log!("NativeShowComplete");
            tile.show_animating = false;
            platform::reset_show_animation();
            Task::none()
        }

        Message::ApplyDebouncedWindowResize(token, target_h) => {
            use crate::app::WINDOW_WIDTH;
            coco_log!(
                "ApplyDebouncedWindowResize token={} target={:.1}",
                token,
                target_h
            );

            if token != tile.window_resize_token {
                coco_log!(
                    "ApplyDebouncedWindowResize stale token current={}",
                    tile.window_resize_token
                );
                return Task::none();
            }
            let Some(pending) = tile.pending_window_height else {
                coco_log!("ApplyDebouncedWindowResize missing pending");
                return Task::none();
            };
            if (pending - target_h).abs() >= 1.0 {
                coco_log!(
                    "ApplyDebouncedWindowResize target mismatch pending={:.1}",
                    pending
                );
                return Task::none();
            }

            let typing_active = tile.visible
                && tile.page == Page::Main
                && !tile.query_lc.is_empty()
                && !tile.hide_animating;
            let window_height_changed = (tile.target_window_height - target_h).abs() >= 1.0;
            if typing_active
                && window_height_changed
                && let Some(last_edit) = tile.last_query_edit_time
            {
                let idle_threshold = if tile.results.is_empty() {
                    std::time::Duration::from_millis(900)
                } else if tile.query_lc.chars().count() <= 2 {
                    std::time::Duration::from_millis(700)
                } else {
                    std::time::Duration::from_millis(450)
                };
                let elapsed = last_edit.elapsed();
                if elapsed < idle_threshold {
                    let remaining = idle_threshold.saturating_sub(elapsed);
                    let remaining_ms = remaining.as_millis().max(1) as u64;
                    coco_log!(
                        "ApplyDebouncedWindowResize height gated by recent typing qlen={} elapsed={}ms remain={}ms target={:.1}",
                        tile.query_lc.chars().count(),
                        elapsed.as_millis(),
                        remaining_ms,
                        target_h
                    );
                    return Task::perform(
                        async move {
                            tokio::time::sleep(std::time::Duration::from_millis(remaining_ms))
                                .await;
                            (token, target_h)
                        },
                        |(token, height)| Message::ApplyDebouncedWindowResize(token, height),
                    );
                }
            }

            tile.pending_window_height = None;

            let mut tasks = Vec::new();
            let mut main_resized_sync = false;

            // In the debounced path, prefer a synchronous native main-window
            // resize so the blur child can be updated immediately after without
            // a visible one-frame height mismatch.
            if (tile.target_window_height - target_h).abs() >= 1.0 {
                tile.target_window_height = target_h;
                if platform::resize_main_window_top_anchored(target_h, WINDOW_WIDTH as f64) {
                    main_resized_sync = true;
                    coco_log!(
                        "ApplyDebouncedWindowResize native top-anchored main resize applied {:.1} typing_active={}",
                        target_h,
                        typing_active
                    );
                } else {
                    coco_log!(
                        "ApplyDebouncedWindowResize native main resize unavailable -> iced resize {:.1}",
                        target_h
                    );
                    if let Some(id) = tile.main_window_id {
                        tasks.push(window::resize::<Message>(
                            id,
                            iced::Size {
                                width: WINDOW_WIDTH,
                                height: target_h as f32,
                            },
                        ));
                    }
                }
            }

            if (tile.target_blur_height - target_h).abs() >= 1.0 {
                tile.target_blur_height = target_h;
                if !main_resized_sync && !tasks.is_empty() {
                    coco_log!(
                        "ApplyDebouncedWindowResize blur resize before async iced main resize {:.1} (fallback path)",
                        target_h
                    );
                }
                platform::resize_blur_window(target_h, WINDOW_WIDTH as f64);
            }

            if tasks.is_empty() {
                Task::none()
            } else {
                Task::batch(tasks)
            }
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
            if tile.main_window_id != Some(wid) {
                return Task::none();
            }
            coco_log!(
                "WindowFocusChanged({wid:?}, focused={focused}): show_anim={} hide_anim={} visible={}",
                tile.show_animating,
                tile.hide_animating,
                tile.visible
            );
            tile.focused = focused;
            if !focused {
                // Don't auto-hide during show animation — the window may
                // briefly lose focus while appearing; hiding it now would
                // cancel the animation and make it seem unresponsive.
                if tile.show_animating {
                    coco_log!("WindowFocusChanged: ignoring unfocus during show animation");
                    return Task::none();
                }
                // Don't auto-hide if we're already in a hide animation
                if tile.hide_animating {
                    coco_log!("WindowFocusChanged: ignoring unfocus during hide animation");
                    return Task::none();
                }
                coco_log!("WindowFocusChanged: triggering HideWindow");
                Task::done(Message::HideWindow(wid))
            } else {
                Task::none()
            }
        }

        Message::ClipboardHistory(content) => {
            tile.clipboard_store.push(content);
            tile.clipboard_rebuild_filtered();
            Task::none()
        }

        Message::ToggleAgentMode => {
            if !tile.visible {
                // Launcher not shown: open it and switch to agent list
                if let Some(wid) = tile.main_window_id {
                    platform::prepare_show_animation();
                    return window::set_mode::<Message>(wid, window::Mode::Windowed)
                        .chain(Task::done(Message::OpenWindow(Some(wid))))
                        .chain(Task::done(Message::SwitchToPage(Page::AgentList)));
                }
                return Task::none();
            }
            if tile.page == Page::AgentList {
                tile.page = Page::Main;
                let banner_h = permission_banner_height(tile);
                let resize = tile.snap_resize(blur_height(banner_h, 0.0));
                Task::batch([
                    resize,
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
            let hide = if let Some(main_wid) = tile.main_window_id {
                Task::done(Message::HideWindow(main_wid))
            } else {
                Task::none()
            };

            // Note: we don't spawn claude here — the user needs to send a new message first.
            // Just open the window and wait.
            tile.agent_status = AgentStatus::Idle;

            Task::batch([open_task.discard().chain(configure).discard(), hide])
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
            let hide = if let Some(main_wid) = tile.main_window_id {
                Task::done(Message::HideWindow(main_wid))
            } else {
                Task::none()
            };

            if prompt.is_empty() {
                tile.agent_status = AgentStatus::Idle;
                Task::batch([open_task.discard().chain(configure).discard(), hide])
            } else {
                // Spawn claude immediately with the prompt
                tile.agent_messages.push(ChatMessage::User(prompt.clone()));
                tile.agent_messages
                    .push(ChatMessage::Assistant(String::new()));
                tile.agent_status = AgentStatus::Thinking;

                if let Some(ref sender) = tile.sender {
                    spawn_claude(prompt, None, sender.0.clone());
                }

                Task::batch([open_task.discard().chain(configure).discard(), hide])
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
            tile.agent_messages
                .push(ChatMessage::Assistant(String::new()));
            tile.agent_input.clear();
            tile.agent_status = AgentStatus::Thinking;
            tile.agent_markdown = iced::widget::markdown::Content::new();

            if let Some(ref sender) = tile.sender {
                spawn_claude(input, tile.agent_session_id.clone(), sender.0.clone());
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

        Message::ShowActions => {
            if tile.show_actions {
                // Toggle off
                tile.show_actions = false;
                tile.actions.clear();
                return Task::none();
            }
            if let Some(app) = tile.results.get(tile.focus_id as usize) {
                let actions = crate::app::actions::compute_actions(app);
                if !actions.is_empty() {
                    tile.action_target_name = app.name.clone();
                    tile.actions = actions;
                    tile.action_focus_id = 0;
                    tile.show_actions = true;
                }
            }
            Task::none()
        }

        Message::ExecuteAction(action) => {
            let func = crate::app::actions::action_to_function(&action);
            tile.show_actions = false;
            tile.actions.clear();
            Task::done(Message::RunFunction(func))
        }

        Message::ActionFocusChanged(key) => {
            if !tile.show_actions || tile.actions.is_empty() {
                return Task::none();
            }
            let len = tile.actions.len() as u32;
            match key {
                ArrowKey::Down => {
                    tile.action_focus_id = (tile.action_focus_id + 1) % len;
                }
                ArrowKey::Up => {
                    tile.action_focus_id = (tile.action_focus_id + len - 1) % len;
                }
                _ => {}
            }
            Task::none()
        }

        Message::FocusWindow(pid, window_id) => {
            platform::focus_window(pid, window_id);
            if let Some(wid) = tile.main_window_id {
                Task::done(Message::HideWindow(wid)).chain(Task::done(Message::ClearSearchQuery))
            } else {
                Task::none()
            }
        }

        Message::ClipboardTogglePinFocused => {
            if tile.page != Page::ClipboardHistory {
                return Task::none();
            }
            let indices = tile.clipboard_display_indices();
            if let Some(&entry_idx) = indices.get(tile.focus_id as usize) {
                if let Some(entry) = tile.clipboard_store.get(entry_idx) {
                    let id = entry.id;
                    tile.clipboard_store.toggle_pin(id);
                    tile.clipboard_rebuild_filtered();
                }
            }
            Task::none()
        }

        Message::ClipboardDeleteFocused => {
            if tile.page != Page::ClipboardHistory {
                return Task::none();
            }
            let indices = tile.clipboard_display_indices();
            if let Some(&entry_idx) = indices.get(tile.focus_id as usize) {
                if let Some(entry) = tile.clipboard_store.get(entry_idx) {
                    let id = entry.id;
                    tile.clipboard_store.delete(id);
                    tile.clipboard_rebuild_filtered();
                    // Adjust focus_id
                    let count = tile.clipboard_display_count();
                    if count == 0 {
                        tile.focus_id = 0;
                    } else if tile.focus_id as usize >= count {
                        tile.focus_id = (count - 1) as u32;
                    }
                    let banner_h = permission_banner_height(tile);
                    let content_h = if count > 0 { 360.0 } else { 0.0 };
                    return tile.snap_resize(blur_height(banner_h, content_h));
                }
            }
            Task::none()
        }

        Message::SearchQueryChanged(input, _id) => {
            tile.focus_id = 0;

            if tile.config.haptic_feedback {
                perform_haptic(HapticPattern::Alignment);
            }

            tile.query_lc = input.trim().to_lowercase();
            tile.query = input;
            tile.last_query_edit_time = if tile.query_lc.is_empty() {
                None
            } else {
                Some(std::time::Instant::now())
            };

            // ClipboardHistory page: filter in-place, don't switch pages
            if tile.page == Page::ClipboardHistory {
                tile.focus_id = 0;
                tile.clipboard_rebuild_filtered();
                let banner_h = permission_banner_height(tile);
                let content_h = if tile.clipboard_display_count() > 0 {
                    360.0
                } else {
                    0.0
                };
                return tile.snap_resize(blur_height(banner_h, content_h));
            }

            if tile.query_lc.is_empty() && tile.page != Page::ClipboardHistory {
                // Zero-query state: use cache (rebuilt on OpenWindow)
                tile.results = tile.zero_query_cache.clone();
                let banner_h = permission_banner_height(tile);
                let content_h = if tile.results.is_empty() {
                    0.0
                } else {
                    zero_query_scrollable_height(&tile.results)
                        .min(crate::app::MAX_RESULTS_SCROLL_HEIGHT)
                };
                let resize = tile.snap_resize(blur_height(banner_h, content_h));
                tile.focus_id = 0;
                return resize;
            } else if tile.query_lc == "randomvar" {
                let rand_num = rand::random_range(0..100);
                tile.results = vec![App {
                    open_command: AppCommand::Function(Function::RandomVar(rand_num)),
                    desc: "Easter egg".to_string(),
                    icons: None,
                    name: rand_num.to_string(),
                    name_lc: String::new(),
                    localized_name: None,
                    category: None,
                    bundle_path: None,
                    bundle_id: None,
                    pid: None,
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
                    category: None,
                    bundle_path: None,
                    bundle_id: None,
                    pid: None,
                }];
            } else if tile.query_lc.ends_with("?") {
                tile.results = vec![App {
                    open_command: AppCommand::Function(Function::GoogleSearch(tile.query.clone())),
                    icons: None,
                    desc: "Web Search".to_string(),
                    name: format!("Search for: {}", tile.query),
                    name_lc: String::new(),
                    localized_name: None,
                    category: None,
                    bundle_path: None,
                    bundle_id: None,
                    pid: None,
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
                    desc: COCO_DESC_NAME.to_string(),
                    icons: None,
                    name: res.eval().map(|x| x.to_string()).unwrap_or("".to_string()),
                    name_lc: "".to_string(),
                    localized_name: None,
                    category: None,
                    bundle_path: None,
                    bundle_id: None,
                    pid: None,
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
                            category: None,
                            bundle_path: None,
                            bundle_id: None,
                            pid: None,
                        }
                    })
                    .collect();
            } else if tile.results.is_empty()
                && let Some(conversions) = currency_conversion::convert_query(&tile.query)
            {
                tile.results = conversions
                    .into_iter()
                    .map(|c| {
                        let formatted =
                            currency_conversion::format_currency(c.target_value, c.target_code);
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
                            category: None,
                            bundle_path: None,
                            bundle_id: None,
                            pid: None,
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
                    category: None,
                    bundle_path: None,
                    bundle_id: None,
                    pid: None,
                });
            } else if tile.query_lc.split(' ').count() > 1 {
                tile.results.push(App {
                    open_command: AppCommand::Function(Function::GoogleSearch(tile.query.clone())),
                    icons: None,
                    desc: "Web Search".to_string(),
                    name: format!("Search for: {}", tile.query),
                    name_lc: String::new(),
                    localized_name: None,
                    category: None,
                    bundle_path: None,
                    bundle_id: None,
                    pid: None,
                });
            } else if tile.results.is_empty() && tile.query_lc == "lemon" {
                tile.results.push(App {
                    open_command: AppCommand::Display,
                    desc: "Easter Egg".to_string(),
                    icons: Some(Handle::from_path(Path::new(
                        "/Applications/Coco.app/Contents/Resources/lemon.png",
                    ))),
                    name: "Lemon".to_string(),
                    name_lc: "".to_string(),
                    localized_name: None,
                    category: None,
                    bundle_path: None,
                    bundle_id: None,
                    pid: None,
                });
            }
            if !tile.query_lc.is_empty() && tile.page == Page::EmojiSearch {
                tile.results = tile.emoji_apps.all();
            }

            let has_results_now = !tile.results.is_empty();
            let banner_h = permission_banner_height(tile);

            // Resize the blur child window to match content — no wgpu flicker
            // since only the native child NSWindow is resized, not the main window.
            let scrollable_h =
                if tile.page == Page::ClipboardHistory && tile.clipboard_display_count() > 0 {
                    360.0
                } else if has_results_now {
                    let rows = std::cmp::min(tile.results.len(), 7) as f64;
                    rows * ROW_HEIGHT
                } else {
                    0.0
                };
            let resize = tile.snap_resize(blur_height(banner_h, scrollable_h));

            if has_results_now {
                Task::batch([resize, Task::done(Message::ChangeFocus(ArrowKey::Left))])
            } else {
                resize
            }
        }
    }
}

fn scroll_offset_with_mid_threshold(
    focus_id: u32,
    len: u32,
    row_h: f32,
    viewport_h: f32,
    total_h: f32,
    wrapped_up: bool,
    wrapped_down: bool,
) -> f32 {
    scroll_offset_with_mid_threshold_from_focus_y(
        focus_id as f32 * row_h,
        len,
        row_h,
        viewport_h,
        total_h,
        wrapped_up,
        wrapped_down,
    )
}

fn scroll_offset_with_mid_threshold_from_focus_y(
    focus_y: f32,
    _len: u32,
    row_h: f32,
    viewport_h: f32,
    total_h: f32,
    wrapped_up: bool,
    wrapped_down: bool,
) -> f32 {
    let max_scroll = (total_h - viewport_h).max(0.0);
    if wrapped_down {
        return 0.0;
    }
    if wrapped_up {
        return max_scroll;
    }

    // Native-like behavior: keep top items pinned until focus reaches roughly
    // the middle of the viewport, then scroll to keep the focused row near the
    // center line.
    let threshold_before_scroll = (viewport_h * 0.5 - row_h).max(0.0);
    (focus_y - threshold_before_scroll).clamp(0.0, max_scroll)
}

/// Build the zero-query results: running apps + recent apps (from history).
///
/// Icons are resolved from the pre-loaded installed-apps index (cheap lookup)
/// instead of calling NSWorkspace.iconForFile on the main thread.
///
/// Takes the options index and show_icons flag separately to avoid borrowing
/// all of `Tile` while we need to mutate other fields.
fn build_zero_query_results_inner(options: &AppIndex, show_icons: bool) -> Vec<App> {
    use crate::app::apps::AppCategory;
    use crate::history::{History, format_relative_time};

    let mut results = Vec::new();

    // Running apps — fetch list WITHOUT icons (fast), then patch icons from cache.
    let mut running = platform::get_running_apps(false);
    for app in &mut running {
        if show_icons {
            app.icons = find_icon_in_index(options, app.bundle_path.as_deref());
        }
    }
    let running_paths: std::collections::HashSet<String> = running
        .iter()
        .filter_map(|a| a.bundle_path.clone())
        .collect();
    results.extend(running);

    // Recent apps from history (deduplicated against running)
    let history = History::load();
    let recent = history.top_recent(5);
    for entry in recent {
        if running_paths.contains(&entry.bundle_path) {
            continue;
        }
        if !std::path::Path::new(&entry.bundle_path).exists() {
            continue;
        }
        let icon = if show_icons {
            find_icon_in_index(options, Some(&entry.bundle_path))
        } else {
            None
        };
        let time_str = format!("Last: {}", format_relative_time(&entry.last_used));
        results.push(App {
            open_command: AppCommand::Function(Function::OpenApp(entry.bundle_path.clone())),
            desc: time_str,
            icons: icon,
            name: entry.name.clone(),
            name_lc: String::new(),
            localized_name: None,
            category: Some(AppCategory::Recent),
            bundle_path: Some(entry.bundle_path),
            bundle_id: None,
            pid: None,
        });
    }

    results
}

/// Look up an icon from the pre-loaded installed-apps index by bundle path.
/// This avoids calling NSWorkspace.iconForFile on the main thread.
fn find_icon_in_index(
    index: &crate::app::tile::AppIndex,
    bundle_path: Option<&str>,
) -> Option<iced::widget::image::Handle> {
    let path = bundle_path?;
    // The index stores apps whose desc is the bundle path (for installed apps from discovery)
    // or whose open_command contains the path.
    for indexed_app in index.all() {
        if let AppCommand::Function(Function::OpenApp(ref app_path)) = indexed_app.open_command {
            if app_path == path {
                return indexed_app.icons.clone();
            }
        }
    }
    None
}

/// Calculate the scrollable content height for zero-query results.
///
/// Uses shared constants from `crate::app` so that the view and this
/// formula always agree on exact pixel values.
pub fn zero_query_scrollable_height(results: &[App]) -> f64 {
    use crate::app::apps::AppCategory;
    use crate::app::{ROW_HEIGHT, ZQ_COL_PADDING_V, ZQ_HEADER_HEIGHT};
    if results.is_empty() {
        return 0.0;
    }
    let mut header_count = 0u32;
    let mut last_cat: Option<&AppCategory> = None;
    for app in results {
        if let Some(ref cat) = app.category {
            match last_cat {
                None => header_count += 1,
                Some(prev) if prev != cat => header_count += 1,
                _ => {}
            }
            last_cat = Some(cat);
        }
    }
    let row_h = results.len() as f64 * ROW_HEIGHT;
    let header_h = header_count as f64 * ZQ_HEADER_HEIGHT;
    row_h + header_h + ZQ_COL_PADDING_V
}

/// Y-offset (inside the zero-query scroll content) of the focused result row.
/// Includes top padding and section headers ("RUNNING", "RECENT") inserted
/// before grouped rows.
fn zero_query_focus_offset(results: &[App], focus_index: usize) -> f64 {
    use crate::app::apps::AppCategory;
    use crate::app::{ROW_HEIGHT, ZQ_COL_PADDING_V, ZQ_HEADER_HEIGHT};

    if results.is_empty() {
        return 0.0;
    }

    let mut y = ZQ_COL_PADDING_V * 0.5; // top padding from Column::padding([2, 6])
    let mut last_category: Option<&AppCategory> = None;

    for (i, app) in results.iter().enumerate() {
        if let Some(ref cat) = app.category {
            let show_header = match last_category {
                None => true,
                Some(prev) => prev != cat,
            };
            if show_header {
                y += ZQ_HEADER_HEIGHT;
            }
            last_category = Some(cat);
        }

        if i == focus_index {
            return y;
        }

        y += ROW_HEIGHT;
    }

    y
}

/// Compute the total blur-window height from its parts.
///
/// `content_h` is the scrollable area height (0 when there are no results).
/// When content is present the layout is: search + banner + separator + content + footer.
/// When empty: search + banner only.
fn blur_height(banner_h: f64, content_h: f64) -> f64 {
    use crate::app::{FOOTER_HEIGHT, SEARCH_BAR_HEIGHT, SEPARATOR_HEIGHT};
    if content_h > 0.0 {
        SEARCH_BAR_HEIGHT + banner_h + SEPARATOR_HEIGHT + content_h + FOOTER_HEIGHT
    } else {
        SEARCH_BAR_HEIGHT + banner_h
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
    // Each banner: Row(28) + container padding([2,8]) = 28+2+2 = 32px
    // Column: padding([4,14]) = 4+4 = 8px vertical, spacing(4) between items
    let banner_row_h = 32.0; // Row(28) + container padding top(2) + bottom(2)
    let col_padding = 8.0; // Column padding([4,14]) → 4+4
    let col_spacing = 4.0; // Column spacing(4)
    col_padding + (count as f64 * banner_row_h) + ((count - 1) as f64 * col_spacing)
}
