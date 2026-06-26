//! Library scanning helpers: find video files on disk and work out which
//! subtitle languages they still lack.
//!
//! These are pure local-filesystem functions (no network, no engine state) so
//! they can be shared by the `ost scan` CLI command, the daemon's `POST /scan`
//! endpoint, and the wanted-list scheduler. Keeping the walk + sidecar-detection
//! logic in one tested place is what makes those three call sites behave
//! identically.

use os_core::Language;
use std::path::{Path, PathBuf};

/// Video container extensions we treat as "media files" worth subtitling.
const VIDEO_EXTS: &[&str] = &[
    "mkv", "mp4", "avi", "mov", "m4v", "wmv", "flv", "webm", "mpg", "mpeg", "ts", "m2ts", "ogv",
    "ogm", "vob", "3gp", "divx", "mts",
];

/// Subtitle extensions we treat as existing sidecars.
const SUB_EXTS: &[&str] = &["srt", "ass", "ssa", "vtt", "sub", "ttml"];

fn ext_lower(path: &Path) -> Option<String> {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
}

/// Whether `path` looks like a video file by extension (case-insensitive).
pub fn is_video_file(path: &Path) -> bool {
    ext_lower(path)
        .map(|e| VIDEO_EXTS.contains(&e.as_str()))
        .unwrap_or(false)
}

/// Whether `path` looks like a subtitle file by extension (case-insensitive).
pub fn is_subtitle_file(path: &Path) -> bool {
    ext_lower(path)
        .map(|e| SUB_EXTS.contains(&e.as_str()))
        .unwrap_or(false)
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.'))
        .unwrap_or(false)
}

/// Collect video files under `root`, sorted for deterministic output.
///
/// If `root` is itself a video file it is returned on its own. Directories
/// whose name starts with `.` are skipped; unreadable entries are ignored
/// rather than aborting the walk.
pub fn walk_videos(root: &Path, recursive: bool) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if root.is_file() {
        if is_video_file(root) {
            out.push(root.to_path_buf());
        }
        return out;
    }
    walk_dir(root, recursive, &mut out);
    out.sort();
    out
}

fn walk_dir(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            if recursive && !is_hidden(&path) {
                walk_dir(&path, recursive, out);
            }
        } else if file_type.is_file() && is_video_file(&path) {
            out.push(path);
        }
    }
}

/// The subtitle languages already present as sidecars next to `video`.
///
/// A sidecar is any sibling file that shares the video's stem and has a subtitle
/// extension (`Movie.en.srt`, `Movie.pt-BR.hi.srt`, …). By the common
/// Plex/Jellyfin/Bazarr convention — and exactly how this engine writes them via
/// [`os_core::SubtitleFile::sidecar_name`] — the language is the **first**
/// dot-token after the stem, so trailing `hi`/`forced` flags don't get
/// misread (e.g. `.en.hi.srt` is English, not Hindi). An untagged sidecar
/// (`Movie.srt`) carries no language and matches nothing.
pub fn existing_sub_languages(video: &Path) -> Vec<Language> {
    let (Some(dir), Some(stem)) = (video.parent(), video.file_stem().and_then(|s| s.to_str()))
    else {
        return Vec::new();
    };
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut langs: Vec<Language> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_subtitle_file(&path) {
            continue;
        }
        let Some(fname) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        // Sidecar must share the video stem, with a `.` separator after it
        // (so `Movie.en.srt` matches `Movie` but `MovieClip.en.srt` doesn't).
        let Some(rest) = fname.strip_prefix(stem).and_then(|r| r.strip_prefix('.')) else {
            continue;
        };
        // `rest` is now e.g. `en.srt`, `pt-BR.hi.srt`, or just `srt` (untagged).
        // Drop the trailing extension token, then read the language from the
        // first remaining tag (so untagged `Movie.srt` yields nothing, and the
        // extension itself is never mistaken for a 3-letter language code).
        let mut tags: Vec<&str> = rest.split('.').filter(|t| !t.is_empty()).collect();
        tags.pop();
        let Some(first) = tags.first() else { continue };
        if let Some(lang) = Language::parse(first) {
            if !langs.iter().any(|l| l.same_language(&lang)) {
                langs.push(lang);
            }
        }
    }
    langs
}

