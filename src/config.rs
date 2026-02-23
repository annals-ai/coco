//! This is the config file type definitions for rustcast
use std::{path::Path, sync::Arc};

use iced::{Font, font::Family, theme::Custom, widget::image::Handle};
use serde::{Deserialize, Serialize};

use crate::{
    app::apps::{App, AppCommand},
    commands::Function,
    utils::handle_from_icns,
};

/// The main config struct (effectively the config file's "schema")
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub toggle_hotkey: String,
    pub clipboard_hotkey: Option<String>,
    pub buffer_rules: Buffer,
    pub theme: Theme,
    pub placeholder: String,
    pub search_url: String,
    pub haptic_feedback: bool,
    pub show_trayicon: bool,
    pub shells: Vec<Shelly>,
}

impl Default for Config {
    /// The default config
    fn default() -> Self {
        Self {
            toggle_hotkey: "ALT+SPACE".to_string(),
            clipboard_hotkey: None,
            buffer_rules: Buffer::default(),
            theme: Theme::default(),
            placeholder: String::from("Time to be productive!"),
            search_url: "https://google.com/search?q=%s".to_string(),
            haptic_feedback: false,
            show_trayicon: true,
            shells: vec![],
        }
    }
}

/// The settings you can set for the theme
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Theme {
    pub text_color: (f32, f32, f32),
    pub background_color: (f32, f32, f32),
    pub accent_color: (f32, f32, f32),
    pub blur: bool,
    pub show_icons: bool,
    pub show_scroll_bar: bool,
    pub show_footer_hints: bool,
    pub font: Option<String>,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            text_color: (0.93, 0.93, 0.95),
            background_color: (0.10, 0.10, 0.11),
            accent_color: (0.30, 0.42, 0.85),
            blur: true,
            show_icons: true,
            show_scroll_bar: false,
            show_footer_hints: true,
            font: None,
        }
    }
}

impl From<Theme> for iced::Theme {
    fn from(value: Theme) -> Self {
        let palette = iced::theme::Palette {
            background: value.bg_color(),
            text: value.text_color(1.),
            primary: value.accent_color(),
            danger: iced::Color {
                r: 0.95,
                g: 0.26,
                b: 0.21,
                a: 1.0,
            },
            warning: iced::Color {
                r: 1.0,
                g: 0.76,
                b: 0.03,
                a: 1.0,
            },
            success: iced::Color {
                r: 0.30,
                g: 0.69,
                b: 0.31,
                a: 1.0,
            },
        };
        iced::Theme::Custom(Arc::new(Custom::new("RustCast Theme".to_string(), palette)))
    }
}

impl Theme {
    /// Return the accent color in the theme config of type [`iced::Color`]
    pub fn accent_color(&self) -> iced::Color {
        iced::Color {
            r: self.accent_color.0,
            g: self.accent_color.1,
            b: self.accent_color.2,
            a: 1.0,
        }
    }

    /// Return the text color in the theme config of type [`iced::Color`]
    pub fn text_color(&self, opacity: f32) -> iced::Color {
        let theme = self.to_owned();
        iced::Color {
            r: theme.text_color.0,
            g: theme.text_color.1,
            b: theme.text_color.2,
            a: opacity,
        }
    }

    /// Return the background color in the theme config of type [`iced::Color`]
    /// Returns fully transparent (0,0,0,0) for the iced palette clear color,
    /// so the window background is invisible and blur/desktop shows through.
    pub fn bg_color(&self) -> iced::Color {
        iced::Color::TRANSPARENT
    }

    /// Return the font in the theme config of type [`iced::Font`]
    pub fn font(&self) -> Font {
        let opt_font_name = self.font.clone();
        match opt_font_name {
            Some(font_name) => Font {
                family: Family::Name(font_name.leak()),
                ..Default::default()
            },
            None => Font {
                family: Family::SansSerif,
                ..Default::default()
            },
        }
    }
}

/// The rules for the buffer AKA search results
///
/// - clear_on_hide is whether the buffer should be cleared when the window is hidden
/// - clear_on_enter is whether the buffer should be cleared when the user presses enter after
///   searching
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Buffer {
    pub clear_on_hide: bool,
    pub clear_on_enter: bool,
}

impl Default for Buffer {
    fn default() -> Self {
        Buffer {
            clear_on_hide: true,
            clear_on_enter: true,
        }
    }
}

/// Command is the command it will run when the button is clicked
/// Icon_path is the path to an icon, but this is optional
/// Alias is the text that is used to call this command / search for it
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Shelly {
    command: String,
    icon_path: Option<String>,
    alias: String,
    alias_lc: String,
}

impl Shelly {
    /// Converts the shelly struct to an app so that it can be added to the app list
    pub fn to_app(&self) -> App {
        let self_clone = self.clone();
        let icon = self_clone.icon_path.and_then(|x| {
            let x = x.replace("~", &std::env::var("HOME").unwrap());
            if x.ends_with(".icns") {
                handle_from_icns(Path::new(&x))
            } else {
                Some(Handle::from_path(Path::new(&x)))
            }
        });
        App {
            open_command: AppCommand::Function(Function::RunShellCommand(
                self_clone.command,
                self_clone.alias_lc.clone(),
            )),
            desc: "Shell Command".to_string(),
            icons: icon,
            name: self_clone.alias,
            name_lc: self_clone.alias_lc,
            localized_name: None,
        }
    }
}
