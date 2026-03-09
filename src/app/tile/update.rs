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
    LauncherMode, Message, Page, agent_window_settings,
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

fn command_transfers_focus(command: &Function) -> bool {
    matches!(
        command,
        Function::OpenApp(_)
            | Function::ActivateApp(_)
            | Function::OpenTerminal
            | Function::OpenPrefPane
            | Function::GoogleSearch(_)
            | Function::ShowInFinder(_)
            | Function::OpenWebsite(_)
    )
}

fn suppress_frontmost_restore(tile: &mut Tile, reason: &str) {
    if let Some(app) = tile.frontmost.as_ref() {
        let pid = app.processIdentifier();
        let name = app
            .localizedName()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "<unknown>".to_string());
        coco_log!(
            "suppress_frontmost_restore: reason={} pid={} name={}",
            reason,
            pid,
            name
        );
    } else {
        coco_log!("suppress_frontmost_restore: reason={} none", reason);
    }
    tile.frontmost = None;
}

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
            close_clipboard_preview(tile);
            tile.capture_frontmost();
            focus_this_app();
            tile.focused = true;
            tile.visible = true;
            tile.hide_animating = false;
            tile.show_animating = true;
            tile.pending_window_height = None;
            tile.window_resize_token = tile.window_resize_token.wrapping_add(1);
            // Read real AX trusted state so permission UI matches paste behavior.
            tile.missing_accessibility = !platform::accessibility_permission_granted();
            tile.missing_input_monitoring = false;
            tile.missing_paste_permission = platform::paste_permission_warning_active();
            tile.permissions_ok = !(tile.missing_accessibility
                || tile.missing_input_monitoring
                || tile.missing_paste_permission);
            coco_log!(
                "OpenWindow permission state: missing_acc={} missing_input={} missing_paste={} permissions_ok={}",
                tile.missing_accessibility,
                tile.missing_input_monitoring,
                tile.missing_paste_permission,
                tile.permissions_ok
            );
            // Always refresh zero-query cache on open
            let zq = build_zero_query_results_inner();
            tile.zero_query_cache = zq;
            apply_cached_icons_to_apps(&mut tile.zero_query_cache, &tile.icon_cache);
            // Compute target height for content
            let banner_h = permission_banner_height(tile);
            let target_h = if tile.page == Page::ClipboardHistory && tile.clipboard_store.len() > 0
            {
                tile.clipboard_rebuild_filtered();
                blur_height(banner_h, clipboard_content_height(tile))
            } else if tile.page == Page::AgentList {
                tile.agent_refresh_sessions();
                blur_height(
                    banner_h,
                    agent_list_content_height(tile.agent_display_count()),
                )
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
                    let content_h = search_results_scrollable_height(&tile.results)
                        .min(crate::app::MAX_RESULTS_SCROLL_HEIGHT);
                    blur_height(banner_h, content_h)
                } else {
                    blur_height(banner_h, crate::app::MAIN_EMPTY_STATE_HEIGHT)
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

            Task::batch([
                tile.snap_resize(target_h),
                operation::focus("query"),
                Task::done(Message::PrimeVisibleAppIcons),
            ])
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

            // Favorites: cancel editing first
            if tile.editing_favorite_title.is_some() {
                return Task::done(Message::ClipboardFavoriteCancelEdit);
            }

            if tile.page == Page::ClipboardHistory && tile.clipboard_quick_preview_open {
                close_clipboard_preview(tile);
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
                        clipboard_content_height(tile)
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
                    return Task::batch([
                        resize,
                        Task::done(Message::ClearSearchQuery),
                        Task::done(Message::PrimeVisibleAppIcons),
                    ]);
                }
            }

            // ClipboardFavorites: ESC clears search first, then goes to ClipboardHistory
            if tile.page == Page::ClipboardFavorites {
                if !tile.query_lc.is_empty() {
                    tile.query.clear();
                    tile.query_lc.clear();
                    tile.focus_id = 0;
                    tile.favorite_rebuild_filtered();
                    let banner_h = permission_banner_height(tile);
                    let content_h = favorite_content_height(tile);
                    return tile.snap_resize(blur_height(banner_h, content_h));
                } else {
                    // Go back to clipboard history
                    tile.page = Page::ClipboardHistory;
                    tile.focus_id = 0;
                    tile.clipboard_rebuild_filtered();
                    let banner_h = permission_banner_height(tile);
                    let content_h = clipboard_content_height(tile);
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
                return Task::batch([
                    resize,
                    Task::done(Message::ClearSearchQuery),
                    Task::done(Message::PrimeVisibleAppIcons),
                ]);
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
                Task::batch([
                    tile.snap_resize(blur_height(banner_h, content_h)),
                    Task::done(Message::PrimeVisibleAppIcons),
                ])
            }
        }

        Message::ClearSearchQuery => {
            tile.query_lc = String::new();
            tile.query = String::new();
            close_clipboard_preview(tile);
            tile.last_query_edit_time = None;
            Task::none()
        }

        Message::ChangeFocus(key) => {
            // Redirect to actions overlay if open
            if tile.show_actions {
                return Task::done(Message::ActionFocusChanged(key));
            }
            tile.suppress_row_hover_focus = true;

            let len = match tile.page {
                Page::ClipboardHistory => tile.clipboard_display_count() as u32,
                Page::ClipboardFavorites => tile.favorite_display_count() as u32,
                Page::EmojiSearch => tile.results.len() as u32,
                Page::AgentList => tile.agent_display_count() as u32,
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
                Page::ClipboardHistory | Page::ClipboardFavorites => {
                    let row_h = 32.0_f32;
                    let viewport_h = if tile.page == Page::ClipboardHistory {
                        clipboard_content_height(tile) as f32
                    } else {
                        favorite_content_height(tile) as f32
                    };
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
                Page::AgentList | Page::WindowSwitcher => {
                    let row_h = 52.0_f32;
                    let total_h = len as f32 * row_h;
                    let viewport_h = total_h.min(364.0);
                    scroll_offset_with_mid_threshold_from_focus_y(
                        tile.focus_id as f32 * row_h,
                        len,
                        row_h,
                        viewport_h,
                        total_h,
                        wrapped_up,
                        wrapped_down,
                    )
                }
                Page::Main => {
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
                    } else if tile.page == Page::Main && !tile.query_lc.is_empty() {
                        let total_h = search_results_scrollable_height(&tile.results) as f32;
                        let viewport_h = total_h.min(crate::app::MAX_RESULTS_SCROLL_HEIGHT as f32);
                        (
                            search_results_focus_offset(&tile.results, tile.focus_id as usize)
                                as f32,
                            total_h,
                            viewport_h,
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

            if tile.page == Page::ClipboardHistory && tile.clipboard_quick_preview_open {
                if let Some(content) = focused_clipboard_content(tile) {
                    platform::update_clipboard_preview_panel(content);
                } else {
                    close_clipboard_preview(tile);
                }
            }

            Task::batch([
                task,
                operation::scroll_to(
                    "results",
                    AbsoluteOffset {
                        x: None,
                        y: Some(y),
                    },
                ),
                Task::done(Message::PrimeVisibleAppIcons),
            ])
        }

        Message::ResultPointerMoved(x) => {
            if tile.clipboard_quick_preview_open {
                // Native preview panel is open; suppress all hover changes.
                tile.suppress_row_hover_focus = true;
            } else if tile.page == Page::ClipboardHistory || tile.page == Page::ClipboardFavorites {
                let list_boundary_x = crate::app::WINDOW_WIDTH * 0.40;
                tile.suppress_row_hover_focus = x > list_boundary_x;
            } else {
                tile.suppress_row_hover_focus = false;
            }
            Task::done(Message::PrimeVisibleAppIcons)
        }

        Message::ResultPointerExited => {
            if !tile.clipboard_quick_preview_open {
                tile.suppress_row_hover_focus = false;
            }
            Task::none()
        }

        Message::HoverResult(id) => {
            if tile.suppress_row_hover_focus || tile.clipboard_quick_preview_open {
                return Task::none();
            }

            let len = match tile.page {
                Page::ClipboardHistory => tile.clipboard_display_count() as u32,
                Page::ClipboardFavorites => tile.favorite_display_count() as u32,
                Page::AgentList => tile.agent_display_count() as u32,
                Page::WindowSwitcher => tile.window_list.len() as u32,
                _ => tile.results.len() as u32,
            };

            if id >= len || tile.focus_id == id {
                return Task::none();
            }

            tile.focus_id = id;

            if tile.page == Page::ClipboardHistory && tile.clipboard_quick_preview_open {
                if let Some(content) = focused_clipboard_content(tile) {
                    platform::update_clipboard_preview_panel(content);
                } else {
                    close_clipboard_preview(tile);
                }
            }

            Task::none()
        }

        Message::ClipboardOpenAt(display_idx) => {
            if tile.page == Page::ClipboardFavorites {
                open_favorite_entry(tile, display_idx)
            } else {
                open_clipboard_entry(tile, display_idx)
            }
        }

        Message::ClipboardFinalizePaste => {
            coco_log!("ClipboardFinalizePaste: restore frontmost + paste");
            let target_pid = tile.restore_frontmost();
            platform::paste_to_frontmost(target_pid);
            Task::none()
        }

        Message::ApplyCalculatorInput(input) => {
            tile.page = Page::Main;
            tile.query = input;
            tile.query_lc = tile.query.to_lowercase();
            tile.focus_id = 0;
            tile.last_query_edit_time = if tile.query_lc.is_empty() {
                None
            } else {
                Some(std::time::Instant::now())
            };
            tile.suppress_row_hover_focus = true;
            close_clipboard_preview(tile);

            rebuild_results_for_current_query(tile);
            let has_results_now = !tile.results.is_empty();
            let banner_h = permission_banner_height(tile);
            let scrollable_h = if !tile.query_lc.is_empty() && !has_results_now {
                crate::app::MAIN_EMPTY_STATE_HEIGHT
            } else if has_results_now {
                search_results_scrollable_height(&tile.results)
                    .min(crate::app::MAX_RESULTS_SCROLL_HEIGHT)
            } else {
                0.0
            };
            let resize = tile.snap_resize(blur_height(banner_h, scrollable_h));
            Task::batch([resize, operation::focus("query")])
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
                return open_clipboard_entry(tile, tile.focus_id);
            }

            if tile.page == Page::ClipboardFavorites {
                // If editing title, commit the edit
                if tile.editing_favorite_title.is_some() {
                    return Task::done(Message::ClipboardFavoriteCommitEdit);
                }
                return open_favorite_entry(tile, tile.focus_id);
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
                    let filtered_idx = tile.focus_id as usize - 1;
                    if let Some(&session_idx) = tile.agent_display_indices().get(filtered_idx)
                        && let Some(session) = tile.agent_sessions.get(session_idx)
                    {
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

        Message::SpacePressed => {
            coco_log!(
                "SpacePressed: page={:?} show_actions={} q='{}' qlen={} focus_id={}",
                tile.page,
                tile.show_actions,
                tile.query,
                tile.query.chars().count(),
                tile.focus_id
            );
            if !tile.show_actions && tile.page == Page::ClipboardHistory {
                if tile.clipboard_quick_preview_open {
                    close_clipboard_preview(tile);
                    coco_log!("SpacePressed clipboard: hide native preview panel");
                    return Task::none();
                }

                if let Some(content) = focused_clipboard_content(tile).cloned() {
                    tile.clipboard_quick_preview_open = true;
                    platform::show_clipboard_preview_panel(&content);
                    coco_log!("SpacePressed clipboard: show native preview panel");
                } else {
                    close_clipboard_preview(tile);
                    coco_log!("SpacePressed clipboard: no focused entry");
                }
                return Task::none();
            }

            coco_log!("SpacePressed fallback -> FocusTextInput(space)");
            Task::done(Message::FocusTextInput(Move::Forwards(" ".to_string())))
        }

        Message::CycleLauncherMode { reverse } => {
            if !tile.visible || tile.show_actions {
                return Task::none();
            }

            // Shift+Tab in clipboard modes: toggle between History and Favorites
            if reverse && matches!(tile.page, Page::ClipboardHistory | Page::ClipboardFavorites) {
                let target = match tile.page {
                    Page::ClipboardHistory => Page::ClipboardFavorites,
                    Page::ClipboardFavorites => Page::ClipboardHistory,
                    _ => unreachable!(),
                };
                close_clipboard_preview(tile);
                cancel_favorite_editing(tile);
                tile.page = target;
                tile.focus_id = 0;
                return apply_current_query_for_active_mode(tile);
            }

            let Some(current) = launcher_mode_from_page(&tile.page) else {
                return Task::none();
            };
            let target = cycle_launcher_mode(current, reverse);
            Task::done(Message::SwitchLauncherMode(target))
        }

        Message::SwitchLauncherMode(mode) => {
            if !tile.visible {
                return Task::none();
            }
            let target_page = page_for_launcher_mode(mode);
            if tile.show_actions {
                tile.show_actions = false;
                tile.actions.clear();
            }
            close_clipboard_preview(tile);
            cancel_favorite_editing(tile);
            tile.page = target_page;
            tile.focus_id = 0;
            // Keep lowercase query in sync when switching modes so APP mode
            // immediately uses the current input for matching.
            tile.query_lc = tile.query.trim().to_lowercase();
            coco_log!(
                "SwitchLauncherMode -> page={:?} query={:?} query_lc={:?}",
                tile.page,
                tile.query,
                tile.query_lc
            );
            Task::batch([
                apply_current_query_for_active_mode(tile),
                operation::focus("query"),
            ])
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

            let mut new_options = get_installed_apps(false);
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
            close_clipboard_preview(tile);
            cancel_favorite_editing(tile);
            tile.page = page.clone();
            let banner_h = permission_banner_height(tile);
            let resize = match &tile.page {
                Page::ClipboardHistory => {
                    tile.clipboard_rebuild_filtered();
                    let content_h = clipboard_content_height(tile);
                    tile.snap_resize(blur_height(banner_h, content_h))
                }
                Page::ClipboardFavorites => {
                    tile.favorite_rebuild_filtered();
                    let content_h = favorite_content_height(tile);
                    tile.snap_resize(blur_height(banner_h, content_h))
                }
                Page::AgentList => {
                    tile.agent_sessions = crate::agent::session::list_sessions();
                    tile.agent_filtered = (0..tile.agent_sessions.len()).collect();
                    tile.snap_resize(blur_height(
                        banner_h,
                        agent_list_content_height(tile.agent_display_count()),
                    ))
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
            if let Function::Calculate(expr) = &command {
                return handle_calculator_enter(tile, expr);
            }

            let transfers_focus = command_transfers_focus(&command);
            if transfers_focus {
                suppress_frontmost_restore(tile, "run function transfers focus");
            }

            command.execute(&tile.config, &tile.query);

            let return_focus_task = if transfers_focus {
                Task::none()
            } else {
                Task::done(Message::ReturnFocus)
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
            close_clipboard_preview(tile);
            cancel_favorite_editing(tile);
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
            let should_paste = tile.pending_paste_after_hide;
            tile.pending_paste_after_hide = false;
            let target_pid = tile.restore_frontmost();
            if should_paste {
                platform::paste_to_frontmost(target_pid);
            }

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
            let _ = tile.restore_frontmost();
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

        Message::PrimeVisibleAppIcons => prime_visible_app_icons(tile),

        Message::AppIconsLoaded(icon_batch) => {
            for (bundle_path, icon) in icon_batch {
                tile.pending_icon_paths.remove(&bundle_path);
                if let Some(icon) = icon {
                    tile.icon_cache.insert(bundle_path, icon);
                }
            }

            apply_cached_icons_to_apps(&mut tile.results, &tile.icon_cache);
            apply_cached_icons_to_apps(&mut tile.zero_query_cache, &tile.icon_cache);
            Task::none()
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
            suppress_frontmost_restore(tile, "open agent session window");
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
            suppress_frontmost_restore(tile, "open new agent window");
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
            suppress_frontmost_restore(tile, "focus external window");
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
                    if tile.clipboard_quick_preview_open {
                        if let Some(content) = focused_clipboard_content(tile) {
                            platform::update_clipboard_preview_panel(content);
                        } else {
                            close_clipboard_preview(tile);
                        }
                    }
                }
            }
            Task::none()
        }

        Message::ClipboardDeleteFocused => {
            if tile.page != Page::ClipboardHistory && tile.page != Page::ClipboardFavorites {
                return Task::none();
            }

            // If on favorites page, redirect to favorite delete
            if tile.page == Page::ClipboardFavorites {
                return Task::done(Message::ClipboardFavoriteDeleteFocused);
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
                        close_clipboard_preview(tile);
                    } else if tile.focus_id as usize >= count {
                        tile.focus_id = (count - 1) as u32;
                    }
                    if count > 0 && tile.clipboard_quick_preview_open {
                        if let Some(content) = focused_clipboard_content(tile) {
                            platform::update_clipboard_preview_panel(content);
                        } else {
                            close_clipboard_preview(tile);
                        }
                    }
                    let banner_h = permission_banner_height(tile);
                    let content_h = if count > 0 {
                        clipboard_content_height(tile)
                    } else {
                        0.0
                    };
                    return tile.snap_resize(blur_height(banner_h, content_h));
                }
            }
            Task::none()
        }

        Message::ClipboardFavoriteAdd => {
            if tile.page != Page::ClipboardHistory {
                return Task::none();
            }
            let indices = tile.clipboard_display_indices();
            if let Some(&entry_idx) = indices.get(tile.focus_id as usize) {
                if let Some(entry) = tile.clipboard_store.get(entry_idx) {
                    let title = entry.preview_title.clone();
                    let content = entry.content.clone();
                    tile.favorite_store.add(content, title);
                    tile.favorite_rebuild_filtered();
                    perform_haptic(HapticPattern::Alignment);
                }
            }
            Task::none()
        }

        Message::ClipboardFavoriteDeleteFocused => {
            if tile.page != Page::ClipboardFavorites {
                return Task::none();
            }
            cancel_favorite_editing(tile);
            let indices = tile.favorite_display_indices();
            if let Some(&entry_idx) = indices.get(tile.focus_id as usize) {
                if let Some(entry) = tile.favorite_store.get(entry_idx) {
                    let id = entry.id;
                    tile.favorite_store.delete(id);
                    tile.favorite_rebuild_filtered();
                    let count = tile.favorite_display_count();
                    if count == 0 {
                        tile.focus_id = 0;
                    } else if tile.focus_id as usize >= count {
                        tile.focus_id = (count - 1) as u32;
                    }
                    let banner_h = permission_banner_height(tile);
                    let content_h = favorite_content_height(tile);
                    return tile.snap_resize(blur_height(banner_h, content_h));
                }
            }
            Task::none()
        }

        Message::ClipboardFavoriteStartEdit => {
            if tile.page != Page::ClipboardFavorites {
                return Task::none();
            }
            let indices = tile.favorite_display_indices();
            if let Some(&entry_idx) = indices.get(tile.focus_id as usize) {
                if let Some(entry) = tile.favorite_store.get(entry_idx) {
                    let fav_id = entry.id;
                    let saved_query = tile.query.clone();
                    tile.editing_favorite_title = Some((fav_id, saved_query));
                    tile.query = entry.title.clone();
                    tile.query_lc = entry.title.to_lowercase();
                }
            }
            operation::focus("query")
        }

        Message::ClipboardFavoriteCommitEdit => {
            if let Some((fav_id, saved_query)) = tile.editing_favorite_title.take() {
                let new_title = tile.query.trim().to_string();
                if !new_title.is_empty() {
                    tile.favorite_store.rename(fav_id, new_title);
                }
                tile.query = saved_query;
                tile.query_lc = tile.query.trim().to_lowercase();
                tile.favorite_rebuild_filtered();
            }
            Task::none()
        }

        Message::ClipboardFavoriteCancelEdit => {
            if let Some((_fav_id, saved_query)) = tile.editing_favorite_title.take() {
                tile.query = saved_query;
                tile.query_lc = tile.query.trim().to_lowercase();
                tile.favorite_rebuild_filtered();
            }
            Task::none()
        }

        Message::SearchQueryChanged(input, _id) => {
            // Swallow spurious character insertion from ⌘+key shortcuts.
            // iced's text_input may insert the raw character alongside the shortcut message.
            if crate::app::tile::CMD_SHORTCUT_SWALLOW
                .swap(false, std::sync::atomic::Ordering::Relaxed)
            {
                return Task::none();
            }

            // When editing a favorite title, just update query without search
            if tile.page == Page::ClipboardFavorites && tile.editing_favorite_title.is_some() {
                tile.query = input.clone();
                tile.query_lc = input.trim().to_lowercase();
                return Task::none();
            }

            // ClipboardFavorites: swallow space append (same as clipboard history)
            if tile.page == Page::ClipboardFavorites && input == format!("{} ", tile.query) {
                return Task::none();
            }

            // ClipboardHistory reserves Space for native preview panel toggle.
            // `text_input` may still emit an on_input update containing the
            // appended space, so swallow that single-space append here.
            if tile.page == Page::ClipboardHistory && input == format!("{} ", tile.query) {
                if !tile.show_actions {
                    if tile.clipboard_quick_preview_open {
                        close_clipboard_preview(tile);
                        coco_log!(
                            "SearchQueryChanged clipboard space append -> hide native preview"
                        );
                    } else if let Some(content) = focused_clipboard_content(tile).cloned() {
                        tile.clipboard_quick_preview_open = true;
                        platform::show_clipboard_preview_panel(&content);
                        coco_log!(
                            "SearchQueryChanged clipboard space append -> show native preview"
                        );
                    } else {
                        close_clipboard_preview(tile);
                        coco_log!("SearchQueryChanged clipboard space append -> no focused entry");
                    }
                }
                coco_log!(
                    "SearchQueryChanged swallow clipboard space append: prev={:?} next={:?}",
                    tile.query,
                    input
                );
                return Task::none();
            }

            if tile.page == Page::ClipboardHistory {
                close_clipboard_preview(tile);
                coco_log!(
                    "SearchQueryChanged clipboard apply: prev={:?} next={:?}",
                    tile.query,
                    input
                );
            }

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
                let content_h = clipboard_content_height(tile);
                return tile.snap_resize(blur_height(banner_h, content_h));
            }

            // ClipboardFavorites page: filter in-place
            if tile.page == Page::ClipboardFavorites {
                tile.focus_id = 0;
                tile.favorite_rebuild_filtered();
                let banner_h = permission_banner_height(tile);
                let content_h = favorite_content_height(tile);
                return tile.snap_resize(blur_height(banner_h, content_h));
            }

            // AgentList page: filter sessions in-place, keep query text
            if tile.page == Page::AgentList {
                tile.focus_id = 0;
                tile.agent_rebuild_filtered();
                let banner_h = permission_banner_height(tile);
                let content_h = agent_list_content_height(tile.agent_display_count());
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
                return Task::batch([resize, Task::done(Message::PrimeVisibleAppIcons)]);
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
            rebuild_results_for_current_query(tile);

            let has_results_now = !tile.results.is_empty();
            let banner_h = permission_banner_height(tile);

            // Resize the blur child window to match content — no wgpu flicker
            // since only the native child NSWindow is resized, not the main window.
            let scrollable_h =
                if tile.page == Page::ClipboardHistory && tile.clipboard_display_count() > 0 {
                    clipboard_content_height(tile)
                } else if tile.page == Page::Main && !tile.query_lc.is_empty() && !has_results_now {
                    crate::app::MAIN_EMPTY_STATE_HEIGHT
                } else if tile.page == Page::Main && has_results_now && !tile.query_lc.is_empty() {
                    search_results_scrollable_height(&tile.results)
                        .min(crate::app::MAX_RESULTS_SCROLL_HEIGHT)
                } else if has_results_now {
                    let rows = std::cmp::min(tile.results.len(), 7) as f64;
                    rows * ROW_HEIGHT
                } else {
                    0.0
                };
            let resize = tile.snap_resize(blur_height(banner_h, scrollable_h));

            if has_results_now {
                Task::batch([
                    resize,
                    Task::done(Message::ChangeFocus(ArrowKey::Left)),
                    Task::done(Message::PrimeVisibleAppIcons),
                ])
            } else {
                Task::batch([resize, Task::done(Message::PrimeVisibleAppIcons)])
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
/// Icons are loaded lazily after the visible rows are known, so this stays
/// metadata-only and cheap during startup/open-window work.
fn build_zero_query_results_inner() -> Vec<App> {
    use crate::app::apps::AppCategory;
    use crate::history::{History, format_relative_time};

    let mut results = Vec::new();

    let running = platform::get_running_apps(false);
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
        let time_str = format!("Last: {}", format_relative_time(&entry.last_used));
        results.push(App {
            open_command: AppCommand::Function(Function::OpenApp(entry.bundle_path.clone())),
            desc: time_str,
            icons: None,
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

fn prime_visible_app_icons(tile: &mut Tile) -> Task<Message> {
    if !tile.config.theme.show_icons {
        return Task::none();
    }

    apply_cached_icons_to_apps(&mut tile.results, &tile.icon_cache);
    apply_cached_icons_to_apps(&mut tile.zero_query_cache, &tile.icon_cache);

    let mut requested_paths = Vec::new();
    for bundle_path in visible_icon_paths(&tile.results, tile.focus_id as usize) {
        if tile.icon_cache.contains_key(&bundle_path)
            || !tile.pending_icon_paths.insert(bundle_path.clone())
        {
            continue;
        }

        requested_paths.push(bundle_path);
    }

    if requested_paths.is_empty() {
        return Task::none();
    }

    let fallback_paths = requested_paths.clone();
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                requested_paths
                    .into_iter()
                    .map(|bundle_path| {
                        let icon = crate::utils::icon_from_app_bundle(Path::new(&bundle_path));
                        (bundle_path, icon)
                    })
                    .collect::<Vec<_>>()
            })
            .await
            .unwrap_or_else(|_| {
                fallback_paths
                    .into_iter()
                    .map(|bundle_path| (bundle_path, None))
                    .collect()
            })
        },
        Message::AppIconsLoaded,
    )
}

fn apply_cached_icons_to_apps(
    apps: &mut [App],
    icon_cache: &std::collections::HashMap<String, iced::widget::image::Handle>,
) {
    for app in apps {
        if app.icons.is_some() {
            continue;
        }
        let Some(bundle_path) = icon_key_for_app(app) else {
            continue;
        };
        if let Some(icon) = icon_cache.get(bundle_path) {
            app.icons = Some(icon.clone());
        }
    }
}

fn visible_icon_paths(results: &[App], focus_id: usize) -> Vec<String> {
    const ICON_PREFETCH_WINDOW: usize = crate::app::MAX_VISIBLE_ROWS + 4;

    let mut seen = std::collections::HashSet::new();
    let start = focus_id.saturating_sub(ICON_PREFETCH_WINDOW / 2);
    results
        .iter()
        .skip(start)
        .take(ICON_PREFETCH_WINDOW)
        .filter_map(icon_key_for_app)
        .cloned()
        .filter(|bundle_path| seen.insert(bundle_path.clone()))
        .collect()
}

fn icon_key_for_app(app: &App) -> Option<&String> {
    app.bundle_path.as_ref()
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

/// Calculate the scrollable content height for the search-results list
/// when the "APPS" section header is shown.
pub fn search_results_scrollable_height(results: &[App]) -> f64 {
    use crate::app::{
        MAIN_SEARCH_RESULTS_BOTTOM_SPACING, ROW_HEIGHT, ZQ_COL_PADDING_V, ZQ_HEADER_HEIGHT,
    };
    if results.is_empty() {
        return 0.0;
    }
    let header_h = if search_results_has_header(results) {
        ZQ_HEADER_HEIGHT
    } else {
        0.0
    };
    ZQ_COL_PADDING_V
        + header_h
        + (results.len() as f64 * ROW_HEIGHT)
        + MAIN_SEARCH_RESULTS_BOTTOM_SPACING
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

/// Y-offset (inside search results scroll content) of the focused row when
/// the "APPS" header is rendered above the results.
fn search_results_focus_offset(results: &[App], focus_index: usize) -> f64 {
    use crate::app::{ROW_HEIGHT, ZQ_COL_PADDING_V, ZQ_HEADER_HEIGHT};
    let header_h = if search_results_has_header(results) {
        ZQ_HEADER_HEIGHT
    } else {
        0.0
    };
    ZQ_COL_PADDING_V * 0.5 + header_h + (focus_index as f64 * ROW_HEIGHT)
}

fn search_results_has_header(results: &[App]) -> bool {
    !results.is_empty()
        && !results.iter().all(|app| {
            app.name_lc.starts_with("__calc__|") || app.name_lc.starts_with("__calc_history__|")
        })
}

fn launcher_mode_from_page(page: &Page) -> Option<LauncherMode> {
    match page {
        Page::Main => Some(LauncherMode::App),
        Page::ClipboardHistory | Page::ClipboardFavorites => Some(LauncherMode::Clipboard),
        Page::AgentList => None,
        _ => None,
    }
}

fn page_for_launcher_mode(mode: LauncherMode) -> Page {
    match mode {
        LauncherMode::App => Page::Main,
        LauncherMode::Clipboard => Page::ClipboardHistory,
    }
}

fn cycle_launcher_mode(current: LauncherMode, reverse: bool) -> LauncherMode {
    use LauncherMode::{App, Clipboard};
    if reverse {
        match current {
            App => Clipboard,
            Clipboard => App,
        }
    } else {
        match current {
            App => Clipboard,
            Clipboard => App,
        }
    }
}

fn agent_list_content_height(display_count: usize) -> f64 {
    let rows = std::cmp::min(display_count, 7) as f64;
    rows * 52.0
}

fn mode_switch_snap_resize(tile: &mut Tile, target_h: f64) -> Task<Message> {
    // Tab/click mode switching often only changes height by a few pixels
    // (e.g. APP/Clipboard/Agent). Shrinking the native window every switch
    // causes visible stutter. Keep current visual height unless the new mode
    // needs more space.
    if tile.target_blur_height > 0.0 && target_h + 1.0 < tile.target_blur_height {
        coco_log!(
            "mode-switch resize skipped shrink target={:.1} current_blur={:.1} page={:?}",
            target_h,
            tile.target_blur_height,
            tile.page
        );
        return Task::none();
    }

    tile.snap_resize(target_h)
}

fn focused_clipboard_content(tile: &Tile) -> Option<&ClipBoardContentType> {
    let entry_idx = *tile
        .clipboard_display_indices()
        .get(tile.focus_id as usize)?;
    tile.clipboard_store
        .get(entry_idx)
        .map(|entry| &entry.content)
}

fn open_clipboard_entry(tile: &mut Tile, display_idx: u32) -> Task<Message> {
    let indices = tile.clipboard_display_indices();
    coco_log!(
        "open_clipboard_entry try: display_idx={} display_len={} store_len={} focus_id={}",
        display_idx,
        indices.len(),
        tile.clipboard_store.len(),
        tile.focus_id
    );
    if let Some(&entry_idx) = indices.get(display_idx as usize)
        && let Some(entry) = tile.clipboard_store.get(entry_idx)
    {
        tile.focus_id = display_idx;
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
        close_clipboard_preview(tile);
        tile.pending_paste_after_hide = false;
        tile.visible = false;
        tile.focused = false;
        tile.show_animating = false;
        tile.hide_animating = false;
        tile.page = Page::Main;
        coco_log!(
            "open_clipboard_entry: display_idx={} copied, scheduling hide+paste",
            display_idx
        );

        let hide_task = if let Some(wid) = tile.main_window_id {
            window::set_mode::<Message>(wid, window::Mode::Hidden)
        } else {
            Task::none()
        };
        let paste_task = Task::perform(
            async {
                // Give the window hide state one short frame to settle, then paste quickly.
                tokio::time::sleep(std::time::Duration::from_millis(35)).await;
            },
            |_| Message::ClipboardFinalizePaste,
        );

        return if tile.config.buffer_rules.clear_on_hide {
            Task::batch([
                hide_task,
                paste_task,
                Task::done(Message::ClearSearchQuery),
                Task::done(Message::ClearSearchResults),
            ])
        } else {
            Task::batch([hide_task, paste_task])
        };
    }

    coco_log!(
        "open_clipboard_entry miss: display_idx={} display_len={}",
        display_idx,
        indices.len()
    );
    Task::none()
}

fn handle_calculator_enter(tile: &mut Tile, expr: &Expr) -> Task<Message> {
    let Some(value) = expr.eval() else {
        return Task::none();
    };

    let expression = format_calculator_expression(&tile.query);
    let result_text = format_calculator_value(value);
    let history_line = format!("{expression} = {result_text}");
    tile.calculator_history
        .retain(|h| !(h.expression == expression && h.result == result_text));
    tile.calculator_history.insert(
        0,
        crate::app::tile::CalculatorHistoryEntry {
            expression: expression.clone(),
            result: result_text.clone(),
        },
    );
    tile.calculator_history.truncate(60);

    // Record formula + result in clipboard history for later lookup.
    tile.clipboard_store
        .push(ClipBoardContentType::Text(history_line));

    // Keep calculator in-place for chained calculations.
    tile.page = Page::Main;
    tile.query = result_text.clone();
    tile.query_lc = result_text.to_lowercase();
    tile.focus_id = 0;
    tile.last_query_edit_time = Some(std::time::Instant::now());
    tile.suppress_row_hover_focus = true;
    close_clipboard_preview(tile);

    rebuild_results_for_current_query(tile);

    let has_results_now = !tile.results.is_empty();
    let banner_h = permission_banner_height(tile);
    let scrollable_h = if !tile.query_lc.is_empty() && !has_results_now {
        crate::app::MAIN_EMPTY_STATE_HEIGHT
    } else if has_results_now {
        search_results_scrollable_height(&tile.results).min(crate::app::MAX_RESULTS_SCROLL_HEIGHT)
    } else {
        0.0
    };

    let resize = tile.snap_resize(blur_height(banner_h, scrollable_h));
    Task::batch([
        resize,
        operation::focus("query"),
        Task::done(Message::PrimeVisibleAppIcons),
    ])
}

fn close_clipboard_preview(tile: &mut Tile) {
    tile.clipboard_quick_preview_open = false;
    platform::hide_clipboard_preview_panel();
}

#[cfg(test)]
mod tests {
    use super::{command_transfers_focus, handle_update, search_results_scrollable_height};
    use crate::agent::types::AgentStatus;
    use crate::app::apps::{App, AppCommand};
    use crate::app::tile::Tile;
    use crate::app::{MAIN_SEARCH_RESULTS_BOTTOM_SPACING, Message, Page};
    use crate::clipboard_store::ClipboardStore;
    use crate::commands::Function;
    use crate::config::Config;
    use crate::favorite_store::FavoriteStore;
    use crate::search::AppIndex;
    use global_hotkey::hotkey::HotKey;
    use iced::widget::image::Handle;
    use nucleo_matcher::{Config as MatcherConfig, Matcher};
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};

    fn make_app(name: &str) -> App {
        App {
            name: name.to_string(),
            name_lc: name.to_lowercase(),
            localized_name: None,
            desc: String::new(),
            icons: None,
            open_command: AppCommand::Display,
            category: None,
            bundle_path: None,
            bundle_id: None,
            pid: None,
        }
    }

    fn make_bundled_app(name: &str, bundle_path: &str) -> App {
        let mut app = make_app(name);
        app.bundle_path = Some(bundle_path.to_string());
        app
    }

    fn dummy_icon() -> Handle {
        Handle::from_rgba(1, 1, vec![255, 255, 255, 255])
    }

    fn make_tile(results: Vec<App>, query: &str) -> Tile {
        let config = Config::default();
        let hotkey: HotKey = config
            .toggle_hotkey
            .parse()
            .expect("default hotkey should parse");

        Tile {
            theme: config.theme.clone().into(),
            focus_id: 1,
            query: query.to_string(),
            query_lc: query.to_lowercase(),
            results: results.clone(),
            options: AppIndex::from_apps(Vec::new()),
            emoji_apps: AppIndex::from_apps(Vec::new()),
            visible: true,
            focused: true,
            frontmost: None,
            config,
            hotkey,
            clipboard_hotkey: None,
            clipboard_store: ClipboardStore::load(),
            clipboard_filtered: Vec::new(),
            clipboard_quick_preview_open: false,
            tray_icon: None,
            sender: None,
            page: Page::Main,
            fuzzy_matcher: RefCell::new(Matcher::new(MatcherConfig::DEFAULT)),
            agent_sessions: Vec::new(),
            agent_filtered: Vec::new(),
            agent_window_id: None,
            agent_session_id: None,
            agent_messages: Vec::new(),
            agent_input: String::new(),
            agent_status: AgentStatus::Idle,
            agent_markdown: iced::widget::markdown::Content::new(),
            permissions_ok: true,
            missing_accessibility: false,
            missing_input_monitoring: false,
            missing_paste_permission: false,
            zero_query_cache: results,
            icon_cache: HashMap::new(),
            pending_icon_paths: HashSet::new(),
            show_actions: false,
            actions: Vec::new(),
            action_focus_id: 0,
            action_target_name: String::new(),
            window_list: Vec::new(),
            main_window_id: None,
            target_blur_height: 312.0,
            target_window_height: 356.0,
            pending_window_height: Some(344.0),
            window_resize_token: 17,
            show_animating: false,
            hide_animating: false,
            last_hotkey_time: None,
            last_query_edit_time: None,
            suppress_row_hover_focus: false,
            pending_paste_after_hide: false,
            calculator_history: Vec::new(),
            favorite_store: FavoriteStore::load(),
            favorite_filtered: Vec::new(),
            editing_favorite_title: None,
        }
    }

    #[test]
    fn focus_transferring_commands_are_marked() {
        assert!(command_transfers_focus(&Function::OpenApp(
            "/Applications/Safari.app".into()
        )));
        assert!(command_transfers_focus(&Function::ActivateApp(42)));
        assert!(command_transfers_focus(&Function::OpenTerminal));
        assert!(command_transfers_focus(&Function::OpenPrefPane));
        assert!(command_transfers_focus(&Function::GoogleSearch(
            "coco".into()
        )));
        assert!(command_transfers_focus(&Function::ShowInFinder(
            "/tmp".into()
        )));
        assert!(command_transfers_focus(&Function::OpenWebsite(
            "example.com".into()
        )));
    }

    #[test]
    fn in_place_commands_keep_frontmost_restore() {
        assert!(!command_transfers_focus(&Function::CopyPath("/tmp".into())));
        assert!(!command_transfers_focus(&Function::CopyBundleId(
            "com.example.app".into()
        )));
        assert!(!command_transfers_focus(&Function::HideApp(7)));
        assert!(!command_transfers_focus(&Function::QuitApp(7)));
        assert!(!command_transfers_focus(&Function::ForceQuitApp(7)));
        assert!(!command_transfers_focus(&Function::CopyToClipboard(
            crate::clipboard::ClipBoardContentType::Text("hi".into())
        )));
    }

    #[test]
    fn search_results_height_includes_bottom_spacing() {
        use crate::app::{ROW_HEIGHT, ZQ_COL_PADDING_V, ZQ_HEADER_HEIGHT};

        let results = vec![make_app("Codex"), make_app("Xcode")];
        let expected = ZQ_COL_PADDING_V
            + ZQ_HEADER_HEIGHT
            + (results.len() as f64 * ROW_HEIGHT)
            + MAIN_SEARCH_RESULTS_BOTTOM_SPACING;

        assert_eq!(search_results_scrollable_height(&results), expected);
    }

    #[test]
    fn app_icon_batch_keeps_search_state_stable() {
        let results = vec![
            make_bundled_app("Safari", "/Applications/Safari.app"),
            make_bundled_app("Notes", "/Applications/Notes.app"),
            make_bundled_app("Calendar", "/Applications/Calendar.app"),
        ];
        let mut tile = make_tile(results, "sa");

        let before_names = tile
            .results
            .iter()
            .map(|app| app.name.clone())
            .collect::<Vec<_>>();
        let before_paths = tile
            .results
            .iter()
            .map(|app| app.bundle_path.clone())
            .collect::<Vec<_>>();
        let before_height = search_results_scrollable_height(&tile.results);
        let before_focus = tile.focus_id;
        let before_blur_height = tile.target_blur_height;
        let before_window_height = tile.target_window_height;
        let before_pending = tile.pending_window_height;
        let before_resize_token = tile.window_resize_token;

        let _ = handle_update(
            &mut tile,
            Message::AppIconsLoaded(vec![
                ("/Applications/Safari.app".to_string(), Some(dummy_icon())),
                ("/Applications/Notes.app".to_string(), Some(dummy_icon())),
            ]),
        );

        assert_eq!(
            tile.results
                .iter()
                .map(|app| app.name.clone())
                .collect::<Vec<_>>(),
            before_names
        );
        assert_eq!(
            tile.results
                .iter()
                .map(|app| app.bundle_path.clone())
                .collect::<Vec<_>>(),
            before_paths
        );
        assert_eq!(
            search_results_scrollable_height(&tile.results),
            before_height
        );
        assert_eq!(tile.focus_id, before_focus);
        assert_eq!(tile.target_blur_height, before_blur_height);
        assert_eq!(tile.target_window_height, before_window_height);
        assert_eq!(tile.pending_window_height, before_pending);
        assert_eq!(tile.window_resize_token, before_resize_token);

        assert!(tile.icon_cache.contains_key("/Applications/Safari.app"));
        assert!(tile.icon_cache.contains_key("/Applications/Notes.app"));
        assert!(tile.results[0].icons.is_some());
        assert!(tile.results[1].icons.is_some());
        assert!(tile.results[2].icons.is_none());
        assert!(tile.zero_query_cache[0].icons.is_some());
        assert!(tile.zero_query_cache[1].icons.is_some());
        assert!(tile.zero_query_cache[2].icons.is_none());
    }
}

/// Cancel any in-progress favorite title edit, restoring the saved query.
fn cancel_favorite_editing(tile: &mut Tile) {
    if let Some((_fav_id, saved_query)) = tile.editing_favorite_title.take() {
        tile.query = saved_query;
        tile.query_lc = tile.query.trim().to_lowercase();
    }
}

fn clipboard_content_height(tile: &Tile) -> f64 {
    if tile.clipboard_display_count() == 0 {
        0.0
    } else {
        crate::app::CLIPBOARD_CONTENT_HEIGHT
    }
}

fn favorite_content_height(tile: &Tile) -> f64 {
    if tile.favorite_display_count() == 0 {
        0.0
    } else {
        crate::app::CLIPBOARD_CONTENT_HEIGHT
    }
}

fn open_favorite_entry(tile: &mut Tile, display_idx: u32) -> Task<Message> {
    cancel_favorite_editing(tile);
    let indices = tile.favorite_display_indices();
    if let Some(&entry_idx) = indices.get(display_idx as usize)
        && let Some(entry) = tile.favorite_store.get(entry_idx)
    {
        tile.focus_id = display_idx;
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
        close_clipboard_preview(tile);
        tile.pending_paste_after_hide = false;
        tile.visible = false;
        tile.focused = false;
        tile.show_animating = false;
        tile.hide_animating = false;
        tile.page = Page::Main;

        let hide_task = if let Some(wid) = tile.main_window_id {
            window::set_mode::<Message>(wid, window::Mode::Hidden)
        } else {
            Task::none()
        };
        let paste_task = Task::perform(
            async {
                tokio::time::sleep(std::time::Duration::from_millis(35)).await;
            },
            |_| Message::ClipboardFinalizePaste,
        );

        return if tile.config.buffer_rules.clear_on_hide {
            Task::batch([
                hide_task,
                paste_task,
                Task::done(Message::ClearSearchQuery),
                Task::done(Message::ClearSearchResults),
            ])
        } else {
            Task::batch([hide_task, paste_task])
        };
    }

    Task::none()
}

fn apply_current_query_for_active_mode(tile: &mut Tile) -> Task<Message> {
    let banner_h = permission_banner_height(tile);

    match tile.page {
        Page::ClipboardHistory => {
            tile.focus_id = 0;
            tile.clipboard_rebuild_filtered();
            let content_h = clipboard_content_height(tile);
            mode_switch_snap_resize(tile, blur_height(banner_h, content_h))
        }
        Page::ClipboardFavorites => {
            tile.focus_id = 0;
            tile.favorite_rebuild_filtered();
            let content_h = favorite_content_height(tile);
            mode_switch_snap_resize(tile, blur_height(banner_h, content_h))
        }
        Page::AgentList => {
            tile.focus_id = 0;
            if tile.agent_sessions.is_empty() {
                tile.agent_refresh_sessions();
            } else {
                tile.agent_rebuild_filtered();
            }
            let content_h = agent_list_content_height(tile.agent_display_count());
            mode_switch_snap_resize(tile, blur_height(banner_h, content_h))
        }
        Page::Main => {
            tile.focus_id = 0;
            if tile.query_lc.is_empty() {
                tile.results = tile.zero_query_cache.clone();
                let content_h = if tile.results.is_empty() {
                    0.0
                } else {
                    zero_query_scrollable_height(&tile.results)
                        .min(crate::app::MAX_RESULTS_SCROLL_HEIGHT)
                };
                Task::batch([
                    mode_switch_snap_resize(tile, blur_height(banner_h, content_h)),
                    Task::done(Message::PrimeVisibleAppIcons),
                ])
            } else {
                coco_log!(
                    "apply_current_query_for_active_mode main(before): query={:?} query_lc={:?} results={}",
                    tile.query,
                    tile.query_lc,
                    tile.results.len()
                );
                rebuild_results_for_current_query(tile);
                coco_log!(
                    "apply_current_query_for_active_mode main(after): query={:?} query_lc={:?} results={}",
                    tile.query,
                    tile.query_lc,
                    tile.results.len()
                );
                let content_h = if tile.results.is_empty() {
                    crate::app::MAIN_EMPTY_STATE_HEIGHT
                } else {
                    search_results_scrollable_height(&tile.results)
                        .min(crate::app::MAX_RESULTS_SCROLL_HEIGHT)
                };
                Task::batch([
                    mode_switch_snap_resize(tile, blur_height(banner_h, content_h)),
                    Task::done(Message::PrimeVisibleAppIcons),
                ])
            }
        }
        _ => Task::none(),
    }
}

fn rebuild_results_for_current_query(tile: &mut Tile) {
    coco_log!(
        "rebuild_results(start): page={:?} query={:?} query_lc={:?}",
        tile.page,
        tile.query,
        tile.query_lc
    );
    let calc_expr = Expr::from_str(&tile.query).ok();
    let prefer_calculator =
        calc_expr.is_some() && query_prefers_calculator(&tile.query, tile.page == Page::Main);

    if prefer_calculator {
        tile.results.clear();
        coco_log!("rebuild_results(calc-preferred): skip app search");
    } else {
        tile.handle_search_query_changed();
        apply_cached_icons_to_apps(&mut tile.results, &tile.icon_cache);
        coco_log!(
            "rebuild_results(after app search): results={}",
            tile.results.len()
        );
    }

    if (prefer_calculator || tile.results.is_empty())
        && let Some(res) = calc_expr
        && let Some(value) = res.eval()
    {
        let expression = format_calculator_expression(&tile.query);
        let value_text = format_calculator_value(value);
        let current_line = format!("{expression} = {value_text}");
        tile.results.push(App {
            open_command: AppCommand::Function(Function::Calculate(res.clone())),
            desc: COCO_DESC_NAME.to_string(),
            icons: None,
            name: current_line.clone(),
            name_lc: format!("__calc__|{}", expression.to_lowercase()),
            localized_name: None,
            category: None,
            bundle_path: None,
            bundle_id: None,
            pid: None,
        });

        if prefer_calculator {
            for h in tile.calculator_history.iter() {
                let line = format!("{} = {}", h.expression, h.result);
                if line == current_line {
                    continue;
                }
                tile.results.push(App {
                    open_command: AppCommand::Message(Message::ApplyCalculatorInput(
                        h.result.clone(),
                    )),
                    desc: "History".to_string(),
                    icons: None,
                    name: line,
                    name_lc: format!("__calc_history__|{}", h.expression.to_lowercase()),
                    localized_name: None,
                    category: None,
                    bundle_path: None,
                    bundle_id: None,
                    pid: None,
                });
                if tile.results.len() >= 9 {
                    break;
                }
            }
        }
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
    } else if let Some(conversions) = currency_conversion::convert_query(&tile.query) {
        coco_log!(
            "rebuild_results(currency): query={:?} conversions={}",
            tile.query,
            conversions.len()
        );
        tile.results = conversions
            .into_iter()
            .map(|c| {
                let formatted_target =
                    currency_conversion::format_currency(c.target_value, c.target_code);
                let formatted_source =
                    currency_conversion::format_currency(c.source_value, c.source_code);
                let source_name = currency_conversion::currency_name_cn(c.source_code);
                let target_name = currency_conversion::currency_name_cn(c.target_code);
                let source_symbol = currency_conversion::currency_symbol(c.source_code);
                let target_symbol = currency_conversion::currency_symbol(c.target_code);
                let source_flag = currency_conversion::currency_flag(c.source_code);
                let target_flag = currency_conversion::currency_flag(c.target_code);

                let source_display = if source_symbol.is_empty() {
                    format!("{formatted_source} {}", c.source_code)
                } else {
                    format!("{source_symbol}{formatted_source} {}", c.source_code)
                };
                let target_display = if target_symbol.is_empty() {
                    format!("{formatted_target} {}", c.target_code)
                } else {
                    format!("{target_symbol}{formatted_target} {}", c.target_code)
                };
                let source_head = format!("{source_flag} {source_display}");
                let target_head = format!("{target_flag} {target_display}");

                let copy_text = target_display.clone();
                App {
                    open_command: AppCommand::Function(Function::CopyToClipboard(
                        ClipBoardContentType::Text(copy_text),
                    )),
                    desc: format!(
                        "{} ({}) → {} ({}) · 1 {} = {:.4} {}",
                        source_head,
                        source_name,
                        target_head,
                        target_name,
                        c.source_code,
                        c.rate,
                        c.target_code,
                    ),
                    icons: None,
                    name: target_head,
                    name_lc: format!("__currency__|{}|{}", c.source_code, c.target_code),
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
    coco_log!("rebuild_results(done): results={}", tile.results.len());
}

fn format_calculator_expression(query: &str) -> String {
    let mut out = String::with_capacity(query.len() + 8);
    for ch in query.trim().chars() {
        match ch {
            '+' | '-' | '/' | '^' => {
                out.push(' ');
                out.push(ch);
                out.push(' ');
            }
            '*' | 'x' | 'X' | '×' => {
                out.push(' ');
                out.push('x');
                out.push(' ');
            }
            _ => out.push(ch),
        }
    }

    let compact = out.split_whitespace().collect::<Vec<_>>().join(" ");
    compact.replace("( ", "(").replace(" )", ")")
}

fn format_calculator_value(value: f64) -> String {
    let normalized = if value.abs() < 1e-12 { 0.0 } else { value };
    if (normalized - normalized.round()).abs() < 1e-10 {
        return format!("{}", normalized.round() as i64);
    }

    let mut s = format!("{normalized:.10}");
    if let Some(dot_pos) = s.find('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') && dot_pos == s.len() - 1 {
            s.pop();
        }
    }
    s
}

fn query_prefers_calculator(query: &str, on_main_page: bool) -> bool {
    if !on_main_page {
        return false;
    }

    let q = query.trim();
    if q.is_empty() || !q.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }

    if q.to_ascii_lowercase().contains("http") {
        return false;
    }

    q.chars().all(|c| {
        c.is_ascii_digit()
            || c.is_whitespace()
            || matches!(
                c,
                '.' | ',' | '+' | '-' | '*' | '/' | '^' | '(' | ')' | 'x' | 'X' | '×'
            )
            || matches!(c, 'l' | 'L' | 'o' | 'O' | 'g' | 'G' | 'n' | 'N' | 'e' | 'E')
    })
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
    let count = tile.missing_accessibility as u32
        + tile.missing_input_monitoring as u32
        + tile.missing_paste_permission as u32;
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
