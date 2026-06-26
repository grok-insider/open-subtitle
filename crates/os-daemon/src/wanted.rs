//! The persistent **wanted list**: imports/scans that still lack a subtitle,
//! plus the bookkeeping the scheduler uses to re-search them on a timer.
//!
//! Anime fansubs (and slow movie releases) often lag, so a one-shot fetch on
//! import frequently comes up empty. We record the gap and let `ostd` retry
//! until found. The backlog is small, so an in-memory `Vec` guarded by a
//! `Mutex` and rewritten to a single JSON file on each change is plenty — and
//! keeps the daemon free of a database dependency. All mutation goes through
//! [`WantedStore`], whose pure merge / [`due`](WantedStore::due) /
//! [`record_result`](WantedStore::record_result) logic is unit-tested.

use os_core::MediaKind;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

/// Seconds since the Unix epoch.
pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// One tracked media item still missing one or more subtitle languages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WantedItem {
    /// Stable dedupe key (`path:<p>` when a file is known, else a query key).
    pub key: String,
    /// Absolute media file path, when known (used to re-hash + place sidecars).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Free-text query, when there is no reachable file (remote webhook path).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    pub kind: MediaKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub season: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode: Option<u32>,
    // Authoritative ids carried from a webhook payload, overlaid onto identify().
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imdb: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tmdb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_imdb: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_tvdb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub episode_title: Option<String>,
    /// Languages still wanted (canonical alpha-2 tags); pruned as delivered.
    pub languages: Vec<String>,
    /// Human label for display/logging.
    pub label: String,
    /// Origin: `webhook:sonarr` | `webhook:radarr` | `scan` | `api`.
    pub source: String,
    pub added_at: u64,
    /// Unix secs of the last re-search attempt (`0` = never tried yet).
    pub last_attempt: u64,
    pub attempts: u32,
}

impl WantedItem {
    /// Derive the dedupe key: the path when present, else `title|kind|S|E`.
    pub fn make_key(
        path: Option<&str>,
        query: Option<&str>,
        kind: MediaKind,
        season: Option<u32>,
        episode: Option<u32>,
    ) -> String {
        match path.filter(|p| !p.is_empty()) {
            Some(p) => format!("path:{p}"),
            None => format!(
                "q:{}|{:?}|{}|{}",
                query.unwrap_or("").to_lowercase(),
                kind,
                season.unwrap_or(0),
                episode.unwrap_or(0)
            ),
        }
    }

    /// Fold improved metadata + new languages from `new` into `self`, preserving
    /// the attempt history (`added_at`/`last_attempt`/`attempts`).
    fn merge_from(&mut self, new: WantedItem) {
        for l in new.languages {
            if !self.languages.iter().any(|e| e.eq_ignore_ascii_case(&l)) {
                self.languages.push(l);
            }
        }
        if new.path.is_some() {
            self.path = new.path;
        }
        if new.query.is_some() {
            self.query = new.query;
        }
        if new.imdb.is_some() {
            self.imdb = new.imdb;
        }
        if new.tmdb.is_some() {
            self.tmdb = new.tmdb;
        }
        if new.series_imdb.is_some() {
            self.series_imdb = new.series_imdb;
        }
        if new.series_tvdb.is_some() {
            self.series_tvdb = new.series_tvdb;
        }
        if new.episode_title.is_some() {
            self.episode_title = new.episode_title;
        }
        if !new.label.is_empty() {
            self.label = new.label;
        }
        self.source = new.source;
    }
}

/// The persistent wanted list, backed by a JSON file.
pub struct WantedStore {
    path: PathBuf,
    items: Mutex<Vec<WantedItem>>,
}

