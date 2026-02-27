//! This module handles the logic for the new and view functions according to the elm
//! architecture. If the subscription function becomes too large, it should be moved to this file

use std::cell::RefCell;

use global_hotkey::hotkey::HotKey;
use iced::font::Weight;
use iced::widget::scrollable::{Anchor, Direction, Scrollbar};
use iced::widget::text::LineHeight;
use iced::widget::{Button, Column, Row, Scrollable, Text, container, markdown, mouse_area, space};
use iced::{Alignment, window};
use iced::{Element, Task};
use iced::{Length::Fill, widget::text_input};

use nucleo_matcher::{Config as MatcherConfig, Matcher};
use rayon::slice::ParallelSliceMut;

use crate::agent::types::{AgentStatus, ChatMessage};
use crate::app::pages::emoji::emoji_page;
use crate::app::tile::AppIndex;
use crate::config::Theme;
use crate::platform;
use crate::styles::{
    agent_content_style, agent_input_bar_style, agent_title_bar_style, coco_text_input_style,
    contents_style, footer_shortcut_badge_style, footer_style, mode_switch_button_style,
    mode_switch_container_style, permission_banner_button_style, permission_banner_style,
    separator_style, user_bubble_style,
};
use crate::{app::pages::clipboard::clipboard_view, platform::get_installed_apps};
use crate::{
    app::{LauncherMode, Message, Page, apps::App, default_settings, tile::Tile},
    config::Config,
    platform::transform_process_to_ui_element,
};

/// Initialise the base window
pub fn new(hotkey: HotKey, config: &Config) -> (Tile, Task<Message>) {
    // Clear debug log on each app launch
    let _ = std::fs::write("/Users/kcsx/coco_debug.log", "");

    let (id, open) = window::open(default_settings());

    let open = open.discard().chain(window::run(id, |handle| {
        let wh = handle.window_handle().expect("Unable to get window handle");
        platform::window_config(&wh);
        platform::store_main_window(&wh);
        platform::create_blur_child_window(
            &wh,
            crate::app::WINDOW_WIDTH as f64,
            crate::app::SEARCH_BAR_HEIGHT,
        );
        transform_process_to_ui_element();
    }));

    crate::currency_conversion::spawn_rate_updater();

    let store_icons = config.theme.show_icons;

    let mut options = get_installed_apps(store_icons);

    options.extend(config.shells.iter().map(|x| x.to_app()));
    options.extend(App::basic_apps());
    options.par_sort_by_key(|x| x.name.len());
    let options = AppIndex::from_apps(options);
    let clipboard_store = crate::clipboard_store::ClipboardStore::load();
    let clipboard_filtered = (0..clipboard_store.len()).collect();
    let agent_sessions = crate::agent::session::list_sessions();
    let agent_filtered = (0..agent_sessions.len()).collect();

    (
        Tile {
            query: String::new(),
            query_lc: String::new(),
            focus_id: 0,
            results: vec![],
            options,
            emoji_apps: AppIndex::from_apps(App::emoji_apps()),
            hotkey,
            visible: true,
            clipboard_hotkey: config
                .clipboard_hotkey
                .clone()
                .and_then(|x| x.parse::<HotKey>().ok()),
            frontmost: None,
            focused: false,
            config: config.clone(),
            theme: config.theme.to_owned().into(),
            clipboard_store,
            clipboard_filtered,
            clipboard_quick_preview_open: false,
            tray_icon: None,
            sender: None,
            page: Page::Main,
            fuzzy_matcher: RefCell::new(Matcher::new(MatcherConfig::DEFAULT)),
            agent_sessions,
            agent_filtered,
            agent_window_id: None,
            agent_session_id: None,
            agent_messages: Vec::new(),
            agent_input: String::new(),
            agent_status: AgentStatus::Idle,
            agent_markdown: iced::widget::markdown::Content::new(),
            missing_accessibility: true, // will be checked properly on OpenWindow
            missing_input_monitoring: true,
            missing_paste_permission: false,
            permissions_ok: false,
            zero_query_cache: Vec::new(),
            show_actions: false,
            actions: Vec::new(),
            action_focus_id: 0,
            action_target_name: String::new(),
            window_list: Vec::new(),
            main_window_id: Some(id),
            target_blur_height: crate::app::SEARCH_BAR_HEIGHT,
            target_window_height: crate::app::SEARCH_BAR_HEIGHT,
            pending_window_height: None,
            window_resize_token: 0,
            show_animating: false,
            hide_animating: false,
            last_hotkey_time: None,
            last_query_edit_time: None,
            suppress_row_hover_focus: false,
            pending_paste_after_hide: false,
            calculator_history: Vec::new(),
        },
        Task::batch([open.map(move |_| Message::OpenWindow(Some(id)))]),
    )
}

