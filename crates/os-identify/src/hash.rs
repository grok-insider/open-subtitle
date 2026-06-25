//! The OpenSubtitles (OSDB) file hash.
//!
//! `hash = (filesize + sum of first 64 KiB + sum of last 64 KiB)` read as
//! little-endian u64 chunks, wrapped at 64 bits, as a 16-digit lowercase hex
//! string. Used by OpenSubtitles.org/.com and others. Minimum file size 128 KiB.

use os_core::{CoreError, CoreResult, Hasher};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const CHUNK: u64 = 65536; // 64 KiB

/// OSDB hasher (`os_core::Hasher` impl, registered under the name `osdb`).
#[derive(Debug, Clone, Default)]
pub struct OsdbHasher;

/// Pure hash of an already-open reader given its total size, for testability.
pub fn osdb_hash<R: Read + Seek>(reader: &mut R, filesize: u64) -> CoreResult<Option<String>> {
    if filesize < CHUNK * 2 {
        return Ok(None);
    }
    let mut hash: u64 = filesize;

    // First 64 KiB.
    hash = hash.wrapping_add(sum_chunk(reader)?);

    // Last 64 KiB.
    reader
        .seek(SeekFrom::Start(filesize - CHUNK))
        .map_err(|e| CoreError::Io(e.to_string()))?;
    hash = hash.wrapping_add(sum_chunk(reader)?);

    Ok(Some(format!("{hash:016x}")))
}

fn sum_chunk<R: Read>(reader: &mut R) -> CoreResult<u64> {
    let mut buf = [0u8; CHUNK as usize];
    reader
        .read_exact(&mut buf)
        .map_err(|e| CoreError::Io(e.to_string()))?;
    let mut sum: u64 = 0;
    for word in buf.chunks_exact(8) {
        let v = u64::from_le_bytes(word.try_into().unwrap());
        sum = sum.wrapping_add(v);
    }
    Ok(sum)
}

impl Hasher for OsdbHasher {
    fn name(&self) -> &str {
        "osdb"
    }

    fn hash_file(&self, path: &Path) -> CoreResult<Option<String>> {
        let mut file = File::open(path).map_err(|e| CoreError::Io(e.to_string()))?;
        let size = file
            .metadata()
            .map_err(|e| CoreError::Io(e.to_string()))?
            .len();
        osdb_hash(&mut file, size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn too_small_returns_none() {
        let mut c = Cursor::new(vec![0u8; 1000]);
        assert_eq!(osdb_hash(&mut c, 1000).unwrap(), None);
    }

    #[test]
    fn known_vector_all_zeros() {
        // A 128 KiB file of all zeros: first+last chunks sum to 0, so the hash is
        // just the filesize (131072 = 0x20000) → "0000000000020000".
        let size = (CHUNK * 2) as usize;
        let mut c = Cursor::new(vec![0u8; size]);
        let h = osdb_hash(&mut c, size as u64).unwrap().unwrap();
        assert_eq!(h, "0000000000020000");
        assert_eq!(h.len(), 16);
    }

    #[test]
    fn known_vector_with_payload() {
        // 128 KiB, first 8 bytes = 1 (LE u64), rest zero.
        // hash = filesize(131072) + 1 (from first chunk) + 0 (last chunk) = 131073.
        let size = (CHUNK * 2) as usize;
        let mut data = vec![0u8; size];
        data[0] = 1;
        let mut c = Cursor::new(data);
        let h = osdb_hash(&mut c, size as u64).unwrap().unwrap();
        assert_eq!(h, format!("{:016x}", 131073u64));
    }
}
