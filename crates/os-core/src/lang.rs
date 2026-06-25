//! Subtitle language with ISO 639 bridging.
//!
//! Different providers want different code shapes: OpenSubtitles' legacy
//! `sublanguageid` wants ISO 639-2/B (bibliographic, e.g. `fre`, `ger`, `chi`),
//! the modern REST API wants ISO 639-1 (`fr`, `de`, `zh`), and anime sources
//! often use `ja`/`jpn`. We normalize to a canonical ISO 639-3/2T code and
//! expose the variants each provider needs, plus region (`pt-BR`) and the
//! hearing-impaired / forced flags.

use serde::{Deserialize, Serialize};

/// A subtitle language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Language {
    /// Canonical ISO 639-3 / 639-2T code (e.g. `eng`, `spa`, `jpn`, `por`).
    pub alpha3: String,
    /// Optional region/country subtag (e.g. `BR`, `419`, `ES`).
    pub region: Option<String>,
    /// Hearing-impaired (SDH) variant.
    pub hearing_impaired: bool,
    /// Forced (foreign-parts-only) variant.
    pub forced: bool,
}

// (alpha2, alpha3-T, alpha3-B, English name)
const TABLE: &[(&str, &str, &str, &str)] = &[
    ("en", "eng", "eng", "English"),
    ("es", "spa", "spa", "Spanish"),
    ("pt", "por", "por", "Portuguese"),
    ("fr", "fra", "fre", "French"),
    ("de", "deu", "ger", "German"),
    ("it", "ita", "ita", "Italian"),
    ("ja", "jpn", "jpn", "Japanese"),
    ("zh", "zho", "chi", "Chinese"),
    ("ko", "kor", "kor", "Korean"),
    ("ru", "rus", "rus", "Russian"),
    ("ar", "ara", "ara", "Arabic"),
    ("nl", "nld", "dut", "Dutch"),
    ("pl", "pol", "pol", "Polish"),
    ("tr", "tur", "tur", "Turkish"),
    ("sv", "swe", "swe", "Swedish"),
    ("no", "nor", "nor", "Norwegian"),
    ("da", "dan", "dan", "Danish"),
    ("fi", "fin", "fin", "Finnish"),
    ("cs", "ces", "cze", "Czech"),
    ("el", "ell", "gre", "Greek"),
    ("he", "heb", "heb", "Hebrew"),
    ("hi", "hin", "hin", "Hindi"),
    ("id", "ind", "ind", "Indonesian"),
    ("th", "tha", "tha", "Thai"),
    ("vi", "vie", "vie", "Vietnamese"),
    ("uk", "ukr", "ukr", "Ukrainian"),
    ("ro", "ron", "rum", "Romanian"),
    ("hu", "hun", "hun", "Hungarian"),
    ("bg", "bul", "bul", "Bulgarian"),
    ("sr", "srp", "srp", "Serbian"),
    ("hr", "hrv", "hrv", "Croatian"),
    ("fa", "fas", "per", "Persian"),
    ("ca", "cat", "cat", "Catalan"),
];

fn lookup(code: &str) -> Option<&'static (&'static str, &'static str, &'static str, &'static str)> {
    let c = code.to_ascii_lowercase();
    TABLE
        .iter()
        .find(|(a2, a3t, a3b, _)| *a2 == c || *a3t == c || *a3b == c)
}

