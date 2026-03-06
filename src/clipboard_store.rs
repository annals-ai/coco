//! Clipboard history store with persistence, search, pinning, and deduplication.

use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Matcher, Utf32Str};
use pinyin::ToPinyin;
use serde::{Deserialize, Serialize};

use crate::clipboard::ClipBoardContentType;

/// A single clipboard history entry with metadata for display and search.
#[derive(Clone, Debug)]
pub struct ClipboardEntry {
    pub id: u64,
    pub content: ClipBoardContentType,
    pub pinned: bool,
    pub created_at: DateTime<Utc>,
    /// First line of text truncated to 80 chars, or "Image WxH" for images.
    pub preview_title: String,
    /// Lowercase full text (truncated to 2000 chars) for substring search.
    pub content_lc: String,
    /// Pinyin representation of CJK content (first 200 chars).
    pub pinyin_full: String,
    pub has_cjk: bool,
}

/// Persistent representation of a text-only clipboard entry.
#[derive(Serialize, Deserialize)]
struct ClipboardEntryPersist {
    id: u64,
    text: String,
    pinned: bool,
    created_at: DateTime<Utc>,
}

/// Persistent store format saved to disk.
#[derive(Serialize, Deserialize)]
struct ClipboardStorePersist {
    next_id: u64,
    entries: Vec<ClipboardEntryPersist>,
}

/// The in-memory clipboard history store.
pub struct ClipboardStore {
    pub entries: Vec<ClipboardEntry>,
    next_id: u64,
    max_entries: usize,
}

impl ClipboardStore {
    /// Load clipboard history from disk, or create empty store.
    pub fn load() -> Self {
        let path = Self::persist_path();
        let mut store = ClipboardStore {
            entries: Vec::new(),
            next_id: 1,
            max_entries: 500,
        };

        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(persist) = serde_json::from_str::<ClipboardStorePersist>(&data) {
                store.next_id = persist.next_id;
                for ep in persist.entries {
                    let content = ClipBoardContentType::Text(ep.text.clone());
                    store
                        .entries
                        .push(build_entry(ep.id, content, ep.pinned, ep.created_at));
                }
            }
        }

        store
    }

    /// Save text-only entries to disk.
    pub fn save(&self) {
        let path = Self::persist_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok();
        }

        let persist = ClipboardStorePersist {
            next_id: self.next_id,
            entries: self
                .entries
                .iter()
                .filter_map(|e| {
                    if let ClipBoardContentType::Text(ref text) = e.content {
                        Some(ClipboardEntryPersist {
                            id: e.id,
                            text: text.clone(),
                            pinned: e.pinned,
                            created_at: e.created_at,
                        })
                    } else {
                        None
                    }
                })
                .collect(),
        };

        if let Ok(json) = serde_json::to_string(&persist) {
            fs::write(&path, json).ok();
        }
    }

    /// Push a new clipboard content.
    /// Deduplicates adjacent entries only: if same as the latest history item,
    /// refreshes timestamp and skips inserting a new entry.
    pub fn push(&mut self, content: ClipBoardContentType) {
        // Adjacent-only dedupe: compare with latest non-pinned history entry.
        let insert_pos = self
            .entries
            .iter()
            .position(|e| !e.pinned)
            .unwrap_or(self.entries.len());
        if let Some(latest) = self.entries.get_mut(insert_pos)
            && content_eq(&latest.content, &content)
        {
            latest.created_at = Utc::now();
            self.save();
            return;
        }

        let id = self.next_id;
        self.next_id += 1;
        let entry = build_entry(id, content, false, Utc::now());

        // Insert after pinned items (most recent non-pinned first)
        self.entries.insert(insert_pos, entry);

        // Trim excess non-pinned entries
        self.trim();
        self.save();
    }

    /// Delete entry by id.
    pub fn delete(&mut self, id: u64) {
        self.entries.retain(|e| e.id != id);
        self.save();
    }

    /// Toggle pin state for entry by id.
    pub fn toggle_pin(&mut self, id: u64) {
        if let Some(pos) = self.entries.iter().position(|e| e.id == id) {
            let mut entry = self.entries.remove(pos);
            entry.pinned = !entry.pinned;
            if entry.pinned {
                // Move to front (among pinned)
                self.entries.insert(0, entry);
            } else {
                // Move to first non-pinned position
                let insert_pos = self
                    .entries
                    .iter()
                    .position(|e| !e.pinned)
                    .unwrap_or(self.entries.len());
                self.entries.insert(insert_pos, entry);
            }
            self.save();
        }
    }

    /// Get all entries (pinned first, then by recency).
    pub fn all(&self) -> &[ClipboardEntry] {
        &self.entries
    }

    /// Get entry by index.
    pub fn get(&self, index: usize) -> Option<&ClipboardEntry> {
        self.entries.get(index)
    }

    /// Total entry count.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Search entries. Returns indices into `self.entries`.
    /// Three layers: substring → pinyin → fuzzy.
    pub fn search(&self, query: &str, matcher: &mut Matcher) -> Vec<usize> {
        if query.is_empty() {
            return (0..self.entries.len()).collect();
        }

        let query_lc = query.to_lowercase();
        let mut matched: Vec<usize> = Vec::new();
        let mut matched_set = std::collections::HashSet::new();

        // Layer 1: Substring match on content_lc
        for (i, entry) in self.entries.iter().enumerate() {
            if entry.content_lc.contains(&query_lc)
                || entry.preview_title.to_lowercase().contains(&query_lc)
            {
                if matched_set.insert(i) {
                    matched.push(i);
                }
            }
        }

        // Layer 2: Pinyin match
        for (i, entry) in self.entries.iter().enumerate() {
            if !matched_set.contains(&i) && entry.has_cjk && !entry.pinyin_full.is_empty() {
                if entry.pinyin_full.contains(&query_lc) {
                    if matched_set.insert(i) {
                        matched.push(i);
                    }
                }
            }
        }

        // Layer 3: Fuzzy match
        let pattern = Pattern::new(
            &query_lc,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );
        let mut buf = Vec::new();

        for (i, entry) in self.entries.iter().enumerate() {
            if matched_set.contains(&i) {
                continue;
            }
            let haystack = Utf32Str::new(&entry.content_lc, &mut buf);
            if pattern.score(haystack, matcher).is_some() {
                matched.push(i);
            }
        }

        matched
    }

    fn persist_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join(".config/coco/clipboard_history.json")
    }

    fn trim(&mut self) {
        while self.entries.len() > self.max_entries {
            // Remove last non-pinned entry
            if let Some(pos) = self.entries.iter().rposition(|e| !e.pinned) {
                self.entries.remove(pos);
            } else {
                break; // All pinned, can't trim
            }
        }
    }
}

