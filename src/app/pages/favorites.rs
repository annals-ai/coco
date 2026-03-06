//! Clipboard favorites list view.

use iced::widget::scrollable::Anchor;
use iced::widget::{
    Button, Column, Row, Scrollable, Text, container, mouse_area,
    scrollable::{Direction, Scrollbar},
    space,
};
use iced::{Alignment, Color, Element, Length};

use crate::app::{Message, Page, WINDOW_WIDTH};
use crate::clipboard::ClipBoardContentType;
use crate::clipboard_store::format_relative_time;
use crate::config::Theme;
use crate::favorite_store::FavoriteEntry;
use crate::styles::{
    clipboard_preview_style, footer_shortcut_badge_style, footer_style, result_button_style,
    result_row_container_style, with_alpha,
};

/// Sub-tab bar for clipboard modes (History / Favorites).
pub fn clipboard_sub_tabs<'a>(active_page: &Page, theme: &Theme) -> Element<'a, Message> {
    let history_active = *active_page == Page::ClipboardHistory;
    let favorites_active = *active_page == Page::ClipboardFavorites;

    let history_label = Text::new("History")
        .size(11)
        .color(theme.text_color(if history_active { 0.95 } else { 0.40 }))
        .font(theme.font());
    let fav_label = Text::new("Favorites")
        .size(11)
        .color(theme.text_color(if favorites_active { 0.95 } else { 0.40 }))
        .font(theme.font());

    let theme_h = theme.clone();
    let theme_f = theme.clone();

    let history_btn = Button::new(
        container(history_label)
            .center(Length::Fill)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(Message::SwitchToPage(Page::ClipboardHistory))
    .style(move |_, _| sub_tab_button_style(&theme_h, history_active))
    .padding([2, 10])
    .height(22);

    let fav_btn = Button::new(
        container(fav_label)
            .center(Length::Fill)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(Message::SwitchToPage(Page::ClipboardFavorites))
    .style(move |_, _| sub_tab_button_style(&theme_f, favorites_active))
    .padding([2, 10])
    .height(22);

    let theme_for_container = theme.clone();
    container(
        Row::new()
            .push(history_btn)
            .push(fav_btn)
            .spacing(4)
            .align_y(Alignment::Center)
            .height(22),
    )
    .padding([6, 8])
    .width(Length::Fill)
    .style(move |_| sub_tab_container_style(&theme_for_container))
    .into()
}

fn sub_tab_button_style(theme: &Theme, active: bool) -> iced::widget::button::Style {
    iced::widget::button::Style {
        background: if active {
            Some(iced::Background::Color(with_alpha(Color::WHITE, 0.10)))
        } else {
            None
        },
        border: iced::Border {
            radius: 4.0.into(),
            ..Default::default()
        },
        text_color: theme.text_color(if active { 0.95 } else { 0.40 }),
        ..Default::default()
    }
}

fn sub_tab_container_style(_theme: &Theme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(iced::Background::Color(with_alpha(Color::BLACK, 0.08))),
        ..Default::default()
    }
}

/// Main favorites list view (dual pane).
pub fn favorites_view(
    entries: &[FavoriteEntry],
    indices: &[usize],
    focus_id: u32,
    theme: &Theme,
    active_page: &Page,
    editing_favorite_id: Option<u64>,
) -> Element<'static, Message> {
    const CLIPBOARD_ROW_H: f32 = 32.0;
    const WINDOW_ROWS: usize = 48;
    const LIST_TOP_GAP: f32 = 0.0;
    let sub_tabs_h: f32 = 34.0;
    let viewport_h = crate::app::CLIPBOARD_CONTENT_HEIGHT as f32;
    let list_viewport_h = viewport_h - sub_tabs_h;

    if indices.is_empty() {
        let msg = if entries.is_empty() {
            "No favorites yet — press \u{2318}S in History to add"
        } else {
            "No matches"
        };
        let sub_tabs = clipboard_sub_tabs(active_page, theme);
        return Column::new()
            .push(sub_tabs)
            .push(
                container(
                    Text::new(msg)
                        .size(13)
                        .color(theme.text_color(0.35))
                        .font(theme.font()),
                )
                .width(Length::Fill)
                .height(list_viewport_h)
                .center(Length::Fill),
            )
            .height(viewport_h)
            .width(Length::Fill)
            .into();
    }

    let left_width = (WINDOW_WIDTH * 0.40) as f32;

    // ── Left list ────────────────────────────────────────────────
    let mut list_col = Column::new()
        .padding([2, 4])
        .spacing(0)
        .width(Length::Fill)
        .push(space().height(LIST_TOP_GAP));

    let total = indices.len();
    let focus_idx = (focus_id as usize).min(total.saturating_sub(1));
    let mut start = focus_idx.saturating_sub(WINDOW_ROWS / 2);
    let end = (start + WINDOW_ROWS).min(total);
    if end - start < WINDOW_ROWS {
        start = end.saturating_sub(WINDOW_ROWS);
    }

    if start > 0 {
        list_col = list_col.push(space().height(start as f32 * CLIPBOARD_ROW_H));
    }

    for (display_idx, &entry_idx) in indices
        .iter()
        .enumerate()
        .skip(start)
        .take(end.saturating_sub(start))
    {
        if let Some(entry) = entries.get(entry_idx) {
            let focused = display_idx as u32 == focus_id;
            list_col = list_col.push(favorite_row(entry, display_idx as u32, focused, theme));
        }
    }

    if end < total {
        list_col = list_col.push(space().height((total - end) as f32 * CLIPBOARD_ROW_H));
    }

    let list_scrollable = Scrollable::with_direction(
        list_col,
        Direction::Vertical(
            Scrollbar::new()
                .width(4)
                .scroller_width(4)
                .anchor(Anchor::Start),
        ),
    )
    .id("results")
    .width(Length::Fill)
    .height(list_viewport_h);

    let sub_tabs = clipboard_sub_tabs(active_page, theme);
    let left_content = Column::new()
        .push(sub_tabs)
        .push(list_scrollable)
        .height(viewport_h)
        .width(Length::Fill);

    let left_pane = container(left_content)
        .width(left_width)
        .height(viewport_h)
        .clip(true);

    let separator = container(space().width(1).height(Length::Fill))
        .style(move |_| iced::widget::container::Style {
            background: Some(iced::Background::Color(with_alpha(Color::WHITE, 0.06))),
            ..Default::default()
        })
        .height(Length::Fill)
        .width(1);

    let focused_entry = indices.get(focus_id as usize).and_then(|&i| entries.get(i));
    let right_pane = favorites_preview(focused_entry, theme, viewport_h, editing_favorite_id);

    Row::new()
        .push(left_pane)
        .push(separator)
        .push(right_pane)
        .height(viewport_h)
        .width(Length::Fill)
        .into()
}

