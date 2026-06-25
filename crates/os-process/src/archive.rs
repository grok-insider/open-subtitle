//! Decompression / archive extraction. Turns a fetched `RawSubtitle`'s container
//! bytes into the raw bytes of a single subtitle file.

use os_core::{Container, CoreError, CoreResult};
use std::io::Read;

const SUB_EXTS: &[&str] = &["srt", "ass", "ssa", "vtt", "sub", "smi"];

/// Extension (lowercased, no dot) for a name, if any.
fn ext_of(name: &str) -> Option<String> {
    name.rsplit_once('.').map(|(_, e)| e.to_lowercase())
}

fn is_subtitle_name(name: &str) -> bool {
    ext_of(name)
        .map(|e| SUB_EXTS.contains(&e.as_str()))
        .unwrap_or(false)
}

/// Detect the container from magic bytes when the provider didn't say.
pub fn sniff_container(bytes: &[u8]) -> Container {
    if bytes.len() >= 2 && bytes[0] == 0x1f && bytes[1] == 0x8b {
        Container::Gzip
    } else if bytes.len() >= 4 && &bytes[0..2] == b"PK" {
        Container::Zip
    } else if bytes.len() >= 6 && &bytes[0..6] == b"Rar!\x1a\x07" {
        Container::Rar
    } else if bytes.len() >= 6 && bytes[0..6] == [0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00] {
        Container::Xz
    } else {
        Container::Plain
    }
}

/// Extract the subtitle bytes (and chosen member name) from a container.
///
/// `hint` is an optional filename used to pick the best member from a multi-file
/// archive (e.g. the episode coordinate).
pub fn extract(
    bytes: &[u8],
    container: Container,
    hint: Option<&str>,
) -> CoreResult<(Vec<u8>, String)> {
    let container = if container == Container::Unknown {
        sniff_container(bytes)
    } else {
        container
    };

    match container {
        Container::Plain => Ok((bytes.to_vec(), hint.unwrap_or("subtitle.srt").to_string())),
        Container::Gzip => {
            let mut d = flate2::read::GzDecoder::new(bytes);
            let mut out = Vec::new();
            d.read_to_end(&mut out)
                .map_err(|e| CoreError::Parse(format!("gunzip: {e}")))?;
            Ok((out, hint.unwrap_or("subtitle.srt").to_string()))
        }
        Container::Zip => extract_zip(bytes, hint),
        Container::Rar => Err(CoreError::Unsupported(
            "RAR archives not yet supported".into(),
        )),
        Container::Xz => Err(CoreError::Unsupported("XZ not yet supported".into())),
        Container::Unknown => Ok((bytes.to_vec(), "subtitle.srt".to_string())),
    }
}

fn extract_zip(bytes: &[u8], hint: Option<&str>) -> CoreResult<(Vec<u8>, String)> {
    let reader = std::io::Cursor::new(bytes);
    let mut zip =
        zip::ZipArchive::new(reader).map_err(|e| CoreError::Parse(format!("zip: {e}")))?;

    // Collect subtitle members.
    let mut members: Vec<(usize, String)> = Vec::new();
    for i in 0..zip.len() {
        if let Ok(f) = zip.by_index(i) {
            let name = f.name().to_string();
            if is_subtitle_name(&name) {
                members.push((i, name));
            }
        }
    }
    if members.is_empty() {
        return Err(CoreError::NotFound);
    }

    // Pick the best member: prefer one whose name shares the most with the hint,
    // preferring `.srt`, then the largest.
    let best = pick_best(&members, hint);
    let mut file = zip
        .by_index(best)
        .map_err(|e| CoreError::Parse(format!("zip member: {e}")))?;
    let chosen_name = file.name().to_string();
    let mut out = Vec::new();
    file.read_to_end(&mut out)
        .map_err(|e| CoreError::Io(e.to_string()))?;
    Ok((out, chosen_name))
}

fn pick_best(members: &[(usize, String)], hint: Option<&str>) -> usize {
    if members.len() == 1 {
        return members[0].0;
    }
    let hint_lc = hint.map(|h| h.to_lowercase());
    members
        .iter()
        .max_by_key(|(_, name)| {
            let n = name.to_lowercase();
            let mut score = 0i32;
            if let Some(h) = &hint_lc {
                // crude token overlap
                for tok in h
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|t| t.len() > 2)
                {
                    if n.contains(tok) {
                        score += 2;
                    }
                }
            }
            if n.ends_with(".srt") {
                score += 1;
            }
            score
        })
        .map(|(i, _)| *i)
        .unwrap_or(members[0].0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn sniff_works() {
        assert_eq!(sniff_container(&[0x1f, 0x8b, 0, 0]), Container::Gzip);
        assert_eq!(sniff_container(b"PK\x03\x04"), Container::Zip);
        assert_eq!(sniff_container(b"plain text"), Container::Plain);
    }

    #[test]
    fn gzip_roundtrip() {
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(b"1\n00:00:01,000 --> 00:00:02,000\nHi\n")
            .unwrap();
        let gz = e.finish().unwrap();
        let (out, _) = extract(&gz, Container::Gzip, None).unwrap();
        assert!(String::from_utf8_lossy(&out).contains("Hi"));
    }

    #[test]
    fn zip_picks_subtitle_member() {
        let mut buf = Vec::new();
        {
            let w = std::io::Cursor::new(&mut buf);
            let mut zip = zip::ZipWriter::new(w);
            let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
            zip.start_file("readme.txt", opts).unwrap();
            zip.write_all(b"ignore me").unwrap();
            zip.start_file("movie.srt", opts).unwrap();
            zip.write_all(b"1\n00:00:01,000 --> 00:00:02,000\nHello\n")
                .unwrap();
            zip.finish().unwrap();
        }
        let (out, name) = extract(&buf, Container::Zip, None).unwrap();
        assert!(name.ends_with(".srt"));
        assert!(String::from_utf8_lossy(&out).contains("Hello"));
    }
}
