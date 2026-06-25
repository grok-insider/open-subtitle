//! Two-layer matching + scoring (ported from subliminal, hardened with Bazarr's
//! rules). Providers/engine compute a *set of match tags*; a swappable scorer
//! maps the set → integer via a weights table. A hash match dominates but must
//! be corroborated; an ID match implies the identity fields it guarantees.
//!
//! All pure — exhaustively unit-tested with no I/O.

use crate::guess::{guess, Guess};
use crate::model::{Media, MediaKind, SubtitleCandidate};
use std::collections::BTreeSet;

/// An individual signal that a subtitle matches the target media.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Match {
    Hash,
    Series,
    Title,
    Year,
    Country,
    Season,
    Episode,
    ReleaseGroup,
    StreamingService,
    Source,
    Resolution,
    VideoCodec,
    AudioCodec,
    Fps,
    ImdbId,
    TmdbId,
    TvdbId,
    SeriesImdbId,
    SeriesTmdbId,
    SeriesTvdbId,
}

/// A computed score: with and without the hash contribution (for tie-breaking).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Score {
    pub score: i32,
    pub without_hash: i32,
}

/// Equivalent release groups (treated as the same group when matching).
const EQUIVALENT_GROUPS: &[&[&str]] = &[
    &["lol", "dimension"],
    &["asap", "immerse", "fleet"],
    &["avs", "sva"],
];

