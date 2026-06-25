//! # os-translate
//!
//! Translation adapters. [`LibreTranslate`] targets a LibreTranslate-compatible
//! endpoint (self-hostable, key-free, local-first). It parses the subtitle into
//! cues and translates each cue's text so timing is preserved.

use async_trait::async_trait;
use os_core::cue::{emit_srt, parse_srt};
use os_core::ports::Translator;
use os_core::{CoreError, CoreResult, Language, SubtitleFile};
use serde_json::json;

/// A LibreTranslate-compatible translator (`POST {endpoint}/translate`).
pub struct LibreTranslate {
    client: reqwest::Client,
    endpoint: String,
    api_key: Option<String>,
}

impl LibreTranslate {
    pub fn new(
        client: reqwest::Client,
        endpoint: impl Into<String>,
        api_key: Option<String>,
    ) -> Self {
        LibreTranslate {
            client,
            endpoint: endpoint.into(),
            api_key,
        }
    }

    async fn translate_one(&self, text: &str, target: &str) -> CoreResult<String> {
        if text.trim().is_empty() {
            return Ok(text.to_string());
        }
        let mut body = json!({
            "q": text,
            "source": "auto",
            "target": target,
            "format": "text",
        });
        if let Some(k) = &self.api_key {
            body["api_key"] = json!(k);
        }
        let resp = self
            .client
            .post(format!("{}/translate", self.endpoint.trim_end_matches('/')))
            .json(&body)
            .send()
            .await
            .map_err(|e| CoreError::Network(format!("libretranslate: {e}")))?;
        if !resp.status().is_success() {
            return Err(CoreError::Provider(format!(
                "libretranslate: {}",
                resp.status()
            )));
        }
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CoreError::Parse(format!("libretranslate: {e}")))?;
        v.get("translatedText")
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| CoreError::Parse("libretranslate: no translatedText".into()))
    }
}

#[async_trait]
impl Translator for LibreTranslate {
    fn name(&self) -> &str {
        "libretranslate"
    }

    async fn translate(&self, sub: &SubtitleFile, to: &Language) -> CoreResult<SubtitleFile> {
        // Work on SRT cues so timing survives.
        let srt = os_core::cue::to_srt(&sub.text, &sub.format);
        let mut cues = parse_srt(&srt);
        if cues.is_empty() {
            return Err(CoreError::Unsupported(
                "translate: could not parse subtitle into cues".into(),
            ));
        }
        let target = to.alpha2();
        for cue in &mut cues {
            cue.text = self.translate_one(&cue.text, &target).await?;
        }
        Ok(SubtitleFile {
            language: to.clone(),
            format: "srt".to_string(),
            text: emit_srt(&cues),
            ..sub.clone()
        })
    }
}

/// Build a translator from a backend name + config.
pub fn from_backend(
    client: reqwest::Client,
    backend: &str,
    endpoint: Option<String>,
    api_key: Option<String>,
) -> Option<Box<dyn Translator>> {
    match backend {
        "libretranslate" | "local" => {
            let ep = endpoint.unwrap_or_else(|| "http://localhost:5000".to_string());
            Some(Box::new(LibreTranslate::new(client, ep, api_key)) as Box<dyn Translator>)
        }
        _ => None,
    }
}
