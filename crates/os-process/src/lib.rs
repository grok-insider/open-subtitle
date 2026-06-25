//! # os-process
//!
//! Post-processing: decompress/extract a fetched subtitle, decode it to UTF-8,
//! detect/validate its format, and optionally strip hearing-impaired cues.

pub mod archive;
pub mod encoding;
pub mod format;

use os_core::ports::{PostProcessor, ProcessOpts};
use os_core::{CoreResult, RawSubtitle, SubtitleFile};

/// The default post-processor (extract → decode → detect → optional HI strip).
#[derive(Debug, Clone, Default)]
pub struct DefaultPostProcessor;

impl PostProcessor for DefaultPostProcessor {
    fn process(&self, raw: RawSubtitle, opts: &ProcessOpts) -> CoreResult<SubtitleFile> {
        // 1. Extract from any container.
        let (bytes, member_name) =
            archive::extract(&raw.bytes, raw.container, Some(&raw.filename))?;

        // 2. Decode to UTF-8.
        let mut text = if opts.to_utf8 {
            encoding::to_utf8(&bytes)
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };

        // 3. Detect format.
        let name_for_detect = if member_name.contains('.') {
            member_name.as_str()
        } else {
            raw.filename.as_str()
        };
        let mut format = format::detect_format(name_for_detect, &text);

        // 4. Optional format conversion to the requested target (e.g. ass -> srt).
        if let Some(target) = &opts.target_format {
            if target == "srt" && format != "srt" {
                let converted = os_core::cue::to_srt(&text, &format);
                // Only accept the conversion if it produced valid-looking SRT.
                if format::looks_like_srt(&converted) {
                    text = converted;
                    format = "srt".to_string();
                }
            }
        }

        // 5. Optional HI removal (only meaningful for srt/plain).
        if opts.remove_hi && format == "srt" {
            text = format::remove_hi(&text);
        }

        Ok(SubtitleFile {
            language: raw.language,
            format,
            text,
            provider: raw.provider,
            release: raw.release,
            hi: raw.hi && !opts.remove_hi,
            forced: raw.forced,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use os_core::{Container, Language};

    #[test]
    fn end_to_end_plain_srt() {
        let raw = RawSubtitle {
            filename: "movie.srt".into(),
            bytes: b"1\r\n00:00:01,000 --> 00:00:02,000\r\nHello\r\n".to_vec(),
            container: Container::Plain,
            language: Language::parse("en").unwrap(),
            provider: "test".into(),
            release: None,
            hi: false,
            forced: false,
        };
        let out = DefaultPostProcessor
            .process(raw, &ProcessOpts::default())
            .unwrap();
        assert_eq!(out.format, "srt");
        assert!(out.text.contains("Hello"));
        assert!(!out.text.contains('\r')); // normalized
    }
}