/// Weight for a match tag, per media kind. The invariant
/// `weight(Hash) == sum(weight(others))` makes a hash match tie the sum of every
/// other signal, so it dominates while combinations still rank intuitively.
pub fn weight(m: Match, kind: MediaKind) -> i32 {
    use Match::*;
    if kind.is_episodic() {
        match m {
            Hash => 971,
            Series => 486,
            Year => 162,
            Country => 162,
            Season => 54,
            Episode => 54,
            ReleaseGroup => 18,
            StreamingService => 18,
            Fps => 9,
            Source => 4,
            AudioCodec => 2,
            Resolution => 1,
            VideoCodec => 1,
            // ID matches are scored via equivalence expansion, not directly.
            Title | ImdbId | TmdbId | TvdbId | SeriesImdbId | SeriesTmdbId | SeriesTvdbId => 0,
        }
    } else {
        match m {
            Hash => 323,
            Title => 162,
            Year => 54,
            Country => 54,
            ReleaseGroup => 18,
            StreamingService => 18,
            Fps => 9,
            Source => 4,
            AudioCodec => 2,
            Resolution => 1,
            VideoCodec => 1,
            Series | Season | Episode | ImdbId | TmdbId | TvdbId | SeriesImdbId | SeriesTmdbId
            | SeriesTvdbId => 0,
        }
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn groups_equivalent(a: &str, b: &str) -> bool {
    let a = a.to_lowercase();
    let b = b.to_lowercase();
    if a == b {
        return true;
    }
    EQUIVALENT_GROUPS
        .iter()
        .any(|set| set.contains(&a.as_str()) && set.contains(&b.as_str()))
}

/// Compare a parsed release guess against the media, producing match tags.
pub fn guess_matches(media: &Media, g: &Guess) -> BTreeSet<Match> {
    let mut m = BTreeSet::new();

    // Title / series.
    if let Some(t) = &g.title {
        let st = sanitize(t);
        let candidates = std::iter::once(media.title.clone())
            .chain(media.alternative_titles.iter().cloned())
            .chain(media.original_title.iter().cloned());
        if candidates.map(|c| sanitize(&c)).any(|c| c == st) {
            if media.kind.is_episodic() {
                m.insert(Match::Series);
            } else {
                m.insert(Match::Title);
            }
        }
    }

    if media.kind.is_episodic() {
        if let (Some(gs), Some(ms)) = (g.season, media.season) {
            if gs == ms {
                m.insert(Match::Season);
            }
        }
        if !g.episodes.is_empty() && g.episodes == media.episodes {
            m.insert(Match::Episode);
        } else if let (Some(ge), Some(me)) = (g.episodes.iter().min(), media.episode_num()) {
            if *ge == me {
                m.insert(Match::Episode);
            }
        }
    }

    if let (Some(gy), Some(my)) = (g.year, media.year) {
        if gy == my {
            m.insert(Match::Year);
        }
    }

    if let (Some(gg), Some(mg)) = (&g.release_group, &media.release.release_group) {
        if groups_equivalent(gg, mg) {
            m.insert(Match::ReleaseGroup);
        }
    }
    if eq_opt(&g.source, &media.release.source) {
        m.insert(Match::Source);
    }
    if eq_opt(&g.resolution, &media.release.resolution) {
        m.insert(Match::Resolution);
    }
    if eq_opt(&g.video_codec, &media.release.video_codec) {
        m.insert(Match::VideoCodec);
    }
    if eq_opt(&g.audio_codec, &media.release.audio_codec) {
        m.insert(Match::AudioCodec);
    }

    m
}

fn eq_opt(a: &Option<String>, b: &Option<String>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        _ => false,
    }
}

/// Build the full match set for a candidate against the media:
/// hash assertion + release-string fusion + structured ID hints.
pub fn candidate_matches(c: &SubtitleCandidate, media: &Media) -> BTreeSet<Match> {
    let mut m = BTreeSet::new();

    if c.matched_by_hash {
        m.insert(Match::Hash);
    }

    if let Some(rel) = &c.release {
        m.extend(guess_matches(media, &guess(rel)));
    }

    // Structured hints the provider filled in (exact equality).
    if let (Some(h), Some(mi)) = (c.hints.get("imdb"), &media.ids.imdb) {
        if normalize_imdb(h) == normalize_imdb(mi) {
            m.insert(Match::ImdbId);
        }
    }
    if let (Some(h), Some(mi)) = (c.hints.get("series_imdb"), &media.ids.series_imdb) {
        if normalize_imdb(h) == normalize_imdb(mi) {
            m.insert(Match::SeriesImdbId);
        }
    }
    if let (Some(h), Some(mt)) = (c.hints.get("tmdb"), &media.ids.tmdb) {
        if h == &mt.to_string() {
            m.insert(Match::TmdbId);
        }
    }
    if let (Some(h), Some(mt)) = (c.hints.get("tvdb"), &media.ids.tvdb) {
        if h == &mt.to_string() {
            m.insert(Match::TvdbId);
        }
    }
    if media.kind.is_episodic() {
        if let (Some(h), Some(ms)) = (c.hints.get("season"), media.season) {
            if h.parse::<u32>().ok() == Some(ms) {
                m.insert(Match::Season);
            }
        }
        if let (Some(h), Some(me)) = (c.hints.get("episode"), media.episode_num()) {
            if h.parse::<u32>().ok() == Some(me) {
                m.insert(Match::Episode);
            }
        }
        // A provider that searched by series id implies a series match.
        if c.hints.get("series_matched").map(|v| v == "true") == Some(true) {
            m.insert(Match::Series);
        }
    }

    m
}

fn normalize_imdb(s: &str) -> String {
    s.trim_start_matches("tt")
        .trim_start_matches('0')
        .to_string()
}

/// Expand ID matches into the identity fields they guarantee.
pub fn expand_equivalences(m: &mut BTreeSet<Match>, kind: MediaKind) {
    use Match::*;
    if kind.is_episodic() {
        if m.contains(&ImdbId) {
            m.extend([Series, Year, Country, Season, Episode]);
        }
        if m.contains(&SeriesImdbId) {
            m.extend([Series, Year, Country]);
        }
        if m.contains(&TvdbId) {
            m.extend([Series, Year, Season, Episode]);
        }
        if m.contains(&SeriesTvdbId) {
            m.extend([Series, Year]);
        }
        if m.contains(&TmdbId) {
            m.extend([Series, Year, Country, Season, Episode]);
        }
        if m.contains(&SeriesTmdbId) {
            m.extend([Series, Year, Country]);
        }
        // An episode-title match implies the episode.
        if m.contains(&Title) {
            m.insert(Episode);
        }
    } else {
        if m.contains(&ImdbId) {
            m.extend([Title, Year, Country]);
        }
        if m.contains(&TmdbId) {
            m.extend([Title, Year, Country]);
        }
    }
}

/// Sum the weights of a match set for a media kind.
fn score_set(m: &BTreeSet<Match>, kind: MediaKind) -> i32 {
    m.iter().map(|&x| weight(x, kind)).sum()
}

/// A hash match is only trusted if structural signals corroborate it.
fn hash_corroborated(m: &BTreeSet<Match>, kind: MediaKind) -> bool {
    use Match::*;
    if kind.is_episodic() {
        m.contains(&Season)
            && m.contains(&Episode)
            && (m.contains(&Series) || m.contains(&ImdbId) || m.contains(&TvdbId))
    } else {
        m.contains(&Title) || m.contains(&ImdbId) || m.contains(&TmdbId)
    }
}

/// The episode safety gate: don't accept an episode sub that doesn't actually
/// pin the season+episode of the right series. Prevents cross-series false
/// positives that pure scoring can miss.
pub fn series_safety_ok(m: &BTreeSet<Match>, kind: MediaKind) -> bool {
    use Match::*;
    if !kind.is_episodic() {
        return true;
    }
    m.contains(&Season)
        && m.contains(&Episode)
        && (m.contains(&Series) || m.contains(&ImdbId) || m.contains(&TvdbId))
}

/// Compute the score for a candidate against the media.
pub fn compute_score(c: &SubtitleCandidate, media: &Media) -> Score {
    let mut m = candidate_matches(c, media);
    expand_equivalences(&mut m, media.kind);

    let mut without = m.clone();
    without.remove(&Match::Hash);
    let without_hash = score_set(&without, media.kind);

    let score = if m.contains(&Match::Hash) {
        if hash_corroborated(&m, media.kind) {
            // Hash dominates: it ties the sum of everything else.
            weight(Match::Hash, media.kind)
        } else {
            // Drop an uncorroborated hash.
            without_hash
        }
    } else {
        score_set(&m, media.kind)
    };

    let max = weight(Match::Hash, media.kind);
    Score {
        score: score.clamp(0, max),
        without_hash,
    }
}

/// The maximum achievable score for a media kind, excluding hash (used to report
/// `min_score` percentages).
pub fn max_score_without_hash(kind: MediaKind) -> i32 {
    weight(Match::Hash, kind) // == sum of all non-hash weights by construction
}

/// Whether a candidate clears the episode safety gate for the media (always true
/// for movies). Used by the engine before accepting an episode subtitle.
pub fn passes_series_safety(c: &SubtitleCandidate, media: &Media) -> bool {
    let mut m = candidate_matches(c, media);
    expand_equivalences(&mut m, media.kind);
    series_safety_ok(&m, media.kind)
}

/// The default scorer port impl.
#[derive(Debug, Clone, Default)]
pub struct WeightedScorer;

impl crate::ports::Scorer for WeightedScorer {
    fn score(&self, candidate: &SubtitleCandidate, media: &Media) -> Score {
        compute_score(candidate, media)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{IdSet, Media};
    use std::collections::BTreeMap;

    fn cand(release: &str) -> SubtitleCandidate {
        let mut c =
            SubtitleCandidate::new("test", "1", crate::lang::Language::parse("en").unwrap());
        c.release = Some(release.into());
        c
    }

    #[test]
    fn hash_weight_equals_sum_of_others_episode() {
        // The core invariant: hash ties the sum of every other signal.
        let others = [
            Match::Series,
            Match::Year,
            Match::Country,
            Match::Season,
            Match::Episode,
            Match::ReleaseGroup,
            Match::StreamingService,
            Match::Fps,
            Match::Source,
            Match::AudioCodec,
            Match::Resolution,
            Match::VideoCodec,
        ];
        let sum: i32 = others.iter().map(|&m| weight(m, MediaKind::Series)).sum();
        assert_eq!(sum, weight(Match::Hash, MediaKind::Series));
    }

    #[test]
    fn hash_weight_equals_sum_of_others_movie() {
        let others = [
            Match::Title,
            Match::Year,
            Match::Country,
            Match::ReleaseGroup,
            Match::StreamingService,
            Match::Fps,
            Match::Source,
            Match::AudioCodec,
            Match::Resolution,
            Match::VideoCodec,
        ];
        let sum: i32 = others.iter().map(|&m| weight(m, MediaKind::Movie)).sum();
        assert_eq!(sum, weight(Match::Hash, MediaKind::Movie));
    }

    #[test]
    fn series_episode_ranks_above_release_details() {
        let mut media = Media::episode("The Show", 1, 2);
        media.release.resolution = Some("1080p".into());
        let strong = cand("The.Show.S01E02.1080p.WEB-DL.x264-GRP");
        let weak = cand("Other.Show.S01E02.1080p");
        assert!(compute_score(&strong, &media).score > compute_score(&weak, &media).score);
    }

    #[test]
    fn uncorroborated_hash_is_dropped() {
        let media = Media::episode("The Show", 1, 2);
        let mut c = cand("Totally.Unrelated.Name");
        c.matched_by_hash = true;
        // Hash present but nothing corroborates → falls back to without-hash score.
        let s = compute_score(&c, &media);
        assert_eq!(s.score, s.without_hash);
        assert!(s.score < weight(Match::Hash, MediaKind::Series));
    }

    #[test]
    fn corroborated_hash_dominates() {
        let media = Media::episode("The Show", 1, 2);
        let mut c = cand("The.Show.S01E02.720p");
        c.matched_by_hash = true;
        let s = compute_score(&c, &media);
        assert_eq!(s.score, weight(Match::Hash, MediaKind::Series));
    }

    #[test]
    fn imdb_match_implies_identity_fields() {
        let mut media = Media::movie("Interstellar");
        media.year = Some(2014);
        media.ids = IdSet {
            imdb: Some("0816692".into()),
            ..Default::default()
        };
        let mut c = SubtitleCandidate::new("t", "1", crate::lang::Language::parse("en").unwrap());
        c.hints = BTreeMap::from([("imdb".to_string(), "tt0816692".to_string())]);
        let mut m = candidate_matches(&c, &media);
        expand_equivalences(&mut m, media.kind);
        assert!(m.contains(&Match::Title));
        assert!(m.contains(&Match::Year));
    }

    #[test]
    fn series_safety_gate_blocks_partial() {
        // Only season matched, no episode/series → gate fails.
        let mut m = BTreeSet::new();
        m.insert(Match::Season);
        assert!(!series_safety_ok(&m, MediaKind::Series));
        m.insert(Match::Episode);
        m.insert(Match::Series);
        assert!(series_safety_ok(&m, MediaKind::Series));
    }
}
