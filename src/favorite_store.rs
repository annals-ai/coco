//! Clipboard favorites store with persistence and search.

use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Matcher, Utf32Str};
use pinyin::ToPinyin;
use serde::{Deserialize, Serialize};

use crate::clipboard::ClipBoardContentType;

/// A single favorite entry with custom title and search metadata.
#[derive(Clone, Debug)]
pub struct FavoriteEntry {
    pub id: u64,
    pub content: ClipBoardContentType,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub title_lc: String,
    pub content_lc: String,
    pub pinyin_full: String,
    pub has_cjk: bool,
}

/// Persistent representation of a favorite entry (text only).
#[derive(Serialize, Deserialize)]
struct FavoriteEntryPersist {
    id: u64,
    text: String,
    title: String,
    created_at: DateTime<Utc>,
}

/// Persistent store format saved to disk.
#[derive(Serialize, Deserialize)]
struct FavoriteStorePersist {
    next_id: u64,
    entries: Vec<FavoriteEntryPersist>,
}

/// The in-memory favorites store.
pub struct FavoriteStore {
    pub entries: Vec<FavoriteEntry>,
    next_id: u64,
}

impl FavoriteStore {
    /// Load favorites from disk, or create empty store.
    pub fn load() -> Self {
        let path = Self::persist_path();
        let mut store = FavoriteStore {
            entries: Vec::new(),
            next_id: 1,
        };

        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(persist) = serde_json::from_str::<FavoriteStorePersist>(&data) {
                store.next_id = persist.next_id;
                for ep in persist.entries {
                    let content = ClipBoardContentType::Text(ep.text.clone());
                    store.entries.push(build_favorite_entry(
                        ep.id,
                        content,
                        ep.title,
                        ep.created_at,
                    ));
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

        let persist = FavoriteStorePersist {
            next_id: self.next_id,
            entries: self
                .entries
                .iter()
                .filter_map(|e| {
                    if let ClipBoardContentType::Text(ref text) = e.content {
                        Some(FavoriteEntryPersist {
                            id: e.id,
                            text: text.clone(),
                            title: e.title.clone(),
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

    /// Add a new favorite with a title.
    pub fn add(&mut self, content: ClipBoardContentType, title: String) {
        // Adjacent-only dedupe: if latest favorite has same content, skip insert.
        if let Some(latest) = self.entries.first()
            && content_eq(&latest.content, &content)
        {
            return;
        }

        let id = self.next_id;
        self.next_id += 1;
        let entry = build_favorite_entry(id, content, title, Utc::now());
        self.entries.insert(0, entry);
        self.save();
    }

    /// Delete a favorite by id.
    pub fn delete(&mut self, id: u64) {
        self.entries.retain(|e| e.id != id);
        self.save();
    }

    /// Rename a favorite by id.
    pub fn rename(&mut self, id: u64, new_title: String) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.id == id) {
            entry.title = new_title.clone();
            entry.title_lc = new_title.to_lowercase();
            // Recompute pinyin for the new title
            let combined = format!("{} {}", entry.title, entry.content_lc);
            let has_cjk = combined.chars().take(200).any(is_cjk_char);
            entry.has_cjk = has_cjk;
            entry.pinyin_full = if has_cjk {
                compute_pinyin(&combined.chars().take(200).collect::<String>())
            } else {
                String::new()
            };
            self.save();
        }
    }

    /// Search favorites. Returns indices into `self.entries`.
    /// Three layers: substring -> pinyin -> fuzzy.
    pub fn search(&self, query: &str, matcher: &mut Matcher) -> Vec<usize> {
        if query.is_empty() {
            return (0..self.entries.len()).collect();
        }

        let query_lc = query.to_lowercase();
        let mut matched: Vec<usize> = Vec::new();
        let mut matched_set = std::collections::HashSet::new();

        // Layer 1: Substring match on title_lc and content_lc
        for (i, entry) in self.entries.iter().enumerate() {
            if entry.title_lc.contains(&query_lc) || entry.content_lc.contains(&query_lc) {
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
            let haystack_str = format!("{} {}", entry.title_lc, entry.content_lc);
            let haystack = Utf32Str::new(&haystack_str, &mut buf);
            if pattern.score(haystack, matcher).is_some() {
                matched.push(i);
            }
        }

        matched
    }

    /// Get all entries.
    pub fn all(&self) -> &[FavoriteEntry] {
        &self.entries
    }

    /// Get entry by index.
    pub fn get(&self, index: usize) -> Option<&FavoriteEntry> {
        self.entries.get(index)
    }

    /// Total entry count.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn persist_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home).join(".config/coco/clipboard_favorites.json")
    }
}

/// Build a FavoriteEntry with computed metadata.
fn build_favorite_entry(
    id: u64,
    content: ClipBoardContentType,
    title: String,
    created_at: DateTime<Utc>,
) -> FavoriteEntry {
    let (content_lc, has_cjk, pinyin_full) = match &content {
        ClipBoardContentType::Text(text) => {
            let lc = text.chars().take(2000).collect::<String>().to_lowercase();
            let combined = format!("{} {}", title, lc);
            let has_cjk = combined.chars().take(200).any(is_cjk_char);
            let py = if has_cjk {
                compute_pinyin(&combined.chars().take(200).collect::<String>())
            } else {
                String::new()
            };
            (lc, has_cjk, py)
        }
        ClipBoardContentType::Image(_) => (String::new(), false, String::new()),
    };

    let title_lc = title.to_lowercase();

    FavoriteEntry {
        id,
        content,
        title,
        created_at,
        title_lc,
        content_lc,
        pinyin_full,
        has_cjk,
    }
}

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
