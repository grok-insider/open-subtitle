//! # os-sync
//!
//! Subtitle↔video synchronizers that wrap external tools (`ffsubsync`, `alass`).
//! The tools are **detected at runtime**; if absent, `sync` returns
//! `CoreError::Unsupported` and the engine simply skips synchronization.

use async_trait::async_trait;
use os_core::ports::Synchronizer;
use os_core::{CoreError, CoreResult, SubtitleFile};
use std::path::Path;

/// Whether an external command exists on `PATH`.
fn has_command(bin: &str) -> bool {
    // Probe via `command -v` to avoid spawning the tool itself.
    std::process::Command::new(bin)
        .arg("--help")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .stdin(std::process::Stdio::null())
        .status()
        .map(|s| s.success() || s.code().is_some())
        .unwrap_or(false)
}

async fn run_to_file(
    bin: &str,
    args: &[String],
    sub: &SubtitleFile,
    out_path: &Path,
    in_path: &Path,
) -> CoreResult<SubtitleFile> {
    tokio::fs::write(in_path, &sub.text)
        .await
        .map_err(|e| CoreError::Io(e.to_string()))?;

    let status = tokio::process::Command::new(bin)
        .args(args)
        .status()
        .await
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                CoreError::Unsupported(format!("{bin} not installed"))
            } else {
                CoreError::Io(e.to_string())
            }
        })?;
    if !status.success() {
        return Err(CoreError::Provider(format!(
            "{bin} exited with {}",
            status.code().unwrap_or(-1)
        )));
    }

    let text = tokio::fs::read_to_string(out_path)
        .await
        .map_err(|e| CoreError::Io(format!("reading synced sub: {e}")))?;
    let _ = tokio::fs::remove_file(in_path).await;
    let _ = tokio::fs::remove_file(out_path).await;

    Ok(SubtitleFile {
        text,
        ..sub.clone()
    })
}

fn temp_paths(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    (
        dir.join(format!("ost-{tag}-{pid}-{nonce}.in.srt")),
        dir.join(format!("ost-{tag}-{pid}-{nonce}.out.srt")),
    )
}

/// `ffsubsync` adapter: aligns subtitle activity to VAD-detected speech.
pub struct Ffsubsync {
    bin: String,
}

impl Default for Ffsubsync {
    fn default() -> Self {
        Ffsubsync {
            bin: "ffsubsync".to_string(),
        }
    }
}

impl Ffsubsync {
    pub fn new(bin: impl Into<String>) -> Self {
        Ffsubsync { bin: bin.into() }
    }
    pub fn available(&self) -> bool {
        has_command(&self.bin)
    }
}

#[async_trait]
impl Synchronizer for Ffsubsync {
    fn name(&self) -> &str {
        "ffsubsync"
    }

    async fn sync(&self, sub: &SubtitleFile, reference: &Path) -> CoreResult<SubtitleFile> {
        let (in_path, out_path) = temp_paths("ffs");
        let args = vec![
            reference.to_string_lossy().into_owned(),
            "-i".into(),
            in_path.to_string_lossy().into_owned(),
            "-o".into(),
            out_path.to_string_lossy().into_owned(),
        ];
        run_to_file(&self.bin, &args, sub, &out_path, &in_path).await
    }
}

/// `alass` adapter: handles variable offsets / ad-breaks.
pub struct Alass {
    bin: String,
}

impl Default for Alass {
    fn default() -> Self {
        Alass {
            bin: "alass".to_string(),
        }
    }
}

impl Alass {
    pub fn new(bin: impl Into<String>) -> Self {
        Alass { bin: bin.into() }
    }
    pub fn available(&self) -> bool {
        has_command(&self.bin)
    }
}

#[async_trait]
impl Synchronizer for Alass {
    fn name(&self) -> &str {
        "alass"
    }

    async fn sync(&self, sub: &SubtitleFile, reference: &Path) -> CoreResult<SubtitleFile> {
        let (in_path, out_path) = temp_paths("alass");
        let args = vec![
            reference.to_string_lossy().into_owned(),
            in_path.to_string_lossy().into_owned(),
            out_path.to_string_lossy().into_owned(),
        ];
        run_to_file(&self.bin, &args, sub, &out_path, &in_path).await
    }
}

/// Build a synchronizer by backend name, if the tool is available.
pub fn from_backend(backend: &str) -> Option<Box<dyn Synchronizer>> {
    match backend {
        "ffsubsync" => {
            let s = Ffsubsync::default();
            s.available().then(|| Box::new(s) as Box<dyn Synchronizer>)
        }
        "alass" => {
            let s = Alass::default();
            s.available().then(|| Box::new(s) as Box<dyn Synchronizer>)
        }
        _ => None,
    }
}