/// A single row in the left pane (fixed 32px height).
fn favorite_row(
    entry: &FavoriteEntry,
    display_idx: u32,
    focused: bool,
    theme: &Theme,
) -> Element<'static, Message> {
    let title_opacity = if focused { 1.0 } else { 0.85 };
    let time_opacity = if focused { 0.45 } else { 0.30 };

    let title = Text::new(entry.title.clone())
        .size(12)
        .color(theme.text_color(title_opacity))
        .font(theme.font())
        .wrapping(iced::widget::text::Wrapping::None);

    let time_str = format_relative_time(&entry.created_at);
    let time_text = Text::new(time_str)
        .size(10)
        .color(theme.text_color(time_opacity))
        .font(theme.font())
        .wrapping(iced::widget::text::Wrapping::None);

    let row = Row::new()
        .push(container(title).width(Length::Fill).clip(true))
        .push(space().width(8))
        .push(time_text)
        .spacing(4)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .height(32);

    let theme_for_btn = theme.clone();
    let content = Button::new(row)
        .on_press(Message::ClipboardOpenAt(display_idx))
        .style(move |_, status| result_button_style(&theme_for_btn, focused, status))
        .width(Length::Fill)
        .height(32)
        .padding(0);

    let theme_for_cont = theme.clone();
    let row_container = container(content)
        .padding([0, 4])
        .width(Length::Fill)
        .height(32)
        .clip(true)
        .style(move |_| result_row_container_style(&theme_for_cont, focused));

    mouse_area(row_container)
        .on_enter(Message::HoverResult(display_idx))
        .into()
}

