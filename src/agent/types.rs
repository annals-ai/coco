/// A single conversation session from the Claude CLI history.
#[derive(Debug, Clone)]
pub struct AgentSession {
    pub session_id: String,
    pub title: String,
    pub last_active: u64, // unix timestamp
}

/// Current status of the agent process.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentStatus {
    Idle,
    Thinking,
    Streaming,
}

/// A single chat message in the conversation.
#[derive(Debug, Clone)]
pub enum ChatMessage {
    User(String),
    Assistant(String),
}

/// Events emitted by the Claude CLI stream-json output.
#[derive(Debug, Clone)]
pub enum ClaudeEvent {
    SessionStarted(String),
    TextDelta(String),
    ToolUse { name: String },
    Finished,
    Error(String),
}
