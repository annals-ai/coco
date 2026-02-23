use iced::futures::channel::mpsc::Sender;
use iced::futures::SinkExt;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::app::Message;
use super::types::ClaudeEvent;

/// Spawn a claude CLI subprocess and stream events back via the iced channel.
///
/// - `prompt`: the user's message (empty string for resume-only)
/// - `session_id`: if Some, resumes an existing session
/// - `sender`: iced mpsc sender for pushing Message::AgentEvent
pub fn spawn_claude(
    prompt: String,
    session_id: Option<String>,
    mut sender: Sender<Message>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut cmd = Command::new("claude");
        cmd.arg("-p")
            .arg("--output-format")
            .arg("stream-json")
            .arg("--dangerously-skip-permissions");

        if let Some(ref sid) = session_id {
            cmd.arg("--resume").arg(sid);
        }

        // Pass prompt as the final argument (if non-empty)
        if !prompt.is_empty() {
            cmd.arg(&prompt);
        }

        // Avoid nested Claude Code detection
        cmd.env_remove("CLAUDECODE");

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::null());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = sender
                    .send(Message::AgentEvent(ClaudeEvent::Error(format!(
                        "Failed to spawn claude: {}",
                        e
                    ))))
                    .await;
                return;
            }
        };

        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                let _ = sender
                    .send(Message::AgentEvent(ClaudeEvent::Error(
                        "No stdout from claude process".to_string(),
                    )))
                    .await;
                return;
            }
        };

        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        // Track cumulative assistant text to compute deltas
        let mut last_text_len: usize = 0;

        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            let obj: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let event = parse_event(&obj, &mut last_text_len);
            if let Some(evt) = event {
                let is_finished = matches!(evt, ClaudeEvent::Finished);
                let _ = sender.send(Message::AgentEvent(evt)).await;
                if is_finished {
                    break;
                }
            }
        }

        // Ensure the child process is cleaned up
        let _ = child.wait().await;
    })
}

/// Parse a single NDJSON line from claude --output-format stream-json.
///
/// Known event types:
/// - `{"type":"system","subtype":"init","session_id":"..."}` → SessionStarted
/// - `{"type":"assistant","message":{"content":[{"type":"text","text":"..."},{"type":"tool_use",...}]}}` → TextDelta / ToolUse
/// - `{"type":"result","result":"..."}` → Finished
fn parse_event(obj: &serde_json::Value, last_text_len: &mut usize) -> Option<ClaudeEvent> {
    let event_type = obj.get("type")?.as_str()?;

    match event_type {
        "system" => {
            let subtype = obj.get("subtype").and_then(|s| s.as_str()).unwrap_or("");
            if subtype == "init" {
                let sid = obj
                    .get("session_id")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();
                Some(ClaudeEvent::SessionStarted(sid))
            } else {
                None
            }
        }
        "assistant" => {
            let message = obj.get("message")?;
            let content = message.get("content")?;
            let arr = content.as_array()?;

            let mut events = Vec::new();

            // Collect all text content into a single string
            let mut full_text = String::new();
            for block in arr {
                let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match block_type {
                    "text" => {
                        if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                            full_text.push_str(text);
                        }
                    }
                    "tool_use" => {
                        let name = block
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        events.push(ClaudeEvent::ToolUse { name });
                    }
                    _ => {}
                }
            }

            // Compute text delta
            if full_text.len() > *last_text_len {
                let delta = full_text[*last_text_len..].to_string();
                *last_text_len = full_text.len();
                // Return text delta; tool_use events are secondary
                return Some(ClaudeEvent::TextDelta(delta));
            }

            // If no text delta, return first tool_use event
            events.into_iter().next()
        }
        "result" => {
            *last_text_len = 0; // Reset for next turn
            Some(ClaudeEvent::Finished)
        }
        _ => None,
    }
}
