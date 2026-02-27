//! This modules handles the logic for each "app" that Coco can load
//!
//! An "app" is effectively, one of the results that Coco returns when you search for something
use std::path::Path;

use iced::{
    Alignment,
    Length::Fill,
    font::Weight,
    widget::{Button, Row, Text, container, image, mouse_area, text::Wrapping},
};

use crate::{
    app::{
        COCO_DESC_NAME, Message, Page, RESULT_ICON_SIZE, RESULT_ICON_SLOT, RESULT_ROW_CONTENT_GAP,
        RESULT_ROW_CONTENT_HEIGHT, RESULT_ROW_PADDING_X, RESULT_ROW_PADDING_Y,
    },
    clipboard::ClipBoardContentType,
    commands::Function,
    styles::{result_button_style, result_row_container_style},
};

/// This tells each "App" what to do when it is clicked, whether it is a function, a message, or a display
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum AppCommand {
    Function(Function),
    Message(Message),
    Display,
}

/// Category for grouping apps in the zero-query state
#[derive(Debug, Clone, PartialEq)]
pub enum AppCategory {
    Running,
    Recent,
}

/// The main app struct, that represents an "App"
///
/// This struct represents a command that Coco can perform, providing Coco
/// the data needed to search for the app, to display the app in search results, and to actually
/// "run" the app.
#[derive(Debug, Clone)]
pub struct App {
    pub open_command: AppCommand,
    pub desc: String,
    pub icons: Option<iced::widget::image::Handle>,
    pub name: String,
    pub name_lc: String,
    /// Optional localized name (e.g. Chinese name from zh-Hans.lproj)
    pub localized_name: Option<String>,
    /// Category for zero-query state grouping
    pub category: Option<AppCategory>,
    /// Bundle path (used for actions)
    pub bundle_path: Option<String>,
    /// Bundle identifier (used for actions)
    pub bundle_id: Option<String>,
    /// PID of the running process (if running)
    pub pid: Option<i32>,
}

impl PartialEq for App {
    fn eq(&self, other: &Self) -> bool {
        self.name_lc == other.name_lc
            && self.icons == other.icons
            && self.desc == other.desc
            && self.name == other.name
    }
}

impl App {
    /// Helper to create an App with default None fields for category/bundle_path/bundle_id/pid
    pub fn simple(
        open_command: AppCommand,
        desc: String,
        icons: Option<iced::widget::image::Handle>,
        name: String,
        name_lc: String,
        localized_name: Option<String>,
    ) -> Self {
        Self {
            open_command,
            desc,
            icons,
            name,
            name_lc,
            localized_name,
            category: None,
            bundle_path: None,
            bundle_id: None,
            pid: None,
        }
    }
}

impl App {
    /// A vec of all the emojis as App structs
    pub fn emoji_apps() -> Vec<App> {
        emojis::iter()
            .filter(|x| x.unicode_version() < emojis::UnicodeVersion::new(17, 13))
            .map(|x| {
                App::simple(
                    AppCommand::Function(Function::CopyToClipboard(ClipBoardContentType::Text(
                        x.to_string(),
                    ))),
                    x.name().to_string(),
                    None,
                    x.to_string(),
                    x.name().to_string(),
                    None,
                )
            })
            .collect()
    }
    /// This returns the basic apps that Coco has, such as quiting Coco and opening preferences
    pub fn basic_apps() -> Vec<App> {
        let app_version = option_env!("APP_VERSION").unwrap_or("Unknown Version");
        let icon = || {
            Some(iced::widget::image::Handle::from_path(Path::new(
                "/Applications/Coco.app/Contents/Resources/coco_list_icon.png",
            )))
        };

        vec![
            App::simple(
                AppCommand::Function(Function::Quit),
                COCO_DESC_NAME.to_string(),
                icon(),
                "Quit Coco".to_string(),
                "quit".to_string(),
                None,
            ),
            App::simple(
                AppCommand::Function(Function::OpenPrefPane),
                COCO_DESC_NAME.to_string(),
                icon(),
                "Open Coco Preferences".to_string(),
                "settings".to_string(),
                None,
            ),
            App::simple(
                AppCommand::Message(Message::SwitchToPage(Page::EmojiSearch)),
                COCO_DESC_NAME.to_string(),
                icon(),
                "Search for an Emoji".to_string(),
                "emoji".to_string(),
                None,
            ),
            App::simple(
                AppCommand::Message(Message::SwitchToPage(Page::ClipboardHistory)),
                COCO_DESC_NAME.to_string(),
                icon(),
                "Clipboard History".to_string(),
                "clipboard".to_string(),
                None,
            ),
            App::simple(
                AppCommand::Message(Message::ReloadConfig),
                COCO_DESC_NAME.to_string(),
                icon(),
                "Reload Coco".to_string(),
                "refresh".to_string(),
                None,
            ),
            App::simple(
                AppCommand::Display,
                COCO_DESC_NAME.to_string(),
                icon(),
                format!("Current Coco Version: {app_version}"),
                "version".to_string(),
                None,
            ),
            App::simple(
                AppCommand::Function(Function::OpenTerminal),
                COCO_DESC_NAME.to_string(),
                icon(),
                "Open Terminal".to_string(),
                "terminal agent".to_string(),
                None,
            ),
        ]
    }