pub fn view(tile: &Tile, wid: window::Id) -> Element<'_, Message> {
    // Agent chat window — completely separate view
    if tile.agent_window_id == Some(wid) {
        return agent_window_view(tile);
    }

    // During hide animation, keep rendering until the animation completes
    if !tile.visible && !tile.hide_animating {
        return space().into();
    }

    let show_main_empty_state =
        tile.page == Page::Main && !tile.query.is_empty() && tile.results.is_empty();
    let current_launcher_mode = launcher_mode_for_page(&tile.page);

    let round_bottom_edges = match &tile.page {
        Page::Main => tile.results.is_empty() && !show_main_empty_state,
        Page::EmojiSearch => tile.results.is_empty(),
        Page::ClipboardHistory => tile.clipboard_display_count() == 0,
        Page::AgentList => false,
        Page::WindowSwitcher => tile.window_list.is_empty(),
    };
    let theme = &tile.config.theme;

    // ── Search bar ────────────────────────────────────────────────────
    let search_icon = container(crate::icons::icon(
        crate::icons::SEARCH,
        20.0,
        theme.text_color(0.62),
    ))
    .padding([0, 2]);

    let mut search_font = theme.font();
    search_font.weight = Weight::Semibold;

    let title_input = text_input(tile.config.placeholder.as_str(), &tile.query)
        .on_input(move |a| Message::SearchQueryChanged(a, wid))
        .on_paste(move |a| Message::SearchQueryChanged(a, wid))
        .font(search_font)
        .on_submit(Message::OpenFocused)
        .id("query")
        .width(Fill)
        .size(18)
        .line_height(LineHeight::Relative(1.25))
        .style(move |_, _| coco_text_input_style(theme, round_bottom_edges))
        .padding([0, 0]);

    let mut search_row = Row::new()
        .push(search_icon)
        .push(title_input)
        .spacing(10)
        .align_y(Alignment::Center)
        .width(Fill)
        .height(42);

    if let Some(mode) = current_launcher_mode {
        search_row = search_row.push(mode_switch(mode, theme));
    }

    let search_bar = container(search_row)
        .padding([8, 18])
        .width(Fill)
        .height(crate::app::SEARCH_BAR_HEIGHT as u32);

    // ── Scrollbar direction ───────────────────────────────────────────
    let scrollbar_direction = if theme.show_scroll_bar {
        Direction::Vertical(
            Scrollbar::new()
                .width(6)
                .scroller_width(6)
                .anchor(Anchor::Start),
        )
    } else {
        Direction::Vertical(Scrollbar::hidden())
    };

    // ── Results content ───────────────────────────────────────────────
    let results = if tile.page == Page::ClipboardHistory {
        clipboard_view(
            tile.clipboard_store.all(),
            tile.clipboard_display_indices(),
            tile.focus_id,
            theme,
        )
    } else if tile.page == Page::AgentList {
        crate::app::pages::agent::agent_list_view(
            &tile.agent_sessions,
            tile.agent_display_indices(),
            tile.focus_id,
            theme.clone(),
        )
    } else if tile.page == Page::WindowSwitcher {
        window_switcher_view(&tile.window_list, tile.focus_id, theme)
    } else if show_main_empty_state {
        main_empty_search_state_view(theme)
    } else if tile.results.is_empty() {
        space().into()
    } else if tile.page == Page::EmojiSearch {
        emoji_page(theme.clone(), tile.results.clone(), tile.focus_id)
    } else if tile.query.is_empty() && tile.results.iter().any(|a| a.category.is_some()) {
        // Zero-query state: render with group headers
        zero_query_results_view(&tile.results, tile.focus_id, theme)
    } else {
        search_results_view(&tile.results, tile.focus_id, theme)
    };

    let results_count = match &tile.page {
        Page::Main => tile.results.len(),
        Page::ClipboardHistory => tile.clipboard_display_count(),
        Page::EmojiSearch => tile.results.len(),
        Page::AgentList => tile.agent_display_count(),
        Page::WindowSwitcher => tile.window_list.len(),
    };

    let has_results = match &tile.page {
        Page::Main | Page::EmojiSearch => !tile.results.is_empty(),
        Page::ClipboardHistory => tile.clipboard_display_count() > 0,
        Page::AgentList => true,
        Page::WindowSwitcher => !tile.window_list.is_empty(),
    };

    let has_body_content = has_results || show_main_empty_state;

    let height = match tile.page {
        Page::ClipboardHistory => crate::app::CLIPBOARD_CONTENT_HEIGHT as usize,
        Page::AgentList => std::cmp::min(tile.agent_display_count() * 52, 364),
        Page::WindowSwitcher => std::cmp::min(tile.window_list.len() * 52, 364),
        _ => {
            // For zero-query state, account for section headers
            let is_zero_query =
                tile.query.is_empty() && tile.results.iter().any(|a| a.category.is_some());
            if is_zero_query {
                let h = crate::app::tile::update::zero_query_scrollable_height(&tile.results);
                h.min(crate::app::MAX_RESULTS_SCROLL_HEIGHT) as usize
            } else if show_main_empty_state {
                crate::app::MAIN_EMPTY_STATE_HEIGHT as usize
            } else if tile.page == Page::Main && !tile.query.is_empty() {
                let h = crate::app::tile::update::search_results_scrollable_height(&tile.results);
                h.min(crate::app::MAX_RESULTS_SCROLL_HEIGHT) as usize
            } else {
                let max_h = crate::app::MAX_RESULTS_SCROLL_HEIGHT as usize;
                std::cmp::min(tile.results.len() * crate::app::ROW_HEIGHT as usize, max_h)
            }
        }
    };

    let scrollable = Scrollable::with_direction(results, scrollbar_direction)
        .id("results")
        .height(height as u32);
    let scrollable: Element<'_, Message> = mouse_area(scrollable)
        .on_move(|position| Message::ResultPointerMoved(position.x))
        .on_exit(Message::ResultPointerExited)
        .into();

    // ── Separator ─────────────────────────────────────────────────────
    let theme_for_sep = theme.clone();
    let separator: Element<'_, Message> = if has_body_content {
        container(space().height(1).width(Fill))
            .padding([0, 18])
            .width(Fill)
            .style(move |_| separator_style(&theme_for_sep))
            .into()
    } else {
        space().height(0).into()
    };

    // ── Permission banners ─────────────────────────────────────────────
    let banners = permission_banners(tile, theme);

    // ── Assembly ──────────────────────────────────────────────────────
    let mut column = Column::new().push(search_bar).push(banners).push(separator);

    // If actions overlay is open, show it instead of regular results
    if tile.show_actions && !tile.actions.is_empty() {
        column = column.push(actions_overlay_view(
            &tile.actions,
            &tile.action_target_name,
            tile.action_focus_id,
            theme,
        ));
        column = column.push(actions_footer(theme.clone(), tile.actions.len()));
    } else {
        column = column.push(scrollable);
        if has_results {
            if tile.page == Page::ClipboardHistory {
                column = column.push(crate::app::pages::clipboard::clipboard_footer(
                    theme,
                    results_count,
                ));
            } else if tile.page != Page::Main {
                column = column.push(footer(theme.clone(), results_count));
            } else {
                // Spotlight-style main results omit the persistent shortcut footer.
            }
        }
    }
    column = column.spacing(0);

    // No explicit height — iced determines natural content height.
    // Window is resized to match, blur fills window. Always accurate.
    // Do not apply a rectangular top-level clip here: during native scale
    // animations it makes the rounded panel corners appear square.
    let clipped_content = container(column).width(Fill);
    let theme_for_outer = theme.clone();
    let visual_panel_h = (tile.target_blur_height.max(1.0)).round() as f32;
    let inner = container(clipped_content)
        .width(Fill)
        .height(visual_panel_h)
        .style(move |_| contents_style(&theme_for_outer));

    // Outer container fills the fixed-size window with transparent bg.
    // align_top keeps the glass panel at the top; transparent area below is invisible.
    container(inner)
        .width(Fill)
        .align_top(Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(iced::Color::TRANSPARENT)),
            ..Default::default()
        })
        .into()
}

