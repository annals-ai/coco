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

pub fn coco_text_input_style(theme: &ConfigTheme, _round_bottom_edges: bool) -> text_input::Style {
    text_input::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.,
            radius: Radius::new(0.),
        },
        icon: theme.text_color(0.56),
        placeholder: theme.text_color(0.58),
        value: theme.text_color(0.97),
        selection: with_alpha(Color::WHITE, 0.14),
    }
}

pub fn mode_switch_container_style(_theme: &ConfigTheme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: Radius::new(0.0),
        },
        ..Default::default()
    }
}

pub fn mode_switch_button_style(_theme: &ConfigTheme, active: bool) -> button::Style {
    let _ = active;
    button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: with_alpha(Color::WHITE, 0.9),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: Radius::new(0.0),
        },
        ..Default::default()
    }
}

// ── Main outer container ──────────────────────────────────────────────────
// Near-transparent so macOS NSVisualEffectView shows through.
// Thin inner glow border + subtle outer shadow create depth.

pub fn contents_style(_theme: &ConfigTheme) -> container::Style {
    container::Style {
        // The main content layer owns the "black glass" tone. Native macOS
        // child window behind it provides blur only.
        background: Some(Background::Color(Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.48,
        })),
        text_color: None,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: Radius::new(22.0),
        },
        shadow: Shadow {
            color: Color::TRANSPARENT,
            ..Default::default()
        },
        snap: false,
    }
}

// ── Result row ────────────────────────────────────────────────────────────

pub fn result_button_style(
    theme: &ConfigTheme,
    focused: bool,
    status: button::Status,
) -> button::Style {
    let mut style = button::Style {
        text_color: theme.text_color(0.95),
        background: Some(Background::Color(Color::TRANSPARENT)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: Radius::new(8.0),
        },
        ..Default::default()
    };

    match status {
        button::Status::Hovered if !focused => {
            style.background = Some(Background::Color(with_alpha(Color::WHITE, 0.035)));
            style.border.color = with_alpha(Color::WHITE, 0.08);
            style.border.width = 0.5;
        }
        button::Status::Pressed if !focused => {
            style.background = Some(Background::Color(with_alpha(Color::WHITE, 0.065)));
            style.border.color = with_alpha(Color::WHITE, 0.12);
            style.border.width = 0.5;
        }
        button::Status::Hovered | button::Status::Pressed if focused => {
            style.background = Some(Background::Color(Color::TRANSPARENT));
            style.border.color = Color::TRANSPARENT;
            style.border.width = 0.0;
        }
        button::Status::Hovered | button::Status::Pressed => {}
        button::Status::Disabled => {
            style.text_color = theme.text_color(0.45);
        }
        button::Status::Active => {}
    }

    style
}

pub fn result_row_container_style(_theme: &ConfigTheme, focused: bool) -> container::Style {
    if focused {
        container::Style {
            background: Some(Background::Color(Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 0.085,
            })),
            border: Border {
                color: with_alpha(Color::WHITE, 0.10),
                width: 0.5,
                radius: Radius::new(12.0),
            },
            ..Default::default()
        }
    } else {
        container::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.,
                radius: Radius::new(12.0),
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
            background: Some(Background::Color(with_alpha(accent, 0.12))),
            text_color: Some(theme.text_color(1.)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.,
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
        background: Some(Background::Color(with_alpha(Color::WHITE, 0.17))),
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
            a: 0.04,
        })),
        border: Border {
            color: with_alpha(Color::WHITE, 0.03),
            width: 0.,
            radius: Radius::new(0.).bottom(22.),
        },
        ..Default::default()
    }
}

pub fn footer_shortcut_badge_style(_theme: &ConfigTheme) -> container::Style {
    container::Style {
        background: Some(Background::Color(with_alpha(Color::WHITE, 0.035))),
        border: Border {
            color: with_alpha(Color::WHITE, 0.05),
            width: 0.5,
            radius: Radius::new(5.0),
        },
        ..Default::default()
    }
}

// ── Permission banner ─────────────────────────────────────────────────────

