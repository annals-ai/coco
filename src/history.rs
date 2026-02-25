//! Launch history tracking for Coco.
//!
//! Records each app launch to `~/.config/coco/history.json` and provides
//! scoring/ranking for the "Recent" section of the zero-query state.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub bundle_path: String,
    pub name: String,
    pub count: u32,
    pub last_used: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct History {
    pub entries: HashMap<String, HistoryEntry>,
}

fn history_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home).join(".config/coco/history.json")
}

impl History {
    pub fn load() -> Self {
        let path = history_path();
        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) {
        let path = history_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, json);
        }
    }

    pub fn record_launch(&mut self, bundle_path: &str, name: &str) {
        let entry = self
            .entries
            .entry(bundle_path.to_string())
            .or_insert_with(|| HistoryEntry {
                bundle_path: bundle_path.to_string(),
                name: name.to_string(),
                count: 0,
                last_used: Utc::now(),
            });
        entry.count += 1;
        entry.last_used = Utc::now();
        entry.name = name.to_string();
        self.save();
    }

    /// Return top entries scored by: score = count * 0.3 + recency_score * 0.7
    /// recency_score decays over time (1.0 for now, 0.0 for 30+ days ago)
    pub fn top_recent(&self, limit: usize) -> Vec<HistoryEntry> {
        let now = Utc::now();
        let mut scored: Vec<(f64, HistoryEntry)> = self
            .entries
            .values()
            .map(|e| {
                let age_hours = (now - e.last_used).num_hours().max(0) as f64;
                // Decay over 720 hours (30 days)
                let recency = (1.0 - age_hours / 720.0).max(0.0) * 100.0;
                let score = e.count as f64 * 0.3 + recency * 0.7;
                (score, e.clone())
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        scored.into_iter().map(|(_, e)| e).collect()
    }
}

/// Format a relative time string like "2h ago", "Yesterday", "3d ago"
pub fn format_relative_time(dt: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = now - *dt;

    if diff.num_minutes() < 1 {
        "Just now".to_string()
    } else if diff.num_minutes() < 60 {
        format!("{}m ago", diff.num_minutes())
    } else if diff.num_hours() < 24 {
        format!("{}h ago", diff.num_hours())
    } else if diff.num_hours() < 48 {
        "Yesterday".to_string()
    } else {
        format!("{}d ago", diff.num_days())
    }
}
