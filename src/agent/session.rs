use std::fs;
use std::path::PathBuf;

use super::types::AgentSession;

/// List all Claude CLI sessions from disk.
/// Scans all project directories under ~/.claude/projects/ for JSONL conversation files.
pub fn list_sessions() -> Vec<AgentSession> {
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return Vec::new(),
    };

    let projects_dir = PathBuf::from(&home).join(".claude/projects");
    if !projects_dir.is_dir() {
        return Vec::new();
    }

    let mut sessions = Vec::new();

    // Scan all project directories
    let project_dirs = match fs::read_dir(&projects_dir) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    for project_entry in project_dirs.flatten() {
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }

        let entries = match fs::read_dir(&project_path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }

            let session_id = match path.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            };

            let mtime = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            // Extract title from first meaningful user message
            let title = extract_title(&path).unwrap_or_else(|| session_id[..8].to_string());

            // Skip sessions with no real content (only file-history-snapshot)
            if title == session_id[..std::cmp::min(8, session_id.len())] {
                // Check if there's any user message at all
                if !has_user_message(&path) {
                    continue;
                }
            }

            sessions.push(AgentSession {
                session_id,
                title,
                last_active: mtime,
            });
        }
    }

    // Sort by last_active descending (most recent first)
    sessions.sort_by(|a, b| b.last_active.cmp(&a.last_active));
    sessions
}

/// Extract a title from the first meaningful user message in the JSONL file.
fn extract_title(path: &PathBuf) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let obj: serde_json::Value = serde_json::from_str(line).ok()?;
        if obj.get("type")?.as_str()? != "user" {
            continue;
        }

        let message = obj.get("message")?;
        let content = message.get("content")?;

        let text = if let Some(arr) = content.as_array() {
            arr.iter()
                .filter_map(|c| {
                    if c.get("type")?.as_str()? == "text" {
                        c.get("text")?.as_str().map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .next()?
        } else {
            content.as_str()?.to_string()
        };

        // Skip system/local command messages
        if text.starts_with("<local-command")
            || text.starts_with("<command-name>")
            || text.starts_with("[Request interrupted")
            || text.trim().is_empty()
        {
            continue;
        }

        // Truncate to first 60 chars and first line
        let first_line = text.lines().next().unwrap_or(&text);
        let truncated = if first_line.len() > 60 {
            format!("{}...", &first_line[..57])
        } else {
            first_line.to_string()
        };

        return Some(truncated);
    }

    None
}

/// Check if the file has any user messages at all.
fn has_user_message(path: &PathBuf) -> bool {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return false,
    };

    for line in content.lines() {
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(line) {
            if obj.get("type").and_then(|t| t.as_str()) == Some("user") {
                return true;
            }
        }
    }

    false
}