pub fn permission_banner_style(_theme: &ConfigTheme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            r: 0.85,
            g: 0.55,
            b: 0.10,
            a: 0.15,
        })),
        border: Border {
            color: Color {
                r: 0.85,
                g: 0.55,
                b: 0.10,
                a: 0.25,
            },
            width: 0.5,
            radius: Radius::new(6.),
        },
        ..Default::default()
    }
}

pub fn permission_banner_button_style(_theme: &ConfigTheme) -> button::Style {
    button::Style {
        text_color: Color {
            r: 1.0,
            g: 0.78,
            b: 0.30,
            a: 0.95,
        },
        background: Some(Background::Color(Color::TRANSPARENT)),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.,
            radius: Radius::new(4.),
        },
        ..Default::default()
    }
}

// ── Section header (zero-query state) ────────────────────────────────────

pub fn section_header_style(_theme: &ConfigTheme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        ..Default::default()
    }
}

/// Green dot color for running apps
pub fn running_dot_color() -> Color {
    Color {
        r: 0.30,
        g: 0.85,
        b: 0.40,
        a: 0.90,
    }
}

pub fn action_row_style(_theme: &ConfigTheme, focused: bool) -> container::Style {
    if focused {
        container::Style {
            background: Some(Background::Color(Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 0.085,
            })),
            border: Border {
                color: with_alpha(Color::WHITE, 0.10),
                width: 0.5,
                radius: Radius::new(8.0),
            },
            ..Default::default()
        }
    } else {
        container::Style {
            background: Some(Background::Color(Color::TRANSPARENT)),
            border: Border {
                color: Color::TRANSPARENT,
                width: 0.,
                radius: Radius::new(8.0),
            },
            ..Default::default()
        }
    }
}

pub fn action_separator_style() -> container::Style {
    container::Style {
        background: Some(Background::Color(with_alpha(Color::WHITE, 0.03))),
        ..Default::default()
    }
}

pub fn destructive_text_color() -> Color {
    Color {
        r: 0.95,
        g: 0.30,
        b: 0.25,
        a: 0.95,
    }
}

// ── Clipboard preview panel ──────────────────────────────────────────────

#[allow(dead_code)]
pub fn clipboard_preview_style(_theme: &ConfigTheme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.08,
        })),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.,
            radius: Radius::new(0.),
        },
        ..Default::default()
    }
}

#[allow(dead_code)]
pub fn clipboard_preview_popover_style(_theme: &ConfigTheme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            r: 0.07,
            g: 0.07,
            b: 0.08,
            a: 0.90,
        })),
        border: Border {
            color: with_alpha(Color::WHITE, 0.18),
            width: 0.9,
            radius: Radius::new(16.0),
        },
        shadow: Shadow {
            color: with_alpha(Color::BLACK, 0.40),
            offset: iced::Vector::new(0.0, 14.0),
            blur_radius: 32.0,
        },
        snap: false,
        ..Default::default()
    }
}

// ── Agent window styles ──────────────────────────────────────────────────

pub fn agent_content_style(_theme: &ConfigTheme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            r: 0.06,
            g: 0.06,
            b: 0.08,
            a: 0.45,
        })),
        text_color: None,
        border: Border {
            color: with_alpha(Color::WHITE, 0.12),
            width: 0.5,
            radius: Radius::new(12.0),
        },
        shadow: Shadow::default(),
        snap: false,
    }
}

pub fn user_bubble_style(theme: &ConfigTheme) -> container::Style {
    let accent = theme.accent_color();
    container::Style {
        background: Some(Background::Color(with_alpha(accent, 0.25))),
        border: Border {
            color: with_alpha(accent, 0.35),
            width: 0.5,
            radius: Radius::new(12.0),
        },
        ..Default::default()
    }
}

pub fn agent_title_bar_style(_theme: &ConfigTheme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 0.20,
        })),
        border: Border {
            color: with_alpha(Color::WHITE, 0.04),
            width: 0.,
            radius: Radius::new(12.0).bottom(0.),
        },
        ..Default::default()
    }
}

pub fn agent_input_bar_style(_theme: &ConfigTheme) -> container::Style {
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
            radius: Radius::new(0.).bottom(12.),
        },
        ..Default::default()
    }
}