fn launcher_mode_for_page(page: &Page) -> Option<LauncherMode> {
    match page {
        Page::Main => Some(LauncherMode::App),
        Page::ClipboardHistory => Some(LauncherMode::Clipboard),
        Page::AgentList => None,
        _ => None,
    }
}

fn mode_switch<'a>(active: LauncherMode, theme: &Theme) -> Element<'a, Message> {
    let row = Row::new()
        .push(mode_switch_segment(LauncherMode::App, active, theme))
        .push(mode_switch_segment(LauncherMode::Clipboard, active, theme))
        .spacing(8)
        .align_y(Alignment::Center);

    let theme_for_container = theme.clone();
    container(row)
        .padding([2, 2])
        .style(move |_| mode_switch_container_style(&theme_for_container))
        .into()
}

fn mode_switch_segment<'a>(
    mode: LauncherMode,
    active: LauncherMode,
    theme: &Theme,
) -> Element<'a, Message> {
    let is_active = mode == active;
    let icon_color = if is_active {
        theme.accent_color()
    } else {
        theme.text_color(0.45)
    };
    let icon_char = match mode {
        LauncherMode::App => crate::icons::APP_PACKAGE,
        LauncherMode::Clipboard => crate::icons::CLIPBOARD,
    };
    let icon_size = 15.0;
    let icon = container(crate::icons::icon(icon_char, icon_size, icon_color))
        .center(Fill)
        .width(Fill)
        .height(Fill);
    let theme_for_btn = theme.clone();
    Button::new(icon)
        .on_press(Message::SwitchLauncherMode(mode))
        .style(move |_, _| mode_switch_button_style(&theme_for_btn, is_active))
        .padding([0, 0])
        .width(24)
        .height(24)
        .into()
}

