//! # os-transcribe
//!
//! Last-resort subtitle generation from audio via OpenAI Whisper's CLI
//! (`whisper`). Detected at runtime; absent → `CoreError::Unsupported`.

use async_trait::async_trait;
use os_core::ports::Transcriber;
use os_core::{CoreError, CoreResult, Language, SubtitleFile};
use std::path::Path;

/// Whisper CLI transcriber. `model` is e.g. `tiny`/`base`/`small`/`medium`/`large`.
pub struct Whisper {
    bin: String,
    model: String,
}

impl Default for Whisper {
    fn default() -> Self {
        Whisper {
            bin: "whisper".to_string(),
            model: "small".to_string(),
        }
    }
}

impl Whisper {
    pub fn new(bin: impl Into<String>, model: impl Into<String>) -> Self {
        Whisper {
            bin: bin.into(),
            model: model.into(),
        }
    }

    pub fn available(&self) -> bool {
        std::process::Command::new(&self.bin)
            .arg("--help")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success() || s.code().is_some())
            .unwrap_or(false)
    }
}

#[async_trait]
impl Transcriber for Whisper {
    fn name(&self) -> &str {
        "whisper"
    }

    async fn transcribe(
        &self,
        media_path: &Path,
        lang: Option<&Language>,
    ) -> CoreResult<SubtitleFile> {
        let out_dir = std::env::temp_dir().join(format!("ost-whisper-{}", std::process::id()));
        tokio::fs::create_dir_all(&out_dir)
            .await
            .map_err(|e| CoreError::Io(e.to_string()))?;

        let mut cmd = tokio::process::Command::new(&self.bin);
        cmd.arg(media_path)
            .arg("--model")
            .arg(&self.model)
            .arg("--output_format")
            .arg("srt")
            .arg("--output_dir")
            .arg(&out_dir);
        if let Some(l) = lang {
            cmd.arg("--language").arg(l.alpha2());
        }

        let status = cmd.status().await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                CoreError::Unsupported(format!("{} not installed", self.bin))
            } else {
                CoreError::Io(e.to_string())
            }
        })?;
        if !status.success() {
            return Err(CoreError::Provider(format!(
                "whisper exited with {}",
                status.code().unwrap_or(-1)
            )));
        }

        // Whisper writes <stem>.srt into out_dir.
        let stem = media_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "audio".into());
        let srt_path = out_dir.join(format!("{stem}.srt"));
        let text = tokio::fs::read_to_string(&srt_path)
            .await
            .map_err(|e| CoreError::Io(format!("reading whisper output: {e}")))?;
        let _ = tokio::fs::remove_dir_all(&out_dir).await;

        let language = lang
            .cloned()
            .unwrap_or_else(|| Language::parse("en").unwrap());
        Ok(SubtitleFile {
            language,
            format: "srt".to_string(),
            text,
            provider: "whisper".to_string(),
            release: Some("transcribed".to_string()),
            hi: false,
            forced: false,
        })
    }
}

/// Build a transcriber from a backend name + optional model.
pub fn from_backend(backend: &str, model: Option<String>) -> Option<Box<dyn Transcriber>> {
    match backend {
        "whisper" => {
            let w = Whisper::new("whisper", model.unwrap_or_else(|| "small".into()));
            w.available().then(|| Box::new(w) as Box<dyn Transcriber>)
        }
        _ => None,
    }
}
