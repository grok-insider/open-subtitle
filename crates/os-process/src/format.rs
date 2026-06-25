//! Lightweight subtitle format detection, validation, and an HI-removal pass.
//! Full format conversion (SRT⇆ASS/VTT) is a later phase; for now we detect the
//! format, validate SRT structure loosely, and can strip hearing-impaired cues.

/// Guess the subtitle format from a filename extension and/or content.
pub fn detect_format(filename: &str, text: &str) -> String {
    if let Some((_, ext)) = filename.rsplit_once('.') {
        let e = ext.to_lowercase();
        if ["srt", "ass", "ssa", "vtt", "sub", "smi"].contains(&e.as_str()) {
            return if e == "ssa" { "ass".into() } else { e };
        }
    }
    // Content sniffing.
    let head = text.trim_start();
    if head.starts_with("WEBVTT") {
        "vtt".into()
    } else if head.starts_with("[Script Info]") || head.contains("\nDialogue:") {
        "ass".into()
    } else {
        // Default to SRT (covers "-->" cues and anything unrecognized).
        "srt".into()
    }
}

/// Loose SRT validity check: at least one cue with a `-->` timing line.
pub fn looks_like_srt(text: &str) -> bool {
    text.lines().any(|l| l.contains("-->"))
}

/// Remove hearing-impaired cues from SRT/plain text (best-effort).
///
/// Strips bracketed `[...]` / `(...)` cues, leading `NAME:` speaker labels, and
/// music-symbol lines, dropping any cue whose text becomes empty.
pub fn remove_hi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for block in text.split("\n\n") {
        let mut kept_lines: Vec<String> = Vec::new();
        for line in block.lines() {
            // Keep index and timing lines verbatim.
            if line.trim().chars().all(|c| c.is_ascii_digit()) || line.contains("-->") {
                kept_lines.push(line.to_string());
                continue;
            }
            let cleaned = strip_hi_line(line);
            if !cleaned.trim().is_empty() {
                kept_lines.push(cleaned);
            }
        }
        // Only keep the block if it still has a text line beyond index+timing.
        let has_text = kept_lines
            .iter()
            .any(|l| !l.contains("-->") && !l.trim().chars().all(|c| c.is_ascii_digit()));
        if has_text {
            out.push_str(&kept_lines.join("\n"));
            out.push_str("\n\n");
        }
    }
    out.trim_end().to_string() + "\n"
}

fn strip_hi_line(line: &str) -> String {
    // Music/lyric lines (wrapped in note symbols) are dropped entirely.
    if line.contains('♪') || line.contains('♫') {
        return String::new();
    }
    let mut s = String::with_capacity(line.len());
    let mut depth_sq = 0i32;
    let mut depth_par = 0i32;
    for c in line.chars() {
        match c {
            '[' => depth_sq += 1,
            ']' => depth_sq = depth_sq.saturating_sub(1),
            '(' => depth_par += 1,
            ')' => depth_par = depth_par.saturating_sub(1),
            '♪' | '♫' => {}
            _ if depth_sq == 0 && depth_par == 0 => s.push(c),
            _ => {}
        }
    }
    // Strip a leading "NAME:" speaker label (all-caps before a colon).
    let trimmed = s.trim_start();
    if let Some(idx) = trimmed.find(':') {
        let label = &trimmed[..idx];
        if !label.is_empty()
            && label.len() <= 20
            && label
                .chars()
                .all(|c| c.is_uppercase() || c.is_whitespace() || c == '.' || c == '#')
        {
            return trimmed[idx + 1..].trim_start().to_string();
        }
    }
    s.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_formats() {
        assert_eq!(detect_format("a.srt", ""), "srt");
        assert_eq!(detect_format("a.ssa", ""), "ass");
        assert_eq!(detect_format("x", "WEBVTT\n\n..."), "vtt");
        assert_eq!(
            detect_format("x", "1\n00:00:01,000 --> 00:00:02,000\nhi"),
            "srt"
        );
    }

    #[test]
    fn removes_bracket_and_speaker_cues() {
        let srt = "1\n00:00:01,000 --> 00:00:02,000\n[door creaks]\n\n\
                   2\n00:00:03,000 --> 00:00:04,000\nJOHN: Hello there\n\n\
                   3\n00:00:05,000 --> 00:00:06,000\n♪ music ♪";
        let out = remove_hi(srt);
        assert!(!out.contains("door creaks"));
        assert!(out.contains("Hello there"));
        assert!(!out.contains("JOHN:"));
        // The music-only cue is dropped entirely.
        assert!(!out.contains("music"));
    }
}