// ── Permission banners ────────────────────────────────────────────────────

fn permission_banners<'a>(tile: &Tile, theme: &crate::config::Theme) -> Element<'a, Message> {
    if tile.permissions_ok {
        return space().height(0).into();
    }

    let mut col = Column::new().spacing(4).padding([4, 14]);

    if tile.missing_accessibility {
        let theme_for_banner = theme.clone();
        let theme_for_btn = theme.clone();
        let row = Row::new()
            .push(
                Row::new()
                    .push(crate::icons::icon(
                        crate::icons::EXCLAMATION_TRIANGLE,
                        12.0,
                        theme.text_color(0.75),
                    ))
                    .push(
                        Text::new(" 需要「辅助功能」权限才能正常切换应用")
                            .size(12)
                            .color(theme.text_color(0.75))
                            .font(theme.font()),
                    )
                    .align_y(Alignment::Center),
            )
            .push(space().width(Fill))
            .push(
                Button::new(Text::new("前往设置 →").size(12).font(theme.font()))
                    .on_press(Message::OpenAccessibilitySettings)
                    .padding([2, 8])
                    .style(move |_, _| permission_banner_button_style(&theme_for_btn)),
            )
            .align_y(Alignment::Center)
            .width(Fill)
            .height(28);

        col = col.push(
            container(row)
                .width(Fill)
                .padding([2, 8])
                .style(move |_| permission_banner_style(&theme_for_banner)),
        );
    }

    if tile.missing_input_monitoring {
        let theme_for_banner = theme.clone();
        let theme_for_btn = theme.clone();
        let row = Row::new()
            .push(
                Row::new()
                    .push(crate::icons::icon(
                        crate::icons::EXCLAMATION_TRIANGLE,
                        12.0,
                        theme.text_color(0.75),
                    ))
                    .push(
                        Text::new(" 需要「输入监控」权限才能使用快捷键唤起")
                            .size(12)
                            .color(theme.text_color(0.75))
                            .font(theme.font()),
                    )
                    .align_y(Alignment::Center),
            )
            .push(space().width(Fill))
            .push(
                Button::new(Text::new("前往设置 →").size(12).font(theme.font()))
                    .on_press(Message::OpenInputMonitoringSettings)
                    .padding([2, 8])
                    .style(move |_, _| permission_banner_button_style(&theme_for_btn)),
            )
            .align_y(Alignment::Center)
            .width(Fill)
            .height(28);

        col = col.push(
            container(row)
                .width(Fill)
                .padding([2, 8])
                .style(move |_| permission_banner_style(&theme_for_banner)),
        );
    }

    if tile.missing_paste_permission {
        let theme_for_banner = theme.clone();
        let theme_for_btn = theme.clone();
        let row = Row::new()
            .push(
                Row::new()
                    .push(crate::icons::icon(
                        crate::icons::EXCLAMATION_TRIANGLE,
                        12.0,
                        theme.text_color(0.75),
                    ))
                    .push(
                        Text::new(" 自动粘贴失败，请在系统设置中开启 Coco 的键盘控制权限")
                            .size(12)
                            .color(theme.text_color(0.75))
                            .font(theme.font()),
                    )
                    .align_y(Alignment::Center),
            )
            .push(space().width(Fill))
            .push(
                Button::new(Text::new("前往设置 →").size(12).font(theme.font()))
                    .on_press(Message::OpenAccessibilitySettings)
                    .padding([2, 8])
                    .style(move |_, _| permission_banner_button_style(&theme_for_btn)),
            )
            .align_y(Alignment::Center)
            .width(Fill)
            .height(28);

        col = col.push(
            container(row)
                .width(Fill)
                .padding([2, 8])
                .style(move |_| permission_banner_style(&theme_for_banner)),
        );
    }

    col.into()
}