    /// This renders the app into an iced element, allowing it to be displayed in the search results
    pub fn render(
        self,
        theme: crate::config::Theme,
        id_num: u32,
        focussed_id: u32,
    ) -> iced::Element<'static, Message> {
        let focused = focussed_id == id_num;
        let is_currency_result = self.name_lc.starts_with("__currency__|");
        let is_calculator_result = self.name_lc.starts_with("__calc__|")
            || self.name_lc.starts_with("__calc_history__|");

        let is_app_result = matches!(
            &self.open_command,
            AppCommand::Function(Function::OpenApp(_) | Function::ActivateApp(_))
        );
        let desc_is_generic = matches!(self.desc.as_str(), "Application" | "Finder" | "Utility");
        let show_subtitle = !(is_app_result || desc_is_generic || is_calculator_result);
        let mut title_font = theme.font();
        title_font.weight = Weight::Semibold;
        let title_size = if show_subtitle { 17 } else { 18 };

        // Title + subtitle
        let title_opacity = if focused { 1.00 } else { 0.96 };
        let desc_opacity = if focused { 0.78 } else { 0.62 };
        let title = Text::new(self.name.clone())
            .font(title_font)
            .size(title_size)
            .wrapping(Wrapping::WordOrGlyph)
            .color(theme.text_color(title_opacity));
        let text_block = if show_subtitle {
            iced::widget::Column::new().spacing(1).push(title).push(
                Text::new(self.desc.clone())
                    .font(theme.font())
                    .size(12)
                    .color(theme.text_color(desc_opacity)),
            )
        } else {
            iced::widget::Column::new().spacing(0).push(title)
        };

        let mut row = Row::new()
            .align_y(Alignment::Center)
            .width(Fill)
            .spacing(RESULT_ROW_CONTENT_GAP)
            .height(RESULT_ROW_CONTENT_HEIGHT);

        if theme.show_icons && !is_currency_result && !is_calculator_result {
            if let Some(icon) = &self.icons {
                row = row.push(
                    container(
                        image(icon.clone())
                            .height(RESULT_ICON_SIZE)
                            .width(RESULT_ICON_SIZE),
                    )
                    .width(RESULT_ICON_SLOT)
                    .height(RESULT_ICON_SLOT)
                    .padding(1),
                );
            } else {
                row = row.push(
                    container(crate::icons::icon(
                        crate::icons::APP,
                        16.0,
                        theme.text_color(0.55),
                    ))
                    .width(RESULT_ICON_SLOT)
                    .height(RESULT_ICON_SLOT)
                    .center_x(Fill)
                    .center_y(Fill),
                );
            }
        }
        row = row.push(container(text_block).width(Fill));

        let msg = match self.open_command.clone() {
            AppCommand::Function(func) => Some(Message::RunFunction(func)),
            AppCommand::Message(msg) => Some(msg),
            AppCommand::Display => None,
        };

        let theme_for_button = theme.clone();
        let theme_for_container = theme;
        let calc_button_style = is_calculator_result;
        let calc_container_style = is_calculator_result;

        let content = Button::new(row)
            .on_press_maybe(msg)
            .style(move |_, status| {
                if calc_button_style {
                    iced::widget::button::Style {
                        text_color: theme_for_button.text_color(0.95),
                        background: Some(iced::Background::Color(iced::Color::TRANSPARENT)),
                        border: iced::Border {
                            color: iced::Color::TRANSPARENT,
                            width: 0.0,
                            radius: iced::border::Radius::new(8.0),
                        },
                        ..Default::default()
                    }
                } else {
                    result_button_style(&theme_for_button, focused, status)
                }
            })
            .width(Fill)
            .padding(0)
            .height(RESULT_ROW_CONTENT_HEIGHT);

