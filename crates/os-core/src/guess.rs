//! A pure release-name parser (a focused `guessit`/`anitomy`-lite).
//!
//! Lives in `os-core` because it is pure logic with no I/O, and both the scorer
//! (release-string fusion) and the `os-identify` adapter use it. It handles both
//! western releases (`Show.S01E02.1080p.WEB-DL.x264-GRP`) and anime
//! (`[Group] Title - 01 (1080p) [HASH]`).

use crate::model::{MediaKind, ReleaseInfo};
use regex::Regex;
use std::sync::LazyLock;

/// Structured guess extracted from a release name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Guess {
    pub title: Option<String>,
    pub year: Option<i32>,
    pub season: Option<u32>,
    pub episodes: Vec<u32>,
    pub release_group: Option<String>,
    pub source: Option<String>,
    pub resolution: Option<String>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub is_episode: bool,
}

impl Guess {
    /// The release fields, for filling a `Media`.
    pub fn release_info(&self) -> ReleaseInfo {
        ReleaseInfo {
            release_group: self.release_group.clone(),
            source: self.source.clone(),
            resolution: self.resolution.clone(),
            video_codec: self.video_codec.clone(),
            audio_codec: self.audio_codec.clone(),
            streaming_service: None,
            edition: None,
        }
    }

    /// Inferred media kind: anime if it had a `[group]` prefix + absolute number,
    /// series if SxxEyy/NxNN, else movie.
    pub fn kind(&self, anime_hint: bool) -> MediaKind {
        if !self.is_episode {
            MediaKind::Movie
        } else if anime_hint {
            MediaKind::Anime
        } else {
            MediaKind::Series
        }
    }
}

static RE_SXXEYY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bS(\d{1,2})[\s._-]*E(\d{1,3})(?:[-E]+(\d{1,3}))?").unwrap());
static RE_NXNN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(\d{1,2})x(\d{1,3})\b").unwrap());
static RE_YEAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(19\d{2}|20\d{2})\b").unwrap());
static RE_RES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(\d{3,4})[pi]\b|\b(4k|2160p|1440p|1080p|720p|576p|480p)\b").unwrap()
});
static RE_SOURCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(blu-?ray|bd-?rip|bd-?remux|remux|web-?dl|web-?rip|web|hdtv|dvd-?rip|dvd|hd-?rip|hdcam|cam)\b")
        .unwrap()
});
static RE_VCODEC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(x264|h\.?264|avc|x265|h\.?265|hevc|av1|xvid|divx)\b").unwrap()
});
static RE_ACODEC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(aac|ac-?3|e-?ac-?3|ddp?5\.1|ddp|dd|flac|truehd|dts-?hd|dts|opus|mp3)\b")
        .unwrap()
});
// Anime absolute episode: after a title/group, ` - 01 ` (optionally v2), before space/paren/bracket/end.
static RE_ANIME_EP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\s-\s*(\d{1,4})(?:v\d)?(?:\s*(?:\(|\[|$|\s))").unwrap());
static RE_GROUP_BRACKET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*[\[(]([^\])]+)[\])]").unwrap());
static RE_GROUP_TRAILING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"-([A-Za-z0-9]+)\s*$").unwrap());

/// Strip a file extension if present.
fn strip_ext(name: &str) -> &str {
    for ext in [
        ".mkv", ".mp4", ".avi", ".m4v", ".mov", ".webm", ".ts", ".wmv", ".flv",
    ] {
        if let Some(s) = name.strip_suffix(ext) {
            return s;
        }
        // case-insensitive
        if name.to_ascii_lowercase().ends_with(ext) {
            return &name[..name.len() - ext.len()];
        }
    }
    name
}