// ── Footer ────────────────────────────────────────────────────────────────

fn footer(theme: Theme, results_count: usize) -> Element<'static, Message> {
    if results_count == 0 {
        return space().into();
    }

    let count_text = if results_count == 1 {
        "1 result".to_string()
    } else {
        format!("{} results", results_count)
    };

    let left = Text::new(count_text)
        .size(11)
        .color(theme.text_color(0.35))
        .font(theme.font());

    let theme_clone = theme.clone();

    let open_badge = shortcut_badge_icon(crate::icons::ARROW_RETURN_LEFT, "Open", &theme);
    let esc_badge = shortcut_badge("ESC", "Back", &theme);

    let right = Row::new()
        .push(open_badge)
        .push(esc_badge)
        .spacing(8)
        .align_y(Alignment::Center);

    let row = Row::new()
        .push(container(left).width(Fill))
        .push(right)
        .align_y(Alignment::Center)
        .width(Fill)
        .height(28)
        .padding([0, 16]);

    container(row)
        .width(Fill)
        .padding([4, 0])
        .style(move |_| footer_style(&theme_clone))
        .into()
}

// ── Agent window view ────────────────────────────────────────────────────

fn agent_window_view(tile: &Tile) -> Element<'_, Message> {
    let theme = &tile.config.theme;

    // ── Title bar (draggable area) ──
    let status_text = match tile.agent_status {
        AgentStatus::Idle => "Ready",
        AgentStatus::Thinking => "Thinking...",
        AgentStatus::Streaming => "Streaming...",
    };

    let title_bar = container(
        Row::new()
            .push(
                Text::new("Claude Agent")
                    .font(theme.font())
                    .size(13)
                    .color(theme.text_color(0.9)),
            )
            .push(space().width(Fill))
            .push(
                Text::new(status_text)
                    .font(theme.font())
                    .size(11)
                    .color(theme.text_color(0.5)),
            )
            .align_y(Alignment::Center)
            .width(Fill)
            .padding([0, 16]),
    )
    .width(Fill)
    .height(36)
    .style({
        let t = theme.clone();
        move |_| agent_title_bar_style(&t)
    });

    // ── Messages area ──
    let mut messages_col = Column::new().spacing(8).padding([12, 16]).width(Fill);

    for msg in &tile.agent_messages {
        match msg {
            ChatMessage::User(text) => {
                let theme_for_bubble = theme.clone();
                let bubble = container(
                    Text::new(text.clone())
                        .font(theme.font())
                        .size(13)
                        .color(theme.text_color(0.95)),
                )
                .padding([8, 12])
                .max_width(500)
                .style(move |_| user_bubble_style(&theme_for_bubble));

                messages_col = messages_col.push(container(bubble).width(Fill).align_right(Fill));
            }
            ChatMessage::Assistant(text) => {
                if text.is_empty() && tile.agent_status == AgentStatus::Thinking {
                    messages_col = messages_col.push(
                        Text::new("Thinking...")
                            .font(theme.font())
                            .size(13)
                            .color(theme.text_color(0.4)),
                    );
                } else if !text.is_empty() {
                    // Use the pre-parsed markdown Content from tile
                    let md_view: Element<'_, markdown::Uri> = markdown::view(
                        tile.agent_markdown.items(),
                        markdown::Settings::with_style(markdown::Style::from_palette(
                            tile.theme.palette(),
                        )),
                    );
                    let md_mapped = md_view.map(|_url| Message::AgentSubmit);
                    messages_col =
                        messages_col.push(container(md_mapped).max_width(600).width(Fill));
                }
            }
        }
    }

    let scrollbar_dir = Direction::Vertical(
        Scrollbar::new()
            .width(4)
            .scroller_width(4)
            .anchor(Anchor::End),
    );

    let messages_scroll = Scrollable::with_direction(messages_col, scrollbar_dir)
        .id("agent-messages")
        .height(Fill)
        .width(Fill);

    // ── Input bar ──
    let input = text_input("Send a message...", &tile.agent_input)
        .on_input(Message::AgentInput)
        .on_submit(Message::AgentSubmit)
        .font(theme.font())
        .size(14)
        .id("agent-input")
        .width(Fill)
        .padding([8, 12]);

    let theme_for_input = theme.clone();
    let input_bar = container(input)
        .width(Fill)
        .padding([8, 16])
        .style(move |_| agent_input_bar_style(&theme_for_input));

    // ── Assembly ──
    let content = Column::new()
        .push(title_bar)
        .push(messages_scroll)
        .push(input_bar)
        .width(Fill)
        .height(Fill);

    let theme_for_outer = theme.clone();
    container(
        container(content)
            .width(Fill)
            .height(Fill)
            .clip(true)
            .style(move |_| agent_content_style(&theme_for_outer)),
    )
    .width(Fill)
    .height(Fill)
    .style(|_| container::Style {
        background: Some(iced::Background::Color(iced::Color::TRANSPARENT)),
        ..Default::default()
    })
    .into()
}

