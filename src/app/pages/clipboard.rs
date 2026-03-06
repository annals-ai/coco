//! Clipboard history list view.

use iced::widget::scrollable::Anchor;
use iced::widget::{
    Button, Column, Row, Scrollable, Text, container, mouse_area,
    scrollable::{Direction, Scrollbar},
    space,
};
use iced::{Alignment, Color, Element, Length};
use std::path::{Path, PathBuf};

use crate::app::pages::favorites::clipboard_sub_tabs;
use crate::app::{Message, Page, WINDOW_WIDTH};
use crate::clipboard::ClipBoardContentType;
use crate::clipboard_store::{ClipboardEntry, format_relative_time};
use crate::config::Theme;
use crate::styles::{
    clipboard_preview_style, footer_shortcut_badge_style, footer_style, result_button_style,
    result_row_container_style, with_alpha,
};

/// Main clipboard list view.
pub fn clipboard_view(
    entries: &[ClipboardEntry],
    indices: &[usize],
    focus_id: u32,
    theme: &Theme,
    active_page: &Page,
) -> Element<'static, Message> {
    const CLIPBOARD_ROW_H: f32 = 32.0;
    const WINDOW_ROWS: usize = 48;
    const LIST_TOP_GAP: f32 = 0.0;
    let sub_tabs_h: f32 = 34.0;
    let viewport_h = crate::app::CLIPBOARD_CONTENT_HEIGHT as f32;
    let list_viewport_h = viewport_h - sub_tabs_h;

    if indices.is_empty() {
        let msg = if entries.is_empty() {
            "No clipboard history"
        } else {
            "No matches"
        };
        let sub_tabs = clipboard_sub_tabs(active_page, theme);
        return Column::new()
            .push(sub_tabs)
            .push(
                container(
                    Text::new(msg)
                        .size(14)
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
            list_col = list_col.push(clipboard_row(entry, display_idx as u32, focused, theme));
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
    let right_pane = clipboard_preview(focused_entry, theme, viewport_h);

    Row::new()
        .push(left_pane)
        .push(separator)
        .push(right_pane)
        .height(viewport_h)
        .width(Length::Fill)
        .into()
}

fn local_media_path(text: &str) -> Option<PathBuf> {
    let raw = text.lines().find(|line| !line.trim().is_empty())?.trim();
    if raw.is_empty() {
        return None;
    }

    let dequoted = if let Some(rest) = raw.strip_prefix("file://") {
        if let Some(localhost_path) = rest.strip_prefix("localhost/") {
            format!("/{}", localhost_path)
        } else {
            rest.to_string()
        }
    } else {
        raw.to_string()
    };
    let dequoted = dequoted.trim_matches('"');
    let path = Path::new(dequoted);
    if path.exists() {
        Some(path.to_path_buf())
    } else {
        None
    }
}

fn media_kind(path: &Path) -> Option<&'static str> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())?;
    match ext.as_str() {
        "mp4" | "mov" | "m4v" | "webm" | "mkv" | "avi" | "flv" => Some("Video"),
        "mp3" | "m4a" | "wav" | "aac" | "flac" | "ogg" => Some("Audio"),
        _ => None,
    }
}

fn clipboard_preview(
    entry: Option<&ClipboardEntry>,
    theme: &Theme,
    height: f32,
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
        Some(entry) => match &entry.content {
            ClipBoardContentType::Text(text) => {
                if let Some(path) = local_media_path(text)
                    && let Some(kind) = media_kind(&path)
                {
                    let body = Column::new()
                        .push(
                            Text::new(format!("{kind} File"))
                                .size(15)
                                .color(theme.text_color(0.85))
                                .font(theme.font()),
                        )
                        .push(
                            Text::new(path.to_string_lossy().to_string())
                                .size(12)
                                .color(theme.text_color(0.60))
                                .font(theme.font()),
                        )
                        .push(
                            Text::new("Press SPACE for native preview playback")
                                .size(11)
                                .color(theme.text_color(0.45))
                                .font(theme.font()),
                        )
                        .spacing(10)
                        .padding(16);
                    return panel(container(body).width(Length::Fill).into());
                }

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
                .height(height);

                panel(scrollable.into())
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

                panel(
                    container(image_widget)
                        .width(Length::Fill)
                        .height(height)
                        .padding(16)
                        .center(Length::Fill)
                        .into(),
                )
            }
        },
    }
}

/// A single row in the left pane (fixed 32px height, clipped).
fn clipboard_row(
    entry: &ClipboardEntry,
    display_idx: u32,
    focused: bool,
    theme: &Theme,
) -> Element<'static, Message> {
    let title_opacity = if focused { 1.0 } else { 0.85 };
    let time_opacity = if focused { 0.45 } else { 0.30 };

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

    let fav_badge = shortcut_badge("\u{2318}S", "Fav", theme);
    let paste_badge = shortcut_badge_icon(crate::icons::ARROW_RETURN_LEFT, "Paste", theme);
    let esc_badge = shortcut_badge("ESC", "Back", theme);

    let right = Row::new()
        .push(fav_badge)
        .push(paste_badge)
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