/// Normalize separators (`.`/`_`) to spaces for title extraction.
fn humanize(s: &str) -> String {
    s.replace(['.', '_'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse a release name into a [`Guess`]. Never panics; returns best-effort.
pub fn guess(name: &str) -> Guess {
    let raw = strip_ext(name.trim());
    let mut g = Guess::default();

    // Anime/group bracket prefix → release_group + likely anime.
    let mut anime_like = false;
    if let Some(c) = RE_GROUP_BRACKET.captures(raw) {
        g.release_group = Some(c[1].trim().to_string());
        anime_like = true;
    }

    // Season/episode markers.
    if let Some(c) = RE_SXXEYY.captures(raw) {
        g.season = c.get(1).and_then(|m| m.as_str().parse().ok());
        if let Some(e1) = c.get(2).and_then(|m| m.as_str().parse::<u32>().ok()) {
            g.episodes.push(e1);
        }
        if let Some(e2) = c.get(3).and_then(|m| m.as_str().parse::<u32>().ok()) {
            g.episodes.push(e2);
        }
        g.is_episode = true;
    } else if let Some(c) = RE_NXNN.captures(raw) {
        g.season = c.get(1).and_then(|m| m.as_str().parse().ok());
        if let Some(e1) = c.get(2).and_then(|m| m.as_str().parse::<u32>().ok()) {
            g.episodes.push(e1);
        }
        g.is_episode = true;
    } else if anime_like {
        if let Some(c) = RE_ANIME_EP.captures(raw) {
            if let Some(e) = c.get(1).and_then(|m| m.as_str().parse::<u32>().ok()) {
                g.episodes.push(e);
                g.season = Some(1);
                g.is_episode = true;
            }
        }
    }

    // Year (skip if it's actually a resolution like 2160 — RE_YEAR only matches 19xx/20xx).
    if let Some(c) = RE_YEAR.captures(raw) {
        g.year = c.get(1).and_then(|m| m.as_str().parse().ok());
    }

    // Resolution.
    if let Some(c) = RE_RES.captures(raw) {
        let res = c
            .get(1)
            .map(|m| format!("{}p", m.as_str()))
            .or_else(|| c.get(2).map(|m| m.as_str().to_lowercase()));
        g.resolution = res.map(|r| {
            let r = r.to_lowercase();
            if r == "4k" {
                "2160p".to_string()
            } else {
                r
            }
        });
    }

    // Source / codecs.
    g.source = RE_SOURCE.captures(raw).map(|c| normalize_source(&c[1]));
    g.video_codec = RE_VCODEC.captures(raw).map(|c| normalize_vcodec(&c[1]));
    g.audio_codec = RE_ACODEC.captures(raw).map(|c| normalize_acodec(&c[1]));

    // Trailing `-GROUP` (western releases) if we didn't already get a bracket group.
    if g.release_group.is_none() {
        if let Some(c) = RE_GROUP_TRAILING.captures(raw) {
            let grp = c[1].trim();
            // Avoid catching codec/source fragments as a group.
            if grp.len() >= 2 && !grp.chars().all(|c| c.is_ascii_digit()) {
                g.release_group = Some(grp.to_string());
            }
        }
    }

    // Title = everything before the first strong marker.
    g.title = extract_title(raw, anime_like);

    g
}

fn extract_title(raw: &str, anime_like: bool) -> Option<String> {
    // Find the earliest marker position (SxxEyy / NxNN / anime " - NN" / year).
    let mut cut = raw.len();
    for re in [&*RE_SXXEYY, &*RE_NXNN] {
        if let Some(m) = re.find(raw) {
            cut = cut.min(m.start());
        }
    }
    if let Some(m) = RE_YEAR.find(raw) {
        cut = cut.min(m.start());
    }
    if anime_like {
        if let Some(m) = RE_ANIME_EP.find(raw) {
            cut = cut.min(m.start());
        }
    }

    let mut head = &raw[..cut];

    // For anime, drop the leading [group] bracket.
    if anime_like {
        if let Some(m) = RE_GROUP_BRACKET.find(head) {
            head = &head[m.end()..];
        }
    }

    let title = humanize(head)
        .trim_matches(|c: char| c == '-' || c == '_' || c.is_whitespace())
        .to_string();

    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

fn normalize_source(s: &str) -> String {
    let l = s.to_ascii_lowercase().replace('-', "");
    match l.as_str() {
        "bluray" | "blu ray" => "BluRay".into(),
        "bdrip" => "BDRip".into(),
        "bdremux" | "remux" => "Remux".into(),
        "webdl" => "WEB-DL".into(),
        "webrip" => "WEBRip".into(),
        "web" => "WEB".into(),
        "hdtv" => "HDTV".into(),
        "dvdrip" => "DVDRip".into(),
        "dvd" => "DVD".into(),
        "hdrip" => "HDRip".into(),
        other => other.to_uppercase(),
    }
}

fn normalize_vcodec(s: &str) -> String {
    let l = s.to_ascii_lowercase().replace('.', "");
    match l.as_str() {
        "x264" | "h264" | "avc" => "x264".into(),
        "x265" | "h265" | "hevc" => "x265".into(),
        "av1" => "AV1".into(),
        "xvid" => "XviD".into(),
        "divx" => "DivX".into(),
        other => other.to_uppercase(),
    }
}

fn normalize_acodec(s: &str) -> String {
    let l = s.to_ascii_lowercase().replace('-', "");
    match l.as_str() {
        "aac" => "AAC".into(),
        "ac3" => "AC3".into(),
        "eac3" => "EAC3".into(),
        "ddp" | "ddp5.1" => "DDP".into(),
        "dd" => "DD".into(),
        "flac" => "FLAC".into(),
        "truehd" => "TrueHD".into(),
        "dts" | "dtshd" => "DTS".into(),
        "opus" => "Opus".into(),
        "mp3" => "MP3".into(),
        other => other.to_uppercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn western_episode() {
        let g = guess("The.Show.S01E02.1080p.WEB-DL.x264-GRP.mkv");
        assert_eq!(g.title.as_deref(), Some("The Show"));
        assert_eq!(g.season, Some(1));
        assert_eq!(g.episodes, vec![2]);
        assert_eq!(g.resolution.as_deref(), Some("1080p"));
        assert_eq!(g.source.as_deref(), Some("WEB-DL"));
        assert_eq!(g.video_codec.as_deref(), Some("x264"));
        assert_eq!(g.release_group.as_deref(), Some("GRP"));
        assert!(g.is_episode);
    }

    #[test]
    fn movie_with_year() {
        let g = guess("Interstellar.2014.2160p.BluRay.x265.DTS-HD.mkv");
        assert_eq!(g.title.as_deref(), Some("Interstellar"));
        assert_eq!(g.year, Some(2014));
        assert_eq!(g.resolution.as_deref(), Some("2160p"));
        assert_eq!(g.source.as_deref(), Some("BluRay"));
        assert_eq!(g.video_codec.as_deref(), Some("x265"));
        assert!(!g.is_episode);
    }

    #[test]
    fn anime_release() {
        let g =
            guess("[SubsPlease] Kage no Jitsuryokusha ni Naritakute! - 01 (1080p) [8819B368].mkv");
        assert_eq!(
            g.title.as_deref(),
            Some("Kage no Jitsuryokusha ni Naritakute!")
        );
        assert_eq!(g.episodes, vec![1]);
        assert_eq!(g.season, Some(1));
        assert_eq!(g.resolution.as_deref(), Some("1080p"));
        assert_eq!(g.release_group.as_deref(), Some("SubsPlease"));
        assert!(g.is_episode);
    }

    #[test]
    fn nxnn_form() {
        let g = guess("Show Name 3x07 720p HDTV");
        assert_eq!(g.season, Some(3));
        assert_eq!(g.episodes, vec![7]);
        assert_eq!(g.resolution.as_deref(), Some("720p"));
    }

    #[test]
    fn multi_episode() {
        let g = guess("Show.S02E05E06.1080p.mkv");
        assert_eq!(g.season, Some(2));
        assert_eq!(g.episodes, vec![5, 6]);
    }
}