// ── Window Switcher view ──────────────────────────────────────────────────

fn window_switcher_view<'a>(
    windows: &[crate::platform::WindowInfo],
    focus_id: u32,
    theme: &Theme,
) -> Element<'a, Message> {
    let mut col = Column::new().padding([
        crate::app::RESULT_LIST_PADDING_Y,
        crate::app::RESULT_LIST_PADDING_X,
    ]);

    for (i, win) in windows.iter().enumerate() {
        let focused = i as u32 == focus_id;
        let title_opacity = if focused { 1.0 } else { 0.88 };
        let desc_opacity = if focused { 0.50 } else { 0.38 };

        let text_block = Column::new()
            .spacing(1)
            .push(
                Text::new(win.window_title.clone())
                    .font(theme.font())
                    .size(14)
                    .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                    .color(theme.text_color(title_opacity)),
            )
            .push(
                Text::new(win.owner_name.clone())
                    .font(theme.font())
                    .size(11)
                    .color(theme.text_color(desc_opacity)),
            );

        let mut row = Row::new()
            .align_y(Alignment::Center)
            .width(Fill)
            .spacing(12)
            .height(44);

        if theme.show_icons {
            if let Some(ref icon) = win.icon {
                row = row.push(
                    container(iced::widget::image(icon.clone()).height(32).width(32))
                        .width(32)
                        .height(32),
                );
            }
        }
        row = row.push(container(text_block).width(Fill));

        if focused {
            row = row.push(crate::icons::icon(
                crate::icons::ARROW_RETURN_LEFT,
                13.0,
                theme.text_color(0.25),
            ));
        }

        let pid = win.owner_pid;
        let wid = win.window_id;
        let theme_for_btn = theme.clone();
        let theme_for_cont = theme.clone();

        let btn = Button::new(row)
            .on_press(Message::FocusWindow(pid, wid))
            .style(move |_, status| {
                crate::styles::result_button_style(&theme_for_btn, focused, status)
            })
            .width(Fill)
            .padding(0)
            .height(44);

        col = col.push(
            container(btn)
                .id(format!("result-{}", i))
                .style(move |_| crate::styles::result_row_container_style(&theme_for_cont, focused))
                .padding([4, 8])
                .width(Fill),
        );
    }

    container(col).into()
}

// ── Actions overlay view ──────────────────────────────────────────────────

