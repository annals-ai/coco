//! Raycast-style dual-pane clipboard history view.

use iced::widget::scrollable::Anchor;
use iced::widget::{
    Column, Row, Scrollable, Text, container,
    scrollable::{Direction, Scrollbar},
    space,
};
use iced::{Alignment, Color, Element, Length};

use crate::app::{Message, WINDOW_WIDTH};
use crate::clipboard::ClipBoardContentType;
use crate::clipboard_store::{ClipboardEntry, format_relative_time};
use crate::config::Theme;
use crate::styles::{
    clipboard_preview_style, footer_shortcut_badge_style, footer_style, result_row_container_style,
    with_alpha,
};

/// Main dual-pane clipboard view.
pub fn clipboard_view(
    entries: &[ClipboardEntry],
    indices: &[usize],
    focus_id: u32,
    theme: &Theme,
) -> Element<'static, Message> {
    if indices.is_empty() {
        let msg = if entries.is_empty() {
            "No clipboard history"
        } else {
            "No matches"
        };
        return container(
            Text::new(msg)
                .size(14)
                .color(theme.text_color(0.35))
                .font(theme.font()),
        )
        .width(Length::Fill)
        .height(120)
        .center(Length::Fill)
        .into();
    }

    let left_width = (WINDOW_WIDTH * 0.40) as f32;

    // ── Left pane: list ──────────────────────────────────────────
    let mut list_col = Column::new().padding([2, 4]).spacing(0);

    for (display_idx, &entry_idx) in indices.iter().enumerate() {
        if let Some(entry) = entries.get(entry_idx) {
            let focused = display_idx as u32 == focus_id;
            list_col = list_col.push(clipboard_row(entry, focused, theme));
        }
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
    .height(360);

    let left_pane = container(list_scrollable).width(left_width);

    // ── Vertical separator ───────────────────────────────────────
    let separator = container(space().width(1).height(Length::Fill))
        .style(move |_| iced::widget::container::Style {
            background: Some(iced::Background::Color(with_alpha(Color::WHITE, 0.06))),
            ..Default::default()
        })
        .height(Length::Fill)
        .width(1);

    // ── Right pane: preview ──────────────────────────────────────
    let focused_entry = indices.get(focus_id as usize).and_then(|&i| entries.get(i));
    let right_pane = clipboard_preview(focused_entry, theme);

    let row = Row::new()
        .push(left_pane)
        .push(separator)
        .push(right_pane)
        .height(360)
        .width(Length::Fill);

    container(row).height(360).into()
}

/// A single row in the left pane (fixed 32px height, clipped).
fn clipboard_row(
    entry: &ClipboardEntry,
    focused: bool,
    theme: &Theme,
) -> Element<'static, Message> {
    let title_opacity = if focused { 1.0 } else { 0.85 };
    let time_opacity = if focused { 0.45 } else { 0.30 };

    // Pin indicator (fixed width for alignment)
    let pin_el: Element<'static, Message> = if entry.pinned {
        container(crate::icons::icon(
            crate::icons::PIN_FILL,
            9.0,
            theme.text_color(0.55),
        ))
        .width(12)
        .center_y(Length::Fill)
        .into()
    } else {
        space().width(12).into()
    };

    // Type icon (fixed width)
    let type_icon = match &entry.content {
        ClipBoardContentType::Text(_) => crate::icons::CLIPBOARD,
        ClipBoardContentType::Image(_) => crate::icons::IMAGE,
    };
    let type_el = container(crate::icons::icon(type_icon, 10.0, theme.text_color(0.30)))
        .width(14)
        .center_y(Length::Fill);

    // Title — single line, hard-clipped by outer container
    let title = Text::new(entry.preview_title.clone())
        .size(12)
        .color(theme.text_color(title_opacity))
        .font(theme.font())
        .wrapping(iced::widget::text::Wrapping::None);

    // Relative time (fixed width so it doesn't get pushed out)
    let time_str = format_relative_time(&entry.created_at);
    let time_text = Text::new(time_str)
        .size(10)
        .color(theme.text_color(time_opacity))
        .font(theme.font())
        .wrapping(iced::widget::text::Wrapping::None);

    let row = Row::new()
        .push(pin_el)
        .push(type_el)
        .push(container(title).width(Length::Fill).clip(true))
        .push(time_text)
        .spacing(4)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .height(32);

    let theme_for_cont = theme.clone();
    container(row)
        .padding([0, 6])
        .width(Length::Fill)
        .height(32)
        .clip(true)
        .style(move |_| result_row_container_style(&theme_for_cont, focused))
        .into()
}

/// Right pane preview.
fn clipboard_preview(entry: Option<&ClipboardEntry>, theme: &Theme) -> Element<'static, Message> {
    let theme_for_style = theme.clone();

    match entry {
        None => container(
            Text::new("Select an item to preview")
                .size(13)
                .color(theme.text_color(0.25))
                .font(theme.font()),
        )
        .width(Length::Fill)
        .height(360)
        .center(Length::Fill)
        .style(move |_| clipboard_preview_style(&theme_for_style))
        .into(),

        Some(entry) => match &entry.content {
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

                let scrollable = Scrollable::with_direction(
                    container(content).padding(16).width(Length::Fill),
                    Direction::Vertical(
                        Scrollbar::new()
                            .width(4)
                            .scroller_width(4)
                            .anchor(Anchor::Start),
                    ),
                )
                .height(360);

                container(scrollable)
                    .width(Length::Fill)
                    .height(360)
                    .style(move |_| clipboard_preview_style(&theme_for_style))
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
                    .height(360)
                    .padding(16)
                    .center(Length::Fill)
                    .style(move |_| clipboard_preview_style(&theme_for_style))
                    .into()
            }
        },
    }
}

/// Clipboard-specific footer with shortcuts.
pub fn clipboard_footer(theme: &Theme, count: usize) -> Element<'static, Message> {
    if count == 0 {
        return space().into();
    }

    let count_text = if count == 1 {
        "1 item".to_string()
    } else {
        format!("{} items", count)
    };

    let left = Text::new(count_text)
        .size(11)
        .color(theme.text_color(0.35))
        .font(theme.font());

    let theme_clone = theme.clone();

    let copy_badge = shortcut_badge_icon(crate::icons::ARROW_RETURN_LEFT, "Copy", theme);
    let pin_badge = shortcut_badge("\u{2318}P", "Pin", theme);
    let del_badge = shortcut_badge("\u{2318}D", "Delete", theme);
    let esc_badge = shortcut_badge("ESC", "Back", theme);

    let right = Row::new()
        .push(copy_badge)
        .push(pin_badge)
        .push(del_badge)
        .push(esc_badge)
        .spacing(8)
        .align_y(Alignment::Center);

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