/// Right pane preview for favorites.
fn favorites_preview(
    entry: Option<&FavoriteEntry>,
    theme: &Theme,
    height: f32,
    editing_favorite_id: Option<u64>,
) -> Element<'static, Message> {
    let theme_for_style = theme.clone();
    let panel = |content: Element<'static, Message>| -> Element<'static, Message> {
        container(content)
            .width(Length::Fill)
            .height(height)
            .style(move |_| clipboard_preview_style(&theme_for_style))
            .into()
    };

    match entry {
        None => panel(
            container(
                Text::new("Select an item to preview")
                    .size(13)
                    .color(theme.text_color(0.25))
                    .font(theme.font()),
            )
            .width(Length::Fill)
            .height(height)
            .center(Length::Fill)
            .into(),
        ),
        Some(entry) => {
            let editing_current = editing_favorite_id == Some(entry.id);

            let title = Text::new(entry.title.clone())
                .size(12)
                .color(theme.text_color(0.90))
                .font(theme.font())
                .wrapping(iced::widget::text::Wrapping::None);

            let time_text = Text::new(format_relative_time(&entry.created_at))
                .size(10)
                .color(theme.text_color(0.42))
                .font(theme.font());

            let title_meta = Column::new()
                .push(container(title).width(Length::Fill).clip(true))
                .push(time_text)
                .spacing(2)
                .width(Length::Fill);

            let actions = if editing_current {
                Row::new()
                    .push(preview_action_button(
                        "Save",
                        Message::ClipboardFavoriteCommitEdit,
                        true,
                        theme,
                    ))
                    .push(preview_action_button(
                        "Cancel",
                        Message::ClipboardFavoriteCancelEdit,
                        false,
                        theme,
                    ))
                    .spacing(6)
                    .align_y(Alignment::Center)
            } else {
                Row::new()
                    .push(preview_action_button(
                        "Edit note",
                        Message::ClipboardFavoriteStartEdit,
                        false,
                        theme,
                    ))
                    .spacing(6)
                    .align_y(Alignment::Center)
            };

            let header = container(
                Row::new()
                    .push(title_meta)
                    .push(actions)
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .width(Length::Fill),
            )
            .padding([9, 14])
            .width(Length::Fill);

            let content: Element<'static, Message> = match &entry.content {
                ClipBoardContentType::Text(text) => {
                    let preview_text = if text.len() > 5000 {
                        text.chars().take(5000).collect::<String>() + "..."
                    } else {
                        text.clone()
                    };

                    let content = Text::new(preview_text)
                        .size(13)
                        .color(theme.text_color(0.85))
                        .font(theme.font())
                        .wrapping(iced::widget::text::Wrapping::WordOrGlyph);

                    Scrollable::with_direction(
                        container(content).padding([8, 16]).width(Length::Fill),
                        Direction::Vertical(
                            Scrollbar::new()
                                .width(4)
                                .scroller_width(4)
                                .anchor(Anchor::Start),
                        ),
                    )
                    .height(Length::Fill)
                    .into()
                }
                ClipBoardContentType::Image(img) => {
                    let handle = iced::widget::image::Handle::from_rgba(
                        img.width as u32,
                        img.height as u32,
                        img.bytes.to_vec(),
                    );
                    let image_widget = iced::widget::image(handle)
                        .content_fit(iced::ContentFit::Contain)
                        .width(Length::Fill)
                        .height(Length::Fill);

                    container(image_widget)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .padding([8, 16])
                        .center(Length::Fill)
                        .into()
                }
            };

            let mut column = Column::new().push(header);
            if editing_current {
                let hint = Text::new("Editing note in top input (Enter to save, ESC to cancel)")
                    .size(10)
                    .color(theme.text_color(0.40))
                    .font(theme.font());
                column = column.push(container(hint).padding([4, 16]).width(Length::Fill));
            }
            column = column.push(content);

            panel(column.height(Length::Fill).width(Length::Fill).into())
        }
    }
}

