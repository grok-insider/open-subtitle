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
    pub automation: AutomationConfig,
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
            automation: AutomationConfig::default(),
        }
    }
}

/// Settings for the `ostd` automation webhooks (Sonarr/Radarr "On Import").
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutomationConfig {
    /// Whether webhook-triggered fetching is active.
    pub enabled: bool,
    /// Optional override of the language preference for automation.
    pub languages: Vec<String>,
    /// Path prefix remaps, for when Sonarr/Radarr run in a different mount
    /// namespace than `ostd` (e.g. containers). First matching prefix wins.
    pub path_map: Vec<PathMap>,
    /// Fallback directory for sidecars when the media file isn't reachable.
    pub output_dir: Option<String>,
    /// Record imports/scans that still lack a subtitle in a persistent "wanted"
    /// list and have `ostd` re-search them on a timer until found. Anime fansubs
    /// often lag a release, so a one-shot fetch frequently comes up empty.
    pub track_wanted: bool,
    /// How often the daemon re-searches each unfulfilled wanted item, in seconds.
    /// Also the minimum age before an item is retried. `0` disables the scheduler.
    pub recheck_interval_secs: u64,
    /// Give up on a wanted item after this many attempts (`0` = never give up).
    pub max_attempts: u32,
}

impl Default for AutomationConfig {
    fn default() -> Self {
        AutomationConfig {
            enabled: true,
            languages: Vec::new(),
            path_map: Vec::new(),
            output_dir: None,
            track_wanted: true,
            recheck_interval_secs: 6 * 60 * 60, // 6h
            max_attempts: 0,
        }
    }
}

/// A single path prefix remap (`from` → `to`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PathMap {
    pub from: String,
    pub to: String,
}

impl AutomationConfig {
    /// Apply the configured path remaps to an incoming media path.
    pub fn remap(&self, path: &str) -> String {
        for m in &self.path_map {
            if !m.from.is_empty() && path.starts_with(&m.from) {
                return format!("{}{}", m.to, &path[m.from.len()..]);
            }
        }
        path.to_string()
    }

    /// The effective language preference (automation override, else top-level).
    pub fn languages_or<'a>(&'a self, fallback: &'a [String]) -> &'a [String] {
        if self.languages.is_empty() {
            fallback
        } else {
            &self.languages
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
    fn automation_path_remap() {
        let a = AutomationConfig {
            path_map: vec![PathMap {
                from: "/data".into(),
                to: "/mnt/media".into(),
            }],
            ..AutomationConfig::default()
        };
        assert_eq!(a.remap("/data/tv/Show/ep.mkv"), "/mnt/media/tv/Show/ep.mkv");
        // Non-matching prefix is unchanged.
        assert_eq!(a.remap("/other/x.mkv"), "/other/x.mkv");
    }

    #[test]
    fn automation_defaults_enabled() {
        let c = Config::default();
        assert!(c.automation.enabled);
        assert!(c.automation.path_map.is_empty());
        // Wanted-list tracking is on by default with a 6h re-search cadence and
        // no attempt cap.
        assert!(c.automation.track_wanted);
        assert_eq!(c.automation.recheck_interval_secs, 6 * 60 * 60);
        assert_eq!(c.automation.max_attempts, 0);
    }

    #[test]
    fn automation_wanted_fields_roundtrip() {
        let text = r#"
            [automation]
            track_wanted = false
            recheck_interval_secs = 1800
            max_attempts = 12
        "#;
        let c: Config = toml::from_str(text).unwrap();
        assert!(!c.automation.track_wanted);
        assert_eq!(c.automation.recheck_interval_secs, 1800);
        assert_eq!(c.automation.max_attempts, 12);
        // Untouched automation fields keep their defaults.
        assert!(c.automation.enabled);
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
