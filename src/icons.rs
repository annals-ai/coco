use iced::Color;
use iced::widget::Text;
use iced_fonts::BOOTSTRAP_FONT;

/// Create a Bootstrap Icon text widget with the given codepoint, size, and color.
pub fn icon(codepoint: char, size: f32, color: Color) -> Text<'static> {
    Text::new(codepoint.to_string())
        .font(BOOTSTRAP_FONT)
        .size(size)
        .color(color)
}

// Icon codepoints from Bootstrap Icons
pub const SEARCH: char = '\u{F52A}'; // bi-search
pub const EXCLAMATION_TRIANGLE: char = '\u{F33B}'; // bi-exclamation-triangle
pub const ARROW_RETURN_LEFT: char = '\u{F124}'; // bi-arrow-return-left (⏎ equivalent)
pub const CIRCLE_FILL: char = '\u{F287}'; // bi-circle-fill
pub const CLIPBOARD: char = '\u{F290}'; // bi-clipboard
pub const APP: char = '\u{F0BF}'; // bi-app (generic app placeholder)
pub const APP_PACKAGE: char = '\u{F1C7}'; // bi-box-seam
