//! Subtitle format conversion (pure, no external deps).
//!
//! Parses SRT, WebVTT, and SSA/ASS into a common cue list and emits SRT (the
//! safe interchange format). ASS styling override tags `{\...}` are stripped and
//! `\N`/`\n` become real newlines. This lets the engine deliver `.srt` even when
//! a provider only had `.ass`.

/// A single subtitle cue with millisecond timing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

/// Convert subtitle `text` of the given `format` into SRT. Returns the input
/// unchanged if it's already SRT or can't be parsed.
pub fn to_srt(text: &str, format: &str) -> String {
    let cues = match format {
        "srt" => return text.to_string(),
        "vtt" => parse_vtt(text),
        "ass" | "ssa" => parse_ass(text),
        _ => parse_srt(text),
    };
    if cues.is_empty() {
        return text.to_string();
    }
    emit_srt(&cues)
}

/// Parse a timestamp `HH:MM:SS[,.]mmm` or `H:MM:SS.cs` into milliseconds.
fn parse_ts(s: &str) -> Option<u64> {
    let s = s.trim();
    let (hms, frac) = s.split_once([',', '.'])?;
    let parts: Vec<&str> = hms.split(':').collect();
    let (h, m, sec) = match parts.as_slice() {
        [h, m, s] => (
            h.parse::<u64>().ok()?,
            m.parse::<u64>().ok()?,
            s.parse::<u64>().ok()?,
        ),
        [m, s] => (0, m.parse::<u64>().ok()?, s.parse::<u64>().ok()?),
        _ => return None,
    };
    // Fraction can be milliseconds (3 digits) or centiseconds (2 digits, ASS).
    let frac_ms = match frac.len() {
        2 => frac.parse::<u64>().ok()? * 10,
        3 => frac.parse::<u64>().ok()?,
        _ => {
            let f: u64 = frac.parse().ok()?;
            // normalize to ms
            match frac.len() {
                1 => f * 100,
                _ => f % 1000,
            }
        }
    };
    Some(((h * 3600 + m * 60 + sec) * 1000) + frac_ms)
}

fn fmt_ts(ms: u64) -> String {
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1000;
    let milli = ms % 1000;
    format!("{h:02}:{m:02}:{s:02},{milli:03}")
}

/// Parse SRT into cues.
pub fn parse_srt(text: &str) -> Vec<Cue> {
    let mut cues = Vec::new();
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block.lines().collect();
        // Find the timing line (contains -->).
        let ti = lines.iter().position(|l| l.contains("-->"));
        let ti = match ti {
            Some(i) => i,
            None => continue,
        };
        if let Some((a, b)) = lines[ti].split_once("-->") {
            if let (Some(start), Some(end)) = (
                parse_ts(a),
                parse_ts(b.split_whitespace().next().unwrap_or(b)),
            ) {
                let body = lines[ti + 1..].join("\n");
                if !body.trim().is_empty() {
                    cues.push(Cue {
                        start_ms: start,
                        end_ms: end,
                        text: body.trim().to_string(),
                    });
                }
            }
        }
    }
    cues
}

/// Parse WebVTT into cues (strips the WEBVTT header and inline tags).
pub fn parse_vtt(text: &str) -> Vec<Cue> {
    let mut cues = Vec::new();
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block.lines().collect();
        let ti = match lines.iter().position(|l| l.contains("-->")) {
            Some(i) => i,
            None => continue,
        };
        let timing = lines[ti];
        if let Some((a, rest)) = timing.split_once("-->") {
            let b = rest.split_whitespace().next().unwrap_or(rest);
            if let (Some(start), Some(end)) = (parse_ts(a), parse_ts(b)) {
                let body = lines[ti + 1..].join("\n");
                let body = strip_vtt_tags(&body);
                if !body.trim().is_empty() {
                    cues.push(Cue {
                        start_ms: start,
                        end_ms: end,
                        text: body.trim().to_string(),
                    });
                }
            }
        }
    }
    cues
}

