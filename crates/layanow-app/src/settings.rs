//! User settings persisted to `~/.config/layanow/config.toml` (T-118, E5).
//!
//! The file is optional: a missing or invalid file falls back to [`Settings::default`]
//! and logs a warning, so the applet always starts.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::results::DEFAULT_CONFIDENCE_THRESHOLD;

/// Whether the model stays resident or is unloaded between decisions
/// (ADR-11, T-117).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum UnloadPolicy {
    /// Keep the model resident (the default): instant answers, ~2 GB idle.
    #[default]
    Hot,
    /// Drop the model after each decision and reload on the next one.
    OnDemand,
}

/// The persisted configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Id of the checkpoint to load (ADR-7). An unknown id falls back to the
    /// default checkpoint at load time.
    pub checkpoint: String,
    /// Confidence below which a decision is flagged as low-confidence
    /// (ADR-27 C2). The app never refuses to answer.
    pub confidence_threshold: f32,
    /// Hot vs on-demand model residency (ADR-11, T-117).
    pub unload: UnloadPolicy,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            checkpoint: layanow_model::bundle::DEFAULT_ID.to_string(),
            confidence_threshold: DEFAULT_CONFIDENCE_THRESHOLD,
            unload: UnloadPolicy::Hot,
        }
    }
}

impl Settings {
    /// The path of the settings file:
    /// `$XDG_CONFIG_HOME/layanow/config.toml`, else `~/.config/layanow/config.toml`.
    #[must_use]
    pub fn path() -> PathBuf {
        if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
            return PathBuf::from(dir).join("layanow").join("config.toml");
        }
        PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
            .join(".config")
            .join("layanow")
            .join("config.toml")
    }

    /// Load the persisted settings, falling back to defaults when the file is
    /// absent or cannot be parsed.
    #[must_use]
    pub fn load() -> Self {
        let path = Self::path();
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        match toml::from_str(&text) {
            Ok(settings) => settings,
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "invalid settings; using defaults");
                Self::default()
            }
        }
    }

    /// Write the settings, creating the config directory if needed.
    ///
    /// # Errors
    /// Returns an error if the directory cannot be created or the file cannot
    /// be serialized/written.
    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        std::fs::write(&path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_through_toml() {
        let settings = Settings::default();
        let text = toml::to_string_pretty(&settings).expect("serialize");
        let parsed: Settings = toml::from_str(&text).expect("parse");
        assert_eq!(parsed, settings);
        assert_eq!(parsed.checkpoint, layanow_model::bundle::DEFAULT_ID);
        assert!((parsed.confidence_threshold - DEFAULT_CONFIDENCE_THRESHOLD).abs() < f32::EPSILON);
        assert_eq!(parsed.unload, UnloadPolicy::Hot);
    }

    #[test]
    fn missing_keys_fall_back_to_defaults() {
        let parsed: Settings =
            toml::from_str("confidence_threshold = 0.7\nunload = \"on-demand\"\n").expect("parse");
        assert_eq!(parsed.checkpoint, layanow_model::bundle::DEFAULT_ID);
        assert!((parsed.confidence_threshold - 0.7).abs() < f32::EPSILON);
        assert_eq!(parsed.unload, UnloadPolicy::OnDemand);
    }
}