fn actions_overlay_view<'a>(
    actions: &[crate::app::actions::ActionItem],
    target_name: &str,
    focus_id: u32,
    theme: &Theme,
) -> Element<'a, Message> {
    use crate::app::actions::ActionGroup;
    use crate::styles::{action_row_style, action_separator_style, destructive_text_color};

    let mut col = Column::new().padding([4, 6]).spacing(2);

    // Header
    col = col.push(
        container(
            Text::new(format!("Actions for {}", target_name))
                .size(12)
                .color(theme.text_color(0.50))
                .font(theme.font()),
        )
        .padding([4, 10])
        .width(Fill),
    );

    let mut last_group: Option<&ActionGroup> = None;

    for (i, item) in actions.iter().enumerate() {
        // Add separator between groups
        if let Some(prev) = last_group {
            if prev != &item.group {
                let sep: Element<'a, Message> = container(space().height(1).width(Fill))
                    .padding([2, 8])
                    .width(Fill)
                    .style(move |_| action_separator_style())
                    .into();
                col = col.push(sep);
            }
        }
        last_group = Some(&item.group);

        let focused = i as u32 == focus_id;
        let theme_for_row = theme.clone();

        let label_color = if item.is_destructive {
            destructive_text_color()
        } else {
            theme.text_color(if focused { 1.0 } else { 0.85 })
        };

        let mut row = Row::new()
            .align_y(Alignment::Center)
            .width(Fill)
            .spacing(8)
            .height(32);

        // Focus indicator
        if focused {
            row = row.push(
                Text::new("\u{25B8}")
                    .size(11)
                    .color(theme.text_color(0.60))
                    .font(theme.font()),
            );
        } else {
            row = row.push(space().width(12));
        }

        row = row.push(
            container(
                Text::new(item.label.clone())
                    .size(13)
                    .color(label_color)
                    .font(theme.font()),
            )
            .width(Fill),
        );

        // Shortcut hint
        if let Some(shortcut) = item.shortcut {
            row = row.push(
                Text::new(shortcut)
                    .size(11)
                    .color(theme.text_color(0.30))
                    .font(theme.font()),
            );
        }

        let action_clone = item.action.clone();
        let btn = Button::new(row)
            .on_press(Message::ExecuteAction(action_clone))
            .style(move |_, status| {
                crate::styles::result_button_style(&theme_for_row, focused, status)
            })
            .width(Fill)
            .padding(0)
            .height(32);

        let theme_for_container = theme.clone();
        col = col.push(
            container(btn)
                .style(move |_| action_row_style(&theme_for_container, focused))
                .padding([1, 6])
                .width(Fill),
        );
    }

    let height = std::cmp::min(actions.len() * 36 + 30, 364);

    Scrollable::with_direction(col, Direction::Vertical(Scrollbar::hidden()))
        .height(height as u32)
        .into()
}

fn actions_footer(theme: Theme, count: usize) -> Element<'static, Message> {
    let count_text = if count == 1 {
        "1 action".to_string()
    } else {
        format!("{} actions", count)
    };

    let left = Text::new(count_text)
        .size(11)
        .color(theme.text_color(0.35))
        .font(theme.font());

    let theme_clone = theme.clone();

    let esc_badge = shortcut_badge("ESC", "Close", &theme);
    let enter_badge = shortcut_badge_icon(crate::icons::ARROW_RETURN_LEFT, "Run", &theme);

    let right = Row::new()
        .push(esc_badge)
        .push(enter_badge)
        .spacing(8)
        .align_y(Alignment::Center);

    let row = Row::new()
        .push(container(left).width(Fill))
        .push(right)
        .align_y(Alignment::Center)
        .width(Fill)
        .height(28)
        .padding([0, 16]);

    container(row)
        .width(Fill)
        .padding([4, 0])
        .style(move |_| crate::styles::footer_style(&theme_clone))
        .into()
}

// ── Zero-query state view ──────────────────────────────────────────────────

fn zero_query_results_view<'a>(
    results: &[crate::app::apps::App],
    focus_id: u32,
    theme: &Theme,
) -> Element<'a, Message> {
    use crate::app::apps::AppCategory;
    use crate::styles::section_header_style;

    let mut col = Column::new().padding([
        crate::app::RESULT_LIST_PADDING_Y,
        crate::app::RESULT_LIST_PADDING_X,
    ]);
    let mut last_category: Option<&AppCategory> = None;

    for (i, app) in results.iter().enumerate() {
        // Insert group header when category changes
        if let Some(ref cat) = app.category {
            let should_show_header = match last_category {
                None => true,
                Some(prev) => prev != cat,
            };
            if should_show_header {
                let header_text = match cat {
                    AppCategory::Running => "RUNNING",
                    AppCategory::Recent => "RECENT",
                };
                let theme_for_header = theme.clone();
                let header: Element<'a, Message> = container(
                    Text::new(header_text)
                        .size(10)
                        .color(theme.text_color(0.24))
                        .font(theme.font()),
                )
                .padding([
                    crate::app::ZQ_HEADER_PADDING_Y,
                    crate::app::ZQ_HEADER_PADDING_X,
                ])
                .width(Fill)
                .height(crate::app::ZQ_HEADER_HEIGHT as f32)
                .style(move |_| section_header_style(&theme_for_header))
                .into();
                col = col.push(header);
            }
            last_category = Some(cat);
        }

        col = col.push(
            app.clone()
                .render_with_status(theme.clone(), i as u32, focus_id),
        );
    }

    container(col).into()
}