fn strip_vtt_tags(s: &str) -> String {
    // Remove <...> inline tags (e.g. <c>, <i>, <00:00:00.000>).
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

/// Parse SSA/ASS Events into cues (strips override tags, `\N` → newline).
pub fn parse_ass(text: &str) -> Vec<Cue> {
    let mut cues = Vec::new();
    let mut start_idx = 1usize;
    let mut end_idx = 2usize;
    let mut text_idx = 9usize;
    let mut in_events = false;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_events = trimmed.eq_ignore_ascii_case("[Events]");
            continue;
        }
        if !in_events {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("Format:") {
            let cols: Vec<String> = rest.split(',').map(|c| c.trim().to_lowercase()).collect();
            if let Some(i) = cols.iter().position(|c| c == "start") {
                start_idx = i;
            }
            if let Some(i) = cols.iter().position(|c| c == "end") {
                end_idx = i;
            }
            if let Some(i) = cols.iter().position(|c| c == "text") {
                text_idx = i;
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("Dialogue:") {
            // Split into exactly text_idx+1 fields; the Text field keeps commas.
            let parts: Vec<&str> = rest.splitn(text_idx + 1, ',').collect();
            if parts.len() <= text_idx.max(end_idx).max(start_idx) {
                continue;
            }
            let start = parse_ts(parts[start_idx].trim());
            let end = parse_ts(parts[end_idx].trim());
            if let (Some(s), Some(e)) = (start, end) {
                let body = clean_ass_text(parts[text_idx]);
                if !body.trim().is_empty() {
                    cues.push(Cue {
                        start_ms: s,
                        end_ms: e,
                        text: body,
                    });
                }
            }
        }
    }
    // ASS events are not guaranteed sorted.
    cues.sort_by_key(|c| c.start_ms);
    cues
}

fn clean_ass_text(s: &str) -> String {
    // Remove {\...} override blocks, convert \N and \n to newlines, drop \h.
    let mut out = String::with_capacity(s.len());
    let mut in_brace = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => in_brace = true,
            '}' => in_brace = false,
            '\\' if !in_brace => match chars.peek() {
                Some('N') | Some('n') => {
                    out.push('\n');
                    chars.next();
                }
                Some('h') => {
                    out.push(' ');
                    chars.next();
                }
                _ => out.push('\\'),
            },
            _ if !in_brace => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// Emit an SRT document from cues.
pub fn emit_srt(cues: &[Cue]) -> String {
    let mut out = String::new();
    for (i, c) in cues.iter().enumerate() {
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            fmt_ts(c.start_ms),
            fmt_ts(c.end_ms),
            c.text
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ts_parsing() {
        assert_eq!(parse_ts("00:00:06,000"), Some(6000));
        assert_eq!(parse_ts("0:00:06.50"), Some(6500)); // ASS centiseconds
        assert_eq!(parse_ts("00:01:02.345"), Some(62345)); // VTT ms
    }

    #[test]
    fn ass_to_srt() {
        let ass = "[Script Info]\nTitle: x\n\n[Events]\n\
            Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n\
            Dialogue: 0,0:00:06.00,0:00:09.00,Default,,0,0,0,,{\\i1}Hello{\\i0}\\Nworld\n\
            Dialogue: 0,0:00:01.00,0:00:03.00,Default,,0,0,0,,First, line\n";
        let srt = to_srt(ass, "ass");
        // Sorted by start; tags stripped; \N -> newline; commas in text preserved.
        assert!(srt.starts_with("1\n00:00:01,000 --> 00:00:03,000\nFirst, line\n"));
        assert!(srt.contains("2\n00:00:06,000 --> 00:00:09,000\nHello\nworld"));
        assert!(!srt.contains("\\i1"));
    }

    #[test]
    fn vtt_to_srt() {
        let vtt = "WEBVTT\n\n00:00:06.000 --> 00:00:09.000\n<c>Hello</c> there\n\n\
                   00:00:10.000 --> 00:00:12.000\nNext\n";
        let srt = to_srt(vtt, "vtt");
        assert!(srt.contains("1\n00:00:06,000 --> 00:00:09,000\nHello there"));
        assert!(srt.contains("2\n00:00:10,000 --> 00:00:12,000\nNext"));
    }

    #[test]
    fn srt_passthrough() {
        let srt = "1\n00:00:01,000 --> 00:00:02,000\nHi\n\n";
        assert_eq!(to_srt(srt, "srt"), srt);
    }
}