fn preview_action_button(
    label: &str,
    on_press: Message,
    primary: bool,
    theme: &Theme,
) -> Element<'static, Message> {
    let theme_for_btn = theme.clone();
    Button::new(
        Text::new(label.to_string())
            .size(10)
            .color(theme.text_color(if primary { 0.95 } else { 0.72 }))
            .font(theme.font()),
    )
    .on_press(on_press)
    .style(move |_, status| preview_action_button_style(&theme_for_btn, primary, status))
    .padding([2, 8])
    .height(22)
    .into()
}

fn preview_action_button_style(
    theme: &Theme,
    primary: bool,
    status: iced::widget::button::Status,
) -> iced::widget::button::Style {
    let mut base_alpha = if primary { 0.18 } else { 0.10 };
    if matches!(status, iced::widget::button::Status::Hovered) {
        base_alpha += 0.06;
    }

    iced::widget::button::Style {
        background: Some(iced::Background::Color(with_alpha(
            Color::WHITE,
            base_alpha,
        ))),
        border: iced::Border {
            radius: 6.0.into(),
            width: if primary { 0.0 } else { 1.0 },
            color: if primary {
                Color::TRANSPARENT
            } else {
                with_alpha(theme.text_color(0.50), 0.25)
            },
        },
        text_color: theme.text_color(if primary { 0.95 } else { 0.72 }),
        ..Default::default()
    }
}

/// Favorites-specific footer with shortcuts.
pub fn favorites_footer(theme: &Theme, count: usize, editing: bool) -> Element<'static, Message> {
    if count == 0 && !editing {
        return space().into();
    }

    let count_text = if editing {
        "Editing title...".to_string()
    } else if count == 1 {
        "1 favorite".to_string()
    } else {
        format!("{} favorites", count)
    };

    let left = Text::new(count_text)
        .size(11)
        .color(theme.text_color(0.35))
        .font(theme.font());

    let theme_clone = theme.clone();

    let right = if editing {
        let enter_badge = shortcut_badge_icon(crate::icons::ARROW_RETURN_LEFT, "Save", theme);
        let esc_badge = shortcut_badge("ESC", "Cancel", theme);
        Row::new()
            .push(enter_badge)
            .push(esc_badge)
            .spacing(8)
            .align_y(Alignment::Center)
    } else {
        let edit_badge = shortcut_badge("\u{2318}E", "Edit", theme);
        let del_badge = shortcut_badge("\u{2318}D", "Delete", theme);
        let paste_badge = shortcut_badge_icon(crate::icons::ARROW_RETURN_LEFT, "Paste", theme);
        let esc_badge = shortcut_badge("ESC", "Back", theme);
        Row::new()
            .push(edit_badge)
            .push(del_badge)
            .push(paste_badge)
            .push(esc_badge)
            .spacing(8)
            .align_y(Alignment::Center)
    };

    let row = Row::new()
        .push(container(left).width(Length::Fill))
        .push(right)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .height(28)
        .padding([0, 18]);

    container(row)
        .width(Length::Fill)
        .padding([5, 0])
        .style(move |_| footer_style(&theme_clone))
        .into()
}

fn shortcut_badge_icon(icon_char: char, label: &str, theme: &Theme) -> Element<'static, Message> {
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

fn shortcut_badge(key: &str, label: &str, theme: &Theme) -> Element<'static, Message> {
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