/// Of the `wanted` languages, those with no matching sidecar next to `video`.
///
/// Matching ignores region/flags (a wanted `pt-BR` is satisfied by an existing
/// `pt` sidecar) so the scan doesn't re-fetch near-duplicates.
pub fn missing_languages(video: &Path, wanted: &[Language]) -> Vec<Language> {
    let present = existing_sub_languages(video);
    wanted
        .iter()
        .filter(|w| !present.iter().any(|p| p.same_language(w)))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// A throwaway directory that cleans itself up on drop.
    struct TempTree(PathBuf);
    impl TempTree {
        fn new(tag: &str) -> TempTree {
            let mut p = std::env::temp_dir();
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            p.push(format!("os-engine-lib-{tag}-{nanos}"));
            fs::create_dir_all(&p).unwrap();
            TempTree(p)
        }
        fn touch(&self, rel: &str) {
            let full = self.0.join(rel);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&full, b"x").unwrap();
        }
        fn path(&self, rel: &str) -> PathBuf {
            self.0.join(rel)
        }
    }
    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn lang(code: &str) -> Language {
        Language::parse(code).unwrap()
    }

    #[test]
    fn detects_video_and_subtitle_extensions() {
        assert!(is_video_file(Path::new("A/B/Show.S01E01.mkv")));
        assert!(is_video_file(Path::new("movie.MP4"))); // case-insensitive
        assert!(!is_video_file(Path::new("notes.txt")));
        assert!(!is_video_file(Path::new("subtitle.srt")));
        assert!(is_subtitle_file(Path::new("movie.en.SRT")));
        assert!(!is_subtitle_file(Path::new("movie.mkv")));
    }

    #[test]
    fn walk_is_recursive_sorted_and_skips_hidden() {
        let t = TempTree::new("walk");
        t.touch("a.mkv");
        t.touch("sub/b.mp4");
        t.touch("sub/deep/c.avi");
        t.touch("sub/notes.txt");
        t.touch(".hidden/d.mkv"); // hidden dir → skipped
        t.touch("poster.jpg");

        let mut got = walk_videos(&t.0, true);
        got.sort();
        let names: Vec<String> = got
            .iter()
            .map(|p| p.strip_prefix(&t.0).unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().any(|n| n.ends_with("a.mkv")));
        assert!(names.iter().any(|n| n.ends_with("b.mp4")));
        assert!(names.iter().any(|n| n.ends_with("c.avi")));
        assert_eq!(names.len(), 3, "only videos, hidden dir skipped: {names:?}");
    }

    #[test]
    fn walk_non_recursive_stays_shallow() {
        let t = TempTree::new("shallow");
        t.touch("top.mkv");
        t.touch("nested/deep.mkv");
        let got = walk_videos(&t.0, false);
        assert_eq!(got.len(), 1);
        assert!(got[0].ends_with("top.mkv"));
    }

    #[test]
    fn walk_single_file() {
        let t = TempTree::new("single");
        t.touch("only.mkv");
        let got = walk_videos(&t.path("only.mkv"), true);
        assert_eq!(got.len(), 1);
        // A non-video single file yields nothing.
        t.touch("readme.txt");
        assert!(walk_videos(&t.path("readme.txt"), true).is_empty());
    }

    #[test]
    fn existing_languages_reads_first_tag_only() {
        let t = TempTree::new("present");
        t.touch("Movie.mkv");
        t.touch("Movie.en.srt");
        t.touch("Movie.es.hi.srt"); // Spanish + HI flag — must read as Spanish
        t.touch("Movie.pt-BR.ass");
        t.touch("Movie.srt"); // untagged — matches nothing
        t.touch("Other.fr.srt"); // different stem — ignored

        let present = existing_sub_languages(&t.path("Movie.mkv"));
        let mut codes: Vec<String> = present.iter().map(|l| l.alpha2()).collect();
        codes.sort();
        assert_eq!(codes, vec!["en", "es", "pt"]);
        // Crucially, the `.hi` flag in `Movie.es.hi.srt` was NOT read as Hindi.
        assert!(!present.iter().any(|l| l.alpha2() == "hi"));
    }

    #[test]
    fn missing_languages_ignores_region_and_flags() {
        let t = TempTree::new("missing");
        t.touch("Ep.mkv");
        t.touch("Ep.en.srt");
        t.touch("Ep.pt.srt");

        let wanted = vec![lang("en"), lang("pt-BR"), lang("es")];
        let missing = missing_languages(&t.path("Ep.mkv"), &wanted);
        // en present; pt-BR satisfied by pt; only es missing.
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].alpha2(), "es");
    }

    #[test]
    fn missing_languages_all_when_no_sidecars() {
        let t = TempTree::new("none");
        t.touch("Bare.mkv");
        let wanted = vec![lang("en"), lang("ja")];
        let missing = missing_languages(&t.path("Bare.mkv"), &wanted);
        assert_eq!(missing.len(), 2);
    }
}
