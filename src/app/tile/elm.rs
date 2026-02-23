//! This module handles the logic for the new and view functions according to the elm
//! architecture. If the subscription function becomes too large, it should be moved to this file

use std::cell::RefCell;

use global_hotkey::hotkey::HotKey;
use iced::widget::scrollable::{Anchor, Direction, Scrollbar};
use iced::widget::text::LineHeight;
use iced::widget::{Column, Row, Scrollable, Text, container, space};
use iced::{Alignment, window};
use iced::{Element, Task};
use iced::{Length::Fill, widget::text_input};

use nucleo_matcher::{Config as MatcherConfig, Matcher};
use rayon::slice::ParallelSliceMut;

use crate::app::pages::emoji::emoji_page;
use crate::app::tile::AppIndex;
use crate::config::Theme;
use crate::styles::{
    contents_style, footer_shortcut_badge_style, footer_style, rustcast_text_input_style,
    separator_style,
};
use crate::platform;
use crate::{app::pages::clipboard::clipboard_view, platform::get_installed_apps};
use crate::{
    app::{Message, Page, apps::App, default_settings, tile::Tile},
    config::Config,
    platform::transform_process_to_ui_element,
};

/// Initialise the base window
pub fn new(hotkey: HotKey, config: &Config) -> (Tile, Task<Message>) {
    let (id, open) = window::open(default_settings());

    let open = open.discard().chain(window::run(id, |handle| {
        let wh = handle.window_handle().expect("Unable to get window handle");
        platform::window_config(&wh);
        platform::create_blur_child_window(&wh, crate::app::WINDOW_WIDTH as f64, 52.0);
        transform_process_to_ui_element();
    }));

    crate::currency_conversion::spawn_rate_updater();

    let store_icons = config.theme.show_icons;

    let mut options = get_installed_apps(store_icons);

    options.extend(config.shells.iter().map(|x| x.to_app()));
    options.extend(App::basic_apps());
    options.par_sort_by_key(|x| x.name.len());
    let options = AppIndex::from_apps(options);

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
            clipboard_content: vec![],
            tray_icon: None,
            sender: None,
            page: Page::Main,
            fuzzy_matcher: RefCell::new(Matcher::new(MatcherConfig::DEFAULT)),
        },
        Task::batch([open.map(|_| Message::OpenWindow)]),
    )
}

pub fn view(tile: &Tile, wid: window::Id) -> Element<'_, Message> {
    if !tile.visible {
        return space().into();
    }

    let round_bottom_edges = match &tile.page {
        Page::Main | Page::EmojiSearch => tile.results.is_empty(),
        Page::ClipboardHistory => tile.clipboard_content.is_empty(),
    };
    let theme = &tile.config.theme;

    // ── Search bar ────────────────────────────────────────────────────
    let search_icon = container(
        Text::new("\u{1F50E}\u{FE0E}") // magnifying glass, text presentation
            .size(16)
            .color(theme.text_color(0.35)),
    )
    .padding([0, 2]);

    let title_input = text_input(tile.config.placeholder.as_str(), &tile.query)
        .on_input(move |a| Message::SearchQueryChanged(a, wid))
        .on_paste(move |a| Message::SearchQueryChanged(a, wid))
        .font(theme.font())
        .on_submit(Message::OpenFocused)
        .id("query")
        .width(Fill)
        .size(17)
        .line_height(LineHeight::Relative(1.4))
        .style(move |_, _| rustcast_text_input_style(theme, round_bottom_edges))
        .padding([0, 0]);

    let search_bar = container(
        Row::new()
            .push(search_icon)
            .push(title_input)
            .spacing(8)
            .align_y(Alignment::Center)
            .width(Fill)
            .height(52),
    )
    .padding([0, 18])
    .width(Fill);

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
            tile.clipboard_content.clone(),
            tile.focus_id,
            theme.clone(),
            tile.focus_id,
        )
    } else if tile.results.is_empty() {
        space().into()
    } else if tile.page == Page::EmojiSearch {
        emoji_page(theme.clone(), tile.results.clone(), tile.focus_id)
    } else {
        container(
            Column::from_iter(tile.results.iter().enumerate().map(|(i, app)| {
                app.clone()
                    .render(theme.clone(), i as u32, tile.focus_id)
            }))
            .padding([2, 6]),
        )
        .into()
    };

    let results_count = match &tile.page {
        Page::Main => tile.results.len(),
        Page::ClipboardHistory => tile.clipboard_content.len(),
        Page::EmojiSearch => tile.results.len(),
    };

    let has_results = match &tile.page {
        Page::Main | Page::EmojiSearch => !tile.results.is_empty(),
        Page::ClipboardHistory => !tile.clipboard_content.is_empty(),
    };

    let height = if tile.page == Page::ClipboardHistory {
        360
    } else {
        std::cmp::min(tile.results.len() * 52, 364)
    };

    let scrollable = Scrollable::with_direction(results, scrollbar_direction)
        .id("results")
        .height(height as u32);

    // ── Separator ─────────────────────────────────────────────────────
    let theme_for_sep = theme.clone();
    let separator: Element<'_, Message> = if has_results {
        container(space().height(1).width(Fill))
            .padding([0, 14])
            .width(Fill)
            .style(move |_| separator_style(&theme_for_sep))
            .into()
    } else {
        space().height(0).into()
    };

    // ── Assembly ──────────────────────────────────────────────────────
    let mut column = Column::new()
        .push(search_bar)
        .push(separator)
        .push(scrollable)
        .spacing(0);

    if has_results {
        column = column.push(footer(theme.clone(), results_count));
    }

    let theme_for_outer = theme.clone();
    let inner = container(column)
        .clip(true)
        .width(Fill)
        .style(move |_| contents_style(&theme_for_outer));

    // Outer container fills the window with transparent bg.
    // align_top ensures the glass inner panel stays at the top;
    // any extra window area below is fully transparent (blur shows through
    // but no dark background), so it's invisible.
    container(inner)
        .width(Fill)
        .align_top(Fill)
        .style(|_| container::Style {
            background: Some(iced::Background::Color(iced::Color::TRANSPARENT)),
            ..Default::default()
        })
        .into()
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

    let open_badge = shortcut_badge("\u{23CE}", "Open", &theme);
    let actions_badge = shortcut_badge("\u{2318}K", "Actions", &theme);

    let right = Row::new()
        .push(open_badge)
        .push(actions_badge)
        .spacing(8)
        .align_y(Alignment::Center);

    let row = Row::new()
        .push(container(left).width(Fill))
        .push(right)
        .align_y(Alignment::Center)
        .width(Fill)
        .height(28)
        .padding([0, 18]);

    container(row)
        .width(Fill)
        .padding([5, 0])
        .style(move |_| footer_style(&theme_clone))
        .into()
}

fn shortcut_badge<'a>(key: &'a str, label: &'a str, theme: &Theme) -> Element<'a, Message> {
    let theme_for_badge = theme.clone();
    let badge = container(
        Text::new(key.to_string())
            .size(10)
            .color(theme.text_color(0.50))
            .font(theme.font()),
    )
    .padding([1, 5])
    .style(move |_| footer_shortcut_badge_style(&theme_for_badge));

    let label_text = Text::new(label.to_string())
        .size(11)
        .color(theme.text_color(0.35))
        .font(theme.font());

    Row::new()
        .push(badge)
        .push(label_text)
        .spacing(3)
        .align_y(Alignment::Center)
        .into()
}
