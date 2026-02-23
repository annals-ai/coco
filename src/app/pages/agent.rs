use iced::widget::{Column, Text, container, space};
use iced::{Alignment, Element, Length::Fill};

use crate::agent::types::AgentSession;
use crate::app::Message;
use crate::config::Theme;
use crate::styles::result_row_container_style;

/// Render the agent session list inside the launcher.
pub fn agent_list_view(
    sessions: &[AgentSession],
    focus_id: u32,
    theme: Theme,
) -> Element<'static, Message> {
    let mut col = Column::new().padding([2, 6]);

    // First row: new conversation
    let new_row = agent_row("+ New conversation", "Start a new chat", 0, focus_id, &theme);
    col = col.push(new_row);

    // Session rows
    for (i, session) in sessions.iter().enumerate() {
        let idx = (i + 1) as u32;
        let row = agent_row(&session.title, &relative_time(session.last_active), idx, focus_id, &theme);
        col = col.push(row);
    }

    container(col).into()
}

fn agent_row<'a>(
    title: &str,
    subtitle: &str,
    id: u32,
    focus_id: u32,
    theme: &Theme,
) -> Element<'a, Message> {
    let focused = id == focus_id;
    let title_opacity = if focused { 1.0 } else { 0.88 };
    let desc_opacity = if focused { 0.50 } else { 0.38 };

    let text_block = Column::new()
        .spacing(1)
        .push(
            Text::new(title.to_string())
                .font(theme.font())
                .size(14)
                .color(theme.text_color(title_opacity)),
        )
        .push(
            Text::new(subtitle.to_string())
                .font(theme.font())
                .size(11)
                .color(theme.text_color(desc_opacity)),
        );

    let row = iced::widget::Row::new()
        .align_y(Alignment::Center)
        .width(Fill)
        .spacing(12)
        .height(44)
        .push(container(text_block).width(Fill));

    let theme_clone = theme.clone();
    container(row)
        .style(move |_| result_row_container_style(&theme_clone, focused))
        .padding([4, 8])
        .width(Fill)
        .into()
}

fn relative_time(unix_ts: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let diff = now.saturating_sub(unix_ts);
    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}