        let row_container = container(content)
            .id(format!("result-{}", id_num))
            .style(move |_| {
                if calc_container_style {
                    iced::widget::container::Style {
                        background: Some(iced::Background::Color(iced::Color::TRANSPARENT)),
                        border: iced::Border {
                            color: iced::Color::TRANSPARENT,
                            width: 0.0,
                            radius: iced::border::Radius::new(12.0),
                        },
                        ..Default::default()
                    }
                } else {
                    result_row_container_style(&theme_for_container, focused)
                }
            })
            .padding([RESULT_ROW_PADDING_Y, RESULT_ROW_PADDING_X])
            .width(Fill);

        mouse_area(row_container)
            .on_enter(Message::HoverResult(id_num))
            .into()
    }

    /// Render with a status badge (for zero-query state: "Running" or relative time)
    pub fn render_with_status(
        self,
        theme: crate::config::Theme,
        id_num: u32,
        focussed_id: u32,
    ) -> iced::Element<'static, Message> {
        use crate::styles::running_dot_color;

        let focused = focussed_id == id_num;
        let mut title_font = theme.font();
        title_font.weight = Weight::Semibold;
        let title_opacity = if focused { 1.00 } else { 0.96 };
        let desc_opacity = if focused { 0.78 } else { 0.62 };
        let title_size = 17;

        // Status badge: green dot + "Running" for running apps, desc text for recent
        let status_text = match &self.category {
            Some(AppCategory::Running) => Some(("Running", true)),
            Some(AppCategory::Recent) => Some((&*self.desc, false)),
            None => None,
        };

        let text_block = iced::widget::Column::new()
            .spacing(1)
            .push(
                Text::new(self.name.clone())
                    .font(title_font)
                    .size(title_size)
                    .wrapping(Wrapping::WordOrGlyph)
                    .color(theme.text_color(title_opacity)),
            )
            .push(
                Text::new(
                    self.bundle_path
                        .as_deref()
                        .unwrap_or(&self.desc)
                        .to_string(),
                )
                .font(theme.font())
                .size(12)
                .color(theme.text_color(desc_opacity)),
            );

        let mut row = Row::new()
            .align_y(Alignment::Center)
            .width(Fill)
            .spacing(RESULT_ROW_CONTENT_GAP)
            .height(RESULT_ROW_CONTENT_HEIGHT);

        if theme.show_icons {
            if let Some(icon) = &self.icons {
                row = row.push(
                    container(
                        image(icon.clone())
                            .height(RESULT_ICON_SIZE)
                            .width(RESULT_ICON_SIZE),
                    )
                    .width(RESULT_ICON_SLOT)
                    .height(RESULT_ICON_SLOT)
                    .padding(1),
                );
            } else {
                row = row.push(
                    container(crate::icons::icon(
                        crate::icons::APP,
                        16.0,
                        theme.text_color(0.55),
                    ))
                    .width(RESULT_ICON_SLOT)
                    .height(RESULT_ICON_SLOT)
                    .center_x(Fill)
                    .center_y(Fill),
                );
            }
        }
        row = row.push(container(text_block).width(Fill));

        // Status badge on the right
        if let Some((label, is_running)) = status_text {
            if is_running {
                let dot = crate::icons::icon(crate::icons::CIRCLE_FILL, 8.0, running_dot_color());
                let label = Text::new(label.to_string())
                    .size(12)
                    .color(theme.text_color(0.58))
                    .font(theme.font());
                row = row.push(
                    Row::new()
                        .push(dot)
                        .push(label)
                        .spacing(4)
                        .align_y(Alignment::Center),
                );
            } else {
                row = row.push(
                    Text::new(label.to_string())
                        .size(12)
                        .color(theme.text_color(0.52))
                        .font(theme.font()),
                );
            }
        }

        let msg = match self.open_command.clone() {
            AppCommand::Function(func) => Some(Message::RunFunction(func)),
            AppCommand::Message(msg) => Some(msg),
            AppCommand::Display => None,
        };

        let theme_clone = theme.clone();

        let content = Button::new(row)
            .on_press_maybe(msg)
            .style(move |_, status| result_button_style(&theme_clone, focused, status))
            .width(Fill)
            .padding(0)
            .height(RESULT_ROW_CONTENT_HEIGHT);

        let row_container = container(content)
            .id(format!("result-{}", id_num))
            .style(move |_| result_row_container_style(&theme, focused))
            .padding([RESULT_ROW_PADDING_Y, RESULT_ROW_PADDING_X])
            .width(Fill);

        mouse_area(row_container)
            .on_enter(Message::HoverResult(id_num))
            .into()
    }
}
