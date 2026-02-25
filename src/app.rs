//! Main logic for the app

use crate::agent::types::ClaudeEvent;
use crate::commands::Function;
use crate::{app::tile::ExtSender, clipboard::ClipBoardContentType};

pub mod actions;
pub mod apps;
pub mod menubar;
pub mod pages;
pub mod tile;

use iced::window::{self, Id, Settings};
/// The default window width
pub const WINDOW_WIDTH: f32 = 680.;

/// Fixed initial window height (actual runtime height is dynamic).
/// search bar (~58) + separator (1) + 7 rows (406) + footer (38) ≈ 503
pub const WINDOW_HEIGHT: f32 = 500.;

// ── Shared layout constants ────────────────────────────────────────────────
// All height-sensitive elements use these so blur-height calculation
// and view rendering always agree on exact pixel values.

/// Shared padding for the main results lists (search + zero-query).
pub const RESULT_LIST_PADDING_Y: u16 = 2;
pub const RESULT_LIST_PADDING_X: u16 = 6;
/// Shared row shell metrics used by both search results and zero-query rows.
pub const RESULT_ROW_PADDING_Y: u16 = 4;
pub const RESULT_ROW_PADDING_X: u16 = 12;
pub const RESULT_ROW_CONTENT_HEIGHT: u32 = 50;
pub const RESULT_ROW_CONTENT_GAP: u32 = 16;
pub const RESULT_ICON_SLOT: u32 = 40;
pub const RESULT_ICON_SIZE: u32 = 38;
/// Zero-query section header padding is aligned to row container inset.
pub const ZQ_HEADER_PADDING_Y: u16 = 6;
pub const ZQ_HEADER_PADDING_X: u16 = RESULT_ROW_PADDING_X;

/// Height of one result row (content + outer row container padding)
pub const ROW_HEIGHT: f64 = (RESULT_ROW_CONTENT_HEIGHT + (RESULT_ROW_PADDING_Y as u32 * 2)) as f64;
/// Max visible rows in the main results list before scrolling.
pub const MAX_VISIBLE_ROWS: usize = 7;
/// Scrollable viewport height for the main results list.
pub const MAX_RESULTS_SCROLL_HEIGHT: f64 = ROW_HEIGHT * MAX_VISIBLE_ROWS as f64;
/// Height of a zero-query section header ("RUNNING" / "RECENT")
/// Explicit height ensures formula matches rendered output exactly.
pub const ZQ_HEADER_HEIGHT: f64 = 28.0;
/// Vertical padding of the results Column.
pub const ZQ_COL_PADDING_V: f64 = (RESULT_LIST_PADDING_Y * 2) as f64;
/// Search bar height (includes top/bottom breathing room around the pill)
pub const SEARCH_BAR_HEIGHT: f64 = 58.0;
/// Separator height
pub const SEPARATOR_HEIGHT: f64 = 1.0;
/// Footer height: Row(28) + container padding([5,0]) = 38px
pub const FOOTER_HEIGHT: f64 = 38.0;

/// The Coco descriptor name to be put for all Coco commands
pub const COCO_DESC_NAME: &str = "Utility";

/// The different pages that Coco can have / has
#[derive(Debug, Clone, PartialEq)]
pub enum Page {
    Main,
    ClipboardHistory,
    EmojiSearch,
    AgentList,
    WindowSwitcher,
}

/// The types of arrow keys
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum ArrowKey {
    Up,
    Down,
    Left,
    Right,
}

/// The ways the cursor can move when a key is pressed
#[derive(Debug, Clone)]
pub enum Move {
    Back,
    Forwards(String),
}

/// The message type that iced uses for actions that can do something
#[derive(Debug, Clone)]
pub enum Message {
    OpenWindow(Option<window::Id>),
    SearchQueryChanged(String, Id),
    KeyPressed(u32),
    FocusTextInput(Move),
    HideWindow(Id),
    RunFunction(Function),
    OpenFocused,
    ReturnFocus,
    EscKeyPressed(Id),
    ClearSearchResults,
    WindowFocusChanged(Id, bool),
    ClearSearchQuery,
    HideTrayIcon,
    ReloadConfig,
    SetSender(ExtSender),
    SwitchToPage(Page),
    ClipboardHistory(ClipBoardContentType),
    ChangeFocus(ArrowKey),
    ToggleAgentMode,
    AgentSessionSelected(String),
    NewAgentSession(String),
    AgentInput(String),
    AgentSubmit,
    AgentEvent(ClaudeEvent),
    AgentWindowClosed(window::Id),
    OpenAccessibilitySettings,
    OpenInputMonitoringSettings,
    // Quick Actions (⌘K)
    ShowActions,
    ExecuteAction(crate::app::actions::Action),
    ActionFocusChanged(ArrowKey),
    // Clipboard actions
    ClipboardTogglePinFocused,
    ClipboardDeleteFocused,
    FocusWindow(i32, u32),
    // Native show/hide animation completions (macOS)
    NativeHideComplete,
    NativeShowComplete,
    ApplyDebouncedWindowResize(u64, f64),
}

/// The window settings for Coco
pub fn default_settings() -> Settings {
    Settings {
        resizable: false,
        decorations: false,
        minimizable: false,
        level: window::Level::AlwaysOnTop,
        transparent: true,
        blur: false,
        size: iced::Size {
            width: WINDOW_WIDTH,
            height: WINDOW_HEIGHT,
        },
        ..Default::default()
    }
}

/// Window settings for the standalone agent chat window.
pub fn agent_window_settings() -> Settings {
    Settings {
        resizable: true,
        decorations: false,
        minimizable: true,
        level: window::Level::Normal,
        transparent: true,
        blur: false,
        size: iced::Size {
            width: 720.,
            height: 520.,
        },
        ..Default::default()
    }
}
