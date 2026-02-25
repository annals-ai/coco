//! This handles all the different commands that Coco can perform, such as opening apps,
//! copying to clipboard, etc.
use std::{process::Command, thread};

use arboard::Clipboard;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::NSURL;

use crate::{calculator::Expr, clipboard::ClipBoardContentType, config::Config};

/// The different functions that Coco can perform
#[derive(Debug, Clone, PartialEq)]
pub enum Function {
    OpenApp(String),
    ActivateApp(i32),
    HideApp(i32),
    QuitApp(i32),
    ForceQuitApp(i32),
    ShowInFinder(String),
    CopyPath(String),
    CopyBundleId(String),
    RunShellCommand(String, String),
    OpenWebsite(String),
    RandomVar(i32), // Easter egg function
    CopyToClipboard(ClipBoardContentType),
    GoogleSearch(String),
    Calculate(Expr),
    OpenPrefPane,
    Quit,
}

impl Function {
    /// Run the command
    pub fn execute(&self, config: &Config, query: &str) {
        match self {
            Function::OpenApp(path) => {
                let path = path.to_owned();
                // Record in history
                let name = std::path::Path::new(&path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let path_clone = path.clone();
                thread::spawn(move || {
                    NSWorkspace::new().openURL(&NSURL::fileURLWithPath(
                        &objc2_foundation::NSString::from_str(&path_clone),
                    ));
                });
                // Record launch history in background
                thread::spawn(move || {
                    let mut history = crate::history::History::load();
                    history.record_launch(&path, &name);
                });
            }
            Function::ActivateApp(pid) => {
                crate::platform::activate_app_by_pid(*pid);
            }
            Function::HideApp(pid) => {
                crate::platform::hide_app_by_pid(*pid);
            }
            Function::QuitApp(pid) => {
                crate::platform::quit_app_by_pid(*pid);
            }
            Function::ForceQuitApp(pid) => {
                crate::platform::force_quit_app_by_pid(*pid);
            }
            Function::ShowInFinder(path) => {
                crate::platform::reveal_in_finder(path);
            }
            Function::CopyPath(path) => {
                Clipboard::new().unwrap().set_text(path.clone()).ok();
            }
            Function::CopyBundleId(bundle_id) => {
                Clipboard::new().unwrap().set_text(bundle_id.clone()).ok();
            }
            Function::RunShellCommand(command, alias) => {
                let query = query.to_string();
                let final_command =
                    format!(r#"{} {}"#, command, query.strip_prefix(alias).unwrap_or(""));
                Command::new("sh")
                    .arg("-c")
                    .arg(final_command.trim())
                    .spawn()
                    .ok();
            }
            Function::RandomVar(var) => {
                Clipboard::new()
                    .unwrap()
                    .set_text(var.to_string())
                    .unwrap_or(());
            }

            Function::GoogleSearch(query_string) => {
                let query_args = query_string.replace(" ", "+");
                let query = config.search_url.replace("%s", &query_args);
                let query = query.strip_suffix("?").unwrap_or(&query).to_string();
                thread::spawn(move || {
                    NSWorkspace::new().openURL(
                        &NSURL::URLWithString_relativeToURL(
                            &objc2_foundation::NSString::from_str(&query),
                            None,
                        )
                        .unwrap(),
                    );
                });
            }

            Function::OpenWebsite(url) => {
                let open = if url.starts_with("http") {
                    url.to_owned()
                } else {
                    format!("https://{}", url)
                };
                thread::spawn(move || {
                    NSWorkspace::new().openURL(
                        &NSURL::URLWithString_relativeToURL(
                            &objc2_foundation::NSString::from_str(&open),
                            None,
                        )
                        .unwrap(),
                    );
                });
            }

            Function::Calculate(expr) => {
                Clipboard::new()
                    .unwrap()
                    .set_text(expr.eval().map(|x| x.to_string()).unwrap_or("".to_string()))
                    .unwrap_or(());
            }

            Function::CopyToClipboard(clipboard_content) => match clipboard_content {
                ClipBoardContentType::Text(text) => {
                    Clipboard::new().unwrap().set_text(text).ok();
                }
                ClipBoardContentType::Image(img) => {
                    Clipboard::new().unwrap().set_image(img.to_owned_img()).ok();
                }
            },

            Function::OpenPrefPane => {
                thread::spawn(move || {
                    NSWorkspace::new().openURL(&NSURL::fileURLWithPath(
                        &objc2_foundation::NSString::from_str(
                            &(std::env::var("HOME").unwrap_or("".to_string())
                                + "/.config/coco/config.toml"),
                        ),
                    ));
                });
            }
            Function::Quit => std::process::exit(0),
        }
    }
}