impl Language {
    /// Parse a language tag like `en`, `eng`, `pt-BR`, `es-419`, `pob`
    /// (Brazilian Portuguese alias), optionally with `:hi` / `:forced` suffixes.
    pub fn parse(input: &str) -> Option<Language> {
        let mut hearing_impaired = false;
        let mut forced = false;
        let mut base = input.trim();

        // Strip trailing flag suffixes (any order): foo:hi:forced
        loop {
            let lower = base.to_ascii_lowercase();
            if let Some(rest) = lower
                .strip_suffix(":hi")
                .or_else(|| lower.strip_suffix(":sdh"))
            {
                hearing_impaired = true;
                base = &base[..rest.len()];
            } else if let Some(rest) = lower.strip_suffix(":forced") {
                forced = true;
                base = &base[..rest.len()];
            } else {
                break;
            }
        }

        // Brazilian Portuguese aliases used by OpenSubtitles.
        let lower = base.to_ascii_lowercase();
        if lower == "pob" || lower == "pt-br" || lower == "pt_br" {
            return Some(Language {
                alpha3: "por".into(),
                region: Some("BR".into()),
                hearing_impaired,
                forced,
            });
        }

        let (code, region) = match base.split_once(['-', '_']) {
            Some((c, r)) => (c, Some(r.to_ascii_uppercase())),
            None => (base, None),
        };

        let alpha3 = lookup(code).map(|t| t.1.to_string()).or_else(|| {
            // Accept any plausible 3-letter code we don't know, lowercased.
            if code.len() == 3 && code.chars().all(|c| c.is_ascii_alphabetic()) {
                Some(code.to_ascii_lowercase())
            } else {
                None
            }
        })?;

        Some(Language {
            alpha3,
            region,
            hearing_impaired,
            forced,
        })
    }

    /// ISO 639-1 code where known (e.g. `en`), else the alpha-3 code.
    pub fn alpha2(&self) -> String {
        lookup(&self.alpha3)
            .map(|t| t.0.to_string())
            .unwrap_or_else(|| self.alpha3.clone())
    }

    /// ISO 639-2/B (bibliographic) code (e.g. `fre`), for legacy OpenSubtitles.
    pub fn alpha3b(&self) -> String {
        // Brazilian Portuguese has a dedicated legacy id.
        if self.alpha3 == "por" && self.region.as_deref() == Some("BR") {
            return "pob".into();
        }
        lookup(&self.alpha3)
            .map(|t| t.2.to_string())
            .unwrap_or_else(|| self.alpha3.clone())
    }

    /// English display name where known, else the alpha-3 code.
    pub fn name(&self) -> String {
        lookup(&self.alpha3)
            .map(|t| t.3.to_string())
            .unwrap_or_else(|| self.alpha3.clone())
    }

    /// A short display tag (`en`, `pt-BR`) with flags appended.
    pub fn display_tag(&self) -> String {
        let mut s = self.alpha2();
        if let Some(r) = &self.region {
            s.push('-');
            s.push_str(r);
        }
        if self.hearing_impaired {
            s.push_str(" [hi]");
        }
        if self.forced {
            s.push_str(" [forced]");
        }
        s
    }

    /// Equality ignoring HI/forced flags (same spoken language).
    pub fn same_language(&self, other: &Language) -> bool {
        self.alpha3 == other.alpha3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_codes() {
        assert_eq!(Language::parse("en").unwrap().alpha3, "eng");
        assert_eq!(Language::parse("eng").unwrap().alpha3, "eng");
        assert_eq!(Language::parse("ja").unwrap().alpha3, "jpn");
        assert_eq!(Language::parse("spa").unwrap().alpha2(), "es");
    }

    #[test]
    fn bibliographic_codes() {
        assert_eq!(Language::parse("fr").unwrap().alpha3b(), "fre");
        assert_eq!(Language::parse("de").unwrap().alpha3b(), "ger");
        assert_eq!(Language::parse("zh").unwrap().alpha3b(), "chi");
    }

    #[test]
    fn brazilian_portuguese() {
        let l = Language::parse("pt-BR").unwrap();
        assert_eq!(l.alpha3, "por");
        assert_eq!(l.region.as_deref(), Some("BR"));
        assert_eq!(l.alpha3b(), "pob");
    }

    #[test]
    fn flags() {
        let l = Language::parse("en:hi").unwrap();
        assert!(l.hearing_impaired);
        assert!(!l.forced);
        let l = Language::parse("en:forced").unwrap();
        assert!(l.forced);
    }
}
