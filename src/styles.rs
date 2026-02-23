use iced::border::Radius;
use iced::widget::{button, container};
use iced::{Background, Border, Color, Shadow, widget::text_input};

use crate::config::Theme as ConfigTheme;

/// Helper: mix base color with white (simple "tint")
pub fn tint(mut c: Color, amount: f32) -> Color {
    c.r = c.r + (1.0 - c.r) * amount;
    c.g = c.g + (1.0 - c.g) * amount;
    c.b = c.b + (1.0 - c.b) * amount;
    c
}

/// Helper: apply alpha
pub fn with_alpha(mut c: Color, a: f32) -> Color {
    c.a = a;
    c
}

/// Helper: blend two colors by a factor (0.0 = all a, 1.0 = all b)
#[allow(dead_code)]
pub fn blend(a: Color, b: Color, factor: f32) -> Color {
    Color {
        r: a.r + (b.r - a.r) * factor,
        g: a.g + (b.g - a.g) * factor,
        b: a.b + (b.b - a.b) * factor,
        a: a.a + (b.a - a.a) * factor,
    }
}

// ── Search input ──────────────────────────────────────────────────────────

pub fn rustcast_text_input_style(
    theme: &ConfigTheme,
    _round_bottom_edges: bool,
) -> text_input::Style {
    let accent = theme.accent_color();
    text_input::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.,
            radius: Radius::new(0.),
        },
        icon: theme.text_color(0.5),
        placeholder: theme.text_color(0.30),
        value: theme.text_color(0.95),
        selection: with_alpha(accent, 0.30),
    }
}

// ── Main outer container ──────────────────────────────────────────────────
// Near-transparent so macOS NSVisualEffectView shows through.
// Thin inner glow border + subtle outer shadow create depth.

pub fn contents_style(_theme: &ConfigTheme) -> container::Style {
    container::Style {
        // Very low alpha — the real blur comes from NSVisualEffectView behind
        background: Some(Background::Color(Color {
            r: 0.06,
            g: 0.06,
            b: 0.08,
            a: 0.35,
        })),
        text_color: None,
        border: Border {
            color: with_alpha(Color::WHITE, 0.12),
            width: 0.5,
            radius: Radius::new(14.0),
        },
        shadow: Shadow::default(),
        snap: false,
    }
}

// ── Result row ────────────────────────────────────────────────────────────

pub fn result_button_style(theme: &ConfigTheme) -> button::Style {
    button::Style {
        text_color: theme.text_color(0.95),
        background: Some(Background::Color(Color::TRANSPARENT)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.,
            radius: Radius::new(8.),
        },
        ..Default::default()
    }
}

pub fn result_row_container_style(theme: &ConfigTheme, focused: bool) -> container::Style {
    if focused {
        let accent = theme.accent_color();
        container::Style {
            background: Some(Background::Color(with_alpha(accent, 0.18))),
            border: Border {
                color: with_alpha(accent, 0.28),
                width: 0.5,
                radius: Radius::new(8.),
            },
            ..Default::default()
        }
    } else {
        container::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.,
                radius: Radius::new(8.),
            },
            ..Default::default()
        }
    }
}

// ── Emoji ─────────────────────────────────────────────────────────────────

pub fn emoji_button_container_style(theme: &ConfigTheme, focused: bool) -> container::Style {
    if focused {
        let accent = theme.accent_color();
        container::Style {
            background: Some(Background::Color(with_alpha(accent, 0.18))),
            text_color: Some(theme.text_color(1.)),
            border: Border {
                color: with_alpha(accent, 0.28),
                width: 0.5,
                radius: Radius::new(8.),
            },
            ..Default::default()
        }
    } else {
        container::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            text_color: Some(theme.text_color(1.)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.,
                radius: Radius::new(8.),
            },
            ..Default::default()
        }
    }
}

pub fn emoji_button_style(theme: &ConfigTheme) -> button::Style {
    button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: theme.text_color(1.),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.,
            radius: Radius::new(8.),
        },
        ..Default::default()
    }
}

// ── Separator ─────────────────────────────────────────────────────────────

pub fn separator_style(_theme: &ConfigTheme) -> container::Style {
    container::Style {
        background: Some(Background::Color(with_alpha(Color::WHITE, 0.06))),
        ..Default::default()
    }
}

// ── Footer ────────────────────────────────────────────────────────────────

pub fn footer_style(_theme: &ConfigTheme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.15,
        })),
        border: Border {
            color: with_alpha(Color::WHITE, 0.04),
            width: 0.,
            radius: Radius::new(0.).bottom(14.),
        },
        ..Default::default()
    }
}

pub fn footer_shortcut_badge_style(_theme: &ConfigTheme) -> container::Style {
    container::Style {
        background: Some(Background::Color(with_alpha(Color::WHITE, 0.08))),
        border: Border {
            color: with_alpha(Color::WHITE, 0.10),
            width: 0.5,
            radius: Radius::new(4.),
        },
        ..Default::default()
    }
}