impl WantedStore {
    /// Load from `path`, tolerating a missing or corrupt file (→ empty list).
    pub fn load(path: PathBuf) -> WantedStore {
        let items = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str::<Vec<WantedItem>>(&t).ok())
            .unwrap_or_default();
        WantedStore {
            path,
            items: Mutex::new(items),
        }
    }

    pub fn len(&self) -> usize {
        self.items.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// A snapshot of the current list.
    pub fn list(&self) -> Vec<WantedItem> {
        self.items.lock().unwrap().clone()
    }

    /// Insert a new item, or merge into the existing one with the same key.
    pub fn upsert(&self, item: WantedItem) {
        {
            let mut items = self.items.lock().unwrap();
            if let Some(existing) = items.iter_mut().find(|i| i.key == item.key) {
                existing.merge_from(item);
            } else {
                items.push(item);
            }
        }
        self.persist();
    }

    /// Items eligible for a re-search pass: aged past `interval` seconds and
    /// under the attempt cap (`max_attempts == 0` means unlimited).
    pub fn due(&self, now: u64, interval: u64, max_attempts: u32) -> Vec<WantedItem> {
        self.items
            .lock()
            .unwrap()
            .iter()
            .filter(|i| now.saturating_sub(i.last_attempt) >= interval)
            .filter(|i| max_attempts == 0 || i.attempts < max_attempts)
            .cloned()
            .collect()
    }

    /// Record the outcome of a re-search: drop the `delivered` languages
    /// (alpha-2 tags), bump the attempt bookkeeping, and remove the item once
    /// nothing is left wanted. Returns `true` when the item is fully satisfied.
    pub fn record_result(&self, key: &str, delivered: &[String], now: u64) -> bool {
        let satisfied = {
            let mut items = self.items.lock().unwrap();
            let Some(idx) = items.iter().position(|i| i.key == key) else {
                return false;
            };
            let it = &mut items[idx];
            it.attempts += 1;
            it.last_attempt = now;
            it.languages
                .retain(|l| !delivered.iter().any(|d| d.eq_ignore_ascii_case(l)));
            if it.languages.is_empty() {
                items.remove(idx);
                true
            } else {
                false
            }
        };
        self.persist();
        satisfied
    }

    /// Remove an item by key. Returns `true` if one was removed.
    pub fn remove(&self, key: &str) -> bool {
        let removed = {
            let mut items = self.items.lock().unwrap();
            let before = items.len();
            items.retain(|i| i.key != key);
            items.len() != before
        };
        self.persist();
        removed
    }

    /// Drop every item.
    pub fn clear(&self) {
        self.items.lock().unwrap().clear();
        self.persist();
    }

    /// Write the list to disk (temp file + rename for crash-safety). Best-effort:
    /// a persistence failure is logged, not fatal — the in-memory list is truth
    /// for the running daemon.
    fn persist(&self) {
        let snapshot = { self.items.lock().unwrap().clone() };
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = match serde_json::to_string_pretty(&snapshot) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("wanted: serialize failed: {e}");
                return;
            }
        };
        let tmp = self.path.with_extension("json.tmp");
        if std::fs::write(&tmp, json.as_bytes()).is_ok() {
            let _ = std::fs::rename(&tmp, &self.path);
        } else {
            eprintln!("wanted: write failed: {}", self.path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(key: &str, langs: &[&str]) -> WantedItem {
        WantedItem {
            key: key.into(),
            path: Some(key.into()),
            query: None,
            kind: MediaKind::Series,
            season: Some(1),
            episode: Some(2),
            imdb: None,
            tmdb: None,
            series_imdb: None,
            series_tvdb: None,
            episode_title: None,
            languages: langs.iter().map(|s| s.to_string()).collect(),
            label: "Show - S01E02".into(),
            source: "scan".into(),
            added_at: 100,
            last_attempt: 0,
            attempts: 0,
        }
    }

    fn temp_path(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("os-wanted-{tag}-{nanos}.json"))
    }

    #[test]
    fn make_key_prefers_path_then_query() {
        assert_eq!(
            WantedItem::make_key(Some("/a/b.mkv"), Some("x"), MediaKind::Movie, None, None),
            "path:/a/b.mkv"
        );
        let k = WantedItem::make_key(None, Some("The Show"), MediaKind::Series, Some(1), Some(3));
        assert_eq!(k, "q:the show|Series|1|3");
    }

    #[test]
    fn upsert_merges_languages_without_duplicates() {
        let store = WantedStore::load(temp_path("merge"));
        store.upsert(item("path:/x.mkv", &["en"]));
        store.upsert(item("path:/x.mkv", &["en", "es"]));
        let all = store.list();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].languages, vec!["en", "es"]);
        let _ = std::fs::remove_file(&store.path);
    }

    #[test]
    fn due_respects_interval_and_attempt_cap() {
        let store = WantedStore::load(temp_path("due"));
        let mut a = item("path:/a.mkv", &["en"]);
        a.last_attempt = 1_000;
        a.attempts = 2;
        store.upsert(a);
        let mut b = item("path:/b.mkv", &["en"]);
        b.last_attempt = 0; // never tried → always due
        store.upsert(b);

        // now=1500, interval=600: a (last 1000 → age 500 < 600) not due; b due.
        let due = store.due(1_500, 600, 0);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].key, "path:/b.mkv");

        // now=2000: a age 1000 ≥ 600 → due, but cap=2 and attempts=2 → excluded.
        let due = store.due(2_000, 600, 2);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].key, "path:/b.mkv");
        // Unlimited cap (0) lets a through.
        assert_eq!(store.due(2_000, 600, 0).len(), 2);
        let _ = std::fs::remove_file(&store.path);
    }

    #[test]
    fn record_result_prunes_languages_and_removes_when_empty() {
        let store = WantedStore::load(temp_path("record"));
        store.upsert(item("path:/a.mkv", &["en", "es"]));

        // Deliver English (case-insensitive): item stays, es remains, attempt++.
        let satisfied = store.record_result("path:/a.mkv", &["EN".into()], 1_234);
        assert!(!satisfied);
        let it = &store.list()[0];
        assert_eq!(it.languages, vec!["es"]);
        assert_eq!(it.attempts, 1);
        assert_eq!(it.last_attempt, 1_234);

        // Deliver the last language → item removed entirely.
        let satisfied = store.record_result("path:/a.mkv", &["es".into()], 2_000);
        assert!(satisfied);
        assert!(store.is_empty());
        let _ = std::fs::remove_file(&store.path);
    }

    #[test]
    fn record_result_unknown_key_is_noop() {
        let store = WantedStore::load(temp_path("noop"));
        store.upsert(item("path:/a.mkv", &["en"]));
        assert!(!store.record_result("path:/missing.mkv", &["en".into()], 1));
        assert_eq!(store.len(), 1);
        let _ = std::fs::remove_file(&store.path);
    }

    #[test]
    fn persistence_roundtrips_through_disk() {
        let path = temp_path("persist");
        {
            let store = WantedStore::load(path.clone());
            store.upsert(item("path:/a.mkv", &["en"]));
            store.upsert(item("path:/b.mkv", &["ja"]));
        }
        // Reload from disk: both items survive.
        let reloaded = WantedStore::load(path.clone());
        let mut keys: Vec<String> = reloaded.list().into_iter().map(|i| i.key).collect();
        keys.sort();
        assert_eq!(keys, vec!["path:/a.mkv", "path:/b.mkv"]);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn clear_and_remove() {
        let store = WantedStore::load(temp_path("clear"));
        store.upsert(item("path:/a.mkv", &["en"]));
        store.upsert(item("path:/b.mkv", &["en"]));
        assert!(store.remove("path:/a.mkv"));
        assert!(!store.remove("path:/a.mkv")); // already gone
        assert_eq!(store.len(), 1);
        store.clear();
        assert!(store.is_empty());
        let _ = std::fs::remove_file(&store.path);
    }

    #[test]
    fn load_missing_file_is_empty() {
        let store = WantedStore::load(temp_path("absent"));
        assert!(store.is_empty());
    }
}