fn main_empty_search_state_view<'a>(theme: &Theme) -> Element<'a, Message> {
    let title = Text::new("No results")
        .size(13)
        .color(theme.text_color(0.55))
        .font(theme.font());

    let subtitle = Text::new("Try a different keyword")
        .size(11)
        .color(theme.text_color(0.30))
        .font(theme.font());

    container(
        Column::new()
            .push(title)
            .push(subtitle)
            .spacing(4)
            .align_x(Alignment::Center),
    )
    .width(Fill)
    .height(crate::app::MAIN_EMPTY_STATE_HEIGHT as u32)
    .center(Fill)
    .into()
}

fn search_results_view<'a>(
    results: &[crate::app::apps::App],
    focus_id: u32,
    theme: &Theme,
) -> Element<'a, Message> {
    use crate::styles::section_header_style;

    let is_calculator_section = !results.is_empty()
        && results
            .iter()
            .all(|app| {
                app.name_lc.starts_with("__calc__|")
                    || app.name_lc.starts_with("__calc_history__|")
            });
    let is_currency_section = !results.is_empty()
        && results
            .iter()
            .all(|app| app.name_lc.starts_with("__currency__|"));

    let mut col = Column::new().padding([
        crate::app::RESULT_LIST_PADDING_Y,
        crate::app::RESULT_LIST_PADDING_X,
    ]);

    if !is_calculator_section {
        let header_label = if is_currency_section {
            "CURRENCY"
        } else {
            "APPS"
        };
        let header_row = {
            let mut row = Row::new().align_y(Alignment::Center).width(Fill).push(
                Text::new(header_label)
                    .size(10)
                    .color(theme.text_color(0.24))
                    .font(theme.font()),
            );

            if is_currency_section {
                let updated = crate::currency_conversion::last_updated_label();
                let theme_for_badge = theme.clone();
                row = row.push(space::horizontal()).push(
                    container(
                        Text::new(format!("更新 {updated}"))
                            .size(9)
                            .color(theme.text_color(0.34))
                            .font(theme.font()),
                    )
                    .padding([1, 6])
                    .style(move |_| footer_shortcut_badge_style(&theme_for_badge)),
                );
            }

            row
        };

        let theme_for_header = theme.clone();
        let header: Element<'a, Message> = container(header_row)
            .padding([
                crate::app::ZQ_HEADER_PADDING_Y,
                crate::app::ZQ_HEADER_PADDING_X,
            ])
            .width(Fill)
            .height(crate::app::ZQ_HEADER_HEIGHT as f32)
            .style(move |_| section_header_style(&theme_for_header))
            .into();
        col = col.push(header);
    }

    for (i, app) in results.iter().enumerate() {
        col = col.push(app.clone().render(theme.clone(), i as u32, focus_id));
    }

    container(col).into()
}

fn shortcut_badge_icon<'a>(icon_char: char, label: &'a str, theme: &Theme) -> Element<'a, Message> {
    let theme_for_badge = theme.clone();
    let badge = container(crate::icons::icon(icon_char, 10.0, theme.text_color(0.46)))
        .padding([1, 6])
        .style(move |_| footer_shortcut_badge_style(&theme_for_badge));

    let label_text = Text::new(label.to_string())
        .size(10)
        .color(theme.text_color(0.30))
        .font(theme.font());

    Row::new()
        .push(badge)
        .push(label_text)
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
}

fn shortcut_badge<'a>(key: &'a str, label: &'a str, theme: &Theme) -> Element<'a, Message> {
    let theme_for_badge = theme.clone();
    let badge = container(
        Text::new(key.to_string())
            .size(10)
            .color(theme.text_color(0.46))
            .font(theme.font()),
    )
    .padding([1, 6])
    .style(move |_| footer_shortcut_badge_style(&theme_for_badge));

    let label_text = Text::new(label.to_string())
        .size(10)
        .color(theme.text_color(0.30))
        .font(theme.font());

    Row::new()
        .push(badge)
        .push(label_text)
        .spacing(4)
        .align_y(Alignment::Center)
        .into()
}
