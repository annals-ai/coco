//! Quick Actions (⌘K) overlay for Coco.
//!
//! Provides contextual actions for the currently selected app.

use crate::app::apps::{App, AppCategory};
use crate::commands::Function;

/// An action that can be performed on an app
#[derive(Debug, Clone)]
pub enum Action {
    Activate(i32),
    Hide(i32),
    Quit(i32),
    ForceQuit(i32),
    ShowInFinder(String),
    CopyPath(String),
    CopyBundleId(String),
    Open(String),
}

/// A displayable action item
#[derive(Debug, Clone)]
pub struct ActionItem {
    pub action: Action,
    pub label: String,
    pub shortcut: Option<&'static str>,
    pub is_destructive: bool,
    pub group: ActionGroup,
}

/// Action groups for visual separation
#[derive(Debug, Clone, PartialEq)]
pub enum ActionGroup {
    Running,
    File,
}

/// Compute available actions for a given app
pub fn compute_actions(app: &App) -> Vec<ActionItem> {
    let mut items = Vec::new();

    let is_running = app.category == Some(AppCategory::Running);
    let pid = app.pid;
    let bundle_path = app.bundle_path.clone();

    // Running app actions
    if is_running {
        if let Some(pid) = pid {
            items.push(ActionItem {
                action: Action::Activate(pid),
                label: "Activate".to_string(),
                shortcut: Some("\u{23CE}"),
                is_destructive: false,
                group: ActionGroup::Running,
            });
            items.push(ActionItem {
                action: Action::Hide(pid),
                label: "Hide".to_string(),
                shortcut: Some("\u{2318}H"),
                is_destructive: false,
                group: ActionGroup::Running,
            });
            items.push(ActionItem {
                action: Action::Quit(pid),
                label: "Quit".to_string(),
                shortcut: Some("\u{2318}Q"),
                is_destructive: false,
                group: ActionGroup::Running,
            });
            items.push(ActionItem {
                action: Action::ForceQuit(pid),
                label: "Force Quit".to_string(),
                shortcut: None,
                is_destructive: true,
                group: ActionGroup::Running,
            });
        }
    }

    // File actions (for any app with a bundle path)
    if let Some(ref path) = bundle_path {
        // If not running, "Open" is the first action
        if !is_running {
            items.push(ActionItem {
                action: Action::Open(path.clone()),
                label: "Open".to_string(),
                shortcut: Some("\u{23CE}"),
                is_destructive: false,
                group: ActionGroup::File,
            });
        }

        items.push(ActionItem {
            action: Action::ShowInFinder(path.clone()),
            label: "Show in Finder".to_string(),
            shortcut: Some("\u{21E7}\u{23CE}"),
            is_destructive: false,
            group: ActionGroup::File,
        });
        items.push(ActionItem {
            action: Action::CopyPath(path.clone()),
            label: "Copy Path".to_string(),
            shortcut: Some("\u{2318}C"),
            is_destructive: false,
            group: ActionGroup::File,
        });
    }

    // Bundle ID action
    if let Some(ref bid) = app.bundle_id {
        items.push(ActionItem {
            action: Action::CopyBundleId(bid.clone()),
            label: "Copy Bundle ID".to_string(),
            shortcut: Some("\u{2318}\u{21E7}C"),
            is_destructive: false,
            group: ActionGroup::File,
        });
    }

    // Also add path from desc for non-categorized apps (installed apps from search)
    if bundle_path.is_none() && !is_running {
        if let crate::app::apps::AppCommand::Function(Function::OpenApp(ref path)) =
            app.open_command
        {
            items.push(ActionItem {
                action: Action::ShowInFinder(path.clone()),
                label: "Show in Finder".to_string(),
                shortcut: Some("\u{21E7}\u{23CE}"),
                is_destructive: false,
                group: ActionGroup::File,
            });
            items.push(ActionItem {
                action: Action::CopyPath(path.clone()),
                label: "Copy Path".to_string(),
                shortcut: Some("\u{2318}C"),
                is_destructive: false,
                group: ActionGroup::File,
            });
        }
    }

    items
}

/// Convert an Action to a Function for execution
pub fn action_to_function(action: &Action) -> Function {
    match action {
        Action::Activate(pid) => Function::ActivateApp(*pid),
        Action::Hide(pid) => Function::HideApp(*pid),
        Action::Quit(pid) => Function::QuitApp(*pid),
        Action::ForceQuit(pid) => Function::ForceQuitApp(*pid),
        Action::ShowInFinder(path) => Function::ShowInFinder(path.clone()),
        Action::CopyPath(path) => Function::CopyPath(path.clone()),
        Action::CopyBundleId(bid) => Function::CopyBundleId(bid.clone()),
        Action::Open(path) => Function::OpenApp(path.clone()),
    }
}