/// Build a ClipboardEntry with computed metadata.
fn build_entry(
    id: u64,
    content: ClipBoardContentType,
    pinned: bool,
    created_at: DateTime<Utc>,
) -> ClipboardEntry {
    let (preview_title, content_lc, pinyin_full, has_cjk) = match &content {
        ClipBoardContentType::Text(text) => {
            let first_line = text.lines().next().unwrap_or("");
            let title = if first_line.len() > 80 {
                format!(
                    "{}...",
                    &first_line[..first_line
                        .char_indices()
                        .nth(80)
                        .map(|(i, _)| i)
                        .unwrap_or(first_line.len())]
                )
            } else {
                first_line.to_string()
            };
            let lc = text.chars().take(2000).collect::<String>().to_lowercase();
            let has_cjk = text.chars().take(200).any(is_cjk_char);
            let py = if has_cjk {
                compute_pinyin(&text.chars().take(200).collect::<String>())
            } else {
                String::new()
            };
            (title, lc, py, has_cjk)
        }
        ClipBoardContentType::Image(img) => {
            let title = format!("Image {}x{}", img.width, img.height);
            (title, String::new(), String::new(), false)
        }
    };

    ClipboardEntry {
        id,
        content,
        pinned,
        created_at,
        preview_title,
        content_lc,
        pinyin_full,
        has_cjk,
    }
}

/// Compare two clipboard contents for deduplication.
fn content_eq(a: &ClipBoardContentType, b: &ClipBoardContentType) -> bool {
    match (a, b) {
        (ClipBoardContentType::Text(t1), ClipBoardContentType::Text(t2)) => t1 == t2,
        (ClipBoardContentType::Image(i1), ClipBoardContentType::Image(i2)) => {
            i1.width == i2.width
                && i1.height == i2.height
                && i1.bytes.get(..1024) == i2.bytes.get(..1024)
        }
        _ => false,
    }
}

fn is_cjk_char(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}' |
        '\u{3400}'..='\u{4DBF}' |
        '\u{20000}'..='\u{2A6DF}' |
        '\u{F900}'..='\u{FAFF}' |
        '\u{2F800}'..='\u{2FA1F}'
    )
}

fn compute_pinyin(s: &str) -> String {
    let mut full = String::new();
    for c in s.chars() {
        if let Some(py) = c.to_pinyin() {
            full.push_str(py.plain());
        } else {
            full.push_str(&c.to_lowercase().to_string());
        }
    }
    full
}

/// Format a relative time string from a DateTime<Utc>.
pub fn format_relative_time(dt: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = now.signed_duration_since(*dt);

    if diff.num_seconds() < 60 {
        "Just now".to_string()
    } else if diff.num_minutes() < 60 {
        format!("{}m ago", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{}h ago", diff.num_hours())
    } else if diff.num_days() == 1 {
        "Yesterday".to_string()
    } else if diff.num_days() < 30 {
        format!("{}d ago", diff.num_days())
    } else {
        dt.format("%m/%d").to_string()
    }
}
