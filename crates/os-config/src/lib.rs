//! Configuration: a single TOML file at `~/.config/open-subtitle/config.toml`
//! (XDG-aware). **Secrets live only here** — never compiled in. Keyless sources
//! work out of the box; keys/logins are optional and ship disabled.

use os_core::{CoreError, CoreResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Top-level config.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Preferred subtitle languages, in priority order (e.g. `["en", "es"]`).
    pub languages: Vec<String>,
    pub providers: Providers,
    pub process: ProcessConfig,
    pub sync: SyncConfig,
    pub translate: TranslateConfig,
    pub transcribe: TranscribeConfig,
    pub net: NetConfig,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            languages: vec!["en".to_string()],
            providers: Providers::default(),
            process: ProcessConfig::default(),
            sync: SyncConfig::default(),
            translate: TranslateConfig::default(),
            transcribe: TranscribeConfig::default(),
            net: NetConfig::default(),
        }
    }
}

/// Per-provider toggles + options. Keyless providers default ON; key/login ones
/// default OFF (the keyless-by-default invariant).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Providers {
    pub opensubtitles_org: ProviderEntry,
    pub podnapisi: ProviderEntry,
    pub subdl: ProviderEntry,
    pub opensubtitles_com: ProviderEntry,
    pub jimaku: ProviderEntry,
    pub gestdown: ProviderEntry,
    pub tvsubtitles: ProviderEntry,
    pub animetosho: ProviderEntry,
}

impl Default for Providers {
    fn default() -> Self {
        let on = || ProviderEntry {
            enabled: true,
            ..Default::default()
        };
        let off = || ProviderEntry {
            enabled: false,
            ..Default::default()
        };
        Providers {
            opensubtitles_org: on(),
            podnapisi: on(),
            subdl: on(), // anonymous works (~300/day per IP); key optional
            opensubtitles_com: off(),
            jimaku: off(), // needs a free key
            gestdown: on(),
            tvsubtitles: on(),
            animetosho: on(),
        }
    }
}

/// A single provider's settings. Unknown keys are ignored.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderEntry {
    pub enabled: bool,
    pub api_key: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProcessConfig {
    pub to_utf8: bool,
    pub format: String,
    pub remove_hi: bool,
    pub keep_original_format: bool,
}

impl Default for ProcessConfig {
    fn default() -> Self {
        ProcessConfig {
            to_utf8: true,
            format: "srt".to_string(),
            remove_hi: false,
            keep_original_format: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncConfig {
    /// `ffsubsync` | `alass` | `none`.
    pub backend: String,
}
impl Default for SyncConfig {
    fn default() -> Self {
        SyncConfig {
            backend: "none".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TranslateConfig {
    /// `local` | `libretranslate` | `llm` | `none`.
    pub backend: String,
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
}
impl Default for TranslateConfig {
    fn default() -> Self {
        TranslateConfig {
            backend: "none".to_string(),
            endpoint: None,
            api_key: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TranscribeConfig {
    /// `whisper` | `none`.
    pub backend: String,
    pub model: Option<String>,
}
impl Default for TranscribeConfig {
    fn default() -> Self {
        TranscribeConfig {
            backend: "none".to_string(),
            model: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NetConfig {
    pub max_concurrency: usize,
    pub timeout_secs: u64,
    pub user_agent: String,
}
impl Default for NetConfig {
    fn default() -> Self {
        NetConfig {
            max_concurrency: 8,
            timeout_secs: 20,
            user_agent: format!("open-subtitle/{}", env!("CARGO_PKG_VERSION")),
        }
    }
}

impl Config {
    /// The default config path (`$XDG_CONFIG_HOME/open-subtitle/config.toml`).
    pub fn default_path() -> CoreResult<PathBuf> {
        let dirs = directories::ProjectDirs::from("ai", "opensubtitle", "open-subtitle")
            .ok_or_else(|| CoreError::Config("cannot resolve config dir".into()))?;
        Ok(dirs.config_dir().join("config.toml"))
    }

    /// The default cache dir.
    pub fn cache_dir() -> CoreResult<PathBuf> {
        let dirs = directories::ProjectDirs::from("ai", "opensubtitle", "open-subtitle")
            .ok_or_else(|| CoreError::Config("cannot resolve cache dir".into()))?;
        Ok(dirs.cache_dir().to_path_buf())
    }

    /// Load from a path, or return defaults if it doesn't exist.
    pub fn load(path: &Path) -> CoreResult<Config> {
        if !path.exists() {
            return Ok(Config::default());
        }
        let text = std::fs::read_to_string(path).map_err(|e| CoreError::Io(e.to_string()))?;
        toml::from_str(&text).map_err(|e| CoreError::Config(e.to_string()))
    }

    /// Load from the default path.
    pub fn load_default() -> CoreResult<Config> {
        Config::load(&Config::default_path()?)
    }

    /// Save to a path (creating parent dirs).
    pub fn save(&self, path: &Path) -> CoreResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CoreError::Io(e.to_string()))?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| CoreError::Config(e.to_string()))?;
        std::fs::write(path, text).map_err(|e| CoreError::Io(e.to_string()))
    }

    /// Parsed language preferences (silently drops unparseable codes).
    pub fn languages(&self) -> Vec<os_core::Language> {
        self.languages
            .iter()
            .filter_map(|c| os_core::Language::parse(c))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_keyless_first() {
        let c = Config::default();
        assert!(c.providers.opensubtitles_org.enabled);
        assert!(c.providers.subdl.enabled);
        // Key/login providers default OFF.
        assert!(!c.providers.opensubtitles_com.enabled);
        assert!(!c.providers.jimaku.enabled);
    }

    #[test]
    fn roundtrip_toml() {
        let c = Config::default();
        let text = toml::to_string_pretty(&c).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.languages, c.languages);
        assert_eq!(back.process.format, "srt");
    }

    #[test]
    fn partial_toml_uses_defaults() {
        let text = r#"
            languages = ["es", "en"]
            [providers.jimaku]
            enabled = true
            api_key = "x"
        "#;
        let c: Config = toml::from_str(text).unwrap();
        assert_eq!(c.languages, vec!["es", "en"]);
        assert!(c.providers.jimaku.enabled);
        // Untouched providers keep their defaults.
        assert!(c.providers.opensubtitles_org.enabled);
        assert_eq!(c.net.max_concurrency, 8);
    }
}
